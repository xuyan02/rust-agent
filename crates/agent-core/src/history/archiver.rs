use crate::llm::ChatMessage;
use crate::Result;
use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use super::token_estimator::estimate_messages_tokens;

/// Metadata for archived conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivedHistory {
    /// ISO 8601 timestamp when compression occurred
    pub compressed_at: String,

    /// Number of messages archived
    pub message_count: usize,

    /// Estimated total tokens in archived messages
    pub estimated_tokens: usize,

    /// The archived messages
    pub messages: Vec<ChatMessage>,
}

/// Manages archiving of conversation history to disk.
pub struct HistoryArchiver {
    archive_dir: PathBuf,
}

impl HistoryArchiver {
    /// Create a new archiver for the given directory.
    pub fn new(archive_dir: PathBuf) -> Self {
        Self { archive_dir }
    }

    /// Generate a unique archive filename using timestamp.
    pub fn generate_filename(&self) -> String {
        use std::time::SystemTime;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
        let timestamp = now.as_secs();
        format!("{}.yaml", timestamp)
    }

    /// Save archived messages to a file.
    pub async fn save(
        &self,
        messages: Vec<ChatMessage>,
        filename: &str,
    ) -> Result<PathBuf> {
        // Ensure archive directory exists
        tokio::fs::create_dir_all(&self.archive_dir).await
            .with_context(|| format!(
                "failed to create archived directory: {}",
                self.archive_dir.display()
            ))?;

        let archive_path = self.archive_dir.join(filename);

        let archived = ArchivedHistory {
            compressed_at: chrono::Local::now().to_rfc3339(),
            message_count: messages.len(),
            estimated_tokens: estimate_messages_tokens(&messages),
            messages,
        };

        let yaml = serde_yaml::to_string(&archived)
            .context("failed to serialize archived history")?;

        tokio::fs::write(&archive_path, yaml).await
            .with_context(|| format!(
                "failed to write archive: {}",
                archive_path.display()
            ))?;

        eprintln!(
            "[HistoryArchiver] Archived {} messages to {}",
            archived.message_count,
            archive_path.display()
        );

        Ok(archive_path)
    }

    /// Load archived messages from a file.
    pub async fn load(&self, filename: &str) -> Result<ArchivedHistory> {
        let archive_path = self.archive_dir.join(filename);

        let content = tokio::fs::read_to_string(&archive_path).await
            .with_context(|| format!(
                "failed to read archive: {}",
                archive_path.display()
            ))?;

        let archived: ArchivedHistory = serde_yaml::from_str(&content)
            .with_context(|| format!(
                "failed to parse archived history from {}",
                archive_path.display()
            ))?;

        Ok(archived)
    }

    /// List all archive files in the directory.
    pub async fn list_archives(&self) -> Result<Vec<String>> {
        if !tokio::fs::try_exists(&self.archive_dir).await.unwrap_or(false) {
            return Ok(vec![]);
        }

        let mut entries = tokio::fs::read_dir(&self.archive_dir).await
            .context("failed to read archive directory")?;

        let mut archives = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".yaml") {
                    archives.push(name.to_string());
                }
            }
        }

        archives.sort();
        Ok(archives)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{ChatContent, ChatRole};

    #[tokio::test]
    async fn test_archiver() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let archive_dir = temp_dir.path().join("archived");

        let archiver = HistoryArchiver::new(archive_dir.clone());

        // Save some messages
        let messages = vec![
            ChatMessage {
                role: ChatRole::User,
                content: ChatContent::Text("Hello".to_string()),
            },
            ChatMessage {
                role: ChatRole::Assistant,
                content: ChatContent::Text("Hi!".to_string()),
            },
        ];

        let filename = archiver.generate_filename();
        let path = archiver.save(messages.clone(), &filename).await?;

        assert!(path.exists());

        // Load it back
        let loaded = archiver.load(&filename).await?;
        assert_eq!(loaded.message_count, 2);
        assert_eq!(loaded.messages.len(), 2);

        // List archives
        let archives = archiver.list_archives().await?;
        assert_eq!(archives.len(), 1);

        Ok(())
    }
}
