use crate::llm::{ChatContent, ChatMessage, ChatRole};
use crate::{Agent, AgentContext, AgentContextBuilder, LlmAgent, StaticSystemPromptSegment};
use anyhow::{bail, Context as _, Result};
use async_trait::async_trait;

/// ReAct (Reasoning and Acting) Agent
///
/// Implements the ReAct framework with two phases:
/// 1. Think: Analyze situation and decide next step using prefix markers
/// 2. Act: Execute action (use tools)
///
/// Think phase outputs:
/// - [think] - Continue thinking in next iteration
/// - [act] - Proceed to Act phase
/// - [answer] - Provide final answer and terminate
///
/// The agent runs until Think outputs [answer] or an error occurs.
pub struct ReActAgent {
    enable_logging: bool,
}

impl ReActAgent {
    pub fn new() -> Self {
        Self {
            enable_logging: true,
        }
    }

    pub fn with_logging(mut self, enable: bool) -> Self {
        self.enable_logging = enable;
        self
    }

    fn log(&self, msg: &str) {
        if self.enable_logging {
            tracing::debug!("[ReAct] {}", msg);
        }
    }
}

impl Default for ReActAgent {
    fn default() -> Self {
        Self::new()
    }
}

/// Decision from Think phase based on prefix markers
#[derive(Debug)]
enum ThinkDecision {
    /// [think] - Continue thinking
    ContinueThinking { thought: String },
    /// [act] - Ready to act
    ReadyToAct { thought: String },
    /// [answer] - Final answer
    FinalAnswer { answer: String },
}

#[async_trait(?Send)]
impl Agent for ReActAgent {
    async fn run(&self, ctx: &AgentContext<'_>) -> Result<()> {
        // Wrap entire execution in error handler to clear history on any error
        match self.run_impl(ctx).await {
            Ok(()) => Ok(()),
            Err(e) => {
                // On any error, clear history to prevent getting stuck in a bad state
                tracing::error!("[ReAct] Error occurred: {}. Clearing history.", e);
                if let Err(clear_err) = ctx.history().clear(ctx).await {
                    tracing::error!("[ReAct] Failed to clear history: {}", clear_err);
                }
                Err(e)
            }
        }
    }
}

impl ReActAgent {
    async fn run_impl(&self, ctx: &AgentContext<'_>) -> Result<()> {
        let mut iteration = 0;

        loop {
            iteration += 1;
            self.log(&format!("========== Iteration {} ==========", iteration));
            tracing::info!("[ReAct] Iteration {}", iteration);

            // === Phase 1: Think ===
            self.log("--- THINK Phase ---");
            tracing::info!("[ReAct] Starting THINK phase");
            let think_result = self.run_think_phase(ctx).await?;
            tracing::info!("[ReAct] THINK phase completed");

            match think_result {
                ThinkDecision::ContinueThinking { thought } => {
                    self.log(&format!("Decision: [think]\n{}", thought));

                    // Note: assistant's thought is already appended to history by runtime.execute()
                    // We only need to add the user prompt to continue

                    // Add a user message to continue the conversation
                    // (required because some models don't support ending with assistant message)
                    ctx.history()
                        .append(ctx, ChatMessage::user_text("Continue thinking."))
                        .await?;

                    // Continue to next Think phase
                    continue;
                }
                ThinkDecision::ReadyToAct { thought } => {
                    self.log(&format!("Decision: [act]\n{}", thought));

                    // Note: assistant's thought is already appended to history by runtime.execute()
                    // We only need to add the user prompt before Act phase

                    // Add a user message before Act phase
                    // (required because some models don't support ending with assistant message)
                    ctx.history()
                        .append(ctx, ChatMessage::user_text("Proceed with the action."))
                        .await?;

                    // Proceed to Act phase
                }
                ThinkDecision::FinalAnswer { answer } => {
                    self.log(&format!("Decision: [answer]\n{}", answer));

                    // Note: assistant's final answer is already appended to history by runtime.execute()

                    self.log("========== Task Completed ==========");
                    return Ok(());
                }
            }

            // === Phase 2: Act ===
            self.log("--- ACT Phase ---");
            tracing::info!("[ReAct] Starting ACT phase");
            let observation = self.run_act_phase(ctx).await?;
            tracing::info!("[ReAct] ACT phase completed");

            self.log(&format!("Observation:\n{}", observation));

            // Append observation to history
            ctx.history()
                .append(ctx, ChatMessage::user_text(format!("Observation: {}", observation)))
                .await?;

            // Continue to next iteration (Think phase)
        }
    }
}

