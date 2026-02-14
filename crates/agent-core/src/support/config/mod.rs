use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct OpenAiProviderConfig {
    pub base_url: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct AgentConfig {
    pub model: String,
    pub openai: Option<OpenAiProviderConfig>,
}

pub async fn load_agent_config_yaml_async(path: impl AsRef<Path>) -> Result<AgentConfig> {
    let path = path.as_ref();
    let bytes = tokio::fs::read(path)
        .await
        .with_context(|| format!("failed to read config: {}", path.display()))?;
    let cfg: AgentConfig = serde_yaml::from_slice(&bytes)
        .with_context(|| format!("failed to parse yaml: {}", path.display()))?;
    Ok(cfg)
}

pub fn load_agent_config_yaml(path: impl AsRef<Path>) -> Result<AgentConfig> {
    // Sync helper (tests/one-off); prefer async in main code paths.
    let path = path.as_ref();
    let bytes = std::fs::read(path)
        .with_context(|| format!("failed to read config: {}", path.display()))?;
    let cfg: AgentConfig = serde_yaml::from_slice(&bytes)
        .with_context(|| format!("failed to parse yaml: {}", path.display()))?;
    Ok(cfg)
}
