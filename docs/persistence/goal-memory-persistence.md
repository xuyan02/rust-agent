# Goal and Memory Persistence

## Overview

Bot's goal and memory state can be persisted to disk using the DataStore. When persistence is enabled, goal and memory data are **automatically and transparently** saved to YAML files and restored when needed.

## Features

- **Completely transparent**: No manual load or save calls needed
- **Lazy loading**: Data is loaded from disk on first access
- **Immediate persistence**: Changes are flushed to disk immediately
- **Optional**: Persistence works only when Runtime has a DataStore configured

## Architecture

### Data Types

**GoalData** (`goal.yaml`):
```yaml
type_tag: GoalData
value:
  goal: "Build a robust agent system"
```

**MemoryData** (`memory.yaml`):
```yaml
type_tag: MemoryData
value:
  memories:
    - "User prefers Rust for system programming"
    - "Project uses tokio async runtime"
    - "User's timezone is UTC+8"
```

### Storage Location

When a Bot named "LeaderBot" is created with DataStore:
```
.agent/
└── LeaderBot/
    ├── goal.yaml       # Goal state
    ├── memory.yaml     # Memory records
    └── history.yaml    # Conversation history (PersistentHistory)
```

## Usage

### Creating Bot with Persistence

```rust
use agent_bot::Bot;
use agent_core::RuntimeBuilder;
use std::rc::Rc;

// Create Runtime with DataStore
let runtime = Rc::new(
    RuntimeBuilder::new()
        .set_data_store_root(".agent".into())
        .build()
);

// Create Bot - persistence is completely automatic!
let bot = Bot::new(
    runtime,
    "LeaderBot",
    "gpt-4o",
    tool_constructors,
    sink,
)?;

// That's it! No manual load/save needed.
// Bot will automatically:
// - Load goal and memory from disk on first access
// - Save changes immediately when set-goal or remember is called
```

### How Persistence Works

Persistence is **completely transparent**:

1. **First Access**: When Brain renders system prompt or calls a tool, data is automatically loaded from disk (if exists)
2. **Modifications**: When tools modify goal/memory, changes are immediately flushed to disk
3. **No Manual Management**: You never call load/save - it's all automatic

Example flow:
```
Brain starts
    ↓
System prompt is rendered
    ↓ (GoalSegment/MemorySegment auto-loads from disk)
Display goal/memory to LLM
    ↓
LLM calls set-goal tool
    ↓ (GoalTool auto-flushes to disk)
Change persisted
```

**Important**: Goal and memory are modified through tools only. There are no public methods to directly access them.

## API Reference

### Bot Methods

No public persistence methods needed! Persistence is automatic.

### Tools

Goal and memory are managed through tools:

**GoalTool functions:**
- `set-goal(goal: String)` - Set goal and persist immediately
- `get-goal()` - Get current goal (loads from disk on first call)
- `clear-goal()` - Clear goal and persist immediately

**MemoryTool functions:**
- `remember(memory: String)` - Add memory and persist immediately
- `list-memories()` - List all memories (loads from disk on first call)
- `clear-memories()` - Clear all memories and persist immediately

### Internal Implementation (for reference)

GoalState and MemoryState have internal methods used by tools:

```rust
// Used by GoalTool and GoalSegment internally
impl GoalState {
    pub(crate) fn set_persistence(&self, node: Rc<DataNode>);
    pub(crate) async fn load(&self) -> Result<()>;
    pub(crate) async fn flush(&self) -> Result<()>;
    pub(crate) fn set(&self, goal: String);
    pub(crate) fn get(&self) -> Option<String>;
    pub(crate) fn clear(&self);
}

// Used by MemoryTool and MemorySegment internally
impl MemoryState {
    pub(crate) fn set_persistence(&self, node: Rc<DataNode>);
    pub(crate) async fn load(&self) -> Result<()>;
    pub(crate) async fn flush(&self) -> Result<()>;
    pub(crate) fn add(&self, memory: String);
    pub(crate) fn get_all(&self) -> Vec<String>;
    pub(crate) fn clear(&self);
}
```

These are not public API - use tools instead.

## Implementation Details

### Caching Strategy

1. **Immediate cache update**: When `set()`, `add()`, or `clear()` is called, the change is immediately reflected in the DataNode's cache
2. **Lazy disk write**: Changes are only written to disk when `flush()` is called
3. **Dirty tracking**: DataNode tracks whether it has unsaved changes

### Without Persistence

If Runtime doesn't have a DataStore configured:
- Goal and memory operations work normally (in-memory only)
- `load_state()` and `flush_state()` are no-ops (return Ok immediately)
- No files are created

### Thread Safety

- GoalState and MemoryState use `Rc<RefCell<>>` (single-threaded)
- DataNode is also single-threaded (`!Send`)
- All operations must be on the same thread (compatible with tokio current_thread runtime)

## Best Practices

### Just Use the Tools

Persistence is completely automatic. You don't need to:
- ❌ Call any load/save methods
- ❌ Set up periodic flush timers
- ❌ Worry about when to persist

The system handles everything:
- ✅ Data loads lazily on first access
- ✅ Changes flush immediately after tool calls
- ✅ All operations are transparent

### Shutdown

Bot shutdown is simple - just call `bot.shutdown()`:

```rust
// In your shutdown logic
async fn shutdown(bot: &Bot) {
    // No need to flush - already done by tools
    bot.shutdown();
}
```

Changes are already persisted by the time tools return, so there's nothing to flush on shutdown.

## Testing

Run persistence tests:

```bash
cargo test --package agent-bot --test test_persistence
```

Tests cover:
- Goal persistence (set, flush, load, clear)
- Memory persistence (add, flush, load, clear)
- Operation without persistence node (no-op behavior)

## Error Handling

Persistence errors are handled transparently:

- **Load errors**: If loading fails (corrupted file, parse error), the system starts with empty state
- **Flush errors**: If flushing fails, the tool returns an error to the LLM
- **IO errors**: File permission or disk space issues are reported through tool results

The system is resilient - if persistence fails, the Bot continues to work (just without persistence).

## Migration Notes

### Upgrading from Non-Persistent Bots

If you have existing Bots without persistence:

1. Add DataStore to Runtime (`set_data_store_root()`)
2. That's it! Next time goal/memory is modified, it will be persisted
3. No code changes needed

### Changing Bot Name

Bot's persistence is tied to its name. Renaming a Bot requires:

1. Manually rename the directory: `.agent/OldName` → `.agent/NewName`
2. Or flush old Bot, create new Bot with new name, and recreate state
