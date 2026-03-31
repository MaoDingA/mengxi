// session/store.rs — SessionStore: SQLite-backed session persistence with branching & compaction

use crate::llm::{Message, MessageContent, Role};
use crate::session::types::{Branch, BranchTreeNode, Session, SessionError, SessionInfo};
use mengxi_core::db::DbConnection;

/// Session persistence store. Stateless — opens a DB connection per operation.
pub struct SessionStore;

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStore {
    pub fn new() -> Self {
        Self
    }

    fn conn() -> Result<DbConnection, SessionError> {
        mengxi_core::db::open_db().map_err(|e| SessionError::DbError(e.to_string()))
    }

    /// Resolve a branch UUID by name within a session.
    pub fn resolve_branch_id(session_id: &str, name: &str) -> Result<String, SessionError> {
        let conn = Self::conn()?;
        conn.query_row(
            "SELECT id FROM session_branches WHERE session_id = ?1 AND name = ?2",
            rusqlite::params![session_id, name],
            |row| row.get::<_, String>(0),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                SessionError::BranchNotFound(format!("branch '{}' not found in session", name))
            }
            e => SessionError::DbError(e.to_string()),
        })
    }

    // ── Session CRUD ─────────────────────────────────────────────

    /// Create a new session with an implicit "main" branch.
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

        // Create the "main" branch with a UUID id
        let main_branch_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO session_branches (id, session_id, parent_branch_id, branch_point_seq, name, created_at)
             VALUES (?1, ?2, NULL, NULL, 'main', ?3)",
            rusqlite::params![main_branch_id, id, now],
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
                 LEFT JOIN session_messages m ON m.session_id = s.id AND m.is_compacted = 0
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

    /// Delete a session and all its branches/messages (CASCADE).
    pub fn delete_session(&self, session_id: &str) -> Result<(), SessionError> {
        let conn = Self::conn()?;
        conn.execute("DELETE FROM agent_sessions WHERE id = ?1", [session_id])
            .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;
        Ok(())
    }

    // ── Message operations (main branch, backward-compatible) ────

    /// Save a single message to a session (main branch).
    pub fn save_message(
        &self,
        session_id: &str,
        seq: i64,
        msg: &Message,
    ) -> Result<(), SessionError> {
        let branch_id = Self::resolve_branch_id(session_id, "main")?;
        let conn = Self::conn()?;
        let content_json = serde_json::to_string(&msg.content)
            .map_err(|e: serde_json::Error| SessionError::SerializationError(e.to_string()))?;
        let role_str = role_to_str(&msg.role);

        conn.execute(
            "INSERT INTO session_messages (session_id, seq, role, content, branch_id)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![session_id, seq, role_str, content_json, branch_id],
        )
        .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;

        touch_session(&conn, session_id)?;
        Ok(())
    }

    /// Save multiple messages in batch (main branch).
    pub fn save_messages(
        &self,
        session_id: &str,
        messages: &[Message],
    ) -> Result<(), SessionError> {
        let branch_id = Self::resolve_branch_id(session_id, "main")?;
        let conn = Self::conn()?;

        let max_seq: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(seq), -1) FROM session_messages WHERE session_id = ?1 AND branch_id = ?2",
                rusqlite::params![session_id, branch_id],
                |row: &rusqlite::Row<'_>| row.get(0),
            )
            .unwrap_or(-1);

        for (i, msg) in messages.iter().enumerate() {
            let seq = max_seq + 1 + i as i64;
            let content_json = serde_json::to_string(&msg.content)
                .map_err(|e: serde_json::Error| SessionError::SerializationError(e.to_string()))?;
            let role_str = role_to_str(&msg.role);

            conn.execute(
                "INSERT INTO session_messages (session_id, seq, role, content, branch_id)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![session_id, seq, role_str, content_json, branch_id],
            )
            .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;
        }

        touch_session(&conn, session_id)?;
        Ok(())
    }

    /// Load all uncompacted messages for a session (main branch), ordered by seq.
    pub fn load_messages(&self, session_id: &str) -> Result<Vec<Message>, SessionError> {
        let branch_id = Self::resolve_branch_id(session_id, "main")?;
        self.load_messages_for_branch(session_id, &branch_id)
    }

    // ── Branch operations ────────────────────────────────────────

    /// Create a new branch from a parent branch at a given seq.
    pub fn create_branch(
        &self,
        session_id: &str,
        parent_branch_id: &str,
        branch_point_seq: i64,
        name: &str,
    ) -> Result<Branch, SessionError> {
        let conn = Self::conn()?;

        // Validate parent branch exists in this session
        let parent_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM session_branches WHERE id = ?1 AND session_id = ?2",
                rusqlite::params![parent_branch_id, session_id],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);

        if !parent_exists {
            return Err(SessionError::BranchNotFound(parent_branch_id.to_string()));
        }

        // Validate branch_point_seq is within parent's message range
        let parent_max_seq: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(seq), -1) FROM session_messages WHERE session_id = ?1 AND branch_id = ?2",
                rusqlite::params![session_id, parent_branch_id],
                |row: &rusqlite::Row<'_>| row.get(0),
            )
            .unwrap_or(-1);

        if branch_point_seq > parent_max_seq {
            return Err(SessionError::InvalidBranchPoint(format!(
                "seq {} exceeds parent branch max seq {}",
                branch_point_seq, parent_max_seq
            )));
        }

        let id = uuid::Uuid::new_v4().to_string();
        let now = now_epoch();

        conn.execute(
            "INSERT INTO session_branches (id, session_id, parent_branch_id, branch_point_seq, name, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![id, session_id, parent_branch_id, branch_point_seq, name, now],
        )
        .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;

        Ok(Branch {
            id,
            session_id: session_id.to_string(),
            parent_branch_id: Some(parent_branch_id.to_string()),
            branch_point_seq: Some(branch_point_seq),
            name: name.to_string(),
            created_at: now,
        })
    }

    /// Get a specific branch by its UUID.
    pub fn get_branch(&self, session_id: &str, branch_id: &str) -> Result<Branch, SessionError> {
        let conn = Self::conn()?;
        let result = conn.query_row(
            "SELECT id, session_id, parent_branch_id, branch_point_seq, name, created_at
             FROM session_branches WHERE id = ?1 AND session_id = ?2",
            rusqlite::params![branch_id, session_id],
            |row| {
                Ok(Branch {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    parent_branch_id: row.get(2)?,
                    branch_point_seq: row.get(3)?,
                    name: row.get(4)?,
                    created_at: row.get(5)?,
                })
            },
        );

        match result {
            Ok(b) => Ok(b),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                Err(SessionError::BranchNotFound(branch_id.to_string()))
            }
            Err(e) => Err(SessionError::DbError(e.to_string())),
        }
    }

    /// Get the main branch for a session.
    pub fn get_main_branch(&self, session_id: &str) -> Result<Branch, SessionError> {
        let conn = Self::conn()?;
        let result = conn.query_row(
            "SELECT id, session_id, parent_branch_id, branch_point_seq, name, created_at
             FROM session_branches WHERE session_id = ?1 AND name = 'main'",
            [session_id],
            |row| {
                Ok(Branch {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    parent_branch_id: row.get(2)?,
                    branch_point_seq: row.get(3)?,
                    name: row.get(4)?,
                    created_at: row.get(5)?,
                })
            },
        );

        match result {
            Ok(b) => Ok(b),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                Err(SessionError::BranchNotFound("main".to_string()))
            }
            Err(e) => Err(SessionError::DbError(e.to_string())),
        }
    }

    /// List all branches for a session.
    pub fn list_branches(&self, session_id: &str) -> Result<Vec<Branch>, SessionError> {
        let conn = Self::conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, session_id, parent_branch_id, branch_point_seq, name, created_at
                 FROM session_branches WHERE session_id = ?1 ORDER BY created_at ASC",
            )
            .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;

        let rows = stmt
            .query_map([session_id], |row: &rusqlite::Row<'_>| {
                Ok(Branch {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    parent_branch_id: row.get(2)?,
                    branch_point_seq: row.get(3)?,
                    name: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;

        let mut branches = Vec::new();
        for row in rows {
            let b = row.map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;
            branches.push(b);
        }
        Ok(branches)
    }

    /// Build a branch tree for navigation.
    pub fn get_branch_tree(&self, session_id: &str) -> Result<BranchTreeNode, SessionError> {
        let branches = self.list_branches(session_id)?;
        let conn = Self::conn()?;

        // Count messages per branch
        let mut counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        for b in &branches {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM session_messages WHERE session_id = ?1 AND branch_id = ?2 AND is_compacted = 0",
                    rusqlite::params![session_id, b.id],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            counts.insert(b.id.clone(), count);
        }

        // Find root (main branch — has name = 'main')
        let root = branches
            .iter()
            .find(|b| b.name == "main")
            .ok_or_else(|| SessionError::BranchNotFound("main".to_string()))?;

        fn build_tree(
            parent_id: &str,
            branches: &[Branch],
            counts: &std::collections::HashMap<String, i64>,
        ) -> Vec<BranchTreeNode> {
            branches
                .iter()
                .filter(|b| b.parent_branch_id.as_deref() == Some(parent_id))
                .map(|b| BranchTreeNode {
                    message_count: *counts.get(&b.id).unwrap_or(&0),
                    children: build_tree(&b.id, branches, counts),
                    branch: b.clone(),
                })
                .collect()
        }

        Ok(BranchTreeNode {
            branch: root.clone(),
            message_count: *counts.get(&root.id).unwrap_or(&0),
            children: build_tree(&root.id, &branches, &counts),
        })
    }

    // ── Branch-aware message operations ──────────────────────────

    /// Save a single message to a specific branch, auto-computing seq.
    pub fn save_message_on_branch(
        &self,
        session_id: &str,
        branch_id: &str,
        msg: &Message,
    ) -> Result<i64, SessionError> {
        let conn = Self::conn()?;
        let content_json = serde_json::to_string(&msg.content)
            .map_err(|e: serde_json::Error| SessionError::SerializationError(e.to_string()))?;
        let role_str = role_to_str(&msg.role);

        let max_seq: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(seq), -1) FROM session_messages WHERE session_id = ?1 AND branch_id = ?2",
                rusqlite::params![session_id, branch_id],
                |row: &rusqlite::Row<'_>| row.get(0),
            )
            .unwrap_or(-1);

        let seq = max_seq + 1;

        conn.execute(
            "INSERT INTO session_messages (session_id, seq, role, content, branch_id)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![session_id, seq, role_str, content_json, branch_id],
        )
        .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;

        touch_session(&conn, session_id)?;
        Ok(seq)
    }

    /// Save multiple messages to a specific branch, auto-computing seq.
    pub fn save_messages_on_branch(
        &self,
        session_id: &str,
        branch_id: &str,
        messages: &[Message],
    ) -> Result<(), SessionError> {
        let conn = Self::conn()?;

        let max_seq: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(seq), -1) FROM session_messages WHERE session_id = ?1 AND branch_id = ?2",
                rusqlite::params![session_id, branch_id],
                |row: &rusqlite::Row<'_>| row.get(0),
            )
            .unwrap_or(-1);

        for (i, msg) in messages.iter().enumerate() {
            let seq = max_seq + 1 + i as i64;
            let content_json = serde_json::to_string(&msg.content)
                .map_err(|e: serde_json::Error| SessionError::SerializationError(e.to_string()))?;
            let role_str = role_to_str(&msg.role);

            conn.execute(
                "INSERT INTO session_messages (session_id, seq, role, content, branch_id)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![session_id, seq, role_str, content_json, branch_id],
            )
            .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;
        }

        touch_session(&conn, session_id)?;
        Ok(())
    }

    /// Load messages for a specific branch (by UUID), including parent prefix up to branch point.
    pub fn load_messages_for_branch(
        &self,
        session_id: &str,
        branch_id: &str,
    ) -> Result<Vec<Message>, SessionError> {
        let conn = Self::conn()?;
        let branch = self.get_branch(session_id, branch_id)?;

        let mut messages = Vec::new();

        // If this branch has a parent, load parent prefix recursively
        if let (Some(ref parent_id), Some(point_seq)) =
            (&branch.parent_branch_id, branch.branch_point_seq)
        {
            let parent_msgs = self.load_messages_for_branch(session_id, parent_id)?;
            messages.extend(parent_msgs.into_iter().take((point_seq + 1) as usize));
        }

        // Load this branch's own messages
        let own = load_branch_messages_raw(&conn, session_id, branch_id)?;
        messages.extend(own);

        Ok(messages)
    }

    /// Count uncompacted messages in a branch.
    pub fn get_message_count_for_branch(
        &self,
        session_id: &str,
        branch_id: &str,
    ) -> Result<i64, SessionError> {
        let conn = Self::conn()?;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM session_messages
                 WHERE session_id = ?1 AND branch_id = ?2 AND is_compacted = 0",
                rusqlite::params![session_id, branch_id],
                |row| row.get(0),
            )
            .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;
        Ok(count)
    }

    // ── Compaction DB operations ─────────────────────────────────

    /// Mark messages as compacted (is_compacted = 1) up to a given seq.
    pub fn mark_messages_compacted(
        &self,
        session_id: &str,
        branch_id: &str,
        up_to_seq: i64,
    ) -> Result<usize, SessionError> {
        let conn = Self::conn()?;
        let affected = conn
            .execute(
                "UPDATE session_messages SET is_compacted = 1
                 WHERE session_id = ?1 AND branch_id = ?2 AND seq <= ?3 AND is_compacted = 0",
                rusqlite::params![session_id, branch_id, up_to_seq],
            )
            .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;
        Ok(affected)
    }

    /// Insert a summary message (system role) at a given seq in a branch.
    pub fn insert_summary_message(
        &self,
        session_id: &str,
        branch_id: &str,
        summary_text: &str,
        at_seq: i64,
    ) -> Result<(), SessionError> {
        let conn = Self::conn()?;
        let content_json = serde_json::to_string(&vec![MessageContent::Text {
            text: format!("[Summary] {}", summary_text),
        }])
        .map_err(|e: serde_json::Error| SessionError::SerializationError(e.to_string()))?;

        conn.execute(
            "INSERT INTO session_messages (session_id, seq, role, content, branch_id, is_compacted)
             VALUES (?1, ?2, 'system', ?3, ?4, 0)",
            rusqlite::params![session_id, at_seq, content_json, branch_id],
        )
        .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;

        touch_session(&conn, session_id)?;
        Ok(())
    }

    /// Load raw messages with seq for a branch (used by compaction).
    pub fn load_raw_messages_for_branch(
        &self,
        session_id: &str,
        branch_id: &str,
    ) -> Result<Vec<(i64, Message)>, SessionError> {
        let conn = Self::conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT seq, role, content FROM session_messages
                 WHERE session_id = ?1 AND branch_id = ?2 AND is_compacted = 0
                 ORDER BY seq ASC",
            )
            .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;

        let rows = stmt
            .query_map(rusqlite::params![session_id, branch_id], |row: &rusqlite::Row<'_>| {
                let seq: i64 = row.get(0)?;
                let role_str: String = row.get(1)?;
                let content_json: String = row.get(2)?;
                Ok((seq, role_str, content_json))
            })
            .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;

        let mut messages = Vec::new();
        for row in rows {
            let (seq, role_str, content_json) =
                row.map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;
            let role = str_to_role(&role_str)?;
            let content: Vec<MessageContent> = serde_json::from_str(&content_json)
                .map_err(|e: serde_json::Error| SessionError::SerializationError(e.to_string()))?;
            messages.push((seq, Message { role, content }));
        }
        Ok(messages)
    }
}

