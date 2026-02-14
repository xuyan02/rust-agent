# agent (Rust workspace)

---

## Contributing

### Pre-deployment Checklist:
Before any deployment or pull request:

Run these **format and lint** commands in the root directory:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

Verify with **unit tests**:

```bash
cargo test
```

### Code Coverage (Optional):
To analyze test coverage dynamically:
```bash
cargo tarpaulin --out Html
```
This will produce an HTML report that can be easily reviewed.

### Guidelines:

- Ensure the behavior of logic remains consistent.
- Add both **unit** and **integration tests** for any new features or bug fixes.
- Document new additions appropriately for maintainability.

Happy coding!