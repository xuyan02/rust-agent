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
    pub async fn pwd(&self, workspace: &std::path::Path) -> Result<String> {
        Ok(workspace.to_string_lossy().to_string())
    }
}
