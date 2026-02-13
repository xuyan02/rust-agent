use agent_core::{AgentRunner, RunnerConsole};
use agent_llm::{ChatContent, ChatMessage, ChatRole};
use anyhow::Result;

struct CaptureConsole {
    lines: Vec<String>,
}

impl RunnerConsole for CaptureConsole {
    fn print_line(&mut self, s: &str) {
        self.lines.push(s.to_string());
    }
}

struct NoopAgent;

#[async_trait::async_trait(?Send)]
impl agent_core::Agent for NoopAgent {
    async fn run(&mut self, _ctx: &agent_core::AgentContext<'_>) -> Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn runner_prints_last_assistant_text_even_if_tool_entries_follow() -> Result<()> {
    let runtime = agent_core::RuntimeBuilder::new().build();
    let session = agent_core::SessionBuilder::new(&runtime)
        .set_default_model("fake".to_string())
        .build()?;
    let ctx = agent_core::AgentContextBuilder::new(&session).build()?;

    let mut runner = AgentRunner::new(NoopAgent);

    // Simulate that agent run appended tool call/result after assistant text.
    let _ = ctx
        .history()
        .append(ChatMessage::assistant_text("a1"))
        .await;
    let _ = ctx
        .history()
        .append(ChatMessage {
            role: ChatRole::Assistant,
            content: ChatContent::ToolCalls(serde_json::json!([])),
        })
        .await;
    let _ = ctx
        .history()
        .append(ChatMessage::tool_result(
            "call_1".to_string(),
            serde_json::json!({"ok":true}),
        ))
        .await;

    let mut console = CaptureConsole { lines: vec![] };

    runner
        .run_line(&ctx, &mut console, "hello".to_string())
        .await?;

    assert_eq!(console.lines, vec!["a1".to_string()]);

    Ok(())
}
