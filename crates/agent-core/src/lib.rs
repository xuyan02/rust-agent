mod agent;
mod agent_context;
mod agent_runner;
mod config;
mod history;
mod recap_agent;
mod runtime;
mod session;
mod tool_dispatch;

pub use agent::{Agent, LlmAgent};
pub(crate) use agent::{ToolLoopOptions, run_tool_loop};
pub use agent_context::{AgentContext, AgentContextBuilder, make_user_message};
pub use agent_runner::{AgentRunner, RunnerConsole, RunnerConsoleAdapter};
pub use config::{runtime_from_agent_config, session_from_agent_config};
pub use history::{History, InMemoryHistory};
pub use recap_agent::ReCapAgent;
pub use runtime::{Runtime, RuntimeBuilder};
pub use session::{Session, SessionBuilder};

use anyhow::Result;

// See README.md for tool ownership/precedence semantics.
pub(crate) use tool_dispatch::{find_tool_for_function, parse_tool_calls};
