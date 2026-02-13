use crate::{History, InMemoryHistory, Session};
use agent_llm::ChatMessage;
use agent_tools::Tool;

pub struct AgentContext<'a> {
    session: &'a Session<'a>,
    history: Box<dyn History>,
    system_segments: Vec<String>,
    tools: Vec<Box<dyn Tool>>,
}

impl<'a> AgentContext<'a> {
    pub fn session(&self) -> &Session<'a> {
        self.session
    }

    pub fn history(&self) -> &dyn History {
        self.history.as_ref()
    }

    pub fn system_segments(&self) -> &[String] {
        &self.system_segments
    }

    pub fn tools(&self) -> &[Box<dyn Tool>] {
        &self.tools
    }
}

pub struct AgentContextBuilder<'a> {
    session: &'a Session<'a>,
    history: Option<Box<dyn History>>,
    system_segments: Vec<String>,
    tools: Vec<Box<dyn Tool>>,
}

impl<'a> AgentContextBuilder<'a> {
    pub fn new(session: &'a Session<'a>) -> Self {
        Self {
            session,
            history: None,
            system_segments: vec![],
            tools: vec![],
        }
    }

    pub fn add_system_segment(mut self, seg: String) -> Self {
        if !seg.is_empty() {
            self.system_segments.push(seg);
        }
        self
    }

    pub fn set_history(mut self, history: Box<dyn History>) -> Self {
        self.history = Some(history);
        self
    }

    pub fn add_tool(mut self, tool: Box<dyn Tool>) -> Self {
        self.tools.push(tool);
        self
    }

    pub fn build(self) -> anyhow::Result<AgentContext<'a>> {
        let history: Box<dyn History> = self
            .history
            .unwrap_or_else(|| Box::new(InMemoryHistory::new()));

        Ok(AgentContext {
            session: self.session,
            history,
            system_segments: self.system_segments,
            tools: self.tools,
        })
    }
}

pub fn make_user_message(line: String) -> ChatMessage {
    ChatMessage::user_text(line)
}
