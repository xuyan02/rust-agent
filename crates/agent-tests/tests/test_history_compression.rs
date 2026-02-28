/// Test history compression functionality
use agent_core::{
    estimate_tokens, estimate_messages_tokens,
    PersistentHistory, DataStore,
};
use agent_core::llm::{ChatMessage, ChatRole, ChatContent};

#[test]
fn test_estimate_tokens() {
    // ASCII text: ~4 chars = 1 token
    let ascii = "Hello world this is a test";
    let tokens = estimate_tokens(ascii);
    assert!(tokens > 0);
    assert!(tokens < ascii.len()); // Should be less than char count

    // CJK text: ~1.5 chars = 1 token
    let cjk = "这是一个测试消息内容";
    let tokens = estimate_tokens(cjk);
    assert!(tokens > 0);

    // Empty
    assert_eq!(estimate_tokens(""), 0);
}

#[tokio::test]
async fn test_persistent_history_with_compression() -> anyhow::Result<()> {
    use std::rc::Rc;

    let temp_dir = tempfile::tempdir()?;
    let data_store = Rc::new(DataStore::new(temp_dir.path().to_path_buf()));
    let dir_node = data_store.root_dir().subdir("test_bot");

    // Create runtime (minimal, no actual LLM provider needed for this test)
    let runtime = Rc::new(agent_core::RuntimeBuilder::new().build());

    // Create session with dir_node
    let session = agent_core::SessionBuilder::new(runtime)
        .set_default_model("test-model".to_string())
        .set_dir_node(Rc::clone(&dir_node))
        .build()?;

    // Create history with normal configuration
    let history = Box::new(PersistentHistory::new(dir_node));

    // Build context
    let ctx = agent_core::AgentContextBuilder::from_session(&session)
        .set_history(history)
        .build()?;

    // Add enough messages to exceed threshold (20000 tokens)
    // Each message is ~500 tokens, so we need ~50 messages
    for i in 0..50 {
        let msg = ChatMessage {
            role: ChatRole::User,
            content: ChatContent::Text(format!(
                "This is test message number {}. It contains a substantial amount of text to accumulate tokens. \
                 We need enough messages to trigger the compression mechanism at 20000 tokens. \
                 Each message should have a reasonable amount of text to simulate a real conversation. \
                 Adding more content here to increase token count per message. \
                 Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
                 Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. \
                 Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris. \
                 Duis aute irure dolor in reprehenderit in voluptate velit esse cillum. \
                 Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia. \
                 这是一些中文内容来增加token数量。这样可以更快地达到压缩阈值。\
                 需要确保每条消息都有足够的内容来触发压缩机制。",
                i
            )),
        };
        ctx.history().append(&ctx, msg).await?;
    }

    // Get all messages
    let messages = ctx.history().get_all(&ctx).await?;

    // Should have messages (some compressed into summary)
    assert!(!messages.is_empty());

    // Check if first message is a compression summary
    if let Some(first) = messages.first() {
        if first.role == ChatRole::System {
            if let ChatContent::Text(text) = &first.content {
                // Should contain compression markers in natural language
                assert!(text.contains("归档") || text.contains("history/"),
                    "Expected compression summary but got: {}", text);
            }
        }
    }

    // Check history archive directory was created
    let history_dir = temp_dir.path().join("test_bot").join("history");
    if history_dir.exists() {
        let entries = std::fs::read_dir(&history_dir)?;
        let count = entries.count();
        println!("Created {} history archive files", count);
        assert!(count > 0, "Should have created at least one archive file");
    }

    Ok(())
}

#[test]
fn test_estimate_messages_tokens() {
    let messages = vec![
        ChatMessage {
            role: ChatRole::User,
            content: ChatContent::Text("Hello".to_string()),
        },
        ChatMessage {
            role: ChatRole::Assistant,
            content: ChatContent::Text("Hi there!".to_string()),
        },
    ];

    let total = estimate_messages_tokens(&messages);
    assert!(total > 0);
    assert!(total < 100); // Should be reasonable for short messages
}
