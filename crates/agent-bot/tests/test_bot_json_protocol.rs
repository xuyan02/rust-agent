use agent_bot::{Bot, BotEvent, BotEventSink, Envelope};
use agent_core::{Agent, AgentContext, LocalSpawner, RuntimeBuilder, SessionBuilder};
use anyhow::Result;
use std::{cell::RefCell, pin::Pin, rc::Rc};

struct TokioLocalSpawner;

impl LocalSpawner for TokioLocalSpawner {
    fn spawn_local(&self, fut: Pin<Box<dyn std::future::Future<Output = ()>>>) {
        tokio::task::spawn_local(fut);
    }
}

struct TestAgent {
    response: String,
}

#[async_trait::async_trait(?Send)]
impl Agent for TestAgent {
    async fn run(&self, ctx: &AgentContext<'_>) -> Result<()> {
        ctx.history()
            .append(agent_core::llm::ChatMessage::assistant_text(
                self.response.clone(),
            ))
            .await?;
        Ok(())
    }
}

struct CollectSink {
    events: Rc<RefCell<Vec<BotEvent>>>,
}

impl BotEventSink for CollectSink {
    fn emit(&mut self, event: BotEvent) {
        let cloned_event = match &event {
            BotEvent::OutputMessage { message } => BotEvent::OutputMessage {
                message: message.clone(),
            },
            BotEvent::Error { error } => BotEvent::Error {
                error: anyhow::anyhow!("{}", error),
            },
        };
        self.events.borrow_mut().push(cloned_event);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn bot_parses_json_direct() {
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

            let agent = Box::new(TestAgent {
                response: r#"{"to": "alice", "content": "Hello from JSON!"}"#.to_string(),
            });

            let bot = Bot::new_with_session(
                session,
                "test-bot",
                agent,
                CollectSink {
                    events: Rc::clone(&events),
                },
            )
            .unwrap();

            bot.push(Envelope {
                from: "alice".to_string(),
                to: "test-bot".to_string(),
                content: "Hi there".to_string(),
            });

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            let events = events.borrow();
            assert_eq!(events.len(), 1);

            match &events[0] {
                BotEvent::OutputMessage { message } => {
                    assert_eq!(message.from, "test-bot");
                    assert_eq!(message.to, "alice");
                    assert_eq!(message.content, "Hello from JSON!");
                }
                BotEvent::Error { error } => panic!("unexpected error: {}", error),
            }
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn bot_parses_json_from_markdown() {
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

            let agent = Box::new(TestAgent {
                response: r#"Sure! Here's the response:

```json
{
  "to": "bob",
  "content": "Multi-line\ncontent works!"
}
```
"#
                .to_string(),
            });

            let bot = Bot::new_with_session(
                session,
                "test-bot",
                agent,
                CollectSink {
                    events: Rc::clone(&events),
                },
            )
            .unwrap();

            bot.push(Envelope {
                from: "bob".to_string(),
                to: "test-bot".to_string(),
                content: "Test".to_string(),
            });

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            let events = events.borrow();
            assert_eq!(events.len(), 1);

            match &events[0] {
                BotEvent::OutputMessage { message } => {
                    assert_eq!(message.from, "test-bot");
                    assert_eq!(message.to, "bob");
                    assert_eq!(message.content, "Multi-line\ncontent works!");
                }
                BotEvent::Error { error } => panic!("unexpected error: {}", error),
            }
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn bot_fallback_to_text_protocol() {
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

            let agent = Box::new(TestAgent {
                response: "@charlie: Legacy format still works".to_string(),
            });

            let bot = Bot::new_with_session(
                session,
                "test-bot",
                agent,
                CollectSink {
                    events: Rc::clone(&events),
                },
            )
            .unwrap();

            bot.push(Envelope {
                from: "charlie".to_string(),
                to: "test-bot".to_string(),
                content: "Test".to_string(),
            });

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            let events = events.borrow();
            assert_eq!(events.len(), 1);

            match &events[0] {
                BotEvent::OutputMessage { message } => {
                    assert_eq!(message.from, "test-bot");
                    assert_eq!(message.to, "charlie");
                    assert_eq!(message.content, "Legacy format still works");
                }
                BotEvent::Error { error } => panic!("unexpected error: {}", error),
            }
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn bot_json_with_colon_in_content() {
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

            let agent = Box::new(TestAgent {
                response: r#"{"to": "dave", "content": "The ratio is 3:1 and time is 12:30"}"#
                    .to_string(),
            });

            let bot = Bot::new_with_session(
                session,
                "test-bot",
                agent,
                CollectSink {
                    events: Rc::clone(&events),
                },
            )
            .unwrap();

            bot.push(Envelope {
                from: "dave".to_string(),
                to: "test-bot".to_string(),
                content: "Test".to_string(),
            });

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            let events = events.borrow();
            assert_eq!(events.len(), 1);

            match &events[0] {
                BotEvent::OutputMessage { message } => {
                    assert_eq!(message.content, "The ratio is 3:1 and time is 12:30");
                }
                BotEvent::Error { error } => panic!("unexpected error: {}", error),
            }
        })
        .await;
}
