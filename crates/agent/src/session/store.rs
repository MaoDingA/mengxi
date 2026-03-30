// session/store.rs — SessionStore: SQLite-backed session persistence

use crate::llm::{Message, MessageContent, Role};
use crate::session::types::{Session, SessionError, SessionInfo};
use mengxi_core::db::DbConnection;

/// Session persistence store. Stateless — opens a DB connection per operation.
pub struct SessionStore;

impl SessionStore {
    pub fn new() -> Self {
        Self
    }

    fn conn() -> Result<DbConnection, SessionError> {
        mengxi_core::db::open_db().map_err(|e| SessionError::DbError(e.to_string()))
    }

    /// Create a new session.
    pub fn create_session(
        &self,
        provider: Option<&str>,
        model: Option<&str>,
    ) -> Result<Session, SessionError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_epoch();
        let conn = Self::conn()?;

        conn.execute(
            "INSERT INTO agent_sessions (id, title, provider, model, created_at, updated_at)
             VALUES (?1, '', ?2, ?3, ?4, ?4)",
            [&id as &dyn rusqlite::types::ToSql, &provider, &model, &now as &dyn rusqlite::types::ToSql],
        )
        .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;

        Ok(Session {
            id,
            title: String::new(),
            provider: provider.map(String::from),
            model: model.map(String::from),
            created_at: now,
            updated_at: now,
        })
    }

    /// Get a session by ID.
    pub fn get_session(&self, id: &str) -> Result<Option<Session>, SessionError> {
        let conn = Self::conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, title, provider, model, created_at, updated_at
                 FROM agent_sessions WHERE id = ?1",
            )
            .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;

        let result: Result<Session, rusqlite::Error> = stmt.query_row([id], |row| {
            Ok(Session {
                id: row.get(0)?,
                title: row.get(1)?,
                provider: row.get(2)?,
                model: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        });

        match result {
            Ok(session) => Ok(Some(session)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(SessionError::DbError(e.to_string())),
        }
    }

    /// List all sessions ordered by most recently updated.
    pub fn list_sessions(&self) -> Result<Vec<SessionInfo>, SessionError> {
        let conn = Self::conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT s.id, s.title, COUNT(m.id) AS msg_count, s.updated_at
                 FROM agent_sessions s
                 LEFT JOIN session_messages m ON m.session_id = s.id
                 GROUP BY s.id
                 ORDER BY s.updated_at DESC",
            )
            .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;

        let rows = stmt
            .query_map([], |row: &rusqlite::Row<'_>| {
                Ok(SessionInfo {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    message_count: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            })
            .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;

        let mut infos = Vec::new();
        for row in rows {
            let info = row.map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;
            infos.push(info);
        }
        Ok(infos)
    }

    /// Save a single message to a session.
    pub fn save_message(
        &self,
        session_id: &str,
        seq: i64,
        msg: &Message,
    ) -> Result<(), SessionError> {
        let conn = Self::conn()?;
        let content_json = serde_json::to_string(&msg.content)
            .map_err(|e: serde_json::Error| SessionError::SerializationError(e.to_string()))?;
        let role_str = role_to_str(&msg.role);

        conn.execute(
            "INSERT INTO session_messages (session_id, seq, role, content)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![session_id, seq, role_str, content_json],
        )
        .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;

        touch_session(&conn, session_id)?;
        Ok(())
    }

    /// Save multiple messages in batch.
    pub fn save_messages(
        &self,
        session_id: &str,
        messages: &[Message],
    ) -> Result<(), SessionError> {
        let conn = Self::conn()?;

        let max_seq: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(seq), -1) FROM session_messages WHERE session_id = ?1",
                [session_id],
                |row: &rusqlite::Row<'_>| row.get(0),
            )
            .unwrap_or(-1);

        for (i, msg) in messages.iter().enumerate() {
            let seq = max_seq + 1 + i as i64;
            let content_json = serde_json::to_string(&msg.content)
                .map_err(|e: serde_json::Error| SessionError::SerializationError(e.to_string()))?;
            let role_str = role_to_str(&msg.role);

            conn.execute(
                "INSERT INTO session_messages (session_id, seq, role, content)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![session_id, seq, role_str, content_json],
            )
            .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;
        }

        touch_session(&conn, session_id)?;
        Ok(())
    }

    /// Load all messages for a session, ordered by seq.
    pub fn load_messages(&self, session_id: &str) -> Result<Vec<Message>, SessionError> {
        let conn = Self::conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT role, content FROM session_messages
                 WHERE session_id = ?1 ORDER BY seq ASC",
            )
            .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;

        let rows = stmt
            .query_map([session_id], |row: &rusqlite::Row<'_>| {
                let role_str: String = row.get(0)?;
                let content_json: String = row.get(1)?;
                Ok((role_str, content_json))
            })
            .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;

        let mut messages = Vec::new();
        for row in rows {
            let (role_str, content_json) =
                row.map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;
            let role = str_to_role(&role_str)?;
            let content: Vec<MessageContent> = serde_json::from_str(&content_json)
                .map_err(|e: serde_json::Error| SessionError::SerializationError(e.to_string()))?;
            messages.push(Message { role, content });
        }
        Ok(messages)
    }

    /// Update a session's title.
    pub fn update_title(&self, session_id: &str, title: &str) -> Result<(), SessionError> {
        let conn = Self::conn()?;
        conn.execute(
            "UPDATE agent_sessions SET title = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![title, now_epoch(), session_id],
        )
        .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;
        Ok(())
    }

    /// Delete a session and all its messages.
    pub fn delete_session(&self, session_id: &str) -> Result<(), SessionError> {
        let conn = Self::conn()?;
        conn.execute("DELETE FROM agent_sessions WHERE id = ?1", [session_id])
            .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;
        Ok(())
    }
}

