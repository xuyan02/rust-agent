use anyhow::{Context, Result};
use tokio::io::AsyncBufReadExt;

pub struct Args {
    pub input: Option<String>,
}

pub async fn run(args: Args) -> Result<()> {
    let workspace = std::env::current_dir().context("failed to get cwd")?;
    let agent_dir = workspace.join(".agent");
    let cfg_path = agent_dir.join("agent.yaml");

    let cfg = match agent_core::support::config::load_agent_config_yaml_async(&cfg_path).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: failed to load config: {e:#}");
            eprintln!("hint: create {}", cfg_path.display());
            std::process::exit(1);
        }
    };

    eprintln!("workspace_path: {}", workspace.display());

    let runtime = agent_core::runtime_from_agent_config(&cfg);
    let session =
        agent_core::session_from_agent_config(&runtime, cfg, workspace.clone(), agent_dir.clone())?;

    if session.default_model().is_empty() {
        eprintln!("error: missing default model");
        std::process::exit(1);
    }

    let ctx = agent_core::AgentContextBuilder::new(&session).build()?;
    let mut runner = agent_core::AgentRunner::new(agent_core::ReCapAgent::new());
    let mut out = crate::console::StdoutRunnerConsole;

    if let Some(input) = args.input {
        runner.run_line(&ctx, &mut out, input).await?;
        return Ok(());
    }

    eprintln!("ready.");

    let stdin = tokio::io::stdin();
    let mut lines = tokio::io::BufReader::new(stdin).lines();

    while let Some(line) = lines.next_line().await.context("failed to read stdin")? {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed.is_empty() {
            continue;
        }

        runner.run_line(&ctx, &mut out, trimmed.to_string()).await?;
    }

    Ok(())
}
