use agent_core::llm::{ChatContent, ChatMessage, ChatRole, LlmProvider, LlmSender};
use agent_core::{AgentContextBuilder, AgentRunner, ReCapAgent, SessionBuilder};
use anyhow::Result;
use std::sync::{Arc, Mutex};

struct RecordingProvider {
    log: Arc<Mutex<Vec<Vec<ChatMessage>>>>,
}

impl RecordingProvider {
    fn new() -> (Self, Arc<Mutex<Vec<Vec<ChatMessage>>>>) {
        let log = Arc::new(Mutex::new(vec![]));
        (Self { log: log.clone() }, log)
    }
}

impl LlmProvider for RecordingProvider {
    fn name(&self) -> &str {
        "recording"
    }

    fn supports_model(&self, _model: &str) -> bool {
        true
    }

    fn create_sender(&self, _model: &str) -> Result<Box<dyn LlmSender>> {
        Ok(Box::new(RecordingSender {
            log: self.log.clone(),
        }))
    }
}

struct RecordingSender {
    log: Arc<Mutex<Vec<Vec<ChatMessage>>>>,
}

#[async_trait::async_trait(?Send)]
impl LlmSender for RecordingSender {
    async fn send(&mut self, messages: &[ChatMessage]) -> Result<ChatMessage> {
        self.log.lock().unwrap().push(messages.to_vec());

        // 1st call: recap
        // 2nd call: act
        let call_idx = self.log.lock().unwrap().len();
        if call_idx == 1 {
            return Ok(ChatMessage::assistant_text(
                "Goals\n- g\n\nCurrent plan / next actions\n- p\n\nKey facts\n- f\n\nDecisions made / rationale\n- d\n\nOpen questions / uncertainties\n- o\n\nConstraints / preferences\n- c"
                    .to_string(),
            ));
        }

        Ok(ChatMessage::assistant_text("done"))
    }
}

#[tokio::test]
async fn recap_agent_injects_recap_but_does_not_persist_it_in_history() -> Result<()> {
    let (provider, log) = RecordingProvider::new();

    let runtime = agent_core::RuntimeBuilder::new()
        .add_llm_provider(Box::new(provider))
        .build();

    let session = SessionBuilder::new(&runtime)
        .set_default_model("fake".to_string())
        .build()?;

    let ctx = AgentContextBuilder::new(&session).build()?;

    struct CaptureConsole {
        last: Option<String>,
    }

    impl agent_core::RunnerConsole for CaptureConsole {
        fn print_line(&mut self, s: &str) {
            self.last = Some(s.to_string());
        }
    }

    let mut runner = AgentRunner::new(ReCapAgent::new());
    let mut console = CaptureConsole { last: None };
    runner
        .run_line(&ctx, &mut console, "hi".to_string())
        .await?;

    assert_eq!(console.last.as_deref(), Some("done"));

    // Two LLM calls: recap + act.
    let acting_msgs = {
        let calls = log.lock().unwrap();
        assert_eq!(calls.len(), 2);
        calls[1].clone()
    };

    // Acting call includes an injected ReCAP system message.
    assert!(acting_msgs.iter().any(|m| {
        m.role == ChatRole::System
            && matches!(&m.content, ChatContent::Text(t) if t.starts_with("ReCAP ("))
    }));

    // History should not contain the recap text (only user input + assistant final reply).
    let history = ctx.history().get_all().await?;
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].role, ChatRole::User);
    assert_eq!(history[1].role, ChatRole::Assistant);

    if let ChatContent::Text(t) = &history[1].content {
        assert_eq!(t, "done");
    } else {
        panic!("expected assistant text");
    }

    Ok(())
}
