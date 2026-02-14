use agent_core::llm::ChatMessage;
use agent_core::{History, InMemoryHistory};
use anyhow::Result;

#[tokio::test]
async fn in_memory_history_basic() -> Result<()> {
    let h = InMemoryHistory::new();
    assert!(h.last().await?.is_none());

    h.append(ChatMessage::user_text("hi")).await?;
    assert_eq!(h.last().await?.unwrap(), ChatMessage::user_text("hi"));

    let all = h.get_all().await?;
    assert_eq!(all.len(), 1);
    assert_eq!(all[0], ChatMessage::user_text("hi"));
    Ok(())
}
