use agent_core::tools::{ShellTool, Tool};
use anyhow::Result;

#[tokio::test]
async fn shell_tool_pwd_in_workspace() -> Result<()> {
    let workspace = std::env::temp_dir().join("agent_shell_tool");
    let _ = tokio::fs::remove_dir_all(&workspace).await;
    tokio::fs::create_dir_all(&workspace).await?;

    let tool = ShellTool::new();

    let args = serde_json::json!({"command": "pwd"});
    let runtime = std::rc::Rc::new(agent_core::RuntimeBuilder::new().build());
    let session = agent_core::SessionBuilder::new(std::rc::Rc::clone(&runtime))
        .set_workspace_path(workspace.clone())
        .build()?;
    let ctx = agent_core::AgentContextBuilder::from_session(&session).build()?;
    let out = tool.invoke(&ctx, "shell-exec", &args).await?;
    assert!(out.contains(&*workspace.to_string_lossy()));

    let _ = tokio::fs::remove_dir_all(&workspace).await;
    Ok(())
}
