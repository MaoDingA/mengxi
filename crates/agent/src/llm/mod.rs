// llm/mod.rs — LLM provider abstraction layer

mod provider;
mod events;
mod claude;
mod openai_compat;

pub use claude::ClaudeProvider;
pub use openai_compat::OpenAICompatProvider;

pub use provider::{LlmProvider, LlmError};
pub use events::{LlmEvent, ContentBlock, ToolCall, StopReason, Usage};

use futures::Stream;
use std::pin::Pin;

use serde::{Deserialize, Serialize};

use crate::tool::ToolDefinition;

/// Type alias for boxed event stream.
pub type EventStream = Pin<Box<dyn Stream<Item = Result<LlmEvent, LlmError>> + Send>>;

/// A chat message in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<MessageContent>,
}

impl Message {
    pub fn user(text: &str) -> Self {
        Self {
            role: Role::User,
            content: vec![MessageContent::Text { text: text.to_string() }],
        }
    }

    pub fn assistant(text: &str) -> Self {
        Self {
            role: Role::Assistant,
            content: vec![MessageContent::Text { text: text.to_string() }],
        }
    }

    pub fn system(text: &str) -> Self {
        Self {
            role: Role::System,
            content: vec![MessageContent::Text { text: text.to_string() }],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// Content within a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MessageContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        tool_use_id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
}

/// A request to the LLM.
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub model: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f64>,
    pub system_prompt: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_message_user_factory() {
        let msg = Message::user("Hello world");
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.content.len(), 1);
        assert!(matches!(&msg.content[0], MessageContent::Text { text } if text == "Hello world"));
    }

    #[test]
    fn test_message_assistant_factory() {
        let msg = Message::assistant("Response");
        assert_eq!(msg.role, Role::Assistant);
        assert_eq!(msg.content.len(), 1);
        assert!(matches!(&msg.content[0], MessageContent::Text { text } if text == "Response"));
    }

    #[test]
    fn test_message_system_factory() {
        let msg = Message::system("System prompt");
        assert_eq!(msg.role, Role::System);
        assert_eq!(msg.content.len(), 1);
        assert!(matches!(&msg.content[0], MessageContent::Text { text } if text == "System prompt"));
    }

    #[test]
    fn test_role_serde_system() {
        let role = Role::System;
        let serialized = serde_json::to_string(&role).unwrap();
        assert_eq!(serialized, "\"system\"");

        let deserialized: Role = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, Role::System);
    }

    #[test]
    fn test_role_serde_user() {
        let role = Role::User;
        let serialized = serde_json::to_string(&role).unwrap();
        assert_eq!(serialized, "\"user\"");

        let deserialized: Role = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, Role::User);
    }

    #[test]
    fn test_role_serde_assistant() {
        let role = Role::Assistant;
        let serialized = serde_json::to_string(&role).unwrap();
        assert_eq!(serialized, "\"assistant\"");

        let deserialized: Role = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, Role::Assistant);
    }

    #[test]
    fn test_message_content_text_serialization() {
        let content = MessageContent::Text { text: "Hello".to_string() };
        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("\"type\":\"text\""));
        assert!(json.contains("\"text\":\"Hello\""));
    }

    #[test]
    fn test_message_content_tool_use_serialization() {
        let content = MessageContent::ToolUse {
            tool_use_id: "call_123".to_string(),
            name: "search".to_string(),
            input: serde_json::json!({"query": "test"}),
        };
        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("\"type\":\"tool_use\""));
        assert!(json.contains("\"tool_use_id\":\"call_123\""));
        assert!(json.contains("\"name\":\"search\""));
    }

    #[test]
    fn test_message_content_tool_result_serialization() {
        let content = MessageContent::ToolResult {
            tool_use_id: "call_123".to_string(),
            content: "Result".to_string(),
            is_error: false,
        };
        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("\"type\":\"tool_result\""));
        assert!(json.contains("\"content\":\"Result\""));
        assert!(json.contains("\"is_error\":false"));
    }

    #[test]
    fn test_chat_request_optional_fields() {
        let request = ChatRequest {
            messages: vec![Message::user("Test")],
            tools: vec![],
            model: None,
            max_tokens: None,
            temperature: None,
            system_prompt: None,
        };

        assert_eq!(request.messages.len(), 1);
        assert!(request.tools.is_empty());
        assert!(request.model.is_none());
        assert!(request.max_tokens.is_none());
        assert!(request.temperature.is_none());
        assert!(request.system_prompt.is_none());
    }
}
