use anyhow::{Context, Result};
use tokio::io::AsyncBufReadExt;

use agent_core::Agent;

pub struct Args {
    pub input: Option<String>,
}

pub async fn run(args: Args) -> Result<()> {
    let workspace = std::env::current_dir().context("failed to get cwd")?;
    let agent_dir = workspace.join(".agent");
    let cfg_path = agent_dir.join("agent.yaml");

    let cfg = match crate::config::load_agent_config_yaml_async(&cfg_path).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: failed to load config: {e:#}");
            eprintln!("hint: create {}", cfg_path.display());
            std::process::exit(1);
        }
    };

    eprintln!("workspace_path: {}", workspace.display());

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

    if session.default_model().is_empty() {
        eprintln!("error: missing default model");
        std::process::exit(1);
    }

    let ctx = agent_core::AgentContextBuilder::from_session(&session).build()?;

    if let Some(input) = args.input {
        ctx.history()
            .append(agent_core::make_user_message(input))
            .await?;
        agent_core::LlmAgent::new().run(&ctx).await?;
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

        ctx.history()
            .append(agent_core::make_user_message(trimmed.to_string()))
            .await?;
        agent_core::LlmAgent::new().run(&ctx).await?;
    }

    Ok(())
}
