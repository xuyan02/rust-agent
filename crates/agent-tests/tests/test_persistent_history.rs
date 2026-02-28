use agent_core::{
    AgentContextBuilder, DataStore, History, PersistentHistory, RuntimeBuilder, SessionBuilder,
};
use std::rc::Rc;

#[tokio::test]
async fn test_persistent_history_basic() -> anyhow::Result<()> {
    let temp_dir = tempfile::tempdir()?;

    // Setup runtime with DataStore
    let runtime = Rc::new(
        RuntimeBuilder::new()
            .set_data_store_root(temp_dir.path().to_path_buf())
            .build(),
    );

    let session = SessionBuilder::new(Rc::clone(&runtime))
        .set_default_model("gpt-4o".to_string())
        .build()?;

    // Create context with dir_node
    let data_store = runtime.data_store().expect("runtime should have data store");
    let store_rc = Rc::new(DataStore::new(data_store.root().to_path_buf()));
    let dir_node = store_rc.root_dir().subdir("test_context");

    let history: Box<dyn History> = Box::new(PersistentHistory::new(Rc::clone(&dir_node)));
    let ctx = AgentContextBuilder::from_session(&session)
        .set_history(history)
        .build()?;

    // Append messages
    let msg1 = agent_core::llm::ChatMessage::user_text("Hello");
    ctx.history().append(&ctx, msg1.clone()).await?;

    let msg2 = agent_core::llm::ChatMessage::assistant_text("Hi there");
    ctx.history().append(&ctx, msg2.clone()).await?;

    let all = ctx.history().get_all(&ctx).await?;
    assert_eq!(all.len(), 2);
    assert_eq!(all[0], msg1);
    assert_eq!(all[1], msg2);

    // Test last
    let last = ctx.history().last(&ctx).await?;
    assert_eq!(last, Some(msg2));

    Ok(())
}

#[tokio::test]
async fn test_persistent_history_persistence() -> anyhow::Result<()> {
    let temp_dir = tempfile::tempdir()?;

    // Setup runtime with DataStore
    let runtime = Rc::new(
        RuntimeBuilder::new()
            .set_data_store_root(temp_dir.path().to_path_buf())
            .build(),
    );

    let session = SessionBuilder::new(Rc::clone(&runtime))
        .set_default_model("gpt-4o".to_string())
        .build()?;

    let data_store = runtime.data_store().expect("runtime should have data store");
    let store_rc = Rc::new(DataStore::new(data_store.root().to_path_buf()));
    let dir_node = store_rc.root_dir().subdir("test_context");

    // Create first context and add messages
    {
        let history: Box<dyn History> = Box::new(PersistentHistory::new(Rc::clone(&dir_node)));
        let ctx = AgentContextBuilder::from_session(&session)
            .set_history(history)
            .set_dir_node(Rc::clone(&dir_node))
            .build()?;

        ctx.history().append(&ctx, agent_core::llm::ChatMessage::user_text("Message 1")).await?;
        ctx.history().append(&ctx, agent_core::llm::ChatMessage::assistant_text("Response 1")).await?;
        ctx.history().append(&ctx, agent_core::llm::ChatMessage::user_text("Message 2")).await?;

        let all = ctx.history().get_all(&ctx).await?;
        assert_eq!(all.len(), 3);
    }

    // Create second context and verify persistence
    {
        let history: Box<dyn History> = Box::new(PersistentHistory::new(Rc::clone(&dir_node)));
        let ctx = AgentContextBuilder::from_session(&session)
            .set_history(history)
            .set_dir_node(dir_node)
            .build()?;

        let all = ctx.history().get_all(&ctx).await?;
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].content, agent_core::llm::ChatContent::Text("Message 1".to_string()));
        assert_eq!(all[1].content, agent_core::llm::ChatContent::Text("Response 1".to_string()));
        assert_eq!(all[2].content, agent_core::llm::ChatContent::Text("Message 2".to_string()));
    }

    Ok(())
}

