use agent_bot::{Brain, BrainEvent, BrainEventSink};
use agent_core::{
    Agent, AgentContext, LocalSpawner, RuntimeBuilder,
    llm::{ChatContent, ChatMessage, ChatRole},
};
use anyhow::Result;
use std::{cell::RefCell, pin::Pin, rc::Rc, sync::mpsc};

struct EchoAgent;

#[async_trait::async_trait(?Send)]
impl Agent for EchoAgent {
    async fn run(&self, ctx: &AgentContext<'_>) -> Result<()> {
        let all = ctx.history().get_all().await?;
        let last_user = all.iter().rev().find(|m| m.role == ChatRole::User);

        let text = match last_user.map(|m| &m.content) {
            Some(ChatContent::Text(t)) => t.clone(),
            _ => "".to_string(),
        };

        ctx.history()
            .append(ChatMessage {
                role: ChatRole::Assistant,
                content: ChatContent::Text(format!("echo:{text}")),
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
    tx: mpsc::Sender<BrainEvent>,
}

impl BrainEventSink for ChannelSink {
    fn emit(&mut self, event: BrainEvent) {
        let _ = self.tx.send(event);
    }
}

#[test]
fn brain_outputs_in_order() {
    // Drive using a tokio current-thread runtime + LocalSet.
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

        let session = agent_core::SessionBuilder::new(Rc::clone(&runtime))
            .set_default_model("gpt-4o".to_string())
            .build()
            .unwrap();

        let (tx, rx) = mpsc::channel();
        let brain = Brain::new(session, Box::new(EchoAgent), ChannelSink { tx }).unwrap();

        brain.push_input("a");
        brain.push_input("b");
        brain.push_input("c");

        let mut out = Vec::new();
        while out.len() < 3 {
            tokio::task::yield_now().await;
            match rx.recv_timeout(std::time::Duration::from_secs(2)) {
                Ok(BrainEvent::OutputText { text }) => out.push(text),
                Ok(BrainEvent::Error { error }) => panic!("unexpected error: {error:#}"),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    panic!("timed out waiting for brain output")
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    panic!("channel closed unexpectedly")
                }
            }
        }

        assert_eq!(out, vec!["echo:a", "echo:b", "echo:c"]);

        drop(brain);
    });
}
