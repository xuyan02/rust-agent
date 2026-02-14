use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillDescriptor {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct SkillRegistry {
    skills_dir: PathBuf,
}

impl SkillRegistry {
    pub fn new(skills_dir: PathBuf) -> Self {
        Self { skills_dir }
    }

    pub fn skills_dir(&self) -> &Path {
        &self.skills_dir
    }

    pub async fn list(&self) -> Result<Vec<SkillDescriptor>> {
        let mut out = Vec::new();
        if !self.skills_dir.exists() {
            return Ok(out);
        }

        let mut rd = tokio::fs::read_dir(&self.skills_dir)
            .await
            .with_context(|| format!("failed to read dir {}", self.skills_dir.display()))?;

        while let Some(ent) = rd.next_entry().await? {
            let path = ent.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            if name.is_empty() {
                continue;
            }
            out.push(SkillDescriptor {
                name,
                description: String::new(),
            });
        }

        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    pub async fn load_markdown(&self, name: &str) -> Result<String> {
        let path = self.skills_dir.join(format!("{name}.md"));
        tokio::fs::read_to_string(&path)
            .await
            .with_context(|| format!("failed to read {}", path.display()))
    }

    pub fn execute(&self, _name: &str, _input: &Value) -> Result<Value> {
        anyhow::bail!("skill execution not implemented")
    }
}
