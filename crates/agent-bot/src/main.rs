use anyhow::{Context, Result};
use clap::Parser;

use agent_bot::{bot, config};

#[derive(Debug, Parser)]
#[command(author, version, about = "Programming bot CLI")]
struct Cli {
    /// Single-shot task (non-interactive)
    #[arg(long)]
    task: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    let workspace = std::env::current_dir().context("failed to get cwd")?;
    let agent_dir = workspace.join(".agent");
    let cfg_path = agent_dir.join("agent.yaml");

    let cfg = match config::load_agent_config_yaml_async(&cfg_path).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: failed to load config: {e:#}");
            eprintln!("hint: create {}", cfg_path.display());
            std::process::exit(1);
        }
    };

    let mut runtime_builder = agent_core::RuntimeBuilder::new();
    if let Some(o) = &cfg.openai {
        runtime_builder = runtime_builder.set_openai(agent_core::llm::OpenAiProviderConfig {
            base_url: o.base_url.clone(),
            api_key: o.api_key.clone(),
            model_provider_id: o.model_provider_id.clone(),
        });
    }
    let runtime = runtime_builder.build();

    let session = agent_core::SessionBuilder::new(&runtime)
        .set_workspace_path(workspace.clone())
        .set_agent_path(agent_dir.clone())
        .set_default_model(cfg.model)
        .add_tool(Box::new(agent_core::tools::FileTool::new()))
        .add_tool(Box::new(agent_core::tools::ShellTool::new()))
        .add_tool(Box::new(agent_core::tools::DebugTool::new()))
        .build()?;

    let ctx = agent_core::AgentContextBuilder::from_session(&session).build()?;

    let mut bot = bot::ProgrammingBot::new();

    if let Some(task) = cli.task {
        let out = bot.task(&ctx, task).await?;
        println!("{out}");
        return Ok(());
    }

    repl(&ctx, &mut bot).await
}

async fn repl(ctx: &agent_core::AgentContext<'_>, bot: &mut bot::ProgrammingBot) -> Result<()> {
    use tokio::io::AsyncBufReadExt;

    eprintln!("agent-bot: type 'help' for commands");

    let stdin = tokio::io::stdin();
    let mut lines = tokio::io::BufReader::new(stdin).lines();

    while let Some(line) = lines.next_line().await.context("failed to read stdin")? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let mut parts = line.splitn(2, ' ');
        let cmd = parts.next().unwrap_or("");
        let rest = parts.next().unwrap_or("").trim().to_string();

        match cmd {
            "help" => {
                print!("{}", bot::help_text());
            }
            "exit" | "quit" => return Ok(()),
            "reset" => {
                bot.reset();
                eprintln!("ok");
            }
            "task" => {
                if rest.is_empty() {
                    eprintln!("error: task requires a goal");
                    continue;
                }
                let out = bot.task(ctx, rest).await?;
                println!("{out}");
            }
            "plan" => {
                if rest.is_empty() {
                    eprintln!("error: plan requires a goal");
                    continue;
                }
                let out = bot.plan(ctx, rest).await?;
                println!("{out}");
            }
            "apply" => {
                let out = bot.apply(ctx).await?;
                println!("{out}");
            }
            "verify" => {
                let out = bot.verify(ctx).await?;
                println!("{out}");
            }
            "diff" => {
                let out = bot.diff(ctx).await?;
                println!("{out}");
            }
            "state" => {
                println!("{:#?}", bot.state());
            }
            _ => {
                eprintln!("error: unknown command: {cmd}");
            }
        }
    }

    Ok(())
}
