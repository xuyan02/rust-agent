use agent_bot::{SoulState, SoulTool};
use agent_core::{AgentContextBuilder, SessionBuilder, RuntimeBuilder};
use agent_core::tools::Tool;
use anyhow::Result;
use std::rc::Rc;

#[tokio::test]
async fn test_soul_read_write() -> Result<()> {
    // Create a temporary directory for testing
    let temp_dir = tempfile::tempdir()?;
    let data_store = Rc::new(agent_core::DataStore::new(temp_dir.path().to_path_buf()));
    let soul_node = data_store.root_dir().node("soul");

    let soul_state = SoulState::new(soul_node);
    let soul_tool = SoulTool::new(soul_state.clone());

    // Build a minimal session and context
    let runtime = Rc::new(RuntimeBuilder::new().build());
    let session = SessionBuilder::new(runtime).build()?;
    let ctx = AgentContextBuilder::from_session(&session).build()?;

    // Test 1: Read empty soul
    let result = soul_tool
        .invoke(
            &ctx,
            "read-soul",
            &serde_json::json!({}),
        )
        .await?;
    assert!(result.contains("empty"));

    // Test 2: Write soul
    let soul_content = "I am TestBot, a helpful assistant.\n\nNative Language: English\n\nMy personality: Friendly and concise.\n\nMy capabilities:\n- Code analysis\n- Bug fixing\n- Documentation";
    let result = soul_tool
        .invoke(
            &ctx,
            "write-soul",
            &serde_json::json!({"content": soul_content}),
        )
        .await?;
    assert!(result.contains("Soul updated"));

    // Test 3: Read written soul
    let result = soul_tool
        .invoke(
            &ctx,
            "read-soul",
            &serde_json::json!({}),
        )
        .await?;
    assert!(result.contains("TestBot"));
    assert!(result.contains("English"));
    assert!(result.contains("Friendly"));

    // Test 4: Token limit enforcement
    // Use words to generate realistic tokens (tiktoken typically counts ~1 token per word)
    let words: Vec<String> = (0..600).map(|i| format!("word{} ", i)).collect();
    let long_content = words.join("");
    let result = soul_tool
        .invoke(
            &ctx,
            "write-soul",
            &serde_json::json!({"content": long_content}),
        )
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("too long"));

    Ok(())
}

#[tokio::test]
async fn test_soul_segment_rendering() -> Result<()> {
    use agent_core::SystemPromptSegment;

    // Create a temporary directory for testing
    let temp_dir = tempfile::tempdir()?;
    let data_store = Rc::new(agent_core::DataStore::new(temp_dir.path().to_path_buf()));
    let soul_node = data_store.root_dir().node("soul");

    let soul_state = SoulState::new(soul_node);

    // Build a minimal session and context
    let runtime = Rc::new(RuntimeBuilder::new().build());
    let session = SessionBuilder::new(runtime).build()?;
    let ctx = AgentContextBuilder::from_session(&session).build()?;

    // Test 1: Empty soul renders nothing
    let segment = agent_bot::SoulSegment::new(soul_state.clone());
    let rendered = segment.render(&ctx).await?;
    assert!(rendered.is_empty());

    // Test 2: With content, renders properly
    soul_state.set("I am TestBot.\n\nNative Language: English\n\nPersonality: Helpful.\n\nCapabilities: Testing.".to_string());
    soul_state.flush().await?;

    let rendered = segment.render(&ctx).await?;
    assert!(rendered.contains("## Soul (Who I Am)"));
    assert!(rendered.contains("TestBot"));
    assert!(rendered.contains("Helpful"));
    assert!(rendered.contains("tokens"));

    Ok(())
}
