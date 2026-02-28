# Team Multi-Bot Collaboration System

## Overview

Implemented a `Team` system that enables multiple `Bot` instances to collaborate together. The Team consists of:
- One leader Bot that can create other Bots and communicate with the user
- Multiple worker Bots that can communicate with each other
- A user participant (with a name) who interacts with the leader

## Architecture

### Core Components

#### 1. Team Struct
```rust
pub struct Team {
    inner: Rc<RefCell<Inner>>,
}

struct Inner {
    runtime: Rc<Runtime>,
    user_name: String,
    leader_name: String,
    bots: HashMap<String, Bot>,
    sink: Box<dyn TeamEventSink>,
}
```

The Team owns all Bot instances and manages their lifecycle and communication.

#### 2. TeamEvent
```rust
pub enum TeamEvent {
    /// Message from leader to user
    UserMessage { content: String },
    /// A new bot was created
    BotCreated { name: String },
    /// Error occurred
    Error { error: anyhow::Error },
}
```

Events emitted by the Team to external observers.

#### 3. TeamBotSink
```rust
struct TeamBotSink {
    inner: Rc<RefCell<Inner>>,
}
```

Internal event sink that routes messages between bots and enforces communication rules.

## Key Features

### 1. Leader-Based Architecture
- One Bot is designated as the leader during Team creation
- Only the leader can send messages to the user
- Non-leader Bots attempting to contact the user will trigger an error

### 2. Dynamic Bot Creation
```rust
pub fn create_bot(&self, name: impl Into<String>, agent: Box<dyn Agent>) -> Result<()>
```

The Team can dynamically create new Bots at runtime. Each Bot:
- Has a unique name (duplicates are rejected)
- Runs its own Agent implementation
- Can communicate with other Bots via messages

### 3. Message Routing

The `TeamBotSink` implements intelligent message routing:

```rust
impl BotEventSink for TeamBotSink {
    fn emit(&mut self, event: BotEvent) {
        match event {
            BotEvent::OutputMessage { message } => {
                if message.to == user_name {
                    // Only leader can send to user
                    if message.from == leader_name {
                        emit(TeamEvent::UserMessage { ... })
                    } else {
                        emit(TeamEvent::Error { ... })
                    }
                } else {
                    // Route to target bot
                    if let Some(target_bot) = bots.get(&message.to) {
                        target_bot.push(envelope);
                    } else {
                        emit(TeamEvent::Error { ... })
                    }
                }
            }
            // ...
        }
    }
}
```

**Routing Rules:**
- `user → leader`: User messages are routed to the leader Bot
- `leader → user`: Leader can send messages back to the user
- `bot → bot`: Bots can send messages to each other
- `worker → user`: **Blocked** - only leader can communicate with user
- `any → unknown`: Emits error for unknown recipients

### 4. Concurrency Safety

The implementation uses `Rc<RefCell<Inner>>` for safe mutation in a single-threaded async context:

```rust
// Careful borrow management to avoid RefCell panics
let target_exists = inner.bots.contains_key(&message.to);
if target_exists {
    let envelope = message.clone();
    drop(inner); // Drop mutable borrow

    // Re-borrow immutably to get target
    let inner = self.inner.borrow();
    if let Some(target_bot) = inner.bots.get(&envelope.to) {
        target_bot.push(envelope);
    }
}
```

Key techniques:
- Clone necessary data before calling methods that may borrow
- Drop borrows explicitly before re-borrowing
- Use `contains_key` to check existence before borrowing for access

## API Reference

### Team::new
```rust
pub fn new(
    runtime: Rc<Runtime>,
    user_name: impl Into<String>,
    leader_name: impl Into<String>,
    leader_agent: Box<dyn Agent>,
    sink: impl TeamEventSink + 'static,
) -> Result<Self>
```

Creates a new Team with a leader Bot.

### Team::send_user_message
```rust
pub fn send_user_message(&self, content: impl Into<String>)
```

Sends a message from the user to the leader Bot.

### Team::create_bot
```rust
pub fn create_bot(&self, name: impl Into<String>, agent: Box<dyn Agent>) -> Result<()>
```

Creates a new Bot and adds it to the Team. Returns an error if a Bot with the same name already exists.

### Team::send_bot_message
```rust
pub fn send_bot_message(
    &self,
    from: impl Into<String>,
    to: impl Into<String>,
    content: impl Into<String>
)
```

