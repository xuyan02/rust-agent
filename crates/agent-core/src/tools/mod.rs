mod macro_example;
mod shell;

pub use macro_example::MacroExampleTool;

use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionSpec {
    pub name: String,
    pub description: String,
    pub parameters: ObjectSpec,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeSpec {
    Array(ArraySpec),
    Object(ObjectSpec),
    String(StringSpec),
    Boolean(BooleanSpec),
    Integer(IntegerSpec),
    Number(NumberSpec),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PropertySpec {
    pub name: String,
    pub ty: TypeSpec,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectSpec {
    pub properties: Vec<PropertySpec>,
    pub required: Vec<String>,
    pub additional_properties: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArraySpec {
    pub items: Box<TypeSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StringSpec {
    pub r#enum: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BooleanSpec {}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IntegerSpec {
    pub minimum: Option<i64>,
    pub maximum: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct NumberSpec {
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
}

impl ObjectSpec {
    pub fn to_json_schema_value(&self) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        m.insert(
            "type".to_string(),
            serde_json::Value::String("object".to_string()),
        );

        let mut props = serde_json::Map::new();
        for p in &self.properties {
            props.insert(p.name.clone(), p.ty.to_json_schema_value());
        }
        m.insert("properties".to_string(), serde_json::Value::Object(props));

        let req = self
            .required
            .iter()
            .cloned()
            .map(serde_json::Value::String)
            .collect::<Vec<_>>();
        m.insert("required".to_string(), serde_json::Value::Array(req));

        m.insert(
            "additionalProperties".to_string(),
            serde_json::Value::Bool(self.additional_properties),
        );

        serde_json::Value::Object(m)
    }
}

impl TypeSpec {
    pub fn to_json_schema_value(&self) -> serde_json::Value {
        match self {
            TypeSpec::Object(o) => o.to_json_schema_value(),
            TypeSpec::Array(a) => serde_json::json!({
                "type": "array",
                "items": a.items.to_json_schema_value(),
            }),
            TypeSpec::String(s) => {
                let mut m = serde_json::Map::new();
                m.insert(
                    "type".to_string(),
                    serde_json::Value::String("string".to_string()),
                );
                if let Some(values) = &s.r#enum {
                    m.insert(
                        "enum".to_string(),
                        serde_json::Value::Array(
                            values
                                .iter()
                                .cloned()
                                .map(serde_json::Value::String)
                                .collect(),
                        ),
                    );
                }
                serde_json::Value::Object(m)
            }
            TypeSpec::Boolean(_) => serde_json::json!({"type": "boolean"}),
            TypeSpec::Integer(n) => {
                let mut m = serde_json::Map::new();
                m.insert(
                    "type".to_string(),
                    serde_json::Value::String("integer".to_string()),
                );
                if let Some(min) = n.minimum {
                    m.insert("minimum".to_string(), serde_json::Value::from(min));
                }
                if let Some(max) = n.maximum {
                    m.insert("maximum".to_string(), serde_json::Value::from(max));
                }
                serde_json::Value::Object(m)
            }
            TypeSpec::Number(n) => {
                let mut m = serde_json::Map::new();
                m.insert(
                    "type".to_string(),
                    serde_json::Value::String("number".to_string()),
                );
                if let Some(min) = n.minimum {
                    m.insert("minimum".to_string(), serde_json::Value::from(min));
                }
                if let Some(max) = n.maximum {
                    m.insert("maximum".to_string(), serde_json::Value::from(max));
                }
                serde_json::Value::Object(m)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
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

    async fn invoke(
        &self,
        ctx: &crate::AgentContext<'_>,
        function_name: &str,
        args: &Value,
    ) -> Result<String>;
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

#[tool(id = "file", description = "Workspace file operations")]
impl FileTool {
    #[tool_fn(
        name = "file-read",
        description = "Read a UTF-8 text file under workspace",
        args(offset_lines(default = 0), limit_lines(default = 200))
    )]
    pub async fn read(
        &self,
        ctx: &crate::AgentContext<'_>,
        path: String,
        offset_lines: i64,
        limit_lines: i64,
    ) -> Result<String> {
        let abs = resolve_workspace_relative(ctx.session().workspace_path(), &path)?;

        let meta = tokio::fs::metadata(&abs)
            .await
            .with_context(|| format!("failed to stat {}", abs.display()))?;
        if meta.is_dir() {
            bail!(
                "path is a directory (file-read expects a file): {}",
                abs.display()
            );
        }

        let s = tokio::fs::read_to_string(&abs)
            .await
            .with_context(|| format!("failed to read {}", abs.display()))?;

        let offset = usize::try_from(offset_lines.max(0)).unwrap_or(0);
        let limit = usize::try_from(limit_lines.max(0)).unwrap_or(0);

        if offset == 0 && limit == 0 {
            return Ok(String::new());
        }
        if offset == 0 && limit >= s.lines().count() {
            return Ok(s);
        }

        let out = s
            .lines()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>()
            .join("\n");

        Ok(out)
    }

    #[tool_fn(
        name = "file-glob",
        description = "Find files under workspace by glob pattern. Pattern is relative to workspace. Note: recursive wildcard '**' must be a full path component (use '**/*.rs', not '**.rs'). Examples: '**/*.rs', '.github/workflows/*.yml', 'crates/*/src/**/*.rs'.",
        args(limit(default = 200))
    )]
    pub async fn glob(
        &self,
        ctx: &crate::AgentContext<'_>,
        pattern: String,
        limit: i64,
    ) -> Result<String> {
        let workspace = ctx.session().workspace_path();
        let pattern = pattern.trim();
        if pattern.is_empty() {
            bail!("pattern must not be empty");
        }
        if std::path::Path::new(pattern).is_absolute() {
            bail!("pattern must be relative");
        }

        let full_pattern = workspace.join(pattern).to_string_lossy().to_string();
        let limit = usize::try_from(limit.max(0)).unwrap_or(0).min(10_000);

        let mut out = Vec::new();
        for entry in glob::glob(&full_pattern).with_context(|| "invalid glob pattern")? {
            let path = entry?;
            if path.is_dir() {
                continue;
            }
            let rel = path
                .strip_prefix(workspace)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            out.push(rel);
            if limit != 0 && out.len() >= limit {
                break;
            }
        }

        Ok(out.join("\n"))
    }

    #[tool_fn(
        name = "file-write",
        description = "Write a UTF-8 text file under workspace",
        args(overwrite(default = false))
    )]
    pub async fn write(
        &self,
        ctx: &crate::AgentContext<'_>,
        path: String,
        content: String,
        overwrite: bool,
    ) -> Result<String> {
        let abs = resolve_workspace_relative(ctx.session().workspace_path(), &path)?;
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
        _ctx: &crate::AgentContext<'_>,
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
    pub async fn exec(&self, ctx: &crate::AgentContext<'_>, command: String) -> Result<String> {
        crate::tools::shell::validate_shell_command(&command)?;

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
