use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct JsonFileStorage {
    root: PathBuf,
}

impl JsonFileStorage {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub async fn read<T: DeserializeOwned>(&self, rel: &str) -> Result<Option<T>> {
        let path = self.root.join(rel);
        if !path.exists() {
            return Ok(None);
        }
        let s = tokio::fs::read_to_string(&path)
            .await
            .with_context(|| format!("failed to read {}", path.display()))?;
        let v = serde_json::from_str(&s)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(Some(v))
    }

    pub async fn write<T: Serialize>(&self, rel: &str, v: &T) -> Result<()> {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("failed to create dir {}", parent.display()))?;
        }
        let s = serde_json::to_string_pretty(v).with_context(|| "failed to serialize json")?;
        tokio::fs::write(&path, s)
            .await
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }
}
