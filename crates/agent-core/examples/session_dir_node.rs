/// Example demonstrating Session-level dir_node for PersistentHistory.
///
/// This shows how to:
/// 1. Set dir_node at Session level (all contexts inherit)
/// 2. Override dir_node at Context level
/// 3. Share storage between multiple contexts
use agent_core::{
    AgentContextBuilder, DataStore, History, PersistentHistory, RuntimeBuilder, SessionBuilder,
};
use std::rc::Rc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let data_store_root = std::env::temp_dir().join("session_dir_node_example");

    // Clean up previous runs
    if data_store_root.exists() {
        std::fs::remove_dir_all(&data_store_root)?;
    }

    // 1. Create Runtime with DataStore
    let runtime = Rc::new(
        RuntimeBuilder::new()
            .set_data_store_root(data_store_root.clone())
            .build(),
    );

    // 2. Setup DataStore
    let data_store = runtime
        .data_store()
        .expect("Runtime should have data store");
    let store_rc = Rc::new(DataStore::new(data_store.root().to_path_buf()));

    println!("=== Example 1: Session-level dir_node ===\n");

    // 3. Set dir_node at Session level
    let session_dir = store_rc.root_dir().subdir("session_storage");
    let session = SessionBuilder::new(Rc::clone(&runtime))
        .set_default_model("gpt-4o".to_string())
        .set_dir_node(session_dir.clone())
        .build()?;

    // 4. Create contexts with session's dir_node
    {
        println!("Context 1 (uses Session's dir_node):");
        let dir_node = session.dir_node().expect("session should have dir_node");
        let history: Box<dyn History> = Box::new(PersistentHistory::new(Rc::clone(&dir_node)));
        let ctx1 = AgentContextBuilder::from_session(&session)
            .set_history(history)
            .build()?;

        ctx1.history()
            .append(
                &ctx1,
                agent_core::llm::ChatMessage::user_text("Message from ctx1"),
            )
            .await?;

        let messages = ctx1.history().get_all(&ctx1).await?;
        println!("  Messages: {}", messages.len());
        println!(
            "  Saved to: {}",
            session_dir.node("history").path().display()
        );
    }

    {
        println!("\nContext 2 (also uses Session's dir_node):");
        let dir_node = session.dir_node().expect("session should have dir_node");
        let history: Box<dyn History> = Box::new(PersistentHistory::new(Rc::clone(&dir_node)));
        let ctx2 = AgentContextBuilder::from_session(&session)
            .set_history(history)
            .build()?;

        // ctx2 sees ctx1's message because they share the same storage
        let messages = ctx2.history().get_all(&ctx2).await?;
        println!("  Messages (includes ctx1's): {}", messages.len());

        ctx2.history()
            .append(
                &ctx2,
                agent_core::llm::ChatMessage::user_text("Message from ctx2"),
            )
            .await?;

        let messages = ctx2.history().get_all(&ctx2).await?;
        println!("  Messages (after append): {}", messages.len());
    }

    println!("\n=== Example 2: Context-level override ===\n");

    // 5. Create context with its own dir_node
    let context_private_dir = store_rc.root_dir().subdir("context_private");
    let history: Box<dyn History> = Box::new(PersistentHistory::new(Rc::clone(&context_private_dir)));
    let ctx_private = AgentContextBuilder::from_session(&session)
        .set_history(history)
        .set_dir_node(context_private_dir.clone())
        .build()?;

    ctx_private
        .history()
        .append(
            &ctx_private,
            agent_core::llm::ChatMessage::user_text("Private message"),
        )
        .await?;

    let messages = ctx_private.history().get_all(&ctx_private).await?;
    println!("Private context:");
    println!("  Messages: {}", messages.len());
    println!(
        "  Saved to: {}",
        context_private_dir.node("history").path().display()
    );
    println!(
        "  (Different from Session storage: {})",
        session_dir.node("history").path().display()
    );

    println!("\n=== Summary ===");
    println!("Data store root: {}", data_store_root.display());
    println!("\nCreated files:");
    println!("  - session_storage/history.yaml (2 messages from ctx1 and ctx2)");
    println!("  - context_private/history.yaml (1 message from ctx_private)");

    // Verify files exist
    let session_file = data_store_root.join("session_storage").join("history.yaml");
    let private_file = data_store_root
        .join("context_private")
        .join("history.yaml");

    if session_file.exists() {
        println!("\n✓ Session storage file exists");
    }
    if private_file.exists() {
        println!("✓ Private context storage file exists");
    }

    Ok(())
}
