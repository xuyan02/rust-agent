mod token_estimator;
mod archiver;
mod compression;
mod persistent;

use crate::llm::ChatMessage;
use crate::Result;
use async_trait::async_trait;
use std::cell::RefCell;

// Import for internal use
use compression::CompressionStrategy;

// Re-export public types
pub use token_estimator::{estimate_tokens, estimate_message_tokens, estimate_messages_tokens};
pub use persistent::PersistentHistory;

/// Trait for managing conversation history.
#[async_trait(?Send)]
pub trait History {
    async fn get_all(&self, ctx: &crate::AgentContext<'_>) -> Result<Vec<ChatMessage>>;
    async fn append(&self, ctx: &crate::AgentContext<'_>, message: ChatMessage) -> Result<()>;
    async fn last(&self, ctx: &crate::AgentContext<'_>) -> Result<Option<ChatMessage>>;

    /// Clears all messages from history.
    async fn clear(&self, ctx: &crate::AgentContext<'_>) -> Result<()>;

    /// Returns the most recent `n` messages. If there are fewer than `n` messages,
    /// returns all messages. This is more efficient than get_all() for large histories.
    async fn get_recent(&self, ctx: &crate::AgentContext<'_>, n: usize) -> Result<Vec<ChatMessage>> {
        let all = self.get_all(ctx).await?;
        let start = all.len().saturating_sub(n);
        Ok(all[start..].to_vec())
    }
}

#[async_trait(?Send)]
impl<T: History + ?Sized> History for &T {
    async fn get_all(&self, ctx: &crate::AgentContext<'_>) -> Result<Vec<ChatMessage>> {
        (**self).get_all(ctx).await
    }

    async fn append(&self, ctx: &crate::AgentContext<'_>, message: ChatMessage) -> Result<()> {
        (**self).append(ctx, message).await
    }

    async fn last(&self, ctx: &crate::AgentContext<'_>) -> Result<Option<ChatMessage>> {
        (**self).last(ctx).await
    }

    async fn clear(&self, ctx: &crate::AgentContext<'_>) -> Result<()> {
        (**self).clear(ctx).await
    }

    async fn get_recent(&self, ctx: &crate::AgentContext<'_>, n: usize) -> Result<Vec<ChatMessage>> {
        (**self).get_recent(ctx, n).await
    }
}

/// In-memory history implementation (no persistence).
pub struct InMemoryHistory {
    messages: RefCell<Vec<ChatMessage>>,
    max_size: usize,
}

impl Default for InMemoryHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryHistory {
    /// Creates a new InMemoryHistory with default max size (1000 messages).
    pub fn new() -> Self {
        Self::new_with_limit(1000)
    }

    /// Creates a new InMemoryHistory with a custom max size.
    /// When the limit is reached, older messages are removed (sliding window).
    pub fn new_with_limit(max_size: usize) -> Self {
        Self {
            messages: RefCell::new(Vec::new()),
            max_size,
        }
    }

    /// Returns the current number of messages in history.
    pub fn len(&self) -> usize {
        self.messages.borrow().len()
    }

    /// Returns true if history is empty.
    pub fn is_empty(&self) -> bool {
        self.messages.borrow().is_empty()
    }

    /// Adjust keep_from position to not split tool call/result pairs.
    /// If keep_from points to a Tool message, move it back to include the preceding Assistant ToolCalls.
    fn adjust_keep_from_for_tool_calls(messages: &[ChatMessage], mut keep_from: usize) -> usize {
        use crate::llm::{ChatRole, ChatContent};

        if keep_from == 0 || keep_from >= messages.len() {
            return keep_from;
        }

        // Check if keep_from points to a Tool message
        if messages[keep_from].role == ChatRole::Tool {
            // Search backward for the corresponding Assistant ToolCalls message
            for i in (0..keep_from).rev() {
                if messages[i].role == ChatRole::Assistant {
                    if matches!(messages[i].content, ChatContent::ToolCalls(_)) {
                        // Found the ToolCalls message, move keep_from to before it
                        keep_from = i;
                        break;
                    }
                }
            }
        }

        keep_from
    }
}

#[async_trait(?Send)]
impl History for InMemoryHistory {
    async fn get_all(&self, _ctx: &crate::AgentContext<'_>) -> Result<Vec<ChatMessage>> {
        Ok(self.messages.borrow().clone())
    }

    async fn append(&self, _ctx: &crate::AgentContext<'_>, message: ChatMessage) -> Result<()> {
        let mut msgs = self.messages.borrow_mut();
        msgs.push(message);

        // Implement sliding window: remove oldest messages if limit exceeded
        // Also ensure we don't split tool call/result pairs and don't start with Tool messages
        if msgs.len() > self.max_size {
            let mut keep_from = msgs.len() - self.max_size;

            // Adjust keep_from to not split tool call pairs
            keep_from = Self::adjust_keep_from_for_tool_calls(&msgs, keep_from);

            msgs.drain(0..keep_from);

            // Clean leading Tool messages
            CompressionStrategy::clean_leading_tool_messages(&mut msgs);
        }

        Ok(())
    }

    async fn last(&self, _ctx: &crate::AgentContext<'_>) -> Result<Option<ChatMessage>> {
        Ok(self.messages.borrow().last().cloned())
    }

    async fn clear(&self, _ctx: &crate::AgentContext<'_>) -> Result<()> {
        self.messages.borrow_mut().clear();
        Ok(())
    }

    async fn get_recent(&self, _ctx: &crate::AgentContext<'_>, n: usize) -> Result<Vec<ChatMessage>> {
        let msgs = self.messages.borrow();
        let start = msgs.len().saturating_sub(n);
        Ok(msgs[start..].to_vec())
    }
}
