use agent_core::{
    Agent, AgentContextBuilder, History, Runtime, Session, SessionBuilder, llm::ChatContent,
};
use anyhow::Result;
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

struct State<'a> {
    agent: Box<dyn Agent>,
    session: Session<'a>,
    inbox: VecDeque<String>,
    waker: Option<Waker>,
}

type InFlight<'a> = Pin<Box<dyn Future<Output = (State<'a>, Result<Option<String>>)> + 'a>>;

pub struct Brain<'a> {
    state: Option<State<'a>>,
    in_flight: Option<InFlight<'a>>,
}

impl<'a> Brain<'a> {
    pub fn new(runtime: &'a Runtime, agent: Box<dyn Agent>) -> Result<Self> {
        let session = SessionBuilder::new(runtime)
            .set_default_model("gpt-4o".to_string())
            .build()?;

        Ok(Self {
            state: Some(State {
                agent,
                session,
                inbox: VecDeque::new(),
                waker: None,
            }),
            in_flight: None,
        })
    }

    pub fn input(&mut self, s: String) {
        if let Some(state) = self.state.as_mut() {
            state.inbox.push_back(s);
            if let Some(w) = state.waker.as_ref() {
                w.wake_by_ref();
            }
        }
    }

    pub fn poll_output(&mut self, cx: &mut Context<'_>) -> Poll<Result<String>> {
        if let Some(fut) = self.in_flight.as_mut() {
            match fut.as_mut().poll(cx) {
                Poll::Ready((mut state, res)) => {
                    state.waker = Some(cx.waker().clone());
                    self.in_flight = None;
                    self.state = Some(state);

                    match res {
                        Ok(Some(s)) => return Poll::Ready(Ok(s)),
                        Ok(None) => return Poll::Pending,
                        Err(e) => return Poll::Ready(Err(e)),
                    }
                }
                Poll::Pending => return Poll::Pending,
            }
        }

        let mut state = self.state.take().expect("Brain: state missing");
        state.waker = Some(cx.waker().clone());

        let Some(line) = state.inbox.pop_front() else {
            self.state = Some(state);
            return Poll::Pending;
        };

        let fut = async move {
            let res: Result<Option<String>> = async {
                let ctx = AgentContextBuilder::from_session(&state.session).build()?;

                History::append(ctx.history(), agent_core::make_user_message(line)).await?;

                state.agent.run(&ctx).await?;

                let msgs = History::get_all(ctx.history()).await?;
                let last = msgs.iter().rev().find_map(|m| match &m.content {
                    ChatContent::Text(t) => Some(t.as_str()),
                    _ => None,
                });

                Ok(last.map(|s| s.to_string()))
            }
            .await;

            (state, res)
        };

        self.in_flight = Some(Box::pin(fut));
        Poll::Pending
    }
}
