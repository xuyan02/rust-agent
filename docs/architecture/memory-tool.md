# Memory Tool for Agent Bot

## Overview

The Memory Tool allows bots to record and recall important information across conversations. Memories are stored as short records and automatically appear in all brains' system prompts.

## Features

- **remember**: Add a new memory record (keep it concise, 1-2 sentences)
- **list-memories**: List all recorded memories
- **clear-memories**: Clear all memories

## Architecture

### Components

1. **MemoryState**: Shared state that stores memory records
   - Clone-able, thread-local (Rc<RefCell>)
   - Methods: `add()`, `get_all()`, `clear()`

2. **MemorySegment**: System prompt segment that renders memories
   - Implements `SystemPromptSegment` trait
   - Renders memories in markdown format with numbered list

3. **MemoryTool**: Tool interface for managing memories
   - Provides `remember`, `list-memories`, `clear-memories` functions
   - Available to both Conversation Brain and Work Brain

## Usage Example

### From Conversation Brain

```
@bot: Remember that I prefer Rust for system programming.

[Bot calls: remember(memory="User prefers Rust for system programming")]
Bot: Memory recorded: User prefers Rust for system programming

@bot: What programming languages do I like?

[Bot sees in system prompt:]
═══════════════════════════════════════════════════════
MEMORY:
1. User prefers Rust for system programming
═══════════════════════════════════════════════════════

Bot: Based on my records, you prefer Rust for system programming.
```

### From Work Brain

Work Brain can also use the memory tool during task execution:

```
[Work Brain during ReAct execution]
[think] I should record this important finding about the codebase structure
[act] Record the finding using the remember tool

[Calls: remember(memory="Codebase uses tokio async runtime")]
```

## Memory Format in System Prompt

Memories appear in the system prompt of both brains:

```
═══════════════════════════════════════════════════════
MEMORY:
1. User prefers Rust for system programming
2. Project uses tokio async runtime
3. User's timezone is UTC+8
═══════════════════════════════════════════════════════
```

When no memories are recorded, the section is hidden (empty string).

## Persistence

Memories are automatically persisted to disk when DataStore is configured:

### Storage Location

Memories are saved as `.agent/{BotName}/memory.yaml`:

```yaml
type_tag: MemoryData
value:
  memories:
    - "User prefers Rust for system programming"
    - "Project uses tokio async runtime"
```

### Usage

```rust
// Create Bot with DataStore
let bot = Bot::new(runtime_with_datastore, "MyBot", model, tools, sink)?;

// Load previous memories
bot.load_state().await?;

// Modify memories (cached immediately)
bot.memory_state().add("New memory".to_string());

// Flush to disk
bot.flush_state().await?;
```

See [PERSISTENCE.md](PERSISTENCE.md) for detailed information.

## Implementation Details

### Integration with Bot

The memory tool is integrated into Bot similar to GoalTool:

1. **MemoryState** is created in `Bot::new()`
2. **MemoryTool** is added to both Conversation Brain and Work Brain sessions
3. **MemorySegment** is added to both sessions' system prompt segments
4. Memories are shared across all brains through cloned `MemoryState`
5. **Persistence** is configured automatically if Runtime has DataStore

### Code Structure

```
crates/agent-bot/src/
├── memory_tool.rs          # MemoryState, MemorySegment, MemoryTool
├── bot.rs                  # Integration with Bot
└── lib.rs                  # Exports

crates/agent-bot/tests/
└── test_memory_tool.rs     # Unit and integration tests
```

## Best Practices

1. **Keep memories concise**: Aim for 1-2 sentences per memory
2. **Record important facts**: User preferences, project details, recurring patterns
3. **Use for context**: Things that should be remembered across conversations
4. **Periodic cleanup**: Use `clear-memories` when memories become stale

## Differences from Goal

| Feature | Goal | Memory |
|---------|------|--------|
| Purpose | Current task objective | Long-term information |
| Cardinality | Single goal at a time | Multiple memories |
| Lifecycle | Task-scoped | Persistent across tasks |
| Update pattern | Replace/clear | Append/clear all |

## Testing

Run memory tool tests:

```bash
cargo test --package agent-bot --test test_memory_tool
```

Tests cover:
- Adding and listing memories
- Clearing memories
- MemorySegment rendering with and without memories
