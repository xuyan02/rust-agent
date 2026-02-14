use crate::Result;
use crate::llm::{LlmProvider, LlmSender, OpenAiProviderConfig};
use anyhow::Context;

pub struct Runtime {
    openai: Option<OpenAiProviderConfig>,
    llm_providers: Vec<Box<dyn LlmProvider>>,
}

impl Runtime {
    pub fn create_sender(&self, model: &str) -> Result<Box<dyn LlmSender>> {
        for p in &self.llm_providers {
            if p.supports_model(model) {
                return p.create_sender(model);
            }
        }

        let openai = self
            .openai
            .clone()
            .context("missing openai provider config")?;
        let sender = crate::llm::create_openai_sender(openai, model.to_string())?;
        Ok(Box::new(sender))
    }
}

pub struct RuntimeBuilder {
    openai: Option<OpenAiProviderConfig>,
    llm_providers: Vec<Box<dyn LlmProvider>>,
}

impl RuntimeBuilder {
    pub fn new() -> Self {
        Self {
            openai: None,
            llm_providers: vec![],
        }
    }

    pub fn set_openai(mut self, cfg: OpenAiProviderConfig) -> Self {
        self.openai = Some(cfg);
        self
    }

    pub fn add_llm_provider(mut self, provider: Box<dyn LlmProvider>) -> Self {
        self.llm_providers.push(provider);
        self
    }

    pub fn build(self) -> Runtime {
        Runtime {
            openai: self.openai,
            llm_providers: self.llm_providers,
        }
    }
}

impl Default for RuntimeBuilder {
    fn default() -> Self {
        Self::new()
    }
}
