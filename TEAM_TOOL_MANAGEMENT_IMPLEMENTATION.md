# Team Tool Management Implementation

## Overview

Implemented a flexible tool management system for Team, allowing configuration of both common tools (available to all bots) and bot-specific tools.

## Problem Solved

**Before**: No way to configure tools for team members - tools were hardcoded in `Bot::new()`

**After**: Two levels of tool configuration:
1. **Common tools** - Shared by all team members
2. **Bot-specific tools** - Specialized tools for individual bots

## Solution: Tool Constructor Pattern

Used function closures to construct tools on-demand, solving the ownership problem (Tool trait objects cannot be cloned).

```rust
pub type ToolConstructor = Box<dyn Fn() -> Box<dyn agent_core::tools::Tool>>;
```

## API Design

### 1. TeamConfig

```rust
pub struct TeamConfig {
    common_tool_constructors: Vec<ToolConstructor>,
}

impl TeamConfig {
    pub fn new() -> Self

    pub fn add_common_tool<F>(mut self, constructor: F) -> Self
    where
        F: Fn() -> Box<dyn agent_core::tools::Tool> + 'static

    pub fn common_tool_constructors(&self) -> &[ToolConstructor]
}
```

**Builder pattern** for ergonomic configuration:
```rust
let config = TeamConfig::new()
    .add_common_tool(|| Box::new(DebugTool::new()))
    .add_common_tool(|| Box::new(FileTool::new()));
```

### 2. Team Constructor with Config

```rust
impl Team {
    // Existing - backward compatible
    pub fn new(...) -> Result<Self>

    // New - with tool configuration
    pub fn new_with_config(
        runtime: Rc<Runtime>,
        user_name: impl Into<String>,
        leader_name: impl Into<String>,
        leader_agent: Box<dyn Agent>,
        sink: impl TeamEventSink + 'static,
        config: TeamConfig,
    ) -> Result<Self>
}
```

### 3. Bot Creation with Specific Tools

```rust
impl Team {
    // Existing - creates bot with common tools
    pub fn create_bot(
        &self,
        name: impl Into<String>,
        agent: Box<dyn Agent>,
    ) -> Result<()>

    // New - creates bot with common + specific tools
    pub fn create_bot_with_tools(
        &self,
        name: impl Into<String>,
        agent: Box<dyn Agent>,
        tool_constructors: Vec<ToolConstructor>,
    ) -> Result<()>
}
```

## Implementation Details

### Storage in Team

```rust
struct Inner {
    runtime: Rc<Runtime>,
    user_name: String,
    leader_name: String,
    bots: HashMap<String, Bot>,
    sink: Box<dyn TeamEventSink>,
    config: TeamConfig,  // NEW: Store configuration
}
```

### Helper Functions

```rust
// Build session with tools from constructors
fn build_session_with_tools(
    runtime: Rc<Runtime>,
    tool_constructors: &[ToolConstructor],
) -> Result<Session>

// Build session with combined tools (common + specific)
fn build_session_with_combined_tools(
    runtime: Rc<Runtime>,
    common_constructors: &[ToolConstructor],
    specific_constructors: &[ToolConstructor],
) -> Result<Session>
```

These helper functions:
1. Create SessionBuilder with default model
2. Add DebugTool by default
3. Invoke each tool constructor and add to session
4. Build and return the configured Session

### Tool Constructor Execution

When creating a bot:
```rust
// 1. Get common tool constructors from config
let common_constructors = &inner.config.common_tool_constructors;

// 2. For bot-specific tools, merge with common
let session = build_session_with_combined_tools(
    runtime,
    common_constructors,
    specific_constructors,
)?;

// 3. Create bot with pre-configured session
let bot = Bot::new_with_session(session, name, agent, sink)?;
```

## Usage Examples

### Example 1: Team with Common Tools

```rust
use agent_bot::{Team, TeamConfig};
use agent_core::tools::{DebugTool, FileTool};

let config = TeamConfig::new()
    .add_common_tool(|| Box::new(DebugTool::new()))
    .add_common_tool(|| Box::new(FileTool::new()));

let team = Team::new_with_config(
    runtime,
    "Alice",
    "LeaderBot",
    leader_agent,
    sink,
    config,
)?;

// All subsequently created bots inherit these tools
team.create_bot("Worker1", agent)?;  // Has DebugTool + FileTool
team.create_bot("Worker2", agent)?;  // Has DebugTool + FileTool
```

