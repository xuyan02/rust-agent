use crate::llm::{LlmProvider, LlmSender, OpenAiProviderConfig};
use anyhow::{Context, Result};

pub struct Runtime {
    openai: Option<OpenAiProviderConfig>,
    llm_providers: Vec<Box<dyn LlmProvider>>,
}

impl Runtime {
    pub async fn execute(
        &self,
        ctx: &crate::AgentContext<'_>,
        mut messages: Vec<crate::llm::ChatMessage>,
    ) -> Result<()> {
        use crate::llm::{ChatContent, ChatMessage, ChatRole};
        use anyhow::bail;

        if ctx.session().default_model().is_empty() {
            bail!("agent: missing default model");
        }

        let segs = ctx.system_prompt_segments();
        let mut sys_parts = Vec::with_capacity(segs.len());
        for s in segs {
            let text = s.render(ctx).await?;
            if !text.trim().is_empty() {
                sys_parts.push(text);
            }
        }
        let system_prompt = sys_parts.join("\n\n");

        // Prepend a single system message built from all registered segments.
        if !system_prompt.is_empty() {
            if matches!(messages.first().map(|m| m.role), Some(ChatRole::System)) {
                // Avoid duplicating system prompt if caller already injected system messages.
            } else {
                messages.insert(0, ChatMessage::system_text(system_prompt));
            }
        }

        loop {
            let mut sender = self.create_sender(ctx.session().default_model())?;

            let tools = ctx.tools();
            let reply = sender.send(&messages, tools.as_slice()).await?;

            if reply.role != ChatRole::Assistant {
                bail!("tool_loop: reply role is not assistant");
            }

            let _ = ctx.history().append(reply.clone()).await;
            messages.push(reply.clone());

            match reply.content {
                ChatContent::Text(_) => return Ok(()),
                ChatContent::ToolCalls(tool_calls) => {
                    let calls = crate::parse_tool_calls(&tool_calls)?;
                    if calls.is_empty() {
                        bail!("tool_loop: empty tool_calls");
                    }

                    for c in calls {
                        let tools = ctx.tools();
                        let tool =
                            crate::find_tool_for_function(tools.as_slice(), &c.function_name)
                                .with_context(|| {
                                    format!("tool_loop: no tool for function: {}", c.function_name)
                                })?;

                        let result = match tool.invoke(ctx, &c.function_name, &c.arguments).await {
                            Ok(v) => {
                                crate::agent::maybe_spool_tool_output(ctx, &c.function_name, v)
                                    .await?
                            }
                            Err(e) => {
                                let mut root = e.to_string();
                                let mut cur = e.source();
                                while let Some(s) = cur {
                                    root = s.to_string();
                                    cur = s.source();
                                }
                                format!(
                                    "tool error\nfunction: {}\nmessage: {}",
                                    c.function_name, root
                                )
                            }
                        };

                        let tool_result = ChatMessage::tool_result(c.id, result);
                        let _ = ctx.history().append(tool_result.clone()).await;
                        messages.push(tool_result);
                    }

                    continue;
                }
                _ => bail!("tool_loop: unexpected assistant message"),
            }
        }
    }

    pub fn create_sender(&self, model: &str) -> Result<Box<dyn LlmSender>> {
        for p in &self.llm_providers {
            if p.supports_model(model) {
                return p.create_sender(model);
            }
        }

        let openai = self
            .openai
            .clone()
            .context("missing openai provider config")?;
        let sender = crate::llm::create_openai_sender(openai, model.to_string())?;
        Ok(Box::new(sender))
    }
}

pub struct RuntimeBuilder {
    openai: Option<OpenAiProviderConfig>,
    llm_providers: Vec<Box<dyn LlmProvider>>,
}

impl RuntimeBuilder {
    pub fn new() -> Self {
        Self {
            openai: None,
            llm_providers: vec![],
        }
    }

    pub fn set_openai(mut self, cfg: OpenAiProviderConfig) -> Self {
        self.openai = Some(cfg);
        self
    }

    pub fn add_llm_provider(mut self, provider: Box<dyn LlmProvider>) -> Self {
        self.llm_providers.push(provider);
        self
    }

    pub fn build(self) -> Runtime {
        Runtime {
            openai: self.openai,
            llm_providers: self.llm_providers,
        }
    }
}

impl Default for RuntimeBuilder {
    fn default() -> Self {
        Self::new()
    }
}
