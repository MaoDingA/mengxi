// llm/openai_compat.rs — OpenAI-compatible API provider (Ollama, OpenAI, vLLM, etc.)

use async_trait::async_trait;
use futures::StreamExt;
use reqwest_eventsource::{Event, RequestBuilderExt};
use serde_json::{json, Value};

use super::events::{LlmEvent, StopReason, Usage};
use super::provider::{LlmError, LlmProvider};
use super::{ChatRequest, EventStream};

/// OpenAI-compatible provider (works with Ollama, OpenAI, vLLM, LM Studio, etc.).
pub struct OpenAICompatProvider {
    base_url: String,
    api_key: Option<String>,
    client: reqwest::Client,
    model: String,
}

impl OpenAICompatProvider {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: None,
            client: reqwest::Client::new(),
            model: "gpt-4o".to_string(),
        }
    }

    pub fn with_api_key(mut self, key: &str) -> Self {
        self.api_key = Some(key.to_string());
        self
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    /// Convenience constructor for Ollama (localhost:11434).
    pub fn ollama() -> Self {
        Self::new("http://localhost:11434").with_model("llama3")
    }

    /// Build the JSON body for OpenAI chat completions API.
    fn build_request_body(&self, request: &ChatRequest) -> Value {
        let mut messages: Vec<Value> = Vec::new();

        // System prompt as a system message
        if let Some(sys) = &request.system_prompt {
            messages.push(json!({
                "role": "system",
                "content": sys,
            }));
        }

        // User/assistant messages
        for m in &request.messages {
            let role = match m.role {
                super::Role::System => "system",
                super::Role::User => "user",
                super::Role::Assistant => "assistant",
            };
            let content: Value = m.content.iter().map(|c| {
                match c {
                    super::MessageContent::Text { text } => json!(text),
                    super::MessageContent::ToolResult { content: result, .. } => json!(result),
                }
            }).collect::<Vec<_>>().into();
            messages.push(json!({
                "role": role,
                "content": content,
            }));
        }

        let mut body = json!({
            "model": request.model.as_deref().unwrap_or(&self.model),
            "messages": messages,
            "stream": true,
        });

        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = json!(max_tokens);
        }
        if let Some(temp) = request.temperature {
            body["temperature"] = json!(temp);
        }

        // Tools
        if !request.tools.is_empty() {
            let tools: Vec<Value> = request
                .tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.input_schema,
                        }
                    })
                })
                .collect();
            body["tools"] = json!(tools);
        }

        body
    }

    /// Parse a single SSE event from OpenAI-compatible API.
    fn parse_sse_event(event_data: &str) -> Option<LlmEvent> {
        let parsed: Value = serde_json::from_str(event_data).ok()?;

        let choices = parsed.get("choices")?.as_array()?;
        let choice = choices.first()?;

        // Check for finish_reason
        if let Some(finish) = choice.get("finish_reason").and_then(|f| f.as_str()) {
            if finish != "null" && finish != "stop" || finish == "stop" {
                let stop_reason = match finish {
                    "stop" => StopReason::EndTurn,
                    "tool_calls" => StopReason::ToolUse,
                    "length" => StopReason::MaxTokens,
                    "content_filter" => StopReason::StopSequence,
                    _ => StopReason::EndTurn,
                };
                let usage = parsed.get("usage").and_then(|u| {
                    Some(Usage {
                        input_tokens: u.get("prompt_tokens")?.as_u64()? as u32,
                        output_tokens: u.get("completion_tokens")?.as_u64()? as u32,
                    })
                });
                return Some(LlmEvent::Done { stop_reason, usage });
            }
        }

        let delta = choice.get("delta")?;

        // Text content
        if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
            if !content.is_empty() {
                return Some(LlmEvent::TextDelta(content.to_string()));
            }
        }

        // Tool calls
        if let Some(tool_calls) = delta.get("tool_calls").and_then(|t| t.as_array()) {
            if let Some(tc) = tool_calls.first() {
                if let Some(func) = tc.get("function") {
                    // Tool call start — has name
                    if let Some(name) = func.get("name").and_then(|n| n.as_str()) {
                        let id = tc.get("id")
                            .and_then(|i| i.as_str())
                            .unwrap_or("")
                            .to_string();
                        return Some(LlmEvent::ToolCallStart { id, name: name.to_string() });
                    }
                    // Tool call delta — argument fragment
                    if let Some(args) = func.get("arguments").and_then(|a| a.as_str()) {
                        let id = tc.get("id")
                            .and_then(|i| i.as_str())
                            .unwrap_or("")
                            .to_string();
                        return Some(LlmEvent::ToolCallDelta {
                            id,
                            delta: args.to_string(),
                        });
                    }
                }
            }
        }

        None
    }
}

