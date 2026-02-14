use agent_core::tools::{DebugTool, Tool};
use anyhow::Result;

#[tokio::test]
async fn debug_tool_echo() -> Result<()> {
    let tool = DebugTool::new();
    let args = serde_json::json!({"text": "hello"});

    let runtime = agent_core::RuntimeBuilder::new().build();
    let session = agent_core::SessionBuilder::new(&runtime).build()?;
    let ctx = agent_core::AgentContextBuilder::from_session(&session).build()?;

    let out = tool.invoke(&ctx, "debug.echo", &args).await?;
    assert_eq!(out, "hello");
    Ok(())
}
