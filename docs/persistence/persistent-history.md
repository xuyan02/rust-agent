# PersistentHistory Implementation

## Overview

`PersistentHistory` is a `History` implementation that persists conversation messages to disk using the `DataStore` infrastructure. It dynamically retrieves the storage location from `AgentContext`'s `dir_node`, making it flexible and context-aware.

## Key Features

- **Context-Aware Storage**: Retrieves storage location from `AgentContext.dir_node()` at runtime
- **Automatic Persistence**: Messages are automatically flushed to disk after each `append()` call
- **Type-Safe Storage**: Uses `TypeInfo` trait for runtime type verification
- **Sliding Window**: Supports maximum size limit with automatic pruning of old messages
- **Lazy Loading**: Data is loaded from disk only when needed
- **Hierarchical Organization**: Works with `DirNode` for organized storage

## Architecture

### Storage Format

Messages are stored in a YAML file at `{dir_node}/history.yaml` with type metadata:

```yaml
type_tag: agent_core::history::HistoryData
value:
  messages:
    - role: User
      content:
        Text: "Hello"
    - role: Assistant
      content:
        Text: "Hi there!"
```

### Components

1. **HistoryData**: Internal storage wrapper with `TypeInfo` implementation
   ```rust
   struct HistoryData {
       messages: Vec<ChatMessage>,
   }
   ```

2. **PersistentHistory**: Public API implementing `History` trait
   ```rust
   pub struct PersistentHistory {
       max_size: usize,
   }
   ```

3. **History Trait**: Now receives `AgentContext` parameter
   ```rust
   #[async_trait(?Send)]
   pub trait History {
       async fn get_all(&self, ctx: &AgentContext<'_>) -> Result<Vec<ChatMessage>>;
       async fn append(&self, ctx: &AgentContext<'_>, message: ChatMessage) -> Result<()>;
       async fn last(&self, ctx: &AgentContext<'_>) -> Result<Option<ChatMessage>>;
       async fn get_recent(&self, ctx: &AgentContext<'_>, n: usize) -> Result<Vec<ChatMessage>>;
   }
   ```

## Usage

### Basic Usage

```rust
use agent_core::{
    AgentContextBuilder, DataStore, History, PersistentHistory,
    RuntimeBuilder, SessionBuilder,
};
use std::rc::Rc;

// 1. Create Runtime with DataStore
let runtime = Rc::new(
    RuntimeBuilder::new()
        .set_data_store_root("/path/to/data".into())
        .build()
);

// 2. Create Session
let session = SessionBuilder::new(Rc::clone(&runtime))
    .set_default_model("gpt-4o".to_string())
    .build()?;

// 3. Setup DataStore and create directory node
let data_store = runtime.data_store().expect("runtime should have data store");
let store_rc = Rc::new(DataStore::new(data_store.root().to_path_buf()));
let dir_node = store_rc.root_dir().subdir("my_context");

// 4. Create PersistentHistory (no node parameter needed!)
let history: Box<dyn History> = Box::new(PersistentHistory::new());

// 5. Create AgentContext with history and dir_node
let ctx = AgentContextBuilder::from_session(&session)
    .set_history(history)
    .set_dir_node(dir_node)  // PersistentHistory will use this
    .build()?;

// 6. Use history - storage location is retrieved from ctx
ctx.history().append(&ctx, ChatMessage::user_text("Hello")).await?;
ctx.history().append(&ctx, ChatMessage::assistant_text("Hi!")).await?;

// Messages saved to: /path/to/data/my_context/history.yaml
let messages = ctx.history().get_all(&ctx).await?;
```

### Session-Level Storage

Set `dir_node` at Session level - all contexts will inherit it:

```rust
// Create DataStore
let data_store = runtime.data_store().expect("runtime should have data store");
let store_rc = Rc::new(DataStore::new(data_store.root().to_path_buf()));

// Set dir_node at Session level
let session_dir = store_rc.root_dir().subdir("session_storage");
let session = SessionBuilder::new(Rc::clone(&runtime))
    .set_default_model("gpt-4o".to_string())
    .set_dir_node(session_dir)  // All contexts inherit this
    .build()?;

// Create context without dir_node - inherits from Session
let history: Box<dyn History> = Box::new(PersistentHistory::new());
let ctx = AgentContextBuilder::from_session(&session)
    .set_history(history)
    .build()?;

// Messages saved to: /path/to/data/session_storage/history.yaml
ctx.history().append(&ctx, ChatMessage::user_text("Hello")).await?;
```

