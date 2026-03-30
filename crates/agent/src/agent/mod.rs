// agent/mod.rs — Agent loop: core orchestration

mod config;
mod events;
mod state;

pub use config::AgentConfig;
pub use events::AgentEvent;
pub use state::AgentState;

use tokio::sync::mpsc;
use futures::StreamExt;

use crate::llm::{LlmProvider, ChatRequest, Message, LlmEvent, MessageContent};
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

/// Tracks an in-progress tool call being assembled from streaming deltas.
struct PendingToolCall {
    id: String,
    name: String,
    arguments: String,
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

        let system_prompt = self.build_system_prompt();
        let tool_defs = self.registry.tool_definitions();
        let mut final_response = String::new();

        let _ = self.event_tx.send(AgentEvent::Started);

        // Agent loop: LLM → tool calls → results → repeat
        for turn in 0..self.config.max_turns {
            let _ = self.event_tx.send(AgentEvent::TurnStart { turn });

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

            // Collect text and tool calls from the stream
            let mut text_response = String::new();
            let mut pending_tool_calls: Vec<PendingToolCall> = Vec::new();

            while let Some(event) = stream.next().await {
                match event {
                    Ok(LlmEvent::TextDelta(delta)) => {
                        text_response.push_str(&delta);
                        let _ = self.event_tx.send(AgentEvent::TextDelta(delta));
                    }
                    Ok(LlmEvent::ToolCallStart { id, name }) => {
                        let _ = self.event_tx.send(AgentEvent::ToolCallStart {
                            name: name.clone(),
                            call_id: id.clone(),
                        });
                        pending_tool_calls.push(PendingToolCall {
                            id,
                            name,
                            arguments: String::new(),
                        });
                    }
                    Ok(LlmEvent::ToolCallDelta { id, delta }) => {
                        // Append argument fragment to the matching pending tool call
                        if let Some(tc) = pending_tool_calls.iter_mut().find(|tc| tc.id == id) {
                            tc.arguments.push_str(&delta);
                        }
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

            self.state.advance_turn();

            // If no tool calls, we're done — the LLM gave a final text answer
            if pending_tool_calls.is_empty() {
                final_response = text_response;
                break;
            }

            // Add the assistant message (text + tool_use blocks) to history
            let mut assistant_content: Vec<MessageContent> = Vec::new();
            if !text_response.is_empty() {
                assistant_content.push(MessageContent::Text { text: text_response });
            }
            // Note: tool_use blocks aren't stored in MessageContent currently,
            // but the tool results need to reference the call IDs.

            self.state.add_message(Message {
                role: crate::llm::Role::Assistant,
                content: if assistant_content.is_empty() {
                    vec![MessageContent::Text { text: String::new() }]
                } else {
                    assistant_content
                },
            });

            // Execute tool calls and collect results
            let mut tool_results: Vec<MessageContent> = Vec::new();

            for tc in &pending_tool_calls {
                let params: serde_json::Value = serde_json::from_str(&tc.arguments)
                    .unwrap_or(serde_json::Value::Object(Default::default()));

                let result = self.registry.dispatch(&tc.name, params).await;

                let (content, is_error) = match result {
                    Ok(tool_result) => {
                        let summary = events::ToolResultSummary {
                            success: !tool_result.is_error,
                            content_preview: tool_result.content.chars().take(200).collect(),
                        };
                        let _ = self.event_tx.send(AgentEvent::ToolCallEnd {
                            name: tc.name.clone(),
                            call_id: tc.id.clone(),
                            result: summary,
                        });
                        (tool_result.content, tool_result.is_error)
                    }
                    Err(e) => {
                        let error_msg = e.to_string();
                        let summary = events::ToolResultSummary {
                            success: false,
                            content_preview: error_msg.chars().take(200).collect(),
                        };
                        let _ = self.event_tx.send(AgentEvent::ToolCallEnd {
                            name: tc.name.clone(),
                            call_id: tc.id.clone(),
                            result: summary,
                        });
                        (error_msg, true)
                    }
                };

                tool_results.push(MessageContent::ToolResult {
                    tool_use_id: tc.id.clone(),
                    content,
                    is_error,
                });
            }

            // Add tool results as a user message (required by Claude API convention)
            self.state.add_message(Message {
                role: crate::llm::Role::User,
                content: tool_results,
            });

            // Check if we're about to exceed max turns
            if turn + 1 >= self.config.max_turns {
                return Err(AgentError::MaxTurnsReached(self.config.max_turns));
            }
        }

        let _ = self.event_tx.send(AgentEvent::Done {
            response: final_response.clone(),
        });

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
