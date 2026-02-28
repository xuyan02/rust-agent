/// Test demonstrating Session-level dir_node for PersistentHistory.
///
/// This shows how AgentContext can inherit dir_node from Session.
use agent_core::{
    AgentContextBuilder, DataStore, History, PersistentHistory, RuntimeBuilder, SessionBuilder,
};
use std::rc::Rc;

#[tokio::test]
async fn test_session_dir_node_inheritance() -> anyhow::Result<()> {
    let temp_dir = tempfile::tempdir()?;

    // Setup runtime with DataStore
    let runtime = Rc::new(
        RuntimeBuilder::new()
            .set_data_store_root(temp_dir.path().to_path_buf())
            .build(),
    );

    let data_store = runtime
        .data_store()
        .expect("runtime should have data store");
    let store_rc = Rc::new(DataStore::new(data_store.root().to_path_buf()));

    // Set dir_node at Session level
    let session_dir = store_rc.root_dir().subdir("session_storage");
    let session = SessionBuilder::new(Rc::clone(&runtime))
        .set_default_model("gpt-4o".to_string())
        .set_dir_node(session_dir)
        .build()?;

    // Create context without setting dir_node - it will inherit from Session
    let dir_node = session.dir_node().expect("session should have dir_node");
    let history: Box<dyn History> = Box::new(PersistentHistory::new(Rc::clone(&dir_node)));
    let ctx = AgentContextBuilder::from_session(&session)
        .set_history(history)
        .build()?;

    // History will use Session's dir_node
    ctx.history()
        .append(&ctx, agent_core::llm::ChatMessage::user_text("Message 1"))
        .await?;
    ctx.history()
        .append(
            &ctx,
            agent_core::llm::ChatMessage::assistant_text("Response 1"),
        )
        .await?;

    let all = ctx.history().get_all(&ctx).await?;
    assert_eq!(all.len(), 2);

    // Verify file is in session's dir_node
    let expected_path = temp_dir.path().join("session_storage").join("history.yaml");
    assert!(expected_path.exists(), "History should be in session_storage");

    Ok(())
}

#[tokio::test]
async fn test_context_dir_node_overrides_session() -> anyhow::Result<()> {
    let temp_dir = tempfile::tempdir()?;

    let runtime = Rc::new(
        RuntimeBuilder::new()
            .set_data_store_root(temp_dir.path().to_path_buf())
            .build(),
    );

    let data_store = runtime
        .data_store()
        .expect("runtime should have data store");
    let store_rc = Rc::new(DataStore::new(data_store.root().to_path_buf()));

    // Set dir_node at Session level
    let session_dir = store_rc.root_dir().subdir("session_storage");
    let session = SessionBuilder::new(Rc::clone(&runtime))
        .set_default_model("gpt-4o".to_string())
        .set_dir_node(session_dir)
        .build()?;

    // Create context with its own dir_node - overrides Session's
    let context_dir = store_rc.root_dir().subdir("context_storage");
    let history: Box<dyn History> = Box::new(PersistentHistory::new(Rc::clone(&context_dir)));
    let ctx = AgentContextBuilder::from_session(&session)
        .set_history(history)
        .build()?;

    ctx.history()
        .append(&ctx, agent_core::llm::ChatMessage::user_text("Message 1"))
        .await?;

    // Verify file is in context's dir_node, not session's
    let context_path = temp_dir
        .path()
        .join("context_storage")
        .join("history.yaml");
    let session_path = temp_dir
        .path()
        .join("session_storage")
        .join("history.yaml");

    assert!(context_path.exists(), "History should be in context_storage");
    assert!(
        !session_path.exists(),
        "History should NOT be in session_storage"
    );

    Ok(())
}

#[tokio::test]
async fn test_multiple_contexts_share_session_dir_node() -> anyhow::Result<()> {
    let temp_dir = tempfile::tempdir()?;

    let runtime = Rc::new(
        RuntimeBuilder::new()
            .set_data_store_root(temp_dir.path().to_path_buf())
            .build(),
    );

    let data_store = runtime
        .data_store()
        .expect("runtime should have data store");
    let store_rc = Rc::new(DataStore::new(data_store.root().to_path_buf()));

    // Set dir_node at Session level
    let session_dir = store_rc.root_dir().subdir("shared_storage");
    let session = SessionBuilder::new(Rc::clone(&runtime))
        .set_default_model("gpt-4o".to_string())
        .set_dir_node(session_dir)
        .build()?;

    // Create first context
    {
        let dir_node = session.dir_node().expect("session should have dir_node");
        let history: Box<dyn History> = Box::new(PersistentHistory::new(Rc::clone(&dir_node)));
        let ctx1 = AgentContextBuilder::from_session(&session)
            .set_history(history)
            .build()?;

        ctx1.history()
            .append(&ctx1, agent_core::llm::ChatMessage::user_text("From ctx1"))
            .await?;
    }

    // Create second context - will see first context's messages
    {
        let dir_node = session.dir_node().expect("session should have dir_node");
        let history: Box<dyn History> = Box::new(PersistentHistory::new(Rc::clone(&dir_node)));
        let ctx2 = AgentContextBuilder::from_session(&session)
            .set_history(history)
            .build()?;

        let all = ctx2.history().get_all(&ctx2).await?;
        assert_eq!(all.len(), 1);
        assert_eq!(
            all[0].content,
            agent_core::llm::ChatContent::Text("From ctx1".to_string())
        );

        ctx2.history()
            .append(&ctx2, agent_core::llm::ChatMessage::user_text("From ctx2"))
            .await?;
    }

    // Create third context - sees both
    {
        let dir_node = session.dir_node().expect("session should have dir_node");
        let history: Box<dyn History> = Box::new(PersistentHistory::new(Rc::clone(&dir_node)));
        let ctx3 = AgentContextBuilder::from_session(&session)
            .set_history(history)
            .build()?;

        let all = ctx3.history().get_all(&ctx3).await?;
        assert_eq!(all.len(), 2);
    }

    Ok(())
}
