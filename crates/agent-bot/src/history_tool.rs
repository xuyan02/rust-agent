use agent_core::tools::{FunctionSpec, ObjectSpec, PropertySpec, StringSpec, Tool, ToolSpec, TypeSpec};
use agent_core::{AgentContext, DirNode};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::rc::Rc;

/// Safely truncate a string to at most `max_bytes` bytes, ensuring we don't cut in the middle of a UTF-8 character.
fn truncate_str(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }

    // Find the largest valid UTF-8 character boundary at or before max_bytes
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }

    &s[..end]
}

/// Tool for reading brain histories (read-only)
pub struct HistoryTool {
    conv_dir: Rc<DirNode>,
    work_dir: Rc<DirNode>,
}

impl HistoryTool {
    pub fn new(conv_dir: Rc<DirNode>, work_dir: Rc<DirNode>) -> Self {
        Self { conv_dir, work_dir }
    }

    async fn read_history_file(&self, dir: &DirNode) -> Result<String> {
        let path = dir.full_path().join("history.yaml");

        if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
            return Ok("*No history yet.*".to_string());
        }

        let content = tokio::fs::read_to_string(&path)
            .await
            .with_context(|| format!("failed to read history from {}", path.display()))?;

        // Parse and format for readability
        let messages: Vec<agent_core::llm::ChatMessage> = serde_yaml::from_str(&content)
            .with_context(|| format!("failed to parse history YAML from {}", path.display()))?;

        let mut output = String::new();
        output.push_str("## Recent History (history.yaml)\n\n");

        for (i, msg) in messages.iter().enumerate() {
            let formatted = self.format_message(i + 1, msg);
            output.push_str(&formatted);
        }

        Ok(output)
    }

    async fn list_archives(&self, dir: &DirNode) -> Result<Vec<String>> {
        let archive_dir = dir.full_path().join("history");

        if !tokio::fs::try_exists(&archive_dir).await.unwrap_or(false) {
            return Ok(vec![]);
        }

        let mut entries = tokio::fs::read_dir(&archive_dir).await
            .context("failed to read archive directory")?;

        let mut archives = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".yaml") {
                    archives.push(name.to_string());
                }
            }
        }

        archives.sort();
        Ok(archives)
    }

    async fn read_archive(&self, dir: &DirNode, filename: &str) -> Result<String> {
        let archive_path = dir.full_path().join("history").join(filename);

        let content = tokio::fs::read_to_string(&archive_path).await
            .with_context(|| format!("failed to read archive: {}", archive_path.display()))?;

        // Parse ArchivedHistory
        #[derive(serde::Deserialize)]
        struct ArchivedHistory {
            compressed_at: String,
            message_count: usize,
            estimated_tokens: usize,
            messages: Vec<agent_core::llm::ChatMessage>,
        }

        let archived: ArchivedHistory = serde_yaml::from_str(&content)
            .with_context(|| format!("failed to parse archive from {}", archive_path.display()))?;

        let mut output = String::new();
        output.push_str(&format!(
            "## Archive: {}\nCompressed at: {}\nMessages: {}, Tokens: ~{}\n\n",
            filename, archived.compressed_at, archived.message_count, archived.estimated_tokens
        ));

        for (i, msg) in archived.messages.iter().enumerate() {
            let formatted = self.format_message(i + 1, msg);
            output.push_str(&formatted);
        }

        Ok(output)
    }

    fn format_message(&self, index: usize, msg: &agent_core::llm::ChatMessage) -> String {
        let role = match msg.role {
            agent_core::llm::ChatRole::System => "System",
            agent_core::llm::ChatRole::User => "User",
            agent_core::llm::ChatRole::Assistant => "Assistant",
            agent_core::llm::ChatRole::Tool => "Tool",
        };

        let content_preview = match &msg.content {
            agent_core::llm::ChatContent::Text(text) => {
                if text.len() > 200 {
                    format!("{}...", truncate_str(text, 200))
                } else {
                    text.clone()
                }
            }
            agent_core::llm::ChatContent::ToolCalls(calls) => {
                format!("[Tool calls: {}]", serde_json::to_string(calls).unwrap_or_default())
            }
            agent_core::llm::ChatContent::ToolResult { tool_call_id, result } => {
                let result_preview = if result.len() > 100 {
                    format!("{}...", truncate_str(result, 100))
                } else {
                    result.clone()
                };
                format!("[Tool result {}: {}]", tool_call_id, result_preview)
            }
        };

        format!("{}. {}: {}\n\n", index, role, content_preview)
    }
}

