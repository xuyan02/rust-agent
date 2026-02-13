use anyhow::Result;

#[test]
fn session_defaults() -> Result<()> {
    let runtime = agent_core::RuntimeBuilder::new().build();
    let s = agent_core::SessionBuilder::new(&runtime).build()?;
    assert!(!s.workspace_path().as_os_str().is_empty());
    assert!(s.agent_path().ends_with(".agent"));
    assert!(s.default_model().is_empty());
    Ok(())
}
