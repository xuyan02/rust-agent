/// Load a prompt from a markdown file at compile time and create a StaticSystemPromptSegment.
///
/// # Example
/// ```ignore
/// use agent_core::prompt;
///
/// // Returns StaticSystemPromptSegment
/// let segment = prompt!("../prompts/my_prompt.md");
///
/// // Use in AgentContextBuilder
/// builder.add_system_prompt_segment(Box::new(prompt!("../prompts/my_prompt.md")));
/// ```
#[macro_export]
macro_rules! prompt {
    ($path:expr) => {
        $crate::StaticSystemPromptSegment::new(include_str!($path).to_string())
    };
}

mod agent;
mod agent_context;
pub mod data_store;
mod history;
mod react_agent;
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
pub use data_store::{DataNode, DataStore, DirNode};
pub use history::{
    History, InMemoryHistory, PersistentHistory,
    estimate_tokens, estimate_message_tokens, estimate_messages_tokens,
};
pub use react_agent::ReActAgent;
pub use runtime::{LocalSpawner, Runtime, RuntimeBuilder};
pub use session::{Session, SessionBuilder};
pub use system_prompt::{StaticSystemPromptSegment, SystemPromptSegment};

use anyhow::Result;

// See README.md for tool ownership/precedence semantics.
pub use tool_dispatch::find_tool_for_function;
pub(crate) use tool_dispatch::parse_tool_calls;
