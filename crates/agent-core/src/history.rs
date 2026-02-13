use crate::Result;
use agent_llm::ChatMessage;
use async_trait::async_trait;
use std::cell::RefCell;

#[async_trait(?Send)]
pub trait History: Send {
    async fn get_all(&self) -> Result<Vec<ChatMessage>>;
    async fn append(&self, message: ChatMessage) -> Result<()>;
    async fn last(&self) -> Result<Option<ChatMessage>>;
}

#[derive(Default)]
pub struct InMemoryHistory {
    messages: RefCell<Vec<ChatMessage>>,
}

impl InMemoryHistory {
    pub fn new() -> Self {
        Self {
            messages: RefCell::new(vec![]),
        }
    }
}

#[async_trait(?Send)]
impl History for InMemoryHistory {
    async fn get_all(&self) -> Result<Vec<ChatMessage>> {
        Ok(self.messages.borrow().clone())
    }

    async fn append(&self, message: ChatMessage) -> Result<()> {
        self.messages.borrow_mut().push(message);
        Ok(())
    }

    async fn last(&self) -> Result<Option<ChatMessage>> {
        Ok(self.messages.borrow().last().cloned())
    }
}
