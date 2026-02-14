use crate::llm::{ChatMessage, LlmSender};
use anyhow::Result;
use async_trait::async_trait;

pub trait LlmProvider: Send {
    fn name(&self) -> &str;
    fn supports_model(&self, model: &str) -> bool;

    fn create_sender(&self, model: &str) -> Result<Box<dyn LlmSender>>;

    fn create_request<'a>(
        &'a self,
        model: &str,
        messages: Vec<ChatMessage>,
        tools: Vec<&'a dyn crate::tools::Tool>,
    ) -> Result<Box<dyn LlmRequest + 'a>> {
        let sender = self.create_sender(model)?;
        Ok(Box::new(SenderBackedRequest {
            sender,
            messages,
            tools,
        }))
    }
}

#[async_trait(?Send)]
pub trait LlmRequest {
    async fn run(&mut self) -> Result<ChatMessage>;
}

struct SenderBackedRequest<'a> {
    sender: Box<dyn LlmSender>,
    messages: Vec<ChatMessage>,
    tools: Vec<&'a dyn crate::tools::Tool>,
}

#[async_trait(?Send)]
impl LlmRequest for SenderBackedRequest<'_> {
    async fn run(&mut self) -> Result<ChatMessage> {
        let msgs = std::mem::take(&mut self.messages);
        let tools = std::mem::take(&mut self.tools);
        self.sender.send(&msgs, tools.as_slice()).await
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

    pub fn create<'a>(
        &'a self,
        model: &str,
        messages: Vec<ChatMessage>,
        tools: Vec<&'a dyn crate::tools::Tool>,
    ) -> Option<Result<Box<dyn LlmRequest + 'a>>> {
        for p in &self.providers {
            if p.supports_model(model) {
                return Some(p.create_request(model, messages, tools));
            }
        }
        None
    }
}
