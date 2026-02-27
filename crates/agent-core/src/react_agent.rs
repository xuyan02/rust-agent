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
            tracing::info!("[ReAct] {}", msg);
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
        let mut iteration = 0;

        loop {
            iteration += 1;
            self.log(&format!("========== Iteration {} ==========", iteration));

            // === Phase 1: Think ===
            self.log("--- THINK Phase ---");
            let think_result = self.run_think_phase(ctx).await?;

            match think_result {
                ThinkDecision::ContinueThinking { thought } => {
                    self.log(&format!("Decision: [think]\n{}", thought));

                    // Append to history
                    ctx.history()
                        .append(ctx, ChatMessage::assistant_text(thought))
                        .await?;

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

                    // Append to history
                    ctx.history()
                        .append(ctx, ChatMessage::assistant_text(thought))
                        .await?;

                    // Add a user message before Act phase
                    // (required because some models don't support ending with assistant message)
                    ctx.history()
                        .append(ctx, ChatMessage::user_text("Proceed with the action."))
                        .await?;

                    // Proceed to Act phase
                }
                ThinkDecision::FinalAnswer { answer } => {
                    self.log(&format!("Decision: [answer]\n{}", answer));

                    // Append final answer
                    ctx.history()
                        .append(ctx, ChatMessage::assistant_text(answer))
                        .await?;

                    self.log("========== Task Completed ==========");
                    return Ok(());
                }
            }

            // === Phase 2: Act ===
            self.log("--- ACT Phase ---");
            let observation = self.run_act_phase(ctx).await?;

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
        let think_prompt = self.build_think_prompt();

        let think_ctx = AgentContextBuilder::from_parent_ctx(ctx)
            .add_system_prompt_segment(Box::new(StaticSystemPromptSegment::new(think_prompt)))
            .disable_tools()  // Think phase should not use tools
            .build()?;

        // Run LLM without tools (pure reasoning)
        let think_agent = LlmAgent::new();
        think_agent.run(&think_ctx).await?;

        // Extract thought
        let output = self.extract_last_assistant_text(&think_ctx).await?;

        // Log the complete think output
        self.log(&format!("Think output:\n{}", output));

        // Parse prefix to determine decision
        self.parse_think_decision(&output)
    }

    fn build_think_prompt(&self) -> String {
        r#"You are in the THINK phase of the ReAct (Reasoning and Acting) framework.

Your task is to analyze the current situation and decide what to do next.

Review:
1. The user's original question
2. All previous thoughts and observations
3. What information you currently have
4. What information is still missing

CRITICAL: You MUST start your response with EXACTLY ONE prefix:

[think] - If you need more time to analyze before taking action
[act] - If you're ready to take an action (use a tool)
[answer] - If you have enough information to provide the final answer

Examples:

[think] I need to understand the problem better. Let me analyze...

[act] I will search for files using file-glob to find all Rust files.

[answer] Based on the observations, the answer is: ...

STRICT RULES:
- Use ONLY ONE prefix per response
- The prefix MUST be at the very start (first line)
- NEVER use multiple prefixes in the same response
- NEVER output [think] followed by [act] or [answer]
- Choose [think] if you're uncertain or need to reason more
- Choose [act] when you have a clear action to take
- Choose [answer] when you're ready to provide the final answer to the user"#
            .to_string()
    }

    fn parse_think_decision(&self, output: &str) -> Result<ThinkDecision> {
        let trimmed = output.trim();

        // Count how many decision markers appear in the output
        let think_count = trimmed.matches("[think]").count();
        let act_count = trimmed.matches("[act]").count();
        let answer_count = trimmed.matches("[answer]").count();
        let total_markers = think_count + act_count + answer_count;

        // Validate: exactly one marker allowed
        if total_markers == 0 {
            bail!(
                "Think phase output must start with [think], [act], or [answer]. Got: {}",
                trimmed.chars().take(50).collect::<String>()
            );
        }

        if total_markers > 1 {
            bail!(
                "Think phase output must contain ONLY ONE decision marker. Found: [think]={}, [act]={}, [answer]={}. Output: {}",
                think_count,
                act_count,
                answer_count,
                trimmed.chars().take(100).collect::<String>()
            );
        }

        // Parse the single marker
        if let Some(content) = trimmed.strip_prefix("[think]") {
            return Ok(ThinkDecision::ContinueThinking {
                thought: format!("[think]{}", content),
            });
        }

        if let Some(content) = trimmed.strip_prefix("[act]") {
            return Ok(ThinkDecision::ReadyToAct {
                thought: format!("[act]{}", content),
            });
        }

        if let Some(content) = trimmed.strip_prefix("[answer]") {
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
        let act_prompt = self.build_act_prompt();

        let act_ctx = AgentContextBuilder::from_parent_ctx(ctx)
            .add_system_prompt_segment(Box::new(StaticSystemPromptSegment::new(act_prompt)))
            .build()?;

        // Run LLM with tools - allows multiple tool calls
        let act_agent = LlmAgent::new();
        act_agent.run(&act_ctx).await?;

        // Extract the final text output as observation
        let observation = self.extract_last_assistant_text(&act_ctx).await?;

        self.log(&format!("Act output:\n{}", observation));

        Ok(observation)
    }

    fn build_act_prompt(&self) -> String {
        r#"You are in the ACT phase of the ReAct framework.

Based on your previous thinking, now execute the planned action.

You can:
- Use tools to gather information or perform actions
- Call multiple tools in sequence if needed for this action
- When you're done with the action, summarize what you accomplished

After you finish using tools, provide a brief summary of what you did and what you learned.
This summary will be provided as "Observation" to the next THINK phase.

Example:
[Calls file-glob tool]
[Calls file-read tool]
"I searched for Rust files and found 10 files. I read the first file which contains..."

The observation will help you analyze the results in the next thinking phase."#
            .to_string()
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
    fn test_parse_think_decision_multiple_markers_error() {
        let agent = ReActAgent::new();

        // Invalid: multiple markers
        let result = agent.parse_think_decision("[think] Let me think... [act] Now I will act.");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("ONLY ONE decision marker"));

        // Invalid: multiple same markers
        let result = agent.parse_think_decision("[think] First thought [think] Second thought");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("ONLY ONE decision marker"));
    }

    #[test]
    fn test_parse_think_decision_no_marker_error() {
        let agent = ReActAgent::new();

        // Invalid: no marker
        let result = agent.parse_think_decision("I forgot to add a marker.");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("must start with"));
    }

    #[test]
    fn test_parse_think_decision_marker_not_at_start() {
        let agent = ReActAgent::new();

        // Invalid: marker not at start
        let result = agent.parse_think_decision("Let me think... [think] Now analyzing.");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("must START with"));
    }
}
