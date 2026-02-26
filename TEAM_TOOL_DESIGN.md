# Team Tool Management Design

## Problem Statement

How to manage tools for team members? Two key scenarios:
1. **Common tools** - Tools available to all team members
2. **Specialized tools** - Tools available to specific team members

## Current Architecture Limitations

```rust
// Current: Team::new() calls Bot::new()
Bot::new(runtime, name, agent, sink)
  └─> Creates Session internally
      └─> Hardcoded tools (DebugTool)
```

**Issues:**
- No way to configure tools when creating team
- No way to add tools to specific bots
- Tools are hardcoded in Bot::new()

## Design Constraints

1. **Tool ownership**: `Tool` trait objects cannot be cloned
2. **Single-threaded**: Using `Rc<RefCell<>>` pattern
3. **Dynamic creation**: Bots can be created at runtime by the leader
4. **Flexibility**: Need both common and bot-specific tool configuration

## Proposed Solutions

### Solution 1: Tool Constructor Functions ⭐ (Recommended)

Use functions/closures to construct tools on-demand:

```rust
pub type ToolConstructor = Box<dyn Fn() -> Box<dyn Tool>>;

pub struct TeamConfig {
    /// Tool constructors available to all bots
    common_tool_constructors: Vec<ToolConstructor>,
}

impl Team {
    pub fn new_with_config(
        runtime: Rc<Runtime>,
        user_name: impl Into<String>,
        leader_name: impl Into<String>,
        leader_agent: Box<dyn Agent>,
        sink: impl TeamEventSink + 'static,
        config: TeamConfig,
    ) -> Result<Self>

    pub fn create_bot_with_tools(
        &self,
        name: impl Into<String>,
        agent: Box<dyn Agent>,
        tool_constructors: Vec<ToolConstructor>,
    ) -> Result<()>
}
```

**Advantages:**
- ✅ Solves ownership problem (creates new instances)
- ✅ Flexible - can configure per-bot
- ✅ Simple API
- ✅ Works with dynamic bot creation

**Disadvantages:**
- Need to box closures
- Slight overhead from function calls

### Solution 2: Pass Session Directly

Allow passing pre-configured Session:

```rust
impl Team {
    pub fn new_with_session(
        runtime: Rc<Runtime>,
        user_name: impl Into<String>,
        leader_name: impl Into<String>,
        leader_session: Session,
        leader_agent: Box<dyn Agent>,
        sink: impl TeamEventSink + 'static,
    ) -> Result<Self>
}
```

**Advantages:**
- ✅ Maximum flexibility
- ✅ Reuses existing Session API
- ✅ No new abstractions

**Disadvantages:**
- ❌ User needs Runtime to create Session
- ❌ Less convenient for common case
- ❌ Doesn't solve dynamic bot creation

### Solution 3: Hybrid Approach ⭐⭐ (Most Practical)

Combine both solutions:

```rust
// For fine-grained control (leader)
pub fn new_with_session(...)

// For common tools (all bots)
pub struct TeamConfig {
    common_tool_constructors: Vec<ToolConstructor>,
}

impl Team {
    pub fn new_with_config(...)

    pub fn create_bot_with_tools(...)
}
```

**Advantages:**
- ✅ Flexibility for advanced users (Session)
- ✅ Convenience for common cases (ToolConstructors)
- ✅ Solves both scenarios

## Recommended Implementation

