use agent_core::tools::{ShellTool, Tool};
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
        let runtime = std::rc::Rc::new(agent_core::RuntimeBuilder::new().build());
        let session = agent_core::SessionBuilder::new(std::rc::Rc::clone(&runtime))
            .set_workspace_path(ws.to_path_buf())
            .build()
            .unwrap();
        let ctx = agent_core::AgentContextBuilder::from_session(&session)
            .build()
            .unwrap();

        let err = tool.invoke(&ctx, "shell-exec", &args).await.unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("unsafe shell command"));
    }

    Ok(())
}
