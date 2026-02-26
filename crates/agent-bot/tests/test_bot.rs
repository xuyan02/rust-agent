use agent_bot::{Bot, BotEvent, BotEventSink, Envelope};
use agent_core::{Agent, AgentContext, LocalSpawner, RuntimeBuilder};
use anyhow::Result;
use std::{cell::RefCell, pin::Pin, rc::Rc, sync::mpsc};

struct ScriptedAgent {
    // Maps last user message '@from: content' -> assistant output '@to: reply'
    // Here we just echo back to the same from.
}

#[async_trait::async_trait(?Send)]
impl Agent for ScriptedAgent {
    async fn run(&self, ctx: &AgentContext<'_>) -> Result<()> {
        let all = ctx.history().get_all().await?;
        let last_user = all
            .iter()
            .rev()
            .find(|m| m.role == agent_core::llm::ChatRole::User);

        let text = match last_user.map(|m| &m.content) {
            Some(agent_core::llm::ChatContent::Text(t)) => t.as_str(),
            _ => "",
        };

        let (to, content) = match text.trim().strip_prefix('@') {
            Some(rest) => match rest.split_once(':') {
                Some((from, content)) => (from.trim(), content.trim()),
                None => ("unknown", text.trim()),
            },
            None => ("unknown", text.trim()),
        };

        ctx.history()
            .append(agent_core::llm::ChatMessage {
                role: agent_core::llm::ChatRole::Assistant,
                content: agent_core::llm::ChatContent::Text(format!("@{to}: echo:{content}")),
            })
            .await?;

        Ok(())
    }
}

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

struct ChannelSink {
    tx: mpsc::Sender<BotEvent>,
}

impl BotEventSink for ChannelSink {
    fn emit(&mut self, event: BotEvent) {
        let _ = self.tx.send(event);
    }
}

#[test]
fn bot_formats_input_and_parses_output() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, async {
        let spawner: Rc<dyn LocalSpawner> = Rc::new(TestSpawner::new());
        let runtime = Rc::new(
            RuntimeBuilder::new()
                .set_local_spawner(Rc::clone(&spawner))
                .build(),
        );

        let (tx, rx) = mpsc::channel();
        let bot = Bot::new(
            Rc::clone(&runtime),
            "botA",
            Box::new(ScriptedAgent {}),
            ChannelSink { tx },
        )
        .unwrap();

        bot.push(Envelope {
            from: "alice".to_string(),
            to: "botA".to_string(),
            content: "hi".to_string(),
        });

        tokio::task::yield_now().await;

        match rx.recv_timeout(std::time::Duration::from_secs(2)) {
            Ok(BotEvent::OutputMessage { message }) => {
                assert_eq!(
                    message,
                    Envelope {
                        from: "botA".to_string(),
                        to: "alice".to_string(),
                        content: "echo:hi".to_string(),
                    }
                );
            }
            Ok(BotEvent::Error { error }) => panic!("unexpected error: {error:#}"),
            Err(e) => panic!("no output: {e:?}"),
        }

        bot.shutdown();
    });
}

#[test]
fn bot_emits_error_on_invalid_brain_output() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, async {
        let spawner: Rc<dyn LocalSpawner> = Rc::new(TestSpawner::new());
        let runtime = Rc::new(
            RuntimeBuilder::new()
                .set_local_spawner(Rc::clone(&spawner))
                .build(),
        );

        struct BadAgent;
        #[async_trait::async_trait(?Send)]
        impl Agent for BadAgent {
            async fn run(&self, ctx: &AgentContext<'_>) -> Result<()> {
                ctx.history()
                    .append(agent_core::llm::ChatMessage {
                        role: agent_core::llm::ChatRole::Assistant,
                        content: agent_core::llm::ChatContent::Text("not-protocol".to_string()),
                    })
                    .await?;
                Ok(())
            }
        }

        let (tx, rx) = mpsc::channel();
        let bot = Bot::new(
            Rc::clone(&runtime),
            "botA",
            Box::new(BadAgent),
            ChannelSink { tx },
        )
        .unwrap();

        bot.push(Envelope {
            from: "alice".to_string(),
            to: "botA".to_string(),
            content: "hi".to_string(),
        });

        tokio::task::yield_now().await;

        match rx.recv_timeout(std::time::Duration::from_secs(2)) {
            Ok(BotEvent::Error { .. }) => {}
            Ok(BotEvent::OutputMessage { message }) => panic!("unexpected message: {message:?}"),
            Err(e) => panic!("no output: {e:?}"),
        }

        bot.shutdown();
    });
}
