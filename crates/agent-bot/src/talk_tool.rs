use agent_core::tools::{FunctionSpec, ObjectSpec, PropertySpec, StringSpec, Tool, ToolSpec, TypeSpec};
use agent_core::AgentContext;
use anyhow::Result;
use std::cell::RefCell;
use std::rc::Rc;

/// Shared reference to conversation brain for sending messages
#[derive(Clone)]
pub struct TalkChannel {
    conversation_brain: Rc<RefCell<Option<Box<crate::Brain>>>>,
}

impl TalkChannel {
    pub fn new(conversation_brain: Rc<RefCell<Option<Box<crate::Brain>>>>) -> Self {
        Self { conversation_brain }
    }

    /// Send a message to conversation brain
    pub fn send(&self, message: String) {
        if let Some(brain) = self.conversation_brain.borrow().as_ref() {
            brain.push_input(message);
        }
    }
}

/// Talk tool for Work Brain and Introspection Brain to send messages to Conversation Brain
pub struct TalkTool {
    channel: TalkChannel,
    brain_name: String,
}

impl TalkTool {
    pub fn new(channel: TalkChannel, brain_name: impl Into<String>) -> Self {
        Self {
            channel,
            brain_name: brain_name.into(),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Tool for TalkTool {
    fn spec(&self) -> &ToolSpec {
        static SPEC: std::sync::OnceLock<ToolSpec> = std::sync::OnceLock::new();
        SPEC.get_or_init(|| ToolSpec {
            id: "talk".to_string(),
            description: "Send a message to the conversation brain. Use this to report task results, ask for clarification, or provide updates.".to_string(),
            functions: vec![
                FunctionSpec {
                    name: "send-message".to_string(),
                    description: "Send a message to the conversation brain.".to_string(),
                    parameters: ObjectSpec {
                        properties: vec![PropertySpec {
                            name: "message".to_string(),
                            ty: TypeSpec::String(StringSpec::default()),
                        }],
                        required: vec!["message".to_string()],
                        additional_properties: false,
                    },
                },
            ],
        })
    }

    async fn invoke(
        &self,
        _ctx: &AgentContext<'_>,
        function_name: &str,
        args: &serde_json::Value,
    ) -> Result<String> {
        match function_name {
            "send-message" => {
                let message = args
                    .get("message")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'message' argument"))?;

                let formatted_message = format!("{} says: {}", self.brain_name, message);
                self.channel.send(formatted_message);

                Ok(format!("Message sent to conversation brain: {}", message))
            }
            _ => anyhow::bail!("unknown function: {}", function_name),
        }
    }
}
