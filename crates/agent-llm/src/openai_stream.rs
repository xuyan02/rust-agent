use anyhow::{Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallDelta {
    pub index: usize,
    pub id: Option<String>,
    pub name: Option<String>,
    pub arguments_json: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenAiStreamDelta {
    pub tool_calls_delta: Vec<ToolCallDelta>,
}

#[derive(Default)]
pub struct OpenAiStreamAccumulator {
    tool_calls: Vec<AccumToolCall>,
}

#[derive(Debug, Clone, Default)]
struct AccumToolCall {
    id: String,
    name: String,
    arguments_json: String,
}

impl OpenAiStreamAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed_data_line(
        &mut self,
        line: &str,
        delta_out: &mut OpenAiStreamDelta,
    ) -> Result<bool> {
        delta_out.tool_calls_delta.clear();

        let root = agent_json::parse(line).with_context(|| "failed to parse stream line json")?;
        let choices = root
            .get("choices")
            .and_then(|v| v.as_array())
            .context("missing choices")?;
        let choice0 = choices.first().context("empty choices")?;
        let delta = choice0
            .get("delta")
            .and_then(|v| v.as_object())
            .context("missing choices[0].delta")?;

        let tool_calls = match delta.get("tool_calls") {
            None => return Ok(true),
            Some(v) => v.as_array().context("delta.tool_calls not array")?,
        };

        for tc in tool_calls {
            let index = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            while self.tool_calls.len() <= index {
                self.tool_calls.push(AccumToolCall::default());
            }

            let mut out = ToolCallDelta {
                index,
                id: None,
                name: None,
                arguments_json: None,
            };

            if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                self.tool_calls[index].id = id.to_string();
                out.id = Some(id.to_string());
            }

            if let Some(fn_obj) = tc.get("function").and_then(|v| v.as_object()) {
                if let Some(name) = fn_obj.get("name").and_then(|v| v.as_str()) {
                    self.tool_calls[index].name = name.to_string();
                    out.name = Some(name.to_string());
                }
                if let Some(args) = fn_obj.get("arguments").and_then(|v| v.as_str()) {
                    self.tool_calls[index].arguments_json.push_str(args);
                    out.arguments_json = Some(args.to_string());
                }
            }

            delta_out.tool_calls_delta.push(out);
        }

        Ok(true)
    }

    pub fn has_tool_calls(&self) -> bool {
        self.tool_calls
            .iter()
            .any(|t| !t.id.is_empty() || !t.name.is_empty() || !t.arguments_json.is_empty())
    }

    pub fn build_assistant_tool_calls_value(&self) -> serde_json::Value {
        let mut arr = Vec::new();
        for t in &self.tool_calls {
            if t.id.is_empty() && t.name.is_empty() && t.arguments_json.is_empty() {
                continue;
            }
            arr.push(serde_json::json!({
                "id": t.id,
                "type": "function",
                "function": {
                    "name": t.name,
                    "arguments": t.arguments_json,
                }
            }));
        }
        serde_json::Value::Array(arr)
    }
}
