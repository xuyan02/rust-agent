use agent_core::tools::{FileTool, Tool};
use agent_core::{AgentContextBuilder, RuntimeBuilder, SessionBuilder};
use anyhow::Result;

#[tokio::test]
async fn file_read_errors_on_directory_path() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let ws = tmp.path().to_path_buf();
    let agent_dir = ws.join(".agent");
    tokio::fs::create_dir_all(&agent_dir).await?;

    let runtime = std::rc::Rc::new(RuntimeBuilder::new().build());
    let session = SessionBuilder::new(std::rc::Rc::clone(&runtime))
        .set_workspace_path(ws.clone())
        .set_agent_path(agent_dir)
        .set_default_model("dummy".to_string())
        .add_tool(Box::new(FileTool::new()))
        .build()?;

    let ctx = AgentContextBuilder::from_session(&session).build()?;

    let args = serde_json::json!({"path": ".", "offset_lines": 0, "limit_lines": 10});
    let err = FileTool::new()
        .invoke(&ctx, "file-read", &args)
        .await
        .unwrap_err();

    let msg = format!("{err:#}");
    assert!(msg.contains("directory"), "unexpected error: {msg}");

    Ok(())
}
