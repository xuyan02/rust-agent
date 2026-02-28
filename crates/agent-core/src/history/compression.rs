use crate::llm::{ChatMessage, ChatRole};
use super::token_estimator::estimate_message_tokens;
use crate::Agent;

/// Safely truncate a string to at most `max_bytes` bytes, ensuring we don't cut in the middle of a UTF-8 character.
fn truncate_str(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }

    // Find the largest valid UTF-8 character boundary at or before max_bytes
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }

    &s[..end]
}

/// Configuration for history compression.
#[derive(Debug, Clone)]
pub struct CompressionConfig {
    /// Trigger compression when total tokens exceed this threshold (default: 16000)
    pub compress_threshold_tokens: usize,

    /// Target tokens to compress in each operation (default: 8000)
    pub compress_target_tokens: usize,

    /// Keep recent messages (in tokens) uncompressed (default: 4000)
    pub keep_recent_tokens: usize,

    /// Enable compression (default: true)
    pub enabled: bool,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            compress_threshold_tokens: 16000,
            compress_target_tokens: 8000,
            keep_recent_tokens: 4000,
            enabled: true,
        }
    }
}

/// Strategy for compressing conversation history.
pub struct CompressionStrategy {
    config: CompressionConfig,
}

impl CompressionStrategy {
    pub fn new(config: CompressionConfig) -> Self {
        Self { config }
    }

    /// Check if compression should be triggered.
    pub fn should_compress(&self, messages: &[ChatMessage]) -> bool {
        if !self.config.enabled {
            return false;
        }

        let total_tokens: usize = messages
            .iter()
            .map(estimate_message_tokens)
            .sum();

        total_tokens > self.config.compress_threshold_tokens
    }

    /// Find the split point for compression.
    /// Returns (compress_until_index, keep_from_index).
    pub fn find_split_point(&self, messages: &[ChatMessage]) -> (usize, usize) {
        let mut accumulated_tokens = 0;
        let target_tokens = self.config.compress_target_tokens;

        // Find how many messages to compress
        let mut compress_until = 0;
        for (i, msg) in messages.iter().enumerate() {
            accumulated_tokens += estimate_message_tokens(msg);
            if accumulated_tokens >= target_tokens {
                compress_until = i + 1;
                break;
            }
        }

        // Ensure we don't compress everything
        if compress_until >= messages.len() {
            compress_until = messages.len().saturating_sub(5).max(1);
        }

        // Calculate keep_from based on keep_recent_tokens
        let mut recent_tokens = 0;
        let keep_recent_tokens = self.config.keep_recent_tokens;
        let mut keep_from = messages.len();

        for i in (0..messages.len()).rev() {
            recent_tokens += estimate_message_tokens(&messages[i]);
            if recent_tokens >= keep_recent_tokens {
                keep_from = i;
                break;
            }
        }

        // Adjust keep_from to not split tool call pairs
        keep_from = self.adjust_keep_from_for_tool_calls(messages, keep_from);

        // Adjust compression range to not overlap with keep_from
        if compress_until > keep_from {
            compress_until = keep_from;
        }

        // Ensure we don't split tool call pairs at compress_until boundary
        compress_until = self.adjust_compress_until_for_tool_calls(messages, compress_until);

        (compress_until, keep_from)
    }

    /// Adjust compress_until boundary to not split tool call pairs.
    /// If compress_until is in the middle of a tool call pair, move it forward to include all tool results.
    fn adjust_compress_until_for_tool_calls(&self, messages: &[ChatMessage], mut compress_until: usize) -> usize {
        if compress_until == 0 || compress_until >= messages.len() {
            return compress_until;
        }

        // Check if we're about to split a tool call pair
        // A tool call pair is: Assistant(ToolCalls) followed by Tool(ToolResult)
        if compress_until > 0 {
            let prev_msg = &messages[compress_until - 1];

            // If the message before the cut is Assistant with ToolCalls
            if prev_msg.role == ChatRole::Assistant {
                if let crate::llm::ChatContent::ToolCalls(_) = &prev_msg.content {
                    // Look ahead to find all corresponding Tool messages
                    let mut i = compress_until;
                    while i < messages.len() {
                        if messages[i].role == ChatRole::Tool {
                            i += 1;
                        } else {
                            break;
                        }
                    }
                    // Move compress_until to include all Tool messages
                    compress_until = i;
                }
            }
        }

        compress_until
    }

