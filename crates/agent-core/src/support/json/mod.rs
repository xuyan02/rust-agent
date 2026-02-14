use anyhow::{Context, Result};

pub fn parse(s: &str) -> Result<serde_json::Value> {
    serde_json::from_str(s).with_context(|| "failed to parse json")
}

pub fn dump(v: &serde_json::Value) -> Result<String> {
    serde_json::to_string(v).with_context(|| "failed to serialize json")
}
