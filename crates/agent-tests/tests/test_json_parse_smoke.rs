use anyhow::Result;

#[test]
fn json_parse_smoke() -> Result<()> {
    let v = agent_core::support::json::parse(r#"{"a":1,"b":[true,false]}"#)?;
    assert_eq!(v["a"].as_i64(), Some(1));
    assert_eq!(v["b"].as_array().map(|a| a.len()), Some(2));

    let s = agent_core::support::json::dump(&v)?;
    let v2 = agent_core::support::json::parse(&s)?;
    assert_eq!(v, v2);
    Ok(())
}
