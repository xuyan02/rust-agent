use agent_core::{AgentContextBuilder, AgentRunner, RunnerConsole};
use agent_llm::{ChatContent, ChatMessage, ChatRole, LlmProvider, LlmSender};
use anyhow::Result;

struct CaptureConsole {
    lines: Vec<String>,
}

impl RunnerConsole for CaptureConsole {
    fn print_line(&mut self, s: &str) {
        self.lines.push(s.to_string());
    }
}

struct AssertSystemFirstProvider;

impl LlmProvider for AssertSystemFirstProvider {
    fn name(&self) -> &str {
        "assert-system-first"
    }

    fn supports_model(&self, _model: &str) -> bool {
        true
    }

    fn create_sender(&self, _model: &str) -> Result<Box<dyn LlmSender>> {
        Ok(Box::new(AssertSystemFirstSender))
    }
}

struct AssertSystemFirstSender;

#[async_trait::async_trait(?Send)]
impl LlmSender for AssertSystemFirstSender {
    async fn send(&mut self, messages: &[ChatMessage]) -> Result<ChatMessage> {
        assert!(!messages.is_empty());
        assert_eq!(messages[0].role, ChatRole::System);
        match &messages[0].content {
            ChatContent::Text(t) => assert_eq!(t, "sys-1"),
            _ => panic!("expected system text"),
        }

        Ok(ChatMessage::assistant_text("ok"))
    }
}

#[tokio::test]
async fn system_segments_are_prefixed_to_llm_messages() -> Result<()> {
    let runtime = agent_core::RuntimeBuilder::new()
        .add_llm_provider(Box::new(AssertSystemFirstProvider))
        .build();

    let session = agent_core::SessionBuilder::new(&runtime)
        .set_default_model("fake".to_string())
        .build()?;

    let ctx = AgentContextBuilder::new(&session)
        .add_system_segment("sys-1".to_string())
        .build()?;

    let mut runner = AgentRunner::new(agent_core::LlmAgent::new());
    let mut console = CaptureConsole { lines: vec![] };

    runner
        .run_line(&ctx, &mut console, "hi".to_string())
        .await?;

    assert_eq!(console.lines, vec!["ok".to_string()]);
    Ok(())
}
