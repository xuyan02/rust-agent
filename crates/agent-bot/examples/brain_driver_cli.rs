use agent_bot::{Brain, BrainEvent, BrainEventSink};
use agent_core::{LlmAgent, LocalSpawner, RuntimeBuilder};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::{pin::Pin, rc::Rc, sync::mpsc};
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

struct TokioSpawner;

impl LocalSpawner for TokioSpawner {
    fn spawn_local(&self, fut: Pin<Box<dyn std::future::Future<Output = ()>>>) {
        tokio::task::spawn_local(fut);
    }
}

struct ChannelSink {
    tx: mpsc::Sender<BrainEvent>,
}

impl BrainEventSink for ChannelSink {
    fn emit(&mut self, event: BrainEvent) {
        let _ = self.tx.send(event);
    }
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

    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, async move {
        let runtime = Rc::new(
            RuntimeBuilder::new()
                .set_local_spawner(Rc::new(TokioSpawner))
                .add_llm_provider(Box::new(agent_core::llm::OpenAiProvider::new(
                    agent_core::llm::OpenAiProviderConfig {
                        base_url: openai.base_url,
                        api_key: openai.api_key,
                        model_provider_id: openai.model_provider_id,
                    },
                )))
                .build(),
        );

        let session = agent_core::SessionBuilder::new(Rc::clone(&runtime))
            .set_default_model("gpt-4o".to_string())
            .add_tool(Box::new(agent_core::tools::DebugTool::new()))
            .build()?;

        let (tx, rx) = mpsc::channel();
        let brain = Brain::new("brain-cli", session, Box::new(LlmAgent::new()), ChannelSink { tx })?;

        eprintln!("brain_cli ready. type and press enter; 'exit' to quit.");

        let mut stdin = tokio::io::BufReader::new(tokio::io::stdin()).lines();

        loop {
            tokio::select! {
                line = stdin.next_line() => {
                    let Ok(Some(line)) = line else { break };
                    let trimmed = line.trim().to_string();
                    if trimmed == "exit" {
                        brain.shutdown();
                        break;
                    }
                    if trimmed.is_empty() {
                        continue;
                    }
                    brain.push_input(trimmed);
                }
                _ = tokio::task::yield_now() => {
                    match rx.try_recv() {
                        Ok(BrainEvent::OutputText { text }) => {
                            println!("{text}");
                        }
                        Ok(BrainEvent::Error { error }) => {
                            eprintln!("brain error: {error:#}");
                            break;
                        }
                        Err(mpsc::TryRecvError::Empty) => {}
                        Err(mpsc::TryRecvError::Disconnected) => break,
                    }
                }
            }
        }

        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}
