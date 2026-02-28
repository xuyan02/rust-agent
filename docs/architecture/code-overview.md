# Code Architecture Overview

This document provides a detailed overview of the project's Rust codebase, generated from code review. It covers every significant source file across all crates.

---

## Crate Structure

The project consists of 7 crates:

| Crate | Purpose |
|-------|---------|
| `agent-core` | Core framework: Agent, Runtime, Session, History, Tools, LLM |
| `agent-bot` | Bot/Brain layer: multi-brain architecture, Goal/Memory/Knowledge |
| `agent-macros` | Proc-macros for `#[tool]`, `#[tool_fn]`, `#[tool_arg]` |
| `agent-cli` | CLI entrypoint for single-bot interaction |
| `brain-cli` | CLI entrypoint for brain-level interaction |
| `team-cli` | CLI entrypoint for multi-bot team collaboration |
| `agent-tests` | Integration tests |

---

## agent-core — Core Framework

### agent.rs (~100 lines)
- **`Agent` trait**: Core abstraction with `async fn run(&self, ctx: &AgentContext) -> Result<()>`.
- **`LlmAgent`**: Simplest Agent implementation — gets history messages, calls `runtime.execute()`.
- **`MAX_TOOL_ITERATIONS = 50`**: Safety limit to prevent infinite tool-call loops.
- **`maybe_spool_tool_output()`**: When tool output exceeds 8KB, truncates to first 80 lines preview and saves full output to `.agent/spool/{timestamp}_{function_name}.log`.

### agent_context.rs (~185 lines)
- **`AgentContext`**: Tree-structured context with `parent: AgentContextParent` (Session or another AgentContext).
- Fields: `parent`, `history`, `system_prompt_segments`, `tools`, `disable_tools`, `dir_node`.
- **Hierarchical resolution**: `session()` walks up to root Session. `history()` uses local or inherits from parent. `system_prompt_segments()` and `tools()` aggregate along the parent chain (local first).
- **`disable_tools`**: Flag to suppress tools (used by ReAct Think phase).
- **`AgentContextBuilder`**: Builder pattern for constructing contexts. Key methods: `from_session()`, `from_parent_ctx()`, `add_system_prompt_segment()`, `set_history()`, `add_tool()`, `disable_tools()`.

### session.rs (~140 lines)
- **`Session`**: Top-level container holding `Rc<Runtime>`, `workspace_path`, `agent_path`, `default_model`, `tools`, `system_prompt_segments`, `history`, `dir_node`.
- Provides read-only getter methods.
- **`SessionBuilder`**: Builder pattern. Defaults: `workspace_path` = cwd, `agent_path` = workspace/.agent, `history` = InMemoryHistory.

### runtime.rs (~220 lines)
- **`Runtime`**: Holds OpenAI config, LLM providers, local spawner, data store.
- **`execute()`**: Core execution loop:
  1. Collects and renders all `system_prompt_segments` into a single system message.
  2. Loops: call LLM → if Text response, return; if ToolCalls, execute each tool and feed results back.
  3. Tool errors are formatted as `"tool error\nfunction: {name}\nmessage: {root_cause}"` and returned to LLM.
- **`create_sender()`**: Checks registered `llm_providers` first, falls back to OpenAI.
- **`RuntimeBuilder`**: Builder with `set_openai()`, `add_llm_provider()`, `set_local_spawner()`, `set_data_store_root()`.

### react_agent.rs (~400+ lines including tests)
- **`ReActAgent`**: Implements the ReAct (Reasoning + Acting) framework.
- **ThinkDecision enum**: `ContinueThinking`, `ReadyToAct`, `FinalAnswer`.
- **Main loop** (`run_impl`): Think → decide → (continue think / act / answer) → observation → repeat.
- **Think phase**: Creates child AgentContext with tools disabled, uses `LlmAgent` to get pure reasoning.
- **Act phase**: Creates child AgentContext with tools enabled, uses `LlmAgent` for tool execution.
- **Error handling**: On any error, clears history to prevent getting stuck in bad state.
- **`parse_think_decision()`**: Parses `[think]`/`[act]`/`[answer]` prefix markers. Validates exactly one marker, no conflicting markers on other line starts. Empty output triggers history clear.
- Includes 6 unit tests covering all marker parsing scenarios.

