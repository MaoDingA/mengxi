// tool/registry.rs — Tool registration and dispatch

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use super::{Concurrency, Tool, ToolDefinition, ToolError, ToolResult};

/// Registry of available tools for the agent.
#[derive(Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool.
    pub fn register(&mut self, tool: impl Tool + 'static) {
        self.tools.insert(tool.name().to_string(), Arc::new(tool));
    }

    /// Get a tool by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// List all registered tools.
    pub fn list(&self) -> Vec<Arc<dyn Tool>> {
        self.tools.values().cloned().collect()
    }

    /// Generate tool definitions for the LLM.
    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .map(|t| ToolDefinition {
                name: t.name().to_string(),
                description: t.description().to_string(),
                input_schema: t.parameters(),
            })
            .collect()
    }

    /// Dispatch a tool call by name.
    pub async fn dispatch(&self, name: &str, params: Value) -> Result<ToolResult, ToolError> {
        let tool = self.get(name).ok_or_else(|| ToolError::NotFound(name.to_string()))?;
        tool.execute(params).await
    }

    /// Get the concurrency mode for a tool.
    pub fn concurrency(&self, name: &str) -> Concurrency {
        self.get(name).map(|t| t.concurrency()).unwrap_or(Concurrency::Shared)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str { "echo" }
        fn description(&self) -> &str { "Echo back the input" }
        fn parameters(&self) -> Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string" }
                },
                "required": ["message"]
            })
        }
        async fn execute(&self, params: Value) -> Result<ToolResult, ToolError> {
            let msg = params.get("message")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidParams("missing 'message'".to_string()))?;
            Ok(ToolResult::ok(msg.to_string()))
        }
    }

    #[tokio::test]
    async fn test_registry_dispatch() {
        let mut registry = ToolRegistry::new();
        registry.register(EchoTool);

        let result = registry.dispatch("echo", serde_json::json!({"message": "hello"})).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().content, "hello");
    }

    #[tokio::test]
    async fn test_registry_not_found() {
        let registry = ToolRegistry::new();
        let result = registry.dispatch("nonexistent", serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_tool_definitions() {
        let mut registry = ToolRegistry::new();
        registry.register(EchoTool);

        let defs = registry.tool_definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "echo");
    }
}
