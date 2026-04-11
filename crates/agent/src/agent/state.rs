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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{Role, MessageContent};

    #[test]
    fn test_new_state_is_empty() {
        let state = AgentState::new();
        assert_eq!(state.messages().len(), 0);
        assert_eq!(state.turn(), 0);
    }

    #[test]
    fn test_add_message_increments() {
        let mut state = AgentState::new();
        assert_eq!(state.messages().len(), 0);

        state.add_message(Message::user("Hello"));
        assert_eq!(state.messages().len(), 1);

        state.add_message(Message::assistant("Hi there"));
        assert_eq!(state.messages().len(), 2);
    }

    #[test]
    fn test_messages_ref() {
        let mut state = AgentState::new();
        let msg1 = Message::user("First");
        let msg2 = Message::assistant("Second");

        state.add_message(msg1.clone());
        state.add_message(msg2.clone());

        let messages = state.messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, Role::User);
        assert_eq!(messages[1].role, Role::Assistant);
    }

    #[test]
    fn test_turn_counter() {
        let mut state = AgentState::new();
        assert_eq!(state.turn(), 0);

        state.advance_turn();
        assert_eq!(state.turn(), 1);

        state.advance_turn();
        assert_eq!(state.turn(), 2);
    }

    #[test]
    fn test_truncate_keeps_last_n() {
        let mut state = AgentState::new();
        for i in 0..5 {
            state.add_message(Message::user(&format!("Message {}", i)));
        }

        assert_eq!(state.messages().len(), 5);
        state.truncate(3);
        assert_eq!(state.messages().len(), 3);

        // Should keep last 3 messages (indices 2, 3, 4 from original)
        assert!(matches!(&state.messages()[0].content[0], MessageContent::Text { text } if text == "Message 2"));
        assert!(matches!(&state.messages()[1].content[0], MessageContent::Text { text } if text == "Message 3"));
        assert!(matches!(&state.messages()[2].content[0], MessageContent::Text { text } if text == "Message 4"));
    }

    #[test]
    fn test_truncate_noop_when_small() {
        let mut state = AgentState::new();
        state.add_message(Message::user("Only one"));

        assert_eq!(state.messages().len(), 1);
        state.truncate(5);
        assert_eq!(state.messages().len(), 1);

        // Exact size should also be no-op
        state.truncate(1);
        assert_eq!(state.messages().len(), 1);
    }

    #[test]
    fn test_default_matches_new() {
        let new_state = AgentState::new();
        let default_state = AgentState::default();

        assert_eq!(new_state.messages().len(), default_state.messages().len());
        assert_eq!(new_state.turn(), default_state.turn());
    }
}
