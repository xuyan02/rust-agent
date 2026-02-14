use agent_bot::Brain;
use agent_core::{
    Agent, AgentContext, RuntimeBuilder,
    llm::{ChatContent, ChatMessage, ChatRole},
};
use anyhow::Result;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

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

fn noop_waker() -> Waker {
    unsafe fn clone(_: *const ()) -> RawWaker {
        RawWaker::new(std::ptr::null(), &VTABLE)
    }
    unsafe fn wake(_: *const ()) {}
    unsafe fn wake_by_ref(_: *const ()) {}
    unsafe fn drop(_: *const ()) {}

    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);

    unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
}

#[test]
fn brain_outputs_in_order() {
    let runtime = RuntimeBuilder::new().build();
    let mut brain = Brain::new(&runtime, Box::new(EchoAgent)).unwrap();

    brain.input("a".to_string());
    brain.input("b".to_string());
    brain.input("c".to_string());

    // Drive using a tokio current-thread runtime.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        let mut out = vec![];
        while out.len() < 3 {
            match brain.poll_output(&mut cx) {
                Poll::Ready(Ok(s)) => out.push(s),
                Poll::Ready(Err(e)) => panic!("unexpected error: {e:#}"),
                Poll::Pending => {
                    // Yield so agent futures can make progress.
                    tokio::task::yield_now().await;
                }
            }
        }

        assert_eq!(
            out,
            vec![
                "echo:a".to_string(),
                "echo:b".to_string(),
                "echo:c".to_string()
            ]
        );
    });
}
