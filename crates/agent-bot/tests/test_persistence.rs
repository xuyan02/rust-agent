use agent_bot::{GoalState, MemoryState};
use agent_core::DataStore;
use std::rc::Rc;
use tempfile::TempDir;

#[tokio::test(flavor = "current_thread")]
async fn test_goal_persistence() {
    let temp_dir = TempDir::new().unwrap();
    let store = Rc::new(DataStore::new(temp_dir.path().to_path_buf()));
    let dir = store.root_dir().subdir("test_bot");

    // Create goal state
    let goal_state = GoalState::new(dir.node("goal"));

    // Load (creates default if file doesn't exist)
    goal_state.load().await.unwrap();

    // Set a goal
    goal_state.set("Build a robust agent system".to_string());
    assert_eq!(
        goal_state.get(),
        Some("Build a robust agent system".to_string())
    );

    // Flush to disk
    goal_state.flush().await.unwrap();

    // Create a new goal state and load from disk
    let goal_state2 = GoalState::new(dir.node("goal"));
    goal_state2.load().await.unwrap();

    // Should have the same goal
    assert_eq!(
        goal_state2.get(),
        Some("Build a robust agent system".to_string())
    );

    // Clear goal
    goal_state2.clear();
    assert_eq!(goal_state2.get(), None);

    goal_state2.flush().await.unwrap();

    // Load again should be empty
    let goal_state3 = GoalState::new(dir.node("goal"));
    goal_state3.load().await.unwrap();
    assert_eq!(goal_state3.get(), None);
}

#[tokio::test(flavor = "current_thread")]
async fn test_memory_persistence() {
    let temp_dir = TempDir::new().unwrap();
    let store = Rc::new(DataStore::new(temp_dir.path().to_path_buf()));
    let dir = store.root_dir().subdir("test_bot");

    // Create memory state with persistence
    let memory_state = MemoryState::new(dir.node("memory"));

    // Load (creates default if file doesn't exist)
    memory_state.load().await.unwrap();

    // Add memories
    memory_state.add("User prefers Rust".to_string());
    memory_state.add("Project uses tokio".to_string());
    assert_eq!(memory_state.get_all().len(), 2);

    // Flush to disk
    memory_state.flush().await.unwrap();

    // Create a new memory state and load from disk
    let memory_state2 = MemoryState::new(dir.node("memory"));
    memory_state2.load().await.unwrap();

    // Should have the same memories
    let memories = memory_state2.get_all();
    assert_eq!(memories.len(), 2);
    assert_eq!(memories[0], "User prefers Rust");
    assert_eq!(memories[1], "Project uses tokio");

    // Add another memory
    memory_state2.add("User's timezone is UTC+8".to_string());
    memory_state2.flush().await.unwrap();

    // Load in a new instance
    let memory_state3 = MemoryState::new(dir.node("memory"));
    memory_state3.load().await.unwrap();
    assert_eq!(memory_state3.get_all().len(), 3);
}
