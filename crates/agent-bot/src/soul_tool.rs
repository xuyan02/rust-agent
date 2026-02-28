use agent_core::tools::{FunctionSpec, ObjectSpec, PropertySpec, StringSpec, Tool, ToolSpec, TypeSpec};
use agent_core::{AgentContext, DataNode, SystemPromptSegment, estimate_tokens};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::rc::Rc;

/// Persistable soul data
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SoulData {
    pub content: String,
}

/// Shared soul state for the bot
#[derive(Clone)]
pub struct SoulState {
    node: Rc<DataNode>,
}

impl SoulState {
    pub fn new(node: Rc<DataNode>) -> Self {
        Self { node }
    }

    /// Load soul from disk (idempotent)
    /// DataNode.load() automatically creates default if file doesn't exist
    pub async fn load(&self) -> Result<()> {
        self.node.load::<SoulData>().await
    }

    /// Flush soul to disk
    pub async fn flush(&self) -> Result<()> {
        self.node.flush().await
    }

    pub fn get(&self) -> String {
        if let Ok(Some(data)) = self.node.get::<SoulData>() {
            data.content.clone()
        } else {
            String::new()
        }
    }

    pub fn set(&self, content: String) {
        if let Ok(mut data) = self.node.get_or_default::<SoulData>() {
            data.content = content;
            // drop data, auto-marks dirty
        }
    }

    /// Count tokens in soul content using precise tiktoken estimation
    pub fn count_tokens(&self) -> usize {
        let content = self.get();
        estimate_tokens(&content)
    }
}

/// Dynamic system prompt segment that renders soul
pub struct SoulSegment {
    soul_state: SoulState,
}

// SoulState is Rc<RefCell<...>> which is not Send, but SoulSegment
// is only used in single-threaded context (Brain is !Send)
unsafe impl Send for SoulSegment {}

impl SoulSegment {
    pub fn new(soul_state: SoulState) -> Self {
        Self { soul_state }
    }
}

#[async_trait::async_trait(?Send)]
impl SystemPromptSegment for SoulSegment {
    async fn render(&self, _ctx: &AgentContext<'_>) -> Result<String> {
        // Load from disk on first access (idempotent)
        self.soul_state.load().await?;

        let content = self.soul_state.get();
        if content.is_empty() {
            Ok(String::new())
        } else {
            let token_count = self.soul_state.count_tokens();

            let mut result = String::from("\n---\n\n## Soul (Who I Am)\n\n");
            result.push_str(&format!("**{} tokens**\n\n", token_count));
            result.push_str(&content);
            result.push_str("\n\n---\n");
            Ok(result)
        }
    }
}

/// Soul tool for defining bot's identity and personality
pub struct SoulTool {
    soul_state: SoulState,
}

impl SoulTool {
    pub fn new(soul_state: SoulState) -> Self {
        Self { soul_state }
    }
}

#[async_trait::async_trait(?Send)]
impl Tool for SoulTool {
    fn spec(&self) -> &ToolSpec {
        static SPEC: std::sync::OnceLock<ToolSpec> = std::sync::OnceLock::new();
        SPEC.get_or_init(|| ToolSpec {
            id: "soul".to_string(),
            description: "Manage bot's soul - define identity, personality, and capabilities".to_string(),
            functions: vec![
                FunctionSpec {
                    name: "read-soul".to_string(),
                    description: "Read the current soul content.".to_string(),
                    parameters: ObjectSpec {
                        properties: vec![],
                        required: vec![],
                        additional_properties: false,
                    },
                },
                FunctionSpec {
                    name: "write-soul".to_string(),
                    description: "Write or update the soul content. This defines who you are, your personality, and what you're good at. Keep it under 500 tokens.".to_string(),
                    parameters: ObjectSpec {
                        properties: vec![PropertySpec {
                            name: "content".to_string(),
                            ty: TypeSpec::String(StringSpec::default()),
                        }],
                        required: vec!["content".to_string()],
                        additional_properties: false,
                    },
                },
            ],
        })
    }

    async fn invoke(
        &self,
        _ctx: &agent_core::AgentContext<'_>,
        function_name: &str,
        args: &serde_json::Value,
    ) -> Result<String> {
        // Load from disk on first access (idempotent)
        self.soul_state.load().await?;

        match function_name {
            "read-soul" => {
                let content = self.soul_state.get();
                if content.is_empty() {
                    Ok("Soul is empty. You haven't defined your identity yet.".to_string())
                } else {
                    let token_count = self.soul_state.count_tokens();
                    Ok(format!("Current soul ({} tokens):\n\n{}", token_count, content))
                }
            }
            "write-soul" => {
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'content' argument"))?;

                // Check token limit
                let token_count = estimate_tokens(content);
                if token_count > 500 {
                    anyhow::bail!(
                        "Soul content too long: {} tokens (max 500 tokens)",
                        token_count
                    );
                }

                self.soul_state.set(content.to_string());
                // Flush immediately to persist changes
                self.soul_state.flush().await?;

                Ok(format!("Soul updated: {} tokens", token_count))
            }
            _ => anyhow::bail!("unknown function: {}", function_name),
        }
    }
}
