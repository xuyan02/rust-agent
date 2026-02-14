use crate::Result;
use anyhow::{Context, Result as AnyhowResult};
use std::path::{Path, PathBuf};

pub struct Session<'a> {
    runtime: &'a crate::Runtime,
    workspace_path: PathBuf,
    agent_path: PathBuf,
    default_model: String,
    tools: Vec<Box<dyn crate::tools::Tool>>,
}

impl Session<'_> {
    pub fn runtime(&self) -> &crate::Runtime {
        self.runtime
    }

    pub fn workspace_path(&self) -> &Path {
        &self.workspace_path
    }

    pub fn agent_path(&self) -> &Path {
        &self.agent_path
    }

    pub fn default_model(&self) -> &str {
        &self.default_model
    }

    pub fn tools(&self) -> &[Box<dyn crate::tools::Tool>] {
        &self.tools
    }
}

pub struct SessionBuilder<'a> {
    runtime: &'a crate::Runtime,
    workspace_path: Option<PathBuf>,
    agent_path: Option<PathBuf>,
    default_model: Option<String>,
    tools: Vec<Box<dyn crate::tools::Tool>>,
}

impl<'a> SessionBuilder<'a> {
    pub fn new(runtime: &'a crate::Runtime) -> Self {
        Self {
            runtime,
            workspace_path: None,
            agent_path: None,
            default_model: None,
            tools: vec![],
        }
    }
}

impl<'a> SessionBuilder<'a> {
    pub fn set_workspace_path(mut self, p: PathBuf) -> Self {
        self.workspace_path = Some(p);
        self
    }

    pub fn set_agent_path(mut self, p: PathBuf) -> Self {
        self.agent_path = Some(p);
        self
    }

    pub fn set_default_model(mut self, s: String) -> Self {
        self.default_model = Some(s);
        self
    }

    pub fn add_tool(mut self, tool: Box<dyn crate::tools::Tool>) -> Self {
        self.tools.push(tool);
        self
    }

    pub fn build(self) -> Result<Session<'a>> {
        let workspace_path = self
            .workspace_path
            .unwrap_or(std::env::current_dir().context("failed to get cwd")?);
        let agent_path = self
            .agent_path
            .unwrap_or_else(|| workspace_path.join(".agent"));
        let default_model = self.default_model.unwrap_or_default();

        Ok(Session {
            runtime: self.runtime,
            workspace_path,
            agent_path,
            default_model,
            tools: self.tools,
        })
    }
}

// Avoid leaking anyhow::Result in the public API of this module.
fn _typecheck_result(_: AnyhowResult<()>) {}
