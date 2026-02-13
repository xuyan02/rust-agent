use crate::{ChatMessage, LlmSender};
use anyhow::Result;
use async_trait::async_trait;

pub trait LlmProvider: Send {
    fn name(&self) -> &str;
    fn supports_model(&self, model: &str) -> bool;

    fn create_sender(&self, model: &str) -> Result<Box<dyn LlmSender>>;

    fn create_request(
        &self,
        model: &str,
        messages: Vec<ChatMessage>,
        tools_json: Vec<serde_json::Value>,
    ) -> Result<Box<dyn LlmRequest>> {
        let sender = self.create_sender(model)?;
        Ok(Box::new(SenderBackedRequest {
            sender,
            messages,
            tools_json,
        }))
    }
}

#[async_trait(?Send)]
pub trait LlmRequest: Send {
    async fn run(&mut self) -> Result<ChatMessage>;
}

struct SenderBackedRequest {
    sender: Box<dyn LlmSender>,
    messages: Vec<ChatMessage>,
    tools_json: Vec<serde_json::Value>,
}

#[async_trait(?Send)]
impl LlmRequest for SenderBackedRequest {
    async fn run(&mut self) -> Result<ChatMessage> {
        let msgs = std::mem::take(&mut self.messages);
        let _tools = std::mem::take(&mut self.tools_json);
        self.sender.send(&msgs).await
    }
}

#[derive(Default)]
pub struct LlmContext {
    providers: Vec<Box<dyn LlmProvider>>,
}

impl LlmContext {
    pub fn new() -> Self {
        Self { providers: vec![] }
    }

    pub fn clear(&mut self) {
        self.providers.clear();
    }

    pub fn register(&mut self, provider: Box<dyn LlmProvider>) {
        self.providers.push(provider);
    }

    pub fn create(
        &self,
        model: &str,
        messages: Vec<ChatMessage>,
        tools_json: Vec<serde_json::Value>,
    ) -> Option<Result<Box<dyn LlmRequest>>> {
        for p in &self.providers {
            if p.supports_model(model) {
                return Some(p.create_request(model, messages, tools_json));
            }
        }
        None
    }
}