### Context-Level Storage (Override Session)

Override Session's `dir_node` with context-specific storage:

```rust
// Session has default dir_node
let session_dir = store_rc.root_dir().subdir("session_default");
let session = SessionBuilder::new(runtime)
    .set_dir_node(session_dir)
    .build()?;

// Context 1 uses session's dir_node
let ctx1 = AgentContextBuilder::from_session(&session)
    .set_history(Box::new(PersistentHistory::new()))
    .build()?;
// Saves to: /path/to/data/session_default/history.yaml

// Context 2 overrides with its own dir_node
let context2_dir = store_rc.root_dir().subdir("context2_private");
let ctx2 = AgentContextBuilder::from_session(&session)
    .set_history(Box::new(PersistentHistory::new()))
    .set_dir_node(context2_dir)  // Override!
    .build()?;
// Saves to: /path/to/data/context2_private/history.yaml
```

### Hierarchical Organization

```rust
// Create separate histories for different contexts
let context1_dir = store_rc.root_dir().subdir("context1");
let history1: Box<dyn History> = Box::new(PersistentHistory::new());
let ctx1 = AgentContextBuilder::from_session(&session)
    .set_history(history1)
    .set_dir_node(context1_dir)
    .build()?;

let context2_dir = store_rc.root_dir().subdir("context2");
let history2: Box<dyn History> = Box::new(PersistentHistory::new());
let ctx2 = AgentContextBuilder::from_session(&session)
    .set_history(history2)
    .set_dir_node(context2_dir)
    .build()?;

// Files will be saved to:
// - /path/to/data/context1/history.yaml
// - /path/to/data/context2/history.yaml
```

## API

### Constructor Methods

- `PersistentHistory::new()` - Create with default max size (1000)
- `PersistentHistory::new_with_limit(max_size: usize)` - Custom max size

### History Trait Implementation

All methods now require `&AgentContext` parameter:

- `async fn get_all(&self, ctx: &AgentContext<'_>) -> Result<Vec<ChatMessage>>`
- `async fn append(&self, ctx: &AgentContext<'_>, message: ChatMessage) -> Result<()>`
- `async fn last(&self, ctx: &AgentContext<'_>) -> Result<Option<ChatMessage>>`
- `async fn get_recent(&self, ctx: &AgentContext<'_>, n: usize) -> Result<Vec<ChatMessage>>`

### Session and AgentContext Integration

Both Session and AgentContext can hold a `dir_node`:

**Session:**
- `Session::dir_node() -> Option<Rc<DirNode>>` - Get session's storage directory
- `SessionBuilder::set_dir_node(dir_node: Rc<DirNode>)` - Set session-level storage

