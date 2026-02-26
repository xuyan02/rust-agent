use agent_core::tools;
use agent_core::tools::{tool, tool_fn};
use agent_core::{AgentContext, FunctionSpec, Tool, ToolSpec};
use anyhow::Result;

struct ToolA;

#[tool(id = "a", description = "a")]
impl ToolA {
    #[tool_fn(name = "x")]
    async fn x(&self) -> Result<String> {
        Ok("A".to_string())
    }
}

struct ToolB;

#[tool(id = "b", description = "b")]
impl ToolB {
    #[tool_fn(name = "x")]
    async fn x(&self) -> Result<String> {
        Ok("B".to_string())
    }
}

#[tokio::test]
async fn child_ctx_tools_precede_parent_ctx_and_session() -> Result<()> {
    let runtime = std::rc::Rc::new(agent_core::RuntimeBuilder::new().build());

    let session = agent_core::SessionBuilder::new(std::rc::Rc::clone(&runtime))
        .set_default_model("fake".to_string())
        .add_tool(Box::new(ToolA))
        .build()?;

    let parent = agent_core::AgentContextBuilder::from_session(&session)
        .add_tool(Box::new(ToolA))
        .build()?;

    let child = agent_core::AgentContextBuilder::from_parent_ctx(&parent)
        .add_tool(Box::new(ToolB))
        .build()?;

    let tools = child.tools();
    let tool = agent_core::find_tool_for_function(tools.as_slice(), "x").unwrap();
    let out = tool.invoke(&child, "x", &serde_json::json!({})).await?;
    assert_eq!(out, "B");

    Ok(())
}
