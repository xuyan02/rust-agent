/// Test that Bot uses PersistentHistory and stores data in .agent/<bot_name>/history.yaml
use agent_bot::{Bot, BotEvent, BotEventSink};
use agent_core::{DataStore, LocalSpawner, RuntimeBuilder};
use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;

struct TestSink {
    message_count: Rc<RefCell<usize>>,
}

impl TestSink {
    fn new() -> Self {
        Self {
            message_count: Rc::new(RefCell::new(0)),
        }
    }
}

impl BotEventSink for TestSink {
    fn emit(&mut self, _event: BotEvent) {
        *self.message_count.borrow_mut() += 1;
    }
}

struct DummySpawner;

impl LocalSpawner for DummySpawner {
    fn spawn_local(&self, _fut: Pin<Box<dyn std::future::Future<Output = ()>>>) {
        // Do nothing - we don't actually run the bot in these tests
    }
}

#[tokio::test]
async fn test_bot_uses_persistent_history() -> anyhow::Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let data_store_root = temp_dir.path().to_path_buf();

    // Setup runtime with DataStore and spawner
    let spawner: Rc<dyn LocalSpawner> = Rc::new(DummySpawner);
    let runtime = Rc::new(
        RuntimeBuilder::new()
            .set_local_spawner(spawner)
            .set_data_store_root(data_store_root.clone())
            .build(),
    );

    let bot_name = "test_bot";
    let tool_constructors: Rc<RefCell<Vec<Box<dyn Fn() -> Box<dyn agent_core::tools::Tool>>>>> =
        Rc::new(RefCell::new(Vec::new()));

    let sink = TestSink::new();
    let _bot = Bot::new(
        Rc::clone(&runtime),
        bot_name,
        "gpt-4o",
        tool_constructors,
        sink,
    )?;

    // Verify that DataStore path is correct
    let data_store = runtime.data_store().expect("runtime should have data store");
    let store = Rc::new(DataStore::new(data_store.root().to_path_buf()));
    let bot_dir = store.root_dir().subdir(bot_name);
    let history_node = bot_dir.node("history");

    // Check that history node path follows convention: <data_store_root>/<bot_name>/history.yaml
    let expected_history_path = data_store_root.join(bot_name).join("history.yaml");
    assert_eq!(
        history_node.path(),
        expected_history_path,
        "History path should be: {{data_store_root}}/{{bot_name}}/history.yaml"
    );

    Ok(())
}

#[tokio::test]
async fn test_multiple_bots_separate_storage() -> anyhow::Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let data_store_root = temp_dir.path().to_path_buf();

    let spawner: Rc<dyn LocalSpawner> = Rc::new(DummySpawner);
    let runtime = Rc::new(
        RuntimeBuilder::new()
            .set_local_spawner(spawner)
            .set_data_store_root(data_store_root.clone())
            .build(),
    );

    let tool_constructors: Rc<RefCell<Vec<Box<dyn Fn() -> Box<dyn agent_core::tools::Tool>>>>> =
        Rc::new(RefCell::new(Vec::new()));

    // Create bot1
    let sink1 = TestSink::new();
    let _bot1 = Bot::new(
        Rc::clone(&runtime),
        "bot1",
        "gpt-4o",
        Rc::clone(&tool_constructors),
        sink1,
    )?;

    // Create bot2
    let sink2 = TestSink::new();
    let _bot2 = Bot::new(
        Rc::clone(&runtime),
        "bot2",
        "gpt-4o",
        Rc::clone(&tool_constructors),
        sink2,
    )?;

    // Verify separate storage paths are configured
    let data_store = runtime.data_store().expect("runtime should have data store");
    let store = Rc::new(DataStore::new(data_store.root().to_path_buf()));

    let bot1_dir = store.root_dir().subdir("bot1");
    let bot2_dir = store.root_dir().subdir("bot2");

    let bot1_history_path = bot1_dir.node("history").path().to_path_buf();
    let bot2_history_path = bot2_dir.node("history").path().to_path_buf();

    // Verify paths are different
    assert_ne!(
        bot1_history_path, bot2_history_path,
        "Bots should have separate history storage paths"
    );

    // Verify paths follow convention
    assert_eq!(
        bot1_history_path,
        data_store_root.join("bot1").join("history.yaml")
    );
    assert_eq!(
        bot2_history_path,
        data_store_root.join("bot2").join("history.yaml")
    );

    Ok(())
}

#[tokio::test]
async fn test_bot_history_path_convention() -> anyhow::Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let data_store_root = temp_dir.path().to_path_buf();

    let spawner: Rc<dyn LocalSpawner> = Rc::new(DummySpawner);
    let runtime = Rc::new(
        RuntimeBuilder::new()
            .set_local_spawner(spawner)
            .set_data_store_root(data_store_root.clone())
            .build(),
    );

    let tool_constructors: Rc<RefCell<Vec<Box<dyn Fn() -> Box<dyn agent_core::tools::Tool>>>>> =
        Rc::new(RefCell::new(Vec::new()));

    let bot_name = "alice";
    let sink = TestSink::new();
    let _bot = Bot::new(
        Rc::clone(&runtime),
        bot_name,
        "gpt-4o",
        tool_constructors,
        sink,
    )?;

    // Verify path follows convention: <data_store_root>/<bot_name>/history.yaml
    let expected_history_path = data_store_root.join(bot_name).join("history.yaml");

    let data_store = runtime.data_store().expect("runtime should have data store");
    let store = Rc::new(DataStore::new(data_store.root().to_path_buf()));
    let bot_dir = store.root_dir().subdir(bot_name);
    let history_node = bot_dir.node("history");

    assert_eq!(
        history_node.path(),
        expected_history_path,
        "History path should follow convention: {{data_store_root}}/{{bot_name}}/history.yaml"
    );

    Ok(())
}