**AgentContext:**
- `AgentContext::dir_node() -> Option<Rc<DirNode>>` - Get context's storage directory (falls back to Session's dir_node)
- `AgentContextBuilder::set_dir_node(dir_node: Rc<DirNode>)` - Set context-level storage (overrides Session's)

## Implementation Details

### Context-Aware Storage Resolution

PersistentHistory retrieves the storage node dynamically:

```rust
fn get_node(&self, ctx: &AgentContext<'_>) -> Result<Rc<DataNode>> {
    let dir_node = ctx.dir_node()
        .ok_or_else(|| anyhow::anyhow!("AgentContext has no dir_node set"))?;
    Ok(dir_node.node("history"))
}
```

### History Trait Changes

The `History` trait now receives `AgentContext` as a parameter, enabling:
- Dynamic storage location resolution
- Context-specific configuration
- Access to session/runtime information

### Serialization

`ChatMessage`, `ChatRole`, and `ChatContent` are serializable:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: ChatContent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatRole {
    System, User, Assistant, Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ChatContent {
    Text(String),
    ToolCalls(Value),
    ToolResult { tool_call_id: String, result: String },
}
```

### Sliding Window

When `max_size` is exceeded, oldest messages are automatically removed:

```rust
if data.messages.len() > self.max_size {
    let keep_from = data.messages.len() - self.max_size;
    data.messages.drain(0..keep_from);
}
```

## Design Decisions

1. **Context-Based Storage**: PersistentHistory doesn't hold storage references; it retrieves them from `AgentContext` at runtime
   - Flexible: Each context can have its own storage location
   - Clean: No need to pass storage nodes during construction
   - Context-aware: Storage location is tied to the context's lifecycle

2. **History Trait with Context Parameter**: All `History` methods now receive `&AgentContext`
   - Enables dynamic behavior based on context
   - Allows implementations to access session/runtime information
   - Breaking change but provides much more flexibility

3. **Session and AgentContext dir_node**: Added optional `DirNode` field to both
   - **Session.dir_node**: Provides default storage location for all contexts in that session
   - **AgentContext.dir_node**: Can override Session's location for context-specific storage
   - Falls back: Context → Parent Context → Session → None
   - Hierarchical: Allows flexible storage organization
   - Optional: Only needed when using PersistentHistory or other storage-aware components

4. **Automatic Flush**: Every `append()` immediately flushes to disk for data safety

5. **Type Tags**: Runtime type verification prevents deserialization errors

6. **Sliding Window**: Prevents unbounded growth while maintaining recent context

## Testing

Comprehensive tests are available in `crates/agent-tests/tests/test_persistent_history.rs`:

- `test_persistent_history_basic` - Basic append/read operations
- `test_persistent_history_persistence` - Verify disk persistence across contexts
- `test_persistent_history_sliding_window` - Max size enforcement
- `test_persistent_history_get_recent` - Recent message retrieval
- `test_persistent_history_subdirectory` - Hierarchical organization

Run tests:
```bash
cargo test -p agent-tests --test test_persistent_history
```

All tests pass ✓

## Related Components

- **DataStore** (`src/data_store.rs`) - Type-safe persistent storage
- **DataNode** - Individual YAML file storage
- **DirNode** - Hierarchical organization
- **TypeInfo** - Runtime type verification
- **History** trait (`src/history.rs`) - Abstract history interface with context parameter
- **InMemoryHistory** - In-memory alternative (also uses context parameter)
- **AgentContext** - Now holds optional `dir_node` for storage

## Files Modified

1. **crates/agent-core/src/history.rs**
   - Modified `History` trait to accept `&AgentContext` parameter
   - Implemented `PersistentHistory` without node parameter
   - Updated `InMemoryHistory` to accept context (ignored)

2. **crates/agent-core/src/session.rs**
   - Added `dir_node: Option<Rc<DirNode>>` field
   - Added `dir_node()` method
   - Added `set_dir_node()` to builder

3. **crates/agent-core/src/agent_context.rs**
   - Added `dir_node: Option<Rc<DirNode>>` field
   - Added `dir_node()` method with fallback chain (self → parent context → session)
   - Added `set_dir_node()` to builder

4. **crates/agent-core/src/llm/mod.rs**
   - Added `Serialize`/`Deserialize` to `ChatMessage` types

5. **crates/agent-core/src/lib.rs**
   - Exported `PersistentHistory` and `DirNode`

6. **All History usage sites updated**:
   - `crates/agent-core/src/runtime.rs`
   - `crates/agent-core/src/agent.rs`
   - `crates/agent-core/src/react_agent.rs`
   - `crates/agent-core/src/tools/deep_think.rs`
   - `crates/agent-bot/src/brain.rs`
   - `crates/agent-bot/src/bot.rs`
   - `crates/agent-cli/src/app.rs`

7. **Tests and Examples**:
   - `crates/agent-tests/tests/test_persistent_history.rs` - Basic PersistentHistory tests
   - `crates/agent-tests/tests/test_session_dir_node.rs` - Session-level dir_node tests
   - `crates/agent-core/examples/persistent_history_usage.rs` - Usage example

## Migration Guide

If you have existing code using the old `History` trait:

### Before
```rust
history.append(message).await?;
let all = history.get_all().await?;
```

### After
```rust
history.append(&ctx, message).await?;
let all = history.get_all(&ctx).await?;
```

### Creating PersistentHistory

**Before (old approach that held node reference):**
```rust
let history_node = dir_node.node("history");
let history = PersistentHistory::new(history_node);
```

**After (new context-aware approach):**
```rust
let history: Box<dyn History> = Box::new(PersistentHistory::new());
let ctx = AgentContextBuilder::from_session(&session)
    .set_history(history)
    .set_dir_node(dir_node)  // Storage location set here
    .build()?;
```
