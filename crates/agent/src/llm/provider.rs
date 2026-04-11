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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_error_auth_format() {
        let err = LlmError::AuthError("Invalid token".to_string());
        let display = format!("{}", err);
        assert!(display.starts_with("LLM_AUTH_ERROR --"));
        assert!(display.contains("Invalid token"));
    }

    #[test]
    fn test_llm_error_rate_limit_format() {
        let err = LlmError::RateLimit("Too many requests".to_string());
        let display = format!("{}", err);
        assert!(display.starts_with("LLM_RATE_LIMIT --"));
        assert!(display.contains("Too many requests"));
    }

    #[test]
    fn test_llm_error_connection_format() {
        let err = LlmError::ConnectionError("Network timeout".to_string());
        let display = format!("{}", err);
        assert!(display.starts_with("LLM_CONNECTION_ERROR --"));
        assert!(display.contains("Network timeout"));
    }

    #[test]
    fn test_llm_error_api_format() {
        let err = LlmError::ApiError { status: 429, message: "Rate limited".to_string() };
        let display = format!("{}", err);
        assert!(display.starts_with("LLM_API_ERROR --"));
        assert!(display.contains("status 429"));
        assert!(display.contains("Rate limited"));
    }

    #[test]
    fn test_llm_error_parse_format() {
        let err = LlmError::ParseError("Invalid JSON".to_string());
        let display = format!("{}", err);
        assert!(display.starts_with("LLM_PARSE_ERROR --"));
        assert!(display.contains("Invalid JSON"));
    }

    #[test]
    fn test_llm_error_aborted_format() {
        let err = LlmError::Aborted;
        let display = format!("{}", err);
        assert_eq!(display, "LLM_ABORTED");
    }
}
