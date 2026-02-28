use crate::llm::ChatMessage;
use crate::{DirNode, Result};
use anyhow::Context as _;
use async_trait::async_trait;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use super::{History, archiver::HistoryArchiver, compression::{CompressionConfig, CompressionStrategy}, token_estimator::estimate_messages_tokens};

/// History implementation that persists to disk as YAML.
///
/// Messages are stored as a YAML array directly (no wrapper).
/// Supports automatic compression when token threshold is exceeded.
pub struct PersistentHistory {
    max_size: usize,
    // Cache to avoid re-reading from disk on every operation
    cache: RefCell<Option<Vec<ChatMessage>>>,
    // Compression strategy
    compression_strategy: CompressionStrategy,
    // DirNode for storage location
    dir_node: Rc<DirNode>,
}

impl PersistentHistory {
    /// Creates a new PersistentHistory with the given directory node.
    pub fn new(dir_node: Rc<DirNode>) -> Self {
        // Hardcoded configuration
        // Trigger compression when exceeds 8K tokens, compress oldest 4K tokens
        let max_size = 1000;
        let compression_config = CompressionConfig {
            compress_threshold_tokens: 8000,
            compress_target_tokens: 4000,
            keep_recent_tokens: 2000,
            enabled: true,
        };

        Self {
            max_size,
            cache: RefCell::new(None),
            compression_strategy: CompressionStrategy::new(compression_config),
            dir_node,
        }
    }

    /// Get the history file path.
    fn get_path(&self) -> Result<PathBuf> {
        let full_path = self.dir_node.full_path().join("history.yaml");
        Ok(full_path)
    }

    /// Get the history archive directory path.
    fn get_history_archive_dir(&self) -> Result<PathBuf> {
        Ok(self.dir_node.full_path().join("history"))
    }

    /// Load history from disk if it exists.
    async fn load(&self, _ctx: &crate::AgentContext<'_>) -> Result<Vec<ChatMessage>> {
        // Return cached if available
        if let Some(ref cached) = *self.cache.borrow() {
            return Ok(cached.clone());
        }

        let path = self.get_path()?;

        if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
            let empty = vec![];
            *self.cache.borrow_mut() = Some(empty.clone());
            return Ok(empty);
        }

        let content = tokio::fs::read_to_string(&path).await.with_context(|| {
            format!("failed to read history from {}", path.display())
        })?;

        let messages: Vec<ChatMessage> = serde_yaml::from_str(&content).with_context(|| {
            format!("failed to parse history YAML from {}", path.display())
        })?;

        *self.cache.borrow_mut() = Some(messages.clone());
        Ok(messages)
    }

    /// Save history to disk.
    async fn save(&self, _ctx: &crate::AgentContext<'_>, messages: Vec<ChatMessage>) -> Result<()> {
        let path = self.get_path()?;

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }

        let yaml =
            serde_yaml::to_string(&messages).context("failed to serialize messages to YAML")?;

        tokio::fs::write(&path, yaml)
            .await
            .with_context(|| format!("failed to write history to {}", path.display()))?;

        // Update cache
        *self.cache.borrow_mut() = Some(messages);
        Ok(())
    }

    /// Perform compression if needed.
    async fn maybe_compress(
        &self,
        ctx: &crate::AgentContext<'_>,
        messages: &mut Vec<ChatMessage>,
    ) -> Result<()> {
        if !self.compression_strategy.should_compress(messages) {
            return Ok(());
        }

        let (compress_until, _keep_from) = self.compression_strategy.find_split_point(messages);

        if compress_until == 0 {
            return Ok(());
        }

        // Extract messages to compress
        let to_compress: Vec<ChatMessage> = messages.drain(0..compress_until).collect();

        // Setup archiver
        let archive_dir = self.get_history_archive_dir()?;
        let archiver = HistoryArchiver::new(archive_dir);

        // Generate archive filename and save
        let archive_filename = archiver.generate_filename();
        archiver
            .save(to_compress.clone(), &archive_filename)
            .await?;

        // Create compression summary using LLM
        let summary = self.compression_strategy.create_summary_message(
            ctx,
            &to_compress,
            &archive_filename,
            to_compress.len(),
            estimate_messages_tokens(&to_compress),
        ).await?;

        // Insert summary at the beginning
        messages.insert(0, summary);

        Ok(())
    }
}

#[async_trait(?Send)]
impl History for PersistentHistory {
    async fn get_all(&self, ctx: &crate::AgentContext<'_>) -> Result<Vec<ChatMessage>> {
        self.load(ctx).await
    }

    async fn append(&self, ctx: &crate::AgentContext<'_>, message: ChatMessage) -> Result<()> {
        let mut messages = self.load(ctx).await?;
        messages.push(message.clone());

        // Try compression if threshold exceeded
        self.maybe_compress(ctx, &mut messages).await?;

        // Implement sliding window: remove oldest messages if limit exceeded
        // Also ensure we don't start with Tool messages (invalid for OpenAI API)
        if messages.len() > self.max_size {
            let keep_from = messages.len() - self.max_size;
            messages.drain(0..keep_from);

            // Clean leading Tool messages
            CompressionStrategy::clean_leading_tool_messages(&mut messages);
        }

        // Save to disk
        self.save(ctx, messages).await?;

        Ok(())
    }

    async fn last(&self, ctx: &crate::AgentContext<'_>) -> Result<Option<ChatMessage>> {
        let messages = self.load(ctx).await?;
        Ok(messages.last().cloned())
    }

    async fn clear(&self, ctx: &crate::AgentContext<'_>) -> Result<()> {
        self.save(ctx, vec![]).await
    }

    async fn get_recent(&self, ctx: &crate::AgentContext<'_>, n: usize) -> Result<Vec<ChatMessage>> {
        let messages = self.load(ctx).await?;
        let start = messages.len().saturating_sub(n);
        Ok(messages[start..].to_vec())
    }
}
