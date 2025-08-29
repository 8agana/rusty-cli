use super::{Tool, ToolSpec};
use crate::mcp::client::McpClient;
use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;

pub struct McpTool {
    client: McpClient,
    spec_: ToolSpec,
}

impl McpTool {
    pub fn new(client: McpClient, spec: ToolSpec) -> Self { Self { client, spec_: spec } }
}

impl Tool for McpTool {
    fn spec(&self) -> ToolSpec { self.spec_.clone() }
    fn call(&self, args: &Value) -> Result<Value> {
        // Call is async; block-on for MVP in CLI context
        tokio::runtime::Handle::current().block_on(async {
            self.client.call_tool(&self.spec_.name, args).await
        })
    }
}

