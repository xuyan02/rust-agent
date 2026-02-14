use crate::llm::{ChatContent, ChatMessage};
use crate::{AgentContext, find_tool_for_function, parse_tool_calls};
use anyhow::{Context, Result, bail};

use async_trait::async_trait;

#[async_trait(?Send)]
pub trait Agent: Send {
    async fn run(&mut self, ctx: &AgentContext<'_>) -> Result<()>;
}

pub(crate) struct ToolLoopOptions {
    pub max_tool_rounds: usize,
}

impl Default for ToolLoopOptions {
    fn default() -> Self {
        Self {
            max_tool_rounds: 20,
        }
    }
}

pub(crate) async fn run_tool_loop(
    ctx: &AgentContext<'_>,
    mut messages: Vec<ChatMessage>,
    opts: ToolLoopOptions,
) -> Result<()> {
    if ctx.session().default_model().is_empty() {
        bail!("agent: missing default model");
    }

    let mut rounds: usize = 0;

    loop {
        let mut sender = ctx
            .session()
            .runtime()
            .create_sender(ctx.session().default_model())?;
        let reply = sender.send(&messages).await?;

        if reply.role != crate::llm::ChatRole::Assistant {
            bail!("tool_loop: reply role is not assistant");
        }

        let _ = ctx.history().append(reply.clone()).await;
        messages.push(reply.clone());

        match reply.content {
            ChatContent::Text(_) => return Ok(()),
            ChatContent::ToolCalls(tool_calls) => {
                rounds += 1;
                if rounds > opts.max_tool_rounds {
                    bail!(
                        "tool_loop: exceeded max tool rounds ({})",
                        opts.max_tool_rounds
                    );
                }

                let calls = parse_tool_calls(&tool_calls)?;
                if calls.is_empty() {
                    bail!("tool_loop: empty tool_calls");
                }

                for c in calls {
                    let tool = find_tool_for_function(
                        ctx.tools(),
                        ctx.session().tools(),
                        &c.function_name,
                    )
                    .with_context(|| {
                        format!("tool_loop: no tool for function: {}", c.function_name)
                    })?;

                    let result = tool
                        .invoke(
                            ctx.session().workspace_path(),
                            &c.function_name,
                            &c.arguments,
                        )
                        .await?;

                    let tool_result = ChatMessage::tool_result(c.id, result);
                    let _ = ctx.history().append(tool_result.clone()).await;
                    messages.push(tool_result);
                }

                continue;
            }
            _ => bail!("tool_loop: unexpected assistant message"),
        }
    }
}

pub struct LlmAgent;

impl LlmAgent {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LlmAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait(?Send)]
impl Agent for LlmAgent {
    async fn run(&mut self, ctx: &AgentContext<'_>) -> Result<()> {
        let mut messages: Vec<ChatMessage> = ctx
            .system_segments()
            .iter()
            .map(|s| ChatMessage::system_text(s.clone()))
            .collect();
        messages.extend(ctx.history().get_all().await?);

        run_tool_loop(ctx, messages, ToolLoopOptions::default()).await
    }
}
