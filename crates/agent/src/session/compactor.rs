// session/compactor.rs — Conversation compaction with LLM summarization

use crate::llm::{ChatRequest, LlmEvent, LlmProvider, Message, MessageContent};
use crate::session::store::SessionStore;
use crate::session::types::{CompactionResult, SessionError};
use futures::StreamExt;

/// Configuration for compaction behavior.
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// Minimum number of uncompacted messages before compaction is considered.
    pub threshold: usize,
    /// Number of recent messages to preserve (not compacted).
    pub keep_recent: usize,
    /// Maximum characters for the summary.
    pub max_summary_chars: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            threshold: 20,
            keep_recent: 6,
            max_summary_chars: 500,
        }
    }
}

/// Compacts old messages in a session branch by summarizing them via LLM.
pub struct Compactor {
    store: SessionStore,
}

impl Compactor {
    pub fn new(store: SessionStore) -> Self {
        Self { store }
    }

    /// Check if compaction is needed for a branch.
    pub fn needs_compaction(
        &self,
        session_id: &str,
        branch_id: &str,
        config: &CompactionConfig,
    ) -> Result<bool, SessionError> {
        let count = self
            .store
            .get_message_count_for_branch(session_id, branch_id)?;
        Ok((count as usize) >= config.threshold)
    }

    /// Compact old messages in a branch.
    ///
    /// Summarizes old messages via LLM, inserts a summary, and marks old
    /// messages as compacted. Falls back to truncation if LLM is unavailable.
    pub async fn compact(
        &self,
        provider: &dyn LlmProvider,
        session_id: &str,
        branch_id: &str,
        config: &CompactionConfig,
    ) -> Result<CompactionResult, SessionError> {
        let raw = self
            .store
            .load_raw_messages_for_branch(session_id, branch_id)?;

        if raw.len() < config.threshold {
            return Ok(CompactionResult {
                messages_compacted: 0,
                summary_seq: -1,
                messages_preserved: raw.len(),
            });
        }

        let split_point = raw.len().saturating_sub(config.keep_recent);
        let old_raw = &raw[..split_point];
        let _recent_raw = &raw[split_point..];

        let old_messages: Vec<&Message> = old_raw.iter().map(|(_, m)| m).collect();

        // Generate summary
        let summary = match self.summarize(provider, &old_messages, config).await {
            Ok(s) => s,
            Err(_) => self.truncate_summary(&old_messages, config.max_summary_chars),
        };

        // Summary goes at the seq after the last compacted message
        let summary_seq = old_raw.last().map(|(seq, _)| *seq).unwrap_or(0);

        // Mark old messages as compacted
        let up_to_seq = old_raw.last().map(|(seq, _)| *seq).unwrap_or(0);
        let compacted_count = self
            .store
            .mark_messages_compacted(session_id, branch_id, up_to_seq)?;

        // Insert summary
        self.store
            .insert_summary_message(session_id, branch_id, &summary, summary_seq)?;

        Ok(CompactionResult {
            messages_compacted: compacted_count,
            summary_seq,
            messages_preserved: raw.len() - compacted_count,
        })
    }

    /// Summarize messages using the LLM.
    async fn summarize(
        &self,
        provider: &dyn LlmProvider,
        messages: &[&Message],
        config: &CompactionConfig,
    ) -> Result<String, SessionError> {
        let conversation_text = format_messages(messages);

        let prompt = format!(
            "Summarize the following conversation in a concise paragraph. \
             Preserve key decisions, tool calls made, and results obtained. \
             Maximum {} characters.\n\n{}",
            config.max_summary_chars, conversation_text
        );

        let request = ChatRequest {
            messages: vec![Message::user(&prompt)],
            tools: vec![],
            model: None,
            max_tokens: Some(1024),
            temperature: Some(0.3),
            system_prompt: Some(
                "You are a conversation summarizer. Produce concise, factual summaries.".into(),
            ),
        };

        let mut stream = provider
            .stream_chat(request)
            .await
            .map_err(|e| SessionError::CompactionError(e.to_string()))?;

        let mut summary = String::new();
        while let Some(event) = stream.next().await {
            match event {
                Ok(LlmEvent::TextDelta(delta)) => summary.push_str(&delta),
                Ok(LlmEvent::Done { .. }) => break,
                Ok(LlmEvent::Error(msg)) => {
                    return Err(SessionError::CompactionError(msg));
                }
                Err(e) => {
                    return Err(SessionError::CompactionError(e.to_string()));
                }
                _ => {}
            }
        }

        if summary.is_empty() {
            return Err(SessionError::CompactionError(
                "LLM returned empty summary".into(),
            ));
        }

        Ok(summary)
    }

