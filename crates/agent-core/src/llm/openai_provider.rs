use crate::llm::{LlmProvider, LlmSender, OpenAiProviderConfig, create_openai_sender};
use anyhow::Result;

pub struct OpenAiProvider {
    cfg: OpenAiProviderConfig,
}

impl OpenAiProvider {
    pub fn new(cfg: OpenAiProviderConfig) -> Self {
        Self { cfg }
    }
}

impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    fn supports_model(&self, _model: &str) -> bool {
        true
    }

    fn create_sender(&self, model: &str) -> Result<Box<dyn LlmSender>> {
        Ok(Box::new(create_openai_sender(
            self.cfg.clone(),
            model.to_string(),
        )?))
    }
}
