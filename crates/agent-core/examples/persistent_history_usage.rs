/// Example demonstrating PersistentHistory usage in AgentContext.
///
/// This shows how to:
/// 1. Create a DataStore with a root directory
/// 2. Create a PersistentHistory that gets storage location from AgentContext
/// 3. Set dir_node on AgentContext
/// 4. Messages are automatically persisted to disk at ctx.dir_node()/history.yaml
use agent_core::{
    AgentContextBuilder, DataStore, History, PersistentHistory, RuntimeBuilder, SessionBuilder,
};
use std::rc::Rc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Create Runtime with DataStore
    let data_store_root = std::env::temp_dir().join("agent_data_example");
    let runtime = Rc::new(
        RuntimeBuilder::new()
            .set_data_store_root(data_store_root.clone())
            .build(),
    );

    // 2. Create Session
    let session = SessionBuilder::new(Rc::clone(&runtime))
        .set_default_model("gpt-4o".to_string())
        .build()?;

    // 3. Get DataStore and create directory for this context
    let data_store = runtime
        .data_store()
        .expect("Runtime should have data store");
    let store_rc = Rc::new(DataStore::new(data_store.root().to_path_buf()));
    let context_dir = store_rc.root_dir().subdir("example_context");

    // 4. Create PersistentHistory (no node parameter needed)
    let history: Box<dyn History> = Box::new(PersistentHistory::new());

    // 5. Create AgentContext with persistent history and dir_node
    let ctx = AgentContextBuilder::from_session(&session)
        .set_history(history)
        .set_dir_node(context_dir.clone())
        .build()?;

    // 6. Use the context - messages are automatically persisted
    // PersistentHistory will get storage location from ctx.dir_node()
    ctx.history()
        .append(&ctx, agent_core::llm::ChatMessage::user_text(
            "What is the capital of France?",
        ))
        .await?;

    ctx.history()
        .append(&ctx, agent_core::llm::ChatMessage::assistant_text(
            "The capital of France is Paris.",
        ))
        .await?;

    println!("Messages saved to: {}", data_store_root.display());
    println!(
        "History file: {}",
        context_dir.node("history").path().display()
    );

    // 7. Verify persistence - create new context with same dir_node
    let history2: Box<dyn History> = Box::new(PersistentHistory::new());

    let ctx2 = AgentContextBuilder::from_session(&session)
        .set_history(history2)
        .set_dir_node(context_dir)
        .build()?;

    let messages = ctx2.history().get_all(&ctx2).await?;
    println!("\nLoaded {} messages from disk:", messages.len());
    for (i, msg) in messages.iter().enumerate() {
        println!("  [{}] {:?}: {:?}", i, msg.role, msg.content);
    }

    Ok(())
}
