use agent_tools::{ShellTool, Tool};
use anyhow::Result;

#[tokio::test]
async fn shell_tool_rejects_substitution_and_redirection() -> Result<()> {
    let tool = ShellTool::new();
    let ws = std::path::Path::new(".");

    for cmd in [
        "echo $(whoami)",
        "echo `whoami`",
        "echo hi > out.txt",
        "cat < /etc/passwd",
        "echo hi | wc -c",
        "echo hi && echo ok",
        "echo hi; echo ok",
        "echo hi &",
    ] {
        let args = serde_json::json!({"command": cmd});
        let err = tool.invoke(ws, "shell.exec", &args).await.unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("unsafe shell command"));
    }

    Ok(())
}
