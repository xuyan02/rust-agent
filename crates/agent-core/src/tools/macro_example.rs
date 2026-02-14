use anyhow::Result;

use crate::tools::{tool, tool_fn};

pub struct MacroExampleTool;

#[tool(
    id = "macro_example",
    description = "Example tool implemented via proc-macro"
)]
impl MacroExampleTool {
    /// Echo input for testing macro-generated tools.
    #[tool_fn(name = "macro.echo")]
    pub async fn echo(&self, text: String) -> Result<String> {
        Ok(text)
    }

    #[tool_fn(name = "macro.pwd")]
    pub async fn pwd(&self, ctx: &crate::AgentContext<'_>) -> Result<String> {
        Ok(ctx.session().workspace_path().to_string_lossy().to_string())
    }
}
