use crate::Result;
use crate::tools::Tool;
use anyhow::{Context, bail};

#[derive(Debug, Clone)]
pub(crate) struct ParsedToolCall {
    pub id: String,
    pub function_name: String,
    pub arguments: serde_json::Value,
}

pub(crate) fn parse_tool_calls(tool_calls: &serde_json::Value) -> Result<Vec<ParsedToolCall>> {
    let arr = tool_calls.as_array().context("tool_calls is not array")?;

    let mut out = Vec::with_capacity(arr.len());
    for tc in arr {
        let obj = tc.as_object().context("tool_call is not object")?;
        let id = obj
            .get("id")
            .and_then(|v| v.as_str())
            .context("tool_call missing id")?;
        let ty = obj
            .get("type")
            .and_then(|v| v.as_str())
            .context("tool_call missing type")?;
        if ty != "function" {
            bail!("tool_call type is not function");
        }

        let fn_obj = obj
            .get("function")
            .and_then(|v| v.as_object())
            .context("tool_call missing function")?;
        let name = fn_obj
            .get("name")
            .and_then(|v| v.as_str())
            .context("tool_call missing function.name")?;
        let args_str = fn_obj
            .get("arguments")
            .and_then(|v| v.as_str())
            .context("tool_call missing function.arguments")?;
        let arguments = serde_json::from_str(args_str)
            .context("failed to parse json")
            .context("failed to parse function.arguments")?;

        out.push(ParsedToolCall {
            id: id.to_string(),
            function_name: name.to_string(),
            arguments,
        });
    }

    Ok(out)
}

pub fn find_tool_for_function<'a>(
    tools: &'a [&'a dyn Tool],
    function_name: &str,
) -> Option<&'a dyn Tool> {
    // Earlier tools win (AgentContext::tools() yields local-first).
    for t in tools {
        if t.spec().functions.iter().any(|f| f.name == function_name) {
            return Some(*t);
        }
    }

    None
}
