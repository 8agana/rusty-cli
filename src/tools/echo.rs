use super::{Tool, ToolSpec};
use anyhow::Result;
use serde_json::{Value, json};

pub struct Echo;

impl Tool for Echo {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "echo".into(),
            description: "Return the provided input for debugging".into(),
            parameters: json!({
                "type": "object",
                "properties": { "text": { "type": "string" } },
                "required": ["text"],
                "additionalProperties": false
            }),
            read_only: true,
        }
    }

    fn call(&self, args: &Value) -> Result<Value> {
        Ok(json!({ "echo": args }))
    }
}
