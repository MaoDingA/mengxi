// llm/claude.rs — Anthropic Claude API provider with SSE streaming

use async_trait::async_trait;
use futures::StreamExt;
use reqwest_eventsource::{Event, RequestBuilderExt};
use serde_json::{json, Value};

use super::events::{LlmEvent, StopReason, Usage};
use super::provider::{LlmError, LlmProvider};
use super::{ChatRequest, EventStream};

/// Claude API (Anthropic) provider.
pub struct ClaudeProvider {
    api_key: String,
    base_url: String,
    client: reqwest::Client,
    model: String,
}

impl ClaudeProvider {
    pub fn new(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            base_url: "https://api.anthropic.com".to_string(),
            client: reqwest::Client::new(),
            model: "claude-sonnet-4-20250514".to_string(),
        }
    }

    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = url.trim_end_matches('/').to_string();
        self
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    /// Build the JSON body for the Claude Messages API.
    fn build_request_body(&self, request: &ChatRequest) -> Value {
        let mut body = json!({
            "model": request.model.as_deref().unwrap_or(&self.model),
            "max_tokens": request.max_tokens.unwrap_or(8192),
            "stream": true,
        });

        if let Some(temp) = request.temperature {
            body["temperature"] = json!(temp);
        }

        // System prompt
        if let Some(sys) = &request.system_prompt {
            body["system"] = json!(sys);
        }

        // Messages (exclude system from the message array)
        let messages: Vec<Value> = request
            .messages
            .iter()
            .filter_map(|m| {
                let role = match m.role {
                    super::Role::System => return None,
                    super::Role::User => "user",
                    super::Role::Assistant => "assistant",
                };
                Some(json!({
                    "role": role,
                    "content": serialize_content(&m.content),
                }))
            })
            .collect();
        body["messages"] = json!(messages);

        // Tools
        if !request.tools.is_empty() {
            let tools: Vec<Value> = request
                .tools
                .iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "input_schema": t.input_schema,
                    })
                })
                .collect();
            body["tools"] = json!(tools);
        }

        body
    }

    /// Parse a single SSE event from Claude.
    fn parse_sse_event(event_data: &str) -> Option<LlmEvent> {
        let parsed: Value = serde_json::from_str(event_data).ok()?;

        match parsed.get("type")?.as_str()? {
            "content_block_start" => {
                let block = parsed.get("content_block")?;
                match block.get("type")?.as_str()? {
                    "text" => {
                        // Text block starting — first delta will come separately
                        None
                    }
                    "tool_use" => {
                        let id = block.get("id")?.as_str()?.to_string();
                        let name = block.get("name")?.as_str()?.to_string();
                        Some(LlmEvent::ToolCallStart { id, name })
                    }
                    _ => None,
                }
            }
            "content_block_delta" => {
                let delta = parsed.get("delta")?;
                match delta.get("type")?.as_str()? {
                    "text_delta" => {
                        let text = delta.get("text")?.as_str()?.to_string();
                        Some(LlmEvent::TextDelta(text))
                    }
                    "input_json_delta" => {
                        let index = parsed.get("index")
                            .and_then(|i| i.as_u64())
                            .unwrap_or(0) as usize;
                        let fragment = delta.get("partial_json")?.as_str()?.to_string();
                        Some(LlmEvent::ToolCallDelta {
                            index,
                            delta: fragment,
                        })
                    }
                    _ => None,
                }
            }
            "content_block_stop" => {
                // A content block has finished — we'll emit ContentBlock in message_stop
                None
            }
            "message_delta" => {
                let delta = parsed.get("delta")?;
                let stop_reason_str = delta.get("stop_reason").and_then(|s| s.as_str()).unwrap_or("end_turn");
                let stop_reason = match stop_reason_str {
                    "end_turn" => StopReason::EndTurn,
                    "tool_use" => StopReason::ToolUse,
                    "max_tokens" => StopReason::MaxTokens,
                    "stop_sequence" => StopReason::StopSequence,
                    _ => StopReason::EndTurn,
                };

                let usage = parsed.get("usage").and_then(|u| {
                    Some(Usage {
                        input_tokens: 0,
                        output_tokens: u.get("output_tokens")?.as_u64()? as u32,
                    })
                });

                // input_tokens come from message_start, but we report output here
                Some(LlmEvent::Done {
                    stop_reason,
                    usage,
                })
            }
            "message_start" => {
                // Contains message ID and input usage — we can ignore for streaming
                None
            }
            "message_stop" => None,
            "ping" => None,
            _ => None,
        }
    }
}

