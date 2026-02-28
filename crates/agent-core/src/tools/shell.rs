use crate::AgentContext;
use anyhow::{Context, Result};

use super::{tool, tool_fn};

/// Shell tool for executing commands in workspace
pub struct ShellTool;

impl ShellTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ShellTool {
    fn default() -> Self {
        Self::new()
    }
}

#[tool(id = "shell", description = "Execute shell commands in workspace")]
impl ShellTool {
    #[tool_fn(
        name = "shell-exec",
        description = "Execute a shell command (bash -lc) with cwd=workspace"
    )]
    pub async fn exec(&self, ctx: &AgentContext<'_>, command: String) -> Result<String> {
        validate_shell_command(&command)?;

        let output = tokio::process::Command::new("bash")
            .arg("-lc")
            .arg(&command)
            .current_dir(ctx.session().workspace_path())
            .output()
            .await
            .with_context(|| "failed to execute bash")?;

        let mut combined = Vec::new();
        combined.extend_from_slice(&output.stdout);
        combined.extend_from_slice(&output.stderr);

        Ok(String::from_utf8_lossy(&combined).to_string())
    }
}

/// Validate shell command for safety
pub fn validate_shell_command(cmd: &str) -> Result<()> {
    let cmd = cmd.trim();
    if cmd.is_empty() {
        anyhow::bail!("command must not be empty");
    }

    // Basic safety checks
    let dangerous_patterns = [
        "rm -rf /",
        "rm -rf /*",
        ":(){ :|:& };:",
        "mkfs",
        "dd if=/dev/zero",
        "chmod -R 777 /",
    ];

    for pattern in dangerous_patterns {
        if cmd.contains(pattern) {
            anyhow::bail!("command contains dangerous pattern: {}", pattern);
        }
    }

    Ok(())
}