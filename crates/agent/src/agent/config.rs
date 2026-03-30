// agent/config.rs — Agent configuration

/// Configuration for the agent loop.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Maximum turns (LLM → tool → LLM cycles) before forcing stop.
    pub max_turns: usize,
    /// Maximum tokens for LLM response.
    pub max_tokens: u32,
    /// Temperature for LLM sampling.
    pub temperature: f64,
    /// System prompt prefix.
    pub system_prompt: String,
    /// Color science context injected into the system prompt.
    pub color_science_context: Option<String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_turns: 50,
            max_tokens: 8192,
            temperature: 0.7,
            system_prompt: "You are mengxi, an AI assistant for colorists and filmmakers. You help with color grading, style matching, and LUT management.".to_string(),
            color_science_context: Some(include_str!("color_science_context.md").to_string()),
        }
    }
}
