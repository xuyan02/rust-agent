use crate::Result;
use crate::llm::ChatMessage;
use async_trait::async_trait;
use std::cell::RefCell;

#[async_trait(?Send)]
pub trait History {
    async fn get_all(&self) -> Result<Vec<ChatMessage>>;
    async fn append(&self, message: ChatMessage) -> Result<()>;
    async fn last(&self) -> Result<Option<ChatMessage>>;
}

#[async_trait(?Send)]
impl<T: History + ?Sized> History for &T {
    async fn get_all(&self) -> Result<Vec<ChatMessage>> {
        (**self).get_all().await
    }

    async fn append(&self, message: ChatMessage) -> Result<()> {
        (**self).append(message).await
    }

    async fn last(&self) -> Result<Option<ChatMessage>> {
        (**self).last().await
    }
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
