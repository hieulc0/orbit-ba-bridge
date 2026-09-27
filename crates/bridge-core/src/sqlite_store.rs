//! SQLite implementation of `ConversationStore`.

use crate::conversation::{Conversation, ConversationStatus, NewConversation};
use crate::message::{ConversationMessage, MessageKind, NewMessage};
use crate::participant::{ActorRole, NewParticipant, Participant, ParticipantSource};
use crate::store::ConversationStore;
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use protocol::{ConversationId, CorrelationId, MessageId, ParticipantId};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// SQLite-backed persistent conversation store.
pub struct SqliteConversationStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteConversationStore {
    /// Opens or creates an SQLite database at the specified filesystem path.
    pub fn open(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// Opens an in-memory SQLite database (ideal for tests and ephemeral sessions).
    pub fn open_in_memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> anyhow::Result<Self> {
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;

             CREATE TABLE IF NOT EXISTS conversations (
                 id TEXT PRIMARY KEY,
                 title TEXT,
                 status TEXT NOT NULL,
                 orbit_workflow_id TEXT,
                 external_conversation_ref TEXT,
                 created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL
             );

             CREATE TABLE IF NOT EXISTS participants (
                 id TEXT PRIMARY KEY,
                 conversation_id TEXT NOT NULL,
                 role TEXT NOT NULL,
                 display_name TEXT NOT NULL,
                 source TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 FOREIGN KEY (conversation_id) REFERENCES conversations(id)
             );

             CREATE TABLE IF NOT EXISTS messages (
                 id TEXT PRIMARY KEY,
                 conversation_id TEXT NOT NULL,
                 sequence INTEGER NOT NULL,
                 actor_id TEXT NOT NULL,
                 kind TEXT NOT NULL,
                 content TEXT NOT NULL,
                 correlation_id TEXT,
                 reply_to TEXT,
                 external_message_id TEXT,
                 orbit_workflow_id TEXT,
                 orbit_role_execution_id TEXT,
                 created_at INTEGER NOT NULL,
                 UNIQUE (conversation_id, sequence),
                 FOREIGN KEY (conversation_id) REFERENCES conversations(id),
                 FOREIGN KEY (actor_id) REFERENCES participants(id)
             );

             CREATE INDEX IF NOT EXISTS idx_messages_conv_seq ON messages (conversation_id, sequence);
             CREATE INDEX IF NOT EXISTS idx_messages_external ON messages (conversation_id, external_message_id);

             CREATE TABLE IF NOT EXISTS external_message_links (
                 message_id TEXT NOT NULL,
                 external_id TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 PRIMARY KEY (message_id, external_id),
                 FOREIGN KEY (message_id) REFERENCES messages(id)
             );

             CREATE TABLE IF NOT EXISTS artifact_refs (
                 id TEXT PRIMARY KEY,
                 conversation_id TEXT NOT NULL,
                 artifact_type TEXT NOT NULL,
                 digest TEXT,
                 orbit_artifact_id TEXT,
                 created_at INTEGER NOT NULL,
                 FOREIGN KEY (conversation_id) REFERENCES conversations(id)
             );",
        )?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }
}

fn dt_from_timestamp(ts: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(ts, 0).single().unwrap_or_default()
}

#[async_trait]
impl ConversationStore for SqliteConversationStore {
    async fn create_conversation(&self, input: NewConversation) -> anyhow::Result<Conversation> {
        let id = input.id.unwrap_or_else(ConversationId::generate);
        let now = Utc::now();
        let now_ts = now.timestamp();
        let status = ConversationStatus::Active;

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO conversations (id, title, status, orbit_workflow_id, external_conversation_ref, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id.as_str(),
                input.title,
                status.as_str(),
                input.orbit_workflow_id,
                input.external_conversation_ref,
                now_ts,
                now_ts,
            ],
        )?;

