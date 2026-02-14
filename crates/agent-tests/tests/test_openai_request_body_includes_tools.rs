use agent_core::llm::{ChatMessage, build_chat_completions_body, tools_to_openai_json};
use agent_core::tools::{FileTool, Tool};
use anyhow::Result;

#[test]
fn openai_body_includes_tools_json() -> Result<()> {
    let tools: Vec<Box<dyn Tool>> = vec![Box::new(FileTool::new())];
    let tool_refs: Vec<&dyn Tool> = tools.iter().map(|t| t.as_ref()).collect();

    let tools_json = tools_to_openai_json(&tool_refs);
    assert!(!tools_json.is_empty());

    let messages = vec![ChatMessage::user_text("hi".to_string())];
    let body = build_chat_completions_body("gpt-test", &messages, &tools_json)?;

    assert!(body.get("tools").is_some());
    assert_eq!(
        body.get("tool_choice").and_then(|v| v.as_str()),
        Some("auto")
    );
    Ok(())
}
