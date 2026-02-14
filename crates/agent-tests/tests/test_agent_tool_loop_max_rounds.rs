use agent_core::llm::{ChatContent, ChatMessage, ChatRole};
use agent_core::llm::{LlmProvider, LlmSender};
use agent_core::{Agent, AgentContextBuilder, SessionBuilder};
use anyhow::Result;

struct AlwaysToolCallsSender;

struct TestProvider;

impl LlmProvider for TestProvider {
    fn name(&self) -> &str {
        "test"
    }

    fn supports_model(&self, _model: &str) -> bool {
        true
    }

    fn create_sender(&self, _model: &str) -> Result<Box<dyn LlmSender>> {
        Ok(Box::new(AlwaysToolCallsSender))
    }
}

#[async_trait::async_trait(?Send)]
impl LlmSender for AlwaysToolCallsSender {
    async fn send(
        &mut self,
        _messages: &[ChatMessage],
        _tools: &[&dyn agent_core::Tool],
    ) -> Result<ChatMessage> {
        Ok(ChatMessage {
            role: ChatRole::Assistant,
            content: ChatContent::ToolCalls(serde_json::json!([
                {
                    "id": "call-1",
                    "type": "function",
                    "function": {"name": "debug.echo", "arguments": "{\"text\":\"hi\"}"}
                }
            ])),
        })
    }
}

#[tokio::test]
async fn agent_tool_loop_aborts_after_max_rounds() -> Result<()> {
    let runtime = agent_core::RuntimeBuilder::new()
        .add_llm_provider(Box::new(TestProvider))
        .build();

    let session = SessionBuilder::new(&runtime)
        .set_default_model("gpt-test".to_string())
        .add_tool(Box::new(agent_core::tools::DebugTool::new()))
        .build()?;

    let ctx = AgentContextBuilder::from_session(&session).build()?;
    let _ = ctx
        .history()
        .append(agent_core::make_user_message("hi".to_string()))
        .await;

    let mut agent = agent_core::LlmAgent::new();
    let err = agent.run(&ctx).await.unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("max tool rounds"), "unexpected error: {msg}");

    Ok(())
}
