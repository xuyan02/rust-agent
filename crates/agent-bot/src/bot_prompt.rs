use agent_core::{AgentContext, SystemPromptSegment};
use anyhow::Result;
use crate::{GoalState, MemoryState};

/// Common bot prompt segment including goal, memory, and operating principles
pub struct BotPromptSegment {
    goal_state: GoalState,
    memory_state: MemoryState,
}

impl BotPromptSegment {
    pub fn new(goal_state: GoalState, memory_state: MemoryState) -> Self {
        Self {
            goal_state,
            memory_state,
        }
    }
}

// Safe because all fields are Send
unsafe impl Send for BotPromptSegment {}

#[async_trait::async_trait(?Send)]
impl SystemPromptSegment for BotPromptSegment {
    async fn render(&self, _ctx: &AgentContext<'_>) -> Result<String> {
        // Load states from disk
        self.goal_state.load().await?;
        self.memory_state.load().await?;

        // Start with operating principles from bot.md
        let mut output = String::new();
        output.push_str(include_str!("../prompts/bot.md"));
        output.push_str("\n\n---\n\n");

        output.push_str("#Who are you\n Your name is Billy.\n\n");

        // Add goal section
        output.push_str("# Current Goal\n\n");
        if let Some(goal) = self.goal_state.get() {
            output.push_str(&goal);
            output.push_str("\n\n");
        } else {
            output.push_str("*No current goal set.*\n\n");
        }

        // Add memory section
        let memories = self.memory_state.get_all();
        output.push_str("# Memory\n\n");
        if memories.is_empty() {
            output.push_str("*No memories recorded yet.*\n\n");
        } else {
            for (i, memory) in memories.iter().enumerate() {
                output.push_str(&(i + 1).to_string());
                output.push_str(". ");
                output.push_str(memory);
                output.push_str("\n");
            }
            output.push_str("\n");
        }

        Ok(output)
    }
}
