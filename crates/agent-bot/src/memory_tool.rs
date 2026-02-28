use agent_core::tools::{ArraySpec, FunctionSpec, ObjectSpec, PropertySpec, StringSpec, Tool, ToolSpec, TypeSpec};
use agent_core::{AgentContext, DataNode, SystemPromptSegment, estimate_tokens};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::rc::Rc;

/// Persistable memory data
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryData {
    pub memories: Vec<String>,
}

/// Shared memory state for the bot
#[derive(Clone)]
pub struct MemoryState {
    node: Rc<DataNode>,
}

impl MemoryState {
    pub fn new(node: Rc<DataNode>) -> Self {
        Self { node }
    }

    /// Load memories from disk (idempotent)
    /// DataNode.load() automatically creates default if file doesn't exist
    pub async fn load(&self) -> Result<()> {
        self.node.load::<MemoryData>().await
    }

    /// Flush memories to disk
    pub async fn flush(&self) -> Result<()> {
        self.node.flush().await
    }

    pub fn add(&self, memory: String) {
        // Directly modify DataNode's cache
        if let Ok(mut data) = self.node.get_or_default::<MemoryData>() {
            data.memories.push(memory);
            // drop data, auto-marks dirty
        }
    }

    pub fn get_all(&self) -> Vec<String> {
        // Read from DataNode's cache
        if let Ok(Some(data)) = self.node.get::<MemoryData>() {
            data.memories.clone()
        } else {
            Vec::new()
        }
    }

    /// Replace all memories with a new set (used for compression)
    pub fn replace_all(&self, new_memories: Vec<String>) {
        if let Ok(mut data) = self.node.get_or_default::<MemoryData>() {
            data.memories = new_memories;
            // drop data, auto-marks dirty
        }
    }

    /// Count total tokens in all memories using precise tiktoken estimation
    pub fn count_tokens(&self) -> usize {
        let memories = self.get_all();
        memories.iter()
            .map(|m| estimate_tokens(m))
            .sum()
    }
}


/// Dynamic system prompt segment that renders memories
pub struct MemorySegment {
    memory_state: MemoryState,
}

// MemoryState is Rc<RefCell<...>> which is not Send, but MemorySegment
// is only used in single-threaded context (Brain is !Send)
unsafe impl Send for MemorySegment {}

impl MemorySegment {
    pub fn new(memory_state: MemoryState) -> Self {
        Self { memory_state }
    }
}

#[async_trait::async_trait(?Send)]
impl SystemPromptSegment for MemorySegment {
    async fn render(&self, _ctx: &AgentContext<'_>) -> Result<String> {
        // Load from disk on first access (idempotent)
        self.memory_state.load().await?;

        let memories = self.memory_state.get_all();
        if memories.is_empty() {
            Ok(String::new())
        } else {
            let token_count = self.memory_state.count_tokens();
            let memory_count = memories.len();

            let mut result = String::from(
                "═══════════════════════════════════════════════════════\n\
                MEMORY:\n"
            );
            result.push_str(&format!("(Total: {} memories, {} tokens)\n\n", memory_count, token_count));
            for (i, memory) in memories.iter().enumerate() {
                result.push_str(&format!("{}. {}\n", i + 1, memory));
            }
            result.push_str("═══════════════════════════════════════════════════════");
            Ok(result)
        }
    }
}

/// Memory tool for recording important information
pub struct MemoryTool {
    memory_state: MemoryState,
}

impl MemoryTool {
    pub fn new(memory_state: MemoryState) -> Self {
        Self { memory_state }
    }
}

#[async_trait::async_trait(?Send)]
impl Tool for MemoryTool {
    fn spec(&self) -> &ToolSpec {
        static SPEC: std::sync::OnceLock<ToolSpec> = std::sync::OnceLock::new();
        SPEC.get_or_init(|| ToolSpec {
            id: "memory".to_string(),
            description: "Manage bot memory - record and recall important information".to_string(),
            functions: vec![
                FunctionSpec {
                    name: "remember".to_string(),
                    description: "Add a short memory record. Keep it concise (1-2 sentences). This will appear in all brains' prompts.".to_string(),
                    parameters: ObjectSpec {
                        properties: vec![PropertySpec {
                            name: "memory".to_string(),
                            ty: TypeSpec::String(StringSpec::default()),
                        }],
                        required: vec!["memory".to_string()],
                        additional_properties: false,
                    },
                },
                FunctionSpec {
                    name: "list-memories".to_string(),
                    description: "List all memory records.".to_string(),
                    parameters: ObjectSpec {
                        properties: vec![],
                        required: vec![],
                        additional_properties: false,
                    },
                },
                FunctionSpec {
                    name: "get-memory-size".to_string(),
                    description: "Get the current memory size (number of records and precise token count).".to_string(),
                    parameters: ObjectSpec {
                        properties: vec![],
                        required: vec![],
                        additional_properties: false,
                    },
                },
                FunctionSpec {
                    name: "replace-memories".to_string(),
                    description: "Replace all current memories with a new compressed set. Used for memory compression. Provide an array of memory strings.".to_string(),
                    parameters: ObjectSpec {
                        properties: vec![PropertySpec {
                            name: "memories".to_string(),
                            ty: TypeSpec::Array(ArraySpec {
                                items: Box::new(TypeSpec::String(StringSpec::default())),
                            }),
                        }],
                        required: vec!["memories".to_string()],
                        additional_properties: false,
                    },
                },
            ],
        })
    }

    async fn invoke(
        &self,
        _ctx: &agent_core::AgentContext<'_>,
        function_name: &str,
        args: &serde_json::Value,
    ) -> Result<String> {
        // Load from disk on first access (idempotent)
        self.memory_state.load().await?;

        match function_name {
            "remember" => {
                let memory = args
                    .get("memory")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'memory' argument"))?;

                self.memory_state.add(memory.to_string());
                // Flush immediately to persist changes
                self.memory_state.flush().await?;
                Ok(format!("Memory recorded: {}", memory))
            }
            "list-memories" => {
                let memories = self.memory_state.get_all();
                if memories.is_empty() {
                    Ok("No memories recorded.".to_string())
                } else {
                    let mut result = String::from("Recorded memories:\n");
                    for (i, memory) in memories.iter().enumerate() {
                        result.push_str(&format!("{}. {}\n", i + 1, memory));
                    }
                    Ok(result)
                }
            }
            "get-memory-size" => {
                let token_count = self.memory_state.count_tokens();
                let memory_count = self.memory_state.get_all().len();
                Ok(format!(
                    "Memory size: {} memories, {} tokens",
                    memory_count, token_count
                ))
            }
            "replace-memories" => {
                let new_memories = args
                    .get("memories")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| anyhow::anyhow!("missing 'memories' argument"))?
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>();

                self.memory_state.replace_all(new_memories.clone());
                // Flush immediately to persist changes
                self.memory_state.flush().await?;

                let new_token_count = self.memory_state.count_tokens();
                Ok(format!(
                    "Memories replaced: {} memories, {} tokens",
                    new_memories.len(),
                    new_token_count
                ))
            }
            _ => anyhow::bail!("unknown function: {}", function_name),
        }
    }
}
