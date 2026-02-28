use crate::data_store::DirNode;
use crate::llm::ChatMessage;
use crate::tools::Tool;
use crate::{History, Session};
use std::rc::Rc;

pub enum AgentContextParent<'a> {
    Session(&'a Session),
    Context(&'a AgentContext<'a>),
}

pub struct AgentContext<'a> {
    parent: AgentContextParent<'a>,
    history: Option<Box<dyn History + 'a>>,
    system_prompt_segments: Vec<Box<dyn crate::SystemPromptSegment>>,
    tools: Vec<Box<dyn Tool>>,
    disable_tools: bool,
    tool_whitelist: Option<Vec<String>>,
    dir_node: Option<Rc<DirNode>>,
}

impl<'a> AgentContext<'a> {
    pub fn session(&self) -> &Session {
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

    /// Get the directory node for this context's data storage.
    /// Falls back to parent's dir_node if not set locally.
    /// Eventually falls back to Session's dir_node.
    pub fn dir_node(&self) -> Option<Rc<DirNode>> {
        if let Some(ref dir) = self.dir_node {
            return Some(Rc::clone(dir));
        }

        match &self.parent {
            AgentContextParent::Session(s) => s.dir_node(),
            AgentContextParent::Context(c) => c.dir_node(),
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
    /// If a tool whitelist is set, only tools with IDs in the whitelist are returned.
    pub fn tools(&self) -> Vec<&dyn Tool> {
        // If tools are disabled, return empty list
        if self.disable_tools {
            return vec![];
        }

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

        // Apply tool whitelist if set
        if let Some(ref whitelist) = self.tool_whitelist {
            out.retain(|tool| whitelist.contains(&tool.spec().id));
        }

        out
    }
}

pub struct AgentContextBuilder<'a> {
    parent: AgentContextParent<'a>,
    history: Option<Box<dyn History + 'a>>,
    system_prompt_segments: Vec<Box<dyn crate::SystemPromptSegment>>,
    tools: Vec<Box<dyn Tool>>,
    disable_tools: bool,
    tool_whitelist: Option<Vec<String>>,
    dir_node: Option<Rc<DirNode>>,
}

impl<'a> AgentContextBuilder<'a> {
    pub fn new(parent: AgentContextParent<'a>) -> Self {
        Self {
            parent,
            history: None,
            system_prompt_segments: vec![],
            tools: vec![],
            disable_tools: false,
            tool_whitelist: None,
            dir_node: None,
        }
    }

    pub fn from_session(session: &'a Session) -> Self {
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

    pub fn disable_tools(mut self) -> Self {
        self.disable_tools = true;
        self
    }

    /// Set a whitelist of tool IDs. Only tools with IDs in this list will be available.
    /// If the whitelist is empty, no tools will be available.
    pub fn set_tool_whitelist(mut self, whitelist: Vec<String>) -> Self {
        self.tool_whitelist = Some(whitelist);
        self
    }

    pub fn set_dir_node(mut self, dir_node: Rc<DirNode>) -> Self {
        self.dir_node = Some(dir_node);
        self
    }

    pub fn build(self) -> anyhow::Result<AgentContext<'a>> {
        Ok(AgentContext {
            parent: self.parent,
            history: self.history,
            system_prompt_segments: self.system_prompt_segments,
            tools: self.tools,
            disable_tools: self.disable_tools,
            tool_whitelist: self.tool_whitelist,
            dir_node: self.dir_node,
        })
    }
}

pub fn make_user_message(line: String) -> ChatMessage {
    ChatMessage::user_text(line)
}
