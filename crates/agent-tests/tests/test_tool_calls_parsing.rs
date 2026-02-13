use anyhow::Result;

fn parse_calls(v: serde_json::Value) -> Result<Vec<(String, String, serde_json::Value)>> {
    // Mirror the shape expected by agent-core.
    let arr = v.as_array().unwrap();
    let mut out = Vec::new();
    for tc in arr {
        let id = tc.get("id").and_then(|v| v.as_str()).unwrap().to_string();
        let fn_obj = tc.get("function").and_then(|v| v.as_object()).unwrap();
        let name = fn_obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();
        let args = fn_obj.get("arguments").and_then(|v| v.as_str()).unwrap();
        out.push((id, name, agent_json::parse(args)?));
    }
    Ok(out)
}

#[test]
fn tool_calls_parsing_smoke() -> Result<()> {
    let tool_calls = serde_json::json!([
      {
        "id": "call_1",
        "type": "function",
        "function": {
          "name": "file.read",
          "arguments": "{\"path\":\"README.md\"}"
        }
      }
    ]);

    let calls = parse_calls(tool_calls)?;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "call_1");
    assert_eq!(calls[0].1, "file.read");
    assert_eq!(calls[0].2["path"].as_str(), Some("README.md"));
    Ok(())
}
