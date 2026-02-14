use agent_core::tools::{FileTool, Tool};
use agent_core::{AgentContextBuilder, RuntimeBuilder, SessionBuilder};
use anyhow::Result;

#[tokio::test]
async fn file_glob_finds_files_under_workspace() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let ws = tmp.path().to_path_buf();
    let agent_dir = ws.join(".agent");
    tokio::fs::create_dir_all(&agent_dir).await?;

    tokio::fs::create_dir_all(ws.join("a/b")).await?;
    tokio::fs::write(ws.join("a/b/c.txt"), "x").await?;

    let runtime = RuntimeBuilder::new().build();
    let session = SessionBuilder::new(&runtime)
        .set_workspace_path(ws.clone())
        .set_agent_path(agent_dir)
        .set_default_model("dummy".to_string())
        .add_tool(Box::new(FileTool::new()))
        .build()?;

    let ctx = AgentContextBuilder::from_session(&session).build()?;

    let args = serde_json::json!({"pattern":"a/**/*.txt","limit":10});
    let out = FileTool::new().invoke(&ctx, "file-glob", &args).await?;

    assert!(out.lines().any(|l| l == "a/b/c.txt"), "out={out:?}");

    Ok(())
}

#[tokio::test]
async fn file_glob_rejects_absolute_patterns() -> Result<()> {
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

    let args = serde_json::json!({"pattern":"/etc/*","limit":10});
    let err = FileTool::new()
        .invoke(&ctx, "file-glob", &args)
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("relative"));

    Ok(())
}
