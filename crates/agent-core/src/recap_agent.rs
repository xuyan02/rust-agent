use crate::{Agent, AgentContext, ToolLoopOptions, run_tool_loop};
use agent_llm::{ChatMessage, ChatRole};
use anyhow::{Result, bail};
use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, Eq)]
enum ReCapLevel {
    /// A recap produced by compressing raw interaction history.
    History,
    /// A recap produced by compressing multiple prior recaps.
    Recap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReCapNode {
    level: ReCapLevel,
    text: String,
}

impl ReCapNode {
    fn as_system_message(&self) -> ChatMessage {
        // Keep as system so it reliably constrains/steers the next step.
        // This is ephemeral (not appended to History).
        ChatMessage::system_text(format!("ReCAP ({:?}):\n{}", self.level, self.text.trim()))
    }
}

pub struct ReCapAgent {
    nodes: Vec<ReCapNode>,
    debug: bool,
}

impl ReCapAgent {
    pub fn new() -> Self {
        let debug = std::env::var("AGENT_DEBUG_RECAP")
            .ok()
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false);
        Self {
            nodes: vec![],
            debug,
        }
    }

    fn debug_enabled(&self) -> bool {
        self.debug
    }

    async fn recap_once(&mut self, ctx: &AgentContext<'_>) -> Result<()> {
        // Build recap input using system segments + existing recaps + full history.
        let mut messages: Vec<ChatMessage> = ctx
            .system_segments()
            .iter()
            .map(|s| ChatMessage::system_text(s.clone()))
            .collect();

        // Provide previous recaps (if any) so the new recap is consistent.
        if !self.nodes.is_empty() {
            messages.push(ChatMessage::system_text(
                "Existing ReCAP memory (most recent last):".to_string(),
            ));
            for n in &self.nodes {
                messages.push(n.as_system_message());
            }
        }

        messages.push(ChatMessage::system_text(
            "You are producing a ReCAP memory update.\n\
Return ONLY plain text with the following sections:\n\
- Goals\n- Current plan / next actions\n- Key facts (including tool outputs worth keeping)\n- Decisions made / rationale (high level)\n- Open questions / uncertainties\n- Constraints / preferences\n\
Do NOT include secrets. Do NOT include tool call JSON.".to_string(),
        ));

        messages.push(ChatMessage::system_text(
            "Raw interaction history follows:".to_string(),
        ));
        messages.extend(ctx.history().get_all().await?);

        if self.debug_enabled() {
            let history_len = ctx.history().get_all().await.map(|v| v.len()).unwrap_or(0);
            eprintln!(
                "[ReCapAgent] recap: generating recap (history_messages={}, existing_recaps={})",
                history_len,
                self.nodes.len()
            );
        }

        let mut sender = ctx
            .session()
            .runtime()
            .create_sender(ctx.session().default_model())?;
        let reply = sender.send(&messages).await?;

        if reply.role != ChatRole::Assistant {
            bail!("recap: reply role is not assistant");
        }

        let text = match reply.content {
            agent_llm::ChatContent::Text(t) => t,
            agent_llm::ChatContent::ToolCalls(_) => bail!("recap: unexpected tool_calls"),
            agent_llm::ChatContent::ToolResult { .. } => bail!("recap: unexpected tool result"),
        };

        if self.debug_enabled() {
            eprintln!(
                "[ReCapAgent] recap: stored recap node (level={:?}, chars={})",
                ReCapLevel::History,
                text.len()
            );
        }

        self.nodes.push(ReCapNode {
            level: ReCapLevel::History,
            text,
        });

        Ok(())
    }

    async fn rollup_recaps_if_needed(&mut self, ctx: &AgentContext<'_>) -> Result<()> {
        // "Pure" ReCAP still needs to avoid unbounded recap growth.
        // We implement a simple recursive roll-up: when there are too many recap nodes,
        // compress the older half into a higher-level recap.
        const MAX_RECAP_NODES: usize = 8;
        if self.nodes.len() <= MAX_RECAP_NODES {
            return Ok(());
        }

        if self.debug_enabled() {
            eprintln!(
                "[ReCapAgent] recap_rollup: rolling up recaps (nodes={}, max={})",
                self.nodes.len(),
                MAX_RECAP_NODES
            );
        }

        let split_at = self.nodes.len() / 2;
        let older = self.nodes[..split_at].to_vec();
        let newer = self.nodes[split_at..].to_vec();

        let mut messages: Vec<ChatMessage> = ctx
            .system_segments()
            .iter()
            .map(|s| ChatMessage::system_text(s.clone()))
            .collect();

        messages.push(ChatMessage::system_text(
            "You are rolling up multiple ReCAP summaries into a higher-level ReCAP.\n\
Return ONLY plain text with the same sections as before, but preserve long-term goals, constraints, and decisions.".to_string(),
        ));

        for n in &older {
            messages.push(n.as_system_message());
        }

        let mut sender = ctx
            .session()
            .runtime()
            .create_sender(ctx.session().default_model())?;
        let reply = sender.send(&messages).await?;

        if reply.role != ChatRole::Assistant {
            bail!("recap_rollup: reply role is not assistant");
        }

        let text = match reply.content {
            agent_llm::ChatContent::Text(t) => t,
            agent_llm::ChatContent::ToolCalls(_) => bail!("recap_rollup: unexpected tool_calls"),
            agent_llm::ChatContent::ToolResult { .. } => {
                bail!("recap_rollup: unexpected tool result")
            }
        };

        if self.debug_enabled() {
            eprintln!(
                "[ReCapAgent] recap_rollup: produced rollup recap (chars={}), keeping newer_nodes={}",
                text.len(),
                newer.len()
            );
        }

        self.nodes = vec![ReCapNode {
            level: ReCapLevel::Recap,
            text,
        }];
        self.nodes.extend(newer);

        Ok(())
    }
}

impl Default for ReCapAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait(?Send)]
impl Agent for ReCapAgent {
    async fn run(&mut self, ctx: &AgentContext<'_>) -> Result<()> {
        // Always produce a recap update before acting.
        // This is the "pure" setting requested: no thresholds/K-window.
        self.recap_once(ctx).await?;
        self.rollup_recaps_if_needed(ctx).await?;

        // Inject recaps but do not persist them.
        let mut messages: Vec<ChatMessage> = ctx
            .system_segments()
            .iter()
            .map(|s| ChatMessage::system_text(s.clone()))
            .collect();
        for n in &self.nodes {
            messages.push(n.as_system_message());
        }
        messages.extend(ctx.history().get_all().await?);

        if self.debug_enabled() {
            eprintln!(
                "[ReCapAgent] act: injected_recaps={} total_prompt_messages={}",
                self.nodes.len(),
                messages.len()
            );
        }

        run_tool_loop(ctx, messages, ToolLoopOptions::default()).await
    }
}
