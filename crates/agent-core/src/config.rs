use crate::{Result, Runtime, RuntimeBuilder, Session, SessionBuilder};
use agent_config::AgentConfig;
use agent_llm::OpenAiProviderConfig;
use std::path::PathBuf;

pub fn session_from_agent_config<'a>(
    runtime: &'a Runtime,
    cfg: AgentConfig,
    workspace: PathBuf,
    agent_dir: PathBuf,
) -> Result<Session<'a>> {
    SessionBuilder::new(runtime)
        .set_workspace_path(workspace)
        .set_agent_path(agent_dir)
        .set_default_model(cfg.model)
        .add_tool(Box::new(agent_tools::FileTool::new()))
        .add_tool(Box::new(agent_tools::ShellTool::new()))
        .add_tool(Box::new(agent_tools::DebugTool::new()))
        .build()
}

pub fn runtime_from_agent_config(cfg: &AgentConfig) -> Runtime {
    let mut b = RuntimeBuilder::new();

    if let Some(o) = &cfg.openai {
        b = b.set_openai(OpenAiProviderConfig {
            base_url: o.base_url.clone(),
            api_key: o.api_key.clone(),
        });
    }

    b.build()
}
