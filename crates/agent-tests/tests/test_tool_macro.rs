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
        .find(|f| f.name == "macro.echo")
        .unwrap();
    assert_eq!(echo.parameters["type"].as_str(), Some("object"));
    assert_eq!(
        echo.parameters["additionalProperties"].as_bool(),
        Some(false)
    );
    assert!(echo.parameters["properties"].get("text").is_some());

    let ws = std::path::Path::new("/tmp");
    let out = t
        .invoke(ws, "macro.echo", &serde_json::json!({"text": "hi"}))
        .await?;
    assert_eq!(out, "hi");

    let out = t.invoke(ws, "macro.pwd", &serde_json::json!({})).await?;
    assert_eq!(out, "/tmp");

    Ok(())
}

#[tokio::test]
async fn tool_macro_errors_are_stable() -> Result<()> {
    let t = agent_core::tools::MacroExampleTool;
    let ws = std::path::Path::new("/tmp");

    let err = t
        .invoke(ws, "macro.echo", &serde_json::json!({}))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("tool arg missing: text"));

    let err = t
        .invoke(ws, "macro.echo", &serde_json::json!({"text": 1}))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("tool arg type mismatch: text"));

    let err = t
        .invoke(ws, "macro.echo", &serde_json::json!({"text": "hi", "x": 1}))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("tool arg unknown: x"));

    Ok(())
}