#[tokio::test]
async fn test_persistent_history_get_recent() -> anyhow::Result<()> {
    let temp_dir = tempfile::tempdir()?;

    let runtime = Rc::new(
        RuntimeBuilder::new()
            .set_data_store_root(temp_dir.path().to_path_buf())
            .build(),
    );

    let session = SessionBuilder::new(Rc::clone(&runtime))
        .set_default_model("gpt-4o".to_string())
        .build()?;

    let data_store = runtime.data_store().expect("runtime should have data store");
    let store_rc = Rc::new(DataStore::new(data_store.root().to_path_buf()));
    let dir_node = store_rc.root_dir().subdir("test_context");

    let history: Box<dyn History> = Box::new(PersistentHistory::new(Rc::clone(&dir_node)));
    let ctx = AgentContextBuilder::from_session(&session)
        .set_history(history)
        .set_dir_node(dir_node)
        .build()?;

    // Add 5 messages
    for i in 1..=5 {
        ctx.history().append(&ctx, agent_core::llm::ChatMessage::user_text(format!("Message {}", i))).await?;
    }

    // Get recent 2
    let recent = ctx.history().get_recent(&ctx, 2).await?;
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].content, agent_core::llm::ChatContent::Text("Message 4".to_string()));
    assert_eq!(recent[1].content, agent_core::llm::ChatContent::Text("Message 5".to_string()));

    // Get recent 10 (more than available)
    let recent = ctx.history().get_recent(&ctx, 10).await?;
    assert_eq!(recent.len(), 5);

    // Get recent 0
    let recent = ctx.history().get_recent(&ctx, 0).await?;
    assert_eq!(recent.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_persistent_history_subdirectory() -> anyhow::Result<()> {
    let temp_dir = tempfile::tempdir()?;

    let runtime = Rc::new(
        RuntimeBuilder::new()
            .set_data_store_root(temp_dir.path().to_path_buf())
            .build(),
    );

    let session = SessionBuilder::new(Rc::clone(&runtime))
        .set_default_model("gpt-4o".to_string())
        .build()?;

    let data_store = runtime.data_store().expect("runtime should have data store");
    let store_rc = Rc::new(DataStore::new(data_store.root().to_path_buf()));

    // Create history in a subdirectory
    let dir_node = store_rc.root_dir().subdir("context1");

    let history: Box<dyn History> = Box::new(PersistentHistory::new(Rc::clone(&dir_node)));
    let ctx = AgentContextBuilder::from_session(&session)
        .set_history(history)
        .set_dir_node(dir_node)
        .build()?;

    ctx.history().append(&ctx, agent_core::llm::ChatMessage::user_text("Test in subdir")).await?;

    let all = ctx.history().get_all(&ctx).await?;
    assert_eq!(all.len(), 1);

    // Verify the file is in the correct location
    let expected_path = temp_dir.path().join("context1").join("history.yaml");
    assert!(expected_path.exists());

    Ok(())
}

#[tokio::test]
async fn test_persistent_history_unpaired_tool_call_cleanup() -> anyhow::Result<()> {
    use agent_core::llm::{ChatRole, ChatContent};
    use serde_json::json;

    let temp_dir = tempfile::tempdir()?;

    let runtime = Rc::new(
        RuntimeBuilder::new()
            .set_data_store_root(temp_dir.path().to_path_buf())
            .build(),
    );

    let session = SessionBuilder::new(Rc::clone(&runtime))
        .set_default_model("gpt-4o".to_string())
        .build()?;

    let data_store = runtime.data_store().expect("runtime should have data store");
    let store_rc = Rc::new(DataStore::new(data_store.root().to_path_buf()));
    let dir_node = store_rc.root_dir().subdir("test_unpaired");

    // Create messages programmatically:
    // 1. Normal user/assistant exchange
    // 2. Paired tool call/result
    // 3. Unpaired tool call at the end (simulating crash/interruption)
    let messages = vec![
        agent_core::llm::ChatMessage::user_text("Hello"),
        agent_core::llm::ChatMessage::assistant_text("Hi, let me check something"),
        agent_core::llm::ChatMessage::assistant_tool_calls(json!([{
            "id": "call1",
            "function_name": "test_tool",
            "arguments": {}
        }])),
        agent_core::llm::ChatMessage::tool_result("call1", "Tool executed successfully"),
        agent_core::llm::ChatMessage::assistant_text("Got the result"),
        agent_core::llm::ChatMessage::user_text("Do another check"),
        // This unpaired tool call simulates a crash/interruption
        agent_core::llm::ChatMessage::assistant_tool_calls(json!([{
            "id": "call2",
            "function_name": "another_tool",
            "arguments": {}
        }])),
    ];

    // Manually save the history file with the unpaired tool call
    let history_dir = temp_dir.path().join("test_unpaired");
    tokio::fs::create_dir_all(&history_dir).await?;
    let history_file = history_dir.join("history.yaml");

    let yaml = serde_yaml::to_string(&messages)?;
    tokio::fs::write(&history_file, yaml).await?;

    // Load the history - unpaired tool call should be removed
    let history: Box<dyn History> = Box::new(PersistentHistory::new(Rc::clone(&dir_node)));
    let ctx = AgentContextBuilder::from_session(&session)
        .set_history(history)
        .set_dir_node(dir_node)
        .build()?;

    let all = ctx.history().get_all(&ctx).await?;

    // Should have 6 messages (7 - 1 unpaired tool call)
    assert_eq!(all.len(), 6, "Unpaired tool call should be removed");

    // Verify the messages are correct
    assert_eq!(all[0].role, ChatRole::User);
    assert_eq!(all[1].role, ChatRole::Assistant);
    assert_eq!(all[2].role, ChatRole::Assistant);
    assert!(matches!(all[2].content, ChatContent::ToolCalls(_)));
    assert_eq!(all[3].role, ChatRole::Tool);
    assert_eq!(all[4].role, ChatRole::Assistant);
    assert_eq!(all[5].role, ChatRole::User);

    // The unpaired tool call (call2) should not be present
    // Last message should be User, not Assistant with ToolCalls
    assert_eq!(all.last().unwrap().role, ChatRole::User);
    if let ChatContent::Text(text) = &all.last().unwrap().content {
        assert_eq!(text, "Do another check");
    } else {
        panic!("Last message should be User text");
    }

    Ok(())
}
