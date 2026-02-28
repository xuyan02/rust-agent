use agent_bot::{MemoryState, MemoryTool};
use agent_core::{DataStore, RuntimeBuilder, SessionBuilder};
use agent_core::tools::Tool;
use std::rc::Rc;
use tempfile::TempDir;

#[tokio::test(flavor = "current_thread")]
async fn test_memory_tool_remember_and_list() {
    let temp_dir = TempDir::new().unwrap();
    let store = Rc::new(DataStore::new(temp_dir.path().to_path_buf()));
    let dir = store.root_dir().subdir("test_bot");

    let runtime = Rc::new(RuntimeBuilder::new().build());
    let session = SessionBuilder::new(runtime)
        .set_default_model("test-model".to_string())
        .build()
        .unwrap();

    let ctx = agent_core::AgentContextBuilder::from_session(&session)
        .build()
        .unwrap();

    let memory_state = MemoryState::new(dir.node("memory"));
    let memory_tool = MemoryTool::new(memory_state.clone());

    // Initially no memories
    let result = memory_tool
        .invoke(&ctx, "list-memories", &serde_json::json!({}))
        .await
        .unwrap();
    assert!(result.contains("No memories"));

    // Add a memory
    let result = memory_tool
        .invoke(
            &ctx,
            "remember",
            &serde_json::json!({"memory": "User prefers Rust for system programming"}),
        )
        .await
        .unwrap();
    assert!(result.contains("recorded"));

    // List memories
    let result = memory_tool
        .invoke(&ctx, "list-memories", &serde_json::json!({}))
        .await
        .unwrap();
    assert!(result.contains("Rust for system programming"));

    // Add another memory
    memory_tool
        .invoke(
            &ctx,
            "remember",
            &serde_json::json!({"memory": "User is working on agent-bot project"}),
        )
        .await
        .unwrap();

    // Check state directly
    let memories = memory_state.get_all();
    assert_eq!(memories.len(), 2);
    assert!(memories[0].contains("Rust"));
    assert!(memories[1].contains("agent-bot"));
}

#[tokio::test(flavor = "current_thread")]
async fn test_memory_segment_rendering() {
    use agent_bot::MemorySegment;
    use agent_core::SystemPromptSegment;

    let temp_dir = TempDir::new().unwrap();
    let store = Rc::new(DataStore::new(temp_dir.path().to_path_buf()));
    let dir = store.root_dir().subdir("test_bot");

    let runtime = Rc::new(RuntimeBuilder::new().build());
    let session = SessionBuilder::new(runtime)
        .set_default_model("test-model".to_string())
        .build()
        .unwrap();

    let ctx = agent_core::AgentContextBuilder::from_session(&session)
        .build()
        .unwrap();

    let memory_state = MemoryState::new(dir.node("memory"));
    let segment = MemorySegment::new(memory_state.clone());

    // Empty state renders empty
    let rendered = segment.render(&ctx).await.unwrap();
    assert_eq!(rendered, "");

    // Add memories
    memory_state.add("User likes vim keybindings".to_string());
    memory_state.add("Project uses tokio runtime".to_string());

    // Render with memories
    let rendered = segment.render(&ctx).await.unwrap();
    assert!(rendered.contains("## Memory"));
    assert!(rendered.contains("1. User likes vim keybindings"));
    assert!(rendered.contains("2. Project uses tokio runtime"));
    assert!(rendered.contains("---"));
    assert!(rendered.contains("Total:"));
    assert!(rendered.contains("tokens"));
}
