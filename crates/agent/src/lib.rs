// mengxi-agent: AI agent infrastructure for mengxi
//
// Provides LLM provider abstraction, tool harness, agent loop,
// and session management for the mengxi AI agent.

pub mod llm;
pub mod tool;
pub mod agent;
pub mod session;
pub mod subagent;
pub mod tools;

// Re-export key types
pub use agent::{Agent, AgentConfig, AgentEvent, AgentError};
pub use session::{SessionStore, Compactor, CompactionConfig, Session, SessionError, Branch, BranchTreeNode, CompactionResult};
pub use subagent::{SubagentDefinition, SubagentDefinitionError, SubagentRuntime, SubagentTool, ProviderFactory};
pub use tool::{Tool, ToolRegistry, ToolResult, ToolError, ToolDefinition};
pub use tools::LutEditStore;
