use crate::llm::{ChatContent, ChatMessage};
use crate::{AgentContext, find_tool_for_function, parse_tool_calls};
use anyhow::{Context, Result, bail};
use std::path::PathBuf;

use async_trait::async_trait;

#[async_trait(?Send)]
pub trait Agent: Send {
    async fn run(&mut self, ctx: &AgentContext<'_>) -> Result<()>;
}

pub struct ToolLoopOptions {
    pub max_tool_rounds: usize,
}

impl Default for ToolLoopOptions {
    fn default() -> Self {
        Self {
            max_tool_rounds: 20,
        }
    }
}

pub async fn run_tool_loop(
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

        let tools = ctx.tools();
        let reply = sender.send(&messages, tools.as_slice()).await?;

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
                    let tools = ctx.tools();
                    let tool = find_tool_for_function(tools.as_slice(), &c.function_name)
                        .with_context(|| {
                            format!("tool_loop: no tool for function: {}", c.function_name)
                        })?;

                    let result = tool.invoke(ctx, &c.function_name, &c.arguments).await?;
                    let result = maybe_spool_tool_output(ctx, &c.function_name, result).await?;

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

async fn maybe_spool_tool_output(
    ctx: &AgentContext<'_>,
    function_name: &str,
    output: String,
) -> Result<String> {
    const MAX_CHARS: usize = 8 * 1024;
    const PREVIEW_LINES: usize = 80;

    if output.len() <= MAX_CHARS {
        return Ok(output);
    }

    let spool_dir = ctx.session().agent_path().join("spool");
    tokio::fs::create_dir_all(&spool_dir)
        .await
        .with_context(|| format!("failed to create spool dir: {}", spool_dir.display()))?;

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let safe_fn = function_name.replace(['/', '.', ':'], "_");
    let filename = format!("{ts}_{safe_fn}.log");
    let abs: PathBuf = spool_dir.join(filename);

    tokio::fs::write(&abs, output.as_bytes())
        .await
        .with_context(|| format!("failed to write spool file: {}", abs.display()))?;

    let mut preview = String::new();
    for (i, line) in output.lines().take(PREVIEW_LINES).enumerate() {
        if i > 0 {
            preview.push('\n');
        }
        preview.push_str(line);
    }

    Ok(format!(
        "{preview}\n\n[tool output truncated: {} chars; full output saved to {}]\n\
To read more: file.read {{\"path\": \"{}\", \"offset_lines\": 0, \"limit_lines\": 200}}",
        output.len(),
        abs.display(),
        abs.strip_prefix(ctx.session().workspace_path())
            .unwrap_or(&abs)
            .display()
    ))
}

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
