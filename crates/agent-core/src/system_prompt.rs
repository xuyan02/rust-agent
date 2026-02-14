use crate::AgentContext;
use anyhow::Result;
use async_trait::async_trait;

#[async_trait(?Send)]
pub trait SystemPromptSegment: Send {
    async fn render(&self, ctx: &AgentContext<'_>) -> Result<String>;
}

pub struct StaticSystemPromptSegment {
    text: String,
}

impl StaticSystemPromptSegment {
    pub fn new(text: String) -> Self {
        Self { text }
    }
}

#[async_trait(?Send)]
impl SystemPromptSegment for StaticSystemPromptSegment {
    async fn render(&self, _ctx: &AgentContext<'_>) -> Result<String> {
        Ok(self.text.clone())
    }
}
