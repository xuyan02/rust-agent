use serde_json::Value;

/// Specification for a tool function
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionSpec {
    pub name: String,
    pub description: String,
    pub parameters: ObjectSpec,
}

/// Type specification for tool parameters
#[derive(Debug, Clone, PartialEq)]
pub enum TypeSpec {
    Array(ArraySpec),
    Object(ObjectSpec),
    String(StringSpec),
    Boolean(BooleanSpec),
    Integer(IntegerSpec),
    Number(NumberSpec),
}

/// Property specification for object types
#[derive(Debug, Clone, PartialEq)]
pub struct PropertySpec {
    pub name: String,
    pub ty: TypeSpec,
}

/// Object type specification
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectSpec {
    pub properties: Vec<PropertySpec>,
    pub required: Vec<String>,
    pub additional_properties: bool,
}

/// Array type specification
#[derive(Debug, Clone, PartialEq)]
pub struct ArraySpec {
    pub items: Box<TypeSpec>,
}

/// String type specification
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StringSpec {
    pub r#enum: Option<Vec<String>>,
}

/// Boolean type specification
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BooleanSpec {}

/// Integer type specification
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IntegerSpec {
    pub minimum: Option<i64>,
    pub maximum: Option<i64>,
}

/// Number type specification
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NumberSpec {
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
}

impl ObjectSpec {
    /// Convert object specification to JSON Schema value
    pub fn to_json_schema_value(&self) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        m.insert(
            "type".to_string(),
            serde_json::Value::String("object".to_string()),
        );

        let mut props = serde_json::Map::new();
        for p in &self.properties {
            props.insert(p.name.clone(), p.ty.to_json_schema_value());
        }
        m.insert("properties".to_string(), serde_json::Value::Object(props));

        let req = self
            .required
            .iter()
            .cloned()
            .map(serde_json::Value::String)
            .collect::<Vec<_>>();
        m.insert("required".to_string(), serde_json::Value::Array(req));

        m.insert(
            "additionalProperties".to_string(),
            serde_json::Value::Bool(self.additional_properties),
        );

        serde_json::Value::Object(m)
    }
}

impl TypeSpec {
    /// Convert type specification to JSON Schema value
    pub fn to_json_schema_value(&self) -> serde_json::Value {
        match self {
            TypeSpec::Object(o) => o.to_json_schema_value(),
            TypeSpec::Array(a) => serde_json::json!({
                "type": "array",
                "items": a.items.to_json_schema_value(),
            }),
            TypeSpec::String(s) => {
                let mut m = serde_json::Map::new();
                m.insert(
                    "type".to_string(),
                    serde_json::Value::String("string".to_string()),
                );
                if let Some(values) = &s.r#enum {
                    m.insert(
                        "enum".to_string(),
                        serde_json::Value::Array(
                            values
                                .iter()
                                .cloned()
                                .map(serde_json::Value::String)
                                .collect(),
                        ),
                    );
                }
                serde_json::Value::Object(m)
            }
            TypeSpec::Boolean(_) => serde_json::json!({"type": "boolean"}),
            TypeSpec::Integer(n) => {
                let mut m = serde_json::Map::new();
                m.insert(
                    "type".to_string(),
                    serde_json::Value::String("integer".to_string()),
                );
                if let Some(min) = n.minimum {
                    m.insert("minimum".to_string(), serde_json::Value::from(min));
                }
                if let Some(max) = n.maximum {
                    m.insert("maximum".to_string(), serde_json::Value::from(max));
                }
                serde_json::Value::Object(m)
            }
            TypeSpec::Number(n) => {
                let mut m = serde_json::Map::new();
                m.insert(
                    "type".to_string(),
                    serde_json::Value::String("number".to_string()),
                );
                if let Some(min) = n.minimum {
                    m.insert("minimum".to_string(), serde_json::Value::from(min));
                }
                if let Some(max) = n.maximum {
                    m.insert("maximum".to_string(), serde_json::Value::from(max));
                }
                serde_json::Value::Object(m)
            }
        }
    }
}

/// Tool specification containing metadata and functions
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub id: String,
    pub description: String,
    pub functions: Vec<FunctionSpec>,
}

/// Tool call representation
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub function_name: String,
    pub arguments: Value,
}