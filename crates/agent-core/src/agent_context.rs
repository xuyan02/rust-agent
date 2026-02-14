use crate::llm::ChatMessage;
use crate::tools::Tool;
use crate::{History, Session};

pub enum AgentContextParent<'a> {
    Session(&'a Session<'a>),
    Context(&'a AgentContext<'a>),
}

pub struct AgentContext<'a> {
    parent: AgentContextParent<'a>,
    history: Option<Box<dyn History + 'a>>,
    system_prompt_segments: Vec<Box<dyn crate::SystemPromptSegment>>,
    tools: Vec<Box<dyn Tool>>,
}

impl<'a> AgentContext<'a> {
    pub fn session(&self) -> &Session<'a> {
        match &self.parent {
            AgentContextParent::Session(s) => s,
            AgentContextParent::Context(c) => c.session(),
        }
    }

    pub fn parent(&self) -> &AgentContextParent<'a> {
        &self.parent
    }

    pub fn history(&self) -> &dyn History {
        if let Some(h) = self.history.as_deref() {
            return h;
        }

        match &self.parent {
            AgentContextParent::Session(s) => s.history(),
            AgentContextParent::Context(c) => c.history(),
        }
    }

    pub fn system_prompt_segments(&self) -> Vec<&dyn crate::SystemPromptSegment> {
        let mut out: Vec<&dyn crate::SystemPromptSegment> = self
            .system_prompt_segments
            .iter()
            .map(|s| s.as_ref())
            .collect();

        let mut cur = &self.parent;
        loop {
            match cur {
                AgentContextParent::Session(s) => {
                    out.extend(s.system_prompt_segments().iter().map(|p| p.as_ref()));
                    break;
                }
                AgentContextParent::Context(c) => {
                    out.extend(c.system_prompt_segments.iter().map(|p| p.as_ref()));
                    cur = c.parent();
                }
            }
        }

        out
    }

    /// Tools visible from this context (local first, then parent chain).
    pub fn tools(&self) -> Vec<&dyn Tool> {
        let mut out: Vec<&dyn Tool> = self.tools.iter().map(|t| t.as_ref()).collect();

        let mut cur = &self.parent;
        loop {
            match cur {
                AgentContextParent::Session(s) => {
                    out.extend(s.tools().iter().map(|t| t.as_ref()));
                    break;
                }
                AgentContextParent::Context(c) => {
                    // NB: We intentionally include the parent's *local* tools here.
                    out.extend(c.tools.iter().map(|t| t.as_ref()));
                    cur = c.parent();
                }
            }
        }

        out
    }
}

pub struct AgentContextBuilder<'a> {
    parent: AgentContextParent<'a>,
    history: Option<Box<dyn History + 'a>>,
    system_prompt_segments: Vec<Box<dyn crate::SystemPromptSegment>>,
    tools: Vec<Box<dyn Tool>>,
}

impl<'a> AgentContextBuilder<'a> {
    pub fn new(parent: AgentContextParent<'a>) -> Self {
        Self {
            parent,
            history: None,
            system_prompt_segments: vec![],
            tools: vec![],
        }
    }

    pub fn from_session(session: &'a Session<'a>) -> Self {
        Self::new(AgentContextParent::Session(session))
    }

    pub fn from_parent_ctx(parent: &'a AgentContext<'a>) -> Self {
        Self::new(AgentContextParent::Context(parent))
    }

    pub fn add_system_prompt_segment(mut self, seg: Box<dyn crate::SystemPromptSegment>) -> Self {
        self.system_prompt_segments.push(seg);
        self
    }

    pub fn add_system_segment(self, seg: String) -> Self {
        if seg.is_empty() {
            return self;
        }
        self.add_system_prompt_segment(Box::new(crate::StaticSystemPromptSegment::new(seg)))
    }

    pub fn set_history(mut self, history: Box<dyn History + 'a>) -> Self {
        self.history = Some(history);
        self
    }

    pub fn add_tool(mut self, tool: Box<dyn Tool>) -> Self {
        self.tools.push(tool);
        self
    }

    pub fn build(self) -> anyhow::Result<AgentContext<'a>> {
        Ok(AgentContext {
            parent: self.parent,
            history: self.history,
            system_prompt_segments: self.system_prompt_segments,
            tools: self.tools,
        })
    }
}

pub fn make_user_message(line: String) -> ChatMessage {
    ChatMessage::user_text(line)
}