impl ReActAgent {
    /// Run Think phase: analyze situation and decide next step
    async fn run_think_phase(&self, ctx: &AgentContext<'_>) -> Result<ThinkDecision> {
        tracing::info!("[ReAct::Think] Building prompt");
        let think_prompt = self.build_think_prompt();

        tracing::info!("[ReAct::Think] Building context");
        let think_ctx = AgentContextBuilder::from_parent_ctx(ctx)
            .add_system_prompt_segment(Box::new(StaticSystemPromptSegment::new(think_prompt)))
            .disable_tools()  // Think phase should not use tools
            .build()?;

        // Run LLM without tools (pure reasoning)
        tracing::info!("[ReAct::Think] Calling LLM");
        let think_agent = LlmAgent::new();
        think_agent.run(&think_ctx).await?;
        tracing::info!("[ReAct::Think] LLM call completed");

        // Extract thought
        tracing::info!("[ReAct::Think] Extracting output");
        let output = self.extract_last_assistant_text(&think_ctx).await?;

        // Log the complete think output
        self.log(&format!("Think output:\n{}", output));

        // Parse prefix to determine decision
        tracing::info!("[ReAct::Think] Parsing decision");
        self.parse_think_decision(&output)
    }

    fn build_think_prompt(&self) -> String {
        include_str!("../prompts/react_think.md").to_string()
    }

    fn parse_think_decision(&self, output: &str) -> Result<ThinkDecision> {
        let trimmed = output.trim();

        // Special case: empty output indicates LLM failure, need to clear history
        if trimmed.is_empty() {
            bail!("Think phase returned empty output (likely due to unstable LLM response). History will be cleared.");
        }

        // Check which marker appears at the start
        let starts_with_think = trimmed.starts_with("[think]");
        let starts_with_act = trimmed.starts_with("[act]");
        let starts_with_answer = trimmed.starts_with("[answer]");

        // Validate: exactly one marker at the start
        let marker_count = [starts_with_think, starts_with_act, starts_with_answer]
            .iter()
            .filter(|&&x| x)
            .count();

        if marker_count == 0 {
            bail!(
                "Think phase output missing marker (likely due to unstable LLM response). History will be cleared. Got: {}",
                trimmed.chars().take(100).collect::<String>()
            );
        }

        if marker_count > 1 {
            bail!(
                "Think phase output starts with multiple markers (should not happen). Got: {}",
                trimmed.chars().take(100).collect::<String>()
            );
        }

        // Parse the single marker
        if let Some(content) = trimmed.strip_prefix("[think]") {
            // Validate: check if other markers appear at the START of lines in content
            // (Allow mentioning markers in discussion, but forbid actual decision markers)
            let content_lines: Vec<&str> = content.lines().collect();
            for line in &content_lines {
                let line_trimmed = line.trim_start();
                if line_trimmed.starts_with("[act]") || line_trimmed.starts_with("[answer]") {
                    bail!(
                        "Think phase output contains multiple decision markers. Use ONLY ONE marker. Got: {}",
                        trimmed.chars().take(200).collect::<String>()
                    );
                }
            }
            return Ok(ThinkDecision::ContinueThinking {
                thought: format!("[think]{}", content),
            });
        }

        if let Some(content) = trimmed.strip_prefix("[act]") {
            // Validate: check if other markers appear at the START of lines in content
            let content_lines: Vec<&str> = content.lines().collect();
            for line in &content_lines {
                let line_trimmed = line.trim_start();
                if line_trimmed.starts_with("[think]") || line_trimmed.starts_with("[answer]") {
                    bail!(
                        "Think phase output contains multiple decision markers. Use ONLY ONE marker. Got: {}",
                        trimmed.chars().take(200).collect::<String>()
                    );
                }
            }
            return Ok(ThinkDecision::ReadyToAct {
                thought: format!("[act]{}", content),
            });
        }

        if let Some(content) = trimmed.strip_prefix("[answer]") {
            // Validate: check if other markers appear at the START of lines in content
            let content_lines: Vec<&str> = content.lines().collect();
            for line in &content_lines {
                let line_trimmed = line.trim_start();
                if line_trimmed.starts_with("[think]") || line_trimmed.starts_with("[act]") {
                    bail!(
                        "Think phase output contains multiple decision markers. Use ONLY ONE marker. Got: {}",
                        trimmed.chars().take(200).collect::<String>()
                    );
                }
            }
            return Ok(ThinkDecision::FinalAnswer {
                answer: content.trim().to_string(),
            });
        }

        bail!(
            "Think phase output must START with [think], [act], or [answer]. Got: {}",
            trimmed.chars().take(50).collect::<String>()
        );
    }

