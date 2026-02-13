use agent_tools::{DebugTool, Tool};
use anyhow::Result;

#[tokio::test]
async fn debug_tool_echo() -> Result<()> {
    let tool = DebugTool::new();
    let args = serde_json::json!({"text": "hello"});

    let out = tool
        .invoke(std::path::Path::new("."), "debug.echo", &args)
        .await?;
    assert_eq!(out.as_str(), Some("hello"));
    Ok(())
}
