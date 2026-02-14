use agent_core::tools::Tool;
use anyhow::Result;

#[tokio::test]
async fn tool_macro_generates_spec_and_invoke() -> Result<()> {
    let t = agent_core::tools::MacroExampleTool;

    let spec = t.spec();
    assert_eq!(spec.id, "macro_example");

    let echo = spec
        .functions
        .iter()
        .find(|f| f.name == "macro-echo")
        .unwrap();
    let params_json = echo.parameters.to_json_schema_value();
    assert_eq!(params_json["type"].as_str(), Some("object"));
    assert_eq!(params_json["additionalProperties"].as_bool(), Some(false));
    assert!(params_json["properties"].get("text").is_some());

    let runtime = agent_core::RuntimeBuilder::new().build();
    let session = agent_core::SessionBuilder::new(&runtime)
        .set_workspace_path(std::path::PathBuf::from("/tmp"))
        .build()?;
    let ctx = agent_core::AgentContextBuilder::from_session(&session).build()?;

    let out = t
        .invoke(&ctx, "macro-echo", &serde_json::json!({"text": "hi"}))
        .await?;
    assert_eq!(out, "hi");

    let out = t.invoke(&ctx, "macro-pwd", &serde_json::json!({})).await?;
    assert_eq!(out, "/tmp");

    Ok(())
}

#[tokio::test]
async fn tool_macro_errors_are_stable() -> Result<()> {
    let t = agent_core::tools::MacroExampleTool;

    let runtime = agent_core::RuntimeBuilder::new().build();
    let session = agent_core::SessionBuilder::new(&runtime)
        .set_workspace_path(std::path::PathBuf::from("/tmp"))
        .build()?;
    let ctx = agent_core::AgentContextBuilder::from_session(&session).build()?;

    let err = t
        .invoke(&ctx, "macro-echo", &serde_json::json!({}))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("tool arg missing: text"));

    let err = t
        .invoke(&ctx, "macro-echo", &serde_json::json!({"text": 1}))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("tool arg type mismatch: text"));

    let err = t
        .invoke(
            &ctx,
            "macro-echo",
            &serde_json::json!({"text": "hi", "x": 1}),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("tool arg unknown: x"));

    Ok(())
}
