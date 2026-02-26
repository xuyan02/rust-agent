use agent_core::{History, InMemoryHistory, AgentContextBuilder, SessionBuilder};
use agent_core::llm::{ChatContent, ChatMessage};
use std::rc::Rc;

fn get_text(content: &ChatContent) -> &str {
    match content {
        ChatContent::Text(s) => s,
        _ => panic!("Expected text content"),
    }
}

#[tokio::test]
async fn history_respects_max_size() {
    // Create minimal runtime and session for AgentContext
    let runtime = Rc::new(agent_core::RuntimeBuilder::new().build());
    let session = SessionBuilder::new(runtime)
        .set_default_model("test-model".to_string())
        .build()
        .unwrap();
    let ctx = AgentContextBuilder::from_session(&session).build().unwrap();

    let history = InMemoryHistory::new_with_limit(3);

    // Add 5 messages
    for i in 1..=5 {
        history
            .append(&ctx, ChatMessage::user_text(format!("message {}", i)))
            .await
            .unwrap();
    }

    // Should only keep the last 3
    assert_eq!(history.len(), 3);

    let messages = history.get_all(&ctx).await.unwrap();
    assert_eq!(messages.len(), 3);

    // Verify we kept messages 3, 4, 5
    assert_eq!(get_text(&messages[0].content), "message 3");
    assert_eq!(get_text(&messages[1].content), "message 4");
    assert_eq!(get_text(&messages[2].content), "message 5");
}

#[tokio::test]
async fn history_sliding_window_on_append() {
    // Create minimal runtime and session for AgentContext
    let runtime = Rc::new(agent_core::RuntimeBuilder::new().build());
    let session = SessionBuilder::new(runtime)
        .set_default_model("test-model".to_string())
        .build()
        .unwrap();
    let ctx = AgentContextBuilder::from_session(&session).build().unwrap();

    let history = InMemoryHistory::new_with_limit(2);

    history
        .append(&ctx, ChatMessage::user_text("msg1".to_string()))
        .await
        .unwrap();
    assert_eq!(history.len(), 1);

    history
        .append(&ctx, ChatMessage::user_text("msg2".to_string()))
        .await
        .unwrap();
    assert_eq!(history.len(), 2);

    // This should trigger the sliding window
    history
        .append(&ctx, ChatMessage::user_text("msg3".to_string()))
        .await
        .unwrap();
    assert_eq!(history.len(), 2);

    let messages = history.get_all(&ctx).await.unwrap();
    assert_eq!(get_text(&messages[0].content), "msg2");
    assert_eq!(get_text(&messages[1].content), "msg3");
}

#[tokio::test]
async fn history_get_recent_returns_last_n() {
    // Create minimal runtime and session for AgentContext
    let runtime = Rc::new(agent_core::RuntimeBuilder::new().build());
    let session = SessionBuilder::new(runtime)
        .set_default_model("test-model".to_string())
        .build()
        .unwrap();
    let ctx = AgentContextBuilder::from_session(&session).build().unwrap();

    let history = InMemoryHistory::new_with_limit(10);

    for i in 1..=5 {
        history
            .append(&ctx, ChatMessage::user_text(format!("msg{}", i)))
            .await
            .unwrap();
    }

    // Get last 3 messages
    let recent = history.get_recent(&ctx, 3).await.unwrap();
    assert_eq!(recent.len(), 3);
    assert_eq!(get_text(&recent[0].content), "msg3");
    assert_eq!(get_text(&recent[1].content), "msg4");
    assert_eq!(get_text(&recent[2].content), "msg5");
}

#[tokio::test]
async fn history_get_recent_handles_fewer_messages() {
    // Create minimal runtime and session for AgentContext
    let runtime = Rc::new(agent_core::RuntimeBuilder::new().build());
    let session = SessionBuilder::new(runtime)
        .set_default_model("test-model".to_string())
        .build()
        .unwrap();
    let ctx = AgentContextBuilder::from_session(&session).build().unwrap();

    let history = InMemoryHistory::new_with_limit(10);

    history
        .append(&ctx, ChatMessage::user_text("only_msg".to_string()))
        .await
        .unwrap();

    // Request more than available
    let recent = history.get_recent(&ctx, 5).await.unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(get_text(&recent[0].content), "only_msg");
}

#[tokio::test]
async fn history_default_limit_is_1000() {
    // Create minimal runtime and session for AgentContext
    let runtime = Rc::new(agent_core::RuntimeBuilder::new().build());
    let session = SessionBuilder::new(runtime)
        .set_default_model("test-model".to_string())
        .build()
        .unwrap();
    let ctx = AgentContextBuilder::from_session(&session).build().unwrap();

    let history = InMemoryHistory::new();

    // Add 1500 messages
    for i in 1..=1500 {
        history
            .append(&ctx, ChatMessage::user_text(format!("msg{}", i)))
            .await
            .unwrap();
    }

    // Should only keep 1000
    assert_eq!(history.len(), 1000);

    let messages = history.get_all(&ctx).await.unwrap();
    assert_eq!(messages.len(), 1000);

    // Verify we kept messages 501-1500
    assert_eq!(get_text(&messages[0].content), "msg501");
    assert_eq!(get_text(&messages[999].content), "msg1500");
}

#[tokio::test]
async fn history_empty_check() {
    // Create minimal runtime and session for AgentContext
    let runtime = Rc::new(agent_core::RuntimeBuilder::new().build());
    let session = SessionBuilder::new(runtime)
        .set_default_model("test-model".to_string())
        .build()
        .unwrap();
    let ctx = AgentContextBuilder::from_session(&session).build().unwrap();

    let history = InMemoryHistory::new();
    assert!(history.is_empty());

    history
        .append(&ctx, ChatMessage::user_text("msg".to_string()))
        .await
        .unwrap();
    assert!(!history.is_empty());
}
