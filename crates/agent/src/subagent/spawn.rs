// subagent/spawn.rs — Subagent spawning, concurrency, and Tool implementation
//
// SubagentRuntime manages bounded concurrency and creates child Agent instances.
// SubagentTool implements the Tool trait so the parent agent can invoke subagents
// through the standard tool dispatch mechanism.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::Semaphore;

use crate::agent::{Agent, AgentConfig, AgentError};
use crate::llm::LlmProvider;
use crate::tool::{Tool, ToolError, ToolRegistry, ToolResult};

use super::definition::SubagentDefinition;

// ---------------------------------------------------------------------------
// ProviderFactory — abstraction for creating LLM providers
// ---------------------------------------------------------------------------

/// Factory trait for creating LLM providers for subagents.
///
/// This decouples the subagent module from concrete provider types,
/// allowing the CLI/TUI to inject the appropriate provider configuration.
pub trait ProviderFactory: Send + Sync {
    /// Create a new LLM provider.
    ///
    /// `model_override` allows the subagent definition to specify a
    /// different model than the provider's default.
    fn create_provider(&self, model_override: Option<&str>) -> Box<dyn LlmProvider>;
}

// ---------------------------------------------------------------------------
// SubagentRuntime — concurrency management and child agent creation
// ---------------------------------------------------------------------------

/// Manages subagent execution with bounded concurrency.
pub struct SubagentRuntime {
    provider_factory: Arc<dyn ProviderFactory>,
    parent_registry: Arc<ToolRegistry>,
    semaphore: Arc<Semaphore>,
}

impl SubagentRuntime {
    /// Create a new runtime.
    ///
    /// `max_concurrent` sets the upper bound on simultaneously running subagents.
    pub fn new(
        provider_factory: Arc<dyn ProviderFactory>,
        parent_registry: Arc<ToolRegistry>,
        max_concurrent: usize,
    ) -> Self {
        Self {
            provider_factory,
            parent_registry,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    /// Spawn and run a subagent with the given definition and user message.
    ///
    /// Acquires a permit from the semaphore (blocks if at capacity),
    /// creates a filtered tool registry and a child Agent, then runs it.
    pub async fn run_subagent(
        &self,
        definition: &SubagentDefinition,
        user_message: &str,
    ) -> Result<String, SubagentError> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| SubagentError::RuntimeError("Semaphore closed".into()))?;

        let sub_registry = self.build_sub_registry(definition)?;
        let provider = self.provider_factory.create_provider(definition.model.as_deref());

        let config = AgentConfig {
            max_turns: definition.max_turns,
            system_prompt: definition.system_prompt.clone(),
            ..AgentConfig::default()
        };

        let mut agent = Agent::new(provider, sub_registry).with_config(config);
        agent
            .run(user_message)
            .await
            .map_err(|e| SubagentError::AgentError(e.to_string()))
    }

    /// Build a filtered ToolRegistry containing only the tools in the definition.
    fn build_sub_registry(
        &self,
        definition: &SubagentDefinition,
    ) -> Result<ToolRegistry, SubagentError> {
        let mut registry = ToolRegistry::new();
        for tool_name in &definition.tools {
            let tool = self
                .parent_registry
                .get(tool_name)
                .ok_or_else(|| SubagentError::ToolNotFound(tool_name.clone()))?;
            registry.register(ArcTool(tool));
        }
        Ok(registry)
    }
}

// ---------------------------------------------------------------------------
// ArcTool — newtype wrapper to re-register Arc<dyn Tool>
// ---------------------------------------------------------------------------

/// Wrapper allowing `Arc<dyn Tool>` to be registered in a new `ToolRegistry`.
struct ArcTool(Arc<dyn Tool>);

#[async_trait]
impl Tool for ArcTool {
    fn name(&self) -> &str {
        self.0.name()
    }

    fn description(&self) -> &str {
        self.0.description()
    }

    fn parameters(&self) -> Value {
        self.0.parameters()
    }

    async fn execute(&self, params: Value) -> Result<ToolResult, ToolError> {
        self.0.execute(params).await
    }
}

// ---------------------------------------------------------------------------
// SubagentTool — Tool trait implementation for parent agent dispatch
// ---------------------------------------------------------------------------

/// A subagent exposed as a tool to the parent agent.
pub struct SubagentTool {
    tool_name_str: String,
    description_str: String,
    definition: SubagentDefinition,
    runtime: Arc<SubagentRuntime>,
}

impl SubagentTool {
    pub fn new(definition: SubagentDefinition, runtime: Arc<SubagentRuntime>) -> Self {
        let tool_name_str = format!("subagent_{}", definition.name);
        let prompt_preview: String = definition
            .system_prompt
            .lines()
            .take(3)
            .collect::<Vec<_>>()
            .join(" ");
        let description_str = format!(
            "Delegate to the {} subagent. {}",
            definition.name, prompt_preview
        );
        Self {
            tool_name_str,
            description_str,
            definition,
            runtime,
        }
    }
}