        Ok(Conversation {
            id,
            title: input.title,
            status,
            created_at: now,
            updated_at: now,
            orbit_workflow_id: input.orbit_workflow_id,
            external_conversation_ref: input.external_conversation_ref,
        })
    }

    async fn get_conversation(&self, id: &ConversationId) -> anyhow::Result<Option<Conversation>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, status, orbit_workflow_id, external_conversation_ref, created_at, updated_at
             FROM conversations WHERE id = ?1",
        )?;

        let conv = stmt
            .query_row(params![id.as_str()], |row| {
                let id_str: String = row.get(0)?;
                let title: Option<String> = row.get(1)?;
                let status_str: String = row.get(2)?;
                let orbit_workflow_id: Option<String> = row.get(3)?;
                let external_conversation_ref: Option<String> = row.get(4)?;
                let created_ts: i64 = row.get(5)?;
                let updated_ts: i64 = row.get(6)?;

                let status = ConversationStatus::from_str_name(&status_str)
                    .unwrap_or(ConversationStatus::Active);

                Ok(Conversation {
                    id: ConversationId::new(id_str).unwrap(),
                    title,
                    status,
                    created_at: dt_from_timestamp(created_ts),
                    updated_at: dt_from_timestamp(updated_ts),
                    orbit_workflow_id,
                    external_conversation_ref,
                })
            })
            .optional()?;

        Ok(conv)
    }

    async fn update_conversation_status(
        &self,
        id: &ConversationId,
        status: ConversationStatus,
    ) -> anyhow::Result<()> {
        let now_ts = Utc::now().timestamp();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE conversations SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status.as_str(), now_ts, id.as_str()],
        )?;
        Ok(())
    }

    async fn add_participant(&self, input: NewParticipant) -> anyhow::Result<Participant> {
        let id = input.id.unwrap_or_else(ParticipantId::generate);
        let now = Utc::now();
        let now_ts = now.timestamp();

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO participants (id, conversation_id, role, display_name, source, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id.as_str(),
                input.conversation_id.as_str(),
                input.role.as_str(),
                input.display_name,
                input.source.as_str(),
                now_ts,
            ],
        )?;

        Ok(Participant {
            id,
            conversation_id: input.conversation_id,
            role: input.role,
            display_name: input.display_name,
            source: input.source,
            created_at: now,
        })
    }

    async fn get_participant(&self, id: &ParticipantId) -> anyhow::Result<Option<Participant>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, role, display_name, source, created_at
             FROM participants WHERE id = ?1",
        )?;

        let part = stmt
            .query_row(params![id.as_str()], |row| {
                let id_str: String = row.get(0)?;
                let conv_str: String = row.get(1)?;
                let role_str: String = row.get(2)?;
                let display_name: String = row.get(3)?;
                let source_str: String = row.get(4)?;
                let created_ts: i64 = row.get(5)?;

                let role = ActorRole::from_str_name(&role_str).unwrap_or(ActorRole::Human);
                let source = ParticipantSource::from_str_name(&source_str)
                    .unwrap_or(ParticipantSource::System);

                Ok(Participant {
                    id: ParticipantId::new(id_str).unwrap(),
                    conversation_id: ConversationId::new(conv_str).unwrap(),
                    role,
                    display_name,
                    source,
                    created_at: dt_from_timestamp(created_ts),
                })
            })
            .optional()?;

        Ok(part)
    }

    async fn list_participants(
        &self,
        conversation_id: &ConversationId,
    ) -> anyhow::Result<Vec<Participant>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, role, display_name, source, created_at
             FROM participants WHERE conversation_id = ?1 ORDER BY created_at ASC",
        )?;

        let iter = stmt.query_map(params![conversation_id.as_str()], |row| {
            let id_str: String = row.get(0)?;
            let conv_str: String = row.get(1)?;
            let role_str: String = row.get(2)?;
            let display_name: String = row.get(3)?;
            let source_str: String = row.get(4)?;
            let created_ts: i64 = row.get(5)?;

            let role = ActorRole::from_str_name(&role_str).unwrap_or(ActorRole::Human);
            let source =
                ParticipantSource::from_str_name(&source_str).unwrap_or(ParticipantSource::System);

            Ok(Participant {
                id: ParticipantId::new(id_str).unwrap(),
                conversation_id: ConversationId::new(conv_str).unwrap(),
                role,
                display_name,
                source,
                created_at: dt_from_timestamp(created_ts),
            })
        })?;

        let mut list = Vec::new();
        for item in iter {
            list.push(item?);
        }
        Ok(list)
    }

    async fn append_message(&self, input: NewMessage) -> anyhow::Result<ConversationMessage> {
        let id = input.id.unwrap_or_else(MessageId::generate);
        let now = Utc::now();
        let now_ts = now.timestamp();

        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        // Calculate strictly monotonic sequence within the conversation
        let sequence_i64: i64 = tx.query_row(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM messages WHERE conversation_id = ?1",
            params![input.conversation_id.as_str()],
            |row| row.get(0),
        )?;
        let sequence = sequence_i64 as u64;

        let corr_str = input.correlation_id.as_ref().map(|c| c.as_str());
        let reply_str = input.reply_to.as_ref().map(|r| r.as_str());

        tx.execute(
            "INSERT INTO messages (id, conversation_id, sequence, actor_id, kind, content, correlation_id, reply_to, external_message_id, orbit_workflow_id, orbit_role_execution_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                id.as_str(),
                input.conversation_id.as_str(),
                sequence_i64,
                input.actor_id.as_str(),
                input.kind.as_str(),
                input.content,
                corr_str,
                reply_str,
                input.external_message_id,
                input.orbit_workflow_id,
                input.orbit_role_execution_id,
                now_ts,
            ],
        )?;

        tx.execute(
            "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
            params![now_ts, input.conversation_id.as_str()],
        )?;

        tx.commit()?;

        Ok(ConversationMessage {
            id,
            conversation_id: input.conversation_id,
            sequence,
            actor_id: input.actor_id,
            kind: input.kind,
            content: input.content,
            correlation_id: input.correlation_id,
            reply_to: input.reply_to,
            external_message_id: input.external_message_id,
            orbit_workflow_id: input.orbit_workflow_id,
            orbit_role_execution_id: input.orbit_role_execution_id,
            created_at: now,
        })
    }

    async fn get_message(&self, id: &MessageId) -> anyhow::Result<Option<ConversationMessage>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, sequence, actor_id, kind, content, correlation_id, reply_to, external_message_id, orbit_workflow_id, orbit_role_execution_id, created_at
             FROM messages WHERE id = ?1",
        )?;

        let msg = stmt
            .query_row(params![id.as_str()], |row| {
                let id_str: String = row.get(0)?;
                let conv_str: String = row.get(1)?;
                let sequence_i64: i64 = row.get(2)?;
                let actor_str: String = row.get(3)?;
                let kind_str: String = row.get(4)?;
                let content: String = row.get(5)?;
                let corr_str: Option<String> = row.get(6)?;
                let reply_str: Option<String> = row.get(7)?;
                let external_message_id: Option<String> = row.get(8)?;
                let orbit_workflow_id: Option<String> = row.get(9)?;
                let orbit_role_execution_id: Option<String> = row.get(10)?;
                let created_ts: i64 = row.get(11)?;

                let kind =
                    MessageKind::from_str_name(&kind_str).unwrap_or(MessageKind::Conversation);
                let correlation_id = corr_str.map(|s| CorrelationId::new(s).unwrap());
                let reply_to = reply_str.map(|s| MessageId::new(s).unwrap());

                Ok(ConversationMessage {
                    id: MessageId::new(id_str).unwrap(),
                    conversation_id: ConversationId::new(conv_str).unwrap(),
                    sequence: sequence_i64 as u64,
                    actor_id: ParticipantId::new(actor_str).unwrap(),
                    kind,
                    content,
                    correlation_id,
                    reply_to,
                    external_message_id,
                    orbit_workflow_id,
                    orbit_role_execution_id,
                    created_at: dt_from_timestamp(created_ts),
                })
            })
            .optional()?;

        Ok(msg)
    }

    async fn load_messages(
        &self,
        conversation_id: &ConversationId,
        after_sequence: Option<u64>,
        limit: usize,
    ) -> anyhow::Result<Vec<ConversationMessage>> {
        let conn = self.conn.lock().unwrap();
        let min_seq_i64 = after_sequence.unwrap_or(0) as i64;

        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, sequence, actor_id, kind, content, correlation_id, reply_to, external_message_id, orbit_workflow_id, orbit_role_execution_id, created_at
             FROM messages
             WHERE conversation_id = ?1 AND sequence > ?2
             ORDER BY sequence ASC
             LIMIT ?3",
        )?;

        let iter = stmt.query_map(
            params![conversation_id.as_str(), min_seq_i64, limit as i64],
            |row| {
                let id_str: String = row.get(0)?;
                let conv_str: String = row.get(1)?;
                let sequence_i64: i64 = row.get(2)?;
                let actor_str: String = row.get(3)?;
                let kind_str: String = row.get(4)?;
                let content: String = row.get(5)?;
                let corr_str: Option<String> = row.get(6)?;
                let reply_str: Option<String> = row.get(7)?;
                let external_message_id: Option<String> = row.get(8)?;
                let orbit_workflow_id: Option<String> = row.get(9)?;
                let orbit_role_execution_id: Option<String> = row.get(10)?;
                let created_ts: i64 = row.get(11)?;

                let kind =
                    MessageKind::from_str_name(&kind_str).unwrap_or(MessageKind::Conversation);
                let correlation_id = corr_str.map(|s| CorrelationId::new(s).unwrap());
                let reply_to = reply_str.map(|s| MessageId::new(s).unwrap());

                Ok(ConversationMessage {
                    id: MessageId::new(id_str).unwrap(),
                    conversation_id: ConversationId::new(conv_str).unwrap(),
                    sequence: sequence_i64 as u64,
                    actor_id: ParticipantId::new(actor_str).unwrap(),
                    kind,
                    content,
                    correlation_id,
                    reply_to,
                    external_message_id,
                    orbit_workflow_id,
                    orbit_role_execution_id,
                    created_at: dt_from_timestamp(created_ts),
                })
            },
        )?;

        let mut list = Vec::new();
        for item in iter {
            list.push(item?);
        }
        Ok(list)
    }

    async fn link_external_message(
        &self,
        message_id: &MessageId,
        external_id: &str,
    ) -> anyhow::Result<()> {
        let now_ts = Utc::now().timestamp();
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "UPDATE messages SET external_message_id = ?1 WHERE id = ?2",
            params![external_id, message_id.as_str()],
        )?;

        conn.execute(
            "INSERT OR IGNORE INTO external_message_links (message_id, external_id, created_at)
             VALUES (?1, ?2, ?3)",
            params![message_id.as_str(), external_id, now_ts],
        )?;

        Ok(())
    }

    async fn find_message_by_external_id(
        &self,
        conversation_id: &ConversationId,
        external_id: &str,
    ) -> anyhow::Result<Option<ConversationMessage>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, sequence, actor_id, kind, content, correlation_id, reply_to, external_message_id, orbit_workflow_id, orbit_role_execution_id, created_at
             FROM messages
             WHERE conversation_id = ?1 AND external_message_id = ?2",
        )?;

        let msg = stmt
            .query_row(params![conversation_id.as_str(), external_id], |row| {
                let id_str: String = row.get(0)?;
                let conv_str: String = row.get(1)?;
                let sequence_i64: i64 = row.get(2)?;
                let actor_str: String = row.get(3)?;
                let kind_str: String = row.get(4)?;
                let content: String = row.get(5)?;
                let corr_str: Option<String> = row.get(6)?;
                let reply_str: Option<String> = row.get(7)?;
                let external_message_id: Option<String> = row.get(8)?;
                let orbit_workflow_id: Option<String> = row.get(9)?;
                let orbit_role_execution_id: Option<String> = row.get(10)?;
                let created_ts: i64 = row.get(11)?;

                let kind =
                    MessageKind::from_str_name(&kind_str).unwrap_or(MessageKind::Conversation);
                let correlation_id = corr_str.map(|s| CorrelationId::new(s).unwrap());
                let reply_to = reply_str.map(|s| MessageId::new(s).unwrap());

                Ok(ConversationMessage {
                    id: MessageId::new(id_str).unwrap(),
                    conversation_id: ConversationId::new(conv_str).unwrap(),
                    sequence: sequence_i64 as u64,
                    actor_id: ParticipantId::new(actor_str).unwrap(),
                    kind,
                    content,
                    correlation_id,
                    reply_to,
                    external_message_id,
                    orbit_workflow_id,
                    orbit_role_execution_id,
                    created_at: dt_from_timestamp(created_ts),
                })
            })
            .optional()?;

        Ok(msg)
    }
}
