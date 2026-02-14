use agent_core::{AgentContextBuilder, SessionBuilder};
use anyhow::Result;

#[test]
fn agent_context_build_seeds_tools_from_session_and_allows_overrides() -> Result<()> {
    let runtime = agent_core::RuntimeBuilder::new().build();

    let session = SessionBuilder::new(&runtime)
        .set_default_model("gpt-test".to_string())
        .add_tool(Box::new(agent_core::tools::DebugTool::new()))
        .build()?;

    let ctx = AgentContextBuilder::from_session(&session).build()?;
    assert!(
        ctx.session().tools().iter().any(|t| t
            .spec()
            .functions
            .iter()
            .any(|f| f.name == "debug.echo"))
    );

    Ok(())
}