#[async_trait(?Send)]
impl Tool for HistoryTool {
    fn spec(&self) -> &ToolSpec {
        static SPEC: std::sync::OnceLock<ToolSpec> = std::sync::OnceLock::new();
        SPEC.get_or_init(|| ToolSpec {
            id: "history".to_string(),
            description: "Read conversation and work brain histories for analysis".to_string(),
            functions: vec![
                FunctionSpec {
                    name: "read-conv-history".to_string(),
                    description: "Read the conversation brain's recent history (interactions with users). Only shows recent messages in history.yaml.".to_string(),
                    parameters: ObjectSpec {
                        properties: vec![],
                        required: vec![],
                        additional_properties: false,
                    },
                },
                FunctionSpec {
                    name: "read-work-history".to_string(),
                    description: "Read the work brain's recent history (task execution details). Only shows recent messages in history.yaml.".to_string(),
                    parameters: ObjectSpec {
                        properties: vec![],
                        required: vec![],
                        additional_properties: false,
                    },
                },
                FunctionSpec {
                    name: "read-conv-archive".to_string(),
                    description: "Read a specific archived history file for conversation brain. Use this when you see '[Previous N messages archived to history/{filename}]' in the history to access the detailed archived content.".to_string(),
                    parameters: ObjectSpec {
                        properties: vec![PropertySpec {
                            name: "filename".to_string(),
                            ty: TypeSpec::String(StringSpec::default()),
                        }],
                        required: vec!["filename".to_string()],
                        additional_properties: false,
                    },
                },
                FunctionSpec {
                    name: "read-work-archive".to_string(),
                    description: "Read a specific archived history file for work brain. Use this when you see '[Previous N messages archived to history/{filename}]' in the history to access the detailed archived content.".to_string(),
                    parameters: ObjectSpec {
                        properties: vec![PropertySpec {
                            name: "filename".to_string(),
                            ty: TypeSpec::String(StringSpec::default()),
                        }],
                        required: vec!["filename".to_string()],
                        additional_properties: false,
                    },
                },
                FunctionSpec {
                    name: "list-conv-archives".to_string(),
                    description: "List all archived history files for conversation brain.".to_string(),
                    parameters: ObjectSpec {
                        properties: vec![],
                        required: vec![],
                        additional_properties: false,
                    },
                },
                FunctionSpec {
                    name: "list-work-archives".to_string(),
                    description: "List all archived history files for work brain.".to_string(),
                    parameters: ObjectSpec {
                        properties: vec![],
                        required: vec![],
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
            "read-conv-history" => {
                self.read_history_file(&self.conv_dir).await
            }
            "read-work-history" => {
                self.read_history_file(&self.work_dir).await
            }
            "read-conv-archive" => {
                let filename = arguments
                    .get("filename")
                    .and_then(|v| v.as_str())
                    .context("missing required parameter: filename")?;
                self.read_archive(&self.conv_dir, filename).await
            }
            "read-work-archive" => {
                let filename = arguments
                    .get("filename")
                    .and_then(|v| v.as_str())
                    .context("missing required parameter: filename")?;
                self.read_archive(&self.work_dir, filename).await
            }
            "list-conv-archives" => {
                let archives = self.list_archives(&self.conv_dir).await?;
                if archives.is_empty() {
                    Ok("*No archived history yet.*".to_string())
                } else {
                    let mut output = String::from("# Conversation Brain Archives\n\n");
                    for archive in archives {
                        output.push_str(&format!("- {}\n", archive));
                    }
                    Ok(output)
                }
            }
            "list-work-archives" => {
                let archives = self.list_archives(&self.work_dir).await?;
                if archives.is_empty() {
                    Ok("*No archived history yet.*".to_string())
                } else {
                    let mut output = String::from("# Work Brain Archives\n\n");
                    for archive in archives {
                        output.push_str(&format!("- {}\n", archive));
                    }
                    Ok(output)
                }
            }
            _ => anyhow::bail!("unknown function: {}", function_name),
        }
    }
}
