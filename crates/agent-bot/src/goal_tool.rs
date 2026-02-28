use agent_core::tools::{FunctionSpec, ObjectSpec, PropertySpec, StringSpec, Tool, ToolSpec, TypeSpec};
use agent_core::{AgentContext, DataNode, SystemPromptSegment};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::rc::Rc;

/// Persistable goal data
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoalData {
    pub goal: Option<String>,
}

/// Shared goal state for the bot
#[derive(Clone)]
pub struct GoalState {
    node: Rc<DataNode>,
}

impl GoalState {
    pub fn new(node: Rc<DataNode>) -> Self {
        Self { node }
    }

    /// Load goal from disk (idempotent)
    /// DataNode.load() automatically creates default if file doesn't exist
    pub async fn load(&self) -> Result<()> {
        self.node.load::<GoalData>().await
    }

    /// Flush goal to disk
    pub async fn flush(&self) -> Result<()> {
        self.node.flush().await
    }

    pub fn set(&self, goal: String) {
        // Get mutable reference to DataNode's cache
        if let Ok(Some(mut data)) = self.node.get_mut::<GoalData>() {
            data.goal = Some(goal);
            // drop data, auto-marks dirty
        }
    }

    pub fn get(&self) -> Option<String> {
        // Read from DataNode's cache
        if let Ok(Some(data)) = self.node.get::<GoalData>() {
            data.goal.clone()
        } else {
            None
        }
    }

    pub fn clear(&self) {
        // Get mutable reference to DataNode's cache
        if let Ok(Some(mut data)) = self.node.get_mut::<GoalData>() {
            data.goal = None;
            // drop data, auto-marks dirty
        }
    }
}

/// Dynamic system prompt segment that renders the current goal
pub struct GoalSegment {
    goal_state: GoalState,
}

// GoalState is Rc<RefCell<...>> which is not Send, but GoalSegment
// is only used in single-threaded context (Brain is !Send)
unsafe impl Send for GoalSegment {}

impl GoalSegment {
    pub fn new(goal_state: GoalState) -> Self {
        Self { goal_state }
    }
}

#[async_trait::async_trait(?Send)]
impl SystemPromptSegment for GoalSegment {
    async fn render(&self, _ctx: &AgentContext<'_>) -> Result<String> {
        // Load from disk on first access (idempotent)
        self.goal_state.load().await?;

        if let Some(goal) = self.goal_state.get() {
            Ok(format!(
                "═══════════════════════════════════════════════════════\n\
                CURRENT GOAL:\n\
                {}\n\
                ═══════════════════════════════════════════════════════",
                goal
            ))
        } else {
            Ok(String::new())
        }
    }
}

/// Goal tool for setting bot objectives
pub struct GoalTool {
    goal_state: GoalState,
}

impl GoalTool {
    pub fn new(goal_state: GoalState) -> Self {
        Self { goal_state }
    }
}

#[async_trait::async_trait(?Send)]
impl Tool for GoalTool {
    fn spec(&self) -> &ToolSpec {
        static SPEC: std::sync::OnceLock<ToolSpec> = std::sync::OnceLock::new();
        SPEC.get_or_init(|| ToolSpec {
            id: "goal".to_string(),
            description: "Manage bot objectives and goals".to_string(),
            functions: vec![
                FunctionSpec {
                    name: "set-goal".to_string(),
                    description: "Set or update the current goal. This goal will be visible to all agents working on the task.".to_string(),
                    parameters: ObjectSpec {
                        properties: vec![PropertySpec {
                            name: "goal".to_string(),
                            ty: TypeSpec::String(StringSpec::default()),
                        }],
                        required: vec!["goal".to_string()],
                        additional_properties: false,
                    },
                },
                FunctionSpec {
                    name: "get-goal".to_string(),
                    description: "Get the current goal if one is set.".to_string(),
                    parameters: ObjectSpec {
                        properties: vec![],
                        required: vec![],
                        additional_properties: false,
                    },
                },
                FunctionSpec {
                    name: "clear-goal".to_string(),
                    description: "Clear the current goal.".to_string(),
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
        _ctx: &agent_core::AgentContext<'_>,
        function_name: &str,
        args: &serde_json::Value,
    ) -> Result<String> {
        // Load from disk on first access (idempotent)
        self.goal_state.load().await?;

        match function_name {
            "set-goal" => {
                let goal = args
                    .get("goal")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'goal' argument"))?;

                self.goal_state.set(goal.to_string());
                // Flush immediately to persist changes
                self.goal_state.flush().await?;
                Ok(format!("Goal set: {}", goal))
            }
            "get-goal" => {
                if let Some(goal) = self.goal_state.get() {
                    Ok(format!("Current goal: {}", goal))
                } else {
                    Ok("No goal is currently set.".to_string())
                }
            }
            "clear-goal" => {
                self.goal_state.clear();
                // Flush immediately to persist changes
                self.goal_state.flush().await?;
                Ok("Goal cleared.".to_string())
            }
            _ => anyhow::bail!("unknown function: {}", function_name),
        }
    }
}
