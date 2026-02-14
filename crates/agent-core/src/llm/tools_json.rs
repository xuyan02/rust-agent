use serde_json::Value;

pub fn tools_to_openai_json(tools: &[&dyn crate::tools::Tool]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();

    for t in tools {
        for f in &t.spec().functions {
            out.push(serde_json::json!({
                "type": "function",
                "function": {
                    "name": f.name,
                    "description": f.description,
                    "parameters": crate::TypeSpec::Object(f.parameters.clone()).to_json_schema_value(),
                }
            }));
        }
    }

    out
}
