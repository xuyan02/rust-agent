use std::process::Command;

#[test]
fn brain_cli_help_mentions_agent_yaml_default() {
    // Cargo only sets CARGO_BIN_EXE_* for integration tests within the same package.
    // Here we execute via `cargo run -p brain-cli -- --help`.
    let out = Command::new("cargo")
        .args(["run", "-p", "brain-cli", "--", "--help"])
        .output()
        .expect("run brain-cli help");

    assert!(out.status.success());

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Temporary CLI"));
    assert!(stdout.contains("--cfg ./.agent/agent.yaml"));
}
