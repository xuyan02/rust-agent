use agent_core::tools::{FileTool, Tool};
use agent_core::{AgentContextBuilder, RuntimeBuilder, SessionBuilder};
use anyhow::Result;

#[tokio::test]
async fn file_edit_replaces_first_occurrence() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let ws = tmp.path().to_path_buf();
    let agent_dir = ws.join(".agent");
    tokio::fs::create_dir_all(&agent_dir).await?;

    let file = ws.join("test.txt");
    tokio::fs::write(&file, "Hello world\nHello Rust\nHello world\n").await?;

    let runtime = std::rc::Rc::new(RuntimeBuilder::new().build());
    let session = SessionBuilder::new(std::rc::Rc::clone(&runtime))
        .set_workspace_path(ws.clone())
        .set_agent_path(agent_dir)
        .set_default_model("dummy".to_string())
        .add_tool(Box::new(FileTool::new()))
        .build()?;

    let ctx = AgentContextBuilder::from_session(&session).build()?;

    // Replace only first occurrence
    let args = serde_json::json!({
        "path": "test.txt",
        "old_text": "Hello",
        "new_text": "Hi",
        "replace_all": false
    });
    let result = FileTool::new().invoke(&ctx, "file-edit", &args).await?;
    assert!(result.contains("Replaced 1 occurrence(s)"));

    let content = tokio::fs::read_to_string(&file).await?;
    assert_eq!(content, "Hi world\nHello Rust\nHello world\n");

    Ok(())
}

#[tokio::test]
async fn file_edit_replaces_all_occurrences() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let ws = tmp.path().to_path_buf();
    let agent_dir = ws.join(".agent");
    tokio::fs::create_dir_all(&agent_dir).await?;

    let file = ws.join("test.txt");
    tokio::fs::write(&file, "Hello world\nHello Rust\nHello world\n").await?;

    let runtime = std::rc::Rc::new(RuntimeBuilder::new().build());
    let session = SessionBuilder::new(std::rc::Rc::clone(&runtime))
        .set_workspace_path(ws.clone())
        .set_agent_path(agent_dir)
        .set_default_model("dummy".to_string())
        .add_tool(Box::new(FileTool::new()))
        .build()?;

    let ctx = AgentContextBuilder::from_session(&session).build()?;

    // Replace all occurrences
    let args = serde_json::json!({
        "path": "test.txt",
        "old_text": "Hello",
        "new_text": "Hi",
        "replace_all": true
    });
    let result = FileTool::new().invoke(&ctx, "file-edit", &args).await?;
    assert!(result.contains("Replaced 3 occurrence(s)"));

    let content = tokio::fs::read_to_string(&file).await?;
    assert_eq!(content, "Hi world\nHi Rust\nHi world\n");

    Ok(())
}

#[tokio::test]
async fn file_edit_fails_on_missing_file() -> Result<()> {
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

    let args = serde_json::json!({
        "path": "nonexistent.txt",
        "old_text": "foo",
        "new_text": "bar"
    });
    let result = FileTool::new().invoke(&ctx, "file-edit", &args).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("does not exist"));

    Ok(())
}

#[tokio::test]
async fn file_edit_fails_on_old_text_not_found() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let ws = tmp.path().to_path_buf();
    let agent_dir = ws.join(".agent");
    tokio::fs::create_dir_all(&agent_dir).await?;

    let file = ws.join("test.txt");
    tokio::fs::write(&file, "Hello world\n").await?;

    let runtime = std::rc::Rc::new(RuntimeBuilder::new().build());
    let session = SessionBuilder::new(std::rc::Rc::clone(&runtime))
        .set_workspace_path(ws.clone())
        .set_agent_path(agent_dir)
        .set_default_model("dummy".to_string())
        .add_tool(Box::new(FileTool::new()))
        .build()?;

    let ctx = AgentContextBuilder::from_session(&session).build()?;

    let args = serde_json::json!({
        "path": "test.txt",
        "old_text": "NotFound",
        "new_text": "bar"
    });
    let result = FileTool::new().invoke(&ctx, "file-edit", &args).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));

    Ok(())
}

#[tokio::test]
async fn file_edit_handles_multiline_text() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let ws = tmp.path().to_path_buf();
    let agent_dir = ws.join(".agent");
    tokio::fs::create_dir_all(&agent_dir).await?;

    let file = ws.join("test.txt");
    tokio::fs::write(&file, "fn main() {\n    println!(\"Hello\");\n}\n").await?;

    let runtime = std::rc::Rc::new(RuntimeBuilder::new().build());
    let session = SessionBuilder::new(std::rc::Rc::clone(&runtime))
        .set_workspace_path(ws.clone())
        .set_agent_path(agent_dir)
        .set_default_model("dummy".to_string())
        .add_tool(Box::new(FileTool::new()))
        .build()?;

    let ctx = AgentContextBuilder::from_session(&session).build()?;

    // Replace multiline text
    let args = serde_json::json!({
        "path": "test.txt",
        "old_text": "fn main() {\n    println!(\"Hello\");",
        "new_text": "fn main() {\n    println!(\"Hi\");",
        "replace_all": false
    });
    let result = FileTool::new().invoke(&ctx, "file-edit", &args).await?;
    assert!(result.contains("Replaced 1 occurrence(s)"));

    let content = tokio::fs::read_to_string(&file).await?;
    assert_eq!(content, "fn main() {\n    println!(\"Hi\");\n}\n");

    Ok(())
}
