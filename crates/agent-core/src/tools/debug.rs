use crate::AgentContext;
use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use serde_json::Value;

use super::{
    types::{FunctionSpec, ObjectSpec, PropertySpec, StringSpec, ToolSpec, TypeSpec},
    Tool,
};

/// Debug utilities tool
pub struct DebugTool;

impl DebugTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DebugTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait(?Send)]
impl Tool for DebugTool {
    fn spec(&self) -> &ToolSpec {
        static SPEC: std::sync::OnceLock<ToolSpec> = std::sync::OnceLock::new();
        SPEC.get_or_init(|| ToolSpec {
            id: "debug".to_string(),
            description: "Debug utilities".to_string(),
            functions: vec![FunctionSpec {
                name: "debug-echo".to_string(),
                description: "Echo input for debugging".to_string(),
                parameters: ObjectSpec {
                    properties: vec![PropertySpec {
                        name: "text".to_string(),
                        ty: TypeSpec::String(StringSpec::default()),
                    }],
                    required: vec!["text".to_string()],
                    additional_properties: false,
                },
            }],
        })
    }

    async fn invoke(
        &self,
        _ctx: &AgentContext<'_>,
        function_name: &str,
        args: &Value,
    ) -> Result<String> {
        match function_name {
            "debug-echo" => {
                let text = args
                    .get("text")
                    .and_then(|v| v.as_str())
                    .context("missing 'text'")?;
                Ok(text.to_string())
            }
            _ => bail!("unknown function: {function_name}"),
        }
    }
}