### system_prompt.rs (~28 lines)
- **`SystemPromptSegment` trait**: `async fn render(&self, ctx) -> Result<String>`.
- **`StaticSystemPromptSegment`**: Simple wrapper for a fixed string.

### tool_dispatch.rs (~65 lines)
- **`ParsedToolCall`**: Struct with `id`, `function_name`, `arguments`.
- **`parse_tool_calls()`**: Parses OpenAI-format JSON tool_calls array.
- **`find_tool_for_function()`**: Finds the first Tool whose spec contains the function name (earlier tools win = local-first precedence).

### data_store.rs (~370 lines)
- **`DataNode`**: Represents a single `.yaml` file with type-erased in-memory cache (`RefCell<Option<Box<dyn CachedValue>>>`).
  - `load<T>()`: Loads from disk or creates default (idempotent).
  - `get<T>()`: Read-only `Ref`.
  - `get_mut<T>()`: Mutable `RefMut`, marks dirty.
  - `set<T>()`: Replaces value, marks dirty.
  - `get_or_default<T>()`: Gets or creates default.
  - `flush()`: Writes to disk if dirty, creates parent dirs.
  - `remove()`: Deletes file and clears cache.
- **`DirNode`**: Directory in the data store tree. `node(key)` returns cached DataNode, `subdir(name)` returns sub-DirNode.
- **`DataStore`**: Root of the tree. `root_dir()` returns root DirNode. `children(dir)` lists `.yaml` files, `subdirs(dir)` lists subdirectories.
- CachedValue trait uses `Any` for dynamic type dispatch, serializes via `serde_yaml`.

### history/ module

#### mod.rs (~130 lines)
- **`History` trait**: `get_all()`, `append()`, `last()`, `clear()`, `get_recent()`.
- **`InMemoryHistory`**: `RefCell<Vec<ChatMessage>>` with `max_size` (default 1000). Sliding window on overflow, cleans leading Tool messages.
- Blanket impl for `&T: History`.

#### persistent.rs (~185 lines)
- **`PersistentHistory`**: Persists to `history.yaml` via DirNode.
- Hardcoded compression config: threshold 8K tokens, target 4K, keep recent 2K.
- `maybe_compress()`: When threshold exceeded, extracts old messages → archives → LLM-generated summary → inserts at beginning.
- `append()`: Load → add message → compress → sliding window → save.

#### compression.rs (~230+ lines)
- **`CompressionConfig`**: `compress_threshold_tokens`, `compress_target_tokens`, `keep_recent_tokens`, `enabled`.
- **`CompressionStrategy`**:
  - `should_compress()`: Checks total tokens vs threshold.
  - `find_split_point()`: Calculates compression range, ensures tool call pairs aren't split.
  - `clean_leading_tool_messages()`: Removes leading Tool messages for API compliance.
  - `create_summary_message()`: Creates an isolated LlmAgent session to generate compression summary.

#### archiver.rs (~140 lines)
- **`ArchivedHistory`**: Serializable struct with `compressed_at`, `message_count`, `estimated_tokens`, `messages`.
- **`HistoryArchiver`**: Manages archive directory. `generate_filename()` uses Unix timestamp. `save()` / `load()` / `list_archives()`.

#### token_estimator.rs (~55 lines)
- Heuristic token estimation: ASCII ~4 chars/token, CJK ~1.5 chars/token.
- `estimate_tokens()`, `estimate_message_tokens()`, `estimate_messages_tokens()`.

### tools/mod.rs (~300+ lines)
- **`Tool` trait**: `fn spec() -> &ToolSpec` + `async fn invoke(ctx, function_name, args) -> Result<String>`.
- **Type system**: `ToolSpec`, `FunctionSpec`, `TypeSpec` (Array/Object/String/Boolean/Integer/Number), `PropertySpec`, `ObjectSpec`, etc.
- `to_json_schema_value()`: Converts spec types to JSON Schema for OpenAI API.
- **`FileTool`**: Built-in file operations tool.
- Re-exports: `DeepThinkTool`, `MacroExampleTool`, proc-macro attributes `tool`/`tool_fn`/`tool_arg`.