    /// Adjust keep_from boundary to not split tool call pairs.
    /// If keep_from points to a Tool message, move it backward to include the preceding Assistant ToolCalls.
    fn adjust_keep_from_for_tool_calls(&self, messages: &[ChatMessage], mut keep_from: usize) -> usize {
        if keep_from == 0 || keep_from >= messages.len() {
            return keep_from;
        }

        // Check if keep_from points to a Tool message
        if messages[keep_from].role == ChatRole::Tool {
            // Search backward for the corresponding Assistant ToolCalls message
            for i in (0..keep_from).rev() {
                if messages[i].role == ChatRole::Assistant {
                    if let crate::llm::ChatContent::ToolCalls(_) = &messages[i].content {
                        // Found the ToolCalls message, move keep_from to this position
                        keep_from = i;
                        break;
                    }
                }
            }
        }

        keep_from
    }

    /// Clean leading Tool messages to ensure valid OpenAI API format.
    /// OpenAI API requires first message to be System/User/Assistant, not Tool.
    pub fn clean_leading_tool_messages(messages: &mut Vec<ChatMessage>) {
        while let Some(first) = messages.first() {
            if matches!(first.role, ChatRole::Tool) {
                messages.remove(0);
            } else {
                break;
            }
        }
    }

    /// Create a compression summary message using LLM.
    pub async fn create_summary_message(
        &self,
        ctx: &crate::AgentContext<'_>,
        compressed_messages: &[ChatMessage],
        archive_filename: &str,
        message_count: usize,
        _estimated_tokens: usize,
    ) -> crate::Result<ChatMessage> {
        eprintln!("[Compression] Generating LLM summary for {} compressed messages", message_count);

        // Create a fresh session for summarization
        let runtime = ctx.session().runtime_rc();
        let model = ctx.session().default_model();

        let session = crate::SessionBuilder::new(runtime)
            .set_default_model(model.to_string())
            .build()?;

        // Create a new context with isolated history for summarization
        let summary_ctx = crate::AgentContextBuilder::from_session(&session)
            .build()?;

        // Build the summarization prompt
        let prompt = self.build_summarization_prompt(compressed_messages);

        // Add the prompt as user message
        summary_ctx.history().append(&summary_ctx, ChatMessage::user_text(prompt)).await?;

        // Use LlmAgent to generate summary
        let agent = crate::LlmAgent::new();
        agent.run(&summary_ctx).await?;

        // Extract the summary from the assistant's response
        let messages = summary_ctx.history().get_all(&summary_ctx).await?;
        let summary_text = messages
            .iter()
            .rev()
            .find_map(|m| {
                if m.role == ChatRole::Assistant {
                    if let crate::llm::ChatContent::Text(text) = &m.content {
                        Some(text.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "（无法生成摘要）".to_string());

        eprintln!("[Compression] LLM summary generated ({}chars)", summary_text.len());

        // Format the final compression message
        let content = format!(
            "[Previous {} messages archived to history/{}]\n\n\
            Summary:\n{}\n\n\
            Conversation continues...",
            message_count, archive_filename, summary_text.trim()
        );

        Ok(ChatMessage::system_text(content))
    }

    /// Build a prompt for LLM to summarize the compressed messages.
    fn build_summarization_prompt(&self, messages: &[ChatMessage]) -> String {
        use crate::llm::ChatContent;

        let mut dialogue = String::new();

        for (idx, msg) in messages.iter().enumerate() {
            let role_str = match msg.role {
                ChatRole::System => "System",
                ChatRole::User => "User",
                ChatRole::Assistant => "Assistant",
                ChatRole::Tool => "Tool",
            };

            let content_str = match &msg.content {
                ChatContent::Text(text) => text.clone(),
                ChatContent::ToolCalls(calls) => {
                    format!("[Tool calls: {}]",
                        serde_json::to_string(calls).unwrap_or_default())
                }
                ChatContent::ToolResult { tool_call_id, result } => {
                    format!("[Tool result {}: {}]", tool_call_id,
                        if result.len() > 100 {
                            format!("{}...", truncate_str(result, 100))
                        } else {
                            result.clone()
                        })
                }
            };

            dialogue.push_str(&format!("{}. {}: {}\n\n", idx + 1, role_str, content_str));
        }

        format!(
            "Summarize the key information from the following conversation. Requirements:\n\
            1. Concisely summarize the main topics and conclusions\n\
            2. Preserve important facts, decisions, and outcomes\n\
            3. If there are tool calls, explain what operations were performed\n\
            4. Keep it under 300 words\n\
            5. Use bullet point format (start with -)\n\n\
            Conversation:\n\
            {}\n\n\
            Output the summary directly without any preamble:",
            dialogue
        )
    }


}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::ChatContent;

    #[test]
    fn test_compression_config() {
        let config = CompressionConfig::default();
        assert_eq!(config.compress_threshold_tokens, 16000);
        assert!(config.enabled);

        let config = CompressionConfig {
            enabled: false,
            ..Default::default()
        };
        assert!(!config.enabled);
    }

    #[test]
    fn test_should_compress() {
        let config = CompressionConfig {
            compress_threshold_tokens: 100,
            ..Default::default()
        };
        let strategy = CompressionStrategy::new(config);

        let messages = vec![
            ChatMessage {
                role: ChatRole::User,
                content: ChatContent::Text("Short".to_string()),
            },
        ];

        // Short message, should not compress
        assert!(!strategy.should_compress(&messages));

        // Many messages, should compress
        let long_messages: Vec<_> = (0..50)
            .map(|i| ChatMessage {
                role: ChatRole::User,
                content: ChatContent::Text(format!("Message {}", i)),
            })
            .collect();

        assert!(strategy.should_compress(&long_messages));
    }

    #[test]
    fn test_clean_leading_tool_messages() {
        let mut messages = vec![
            ChatMessage::tool_result("id1".to_string(), "result1".to_string()),
            ChatMessage::tool_result("id2".to_string(), "result2".to_string()),
            ChatMessage::user_text("Hello".to_string()),
        ];

        CompressionStrategy::clean_leading_tool_messages(&mut messages);

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, ChatRole::User);
    }

    #[test]
    fn test_adjust_compress_until_for_tool_calls() {
        let strategy = CompressionStrategy::new(CompressionConfig::default());

        // Setup: User -> Assistant(ToolCalls) -> Tool -> Tool -> User
        let tool_calls = serde_json::json!([{
            "id": "call1",
            "function_name": "func1",
            "arguments": {}
        }]);

        let messages = vec![
            ChatMessage::user_text("Question".to_string()),
            ChatMessage::assistant_tool_calls(tool_calls),
            ChatMessage::tool_result("call1".to_string(), "result1".to_string()),
            ChatMessage::tool_result("call2".to_string(), "result2".to_string()),
            ChatMessage::user_text("Follow-up".to_string()),
        ];

        // If compress_until is 2 (right after ToolCalls), it should move to 4 to include all Tool messages
        let adjusted = strategy.adjust_compress_until_for_tool_calls(&messages, 2);
        assert_eq!(adjusted, 4, "Should include all Tool messages after ToolCalls");

        // If compress_until is already at a safe position, no adjustment needed
        let adjusted = strategy.adjust_compress_until_for_tool_calls(&messages, 1);
        assert_eq!(adjusted, 1, "Safe position should not be adjusted");
    }

    #[test]
    fn test_adjust_keep_from_for_tool_calls() {
        let strategy = CompressionStrategy::new(CompressionConfig::default());

        // Setup: User -> Assistant(ToolCalls) -> Tool -> Tool -> User
        let tool_calls = serde_json::json!([{
            "id": "call1",
            "function_name": "func1",
            "arguments": {}
        }]);

        let messages = vec![
            ChatMessage::user_text("Question".to_string()),
            ChatMessage::assistant_tool_calls(tool_calls),
            ChatMessage::tool_result("call1".to_string(), "result1".to_string()),
            ChatMessage::tool_result("call2".to_string(), "result2".to_string()),
            ChatMessage::user_text("Follow-up".to_string()),
        ];

        // If keep_from is 2 (points to Tool message), it should move back to 1 (the ToolCalls)
        let adjusted = strategy.adjust_keep_from_for_tool_calls(&messages, 2);
        assert_eq!(adjusted, 1, "Should move back to include the ToolCalls message");

        // If keep_from is 3 (another Tool message), it should also move back to 1
        let adjusted = strategy.adjust_keep_from_for_tool_calls(&messages, 3);
        assert_eq!(adjusted, 1, "Should move back to include the ToolCalls message");

        // If keep_from is already at a safe position (User message), no adjustment needed
        let adjusted = strategy.adjust_keep_from_for_tool_calls(&messages, 4);
        assert_eq!(adjusted, 4, "Safe position should not be adjusted");
    }
}
