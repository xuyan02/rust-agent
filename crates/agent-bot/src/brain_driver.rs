use crate::Brain;
use anyhow::Result;
use std::{
    cell::RefCell,
    collections::VecDeque,
    rc::Rc,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};

pub struct BrainDriverHandle<'a> {
    inner: Rc<RefCell<Inner<'a>>>,
}

impl<'a> BrainDriverHandle<'a> {
    pub fn input(&self, s: String) {
        let mut inner = self.inner.borrow_mut();
        inner.inbox.push_back(s);
    }

    pub fn shutdown(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.running = false;
    }
}

pub struct BrainDriver<'a> {
    inner: Rc<RefCell<Inner<'a>>>,
}

impl<'a> BrainDriver<'a> {
    pub fn new(brain: Brain<'a>) -> (Self, BrainDriverHandle<'a>) {
        let inner = Rc::new(RefCell::new(Inner {
            brain: Some(brain),
            inbox: VecDeque::new(),
            running: true,
        }));

        (
            Self {
                inner: Rc::clone(&inner),
            },
            BrainDriverHandle { inner },
        )
    }

    /// Run the driver loop.
    ///
    /// This must be awaited on the same thread that owns the `Brain` (single-thread model),
    /// typically inside a tokio `LocalSet` / current-thread runtime.
    pub async fn run(&self) -> Result<()> {
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        loop {
            // Move brain and inbox out without holding a borrow across await.
            let (brain, mut inputs, running) = {
                let mut inner = self.inner.borrow_mut();
                let running = inner.running;
                let brain = inner.brain.take();
                let inputs: VecDeque<String> = inner.inbox.drain(..).collect();
                (brain, inputs, running)
            };

            if !running {
                return Ok(());
            }

            let Some(mut brain) = brain else {
                // Should not happen; yield and retry.
                tokio::task::yield_now().await;
                continue;
            };

            while let Some(s) = inputs.pop_front() {
                brain.input(s);
            }

            match brain.poll_output(&mut cx) {
                Poll::Ready(Ok(s)) => {
                    println!("{s}");
                }
                Poll::Ready(Err(e)) => {
                    // Put brain back so caller can inspect state if desired.
                    {
                        let mut inner = self.inner.borrow_mut();
                        inner.brain = Some(brain);
                        inner.running = false;
                    }
                    return Err(e);
                }
                Poll::Pending => {
                    {
                        let mut inner = self.inner.borrow_mut();
                        inner.brain = Some(brain);
                    }
                    tokio::task::yield_now().await;
                    continue;
                }
            }

            {
                let mut inner = self.inner.borrow_mut();
                inner.brain = Some(brain);
            }

            // Allow in-flight futures to progress.
            tokio::task::yield_now().await;
        }
    }
}

struct Inner<'a> {
    brain: Option<Brain<'a>>,
    inbox: VecDeque<String>,
    running: bool,
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
