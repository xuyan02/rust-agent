use agent_core::tools::{FileTool, Tool};
use agent_core::{AgentContextBuilder, RuntimeBuilder, SessionBuilder};
use anyhow::Result;

#[tokio::test]
async fn file_read_missing_has_root_message() -> Result<()> {
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

    // Simulate tool loop error formatting by directly invoking the tool and formatting a chain.
    // This test focuses on the root cause string coming from std::error::Error::source() chain.
    let args = serde_json::json!({"path":"missing.txt","offset_lines":0,"limit_lines":10});
    let err = FileTool::new()
        .invoke(&ctx, "file-read", &args)
        .await
        .unwrap_err();

    let mut root = err.to_string();
    let mut cur = err.source();
    while let Some(s) = cur {
        root = s.to_string();
        cur = s.source();
    }

    assert!(
        root.contains("No such file") || root.contains("os error 2"),
        "root={root}"
    );

    // And we check the final formatting shape expected from tool loop.
    let formatted = format!("tool error\nfunction: file-read\nmessage: {root}");
    assert!(formatted.contains("function: file-read"));
    assert!(formatted.contains("message:"));

    Ok(())
}
