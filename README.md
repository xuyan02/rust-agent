# agent (Rust workspace)

## agent-bot (programming bot CLI)

Config: create `<repo>/.agent/agent.yaml`:

```yaml
model: "gpt-4.1-mini"
openai:
  base_url: "https://..."
  api_key: "..."
  model_provider_id: "..." # optional
```

Run:

```bash
cargo run -p agent-bot
```

Commands:
- `task <goal>`: plan -> apply -> verify (cargo fmt/clippy/test) -> review
- `plan <goal>` / `apply` / `verify` / `diff` / `reset` / `help` / `exit`

Single-shot:

```bash
cargo run -p agent-bot -- --task "fix clippy warnings"
```

## Verification

From this directory:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```
