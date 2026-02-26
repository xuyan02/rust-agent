use agent_core::{Agent, AgentContextBuilder, History, Session};
use anyhow::{Context as _, Result};
use std::{cell::RefCell, collections::VecDeque, marker::PhantomData, rc::Rc, time::Duration};

pub enum BrainEvent {
    OutputText { text: String },
    Error { error: anyhow::Error },
}

pub trait BrainEventSink {
    fn emit(&mut self, event: BrainEvent);
}

/// Configuration for Brain behavior.
#[derive(Debug, Clone)]
pub struct BrainConfig {
    /// Maximum time to wait for a request to complete.
    /// Defaults to 5 minutes.
    pub request_timeout: Duration,
}

impl Default for BrainConfig {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(5 * 60), // 5 minutes
        }
    }
}

impl BrainConfig {
    /// Creates a new BrainConfig with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the request timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }
}

pub struct Brain {
    inner: Rc<RefCell<Inner>>,
    /// Notify the driver loop that new input is available.
    notify: Rc<tokio::sync::Notify>,
    // Keep sink alive for driver_loop.
    _sink: Rc<RefCell<Box<dyn BrainEventSink>>>,

    // Ensure same-thread-only usage.
    _not_send: PhantomData<Rc<()>>,
}

impl Brain {
    pub fn push_input(&self, text: impl Into<String>) {
        {
            let mut inner = self.inner.borrow_mut();
            inner.inbox.push_back(text.into());
        }
        // Wake the driver loop so it picks up the new message immediately.
        self.notify.notify_one();
    }

    pub fn shutdown(&self) {
        {
            let mut inner = self.inner.borrow_mut();
            inner.shutdown = true;
        }
        self.notify.notify_one();
    }
}

impl Drop for Brain {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Brain {
    /// Creates a new Brain with default configuration.
    pub fn new(
        session: Session,
        agent: Box<dyn Agent>,
        sink: impl BrainEventSink + 'static,
    ) -> Result<Self> {
        Self::new_with_config(session, agent, sink, BrainConfig::default(), vec![])
    }

    /// Creates a new Brain with system prompts.
    pub fn new_with_system_prompts(
        session: Session,
        agent: Box<dyn Agent>,
        sink: impl BrainEventSink + 'static,
        system_prompts: Vec<String>,
    ) -> Result<Self> {
        Self::new_with_config(session, agent, sink, BrainConfig::default(), system_prompts)
    }

    /// Creates a new Brain with custom configuration.
    pub fn new_with_config(
        session: Session,
        agent: Box<dyn Agent>,
        sink: impl BrainEventSink + 'static,
        config: BrainConfig,
        system_prompts: Vec<String>,
    ) -> Result<Self> {
        let spawner = session
            .runtime()
            .local_spawner()
            .context("Brain requires Runtime.local_spawner")?;

        let session_rc = Rc::new(session);
        let sink_rc = Rc::new(RefCell::new(Box::new(sink) as Box<dyn BrainEventSink>));
        let notify = Rc::new(tokio::sync::Notify::new());

        let inner = Rc::new(RefCell::new(Inner {
            agent: Some(agent),
            session: Rc::clone(&session_rc),
            inbox: VecDeque::new(),
            shutdown: false,
            config,
            system_prompts,
        }));

        let handle = Brain {
            inner: Rc::clone(&inner),
            notify: Rc::clone(&notify),
            _sink: Rc::clone(&sink_rc),
            _not_send: PhantomData,
        };

        spawner.spawn_local(Box::pin(driver_loop(inner, sink_rc, notify)));

        Ok(handle)
    }
}

type WorkItem = (Rc<Session>, String, Box<dyn Agent>, Vec<String>);

struct Inner {
    /// `Option` so we can `.take()` the agent across await points instead of
    /// swapping in a dummy placeholder.
    agent: Option<Box<dyn Agent>>,
    session: Rc<Session>,
    inbox: VecDeque<String>,
    shutdown: bool,
    config: BrainConfig,
    system_prompts: Vec<String>,
}

async fn driver_loop(
    inner: Rc<RefCell<Inner>>,
    sink: Rc<RefCell<Box<dyn BrainEventSink>>>,
    notify: Rc<tokio::sync::Notify>,
) {
    loop {
        let maybe_work: Option<(WorkItem, Duration)> = {
            let mut inner = inner.borrow_mut();
            if inner.shutdown && inner.inbox.is_empty() {
                return;
            }

            inner.inbox.pop_front().map(|input| {
                // Take agent out so we don't hold RefCell borrow across await.
                // Uses Option::take() instead of mem::replace with a dummy.
                let agent = inner
                    .agent
                    .take()
                    .expect("agent taken while already borrowed");
                // Clone the Rc to get a safe reference we can use across await.
                let session = Rc::clone(&inner.session);
                let timeout = inner.config.request_timeout;
                let system_prompts = inner.system_prompts.clone();

                ((session, input, agent, system_prompts), timeout)
            })
        };

        let Some(((session, input, agent, system_prompts), timeout)) = maybe_work else {
            // Efficiently wait for a notification instead of busy-looping.
            notify.notified().await;
            continue;
        };

        // Execute the request with timeout
        let res: Result<Option<String>> = match tokio::time::timeout(timeout, async {
            let mut ctx_builder = AgentContextBuilder::from_session(&session);
            for prompt in system_prompts {
                ctx_builder = ctx_builder.add_system_segment(prompt);
            }
            let ctx = ctx_builder.build()?;

            History::append(ctx.history(), &ctx, agent_core::make_user_message(input)).await?;

            agent.run(&ctx).await?;

            let msgs = History::get_all(ctx.history(), &ctx).await?;
            let last_assistant = msgs.iter().rev().find_map(|m| {
                if m.role != agent_core::llm::ChatRole::Assistant {
                    return None;
                }
                match &m.content {
                    agent_core::llm::ChatContent::Text(t) => Some(t.as_str()),
                    _ => None,
                }
            });

            Ok(last_assistant.map(|s| s.to_string()))
        })
        .await
        {
            Ok(result) => result,
            Err(_) => Err(anyhow::anyhow!("Request timed out after {:?}", timeout)),
        };

        // Put agent back.
        {
            let mut inner = inner.borrow_mut();
            inner.agent = Some(agent);
        }

        // Emit event. No need to hold Inner's borrow, sink is separate.
        {
            let mut sink = sink.borrow_mut();
            match res {
                Ok(Some(text)) => sink.emit(BrainEvent::OutputText { text }),
                Ok(None) => {}
                Err(error) => sink.emit(BrainEvent::Error { error }),
            }
        }
    }
}
