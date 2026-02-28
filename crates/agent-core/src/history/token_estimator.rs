use crate::llm::ChatMessage;
use std::sync::OnceLock;
use tiktoken_rs::CoreBPE;

/// Global tokenizer instance (cl100k_base encoding used by GPT-4, GPT-3.5-turbo, and as approximation for Claude)
static TOKENIZER: OnceLock<CoreBPE> = OnceLock::new();

fn get_tokenizer() -> &'static CoreBPE {
    TOKENIZER.get_or_init(|| {
        tiktoken_rs::cl100k_base().expect("Failed to initialize tiktoken cl100k_base")
    })
}

/// Estimate the number of tokens in a text string using tiktoken.
/// Uses cl100k_base encoding (GPT-4, GPT-3.5-turbo).
pub fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }

    let tokenizer = get_tokenizer();
    tokenizer.encode_with_special_tokens(text).len()
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
        // Empty string
        assert_eq!(estimate_tokens(""), 0);

        // ASCII text - "Hello world" is 2 tokens in cl100k_base
        let ascii = "Hello world";
        let tokens = estimate_tokens(ascii);
        assert_eq!(tokens, 2, "Hello world should be 2 tokens");

        // CJK text - "你好世界" is 5 tokens in cl100k_base
        let cjk = "你好世界";
        let tokens = estimate_tokens(cjk);
        assert_eq!(tokens, 5, "你好世界 should be 5 tokens");

        // Mixed text - "Hello 世界" is 5 tokens in cl100k_base
        let mixed = "Hello 世界";
        let tokens = estimate_tokens(mixed);
        assert_eq!(tokens, 5, "Hello 世界 should be 5 tokens");

        // Code snippet
        let code = "fn main() { println!(\"Hello\"); }";
        let tokens = estimate_tokens(code);
        assert_eq!(tokens, 9, "Code snippet should be 9 tokens");
    }
}
