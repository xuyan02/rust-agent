use agent_bot::{Brain, BrainConfig, BrainEvent, BrainEventSink};
use agent_core::{Agent, AgentContext, LocalSpawner, RuntimeBuilder, SessionBuilder};
use anyhow::Result;
use std::{cell::RefCell, pin::Pin, rc::Rc, time::Duration};

struct TokioLocalSpawner;

impl LocalSpawner for TokioLocalSpawner {
    fn spawn_local(&self, fut: Pin<Box<dyn std::future::Future<Output = ()>>>) {
        tokio::task::spawn_local(fut);
    }
}

struct SlowAgent {
    delay: Duration,
    response: String,
}

#[async_trait::async_trait(?Send)]
impl Agent for SlowAgent {
    async fn run(&self, ctx: &AgentContext<'_>) -> Result<()> {
        // Simulate slow processing
        tokio::time::sleep(self.delay).await;

        ctx.history()
            .append(ctx, agent_core::llm::ChatMessage::assistant_text(
                self.response.clone(),
            ))
            .await?;
        Ok(())
    }
}

struct CollectSink {
    events: Rc<RefCell<Vec<BrainEvent>>>,
}

impl BrainEventSink for CollectSink {
    fn emit(&mut self, event: BrainEvent) {
        let cloned_event = match &event {
            BrainEvent::OutputText { text } => BrainEvent::OutputText {
                text: text.clone(),
            },
            BrainEvent::Error { error } => BrainEvent::Error {
                error: anyhow::anyhow!("{}", error),
            },
        };
        self.events.borrow_mut().push(cloned_event);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn brain_request_completes_within_timeout() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let runtime = Rc::new(
                RuntimeBuilder::new()
                    .set_local_spawner(Rc::new(TokioLocalSpawner))
                    .build(),
            );

            let events = Rc::new(RefCell::new(Vec::new()));

            let session = SessionBuilder::new(runtime)
                .set_default_model("test-model".to_string())
                .build()
                .unwrap();

            // Agent that takes 100ms (within timeout)
            let agent = Box::new(SlowAgent {
                delay: Duration::from_millis(100),
                response: "success".to_string(),
            });

            let config = BrainConfig::new().with_timeout(Duration::from_secs(1));

            let brain = Brain::new_with_config(
                "test-brain",
                session,
                agent,
                CollectSink {
                    events: Rc::clone(&events),
                },
                config,
            )
            .unwrap();

            brain.push_input("test");

            // Wait for processing
            tokio::time::sleep(Duration::from_millis(200)).await;

            let events = events.borrow();
            assert_eq!(events.len(), 1);

            match &events[0] {
                BrainEvent::OutputText { text } => {
                    assert_eq!(text, "success");
                }
                BrainEvent::Error { error } => panic!("unexpected error: {}", error),
            }
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn brain_request_times_out_when_too_slow() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let runtime = Rc::new(
                RuntimeBuilder::new()
                    .set_local_spawner(Rc::new(TokioLocalSpawner))
                    .build(),
            );

            let events = Rc::new(RefCell::new(Vec::new()));

            let session = SessionBuilder::new(runtime)
                .set_default_model("test-model".to_string())
                .build()
                .unwrap();

            // Agent that takes 2 seconds (exceeds timeout)
            let agent = Box::new(SlowAgent {
                delay: Duration::from_secs(2),
                response: "should not see this".to_string(),
            });

            // Short timeout of 200ms
            let config = BrainConfig::new().with_timeout(Duration::from_millis(200));

            let brain = Brain::new_with_config(
                "test-brain-slow",
                session,
                agent,
                CollectSink {
                    events: Rc::clone(&events),
                },
                config,
            )
            .unwrap();

            brain.push_input("test");

            // Wait for timeout to trigger
            tokio::time::sleep(Duration::from_millis(400)).await;

            let events = events.borrow();
            assert_eq!(events.len(), 1);

            match &events[0] {
                BrainEvent::Error { error } => {
                    let error_msg = error.to_string();
                    assert!(
                        error_msg.contains("timed out") || error_msg.contains("timeout"),
                        "Expected timeout error, got: {}",
                        error_msg
                    );
                }
                BrainEvent::OutputText { text } => {
                    panic!("Expected timeout error, got output: {}", text)
                }
            }
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn brain_uses_default_timeout() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let runtime = Rc::new(
                RuntimeBuilder::new()
                    .set_local_spawner(Rc::new(TokioLocalSpawner))
                    .build(),
            );

            let events = Rc::new(RefCell::new(Vec::new()));

            let session = SessionBuilder::new(runtime)
                .set_default_model("test-model".to_string())
                .build()
                .unwrap();

            let agent = Box::new(SlowAgent {
                delay: Duration::from_millis(100),
                response: "with default timeout".to_string(),
            });

            // Use default config (5 minute timeout)
            let brain = Brain::new(
                "test-brain-default",
                session,
                agent,
                CollectSink {
                    events: Rc::clone(&events),
                },
            )
            .unwrap();

            brain.push_input("test");

            tokio::time::sleep(Duration::from_millis(200)).await;

            let events = events.borrow();
            assert_eq!(events.len(), 1);

            match &events[0] {
                BrainEvent::OutputText { text } => {
                    assert_eq!(text, "with default timeout");
                }
                BrainEvent::Error { error } => panic!("unexpected error: {}", error),
            }
        })
        .await;
}