    /// Fallback: generate a summary by truncating messages.
    fn truncate_summary(&self, messages: &[&Message], max_chars: usize) -> String {
        let per_msg = if messages.is_empty() {
            return String::new();
        } else {
            max_chars / messages.len()
        };

        let mut parts = Vec::new();
        for msg in messages {
            let text: String = msg
                .content
                .iter()
                .filter_map(|c| match c {
                    MessageContent::Text { text } => Some(text.as_str()),
                    MessageContent::ToolUse { name, .. } => Some(name),
                    MessageContent::ToolResult { .. } => None,
                })
                .collect::<Vec<_>>()
                .join(" ");

            if !text.is_empty() {
                let role = match msg.role {
                    crate::llm::Role::System => "System",
                    crate::llm::Role::User => "User",
                    crate::llm::Role::Assistant => "Assistant",
                };
                let truncated = if text.len() > per_msg {
                    &text[..per_msg]
                } else {
                    &text
                };
                parts.push(format!("{}: {}", role, truncated));
            }
        }

        let result = parts.join(" | ");
        if result.len() > max_chars {
            result[..max_chars].to_string()
        } else {
            result
        }
    }
}

/// Format messages into a readable string for summarization.
fn format_messages(messages: &[&Message]) -> String {
    messages
        .iter()
        .map(|msg| {
            let role = match msg.role {
                crate::llm::Role::System => "System",
                crate::llm::Role::User => "User",
                crate::llm::Role::Assistant => "Assistant",
            };
            let text: String = msg
                .content
                .iter()
                .map(|c| match c {
                    MessageContent::Text { text } => text.clone(),
                    MessageContent::ToolUse { name, input, .. } => {
                        format!("[Tool: {}({})]", name, input)
                    }
                    MessageContent::ToolResult {
                        content, is_error, ..
                    } => {
                        if *is_error {
                            format!("[Tool Error: {}]", content)
                        } else {
                            format!("[Tool Result: {}]", content)
                        }
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            format!("[{}] {}", role, text)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: resolve the main branch UUID for a session.
    fn main_id(session_id: &str) -> String {
        SessionStore::resolve_branch_id(session_id, "main").unwrap()
    }

    #[test]
    fn test_truncate_summary_basic() {
        let store = SessionStore::new();
        let compactor = Compactor::new(store);

        let messages = vec![
            Message::user("What are warm color palettes?"),
            Message::assistant("Warm palettes include oranges, reds, and yellows."),
        ];
        let refs: Vec<&Message> = messages.iter().collect();

        let summary = compactor.truncate_summary(&refs, 200);
        assert!(!summary.is_empty());
        assert!(summary.contains("User:"));
        assert!(summary.contains("Assistant:"));
    }

    #[test]
    fn test_truncate_summary_empty() {
        let store = SessionStore::new();
        let compactor = Compactor::new(store);

        let summary = compactor.truncate_summary(&[], 200);
        assert!(summary.is_empty());
    }

    #[test]
    fn test_needs_compaction_below_threshold() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();
        let mid = main_id(&session.id);

        // Add 5 messages (below default threshold of 20)
        for i in 0..5 {
            store
                .save_message_on_branch(&session.id, &mid, &Message::user(&format!("msg {}", i)))
                .unwrap();
        }

        let compactor = Compactor::new(SessionStore::new());
        let config = CompactionConfig::default();
        let needed = compactor
            .needs_compaction(&session.id, &mid, &config)
            .unwrap();
        assert!(!needed);

        SessionStore::new().delete_session(&session.id).unwrap();
    }

    #[test]
    fn test_needs_compaction_above_threshold() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();
        let mid = main_id(&session.id);

        // Add 25 messages (above threshold of 20)
        for i in 0..25 {
            store
                .save_message_on_branch(&session.id, &mid, &Message::user(&format!("msg {}", i)))
                .unwrap();
        }

        let compactor = Compactor::new(SessionStore::new());
        let config = CompactionConfig {
            threshold: 20,
            keep_recent: 6,
            max_summary_chars: 500,
        };
        let needed = compactor
            .needs_compaction(&session.id, &mid, &config)
            .unwrap();
        assert!(needed);

        SessionStore::new().delete_session(&session.id).unwrap();
    }

    #[test]
    fn test_format_messages() {
        let messages = vec![
            Message::user("hello"),
            Message::assistant("world"),
        ];
        let refs: Vec<&Message> = messages.iter().collect();
        let text = format_messages(&refs);
        assert!(text.contains("[User] hello"));
        assert!(text.contains("[Assistant] world"));
    }

    // Async tests with mock provider

    /// A mock LLM provider that returns a predetermined response.
    struct MockProvider {
        response: String,
        should_fail: bool,
    }

    #[async_trait::async_trait]
    impl crate::llm::LlmProvider for MockProvider {
        async fn stream_chat(
            &self,
            _request: ChatRequest,
        ) -> Result<crate::llm::EventStream, crate::llm::LlmError> {
            if self.should_fail {
                return Err(crate::llm::LlmError::ConnectionError("mock fail".into()));
            }
            let text = self.response.clone();
            let stream = futures::stream::once(async move {
                Ok(LlmEvent::TextDelta(text))
            })
            .chain(futures::stream::once(async move {
                Ok(LlmEvent::Done {
                    stop_reason: crate::llm::StopReason::EndTurn,
                    usage: None,
                })
            }));
            Ok(Box::pin(stream))
        }

        fn name(&self) -> &str {
            "mock"
        }

        fn default_model(&self) -> &str {
            "mock-model"
        }
    }

    #[tokio::test]
    async fn test_compact_with_mock_llm() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();
        let mid = main_id(&session.id);

        // Add 25 messages
        for i in 0..25 {
            store
                .save_message_on_branch(&session.id, &mid, &Message::user(&format!("msg {}", i)))
                .unwrap();
        }

        let compactor = Compactor::new(SessionStore::new());
        let config = CompactionConfig {
            threshold: 20,
            keep_recent: 6,
            max_summary_chars: 500,
        };
        let provider = MockProvider {
            response: "User explored warm color palettes and searched for sunset images.".into(),
            should_fail: false,
        };

        let result = compactor
            .compact(&provider, &session.id, &mid, &config)
            .await
            .unwrap();

        assert_eq!(result.messages_compacted, 19); // 25 - 6 = 19
        assert_eq!(result.messages_preserved, 6); // 6 recent messages

        // Verify summary was inserted
        let msgs = store.load_raw_messages_for_branch(&session.id, &mid).unwrap();
        let has_summary = msgs.iter().any(|(_, m)| {
            m.content.iter().any(|c| {
                matches!(c, MessageContent::Text { text } if text.contains("[Summary]"))
            })
        });
        assert!(has_summary);

        store.delete_session(&session.id).unwrap();
    }

    #[tokio::test]
    async fn test_compact_fallback_to_truncation() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();
        let mid = main_id(&session.id);

        for i in 0..25 {
            store
                .save_message_on_branch(&session.id, &mid, &Message::user(&format!("msg {}", i)))
                .unwrap();
        }

        let compactor = Compactor::new(SessionStore::new());
        let config = CompactionConfig {
            threshold: 20,
            keep_recent: 6,
            max_summary_chars: 500,
        };
        let provider = MockProvider {
            response: String::new(),
            should_fail: true,
        };

        let result = compactor
            .compact(&provider, &session.id, &mid, &config)
            .await
            .unwrap();

        // Should still succeed via truncation fallback
        assert_eq!(result.messages_compacted, 19);

        store.delete_session(&session.id).unwrap();
    }
}
