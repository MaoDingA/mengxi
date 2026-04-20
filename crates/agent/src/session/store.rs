// session/store.rs — SessionStore: SQLite-backed session persistence with branching & compaction.
// All DB operations are delegated to mengxi_core::db agent-session API (no direct rusqlite).

use crate::llm::{Message, MessageContent, Role};
use crate::session::types::{Branch, BranchTreeNode, Session, SessionError, SessionInfo};
use mengxi_core::db::{self, DbConnection};

/// Session persistence store. Stateless -- opens a DB connection per operation.
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
        db::open_db().map_err(|e| SessionError::DbError(e.to_string()))
    }

    /// Resolve a branch UUID by name within a session.
    pub fn resolve_branch_id(session_id: &str, name: &str) -> Result<String, SessionError> {
        let conn = Self::conn()?;
        db::agent_branch_resolve_id(&conn, session_id, name)
            .map_err(|e| match e {
                db::DbError::Query(msg) if msg.contains("no rows") => {
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
        let conn = Self::conn()?;

        // Create session record
        let id = db::agent_session_create(&conn, provider, model)
            .map_err(|e| SessionError::DbError(e.to_string()))?;

        // Create the "main" branch (no parent -- root of the tree)
        let now = epoch_now();
        let _main_branch = db::agent_branch_create(&conn, &id, None, -1, "main")
            .map_err(|e| SessionError::DbError(e.to_string()))?;

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
        let record = db::agent_session_get(&conn, id)
            .map_err(|e| SessionError::DbError(e.to_string()))?;
        Ok(record.map(|r| Session {
            id: r.id,
            title: r.title,
            provider: r.provider,
            model: r.model,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }))
    }

    /// List all sessions ordered by most recently updated.
    pub fn list_sessions(&self) -> Result<Vec<SessionInfo>, SessionError> {
        let conn = Self::conn()?;
        let rows = db::agent_session_list(&conn)
            .map_err(|e| SessionError::DbError(e.to_string()))?;
        Ok(rows.into_iter().map(|r| SessionInfo {
            id: r.id,
            title: r.title,
            message_count: r.message_count,
            updated_at: r.updated_at,
        }).collect())
    }

    /// Update a session's title.
    pub fn update_title(&self, session_id: &str, title: &str) -> Result<(), SessionError> {
        let conn = Self::conn()?;
        db::agent_session_update_title(&conn, session_id, title)
            .map_err(|e| SessionError::DbError(e.to_string()))
    }

    /// Delete a session and all its branches/messages (CASCADE).
    pub fn delete_session(&self, session_id: &str) -> Result<(), SessionError> {
        let conn = Self::conn()?;
        db::agent_session_delete(&conn, session_id)
            .map_err(|e| SessionError::DbError(e.to_string()))
    }

    // ── Message operations (main branch, backward-compatible) ────

    /// Save a single message to a session (main branch).
    pub fn save_message(
        &self,
        session_id: &str,
        _seq: i64,
        msg: &Message,
    ) -> Result<(), SessionError> {
        let conn = Self::conn()?;
        let content_json = serde_json::to_string(&msg.content)
            .map_err(|e: serde_json::Error| SessionError::SerializationError(e.to_string()))?;
        let role_str = role_to_str(&msg.role);

        db::agent_message_save_main(&conn, session_id, role_str, &content_json)
            .map_err(|e| SessionError::DbError(e.to_string()))?;

        Ok(())
    }

    /// Save multiple messages in batch (main branch).
    pub fn save_messages(
        &self,
        session_id: &str,
        messages: &[Message],
    ) -> Result<(), SessionError> {
        let conn = Self::conn()?;

        for msg in messages {
            let content_json = serde_json::to_string(&msg.content)
                .map_err(|e: serde_json::Error| SessionError::SerializationError(e.to_string()))?;
            let role_str = role_to_str(&msg.role);

            db::agent_message_save_main(&conn, session_id, role_str, &content_json)
                .map_err(|e| SessionError::DbError(e.to_string()))?;
        }

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
        let branches = db::agent_branch_list(&conn, session_id)
            .map_err(|e| SessionError::DbError(e.to_string()))?;
        let parent_exists = branches.iter().any(|b| b.id == parent_branch_id);

        if !parent_exists {
            return Err(SessionError::BranchNotFound(parent_branch_id.to_string()));
        }

        // Validate branch_point_seq is within parent's message range
        let raw_msgs = db::agent_message_load_for_branch(&conn, session_id, parent_branch_id)
            .map_err(|e| SessionError::DbError(e.to_string()))?;
        let parent_max_seq = raw_msgs.last().map_or(-1, |m| m.seq);

        if branch_point_seq > parent_max_seq {
            return Err(SessionError::InvalidBranchPoint(format!(
                "seq {} exceeds parent branch max seq {}",
                branch_point_seq, parent_max_seq
            )));
        }

        let record = db::agent_branch_create(&conn, session_id, Some(parent_branch_id), branch_point_seq, name)
            .map_err(|e| SessionError::DbError(e.to_string()))?;

        Ok(Branch {
            id: record.id,
            session_id: record.session_id,
            parent_branch_id: record.parent_branch_id,
            branch_point_seq: record.branch_point_seq,
            name: record.name,
            created_at: record.created_at,
        })
    }

    /// Get a specific branch by its UUID.
    pub fn get_branch(&self, session_id: &str, branch_id: &str) -> Result<Branch, SessionError> {
        let conn = Self::conn()?;
        let branches = db::agent_branch_list(&conn, session_id)
            .map_err(|e| SessionError::DbError(e.to_string()))?;

        branches
            .into_iter()
            .find(|b| b.id == branch_id)
            .map(|r| Branch {
                id: r.id,
                session_id: r.session_id,
                parent_branch_id: r.parent_branch_id,
                branch_point_seq: r.branch_point_seq,
                name: r.name,
                created_at: r.created_at,
            })
            .ok_or_else(|| SessionError::BranchNotFound(branch_id.to_string()))
    }

    /// Get the main branch for a session.
    pub fn get_main_branch(&self, session_id: &str) -> Result<Branch, SessionError> {
        let conn = Self::conn()?;
        let record = db::agent_branch_get_main(&conn, session_id)
            .map_err(|e| SessionError::DbError(e.to_string()))?;

        record
            .map(|r| Branch {
                id: r.id,
                session_id: r.session_id,
                parent_branch_id: r.parent_branch_id,
                branch_point_seq: r.branch_point_seq,
                name: r.name,
                created_at: r.created_at,
            })
            .ok_or_else(|| SessionError::BranchNotFound("main".to_string()))
    }

    /// List all branches for a session.
    pub fn list_branches(&self, session_id: &str) -> Result<Vec<Branch>, SessionError> {
        let conn = Self::conn()?;
        let records = db::agent_branch_list(&conn, session_id)
            .map_err(|e| SessionError::DbError(e.to_string()))?;

        Ok(records.into_iter().map(|r| Branch {
            id: r.id,
            session_id: r.session_id,
            parent_branch_id: r.parent_branch_id,
            branch_point_seq: r.branch_point_seq,
            name: r.name,
            created_at: r.created_at,
        }).collect())
    }

    /// Build a branch tree for navigation.
    pub fn get_branch_tree(&self, session_id: &str) -> Result<BranchTreeNode, SessionError> {
        let branches = self.list_branches(session_id)?;
        let conn = Self::conn()?;

        // Count messages per branch
        let mut counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        for b in &branches {
            let msgs = db::agent_message_load_for_branch(&conn, session_id, &b.id)
                .unwrap_or_default();
            counts.insert(b.id.clone(), msgs.len() as i64);
        }

        // Find root (main branch -- has name = 'main')
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

        let seq = db::agent_message_save(&conn, session_id, branch_id, role_str, &content_json)
            .map_err(|e| SessionError::DbError(e.to_string()))?;

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

        for msg in messages {
            let content_json = serde_json::to_string(&msg.content)
                .map_err(|e: serde_json::Error| SessionError::SerializationError(e.to_string()))?;
            let role_str = role_to_str(&msg.role);

            db::agent_message_save(&conn, session_id, branch_id, role_str, &content_json)
                .map_err(|e| SessionError::DbError(e.to_string()))?;
        }

        Ok(())
    }

    /// Load messages for a specific branch (by UUID), including parent prefix up to branch point.
    pub fn load_messages_for_branch(
        &self,
        session_id: &str,
        branch_id: &str,
    ) -> Result<Vec<Message>, SessionError> {
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
        let own = load_branch_messages_raw(session_id, branch_id)?;
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
        let records = db::agent_message_load_for_branch(&conn, session_id, branch_id)
            .map_err(|e| SessionError::DbError(e.to_string()))?;
        Ok(records.len() as i64)
    }

    // ── Compaction DB operations ─────────────────────────────────

    /// Mark messages as compacted up to a given seq.
    pub fn mark_messages_compacted(
        &self,
        session_id: &str,
        branch_id: &str,
        up_to_seq: i64,
    ) -> Result<usize, SessionError> {
        let conn = Self::conn()?;
        let count = db::agent_message_mark_compacted(&conn, session_id, branch_id, up_to_seq)
            .map_err(|e| SessionError::DbError(e.to_string()))?;
        Ok(count)
    }

    /// Insert a summary message (system role) at a given seq in a branch.
    pub fn insert_summary_message(
        &self,
        session_id: &str,
        branch_id: &str,
        summary_text: &str,
        _at_seq: i64,
    ) -> Result<(), SessionError> {
        let conn = Self::conn()?;
        let content_json = serde_json::to_string(&vec![MessageContent::Text {
            text: format!("[Summary] {}", summary_text),
        }])
        .map_err(|e: serde_json::Error| SessionError::SerializationError(e.to_string()))?;

        db::agent_session_insert_summary(&conn, session_id, branch_id, &content_json)
            .map_err(|e| SessionError::DbError(e.to_string()))?;

        Ok(())
    }

    /// Load raw messages with seq for a branch (used by compaction).
    pub fn load_raw_messages_for_branch(
        &self,
        session_id: &str,
        branch_id: &str,
    ) -> Result<Vec<(i64, Message)>, SessionError> {
        let conn = Self::conn()?;
        let records = db::agent_message_load_for_branch(&conn, session_id, branch_id)
            .map_err(|e| SessionError::DbError(e.to_string()))?;

        let mut messages = Vec::new();
        for rec in records {
            let role = str_to_role(&rec.role)?;
            let content: Vec<MessageContent> = serde_json::from_str(&rec.content_json)
                .map_err(|e: serde_json::Error| SessionError::SerializationError(e.to_string()))?;
            messages.push((rec.seq, Message { role, content }));
        }
        Ok(messages)
    }
}

/// Load raw messages for a branch via core API (uncompacted only).
fn load_branch_messages_raw(
    session_id: &str,
    branch_id: &str,
) -> Result<Vec<Message>, SessionError> {
    let conn = SessionStore::conn()?;
    let records = db::agent_message_load_for_branch(&conn, session_id, branch_id)
        .map_err(|e| SessionError::DbError(e.to_string()))?;

    let mut messages = Vec::new();
    for rec in records {
        let role = str_to_role(&rec.role)?;
        let content: Vec<MessageContent> = serde_json::from_str(&rec.content_json)
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

fn epoch_now() -> i64 {
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

    // =================================================================
    // Task 4.2: SessionStore CRUD round-trip tests (3 tests)
    // =================================================================

    #[test]
    fn test_create_and_get_session_roundtrip() {
        let store = SessionStore::new();
        let session = store
            .create_session(Some("openai"), Some("gpt-4"))
            .unwrap();

        // Verify created session fields
        assert!(!session.id.is_empty());
        assert!(session.provider.as_deref() == Some("openai"));
        assert!(session.model.as_deref() == Some("gpt-4"));
        assert!(session.created_at > 0);
        assert!(session.updated_at > 0);

        // Retrieve the session by ID
        let fetched = store.get_session(&session.id).unwrap();
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.id, session.id);
        assert_eq!(fetched.provider, session.provider);
        assert_eq!(fetched.model, session.model);
        assert_eq!(fetched.created_at, session.created_at);

        store.delete_session(&session.id).unwrap();
    }

    #[test]
    fn test_create_save_main_message_and_load_messages_for_branch_roundtrip() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();
        let main_id = main_branch_id(&session.id);

        // Save messages to main branch via save_message (backward-compat API)
        store
            .save_message(&session.id, 0, &Message::user("Hello, assistant!"))
            .unwrap();
        store
            .save_message(&session.id, 1, &Message::assistant("Hi there! How can I help?"))
            .unwrap();

        // Load messages for the main branch
        let msgs = store
            .load_messages_for_branch(&session.id, &main_id)
            .unwrap();

        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[1].role, Role::Assistant);
        // Verify content survived serialization round-trip
        let user_text = msgs[0]
            .content
            .iter()
            .find_map(|c| match c {
                MessageContent::Text { text } => Some(text.clone()),
                _ => None,
            })
            .unwrap();
        assert!(user_text.contains("Hello"));

        store.delete_session(&session.id).unwrap();
    }

    #[test]
    fn test_get_nonexistent_session_returns_none() {
        let store = SessionStore::new();

        // Use a UUID that almost certainly doesn't exist
        let fake_id = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
        let result = store.get_session(fake_id).unwrap();
        assert!(
            result.is_none(),
            "Expected None for non-existent session ID, got {:?}",
            result
        );
    }

    #[test]
    fn test_list_sessions_returns_created_session() {
        let store = SessionStore::new();
        let session = store.create_session(Some("anthropic"), Some("claude-sonnet")).unwrap();

        let sessions = store.list_sessions().unwrap();
        assert!(!sessions.is_empty());
        let found = sessions.iter().any(|s| s.id == session.id);
        assert!(found, "list_sessions should include the newly created session");

        store.delete_session(&session.id).unwrap();
    }

    #[test]
    fn test_update_title_and_verify() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();

        store.update_title(&session.id, "My Test Session").unwrap();

        let fetched = store.get_session(&session.id).unwrap().unwrap();
        assert_eq!(fetched.title, "My Test Session");

        store.delete_session(&session.id).unwrap();
    }

    #[test]
    fn test_delete_session_then_get_returns_none() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();

        store.delete_session(&session.id).unwrap();

        let fetched = store.get_session(&session.id).unwrap();
        assert!(fetched.is_none(), "After delete, get_session should return None");
    }

    #[test]
    fn test_create_session_default_provider_model() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();

        assert!(session.provider.is_none());
        assert!(session.model.is_none());
        assert!(!session.id.is_empty());

        store.delete_session(&session.id).unwrap();
    }

    #[test]
    fn test_save_message_on_branch_increments_seq() {
        let store = SessionStore::new();
        let session = store.create_session(None, None).unwrap();
        let main_id = main_branch_id(&session.id);

        let seq1 = store.save_message_on_branch(&session.id, &main_id, &Message::user("msg1")).unwrap();
        let seq2 = store.save_message_on_branch(&session.id, &main_id, &Message::assistant("msg2")).unwrap();

        assert_eq!(seq1, 0);
        assert_eq!(seq2, 1);

        store.delete_session(&session.id).unwrap();
    }
}