fn role_to_str(role: &Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}

fn str_to_role(s: &str) -> Result<Role, SessionError> {
    match s {
        "system" => Ok(Role::System),
        "user" => Ok(Role::User),
        "assistant" => Ok(Role::Assistant),
        _ => Err(SessionError::SerializationError(format!(
            "Unknown role: {}",
            s
        ))),
    }
}

fn touch_session(conn: &DbConnection, session_id: &str) -> Result<(), SessionError> {
    conn.execute(
        "UPDATE agent_sessions SET updated_at = ?1 WHERE id = ?2",
        rusqlite::params![now_epoch(), session_id],
    )
    .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;
    Ok(())
}

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_get_session() {
        let store = SessionStore::new();
        let session = store
            .create_session(Some("claude"), Some("claude-sonnet-4-20250514"))
            .unwrap();
        assert!(!session.id.is_empty());
        assert_eq!(session.provider.as_deref(), Some("claude"));

        let loaded = store.get_session(&session.id).unwrap().unwrap();
        assert_eq!(loaded.id, session.id);
        store.delete_session(&session.id).unwrap();
    }

    #[test]
    fn test_get_session_not_found() {
        let store = SessionStore::new();
        assert!(store.get_session("nonexistent_test_id").unwrap().is_none());
    }

    #[test]
    fn test_save_and_load_messages() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();

        store.save_message(&session.id, 0, &Message::user("Hello")).unwrap();
        store.save_message(&session.id, 1, &Message::assistant("Found 5")).unwrap();

        let loaded = store.load_messages(&session.id).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].role, Role::User);
        assert_eq!(loaded[1].role, Role::Assistant);
        store.delete_session(&session.id).unwrap();
    }

    #[test]
    fn test_save_messages_batch_appends() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();

        store.save_messages(&session.id, &[Message::user("first")]).unwrap();
        store.save_messages(&session.id, &[Message::user("second"), Message::assistant("reply")]).unwrap();

        let loaded = store.load_messages(&session.id).unwrap();
        assert_eq!(loaded.len(), 3);
        store.delete_session(&session.id).unwrap();
    }

    #[test]
    fn test_list_sessions_finds_own() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();
        store.save_message(&session.id, 0, &Message::user("hello")).unwrap();

        let sessions = store.list_sessions().unwrap();
        let found = sessions.iter().find(|s| s.id == session.id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().message_count, 1);
        store.delete_session(&session.id).unwrap();
    }

    #[test]
    fn test_update_title() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();

        store.update_title(&session.id, "Warm sunset search").unwrap();
        let loaded = store.get_session(&session.id).unwrap().unwrap();
        assert_eq!(loaded.title, "Warm sunset search");
        store.delete_session(&session.id).unwrap();
    }

    #[test]
    fn test_delete_session() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();
        store.save_message(&session.id, 0, &Message::user("hello")).unwrap();
        store.delete_session(&session.id).unwrap();

        assert!(store.get_session(&session.id).unwrap().is_none());
        assert!(store.load_messages(&session.id).unwrap().is_empty());
    }

    #[test]
    fn test_load_messages_empty() {
        let store = SessionStore::new();
        assert!(store.load_messages("nonexistent_test_id").unwrap().is_empty());
    }

    #[test]
    fn test_message_with_tool_use_roundtrip() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();

        let msgs = vec![
            Message::user("search for warm tones"),
            Message {
                role: Role::Assistant,
                content: vec![
                    MessageContent::Text { text: "Searching.".into() },
                    MessageContent::ToolUse {
                        tool_use_id: "call_123".into(),
                        name: "search_by_tag".into(),
                        input: serde_json::json!({"tag": "warm"}),
                    },
                ],
            },
            Message {
                role: Role::User,
                content: vec![MessageContent::ToolResult {
                    tool_use_id: "call_123".into(),
                    content: "Found 5 results".into(),
                    is_error: false,
                }],
            },
        ];

        store.save_messages(&session.id, &msgs).unwrap();
        let loaded = store.load_messages(&session.id).unwrap();
        assert_eq!(loaded.len(), 3);

        match &loaded[1].content[1] {
            MessageContent::ToolUse { name, tool_use_id, .. } => {
                assert_eq!(name, "search_by_tag");
                assert_eq!(tool_use_id, "call_123");
            }
            _ => panic!("Expected ToolUse"),
        }
        match &loaded[2].content[0] {
            MessageContent::ToolResult { content, is_error, .. } => {
                assert_eq!(content, "Found 5 results");
                assert!(!is_error);
            }
            _ => panic!("Expected ToolResult"),
        }
        store.delete_session(&session.id).unwrap();
    }
}
