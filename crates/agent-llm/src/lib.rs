mod context;

pub use context::{LlmContext, LlmProvider, LlmRequest};

mod openai;
mod openai_stream;

pub use openai::{build_chat_completions_body, parse_chat_completions_response};
pub use openai_stream::{OpenAiStreamAccumulator, OpenAiStreamDelta, ToolCallDelta};

use agent_http::{HttpClient, HttpRequest};
use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use bytes::Bytes;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChatContent {
    Text(String),
    ToolCalls(Value),
    ToolResult { tool_call_id: String, result: Value },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: ChatContent,
}

impl ChatMessage {
    pub fn system_text(s: impl Into<String>) -> Self {
        Self {
            role: ChatRole::System,
            content: ChatContent::Text(s.into()),
        }
    }

    pub fn user_text(s: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: ChatContent::Text(s.into()),
        }
    }

    pub fn assistant_text(s: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: ChatContent::Text(s.into()),
        }
    }

    pub fn assistant_tool_calls(v: Value) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: ChatContent::ToolCalls(v),
        }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, result: Value) -> Self {
        Self {
            role: ChatRole::Tool,
            content: ChatContent::ToolResult {
                tool_call_id: tool_call_id.into(),
                result,
            },
        }
    }
}

#[async_trait(?Send)]
pub trait LlmSender: Send {
    async fn send(&mut self, messages: &[ChatMessage]) -> Result<ChatMessage>;
}

#[derive(Debug, Clone)]
pub struct OpenAiSender {
    base_url: String,
    api_key: String,
    model: String,
    http: HttpClient,
}

impl OpenAiSender {
    pub fn new(base_url: String, api_key: String, model: String) -> Result<Self> {
        Ok(Self {
            base_url,
            api_key,
            model,
            http: HttpClient::new()?,
        })
    }
}

#[async_trait(?Send)]
impl LlmSender for OpenAiSender {
    async fn send(&mut self, messages: &[ChatMessage]) -> Result<ChatMessage> {
        let debug_llm = std::env::var("AGENT_DEBUG_LLM")
            .ok()
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false);

        if debug_llm {
            eprintln!("[LLM][request] provider=openai model={}", self.model);
            for (i, m) in messages.iter().enumerate() {
                eprintln!("[LLM][request][{}] {:?}: {:?}", i, m.role, m.content);
            }
        }

        let url = format!(
            "{}/v1/chat/completions",
            self.base_url.trim_end_matches('/')
        );

        let tools: Vec<Value> = Vec::new();
        let body = crate::build_chat_completions_body(&self.model, messages, &tools)?;

        let req = HttpRequest {
            method: "POST".to_string(),
            url,
            headers: vec![
                (
                    "Authorization".to_string(),
                    format!("Bearer {}", self.api_key),
                ),
                ("Content-Type".to_string(), "application/json".to_string()),
            ],
            body: Bytes::from(agent_json::dump(&body)?),
        };

        let resp = self.http.send(req).await?;
        if resp.status < 200 || resp.status >= 300 {
            bail!("openai: http status={}", resp.status);
        }

        let v = agent_json::parse(
            std::str::from_utf8(&resp.body).context("openai: response is not utf-8")?,
        )
        .context("openai: failed to parse response JSON")?;
        let reply = crate::parse_chat_completions_response(&v)?;

        if debug_llm {
            eprintln!("[LLM][response] provider=openai model={}", self.model);
            eprintln!("[LLM][response] {:?}: {:?}", reply.role, reply.content);
        }

        Ok(reply)
    }
}

#[derive(Debug, Clone)]
pub struct OpenAiProviderConfig {
    pub base_url: String,
    pub api_key: String,
}

pub fn create_openai_sender(cfg: OpenAiProviderConfig, model: String) -> Result<OpenAiSender> {
    OpenAiSender::new(cfg.base_url, cfg.api_key, model)
}
