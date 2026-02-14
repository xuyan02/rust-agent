mod agent;
mod agent_context;
mod history;
mod runtime;
mod session;
mod tool_dispatch;

pub mod llm;
pub mod support;
mod system_prompt;
pub mod tools;

// Provide a stable path for proc-macro generated code (it references crate::Tool/ToolSpec/etc.).
pub use tools::{
    ArraySpec, BooleanSpec, FunctionSpec, IntegerSpec, NumberSpec, ObjectSpec, PropertySpec,
    StringSpec, Tool, ToolCall, ToolSpec, TypeSpec,
};

pub use agent::{Agent, LlmAgent};
pub use agent_context::{AgentContext, AgentContextBuilder, AgentContextParent, make_user_message};
pub use history::{History, InMemoryHistory};
pub use runtime::{Runtime, RuntimeBuilder};
pub use session::{Session, SessionBuilder};
pub use system_prompt::{StaticSystemPromptSegment, SystemPromptSegment};

use anyhow::Result;

// See README.md for tool ownership/precedence semantics.
pub use tool_dispatch::find_tool_for_function;
pub(crate) use tool_dispatch::parse_tool_calls;
