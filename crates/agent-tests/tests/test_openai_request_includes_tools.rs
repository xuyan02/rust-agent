use agent_llm::{ChatMessage, build_chat_completions_body};
use anyhow::Result;

#[test]
fn openai_request_includes_tools() -> Result<()> {
    let tools = vec![serde_json::json!({
        "type": "function",
        "function": {
            "name": "file.read",
            "description": "read file",
            "parameters": {
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"],
                "additionalProperties": false
            }
        }
    })];

    let body = build_chat_completions_body(
        "gpt-test",
        &[
            ChatMessage::system_text("sys"),
            ChatMessage::user_text("hi"),
        ],
        &tools,
    )?;

    assert_eq!(body["model"].as_str(), Some("gpt-test"));
    assert_eq!(body["stream"].as_bool(), Some(false));
    assert!(body.get("tools").is_some());
    assert_eq!(body["tools"].as_array().map(|a| a.len()), Some(1));
    assert_eq!(body["tools"][0]["type"].as_str(), Some("function"));
    assert_eq!(
        body["tools"][0]["function"]["name"].as_str(),
        Some("file.read")
    );

    Ok(())
}