### tools/deep_think.rs (~120 lines)
- **`DeepThinkTool`**: Delegates complex reasoning to a `ReActAgent`.
- Tool function `deep-think` takes a `task` string parameter.
- `invoke()`: Creates an isolated `AgentContext` (from Session, with `InMemoryHistory` to avoid inheriting Bot's protocol prompts). Runs `ReActAgent` on the task, extracts the last assistant message as the answer (strips `[answer]` prefix if present).
- `truncate_str()`: UTF-8 safe string truncation helper.

### tools/shell.rs (~20 lines)
- **`validate_shell_command()`**: Relaxed security denylist for shell commands.
- **Denied**: command substitution and process substitution patterns.
- **Allowed**: `|`, `;`, `&&`, `||`, `>`, `<` (piping, chaining, redirection).

### llm/ module (~615 lines total, 7 files)

The LLM abstraction layer provides a provider-agnostic interface for communicating with language models.

#### mod.rs (~272 lines)
- **Core types**:
  - `ChatRole` enum: `System`, `User`, `Assistant`, `Tool`.
  - `ChatContent` enum: `Text(String)`, `ToolCalls(Value)`, `ToolResult { tool_call_id, result }`.
  - `ChatMessage`: Struct with `role` + `content`, plus convenience constructors (`system_text`, `user_text`, `assistant_text`, `assistant_tool_calls`, `tool_result`).
- **`LlmSender` trait**: `async fn send(&mut self, messages, tools) -> Result<ChatMessage>`.
- **`OpenAiSender`**: Concrete implementation. Fields: `base_url`, `api_key`, `model`, `model_provider_id`, `http` (reqwest client).
  - Sends POST to `{base_url}/v1/chat/completions`.
  - Retry logic: up to 5 retries with exponential backoff (1s, 2s, 4s, 8s, 16s) for HTTP 429 or response containing "rate limit" / "circuit breaker".
  - Debug output via `AGENT_DEBUG_LLM` environment variable.
  - Adds `X-Model-Provider-Id` header when configured.
- **`OpenAiProviderConfig`**: Config struct (`base_url`, `api_key`, `model_provider_id`).
- **`create_openai_sender()`**: Factory function.

#### context.rs (~77 lines)
- **`LlmProvider` trait**: Provider registration interface with `name()`, `supports_model(model)`, `create_sender(model)`, `create_request(model, messages, tools)`.
- **`LlmRequest` trait**: `async fn run() -> Result<ChatMessage>`. `SenderBackedRequest` adapts `LlmSender` to `LlmRequest`.
- **`LlmContext`**: Provider registry. `register()` adds providers, `create()` matches model name to first supporting provider.

#### openai.rs (~93 lines)
- **`build_chat_completions_body()`**: Builds OpenAI-format JSON request body. Sets `stream=false`, `tool_choice="auto"` when tools are present. Maps `ChatContent` variants to appropriate JSON fields.
- **`parse_chat_completions_response()`**: Parses response JSON. Supports both standard OpenAI format (`choices[0].message`) and compatible API format (`choices[0]`). Prioritizes `tool_calls` over text content.

#### openai_provider.rs (~29 lines)
- **`OpenAiProvider`**: Implements `LlmProvider`. `name()` = "openai", `supports_model()` = always true, `create_sender()` delegates to `create_openai_sender()`.

#### openai_stream.rs (~115 lines)
- **SSE streaming support** for tool calls.
- `ToolCallDelta`: Incremental update (index, id, name, arguments_json).
- `OpenAiStreamDelta`: Collection of deltas per SSE line.
- **`OpenAiStreamAccumulator`**: Accumulates streaming tool call fragments via `feed_data_line()`. `has_tool_calls()` checks presence, `build_assistant_tool_calls_value()` produces final JSON Value.

#### tools_json.rs (~20 lines)
- **`tools_to_openai_json()`**: Converts Tool trait objects to OpenAI function calling JSON format. Uses `TypeSpec::Object::to_json_schema_value()` for parameter schemas.

#### json.rs (~9 lines)
- Thin wrappers: `parse(str -> Value)`, `dump(Value -> String)` with anyhow context.

### lib.rs (~60 lines)
- Module declarations and re-exports.
- **`prompt!` macro**: `include_str!` + `StaticSystemPromptSegment::new()` for compile-time prompt loading.

---

## agent-bot — Bot/Brain Layer

### bot.rs (previously reviewed)
- **`Bot`**: Wraps a Brain, manages message routing via `Envelope` (from/to/content).
- **`BotEvent`**: `OutputMessage` / `Error`.
- **`BotEventSink`** trait for event delivery.
- Bot creates Brain with conversation/work/introspection brain configuration.

### brain.rs (previously reviewed)
- **`Brain`**: Self-driven component with three sub-brains (Conversation/Work/Introspection).
- **`BrainConfig`**: Configuration for LLM model, tools, prompts.
- Event-driven architecture via `BrainEventSink`.
- Single-threaded, runs via local spawner.

### bot_prompt.rs (~64 lines)
- **`BotPromptSegment`**: Implements `SystemPromptSegment`.
- Holds `GoalState` + `MemoryState`.
- `render()`: Loads states from disk, concatenates `bot.md` operating principles + "# Current Goal" + "# Memory" (numbered list).

### goal_tool.rs (~195 lines)
- **`GoalData`**: Serializable `Option<String>`.
- **`GoalState`**: Wraps `Rc<DataNode>`, provides `load/flush/set/get/clear`.
- **`GoalSegment`**: `SystemPromptSegment` that renders current goal with decorative `═══` border.
- **`GoalTool`**: Tool with 3 functions: `set-goal`, `get-goal`, `clear-goal`. Flushes to disk immediately on mutation.

### memory_tool.rs (previously reviewed)
- **`MemoryState`**: Persistent memory storage via DataNode.
- **`MemorySegment`**: Renders memories into system prompt.
- **`MemoryTool`**: Tool functions: `remember`, `list-memories`, `get-memory-size`, `replace-memories`.

### knowledge_base.rs (previously reviewed)
- **`KnowledgeBase`**: File-system based knowledge storage under `.agent/knowledge/`.
- Operations: `list_files`, `list_dirs`, `read_file`, `write_file`, `move_file`, `delete_file`.

### knowledge_tools.rs (~190 lines)
- **`KnowledgeTool`**: Tool wrapper for `KnowledgeBase` with 5 functions: `list-knowledge`, `read-knowledge`, `write-knowledge`, `move-knowledge`, `delete-knowledge`.

### history_tool.rs (~200+ lines)
- **`HistoryTool`**: Read-only access to brain histories (conv + work).
- 6 functions: `read-conv-history`, `read-work-history`, `read-conv-archive`, `read-work-archive`, `list-conv-archives`, `list-work-archives`.
- Parses `history.yaml` and `ArchivedHistory` format, formats messages with 200-char preview.
- `truncate_str()`: Safe UTF-8 truncation.

### team.rs (~420 lines)
- **`Team`**: Manages multi-bot collaboration.
- **`TeamEvent`**: `UserMessage` / `BotCreated` / `Error`.
- **`BotConfig`**: `default_model`, `tool_constructors` (Rc<RefCell<Vec>>), `system_prompt_segments`.
- **`TeamConfig`**: `default_bot_config` + optional `leader_config`.
- Core methods: `send_user_message()`, `create_bot()`, `create_bot_with_config()`, `send_bot_message()`, `shutdown()`.
- **`TeamBotSink`**: Routes messages — only leader can send to user, inter-bot messages routed via `bot.push()`.
- Implements `Drop` for automatic shutdown.

### brain_driver.rs (empty file)
- Historically contained BrainDriver, now removed. Empty module kept in source tree.

### lib.rs
- 9 internal modules. Publicly exports all major types: Bot, Brain, Goal/Memory/Knowledge tools and states, Team types.

---


## agent-macros — Proc-Macro Crate (~761 lines)

### lib.rs (~34 lines)
- Three proc-macro attribute macros:
  - **`#[tool]`**: Marks an `impl` block as a Tool implementation. Generates `Tool` trait impl automatically.
  - **`#[tool_fn]`**: Marks async methods inside `#[tool] impl` as tool functions. Marker attribute parsed by `#[tool]`.
  - **`#[tool_arg]`**: Parameter-level marker for customizing argument metadata (consumed during expansion).

### tool.rs (~727 lines)
- Complex proc-macro code generation engine.
- **`parse_tool_args()`**: Parses `#[tool(id="...", description="...")]`.
- **`parse_tool_fn_args()`**: Parses `#[tool_fn(name="...", description="...", hidden, strict, args(...))]`.
  - Supports `hidden` flag (excludes from spec), `strict` flag (default true), nested `args()` list for per-parameter metadata.
- **`ToolFnArgMeta`**: Per-argument customization: `rename`, `desc`, `default`.
- **`schema_for_type()`**: Maps Rust types to `TypeSpec` for JSON Schema generation:
  - `String` → `TypeSpec::String`, `bool` → `Boolean`, integers → `Integer`, floats → `Number`
  - `Vec<T>` → `TypeSpec::Array`, `Option<T>` → unwraps inner type (not required)
- **`decode_expr_for_type()`**: Generates code to decode `serde_json::Value` into Rust types at runtime.
- **`is_agent_context_param()`**: Detects `&AgentContext` parameters for automatic context injection (not exposed as tool argument).
- **`tool_impl()`**: Main code generation:
  1. Generates `spec()` → `ToolSpec` with all `FunctionSpec` entries from `#[tool_fn]` methods.
  2. Generates `invoke()` → match on `function_name`, decode arguments, call method, return `Result<String>`.
  3. Uses doc comments as function descriptions when `#[tool_fn(description=...)]` is not specified.
- **`tools/macro_example.rs`**: Example usage — `MacroExampleTool` with `echo(text)` and `pwd(ctx)` demonstrating the macro system.
## Prompt Files (compile-time loaded)

| File | Purpose |
|------|---------|
| `crates/agent-bot/prompts/bot.md` | Operating principles for all brains |
| `crates/agent-bot/prompts/conversation_brain.md` | Conversation Brain behavior |
| `crates/agent-bot/prompts/work_brain.md` | Work Brain behavior |
| `crates/agent-bot/prompts/introspection_brain.md` | Introspection Brain behavior |
| `crates/agent-core/prompts/react_think.md` | ReAct Think phase instructions |
| `crates/agent-core/prompts/react_act.md` | ReAct Act phase instructions |
| `prompts/intuitive.md` | Intuitive mode prompt |
| `prompts/shallow_think.md` | Shallow think mode prompt |

---

## Key Design Patterns

### 1. Hierarchical Context (AgentContext tree)
```
Session (root)
  └── AgentContext (Brain level)
       ├── AgentContext (Think phase — tools disabled)
       └── AgentContext (Act phase — tools enabled)
```
Tools and system prompt segments aggregate along the parent chain (local-first precedence).

### 2. ReAct Loop
```
Think → [think] → continue thinking
      → [act]   → Act (tool execution) → Observation → Think
      → [answer] → done
```

### 3. Persistent State via DataStore
```
DataStore (root: .agent/)
  └── DirNode (directory)
       ├── DataNode → .yaml file (typed, cached, dirty-tracking)
       └── DirNode (subdirectory) ...
```
Goal, Memory states use DataNode for transparent persistence with load/flush lifecycle.

### 4. History Management
- InMemoryHistory: Simple sliding window.
- PersistentHistory: YAML file + compression (archive old messages + LLM summary) + sliding window.

### 5. Tool System
- Tool trait with ToolSpec (JSON Schema compatible).
- Tool dispatch: parse OpenAI tool_calls → find matching tool → invoke → return result.
- Output spooling for large results (>8KB → .agent/spool/).


### 6. LLM Abstraction
```
LlmContext (provider registry)
  --> LlmProvider (e.g., OpenAiProvider)
       --> LlmSender (e.g., OpenAiSender)
            --> HTTP POST to /v1/chat/completions
```
- Provider-agnostic via trait abstraction (LlmProvider -> LlmSender -> LlmRequest).
- Automatic retry with exponential backoff for rate limits.
- Supports OpenAI-compatible APIs with custom base_url and model_provider_id.
- SSE streaming accumulator for incremental tool call responses.

### 7. Proc-Macro Tool Generation
```
#[tool(id = "example", description = "...")]
impl MyTool {
    #[tool_fn(name = "my-func")]
    async fn my_func(&self, text: String, ctx: &AgentContext<'_>) -> Result<String> { ... }
}
// Generates: impl Tool for MyTool { fn spec() ...; async fn invoke() ... }
```
- Automatic ToolSpec generation from Rust method signatures.
- Type mapping: Rust types -> JSON Schema TypeSpec (String, bool, integers, floats, Vec<T>, Option<T>).
- AgentContext injection: detected by type and excluded from tool parameters.
- Doc comments used as function descriptions when not explicitly specified.
