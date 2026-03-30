// subagent/mod.rs — Subagent framework for task decomposition
//
// Provides definition parsing (Markdown + YAML frontmatter),
// bounded concurrency runtime, and Tool trait integration
// so the parent agent can delegate tasks to specialized child agents.

mod definition;
mod spawn;

pub use definition::{SubagentDefinition, SubagentDefinitionError};
pub use spawn::{ProviderFactory, SubagentError, SubagentRuntime, SubagentTool};