Sends a message from one Bot to another. For external orchestration if needed.

### Team::shutdown
```rust
pub fn shutdown(&self)
```

Gracefully shuts down all Bots in the Team.

### Team Utility Methods
```rust
pub fn bot_count(&self) -> usize
pub fn leader_name(&self) -> String
pub fn user_name(&self) -> String
pub fn list_bots(&self) -> Vec<String>
```

## Usage Example

```rust
use agent_bot::{Team, TeamEvent, TeamEventSink};
use agent_core::{RuntimeBuilder, LocalSpawner};

struct MyTeamSink;

impl TeamEventSink for MyTeamSink {
    fn emit(&mut self, event: TeamEvent) {
        match event {
            TeamEvent::UserMessage { content } => {
                println!("Leader says: {}", content);
            }
            TeamEvent::BotCreated { name } => {
                println!("New bot created: {}", name);
            }
            TeamEvent::Error { error } => {
                eprintln!("Team error: {}", error);
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let runtime = Rc::new(RuntimeBuilder::new()
        .set_local_spawner(spawner)
        .build());

    let team = Team::new(
        runtime,
        "Alice",          // user name
        "LeaderBot",      // leader name
        leader_agent,     // leader's Agent impl
        MyTeamSink,       // event sink
    ).unwrap();

    // User sends message to leader
    team.send_user_message("Hello, please create a worker bot");

    // Leader can create worker bots
    team.create_bot("Worker1", worker_agent).unwrap();

    // Bots communicate with each other automatically via their Agents
    // Leader's Agent can output JSON to send messages:
    // {"to": "Worker1", "content": "Process this task"}
}
```

## Test Coverage

Implemented 9 comprehensive tests:

1. **team_creates_leader_bot** - Verifies Team initialization with leader
2. **team_user_message_to_leader** - Tests user→leader→user communication flow
3. **team_creates_additional_bot** - Tests dynamic bot creation
4. **team_prevents_duplicate_bot_names** - Ensures name uniqueness
5. **team_bot_to_bot_communication** - Tests bot↔bot messaging
6. **team_non_leader_cannot_send_to_user** - Enforces leader-only user access
7. **team_multi_bot_collaboration** - Tests 3+ bot collaboration workflow
8. **team_shutdown_stops_all_bots** - Verifies graceful shutdown
9. **team_message_to_unknown_bot_emits_error** - Tests error handling

All tests pass successfully.

## Integration

The Team module is exported from `agent-bot`:

```rust
// crates/agent-bot/src/lib.rs
pub use team::{Team, TeamEvent, TeamEventSink};
```

## Design Decisions

### 1. Why Single Leader?
- Provides clear ownership of user communication
- Prevents conflicting messages to user
- Simplifies coordination and task delegation
- Leader acts as orchestrator/coordinator

### 2. Why Use Bot Instead of Bare Agent?
- Bot already provides message parsing and routing infrastructure
- Leverages existing JSON protocol for bot-to-bot communication
- Maintains consistency with existing abstractions
- Each Bot gets its own message queue and processing loop

### 3. Why Rc<RefCell<>> Pattern?
- Team operates in single-threaded async context (LocalSet)
- Need shared mutable access from multiple bots
- Rc provides shared ownership without thread overhead
- RefCell enables interior mutability with runtime borrow checking

### 4. Message Routing in Sink
- Centralizes routing logic in one place
- Enforces rules (leader-only user access) automatically
- Bots don't need to know about routing rules
- Makes system behavior predictable and debuggable

## Future Enhancements

Potential improvements for the Team system:

1. **CreateBot Tool** - Add a Tool that the leader Agent can use to dynamically create bots through function calls
2. **Bot Status Tracking** - Track bot states (idle, working, error)
3. **Task Queue** - Add a task queue for coordinating work
4. **Bot Capabilities** - Define capabilities/roles for each bot
5. **Message History** - Track bot-to-bot message history
6. **Broadcast Messages** - Support broadcasting to all bots
7. **Bot Groups** - Organize bots into sub-teams
8. **Leader Delegation** - Allow temporary leadership transfer

## Files Modified

- `crates/agent-bot/src/lib.rs` - Added team module export
- `crates/agent-bot/src/team.rs` - New file (231 lines)
- `crates/agent-bot/tests/test_team.rs` - New test file (9 tests, 438 lines)

## Lines of Code

- Implementation: ~230 lines
- Tests: ~440 lines
- Total: ~670 lines