    /// Run Act phase: execute action using tools
    ///
    /// Act phase uses LlmAgent which allows the LLM to call multiple tools
    /// in sequence if needed. The Act phase completes when LLM returns text
    /// (not tool calls), at which point we return the text as observation.
    async fn run_act_phase(&self, ctx: &AgentContext<'_>) -> Result<String> {
        tracing::info!("[ReAct::Act] Building prompt");
        let act_prompt = self.build_act_prompt();

        tracing::info!("[ReAct::Act] Building context");
        let act_ctx = AgentContextBuilder::from_parent_ctx(ctx)
            .add_system_prompt_segment(Box::new(StaticSystemPromptSegment::new(act_prompt)))
            .build()?;

        // Run LLM with tools - allows multiple tool calls
        tracing::info!("[ReAct::Act] Calling LLM (with tools enabled)");
        let act_agent = LlmAgent::new();
        act_agent.run(&act_ctx).await?;
        tracing::info!("[ReAct::Act] LLM call completed");

        // Extract the final text output as observation
        tracing::info!("[ReAct::Act] Extracting output");
        let observation = self.extract_last_assistant_text(&act_ctx).await?;

        self.log(&format!("Act output:\n{}", observation));

        Ok(observation)
    }

    fn build_act_prompt(&self) -> String {
        include_str!("../prompts/react_act.md").to_string()
    }

    async fn extract_last_assistant_text(&self, ctx: &AgentContext<'_>) -> Result<String> {
        let messages = ctx.history().get_all(ctx).await?;

        let last_msg = messages
            .iter()
            .rev()
            .find(|m| matches!(m.role, ChatRole::Assistant))
            .context("No assistant message found")?;

        match &last_msg.content {
            ChatContent::Text(text) => Ok(text.clone()),
            _ => bail!("Expected text content"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_think_decision_single_marker() {
        let agent = ReActAgent::new();

        // Valid: single [think]
        let result = agent.parse_think_decision("[think] Let me analyze this problem...");
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ThinkDecision::ContinueThinking { .. }));

        // Valid: single [act]
        let result = agent.parse_think_decision("[act] I will use file-glob tool.");
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ThinkDecision::ReadyToAct { .. }));

        // Valid: single [answer]
        let result = agent.parse_think_decision("[answer] The answer is 42.");
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ThinkDecision::FinalAnswer { .. }));
    }

    #[test]
    fn test_parse_think_decision_marker_in_content_ok() {
        let agent = ReActAgent::new();

        // Valid: marker at start, mentions other markers in discussion (not at line start)
        let result = agent.parse_think_decision("[think] I'm considering using [act] later.");
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ThinkDecision::ContinueThinking { .. }));

        // Valid: marker at start, tool call JSON may contain marker strings
        let result = agent.parse_think_decision("[act] I will use tool.\n<tool_call>{\"desc\": \"[act] marker\"}");
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ThinkDecision::ReadyToAct { .. }));
    }

    #[test]
    fn test_parse_think_decision_rejects_multiple_markers() {
        let agent = ReActAgent::new();

        // Invalid: marker at start, followed by another marker at line start
        let result = agent.parse_think_decision("[think] Let me analyze.\n[act] I will use tools.");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("multiple"));

        // Invalid: consecutive markers
        let result = agent.parse_think_decision("[think] Thinking...\n\n[answer] Here's the answer.");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("multiple"));
    }

    #[test]
    fn test_parse_think_decision_no_marker_error() {
        let agent = ReActAgent::new();

        // No marker: should error (will trigger history clear)
        let result = agent.parse_think_decision("I forgot to add a marker.");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("missing marker") || err_msg.contains("History will be cleared"));
    }

    #[test]
    fn test_parse_think_decision_marker_not_at_start_error() {
        let agent = ReActAgent::new();

        // Marker not at start: should error (will trigger history clear)
        let result = agent.parse_think_decision("Let me think... [think] Now analyzing.");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("missing marker") || err_msg.contains("History will be cleared"));
    }

    #[test]
    fn test_parse_think_decision_empty_output_error() {
        let agent = ReActAgent::new();

        // Empty output: should error (will trigger history clear)
        let result = agent.parse_think_decision("");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("empty") || err_msg.contains("History will be cleared"));
    }
}
