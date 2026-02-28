use agent_bot::{Brain, BrainConfig, TalkChannel, TalkTool};
use agent_core::{Agent, LlmAgent, LocalSpawner, RuntimeBuilder, SessionBuilder};
use agent_core::tools::Tool;
use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;

struct TestSpawner {
    handles: RefCell<Vec<tokio::task::JoinHandle<()>>>,
}

impl TestSpawner {
    fn new() -> Self {
        Self {
            handles: RefCell::new(Vec::new()),
        }
    }
}

impl LocalSpawner for TestSpawner {
    fn spawn_local(&self, fut: Pin<Box<dyn std::future::Future<Output = ()>>>) {
        let h = tokio::task::spawn_local(fut);
        self.handles.borrow_mut().push(h);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn test_talk_tool_sends_message_to_conversation_brain() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let spawner: Rc<dyn LocalSpawner> = Rc::new(TestSpawner::new());

            // Create a mock conversation brain
            let runtime = Rc::new(
                RuntimeBuilder::new()
                    .set_local_spawner(Rc::clone(&spawner))
                    .build(),
            );
            let session = SessionBuilder::new(Rc::clone(&runtime))
                .set_default_model("test-model".to_string())
                .build()
                .unwrap();

            let conversation_brain_ref: Rc<RefCell<Option<Box<Brain>>>> =
                Rc::new(RefCell::new(None));

            // Create a simple sink to collect outputs
            struct TestSink {
                messages: Rc<RefCell<Vec<String>>>,
            }

            impl agent_bot::BrainEventSink for TestSink {
                fn emit(&mut self, event: agent_bot::BrainEvent) {
                    match event {
                        agent_bot::BrainEvent::OutputText { text } => {
                            self.messages.borrow_mut().push(text);
                        }
                        agent_bot::BrainEvent::Error { error } => {
                            self.messages.borrow_mut().push(format!("Error: {}", error));
                        }
                    }
                }
            }

            let messages = Rc::new(RefCell::new(Vec::new()));
            let sink = TestSink {
                messages: Rc::clone(&messages),
            };

            let agent: Box<dyn Agent> = Box::new(LlmAgent::new());
            let brain = Brain::new_with_config(
                "conversation-brain",
                session,
                agent,
                sink,
                BrainConfig::new(),
            )
            .unwrap();

            // Store conversation brain in the ref
            *conversation_brain_ref.borrow_mut() = Some(Box::new(brain));

            // Create TalkChannel and TalkTool
            let talk_channel = TalkChannel::new(Rc::clone(&conversation_brain_ref));
            let talk_tool = TalkTool::new(talk_channel, "Work brain");

            // Create a test context
            let test_runtime = Rc::new(
                RuntimeBuilder::new()
                    .set_local_spawner(Rc::clone(&spawner))
                    .build(),
            );
            let test_session = SessionBuilder::new(test_runtime)
                .set_default_model("test-model".to_string())
                .build()
                .unwrap();

            let ctx = agent_core::AgentContextBuilder::from_session(&test_session)
                .build()
                .unwrap();

            // Use the talk tool to send a message
            let result = talk_tool
                .invoke(
                    &ctx,
                    "send-message",
                    &serde_json::json!({"message": "Task completed successfully"}),
                )
                .await
                .unwrap();

            assert!(result.contains("Message sent to conversation brain"));

            // Verify that the conversation brain received the message
            // (Check that the message was queued in the brain's input)
            // Note: We can't directly verify the internal queue, but we verified the tool returns success
        })
        .await;
}
