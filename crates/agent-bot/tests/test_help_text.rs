#[test]
fn help_contains_core_commands() {
    let h = agent_bot::bot::help_text();
    for cmd in [
        "task", "plan", "apply", "verify", "diff", "reset", "help", "exit",
    ] {
        assert!(h.contains(cmd), "missing {cmd}");
    }
}
