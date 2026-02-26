use agent_core::data_store::DataStore;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct BotConfig {
    name: String,
    model: String,
    temperature: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Memory {
    entries: Vec<String>,
}

#[test]
fn test_set_and_get() {
    let dir = tempfile::tempdir().unwrap();
    let store = DataStore::new(dir.path().to_path_buf());

    let cfg = BotConfig {
        name: "alice".into(),
        model: "gpt-4".into(),
        temperature: 0.7,
    };

    let node = store.node("config");
    node.set(&cfg).unwrap();

    let loaded: BotConfig = node.get().unwrap().unwrap();
    assert_eq!(loaded, cfg);
}

#[test]
fn test_nested_path() {
    let dir = tempfile::tempdir().unwrap();
    let store = DataStore::new(dir.path().to_path_buf());

    let cfg = BotConfig {
        name: "bob".into(),
        model: "gpt-4o".into(),
        temperature: 0.5,
    };

    let node = store.node("agents/bot_b");
    node.set(&cfg).unwrap();

    // Verify the file exists at the expected path.
    let expected_path = dir.path().join("agents/bot_b.yaml");
    assert!(expected_path.exists());

    // Verify content is valid YAML.
    let contents = std::fs::read_to_string(&expected_path).unwrap();
    let loaded: BotConfig = serde_yaml::from_str(&contents).unwrap();
    assert_eq!(loaded, cfg);
}

#[test]
fn test_get_nonexistent_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let store = DataStore::new(dir.path().to_path_buf());

    let node = store.node("missing");
    let result: Option<BotConfig> = node.get().unwrap();
    assert!(result.is_none());
}

#[test]
fn test_exists() {
    let dir = tempfile::tempdir().unwrap();
    let store = DataStore::new(dir.path().to_path_buf());

    let node = store.node("check");
    assert!(!node.exists());

    node.set(&42_i64).unwrap();
    assert!(node.exists());
}

#[test]
fn test_remove() {
    let dir = tempfile::tempdir().unwrap();
    let store = DataStore::new(dir.path().to_path_buf());

    let node = store.node("to_remove");
    node.set(&"hello").unwrap();
    assert!(node.exists());

    node.remove().unwrap();
    assert!(!node.exists());

    let result: Option<String> = node.get().unwrap();
    assert!(result.is_none());

    // File should be gone from disk.
    assert!(!node.path().exists());
}

#[test]
fn test_cache_avoids_disk_read() {
    let dir = tempfile::tempdir().unwrap();
    let store = DataStore::new(dir.path().to_path_buf());

    let cfg = BotConfig {
        name: "cached".into(),
        model: "gpt-4".into(),
        temperature: 0.9,
    };

    let node = store.node("cached_node");
    node.set(&cfg).unwrap();

    // Delete the file behind the node's back.
    std::fs::remove_file(node.path()).unwrap();

    // get() should still succeed from cache.
    let loaded: BotConfig = node.get().unwrap().unwrap();
    assert_eq!(loaded, cfg);
}

#[test]
fn test_children() {
    let dir = tempfile::tempdir().unwrap();
    let store = DataStore::new(dir.path().to_path_buf());

    store.node("agents/alice").set(&"a").unwrap();
    store.node("agents/bob").set(&"b").unwrap();
    store.node("agents/charlie").set(&"c").unwrap();
    // A node outside the agents dir.
    store.node("config").set(&"root").unwrap();

    let mut children = store.children("agents").unwrap();
    children.sort();
    assert_eq!(children, vec!["alice", "bob", "charlie"]);

    // Top-level children.
    let top = store.children("").unwrap();
    assert!(top.contains(&"config".to_string()));
}

#[test]
fn test_children_empty_dir() {
    let dir = tempfile::tempdir().unwrap();
    let store = DataStore::new(dir.path().to_path_buf());

    let children = store.children("nonexistent").unwrap();
    assert!(children.is_empty());
}

#[test]
fn test_subdirs() {
    let dir = tempfile::tempdir().unwrap();
    let store = DataStore::new(dir.path().to_path_buf());

    store.node("teams/alpha/bot1").set(&1).unwrap();
    store.node("teams/beta/bot2").set(&2).unwrap();
    store.node("config").set(&"root").unwrap();

    let mut dirs = store.subdirs("teams").unwrap();
    dirs.sort();
    assert_eq!(dirs, vec!["alpha", "beta"]);
}

#[test]
fn test_multiple_nodes_simultaneous() {
    let dir = tempfile::tempdir().unwrap();
    let store = DataStore::new(dir.path().to_path_buf());

    let a = store.node("a");
    let b = store.node("b");

    // Both references can coexist and be used independently.
    a.set(&"value_a").unwrap();
    b.set(&"value_b").unwrap();

    let va: String = a.get().unwrap().unwrap();
    let vb: String = b.get().unwrap().unwrap();
    assert_eq!(va, "value_a");
    assert_eq!(vb, "value_b");
}

#[test]
fn test_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let store = DataStore::new(dir.path().to_path_buf());

    let node = store.node("mutable");
    node.set(&"first").unwrap();

    let v1: String = node.get().unwrap().unwrap();
    assert_eq!(v1, "first");

    node.set(&"second").unwrap();

    let v2: String = node.get().unwrap().unwrap();
    assert_eq!(v2, "second");

    // Disk should also reflect the latest value.
    let contents = std::fs::read_to_string(node.path()).unwrap();
    let disk_val: String = serde_yaml::from_str(&contents).unwrap();
    assert_eq!(disk_val, "second");
}

#[test]
fn test_complex_struct() {
    let dir = tempfile::tempdir().unwrap();
    let store = DataStore::new(dir.path().to_path_buf());

    let mem = Memory {
        entries: vec!["hello".into(), "world".into()],
    };

    let node = store.node("agents/alice/memory");
    node.set(&mem).unwrap();

    let loaded: Memory = node.get().unwrap().unwrap();
    assert_eq!(loaded, mem);
}

#[test]
fn test_load_from_disk_without_cache() {
    let dir = tempfile::tempdir().unwrap();

    // Write a YAML file directly to disk.
    let file_path = dir.path().join("preexisting.yaml");
    let cfg = BotConfig {
        name: "preexisting".into(),
        model: "gpt-3.5".into(),
        temperature: 0.3,
    };
    let yaml = serde_yaml::to_string(&cfg).unwrap();
    std::fs::write(&file_path, &yaml).unwrap();

    // Create a fresh store and read the node – should load from disk.
    let store = DataStore::new(dir.path().to_path_buf());
    let node = store.node("preexisting");
    let loaded: BotConfig = node.get().unwrap().unwrap();
    assert_eq!(loaded, cfg);
}
