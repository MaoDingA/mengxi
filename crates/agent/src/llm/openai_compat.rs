// llm/openai_compat.rs — OpenAI-compatible API provider (Ollama, LocalAI, etc.)

use async_trait::async_trait;

use super::provider::{LlmError, LlmProvider};
use super::ChatRequest;
use super::events::EventStream;

/// OpenAI-compatible provider (works with Ollama, LocalAI, vLLM, LM Studio, etc.).
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
            model: "llama3".to_string(),
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
        Self::new("http://localhost:11434")
    }
}

#[async_trait]
impl LlmProvider for OpenAICompatProvider {
    async fn stream_chat(&self, _request: ChatRequest) -> Result<EventStream, LlmError> {
        // TODO: Implement SSE streaming to OpenAI-compatible /v1/chat/completions
        Err(LlmError::ConnectionError("OpenAI-compatible streaming not yet implemented".to_string()))
    }

    fn name(&self) -> &str {
        "openai-compat"
    }

    fn default_model(&self) -> &str {
        &self.model
    }
}