#[async_trait]
impl Tool for SubagentTool {
    fn name(&self) -> &str {
        &self.tool_name_str
    }

    fn description(&self) -> &str {
        &self.description_str
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": format!("Task to delegate to the {} subagent", self.definition.name)
                }
            },
            "required": ["message"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult, ToolError> {
        let message = params
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::InvalidParams("Missing required parameter: message".into())
            })?;

        match self.runtime.run_subagent(&self.definition, message).await {
            Ok(result) => Ok(ToolResult::ok(result)),
            Err(SubagentError::AgentError(msg)) => Ok(ToolResult::err(format!(
                "Subagent '{}' failed: {}",
                self.definition.name, msg
            ))),
            Err(SubagentError::ToolNotFound(tool)) => Ok(ToolResult::err(format!(
                "Subagent '{}' tool not found: {}",
                self.definition.name, tool
            ))),
            Err(SubagentError::RuntimeError(msg)) => Ok(ToolResult::err(format!(
                "Subagent '{}' runtime error: {}",
                self.definition.name, msg
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// SubagentError
// ---------------------------------------------------------------------------

/// Errors from subagent execution.
#[derive(Debug, thiserror::Error)]
pub enum SubagentError {
    #[error("SUBAGENT_AGENT_ERROR -- {0}")]
    AgentError(String),
    #[error("SUBAGENT_TOOL_NOT_FOUND -- {0}")]
    ToolNotFound(String),
    #[error("SUBAGENT_RUNTIME_ERROR -- {0}")]
    RuntimeError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{ChatRequest, EventStream, LlmError, LlmEvent, StopReason};
    use futures::StreamExt;

    struct MockProvider;

    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn stream_chat(&self, _request: ChatRequest) -> Result<EventStream, LlmError> {
            let stream = futures::stream::once(async move {
                Ok(LlmEvent::TextDelta("Mock response".into()))
            })
            .chain(futures::stream::once(async move {
                Ok(LlmEvent::Done {
                    stop_reason: StopReason::EndTurn,
                    usage: None,
                })
            }));
            Ok(Box::pin(stream))
        }
        fn name(&self) -> &str {
            "mock"
        }
        fn default_model(&self) -> &str {
            "mock-model"
        }
    }

    struct MockProviderFactory;

    impl ProviderFactory for MockProviderFactory {
        fn create_provider(&self, _model_override: Option<&str>) -> Box<dyn LlmProvider> {
            Box::new(MockProvider)
        }
    }

    fn make_definition() -> SubagentDefinition {
        SubagentDefinition {
            name: "test".into(),
            model: None,
            tools: vec![],
            system_prompt: "You are a test agent.".into(),
            max_turns: 5,
        }
    }

    #[test]
    fn test_subagent_tool_name() {
        let defn = make_definition();
        let registry = Arc::new(ToolRegistry::new());
        let runtime = Arc::new(SubagentRuntime::new(
            Arc::new(MockProviderFactory),
            registry,
            3,
        ));
        let tool = SubagentTool::new(defn, runtime);
        assert_eq!(tool.name(), "subagent_test");
    }

    #[test]
    fn test_subagent_tool_parameters() {
        let defn = make_definition();
        let registry = Arc::new(ToolRegistry::new());
        let runtime = Arc::new(SubagentRuntime::new(
            Arc::new(MockProviderFactory),
            registry,
            3,
        ));
        let tool = SubagentTool::new(defn, runtime);
        let params = tool.parameters();
        assert_eq!(params["type"], "object");
        assert!(params["properties"]["message"].is_object());
    }

    #[tokio::test]
    async fn test_run_subagent_with_mock() {
        let defn = make_definition();
        let registry = Arc::new(ToolRegistry::new());
        let runtime = SubagentRuntime::new(
            Arc::new(MockProviderFactory),
            registry,
            3,
        );
        let result = runtime.run_subagent(&defn, "hello").await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Mock response"));
    }

    #[tokio::test]
    async fn test_run_subagent_missing_tool() {
        let defn = SubagentDefinition {
            name: "broken".into(),
            model: None,
            tools: vec!["nonexistent_tool".into()],
            system_prompt: "test".into(),
            max_turns: 3,
        };
        let registry = Arc::new(ToolRegistry::new());
        let runtime = SubagentRuntime::new(Arc::new(MockProviderFactory), registry, 3);
        let result = runtime.run_subagent(&defn, "hello").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            SubagentError::ToolNotFound(name) => assert_eq!(name, "nonexistent_tool"),
            other => panic!("Expected ToolNotFound, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_subagent_tool_execute_success() {
        let defn = make_definition();
        let registry = Arc::new(ToolRegistry::new());
        let runtime = Arc::new(SubagentRuntime::new(
            Arc::new(MockProviderFactory),
            registry,
            3,
        ));
        let tool = SubagentTool::new(defn, runtime);
        let result = tool
            .execute(json!({"message": "analyze this project"}))
            .await;
        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(!tool_result.is_error);
        assert!(tool_result.content.contains("Mock response"));
    }
}
