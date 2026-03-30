// agent/state.rs — Agent conversation state

use crate::llm::Message;

/// Current state of the agent's conversation.
#[derive(Debug, Clone)]
pub struct AgentState {
    messages: Vec<Message>,
    turn: usize,
}

impl AgentState {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            turn: 0,
        }
    }

    /// Add a message to the conversation history.
    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    /// Get all messages for the LLM request.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Current turn number (0-indexed).
    pub fn turn(&self) -> usize {
        self.turn
    }

    /// Advance to the next turn.
    pub fn advance_turn(&mut self) {
        self.turn += 1;
    }

    /// Truncate old messages to fit within a token budget.
    /// Keeps the last N messages. System prompts are sent via ChatRequest,
    /// not stored in the message history, so no special first-message handling.
    pub fn truncate(&mut self, keep_last: usize) {
        if self.messages.len() <= keep_last {
            return;
        }
        let last_n: Vec<_> = self.messages.iter().rev().take(keep_last).cloned().collect();
        self.messages.clear();
        self.messages.extend(last_n.into_iter().rev());
    }
}

impl Default for AgentState {
    fn default() -> Self {
        Self::new()
    }
}
