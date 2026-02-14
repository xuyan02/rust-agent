use agent_core::tools::{FileTool, Tool};
use agent_core::{AgentContextBuilder, RuntimeBuilder, SessionBuilder};
use anyhow::Result;

#[tokio::test]
async fn file_read_missing_file_returns_error_string() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let ws = tmp.path().to_path_buf();
    let agent_dir = ws.join(".agent");
    tokio::fs::create_dir_all(&agent_dir).await?;

    let runtime = RuntimeBuilder::new().build();
    let session = SessionBuilder::new(&runtime)
        .set_workspace_path(ws.clone())
        .set_agent_path(agent_dir)
        .set_default_model("dummy".to_string())
        .add_tool(Box::new(FileTool::new()))
        .build()?;

    let ctx = AgentContextBuilder::from_session(&session).build()?;

    let args = serde_json::json!({"path":"does_not_exist.txt","offset_lines":0,"limit_lines":10});
    let err = FileTool::new()
        .invoke(&ctx, "file-read", &args)
        .await
        .unwrap_err();

    let msg = format!("{err:#}");
    assert!(msg.contains("failed to stat"));

    Ok(())
}
