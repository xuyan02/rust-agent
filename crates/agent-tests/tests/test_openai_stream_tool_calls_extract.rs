use agent_llm::{OpenAiStreamAccumulator, OpenAiStreamDelta};
use anyhow::Result;

#[test]
fn extracts_tool_calls_from_stream() -> Result<()> {
    let mut acc = OpenAiStreamAccumulator::new();

    {
        let line = serde_json::json!({
            "choices": [
                {
                    "delta": {
                        "tool_calls": [
                            {
                                "index": 0,
                                "id": "call_1",
                                "type": "function",
                                "function": {"name": "foo", "arguments": "{\"x\":"}
                            }
                        ]
                    },
                    "finish_reason": null
                }
            ]
        })
        .to_string();
        let mut delta = OpenAiStreamDelta::default();
        assert!(acc.feed_data_line(&line, &mut delta)?);
        assert_eq!(delta.tool_calls_delta.len(), 1);
        assert_eq!(delta.tool_calls_delta[0].id.as_deref(), Some("call_1"));
        assert_eq!(delta.tool_calls_delta[0].name.as_deref(), Some("foo"));
        assert_eq!(
            delta.tool_calls_delta[0].arguments_json.as_deref(),
            Some(r#"{"x":"#)
        );
    }

    {
        let line = serde_json::json!({
            "choices": [
                {
                    "delta": {"tool_calls": [{"index": 0, "function": {"arguments": "1}"}}]},
                    "finish_reason": null
                }
            ]
        })
        .to_string();
        let mut delta = OpenAiStreamDelta::default();
        assert!(acc.feed_data_line(&line, &mut delta)?);
        assert_eq!(delta.tool_calls_delta.len(), 1);
        assert_eq!(
            delta.tool_calls_delta[0].arguments_json.as_deref(),
            Some("1}")
        );
    }

    assert!(acc.has_tool_calls());
    let tool_calls = acc.build_assistant_tool_calls_value();
    assert_eq!(tool_calls.as_array().map(|a| a.len()), Some(1));
    assert_eq!(tool_calls[0]["id"].as_str(), Some("call_1"));
    assert_eq!(tool_calls[0]["function"]["name"].as_str(), Some("foo"));
    assert_eq!(
        tool_calls[0]["function"]["arguments"].as_str(),
        Some(r#"{"x":1}"#)
    );

    Ok(())
}
