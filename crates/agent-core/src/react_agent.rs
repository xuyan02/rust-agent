use crate::llm::{ChatContent, ChatRole};
use crate::{Agent, AgentContext, AgentContextBuilder, LlmAgent, StaticSystemPromptSegment};
use anyhow::{bail, Context as _, Result};
use async_trait::async_trait;

/// ReAct (Reasoning and Acting) Agent
///
/// Implements the ReAct framework with reasoning and action capabilities:
/// - Think phase has access to all tools, including the special `act` tool
/// - The `act` tool creates an isolated execution context with independent history
/// - Think phase can reason with markers: [think] or [answer]
///
/// Think phase outputs:
/// - [think] - Continue thinking/reasoning in next iteration
/// - [answer] - Provide final answer and terminate
/// - Can also call the `act` tool for execution with isolated history
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

/// Decision from Think phase
#[derive(Debug)]
enum ThinkDecision {
    /// No [answer] marker - Continue thinking
    ContinueThinking,
    /// [answer] marker found - Final answer
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
                ThinkDecision::ContinueThinking => {
                    self.log("Decision: Continue thinking");

                    // Note: assistant's thought is already appended to history by runtime.execute()
                    // Continue to next Think phase
                    continue;
                }
                ThinkDecision::FinalAnswer { answer } => {
                    self.log(&format!("Decision: [answer]\n{}", answer));

                    // Note: assistant's final answer is already appended to history by runtime.execute()

                    self.log("========== Task Completed ==========");
                    return Ok(());
                }
            }
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
            .add_tool(Box::new(crate::tools::ActTool::new()))
            .set_tool_whitelist(vec!["act".to_string()])
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

        // Find all [answer] markers that appear at the start of lines
        // If multiple [answer] markers found, use the LAST one
        let lines: Vec<&str> = trimmed.lines().collect();
        let mut last_answer_line_idx: Option<usize> = None;

        for (i, line) in lines.iter().enumerate() {
            let line_trimmed = line.trim_start();
            if line_trimmed.starts_with("[answer]") {
                last_answer_line_idx = Some(i);
            }
        }

        // If [answer] marker found, extract the answer content
        if let Some(answer_line_idx) = last_answer_line_idx {
            // Extract content from the [answer] line to the end
            let content_lines = &lines[answer_line_idx..];
            let full_content = content_lines.join("\n");

            if let Some(content) = full_content.trim_start().strip_prefix("[answer]") {
                Ok(ThinkDecision::FinalAnswer {
                    answer: content.trim().to_string(),
                })
            } else {
                bail!("Failed to parse [answer] marker")
            }
        } else {
            // No [answer] marker - continue thinking
            Ok(ThinkDecision::ContinueThinking)
        }
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
    fn test_parse_think_decision_basic() {
        let agent = ReActAgent::new();

        // No marker: continue thinking
        let result = agent.parse_think_decision("Let me analyze this problem...");
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ThinkDecision::ContinueThinking));

        // Valid: [answer] marker
        let result = agent.parse_think_decision("[answer] The answer is 42.");
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ThinkDecision::FinalAnswer { .. }));

        // Thought with tool calls but no answer: continue thinking
        let result = agent.parse_think_decision("I will call act tool to gather information.");
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ThinkDecision::ContinueThinking));
    }

    #[test]
    fn test_parse_think_decision_marker_in_content_ok() {
        let agent = ReActAgent::new();

        // Valid: content mentions [answer] but not at line start - continue thinking
        let result = agent.parse_think_decision("I will provide [answer] later after gathering info.");
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ThinkDecision::ContinueThinking));

        // Valid: [answer] at line start with JSON content
        let result = agent.parse_think_decision("[answer] The configuration is: {\"key\": \"value\"}");
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ThinkDecision::FinalAnswer { .. }));
    }

    #[test]
    fn test_parse_think_decision_uses_last_answer_marker() {
        let agent = ReActAgent::new();

        // Multiple [answer] markers: use the LAST one
        let result = agent.parse_think_decision("[answer] Wrong answer\n[answer] Correct answer");
        assert!(result.is_ok());
        if let Ok(ThinkDecision::FinalAnswer { answer }) = result {
            assert_eq!(answer, "Correct answer");
        } else {
            panic!("Expected FinalAnswer");
        }

        // Thinking with multiple [answer] markers
        let result = agent.parse_think_decision(
            "I'm thinking about this...\n[answer] First try\nWait, let me reconsider.\n[answer] Final answer"
        );
        assert!(result.is_ok());
        if let Ok(ThinkDecision::FinalAnswer { answer }) = result {
            assert_eq!(answer, "Final answer");
        } else {
            panic!("Expected FinalAnswer");
        }
    }

    #[test]
    fn test_parse_think_decision_no_answer_marker() {
        let agent = ReActAgent::new();

        // No [answer] marker: continue thinking
        let result = agent.parse_think_decision("I need to gather more information.");
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ThinkDecision::ContinueThinking));

        // Multi-line thought without [answer]: continue thinking
        let result = agent.parse_think_decision("First observation.\nSecond observation.\nNeed more data.");
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ThinkDecision::ContinueThinking));
    }

    #[test]
    fn test_parse_think_decision_answer_not_at_line_start() {
        let agent = ReActAgent::new();

        // [answer] not at line start: should be ignored, continue thinking
        let result = agent.parse_think_decision("Let me think... [answer] is what I need to provide.");
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ThinkDecision::ContinueThinking));

        // [answer] with leading spaces on same line: should still work
        let result = agent.parse_think_decision("   [answer] The result is 42.");
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ThinkDecision::FinalAnswer { .. }));
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
