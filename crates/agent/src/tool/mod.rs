// tool/mod.rs — Tool harness: trait, registry, result types

mod registry;

pub use registry::ToolRegistry;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Tool definition exposed to the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// Result of a tool execution.
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub content: String,
    pub is_error: bool,
    /// Optional structured data for TUI rendering.
    pub display_data: Option<Value>,
}

impl ToolResult {
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
            display_data: None,
        }
    }

    pub fn err(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
            display_data: None,
        }
    }

    pub fn with_display(mut self, data: Value) -> Self {
        self.display_data = Some(data);
        self
    }
}

/// Error from tool execution.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("TOOL_PARAM_ERROR -- {0}")]
    InvalidParams(String),
    #[error("TOOL_EXEC_ERROR -- {0}")]
    ExecutionError(String),
    #[error("TOOL_NOT_FOUND -- {0}")]
    NotFound(String),
}

/// Concurrency mode for a tool.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Concurrency {
    /// Can run in parallel with other shared tools.
    Shared,
    /// Must run exclusively (no other tools concurrently).
    Exclusive,
}

/// A tool that the LLM can invoke.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique tool name.
    fn name(&self) -> &str;

    /// Human-readable description shown to the LLM.
    fn description(&self) -> &str;

    /// JSON Schema for the tool's input parameters.
    fn parameters(&self) -> Value;

    /// Concurrency mode.
    fn concurrency(&self) -> Concurrency {
        Concurrency::Shared
    }

    /// Execute the tool with validated parameters.
    async fn execute(&self, params: Value) -> Result<ToolResult, ToolError>;
}
