// agent/mod.rs — Agent loop: core orchestration

mod config;
mod events;
mod state;

pub use config::AgentConfig;
pub use events::AgentEvent;
pub use state::AgentState;

use tokio::sync::mpsc;
use futures::StreamExt;

use crate::llm::{LlmProvider, ChatRequest, Message, LlmEvent};
use crate::tool::ToolRegistry;

/// The mengxi agent — orchestrates LLM calls and tool dispatch.
pub struct Agent {
    provider: Box<dyn LlmProvider>,
    registry: ToolRegistry,
    config: AgentConfig,
    state: AgentState,
    event_tx: mpsc::UnboundedSender<AgentEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<AgentEvent>>,
}

impl Agent {
    /// Create a new agent with the given LLM provider and tool registry.
    pub fn new(provider: Box<dyn LlmProvider>, registry: ToolRegistry) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        Self {
            provider,
            registry,
            config: AgentConfig::default(),
            state: AgentState::new(),
            event_tx,
            event_rx: Some(event_rx),
        }
    }

    /// Configure the agent.
    pub fn with_config(mut self, config: AgentConfig) -> Self {
        self.config = config;
        self
    }

    /// Take ownership of the event receiver (can only be done once).
    pub fn take_events(&mut self) -> Option<mpsc::UnboundedReceiver<AgentEvent>> {
        self.event_rx.take()
    }

    /// Run the agent loop for a single user message.
    ///
    /// This sends the user message to the LLM, dispatches any tool calls,
    /// feeds results back, and repeats until the LLM produces a final text
    /// response with no tool calls.
    pub async fn run(&mut self, user_message: &str) -> Result<String, AgentError> {
        // Add user message to conversation history
        self.state.add_message(Message::user(user_message));

        // Build system prompt
        let system_prompt = self.build_system_prompt();
        let tool_defs = self.registry.tool_definitions();

        let mut final_response = String::new();

        // Agent loop: LLM → tool calls → results → repeat
        for _turn in 0..self.config.max_turns {
            let request = ChatRequest {
                messages: self.state.messages().to_vec(),
                tools: tool_defs.clone(),
                model: Some(self.provider.default_model().to_string()),
                max_tokens: Some(self.config.max_tokens),
                temperature: Some(self.config.temperature),
                system_prompt: Some(system_prompt.clone()),
            };

            // Stream the LLM response
            let mut stream = self.provider.stream_chat(request).await
                .map_err(|e| AgentError::LlmError(e.to_string()))?;

            // Collect response events
            let mut text_response = String::new();
            let mut tool_calls: Vec<crate::llm::ToolCall> = Vec::new(); // TODO: collect tool calls from stream

            while let Some(event) = stream.next().await {
                match event {
                    Ok(LlmEvent::TextDelta(delta)) => {
                        text_response.push_str(&delta);
                        let _ = self.event_tx.send(AgentEvent::TextDelta(delta));
                    }
                    Ok(LlmEvent::ContentBlock(_block)) => {
                        // Handle complete content blocks — future use
                    }
                    Ok(LlmEvent::Done { stop_reason, usage: _ }) => {
                        let _ = self.event_tx.send(AgentEvent::TurnEnd {
                            stop_reason: format!("{:?}", stop_reason),
                        });
                        break;
                    }
                    Ok(LlmEvent::Error(msg)) => {
                        return Err(AgentError::LlmError(msg));
                    }
                    Err(e) => {
                        return Err(AgentError::LlmError(e.to_string()));
                    }
                    _ => {}
                }
            }

            // If no tool calls, we're done
            if tool_calls.is_empty() {
                final_response = text_response;
                break;
            }

            // Execute tool calls
            // TODO: implement tool execution loop
        }

        Ok(final_response)
    }

    /// Build the system prompt with color science context and tool descriptions.
    fn build_system_prompt(&self) -> String {
        let mut prompt = self.config.system_prompt.clone();
        if let Some(extra) = &self.config.color_science_context {
            prompt.push_str("\n\n");
            prompt.push_str(extra);
        }
        prompt
    }
}

/// Agent errors.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("AGENT_LLM_ERROR -- {0}")]
    LlmError(String),
    #[error("AGENT_TOOL_ERROR -- {0}")]
    ToolError(String),
    #[error("AGENT_MAX_TURNS -- reached maximum turns ({0})")]
    MaxTurnsReached(usize),
    #[error("AGENT_ABORTED")]
    Aborted,
}
