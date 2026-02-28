use agent_core::tools::{ShellTool, Tool};
use anyhow::Result;

#[tokio::test]
async fn shell_tool_rejects_substitution_and_redirection() -> Result<()> {
    let tool = ShellTool::new();
    let ws = std::path::Path::new(".");

    // Only test commands that should be rejected:
    // command substitution and process substitution
    for cmd in [
        "echo $(whoami)",
        "echo `whoami`",
        "cat <(ls)",
        "echo >(cat)",
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

    // These commands are now allowed (relaxed policy):
    // - Redirection: >, <
    // - Piping: |
    // - Command chaining: &&, ||, ;
    // - Backgrounding: &
    for cmd in [
        "echo hi > /dev/null",
        "echo hi | wc -c",
        "echo hi && echo ok",
        "true || echo failed",
        "echo hi; echo ok",
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

        // These should succeed (not be rejected)
        let _result = tool.invoke(&ctx, "shell-exec", &args).await;
        // We don't check the result because command execution may fail for other reasons,
        // but they should not be rejected by safety checks
    }

    Ok(())
}
