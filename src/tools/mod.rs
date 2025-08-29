use anyhow::Result;
use serde_json::Value;

#[derive(Clone)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub read_only: bool,
}

pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;
    fn call(&self, args: &Value) -> Result<Value>;
}

pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,    
}

impl ToolRegistry {
    pub fn new() -> Self { Self { tools: vec![] } }
    pub fn with_default() -> Self {
        let mut reg = Self::new();
        reg.register(Box::new(super::tools::read_file::ReadFile));
        reg.register(Box::new(super::tools::echo::Echo));
        reg
    }
    pub fn register(&mut self, tool: Box<dyn Tool>) { self.tools.push(tool); }
    pub fn list(&self) -> Vec<ToolSpec> { self.tools.iter().map(|t| t.spec()).collect() }
    pub fn get(&self, name: &str) -> Option<&Box<dyn Tool>> { self.tools.iter().find(|t| t.spec().name == name) }

    pub fn list_filtered(&self, allow: Option<&Vec<String>>, read_only_only: bool) -> Vec<ToolSpec> {
        let allow_set: Option<std::collections::HashSet<&str>> = allow.map(|v| v.iter().map(|s| s.as_str()).collect());
        self.tools
            .iter()
            .map(|t| t.spec())
            .filter(|spec| match &allow_set { Some(set) => set.contains(spec.name.as_str()), None => true })
            .filter(|spec| if read_only_only { spec.read_only } else { true })
            .collect()
    }
}

pub mod read_file;
pub mod echo;
pub mod mcp_tool;
