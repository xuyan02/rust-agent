use crate::llm::{ChatContent, ChatMessage, ChatRole};
use anyhow::{Context, Result, bail};
use serde_json::Value;

pub fn build_chat_completions_body(
    model: &str,
    messages: &[ChatMessage],
    tools: &[Value],
) -> Result<Value> {
    let mut body = serde_json::Map::new();
    body.insert("model".to_string(), Value::String(model.to_string()));
    body.insert("stream".to_string(), Value::Bool(false));

    if !tools.is_empty() {
        body.insert("tools".to_string(), Value::Array(tools.to_vec()));
        body.insert("tool_choice".to_string(), Value::String("auto".to_string()));
    }

    let mut out_msgs = Vec::with_capacity(messages.len());
    for m in messages {
        let mut msg = serde_json::Map::new();
        msg.insert(
            "role".to_string(),
            Value::String(
                match m.role {
                    ChatRole::System => "system",
                    ChatRole::User => "user",
                    ChatRole::Assistant => "assistant",
                    ChatRole::Tool => "tool",
                }
                .to_string(),
            ),
        );

        match &m.content {
            ChatContent::Text(t) => {
                msg.insert("content".to_string(), Value::String(t.clone()));
            }
            ChatContent::ToolCalls(tc) => {
                msg.insert("tool_calls".to_string(), tc.clone());
            }
            ChatContent::ToolResult {
                tool_call_id,
                result,
            } => {
                msg.insert(
                    "tool_call_id".to_string(),
                    Value::String(tool_call_id.clone()),
                );
                msg.insert("content".to_string(), Value::String(result.clone()));
            }
        }

        out_msgs.push(Value::Object(msg));
    }

    body.insert("messages".to_string(), Value::Array(out_msgs));
    Ok(Value::Object(body))
}

pub fn parse_chat_completions_response(root: &Value) -> Result<ChatMessage> {
    let choices = root
        .get("choices")
        .and_then(|v| v.as_array())
        .context("openai: missing choices")?;
    let choice0 = choices.first().context("openai: empty choices")?;
    let msg = choice0
        .get("message")
        .and_then(|v| v.as_object())
        .context("openai: missing choices[0].message")?;

    if let Some(tool_calls) = msg.get("tool_calls")
        && tool_calls.is_array()
    {
        let arr = tool_calls.as_array().unwrap();
        if !arr.is_empty() {
            return Ok(ChatMessage::assistant_tool_calls(tool_calls.clone()));
        }
    }

    if let Some(content) = msg.get("content").and_then(|v| v.as_str()) {
        return Ok(ChatMessage::assistant_text(content.to_string()));
    }

    bail!("openai: message has neither tool_calls nor string content")
}
