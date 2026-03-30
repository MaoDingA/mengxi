// llm/claude.rs — Anthropic Claude API provider

use async_trait::async_trait;

use super::provider::{LlmError, LlmProvider};
use super::ChatRequest;
use super::events::EventStream;

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
}

#[async_trait]
impl LlmProvider for ClaudeProvider {
    async fn stream_chat(&self, _request: ChatRequest) -> Result<EventStream, LlmError> {
        // TODO: Implement SSE streaming to Claude Messages API
        Err(LlmError::ConnectionError("Claude streaming not yet implemented".to_string()))
    }

    fn name(&self) -> &str {
        "claude"
    }

    fn default_model(&self) -> &str {
        &self.model
    }
}
