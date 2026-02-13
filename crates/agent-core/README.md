# agent-core

Core orchestration types for the agent runtime.

## Tool ownership and precedence

`Session` and `AgentContext` each own their own tool lists:

- `Session` tools are the default tool set (typically built from config and/or `SessionBuilder`).
- `AgentContext` tools are per-run overrides/additions (typically built via `AgentContextBuilder::add_tool`).

During tool dispatch, tools are resolved by function name with the following precedence:

1. Search `AgentContext` tools first (later-added tools win).
2. If not found, search `Session` tools (later-added tools win).

This lets a context override a session-default tool implementation for the same function.

## LLM sender API (messages by reference)

`agent-llm::LlmSender` is called with a borrowed message slice:

```rust
async fn send(&mut self, messages: &[ChatMessage]) -> Result<ChatMessage>;
```

Rationale: callers can build `Vec<ChatMessage>` once per turn and pass `&messages` without cloning.
