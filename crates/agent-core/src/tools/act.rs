use crate::llm::{ChatContent, ChatMessage, ChatRole};
use crate::tools::{FunctionSpec, ObjectSpec, PropertySpec, StringSpec, Tool, ToolSpec, TypeSpec};
use crate::{AgentContext, AgentContextBuilder, LlmAgent, Agent, StaticSystemPromptSegment, PersistentHistory};
use anyhow::{Result, Context as _};
use async_trait::async_trait;
use std::rc::Rc;

/// Act tool for ReAct agent
///
/// This tool creates a sub-context with independent persistent history and executes actions.
/// It allows the think phase to delegate complex actions to a separate execution context.
///
/// The act history is stored separately from the think history in a "act" subdirectory.
/// For example, if the session uses "work/" directory for think history,
/// act history will be stored in "work/act/".
pub struct ActTool;

impl ActTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ActTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait(?Send)]
impl Tool for ActTool {
    fn spec(&self) -> &ToolSpec {
        static SPEC: std::sync::OnceLock<ToolSpec> = std::sync::OnceLock::new();
        SPEC.get_or_init(|| ToolSpec {
            id: "act".to_string(),
            description: "Delegate a task to the action phase for execution. The action phase has access to all available tools and will perform the task.".to_string(),
            functions: vec![FunctionSpec {
                name: "act".to_string(),
                description: "Delegate a task to the action phase. Provide a natural language description of what needs to be done.".to_string(),
                parameters: ObjectSpec {
                    properties: vec![PropertySpec {
                        name: "action".to_string(),
                        ty: TypeSpec::String(StringSpec::default()),
                    }],
                    required: vec!["action".to_string()],
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
            "act" => {
                let action = args
                    .get("action")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'action' argument"))?;

                tracing::info!("[ActTool] Executing action: {}", action);

                // Get dir_node from context to create act history directory
                let act_dir = ctx
                    .dir_node()
                    .context("ActTool requires dir_node to be set in context")?
                    .subdir("act");

                // Create a sub-context with independent persistent history
                // This context inherits tools from parent but has its own history
                // Act history is stored separately from think history
                let act_history = Box::new(PersistentHistory::new(Rc::clone(&act_dir)));

                let act_prompt = include_str!("../../prompts/react_act.md").to_string();

                let act_ctx = AgentContextBuilder::from_parent_ctx(ctx)
                    .set_history(act_history)
                    .add_system_prompt_segment(Box::new(StaticSystemPromptSegment::new(act_prompt)))
                    .build()?;

                // Add the action as a user message in the sub-context
                act_ctx
                    .history()
                    .append(&act_ctx, ChatMessage::user_text(action.to_string()))
                    .await?;

                // Run LlmAgent in the sub-context (with tools enabled)
                let act_agent = LlmAgent::new();
                act_agent.run(&act_ctx).await?;

                // Extract the result from act context history
                let messages = act_ctx.history().get_all(&act_ctx).await?;
                let last_assistant = messages
                    .iter()
                    .rev()
                    .find(|m| matches!(m.role, ChatRole::Assistant))
                    .ok_or_else(|| anyhow::anyhow!("Act tool: no assistant response found"))?;

                let result = match &last_assistant.content {
                    ChatContent::Text(text) => text.clone(),
                    _ => anyhow::bail!("Act tool: expected text response"),
                };

                tracing::info!("[ActTool] Action completed, result: {}", result);

                Ok(result)
            }
            _ => anyhow::bail!("unknown function: {}", function_name),
        }
    }
}
