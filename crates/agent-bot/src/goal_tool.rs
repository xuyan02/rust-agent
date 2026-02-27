use agent_core::tools::{FunctionSpec, ObjectSpec, PropertySpec, StringSpec, Tool, ToolSpec, TypeSpec};
use anyhow::Result;
use std::cell::RefCell;
use std::rc::Rc;

/// Shared goal state for the bot
#[derive(Clone)]
pub struct GoalState {
    inner: Rc<RefCell<Option<String>>>,
}

impl GoalState {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(None)),
        }
    }

    pub fn set(&self, goal: String) {
        *self.inner.borrow_mut() = Some(goal);
    }

    pub fn get(&self) -> Option<String> {
        self.inner.borrow().clone()
    }

    pub fn clear(&self) {
        *self.inner.borrow_mut() = None;
    }
}

impl Default for GoalState {
    fn default() -> Self {
        Self::new()
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
        match function_name {
            "set-goal" => {
                let goal = args
                    .get("goal")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'goal' argument"))?;

                self.goal_state.set(goal.to_string());
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
                Ok("Goal cleared.".to_string())
            }
            _ => anyhow::bail!("unknown function: {}", function_name),
        }
    }
}
