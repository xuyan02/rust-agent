use crate::tools::{Tool, ToolSpec, FunctionSpec, ObjectSpec, PropertySpec, TypeSpec, StringSpec};
use crate::{Agent, AgentContext, AgentContextBuilder, InMemoryHistory, ReActAgent};
use anyhow::Result;
use async_trait::async_trait;

/// Safely truncate a string to at most `max_bytes` bytes, ensuring we don't cut in the middle of a UTF-8 character.
fn truncate_str(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }

    // Find the largest valid UTF-8 character boundary at or before max_bytes
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }

    &s[..end]
}

/// DeepThinkTool - Delegates complex reasoning to a ReActAgent
///
/// This tool allows a simple LLM agent to delegate complex tasks
/// to a ReActAgent for deeper analysis and reasoning.
pub struct DeepThinkTool;

impl DeepThinkTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DeepThinkTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait(?Send)]
impl Tool for DeepThinkTool {
    fn spec(&self) -> &ToolSpec {
        static SPEC: std::sync::OnceLock<ToolSpec> = std::sync::OnceLock::new();
        SPEC.get_or_init(|| ToolSpec {
            id: "deep-think".to_string(),
            description: "Use deep reasoning (ReAct framework) for complex tasks".to_string(),
            functions: vec![FunctionSpec {
                name: "deep-think".to_string(),
                description: "Delegate a complex task to deep reasoning agent. Use this when you need to:\n\
                             - Analyze complex problems step by step\n\
                             - Use tools multiple times to gather information\n\
                             - Make decisions based on gathered information\n\
                             The deep reasoning agent will think through the problem and return the final answer."
                    .to_string(),
                parameters: ObjectSpec {
                    properties: vec![PropertySpec {
                        name: "task".to_string(),
                        ty: TypeSpec::String(StringSpec::default()),
                    }],
                    required: vec!["task".to_string()],
                    additional_properties: false,
                },
            }],
        })
    }

    async fn invoke(
        &self,
        ctx: &AgentContext<'_>,
        function_name: &str,
        args: &serde_json::Value,
    ) -> Result<String> {
        match function_name {
            "deep-think" => {
                let task = args
                    .get("task")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'task' argument"))?;

                // Create context directly from Session to avoid inheriting Bot's protocol prompts
                // Deep Brain should not see Main Brain's JSON format requirements
                let deep_history: Box<dyn crate::History> = Box::new(crate::InMemoryHistory::new());
                let deep_ctx = crate::AgentContextBuilder::from_session(ctx.session())
                    .set_history(deep_history)
                    .build()?;

                // Append the initial task after context is created
                deep_ctx.history()
                    .append(&deep_ctx, crate::llm::ChatMessage::user_text(task))
                    .await?;

                // Create and run ReActAgent
                tracing::debug!("[DeepThink] Starting deep reasoning for task");
                let react_agent = ReActAgent::new().with_logging(true);

                // Run with better error handling
                if let Err(e) = react_agent.run(&deep_ctx).await {
                    tracing::error!("[DeepThink] Failed: {}", e);
                    return Err(anyhow::anyhow!(
                        "Deep thinking failed: {}. This might be due to API rate limits or quota restrictions. Try again in a moment.",
                        e
                    ));
                }

                tracing::debug!("[DeepThink] Completed successfully");

                // Extract the final answer from the isolated history
                let messages = deep_ctx.history().get_all(&deep_ctx).await?;
                let last_assistant = messages
                    .iter()
                    .rev()
                    .find(|m| matches!(m.role, crate::llm::ChatRole::Assistant))
                    .ok_or_else(|| anyhow::anyhow!("no answer from deep think"))?;

                match &last_assistant.content {
                    crate::llm::ChatContent::Text(text) => {
                        // Extract content after [answer] prefix if present
                        let answer = if let Some(content) = text.strip_prefix("[answer]") {
                            content.trim().to_string()
                        } else {
                            text.clone()
                        };

                        tracing::debug!("[DeepThink] Final answer (length: {} chars):\n{}",
                            answer.len(),
                            if answer.len() > 500 {
                                format!("{}...", truncate_str(&answer, 500))
                            } else {
                                answer.clone()
                            });

                        Ok(answer)
                    }
                    _ => anyhow::bail!("unexpected content type from deep think"),
                }
            }
            _ => anyhow::bail!("unknown function: {}", function_name),
        }
    }
}
