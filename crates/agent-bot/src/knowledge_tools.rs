use agent_core::tools::{
    FunctionSpec, ObjectSpec, PropertySpec, StringSpec, Tool, ToolSpec, TypeSpec,
};
use agent_core::AgentContext;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::rc::Rc;

use crate::KnowledgeBase;

/// Tool for managing knowledge base files
pub struct KnowledgeTool {
    kb: Rc<KnowledgeBase>,
}

impl KnowledgeTool {
    pub fn new(kb: Rc<KnowledgeBase>) -> Self {
        Self { kb }
    }
}

#[async_trait(?Send)]
impl Tool for KnowledgeTool {
    fn spec(&self) -> &ToolSpec {
        static SPEC: std::sync::OnceLock<ToolSpec> = std::sync::OnceLock::new();
        SPEC.get_or_init(|| ToolSpec {
            id: "knowledge".to_string(),
            description: "Manage the knowledge base (deep memory) with hierarchical markdown files".to_string(),
            functions: vec![
                FunctionSpec {
                    name: "list-knowledge".to_string(),
                    description: "List all knowledge files and directories in the knowledge base.".to_string(),
                    parameters: ObjectSpec {
                        properties: vec![],
                        required: vec![],
                        additional_properties: false,
                    },
                },
                FunctionSpec {
                    name: "read-knowledge".to_string(),
                    description: "Read the contents of a knowledge file.".to_string(),
                    parameters: ObjectSpec {
                        properties: vec![PropertySpec {
                            name: "path".to_string(),
                            ty: TypeSpec::String(StringSpec::default()),
                        }],
                        required: vec!["path".to_string()],
                        additional_properties: false,
                    },
                },
                FunctionSpec {
                    name: "write-knowledge".to_string(),
                    description: "Create or update a knowledge file. Automatically creates parent directories. Use clear hierarchical paths like \"tech/rust/ownership.md\" or \"workflows/deployment.md\".".to_string(),
                    parameters: ObjectSpec {
                        properties: vec![
                            PropertySpec {
                                name: "path".to_string(),
                                ty: TypeSpec::String(StringSpec::default()),
                            },
                            PropertySpec {
                                name: "content".to_string(),
                                ty: TypeSpec::String(StringSpec::default()),
                            },
                        ],
                        required: vec!["path".to_string(), "content".to_string()],
                        additional_properties: false,
                    },
                },
                FunctionSpec {
                    name: "move-knowledge".to_string(),
                    description: "Move or rename a knowledge file to reorganize the knowledge structure.".to_string(),
                    parameters: ObjectSpec {
                        properties: vec![
                            PropertySpec {
                                name: "from".to_string(),
                                ty: TypeSpec::String(StringSpec::default()),
                            },
                            PropertySpec {
                                name: "to".to_string(),
                                ty: TypeSpec::String(StringSpec::default()),
                            },
                        ],
                        required: vec!["from".to_string(), "to".to_string()],
                        additional_properties: false,
                    },
                },
                FunctionSpec {
                    name: "delete-knowledge".to_string(),
                    description: "Delete an obsolete knowledge file that is no longer needed or has been superseded.".to_string(),
                    parameters: ObjectSpec {
                        properties: vec![PropertySpec {
                            name: "path".to_string(),
                            ty: TypeSpec::String(StringSpec::default()),
                        }],
                        required: vec!["path".to_string()],
                        additional_properties: false,
                    },
                },
            ],
        })
    }

    async fn invoke(
        &self,
        _ctx: &AgentContext<'_>,
        function_name: &str,
        arguments: &Value,
    ) -> Result<String> {
        match function_name {
            "list-knowledge" => {
                let files = self.kb.list_files().await?;
                let dirs = self.kb.list_dirs().await?;

                let mut output = String::new();
                output.push_str("# Knowledge Base Structure\n\n");

                if !dirs.is_empty() {
                    output.push_str("## Directories\n");
                    for dir in dirs {
                        output.push_str(&format!("- {}/\n", dir));
                    }
                    output.push_str("\n");
                }

                output.push_str("## Files\n");
                if files.is_empty() {
                    output.push_str("*No knowledge files yet.*\n");
                } else {
                    for file in files {
                        output.push_str(&format!("- {}\n", file));
                    }
                }

                Ok(output)
            }
            "read-knowledge" => {
                let path = arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .context("missing required parameter: path")?;

                self.kb.read_file(path).await
            }
            "write-knowledge" => {
                let path = arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .context("missing required parameter: path")?;

                let content = arguments
                    .get("content")
                    .and_then(|v| v.as_str())
                    .context("missing required parameter: content")?;

                self.kb.write_file(path, content).await?;
                Ok(format!("Knowledge file written: {}", path))
            }
            "move-knowledge" => {
                let from = arguments
                    .get("from")
                    .and_then(|v| v.as_str())
                    .context("missing required parameter: from")?;

                let to = arguments
                    .get("to")
                    .and_then(|v| v.as_str())
                    .context("missing required parameter: to")?;

                self.kb.move_file(from, to).await?;
                Ok(format!("Knowledge file moved: {} -> {}", from, to))
            }
            "delete-knowledge" => {
                let path = arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .context("missing required parameter: path")?;

                self.kb.delete_file(path).await?;
                Ok(format!("Knowledge file deleted: {}", path))
            }
            _ => anyhow::bail!("unknown function: {}", function_name),
        }
    }
}
