use agent_bot::{Brain, BrainDriver};
use agent_core::{LlmAgent, RuntimeBuilder};
use anyhow::{Context, Result};
use serde::Deserialize;
use tokio::io::AsyncBufReadExt;

#[derive(Debug, Clone, Deserialize)]
struct OpenAiCfg {
    base_url: String,
    api_key: String,

    #[serde(default)]
    model_provider_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct AgentCfg {
    // Intentionally ignored: Brain::new() sets default_model internally.
    #[allow(dead_code)]
    model: Option<String>,

    openai: Option<OpenAiCfg>,
}

async fn load_cfg(path: impl AsRef<std::path::Path>) -> Result<AgentCfg> {
    let path = path.as_ref();
    let bytes = tokio::fs::read(path)
        .await
        .with_context(|| format!("failed to read config: {}", path.display()))?;
    let cfg: AgentCfg = serde_yaml::from_slice(&bytes)
        .with_context(|| format!("failed to parse yaml: {}", path.display()))?;
    Ok(cfg)
}

fn main() -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let cfg_path = std::path::PathBuf::from(".agent").join("agent.yaml");
    let cfg = rt.block_on(load_cfg(cfg_path))?;

    let openai = cfg
        .openai
        .ok_or_else(|| anyhow::anyhow!("missing openai config in .agent/agent.yaml"))?;

    let runtime = RuntimeBuilder::new()
        .add_llm_provider(Box::new(agent_core::llm::OpenAiProvider::new(
            agent_core::llm::OpenAiProviderConfig {
                base_url: openai.base_url,
                api_key: openai.api_key,
                model_provider_id: openai.model_provider_id,
            },
        )))
        .build();

    let brain = Brain::new(&runtime, Box::new(LlmAgent::new()))?;
    let (driver, handle) = BrainDriver::new(brain);

    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, async move {
        eprintln!("brain_driver_cli ready. type and press enter; 'exit' to quit.");

        let mut stdin = tokio::io::BufReader::new(tokio::io::stdin()).lines();

        loop {
            tokio::select! {
                r = driver.run() => {
                    if let Err(e) = r {
                        eprintln!("driver error: {e:#}");
                    }
                    break;
                }
                line = stdin.next_line() => {
                    let Ok(Some(line)) = line else { break };
                    let trimmed = line.trim().to_string();
                    if trimmed == "exit" {
                        handle.shutdown();
                        break;
                    }
                    if trimmed.is_empty() {
                        continue;
                    }
                    handle.input(trimmed);
                }
            }
        }
    });

    Ok(())
}
