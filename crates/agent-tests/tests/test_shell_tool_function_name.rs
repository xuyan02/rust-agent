use agent_core::tools::{ShellTool, Tool};
use agent_core::{AgentContextBuilder, RuntimeBuilder, SessionBuilder};
use anyhow::Result;

#[tokio::test]
async fn shell_tool_exposes_shell_exec_function() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let ws = tmp.path().to_path_buf();
    let agent_dir = ws.join(".agent");
    tokio::fs::create_dir_all(&agent_dir).await?;

    let runtime = RuntimeBuilder::new().build();
    let session = SessionBuilder::new(&runtime)
        .set_workspace_path(ws.clone())
        .set_agent_path(agent_dir)
        .set_default_model("dummy".to_string())
        .add_tool(Box::new(ShellTool::new()))
        .build()?;

    let ctx = AgentContextBuilder::from_session(&session).build()?;

    let args = serde_json::json!({"command":"echo"});
    let err = ShellTool::new()
        .invoke(&ctx, "shell", &args)
        .await
        .unwrap_err();
    assert!(format!("{err:#}").contains("unknown function"));

    Ok(())
}
