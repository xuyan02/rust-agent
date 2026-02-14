use agent_core::llm::{
    ChatContent, ChatMessage, ChatRole, LlmContext, LlmProvider, LlmRequest, LlmSender,
};
use anyhow::Result;
use std::sync::{Arc, Mutex};

struct FakeRequest {
    events: Arc<Mutex<Vec<String>>>,
    system: String,
    user: String,
}

impl Drop for FakeRequest {
    fn drop(&mut self) {
        self.events.lock().unwrap().push("disconnect".to_string());
    }
}

#[async_trait::async_trait(?Send)]
impl LlmRequest for FakeRequest {
    async fn run(&mut self) -> Result<ChatMessage> {
        self.events.lock().unwrap().push("connect".to_string());
        self.events
            .lock()
            .unwrap()
            .push(format!("send_system:{}", self.system));
        self.events
            .lock()
            .unwrap()
            .push(format!("send_user:{}", self.user));
        Ok(ChatMessage::assistant_text("ok"))
    }
}

struct FakeProvider {
    name: String,
    models: Vec<String>,
    events: Arc<Mutex<Vec<String>>>,
}

impl LlmProvider for FakeProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn supports_model(&self, model: &str) -> bool {
        self.models.iter().any(|m| m == model)
    }

    fn create_sender(&self, _model: &str) -> Result<Box<dyn LlmSender>> {
        anyhow::bail!("not used")
    }

    fn create_request<'a>(
        &'a self,
        model: &str,
        messages: Vec<ChatMessage>,
        _tools: Vec<&'a dyn agent_core::Tool>,
    ) -> Result<Box<dyn LlmRequest + 'a>> {
        self.events
            .lock()
            .unwrap()
            .push(format!("provider_create:{}:{}", self.name, model));

        let mut system = String::new();
        let mut user = String::new();
        for m in messages {
            match (m.role, m.content) {
                (ChatRole::System, ChatContent::Text(t)) => system = t,
                (ChatRole::User, ChatContent::Text(t)) => user = t,
                _ => {}
            }
        }

        Ok(Box::new(FakeRequest {
            events: self.events.clone(),
            system,
            user,
        }))
    }
}

#[tokio::test]
async fn llm_context_selects_first_provider_supporting_model() -> Result<()> {
    let mut ctx = LlmContext::new();
    ctx.clear();

    let events: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    ctx.register(Box::new(FakeProvider {
        name: "openai".to_string(),
        models: vec!["model-a".to_string()],
        events: events.clone(),
    }));
    ctx.register(Box::new(FakeProvider {
        name: "openai".to_string(),
        models: vec!["model-b".to_string()],
        events: events.clone(),
    }));
    ctx.register(Box::new(FakeProvider {
        name: "other".to_string(),
        models: vec!["model-a".to_string()],
        events: events.clone(),
    }));

    // Unknown model => None
    {
        let msgs = vec![ChatMessage::system_text(""), ChatMessage::user_text("hi")];
        let req = ctx.create("missing", msgs, vec![]);
        assert!(req.is_none());
    }

    // model-a picks FIRST provider supporting it
    {
        let msgs = vec![
            ChatMessage::system_text("sys"),
            ChatMessage::user_text("hello"),
        ];
        let mut req = ctx.create("model-a", msgs, vec![]).unwrap()?;
        let _ = req.run().await?;
    }

    // model-b served by second provider
    {
        let msgs = vec![
            ChatMessage::system_text(""),
            ChatMessage::user_text("world"),
        ];
        let mut req = ctx.create("model-b", msgs, vec![]).unwrap()?;
        let _ = req.run().await?;
    }

    let expected = vec![
        "provider_create:openai:model-a",
        "connect",
        "send_system:sys",
        "send_user:hello",
        "disconnect",
        "provider_create:openai:model-b",
        "connect",
        "send_system:",
        "send_user:world",
        "disconnect",
    ];

    assert_eq!(*events.lock().unwrap(), expected);
    Ok(())
}
