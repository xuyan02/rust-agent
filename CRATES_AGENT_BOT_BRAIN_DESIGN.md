# Self-driven Brain (agent-bot) — Design Doc

Date: 2026-02-24

## 0. Summary
We will remove `BrainDriver` and evolve `Brain` into a **self-driven, single-threaded component**.

- **Self-driven**: once constructed, `Brain` runs continuously on the **same thread** via a **local spawner attached to `agent-core::Runtime`**.
- **Single-thread contract**: all Brain state is touched **only** on the owning thread. No implicit thread hopping at framework layer.
- **Input**: external inputs are buffered internally.
- **Output**: Brain produces outputs as **events** via a **synchronous sink** that is executed on the owning thread (no polling/receiver required).

This doc focuses on the first milestone: deleting `BrainDriver` and introducing a minimal, logically complete, self-driven Brain with strict single-thread semantics.

## 1. Background / Current State
### 1.1 Current `agent-bot` layout
- `crates/agent-bot/src/brain.rs`: contains a partial state machine (`inbox`, `in_flight`, `waker`) and a `poll_output()` API.
- `crates/agent-bot/src/brain_driver.rs`: wraps Brain inside an `Rc<RefCell<_>>` and runs a driver loop that repeatedly calls `poll_output()`; output goes to `println!`.
- `crates/agent-bot/examples/brain_driver_cli.rs`: current-thread tokio runtime + `LocalSet`, select between stdin lines and `driver.run()`.

### 1.2 `agent-core::Runtime` today
`agent-core::Runtime` is currently a pure execution service:
- holds LLM provider configs
- `execute()` loops tool-calls until assistant text
- does **not** expose any executor/spawner or event system

## 2. Requirements (First Principles)
### 2.1 Invariants
1. **Single-thread**: Brain state read/write must happen on the same thread.
2. **Serialized processing**: at most one in-flight turn at a time.
3. **Non-blocking**: never block the thread; async I/O happens via `.await`.
4. **No implicit thread hop at framework layer**: any switching back to the owning thread must be explicit and local.

### 2.2 Functional goals
- Remove `BrainDriver`.
- Brain buffers external inputs.
- Brain continuously processes inputs and emits outputs as events.

### 2.3 Non-goals (Milestone 1)
- Streaming tokens / deltas.
- Tool-call begin/end events.
- Cross-thread input submission.
- Unified global event bus across all crates.

## 3. Core Design
### 3.1 Conceptual model
`Brain` is a **local actor**:
- Owns all mutable state.
- Runs a **local driver task** (spawned on a local spawner).
- Receives inputs via a handle (same-thread only).
- Emits events via a synchronous sink (same-thread only).

This replaces the external `BrainDriver` loop.

### 3.2 Threading model
- The driver task is spawned via a **local spawner** attached to `agent-core::Runtime`.
- The spawner guarantees execution on the same thread.
- Brain and its handle are explicitly **!Send + !Sync** to prevent misuse.

### 3.3 Public API (agent-bot)
We will switch from `(Brain, BrainDriver)` to a single construct:

```rust
pub struct BrainHandle {
    // !Send, same-thread only
}

pub enum BrainEvent {
    OutputText { text: String },
    Error { error: anyhow::Error },
}

impl BrainHandle {
    pub fn push_input(&self, text: impl Into<String>);
    pub fn shutdown(&self);
}

pub struct Brain;

impl Brain {
    /// Creates a self-driven Brain.
    /// Requires that `runtime` has a local spawner configured.
    pub fn new(runtime: &agent_core::Runtime, agent: Box<dyn agent_core::Agent>, sink: impl BrainEventSink) -> anyhow::Result<BrainHandle>;
}

pub trait BrainEventSink {
    fn emit(&mut self, event: BrainEvent);
}
```

Notes:
- `Brain::new(...)` returns a `BrainHandle` and immediately starts running (no `start()` method).
- Output delivery is push-based via `sink.emit(...)`. There is **no polling** and **no receiver**.

### 3.4 Why a sink (not poll/receiver)
- The goal of adding a spawner is to remove external drive loops.
- Poll/receiver would still require the consumer to explicitly wait/poll, which defeats the purpose.
- A synchronous sink keeps semantics explicit and local: Brain calls `emit` on the same thread.

### 3.5 Runtime extension
We attach a local spawner to `agent-core::Runtime` (capability-only).

