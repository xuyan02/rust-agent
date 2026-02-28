use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::fs;

/// KnowledgeBase manages hierarchical markdown files storing extracted knowledge.
/// Structure is entirely managed by IntrospectionBrain via file paths.
pub struct KnowledgeBase {
    root: PathBuf,
}

impl KnowledgeBase {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// List all markdown files recursively, returning paths relative to root
    pub async fn list_files(&self) -> Result<Vec<String>> {
        let mut files = Vec::new();
        self.walk_dir(&self.root, &mut files).await?;
        Ok(files)
    }

    fn walk_dir<'a>(
        &'a self,
        dir: &'a Path,
        files: &'a mut Vec<String>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + 'a>> {
        Box::pin(async move {
            if !dir.exists() {
                return Ok(());
            }

            let mut entries = fs::read_dir(dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.is_dir() {
                    self.walk_dir(&path, files).await?;
                } else if path.extension().and_then(|s| s.to_str()) == Some("md") {
                    let rel_path = path
                        .strip_prefix(&self.root)
                        .context("failed to get relative path")?;
                    files.push(rel_path.to_string_lossy().to_string());
                }
            }
            Ok(())
        })
    }

    /// Read a knowledge file
    pub async fn read_file(&self, relative_path: &str) -> Result<String> {
        let full_path = self.root.join(relative_path);
        fs::read_to_string(&full_path)
            .await
            .with_context(|| format!("failed to read knowledge file: {}", relative_path))
    }

    /// Write a knowledge file (creates parent directories if needed)
    pub async fn write_file(&self, relative_path: &str, content: &str) -> Result<()> {
        let full_path = self.root.join(relative_path);

        // Create parent directory if needed
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::write(&full_path, content)
            .await
            .with_context(|| format!("failed to write knowledge file: {}", relative_path))
    }

    /// Move/rename a knowledge file
    pub async fn move_file(&self, from: &str, to: &str) -> Result<()> {
        let from_path = self.root.join(from);
        let to_path = self.root.join(to);

        // Create parent directory if needed
        if let Some(parent) = to_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::rename(&from_path, &to_path)
            .await
            .with_context(|| format!("failed to move knowledge file from {} to {}", from, to))
    }

    /// Delete a knowledge file
    pub async fn delete_file(&self, relative_path: &str) -> Result<()> {
        let full_path = self.root.join(relative_path);
        fs::remove_file(&full_path)
            .await
            .with_context(|| format!("failed to delete knowledge file: {}", relative_path))
    }

    /// List all directories recursively
    pub async fn list_dirs(&self) -> Result<Vec<String>> {
        let mut dirs = Vec::new();
        self.walk_dirs(&self.root, &mut dirs).await?;
        Ok(dirs)
    }

    fn walk_dirs<'a>(
        &'a self,
        dir: &'a Path,
        dirs: &'a mut Vec<String>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + 'a>> {
        Box::pin(async move {
            if !dir.exists() {
                return Ok(());
            }

            let mut entries = fs::read_dir(dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.is_dir() {
                    let rel_path = path
                        .strip_prefix(&self.root)
                        .context("failed to get relative path")?;
                    dirs.push(rel_path.to_string_lossy().to_string());
                    self.walk_dirs(&path, dirs).await?;
                }
            }
            Ok(())
        })
    }
}
