use agent_bot::{Brain, BrainDriver};
use agent_core::{
    Agent, AgentContext, RuntimeBuilder,
    llm::{ChatContent, ChatMessage, ChatRole},
};
use anyhow::Result;
use tokio::io::AsyncBufReadExt;

struct EchoAgent;

#[async_trait::async_trait(?Send)]
impl Agent for EchoAgent {
    async fn run(&self, ctx: &AgentContext<'_>) -> Result<()> {
        let all = ctx.history().get_all().await?;
        let last_user = all.iter().rev().find(|m| m.role == ChatRole::User);

        let text = match last_user.map(|m| &m.content) {
            Some(ChatContent::Text(t)) => t.clone(),
            _ => "".to_string(),
        };

        ctx.history()
            .append(ChatMessage {
                role: ChatRole::Assistant,
                content: ChatContent::Text(format!("echo:{text}")),
            })
            .await?;

        Ok(())
    }
}

fn main() -> Result<()> {
    let runtime = RuntimeBuilder::new().build();
    let brain = Brain::new(&runtime, Box::new(EchoAgent))?;
    let (driver, handle) = BrainDriver::new(brain);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
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
