use crate::AgentContext;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use super::types::ToolSpec;

/// Core trait for all tools
#[async_trait(?Send)]
pub trait Tool {
    /// Get the tool specification
    fn spec(&self) -> &ToolSpec;

    /// Invoke a tool function
    async fn invoke(
        &self,
        ctx: &AgentContext<'_>,
        function_name: &str,
        args: &Value,
    ) -> Result<String>;
}