use agent_core::llm::ChatMessage;
use agent_core::{History, InMemoryHistory, AgentContextBuilder, SessionBuilder};
use anyhow::Result;
use std::rc::Rc;

#[tokio::test]
async fn in_memory_history_basic() -> Result<()> {
    // Create minimal runtime and session for AgentContext
    let runtime = Rc::new(agent_core::RuntimeBuilder::new().build());
    let session = SessionBuilder::new(runtime)
        .set_default_model("test-model".to_string())
        .build()?;
    let ctx = AgentContextBuilder::from_session(&session).build()?;

    let h = InMemoryHistory::new();
    assert!(h.last(&ctx).await?.is_none());

    h.append(&ctx, ChatMessage::user_text("hi")).await?;
    assert_eq!(h.last(&ctx).await?.unwrap(), ChatMessage::user_text("hi"));

    let all = h.get_all(&ctx).await?;
    assert_eq!(all.len(), 1);
    assert_eq!(all[0], ChatMessage::user_text("hi"));
    Ok(())
}
