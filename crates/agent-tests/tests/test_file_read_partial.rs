use agent_core::tools::{FileTool, Tool};
use agent_core::{AgentContextBuilder, RuntimeBuilder, SessionBuilder};
use anyhow::Result;

#[tokio::test]
async fn file_read_supports_offset_and_limit_lines() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let ws = tmp.path().to_path_buf();
    let agent_dir = ws.join(".agent");
    tokio::fs::create_dir_all(&agent_dir).await?;

    let file = ws.join("a.txt");
    tokio::fs::write(&file, "l0\nl1\nl2\nl3\nl4\n").await?;

    let runtime = std::rc::Rc::new(RuntimeBuilder::new().build());
    let session = SessionBuilder::new(std::rc::Rc::clone(&runtime))
        .set_workspace_path(ws.clone())
        .set_agent_path(agent_dir)
        .set_default_model("dummy".to_string())
        .add_tool(Box::new(FileTool::new()))
        .build()?;

    let ctx = AgentContextBuilder::from_session(&session).build()?;

    let args = serde_json::json!({"path":"a.txt","offset_lines": 2, "limit_lines": 2});
    let out = FileTool::new().invoke(&ctx, "file-read", &args).await?;
    assert_eq!(out, "l2\nl3");

    Ok(())
}
