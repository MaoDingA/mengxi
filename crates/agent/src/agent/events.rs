// agent/events.rs — Events emitted during the agent loop

/// Events emitted during agent execution.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// Agent has started processing.
    Started,
    /// A new turn (LLM call) has started.
    TurnStart { turn: usize },
    /// Streaming text delta from the LLM.
    TextDelta(String),
    /// A tool call is starting.
    ToolCallStart { name: String, call_id: String },
    /// A tool call has completed.
    ToolCallEnd {
        name: String,
        call_id: String,
        result: ToolResultSummary,
    },
    /// A turn has ended.
    TurnEnd { stop_reason: String },
    /// Agent has finished.
    Done { response: String },
    /// An error occurred.
    Error(String),
}

/// Summary of a tool result for event emission.
#[derive(Debug, Clone)]
pub struct ToolResultSummary {
    pub success: bool,
    pub content_preview: String,
}
