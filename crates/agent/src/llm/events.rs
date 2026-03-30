// llm/events.rs — Streaming events from LLM providers

/// A streaming event from the LLM.
#[derive(Debug, Clone)]
pub enum LlmEvent {
    /// Streaming text content delta.
    TextDelta(String),
    /// A tool call is starting (name resolved, args streaming).
    ToolCallStart {
        id: String,
        name: String,
    },
    /// Streaming tool call arguments (JSON fragment).
    ToolCallDelta {
        id: String,
        delta: String,
    },
    /// A complete content block from the response.
    ContentBlock(ContentBlock),
    /// LLM has finished generating.
    Done {
        stop_reason: StopReason,
        usage: Option<Usage>,
    },
    /// An error occurred.
    Error(String),
}

/// A content block in a complete assistant message.
#[derive(Debug, Clone)]
pub enum ContentBlock {
    Text(String),
    ToolCall(ToolCall),
}

/// A completed tool call from the LLM.
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
}

#[derive(Debug, Clone)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}
