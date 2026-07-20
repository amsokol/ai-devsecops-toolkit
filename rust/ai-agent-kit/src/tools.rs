//! Tool registry over protobuf [`ToolSpec`] definitions.

use std::collections::HashMap;
use std::sync::Arc;

use api::aiagentkit::v1::ToolSpec;

/// Errors from tool execution.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("unknown tool: {0}")]
    Unknown(String),

    #[error("tool `{name}` failed: {message}")]
    Failed { name: String, message: String },
}

/// A callable tool registered with the agent.
pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;

    /// Execute with the raw JSON arguments string from the model.
    fn call(&self, arguments_json: &str) -> Result<String, ToolError>;
}

/// Name → tool lookup used by [`crate::run_agent`].
#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: impl Tool + 'static) {
        let spec = tool.spec();
        let name = spec.name.clone().unwrap_or_default();
        self.tools.insert(name, Arc::new(tool));
    }

    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools.values().map(|t| t.spec()).collect()
    }

    pub fn call(&self, name: &str, arguments_json: &str) -> Result<String, ToolError> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| ToolError::Unknown(name.to_owned()))?;
        tool.call(arguments_json)
    }
}