### Example 2: Bot with Specialized Tools

```rust
use agent_core::tools::ShellTool;

// Create a specialized bot with additional tools
team.create_bot_with_tools(
    "DevOpsBot",
    devops_agent,
    vec![
        Box::new(|| Box::new(ShellTool::new())),
    ],
)?;

// This bot has: common tools + ShellTool
```

### Example 3: Different Specializations

```rust
// Data analyst bot
team.create_bot_with_tools(
    "DataAnalyst",
    analyst_agent,
    vec![
        Box::new(|| Box::new(DataTool::new())),
        Box::new(|| Box::new(PlotTool::new())),
    ],
)?;

// File manager bot
team.create_bot_with_tools(
    "FileManager",
    file_agent,
    vec![
        Box::new(|| Box::new(FileTool::new())),
    ],
)?;

// Each bot gets common tools + their specific tools
```

## Design Benefits

### 1. Solves Ownership Problem
Tool trait objects can't be cloned, but closures can create new instances on demand.

### 2. Flexibility
- Configure once, apply to all (common tools)
- Specialize per-bot when needed (specific tools)
- Mix both approaches

### 3. Backward Compatibility
```rust
// Old API still works
Team::new(runtime, user, leader, agent, sink)?

// New API available when needed
Team::new_with_config(runtime, user, leader, agent, sink, config)?
```

### 4. Type Safety
Compiler ensures tool constructors return correct types.

### 5. Ergonomic Builder Pattern
```rust
TeamConfig::new()
    .add_common_tool(|| Box::new(Tool1::new()))
    .add_common_tool(|| Box::new(Tool2::new()))
```

## Test Coverage

Created 7 comprehensive tests in `test_team_tools.rs`:

1. **team_with_no_common_tools** - Default (empty) configuration
2. **team_with_common_debug_tool** - Single common tool
3. **team_with_multiple_common_tools** - Multiple common tools
4. **team_create_bot_with_specific_tools** - Bot-specific tools
5. **team_config_common_tool_constructors** - Config API
6. **team_backward_compatibility_with_new** - Old API still works
7. **team_all_bots_inherit_common_tools** - Multiple bots inherit tools

All tests pass ✅

## Files Modified

| File | Changes | Lines |
|------|---------|-------|
| `crates/agent-bot/src/team.rs` | Added TeamConfig, new methods, helpers | +120 |
| `crates/agent-bot/src/lib.rs` | Export TeamConfig, ToolConstructor | +1 |
| `crates/agent-bot/tests/test_team_tools.rs` | New test file | +244 |

**Total**: ~365 lines of new code and tests

## API Summary

### Public Types
- `TeamConfig` - Tool configuration for team
- `ToolConstructor` - Type alias for tool constructor functions

### Public Methods

**TeamConfig**:
- `new()` - Create empty configuration
- `add_common_tool(constructor)` - Add common tool
- `common_tool_constructors()` - Get constructors

**Team**:
- `new_with_config(...)` - Create team with configuration
- `create_bot_with_tools(...)` - Create bot with specific tools

### Backward Compatibility
- `Team::new()` - Still works (uses empty TeamConfig)
- `Team::create_bot()` - Still works (uses common tools only)

## Future Enhancements

Potential improvements:

1. **Remove default DebugTool** - Let users explicitly add it via config
2. **Tool groups** - Named sets of tools (e.g., "file_ops", "dev_ops")
3. **Tool validation** - Check for conflicts or missing dependencies
4. **Dynamic tool loading** - Add/remove tools at runtime
5. **Tool usage metrics** - Track which tools are used by which bots

## Related Documentation

- [TEAM_TOOL_DESIGN.md](./TEAM_TOOL_DESIGN.md) - Design exploration and alternatives
- [TEAM_IMPLEMENTATION.md](./TEAM_IMPLEMENTATION.md) - Team system documentation
- [team.rs API docs](./crates/agent-bot/src/team.rs) - Inline documentation

## Conclusion

Successfully implemented a flexible, type-safe tool management system for Team that:
- ✅ Allows common tools for all bots
- ✅ Supports bot-specific tool specialization
- ✅ Maintains backward compatibility
- ✅ Uses idiomatic Rust patterns (builder, closures)
- ✅ Fully tested with 7 new tests
- ✅ Well-documented with examples

The solution elegantly solves the ownership problem using tool constructor closures while providing a clean, ergonomic API.
