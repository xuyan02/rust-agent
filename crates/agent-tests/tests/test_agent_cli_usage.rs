use anyhow::{Context, Result};

fn run(args: &[&str]) -> Result<std::process::Output> {
    // In integration tests, Cargo does not set CARGO_BIN_EXE_* for binaries outside this package.
    // Use workspace target/debug/agent-cli.
    let exe = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("debug")
        .join("agent-cli");

    let out = std::process::Command::new(exe)
        .args(args)
        .output()
        .context("failed to run agent-cli")?;
    Ok(out)
}

#[test]
fn help_exits_0_and_prints_usage() -> Result<()> {
    let out = run(&["--help"])?;
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("agent_cli [--input <text>]"));
    Ok(())
}

#[test]
fn unknown_arg_exits_2() -> Result<()> {
    let out = run(&["--nope"])?;
    assert_eq!(out.status.code(), Some(2));
    let s = String::from_utf8_lossy(&out.stderr);
    assert!(s.contains("error: unknown arg"));
    Ok(())
}

#[test]
fn input_missing_value_exits_2() -> Result<()> {
    let out = run(&["--input"])?;
    assert_eq!(out.status.code(), Some(2));
    Ok(())
}