#[async_trait]
impl LlmProvider for OpenAICompatProvider {
    async fn stream_chat(&self, request: ChatRequest) -> Result<EventStream, LlmError> {
        let body = self.build_request_body(&request);
        let url = format!("{}/v1/chat/completions", self.base_url);

        let mut req = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .json(&body);

        if let Some(key) = &self.api_key {
            req = req.header("authorization", format!("Bearer {}", key));
        }

        let eventsource = req
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
        "openai-compat"
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
        let provider = OpenAICompatProvider::new("http://localhost:11434");
        let request = ChatRequest {
            messages: vec![super::super::Message::user("hello")],
            tools: vec![],
            model: None,
            max_tokens: Some(1024),
            temperature: Some(0.7),
            system_prompt: Some("You are helpful".to_string()),
        };

        let body = provider.build_request_body(&request);
        assert_eq!(body["model"], "gpt-4o");
        assert_eq!(body["stream"], true);
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[1]["role"], "user");
    }

    #[test]
    fn test_build_request_body_with_tools() {
        let provider = OpenAICompatProvider::ollama();
        let request = ChatRequest {
            messages: vec![super::super::Message::user("search warm")],
            tools: vec![crate::tool::ToolDefinition {
                name: "search".to_string(),
                description: "Search items".to_string(),
                input_schema: serde_json::json!({"type": "object", "properties": {"q": {"type": "string"}}}),
            }],
            model: None,
            max_tokens: None,
            temperature: None,
            system_prompt: None,
        };

        let body = provider.build_request_body(&request);
        assert_eq!(body["model"], "llama3");
        let tools = body["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["type"], "function");
        assert_eq!(tools[0]["function"]["name"], "search");
    }

    #[test]
    fn test_parse_sse_text_delta() {
        let event = r#"{"id":"chatcmpl-1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"role":"assistant","content":"Hello"},"finish_reason":null}]}"#;
        let result = OpenAICompatProvider::parse_sse_event(event);
        match result {
            Some(LlmEvent::TextDelta(text)) => assert_eq!(text, "Hello"),
            other => panic!("Expected TextDelta, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_sse_tool_call_start() {
        let event = r#"{"id":"chatcmpl-1","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_abc","function":{"name":"search","arguments":""}}]},"finish_reason":null}]}"#;
        let result = OpenAICompatProvider::parse_sse_event(event);
        match result {
            Some(LlmEvent::ToolCallStart { id, name }) => {
                assert_eq!(id, "call_abc");
                assert_eq!(name, "search");
            }
            other => panic!("Expected ToolCallStart, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_sse_done() {
        let event = r#"{"id":"chatcmpl-1","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":20}}"#;
        let result = OpenAICompatProvider::parse_sse_event(event);
        match result {
            Some(LlmEvent::Done { stop_reason, usage }) => {
                assert_eq!(stop_reason, StopReason::EndTurn);
                let u = usage.unwrap();
                assert_eq!(u.input_tokens, 10);
                assert_eq!(u.output_tokens, 20);
            }
            other => panic!("Expected Done, got {:?}", other),
        }
    }
}
