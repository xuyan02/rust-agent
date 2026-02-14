mod context;
mod json;
mod openai;
mod openai_stream;
mod tools_json;

pub use context::{LlmContext, LlmProvider, LlmRequest};
pub use openai::{build_chat_completions_body, parse_chat_completions_response};
pub use openai_stream::{OpenAiStreamAccumulator, OpenAiStreamDelta, ToolCallDelta};
pub use tools_json::tools_to_openai_json;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use bytes::Bytes;
use reqwest::header::{HeaderMap, HeaderValue};
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
    ToolResult {
        tool_call_id: String,
        result: String,
    },
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

    pub fn tool_result(tool_call_id: impl Into<String>, result: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Tool,
            content: ChatContent::ToolResult {
                tool_call_id: tool_call_id.into(),
                result: result.into(),
            },
        }
    }
}

#[async_trait(?Send)]
pub trait LlmSender: Send {
    async fn send(
        &mut self,
        messages: &[ChatMessage],
        tools: &[&dyn crate::tools::Tool],
    ) -> Result<ChatMessage>;
}

#[derive(Debug, Clone)]
pub struct OpenAiSender {
    base_url: String,
    api_key: String,
    model: String,
    model_provider_id: Option<String>,
    http: reqwest::Client,
}

impl OpenAiSender {
    pub fn new(
        base_url: String,
        api_key: String,
        model: String,
        model_provider_id: Option<String>,
    ) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent("agent/0.1")
            .build()
            .with_context(|| "failed to build http client")?;

        Ok(Self {
            base_url,
            api_key,
            model,
            model_provider_id,
            http,
        })
    }
}

#[async_trait(?Send)]
impl LlmSender for OpenAiSender {
    async fn send(
        &mut self,
        messages: &[ChatMessage],
        tools: &[&dyn crate::tools::Tool],
    ) -> Result<ChatMessage> {
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

        let tools_json = crate::llm::tools_to_openai_json(tools);
        let body = crate::llm::build_chat_completions_body(&self.model, messages, &tools_json)?;

        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.api_key))?,
        );
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );

        if let Some(v) = &self.model_provider_id {
            headers.insert("X-Model-Provider-Id", HeaderValue::from_str(v)?);
        }

        let resp = self
            .http
            .post(url)
            .headers(headers)
            .body(Bytes::from(crate::llm::json::dump(&body)?))
            .send()
            .await
            .with_context(|| "openai: http request failed")?;

        let status = resp.status().as_u16();
        let body = resp
            .bytes()
            .await
            .with_context(|| "openai: failed to read response body")?;

        if !(200..300).contains(&status) {
            let text = std::str::from_utf8(&body)
                .unwrap_or("<non-utf8 response body>")
                .to_string();
            bail!("openai: http status={} body={}", status, text);
        }

        let v = crate::llm::json::parse(
            std::str::from_utf8(&body).context("openai: response is not utf-8")?,
        )
        .context("openai: failed to parse response JSON")?;
        let reply = crate::llm::parse_chat_completions_response(&v)?;

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
    pub model_provider_id: Option<String>,
}

pub fn create_openai_sender(cfg: OpenAiProviderConfig, model: String) -> Result<OpenAiSender> {
    OpenAiSender::new(cfg.base_url, cfg.api_key, model, cfg.model_provider_id)
}
