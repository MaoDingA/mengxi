// mengxi-agent: AI agent infrastructure for mengxi
//
// Provides LLM provider abstraction, tool harness, agent loop,
// and session management for the mengxi AI agent.

pub mod llm;
pub mod tool;
pub mod agent;

// Re-export key types
pub use agent::{Agent, AgentConfig, AgentEvent, AgentError};
pub use llm::{LlmProvider, LlmError, ChatRequest, Message, Role, EventStream};
pub use tool::{Tool, ToolRegistry, ToolResult, ToolError, ToolDefinition};
