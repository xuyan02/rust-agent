use anyhow::Result;
use agent_core::{AgentContextBuilder, SessionBuilder, RuntimeBuilder};
use std::rc::Rc;

#[test]
fn test_tool_whitelist_filters_tools() -> Result<()> {
    let runtime = Rc::new(RuntimeBuilder::new().build());
    let session = SessionBuilder::new(Rc::clone(&runtime))
        .add_tool(Box::new(agent_core::tools::FileTool::new()))
        .add_tool(Box::new(agent_core::tools::DebugTool::new()))
        .add_tool(Box::new(agent_core::tools::ShellTool::new()))
        .build()?;

    // Create context with whitelist allowing only "file" tool
    let ctx = AgentContextBuilder::from_session(&session)
        .set_tool_whitelist(vec!["file".to_string()])
        .build()?;

    let tools = ctx.tools();

    // Should only have file tool
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].spec().id, "file");

    Ok(())
}

#[test]
fn test_tool_whitelist_empty_list() -> Result<()> {
    let runtime = Rc::new(RuntimeBuilder::new().build());
    let session = SessionBuilder::new(Rc::clone(&runtime))
        .add_tool(Box::new(agent_core::tools::FileTool::new()))
        .add_tool(Box::new(agent_core::tools::DebugTool::new()))
        .build()?;

    // Create context with empty whitelist
    let ctx = AgentContextBuilder::from_session(&session)
        .set_tool_whitelist(vec![])
        .build()?;

    let tools = ctx.tools();

    // Should have no tools
    assert_eq!(tools.len(), 0);

    Ok(())
}

#[test]
fn test_tool_whitelist_multiple_tools() -> Result<()> {
    let runtime = Rc::new(RuntimeBuilder::new().build());
    let session = SessionBuilder::new(Rc::clone(&runtime))
        .add_tool(Box::new(agent_core::tools::FileTool::new()))
        .add_tool(Box::new(agent_core::tools::DebugTool::new()))
        .add_tool(Box::new(agent_core::tools::ShellTool::new()))
        .build()?;

    // Create context with whitelist allowing "file" and "debug" tools
    let ctx = AgentContextBuilder::from_session(&session)
        .set_tool_whitelist(vec!["file".to_string(), "debug".to_string()])
        .build()?;

    let tools = ctx.tools();

    // Should have file and debug tools
    assert_eq!(tools.len(), 2);
    let tool_ids: Vec<String> = tools.iter().map(|t| t.spec().id.clone()).collect();
    assert!(tool_ids.contains(&"file".to_string()));
    assert!(tool_ids.contains(&"debug".to_string()));
    assert!(!tool_ids.contains(&"shell".to_string()));

    Ok(())
}

#[test]
fn test_no_whitelist_shows_all_tools() -> Result<()> {
    let runtime = Rc::new(RuntimeBuilder::new().build());
    let session = SessionBuilder::new(Rc::clone(&runtime))
        .add_tool(Box::new(agent_core::tools::FileTool::new()))
        .add_tool(Box::new(agent_core::tools::DebugTool::new()))
        .build()?;

    // Create context without whitelist
    let ctx = AgentContextBuilder::from_session(&session).build()?;

    let tools = ctx.tools();

    // Should have all tools
    assert_eq!(tools.len(), 2);

    Ok(())
}