/// Serialize MessageContent to Claude API format.
fn serialize_content(content: &[super::MessageContent]) -> Value {
    let blocks: Vec<Value> = content
        .iter()
        .map(|c| match c {
            super::MessageContent::Text { text } => json!({
                "type": "text",
                "text": text,
            }),
            super::MessageContent::ToolUse {
                tool_use_id,
                name,
                input,
            } => json!({
                "type": "tool_use",
                "id": tool_use_id,
                "name": name,
                "input": input,
            }),
            super::MessageContent::ToolResult {
                tool_use_id,
                content: result_content,
                is_error,
            } => json!({
                "type": "tool_result",
                "tool_use_id": tool_use_id,
                "content": result_content,
                "is_error": is_error,
            }),
        })
        .collect();
    json!(blocks)
}

#[async_trait]
impl LlmProvider for ClaudeProvider {
    async fn stream_chat(&self, request: ChatRequest) -> Result<EventStream, LlmError> {
        let body = self.build_request_body(&request);
        let url = format!("{}/v1/messages", self.base_url);

        let eventsource = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .eventsource()
            .map_err(|e| LlmError::ConnectionError(e.to_string()))?;

        let stream = eventsource
            .take_while(|result: &Result<Event, _>| {
                std::future::ready(result.is_ok())
            })
            .filter_map(|result| async move {
                match result {
                    Ok(Event::Open) => None,
                    Ok(Event::Message(event)) => {
                        if event.data == "[DONE]" {
                            return None;
                        }
                        Self::parse_sse_event(&event.data)
                    }
                    Err(_) => None,
                }
            })
            .map(Ok);

        Ok(Box::pin(stream))
    }

    fn name(&self) -> &str {
        "claude"
    }

    fn default_model(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_request_body_basic() {
        let provider = ClaudeProvider::new("test-key");
        let request = ChatRequest {
            messages: vec![
                super::super::Message::user("hello"),
            ],
            tools: vec![],
            model: None,
            max_tokens: Some(1024),
            temperature: Some(0.5),
            system_prompt: Some("You are helpful".to_string()),
        };

        let body = provider.build_request_body(&request);
        assert_eq!(body["model"], "claude-sonnet-4-20250514");
        assert_eq!(body["max_tokens"], 1024);
        assert_eq!(body["temperature"], 0.5);
        assert_eq!(body["system"], "You are helpful");
        assert!(body["stream"].as_bool().unwrap());
    }

    #[test]
    fn test_build_request_body_with_tools() {
        let provider = ClaudeProvider::new("test-key");
        let request = ChatRequest {
            messages: vec![super::super::Message::user("search for warm tones")],
            tools: vec![crate::tool::ToolDefinition {
                name: "search_by_color".to_string(),
                description: "Search by color description".to_string(),
                input_schema: serde_json::json!({"type": "object", "properties": {"query": {"type": "string"}}}),
            }],
            model: Some("claude-haiku-4-20250414".to_string()),
            max_tokens: None,
            temperature: None,
            system_prompt: None,
        };

        let body = provider.build_request_body(&request);
        assert_eq!(body["model"], "claude-haiku-4-20250414");
        let tools = body["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "search_by_color");
    }

    #[test]
    fn test_parse_sse_text_delta() {
        let event = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
        let result = ClaudeProvider::parse_sse_event(event);
        match result {
            Some(LlmEvent::TextDelta(text)) => assert_eq!(text, "Hello"),
            other => panic!("Expected TextDelta, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_sse_tool_call_start() {
        let event = r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_01","name":"search_by_color"}}"#;
        let result = ClaudeProvider::parse_sse_event(event);
        match result {
            Some(LlmEvent::ToolCallStart { id, name }) => {
                assert_eq!(id, "toolu_01");
                assert_eq!(name, "search_by_color");
            }
            other => panic!("Expected ToolCallStart, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_sse_done() {
        let event = r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":150}}"#;
        let result = ClaudeProvider::parse_sse_event(event);
        match result {
            Some(LlmEvent::Done { stop_reason, usage }) => {
                assert_eq!(stop_reason, StopReason::ToolUse);
                assert_eq!(usage.unwrap().output_tokens, 150);
            }
            other => panic!("Expected Done, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_sse_ignores_ping() {
        let event = r#"{"type":"ping"}"#;
        assert!(ClaudeProvider::parse_sse_event(event).is_none());
    }
}
