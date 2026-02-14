use agent_core::llm::{ChatContent, ChatMessage, ChatRole, LlmProvider, LlmSender};
use agent_core::{AgentContextBuilder, AgentRunner, LlmAgent, SessionBuilder};
use anyhow::Result;
use std::sync::{Arc, Mutex};

struct FakeProvider {
    calls: Arc<Mutex<usize>>,
}

impl FakeProvider {
    fn new() -> (Self, Arc<Mutex<usize>>) {
        let calls = Arc::new(Mutex::new(0));
        (
            Self {
                calls: calls.clone(),
            },
            calls,
        )
    }
}

impl LlmProvider for FakeProvider {
    fn name(&self) -> &str {
        "fake"
    }

    fn supports_model(&self, _model: &str) -> bool {
        true
    }

    fn create_sender(&self, _model: &str) -> Result<Box<dyn LlmSender>> {
        Ok(Box::new(FakeSender {
            calls: self.calls.clone(),
        }))
    }
}

struct FakeSender {
    calls: Arc<Mutex<usize>>,
}

#[async_trait::async_trait(?Send)]
impl LlmSender for FakeSender {
    async fn send(
        &mut self,
        messages: &[ChatMessage],
        _tools: &[&dyn agent_core::Tool],
    ) -> Result<ChatMessage> {
        let mut n = self.calls.lock().unwrap();
        *n += 1;

        if *n == 1 {
            let tool_calls = serde_json::json!([
                {
                    "id": "call_test_1",
                    "type": "function",
                    "function": {"name": "debug.echo", "arguments": "{\"text\":\"x\"}"}
                }
            ]);
            return Ok(ChatMessage::assistant_tool_calls(tool_calls));
        }

        // Second call should include assistant tool_calls and a tool result.
        let mut saw_assistant_with_tool_calls = false;
        let mut saw_tool = false;
        for m in messages {
            if m.role == ChatRole::Assistant && matches!(m.content, ChatContent::ToolCalls(_)) {
                saw_assistant_with_tool_calls = true;
            }

            if m.role == ChatRole::Tool
                && let ChatContent::ToolResult { tool_call_id, .. } = &m.content
                && tool_call_id == "call_test_1"
            {
                saw_tool = true;
            }
        }

        assert!(saw_assistant_with_tool_calls);
        assert!(saw_tool);

        Ok(ChatMessage::assistant_text("done"))
    }
}

#[tokio::test]
async fn agent_tool_loop_appends_tool_calls_and_results() -> Result<()> {
    let (provider, calls) = FakeProvider::new();

    let runtime = agent_core::RuntimeBuilder::new()
        .add_llm_provider(Box::new(provider))
        .build();

    let session = SessionBuilder::new(&runtime)
        .set_default_model("fake".to_string())
        .add_tool(Box::new(agent_core::tools::DebugTool::new()))
        .build()?;

    let ctx = AgentContextBuilder::from_session(&session).build()?;

    struct CaptureConsole {
        last: Option<String>,
    }

    impl agent_core::RunnerConsole for CaptureConsole {
        fn print_line(&mut self, s: &str) {
            self.last = Some(s.to_string());
        }
    }

    let mut runner = AgentRunner::new(LlmAgent::new());
    let mut console = CaptureConsole { last: None };
    runner
        .run_line(&ctx, &mut console, "hi".to_string())
        .await?;

    assert_eq!(console.last.as_deref(), Some("done"));
    assert_eq!(*calls.lock().unwrap(), 2);

    Ok(())
}
