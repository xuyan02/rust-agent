mod macro_example;
mod shell;

pub use macro_example::MacroExampleTool;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub id: String,
    pub description: String,
    pub functions: Vec<FunctionSpec>,
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub function_name: String,
    pub arguments: Value,
}

use async_trait::async_trait;

pub use agent_macros::{tool, tool_arg, tool_fn};

#[async_trait(?Send)]
pub trait Tool {
    fn spec(&self) -> &ToolSpec;

    async fn invoke(&self, workspace: &Path, function_name: &str, args: &Value) -> Result<String>;
}

pub struct FileTool;

impl FileTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FileTool {
    fn default() -> Self {
        Self::new()
    }
}

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
                name: "debug.echo".to_string(),
                description: "Echo input for debugging".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {"text": {"type": "string"}},
                    "required": ["text"],
                    "additionalProperties": false
                }),
            }],
        })
    }

    async fn invoke(&self, _workspace: &Path, function_name: &str, args: &Value) -> Result<String> {
        match function_name {
            "debug.echo" => {
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

fn resolve_workspace_relative(workspace: &Path, rel: &str) -> Result<PathBuf> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        bail!("path must be relative");
    }

    for c in rel_path.components() {
        if matches!(c, std::path::Component::ParentDir) {
            bail!("path must not contain '..'");
        }
    }

    let joined = workspace.join(rel_path);
    let canon_workspace = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());
    let canon_joined = joined.canonicalize().unwrap_or_else(|_| joined.clone());

    if !canon_joined.starts_with(&canon_workspace) {
        bail!("path escapes workspace");
    }

    Ok(joined)
}

#[async_trait(?Send)]
impl Tool for ShellTool {
    fn spec(&self) -> &ToolSpec {
        static SPEC: std::sync::OnceLock<ToolSpec> = std::sync::OnceLock::new();
        SPEC.get_or_init(|| ToolSpec {
            id: "shell".to_string(),
            description: "Execute shell commands in workspace".to_string(),
            functions: vec![FunctionSpec {
                name: "shell.exec".to_string(),
                description: "Execute a shell command (bash -lc) with cwd=workspace".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {"command": {"type": "string"}},
                    "required": ["command"],
                    "additionalProperties": false
                }),
            }],
        })
    }

    async fn invoke(&self, workspace: &Path, function_name: &str, args: &Value) -> Result<String> {
        match function_name {
            "shell.exec" => {
                let command = args
                    .get("command")
                    .and_then(|v| v.as_str())
                    .context("missing 'command'")?;

                crate::tools::shell::validate_shell_command(command)?;

                let output = tokio::process::Command::new("bash")
                    .arg("-lc")
                    .arg(command)
                    .current_dir(workspace)
                    .output()
                    .await
                    .with_context(|| "failed to execute bash")?;

                let mut combined = Vec::new();
                combined.extend_from_slice(&output.stdout);
                combined.extend_from_slice(&output.stderr);

                Ok(String::from_utf8_lossy(&combined).to_string())
            }
            _ => bail!("unknown function: {function_name}"),
        }
    }
}

#[async_trait(?Send)]
impl Tool for FileTool {
    fn spec(&self) -> &ToolSpec {
        // Static spec.
        static SPEC: std::sync::OnceLock<ToolSpec> = std::sync::OnceLock::new();
        SPEC.get_or_init(|| ToolSpec {
            id: "file".to_string(),
            description: "Workspace file operations".to_string(),
            functions: vec![
                FunctionSpec {
                    name: "file.read".to_string(),
                    description: "Read a UTF-8 text file under workspace".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {"path": {"type": "string"}},
                        "required": ["path"],
                        "additionalProperties": false
                    }),
                },
                FunctionSpec {
                    name: "file.write".to_string(),
                    description: "Write a UTF-8 text file under workspace".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"},
                            "content": {"type": "string"},
                            "overwrite": {"type": "boolean"}
                        },
                        "required": ["path", "content"],
                        "additionalProperties": false
                    }),
                },
            ],
        })
    }

    async fn invoke(&self, workspace: &Path, function_name: &str, args: &Value) -> Result<String> {
        match function_name {
            "file.read" => {
                let path = args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .context("missing 'path'")?;
                let abs = resolve_workspace_relative(workspace, path)?;
                let s = tokio::fs::read_to_string(&abs)
                    .await
                    .with_context(|| format!("failed to read {}", abs.display()))?;
                Ok(s)
            }
            "file.write" => {
                let path = args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .context("missing 'path'")?;
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .context("missing 'content'")?;
                let overwrite = args
                    .get("overwrite")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let abs = resolve_workspace_relative(workspace, path)?;
                if tokio::fs::try_exists(&abs).await.unwrap_or(false) && !overwrite {
                    bail!("file exists (set overwrite=true)");
                }
                if let Some(parent) = abs.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .with_context(|| format!("failed to create dir {}", parent.display()))?;
                }
                tokio::fs::write(&abs, content)
                    .await
                    .with_context(|| format!("failed to write {}", abs.display()))?;

                Ok(String::new())
            }
            _ => bail!("unknown function: {function_name}"),
        }
    }
}