/// Load raw messages for a branch (uncompacted only).
fn load_branch_messages_raw(
    conn: &DbConnection,
    session_id: &str,
    branch_id: &str,
) -> Result<Vec<Message>, SessionError> {
    let mut stmt = conn
        .prepare(
            "SELECT role, content FROM session_messages
             WHERE session_id = ?1 AND branch_id = ?2 AND is_compacted = 0
             ORDER BY seq ASC",
        )
        .map_err(|e: rusqlite::Error| SessionError::DbError(e.to_string()))?;

    let rows = stmt
        .query_map(rusqlite::params![session_id, branch_id], |row: &rusqlite::Row<'_>| {
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

    /// Helper: get main branch UUID for a session.
    fn main_branch_id(session_id: &str) -> String {
        SessionStore::resolve_branch_id(session_id, "main").unwrap()
    }

    #[test]
    fn test_create_session_creates_main_branch() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();

        let branches = store.list_branches(&session.id).unwrap();
        assert_eq!(branches.len(), 1);
        assert_eq!(branches[0].name, "main");
        assert!(branches[0].parent_branch_id.is_none());

        store.delete_session(&session.id).unwrap();
    }

    #[test]
    fn test_create_branch_basic() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();
        let main_id = main_branch_id(&session.id);

        // Add some messages to main
        store.save_messages(&session.id, &[Message::user("hello"), Message::assistant("hi")]).unwrap();

        let branch = store.create_branch(&session.id, &main_id, 0, "exploration").unwrap();
        assert_eq!(branch.name, "exploration");
        assert_eq!(branch.parent_branch_id.as_deref(), Some(main_id.as_str()));
        assert_eq!(branch.branch_point_seq, Some(0));

        let branches = store.list_branches(&session.id).unwrap();
        assert_eq!(branches.len(), 2);

        store.delete_session(&session.id).unwrap();
    }

    #[test]
    fn test_create_branch_invalid_parent() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();

        let result = store.create_branch(&session.id, "nonexistent", 0, "test");
        assert!(matches!(result, Err(SessionError::BranchNotFound(_))));

        store.delete_session(&session.id).unwrap();
    }

    #[test]
    fn test_create_branch_invalid_seq() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();
        let main_id = main_branch_id(&session.id);

        // No messages, so seq 5 is out of range
        let result = store.create_branch(&session.id, &main_id, 5, "bad");
        assert!(matches!(result, Err(SessionError::InvalidBranchPoint(_))));

        store.delete_session(&session.id).unwrap();
    }

    #[test]
    fn test_save_and_load_on_branch() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();
        let main_id = main_branch_id(&session.id);

        // Save on main branch
        store.save_message_on_branch(&session.id, &main_id, &Message::user("main msg")).unwrap();

        // Create a branch and save on it
        store.save_messages_on_branch(&session.id, &main_id, &[Message::assistant("reply")]).unwrap();
        let branch = store.create_branch(&session.id, &main_id, 1, "alt").unwrap();

        store.save_message_on_branch(&session.id, &branch.id, &Message::user("branch msg")).unwrap();

        // Load main branch: only main messages
        let main_msgs = store.load_messages_for_branch(&session.id, &main_id).unwrap();
        assert_eq!(main_msgs.len(), 2);

        // Load child branch: parent prefix + own messages
        let branch_msgs = store.load_messages_for_branch(&session.id, &branch.id).unwrap();
        assert!(branch_msgs.len() >= 3);

        store.delete_session(&session.id).unwrap();
    }

    #[test]
    fn test_mark_compacted_and_filter() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();
        let main_id = main_branch_id(&session.id);

        store.save_messages(&session.id, &[
            Message::user("msg0"),
            Message::assistant("msg1"),
            Message::user("msg2"),
            Message::assistant("msg3"),
        ]).unwrap();

        // Compact first 2 messages
        let count = store.mark_messages_compacted(&session.id, &main_id, 1).unwrap();
        assert_eq!(count, 2);

        // Load should only return uncompacted
        let msgs = store.load_messages(&session.id).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, Role::User); // msg2

        store.delete_session(&session.id).unwrap();
    }

    #[test]
    fn test_insert_summary_message() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();
        let main_id = main_branch_id(&session.id);

        store.save_messages(&session.id, &[Message::user("a"), Message::assistant("b")]).unwrap();

        // Mark first message compacted
        store.mark_messages_compacted(&session.id, &main_id, 0).unwrap();

        // Insert summary
        store.insert_summary_message(&session.id, &main_id, "User asked about colors", 1).unwrap();

        let msgs = store.load_messages(&session.id).unwrap();
        assert!(msgs.len() >= 1);

        let has_summary = msgs.iter().any(|m| {
            m.content.iter().any(|c| {
                matches!(c, MessageContent::Text { text } if text.contains("[Summary]"))
            })
        });
        assert!(has_summary);

        store.delete_session(&session.id).unwrap();
    }

    #[test]
    fn test_get_branch_tree() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();
        let main_id = main_branch_id(&session.id);

        store.save_messages(&session.id, &[Message::user("hello")]).unwrap();

        let b1 = store.create_branch(&session.id, &main_id, 0, "path-a").unwrap();
        store.save_message_on_branch(&session.id, &b1.id, &Message::user("a1")).unwrap();

        let b2 = store.create_branch(&session.id, &main_id, 0, "path-b").unwrap();
        store.save_message_on_branch(&session.id, &b2.id, &Message::user("b1")).unwrap();

        let tree = store.get_branch_tree(&session.id).unwrap();
        assert_eq!(tree.branch.name, "main");
        assert_eq!(tree.children.len(), 2);
        assert_eq!(tree.message_count, 1);

        store.delete_session(&session.id).unwrap();
    }

    #[test]
    fn test_delete_session_cascades_branches() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();
        let main_id = main_branch_id(&session.id);
        store.save_messages(&session.id, &[Message::user("hello")]).unwrap();
        store.create_branch(&session.id, &main_id, 0, "child").unwrap();

        let branches_before = store.list_branches(&session.id).unwrap();
        assert_eq!(branches_before.len(), 2);

        store.delete_session(&session.id).unwrap();

        let branches_after = store.list_branches(&session.id).unwrap();
        assert!(branches_after.is_empty());
    }

    #[test]
    fn test_backward_compat_load_messages() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();

        store.save_messages(&session.id, &[
            Message::user("hello"),
            Message::assistant("world"),
        ]).unwrap();

        let msgs = store.load_messages(&session.id).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[1].role, Role::Assistant);

        store.delete_session(&session.id).unwrap();
    }

    #[test]
    fn test_message_count_for_branch() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();
        let main_id = main_branch_id(&session.id);

        store.save_messages(&session.id, &[Message::user("a"), Message::assistant("b")]).unwrap();

        let count = store.get_message_count_for_branch(&session.id, &main_id).unwrap();
        assert_eq!(count, 2);

        store.delete_session(&session.id).unwrap();
    }

    #[test]
    fn test_load_raw_messages_for_branch() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();
        let main_id = main_branch_id(&session.id);

        store.save_messages(&session.id, &[Message::user("a"), Message::assistant("b")]).unwrap();

        let raw = store.load_raw_messages_for_branch(&session.id, &main_id).unwrap();
        assert_eq!(raw.len(), 2);
        assert_eq!(raw[0].0, 0);
        assert_eq!(raw[1].0, 1);

        store.delete_session(&session.id).unwrap();
    }
}
