use crate::llm::ChatMessage;

/// Estimate the number of tokens in a text string.
/// Uses heuristic: ~4 chars/token for ASCII, ~1.5 chars/token for CJK.
pub fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }

    let char_count = text.chars().count();
    let ascii_count = text.chars().filter(|c| c.is_ascii()).count();
    let ascii_ratio = ascii_count as f64 / char_count as f64;

    // Weighted estimation: ASCII=4 chars/token, CJK=1.5 chars/token
    let estimated = (ascii_ratio * char_count as f64 / 4.0)
                  + ((1.0 - ascii_ratio) * char_count as f64 / 1.5);
    estimated.ceil() as usize
}

/// Estimate total tokens for a message.
pub fn estimate_message_tokens(msg: &ChatMessage) -> usize {
    use crate::llm::ChatContent;
    match &msg.content {
        ChatContent::Text(text) => estimate_tokens(text),
        ChatContent::ToolCalls(tc) => {
            // Estimate tool calls as JSON string
            estimate_tokens(&serde_json::to_string(tc).unwrap_or_default())
        }
        ChatContent::ToolResult { result, .. } => estimate_tokens(result),
    }
}

/// Estimate total tokens for a list of messages.
pub fn estimate_messages_tokens(messages: &[ChatMessage]) -> usize {
    messages.iter().map(estimate_message_tokens).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens(""), 0);

        let ascii = "Hello world";
        let tokens = estimate_tokens(ascii);
        assert!(tokens > 0 && tokens < ascii.len());

        let cjk = "你好世界";
        let tokens = estimate_tokens(cjk);
        assert!(tokens > 0);
    }
}
