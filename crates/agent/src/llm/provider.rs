// llm/provider.rs — LLM provider trait definition

use async_trait::async_trait;

use super::{ChatRequest, EventStream};

/// Errors from LLM providers.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("LLM_AUTH_ERROR -- {0}")]
    AuthError(String),
    #[error("LLM_RATE_LIMIT -- {0}")]
    RateLimit(String),
    #[error("LLM_CONNECTION_ERROR -- {0}")]
    ConnectionError(String),
    #[error("LLM_API_ERROR -- status {status}: {message}")]
    ApiError { status: u16, message: String },
    #[error("LLM_PARSE_ERROR -- {0}")]
    ParseError(String),
    #[error("LLM_ABORTED")]
    Aborted,
}

/// A unified interface for LLM providers.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Stream a chat completion, yielding events as they arrive.
    async fn stream_chat(&self, request: ChatRequest) -> Result<EventStream, LlmError>;

    /// Provider name for logging/config.
    fn name(&self) -> &str;

    /// Default model identifier for this provider.
    fn default_model(&self) -> &str;
}
