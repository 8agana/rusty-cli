use super::{Tool, ToolSpec};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};

pub struct ReadFile;

impl Tool for ReadFile {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "read_file".into(),
            description: "Read a small text file from disk and return its contents".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to the text file" },
                    "max_bytes": { "type": "integer", "minimum": 1, "maximum": 1048576, "default": 65536 }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
            read_only: true,
        }
    }

    fn call(&self, args: &Value) -> Result<Value> {
        let path = args.get("path").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("missing 'path'"))?;
        let max = args.get("max_bytes").and_then(|v| v.as_u64()).unwrap_or(65536) as usize;
        let data = std::fs::read(path)?;
        let truncated = if data.len() > max { &data[..max] } else { &data[..] };
        let text = String::from_utf8_lossy(truncated).to_string();
        Ok(json!({ "path": path, "bytes": truncated.len(), "truncated": data.len() > max, "content": text }))
    }
}