```rust
// 1. Add TeamConfig
pub struct TeamConfig {
    common_tool_constructors: Vec<Box<dyn Fn() -> Box<dyn Tool>>>,
}

impl TeamConfig {
    pub fn new() -> Self {
        Self {
            common_tool_constructors: vec![],
        }
    }

    pub fn add_common_tool<F>(mut self, constructor: F) -> Self
    where
        F: Fn() -> Box<dyn Tool> + 'static,
    {
        self.common_tool_constructors.push(Box::new(constructor));
        self
    }
}

// 2. Extend Team API
impl Team {
    // Existing method - backward compatible
    pub fn new(...) -> Result<Self>

    // New method - with configuration
    pub fn new_with_config(
        runtime: Rc<Runtime>,
        user_name: impl Into<String>,
        leader_name: impl Into<String>,
        leader_agent: Box<dyn Agent>,
        sink: impl TeamEventSink + 'static,
        config: TeamConfig,
    ) -> Result<Self>

    // New method - create bot with specific tools
    pub fn create_bot_with_tools<F>(
        &self,
        name: impl Into<String>,
        agent: Box<dyn Agent>,
        tool_constructors: Vec<F>,
    ) -> Result<()>
    where
        F: Fn() -> Box<dyn Tool> + 'static
}

// 3. Store config in Inner
struct Inner {
    runtime: Rc<Runtime>,
    user_name: String,
    leader_name: String,
    bots: HashMap<String, Bot>,
    sink: Box<dyn TeamEventSink>,
    config: TeamConfig,  // NEW
}
```

## Usage Examples

### Example 1: Common Tools for All Bots

```rust
use agent_bot::{Team, TeamConfig};
use agent_core::tools::{DebugTool, ShellTool};

let config = TeamConfig::new()
    .add_common_tool(|| Box::new(DebugTool::new()))
    .add_common_tool(|| Box::new(ShellTool::new()));

let team = Team::new_with_config(
    runtime,
    "Alice",
    "LeaderBot",
    leader_agent,
    sink,
    config,
)?;

// All subsequently created bots will have DebugTool and ShellTool
team.create_bot("Worker1", worker_agent)?;  // Has common tools
team.create_bot("Worker2", worker_agent)?;  // Has common tools
```

### Example 2: Bot-Specific Tools

```rust
// Create bot with specialized tools
team.create_bot_with_tools(
    "DataAnalyzer",
    analyzer_agent,
    vec![
        Box::new(|| Box::new(DebugTool::new())),
        Box::new(|| Box::new(DataTool::new())),
        Box::new(|| Box::new(PlotTool::new())),
    ],
)?;

// This bot has: common tools + DataTool + PlotTool
```

### Example 3: Leader with Custom Session

```rust
// For advanced control over leader
let leader_session = SessionBuilder::new(runtime.clone())
    .set_default_model("gpt-4".to_string())
    .add_tool(Box::new(DebugTool::new()))
    .add_tool(Box::new(CreateBotTool::new()))  // Special tool
    .add_system_prompt_segment(Box::new(
        StaticSystemPromptSegment::new(LEADER_PROMPT.to_string())
    ))
    .build()?;

let team = Team::new_with_session(
    runtime,
    "Alice",
    "LeaderBot",
    leader_session,
    leader_agent,
    sink,
)?;
```

## Implementation Plan

### Phase 1: Basic Tool Configuration (P0)
1. Add `TeamConfig` struct
2. Add `new_with_config()` method
3. Store config in `Inner`
4. Use config when creating bots
5. Add tests

### Phase 2: Advanced Tool Management (P1)
1. Add `create_bot_with_tools()` method
2. Merge common + specific tools
3. Add tests

### Phase 3: Session-Level Control (P2)
1. Add `new_with_session()` for leader
2. Add `create_bot_with_session()` for workers
3. Add tests

## Alternative Consideration: Tool Trait Enhancement

Could make Tool cloneable via a helper trait:

```rust
pub trait CloneableTool: Tool {
    fn clone_box(&self) -> Box<dyn Tool>;
}

// But this requires modifying all Tool implementations
// and adds complexity - NOT recommended
```

## Decision

**Implement Hybrid Approach (Solution 3):**
- Phase 1 first (TeamConfig with constructors)
- Phase 2 if needed (bot-specific tools)
- Phase 3 for advanced users (custom sessions)

This provides:
- ✅ Backward compatibility (keep existing `new()`)
- ✅ Common tool configuration
- ✅ Bot-specific tool configuration
- ✅ Maximum flexibility when needed
- ✅ Clean, Rust-idiomatic API