```rust
pub trait LocalSpawner {
    fn spawn_local(&self, fut: std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>);
}

pub struct Runtime {
    // existing fields...
    local_spawner: Option<std::rc::Rc<dyn LocalSpawner>>,
}

impl Runtime {
    pub fn local_spawner(&self) -> Option<std::rc::Rc<dyn LocalSpawner>>;
}

impl RuntimeBuilder {
    pub fn set_local_spawner(mut self, spawner: std::rc::Rc<dyn LocalSpawner>) -> Self;
}
```

Constraints:
- `LocalSpawner` must be used only on the thread it belongs to.
- We intentionally keep `LocalSpawner` **minimal** and do not add any cross-thread `post(...)` facility.

A concrete tokio implementation (in CLI/example code) can wrap `tokio::task::LocalSet` or `tokio::task::spawn_local`.

## 4. Internal Brain State Machine
### 4.1 State
- `inbox: VecDeque<String>`
- `in_flight: Option<Future>` (one turn)
- `shutdown: bool`

All stored in an `Rc<RefCell<Inner>>` and accessed only from the owning thread.

### 4.2 Driver loop
Pseudo:
1. If `shutdown` and `in_flight == None` and `inbox.is_empty()`: exit.
2. If `in_flight` is none and inbox has input:
   - pop one input
   - create an async future that:
     - builds context
     - appends user message
     - runs agent
     - extracts last assistant text
     - returns `Result<Option<String>>`
   - set `in_flight = Some(fut)`
3. Await/poll `in_flight`:
   - on Ok(Some(text)): `sink.emit(OutputText{text})`
   - on Ok(None): emit nothing (still completes the turn)
   - on Err(e): `sink.emit(Error{e})`
   - clear `in_flight`
4. Yield (or await a lightweight notifier) and repeat.

Milestone 1 can start with a simple `yield_now()` based loop; later we can refine with a local notifier.

### 4.3 Output extraction rule
Given current `agent-core::Runtime::execute()` semantics, the turn is considered complete when `agent.run()` returns.
We extract:
- the **last** assistant `ChatContent::Text` in history after the user input was appended.

This matches current `brain.rs` behavior.

## 5. Lifecycle
### 5.1 Construction
- `Brain::new(...)` validates that `runtime.local_spawner()` is present.
- Spawns local driver task.

### 5.2 Shutdown
- `BrainHandle::shutdown()` sets `shutdown = true`.
- Driver loop exits when:
  - no in-flight future and inbox is empty.

### 5.3 Drop behavior
- Dropping the handle does not guarantee immediate stop (cannot await in `Drop`).
- We may implement `Drop for BrainHandle` to call `shutdown()` best-effort.

## 6. Single-thread enforcement
- `BrainHandle` contains `std::marker::PhantomData<std::rc::Rc<()>>` to be `!Send`.
- `LocalSpawner` is stored in `Rc<dyn LocalSpawner>` (also `!Send`).
- All state is behind `Rc<RefCell<_>>`, never accessed across await with an active borrow.

## 7. Testing Plan
### 7.1 Unit tests (agent-bot)
- `brain_processes_inputs_serially`: push two inputs, assert sink sees two OutputText in order.
- `brain_shutdown_exits`: push nothing, call shutdown, assert driver stops (observed via sink marker or JoinHandle completion if available).
- `brain_is_not_send`: compile-time check via trait bounds.

### 7.2 Integration
- Update `examples/brain_driver_cli.rs` to use new `Brain::new(..., sink)` and print in sink.

## 8. Migration Plan
1. Introduce local spawner field in `agent-core::Runtime`/`RuntimeBuilder`.
2. Implement a tokio local spawner in the CLI/example (wrapper around `tokio::task::spawn_local`).
3. Rewrite `agent-bot::Brain` to be self-driven and return `BrainHandle`.
4. Delete `brain_driver.rs` and remove exports from `agent-bot/src/lib.rs`.
5. Update example(s) and any references.
6. Add tests.

## 9. Risks / Open Issues (deferred)
- Busy-looping: a naive loop with `yield_now` is functional but may waste cycles.
  - Future improvement: add a local notifier (e.g., `tokio::sync::Notify` used on the same thread) without changing the public API.
- No streaming/tool events: requires changes in `agent-core` execution model.

---

Appendix: Rationale for eliminating `start()`
- Because the spawner is attached to `Runtime`, `Brain::new(...)` can immediately spawn its driver task.
- This keeps user-facing surface minimal and matches the requirement that Brain runs for its lifetime.
