use crate::data_store::DataStore;
use crate::llm::{LlmProvider, LlmSender, OpenAiProviderConfig};
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::{future::Future, pin::Pin, rc::Rc};

pub trait LocalSpawner {
    fn spawn_local(&self, fut: Pin<Box<dyn Future<Output = ()>>>);
}

pub struct Runtime {
    openai: Option<OpenAiProviderConfig>,
    llm_providers: Vec<Box<dyn LlmProvider>>,
    local_spawner: Option<Rc<dyn LocalSpawner>>,
    data_store: Option<DataStore>,
}

impl Runtime {
    pub fn local_spawner(&self) -> Option<Rc<dyn LocalSpawner>> {
        self.local_spawner.as_ref().map(Rc::clone)
    }

    pub fn data_store(&self) -> Option<&DataStore> {
        self.data_store.as_ref()
    }

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

        let model = ctx.session().default_model();
        tracing::info!("[LlmAgent] Starting execution with model: {}", model);
        let mut iteration = 0;

        loop {
            iteration += 1;
            tracing::debug!("[LlmAgent] Iteration {} - using model: {}", iteration, model);

            let mut sender = self.create_sender(model)?;

            let tools = ctx.tools();
            tracing::debug!("[LlmAgent] Sending request to LLM with {} tools", tools.len());
            let reply = sender.send(&messages, tools.as_slice()).await?;

            if reply.role != ChatRole::Assistant {
                bail!("tool_loop: reply role is not assistant");
            }

            // Append assistant reply to history (don't ignore errors)
            if let Err(e) = ctx.history().append(ctx, reply.clone()).await {
                tracing::warn!("[Runtime] Failed to append assistant reply to history: {}", e);
            }
            messages.push(reply.clone());

            match reply.content {
                ChatContent::Text(ref text) => {
                    tracing::info!("[LlmAgent] Received text response (length: {} chars)", text.len());
                    tracing::debug!("[LlmAgent] Response: {}", text.chars().take(200).collect::<String>());
                    tracing::info!("[LlmAgent] Execution completed");
                    return Ok(());
                }
                ChatContent::ToolCalls(tool_calls) => {
                    let calls = crate::parse_tool_calls(&tool_calls)?;
                    if calls.is_empty() {
                        bail!("tool_loop: empty tool_calls");
                    }

                    tracing::info!("[LlmAgent] Received {} tool call(s)", calls.len());

                    for c in calls {
                        tracing::info!("[LlmAgent] Executing tool: {} with args: {}",
                            c.function_name,
                            serde_json::to_string(&c.arguments).unwrap_or_else(|_| format!("{:?}", c.arguments))
                        );

                        let tools = ctx.tools();
                        let tool =
                            crate::find_tool_for_function(tools.as_slice(), &c.function_name)
                                .with_context(|| {
                                    format!("tool_loop: no tool for function: {}", c.function_name)
                                })?;

                        let result = match tool.invoke(ctx, &c.function_name, &c.arguments).await {
                            Ok(v) => {
                                tracing::info!("[LlmAgent] Tool '{}' succeeded (output length: {} chars)",
                                    c.function_name, v.len());
                                crate::agent::maybe_spool_tool_output(ctx, &c.function_name, v)
                                    .await?
                            }
                            Err(e) => {
                                tracing::warn!("[LlmAgent] Tool '{}' failed: {}", c.function_name, e);
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
                        // Append tool result to history (don't ignore errors)
                        if let Err(e) = ctx.history().append(ctx, tool_result.clone()).await {
                            tracing::warn!("[Runtime] Failed to append tool result to history: {}", e);
                        }
                        messages.push(tool_result);
                    }

                    tracing::debug!("[LlmAgent] Continuing to next iteration");
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
    local_spawner: Option<Rc<dyn LocalSpawner>>,
    data_store_root: Option<PathBuf>,
}

impl RuntimeBuilder {
    pub fn new() -> Self {
        Self {
            openai: None,
            llm_providers: vec![],
            local_spawner: None,
            data_store_root: None,
        }
    }

    pub fn set_local_spawner(mut self, spawner: Rc<dyn LocalSpawner>) -> Self {
        self.local_spawner = Some(spawner);
        self
    }

    pub fn set_openai(mut self, cfg: OpenAiProviderConfig) -> Self {
        self.openai = Some(cfg);
        self
    }

    pub fn add_llm_provider(mut self, provider: Box<dyn LlmProvider>) -> Self {
        self.llm_providers.push(provider);
        self
    }

    pub fn set_data_store_root(mut self, root: PathBuf) -> Self {
        self.data_store_root = Some(root);
        self
    }

    pub fn build(self) -> Runtime {
        let data_store = self.data_store_root.map(DataStore::new);
        Runtime {
            openai: self.openai,
            llm_providers: self.llm_providers,
            local_spawner: self.local_spawner,
            data_store,
        }
    }
}

impl Default for RuntimeBuilder {
    fn default() -> Self {
        Self::new()
    }
}
