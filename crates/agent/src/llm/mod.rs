// llm/mod.rs — LLM provider abstraction layer

mod provider;
mod events;

pub use provider::{LlmProvider, LlmError};
pub use events::{LlmEvent, ContentBlock, ToolCall, StopReason, Usage};

use futures::Stream;
use std::pin::Pin;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
