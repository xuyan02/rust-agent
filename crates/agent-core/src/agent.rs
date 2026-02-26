use crate::AgentContext;
use crate::llm::ChatMessage;
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::PathBuf;

/// Maximum number of tool-call iterations before aborting.
/// This prevents infinite loops (and runaway API costs) when the LLM
/// repeatedly returns tool calls without ever producing a text response.
pub const MAX_TOOL_ITERATIONS: usize = 50;

#[async_trait(?Send)]
pub trait Agent {
    async fn run(&self, ctx: &AgentContext<'_>) -> Result<()>;
}

pub struct LlmAgent;

pub(crate) async fn maybe_spool_tool_output(
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

    let rel = abs
        .strip_prefix(ctx.session().agent_path())
        .map(|p| PathBuf::from(".agent").join(p))
        .unwrap_or_else(|_| abs.clone());

    Ok(format!(
        "{preview}\n\n[tool output truncated: {} chars; full output saved to {}]\n\
To read more: file-read {{\"path\": \"{}\", \"offset_lines\": 0, \"limit_lines\": 200}}",
        output.len(),
        abs.display(),
        rel.display()
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
    async fn run(&self, ctx: &AgentContext<'_>) -> Result<()> {
        let mut messages: Vec<ChatMessage> = vec![];
        messages.extend(ctx.history().get_all(ctx).await?);

        ctx.session().runtime().execute(ctx, messages).await
    }
}
