use anyhow::{Context, Result};
use serde::Serialize;

use super::DbPool;

#[derive(Debug, Clone, Serialize)]
pub struct Conversation {
    pub id: String,
    pub agent_id: String,
    pub session_id: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub tool_calls_json: Option<String>,
    pub tool_call_id: Option<String>,
    pub created_at: String,
}

pub fn get_or_create_conversation(
    db: &DbPool,
    agent_id: &str,
    session_id: &str,
) -> Result<Conversation> {
    let conn = db.lock();

    // Try to find existing
    let existing = conn
        .query_row(
            "SELECT id, agent_id, session_id, created_at
             FROM conversations WHERE agent_id = ?1 AND session_id = ?2
             ORDER BY created_at DESC LIMIT 1",
            rusqlite::params![agent_id, session_id],
            |row| {
                Ok(Conversation {
                    id: row.get(0)?,
                    agent_id: row.get(1)?,
                    session_id: row.get(2)?,
                    created_at: row.get(3)?,
                })
            },
        );

    match existing {
        Ok(conv) => Ok(conv),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO conversations (id, agent_id, session_id, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![&id, agent_id, session_id, &now],
            )?;
            Ok(Conversation {
                id,
                agent_id: agent_id.to_string(),
                session_id: session_id.to_string(),
                created_at: now,
            })
        }
        Err(e) => Err(e).context("Failed to query conversation"),
    }
}

pub fn add_message(
    db: &DbPool,
    conversation_id: &str,
    role: &str,
    content: &str,
    tool_calls_json: Option<&str>,
    tool_call_id: Option<&str>,
) -> Result<Message> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let conn = db.lock();
    conn.execute(
        "INSERT INTO messages (id, conversation_id, role, content, tool_calls_json, tool_call_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![&id, conversation_id, role, content, tool_calls_json, tool_call_id, &now],
    )
    .context("Failed to add message")?;

    Ok(Message {
        id,
        conversation_id: conversation_id.to_string(),
        role: role.to_string(),
        content: content.to_string(),
        tool_calls_json: tool_calls_json.map(String::from),
        tool_call_id: tool_call_id.map(String::from),
        created_at: now,
    })
}

pub fn get_messages(db: &DbPool, conversation_id: &str) -> Result<Vec<Message>> {
    let conn = db.lock();
    let mut stmt = conn.prepare(
        "SELECT id, conversation_id, role, content, tool_calls_json, tool_call_id, created_at
         FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC",
    )?;

    let messages = stmt
        .query_map(rusqlite::params![conversation_id], |row| {
            Ok(Message {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                tool_calls_json: row.get(4)?,
                tool_call_id: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to get messages")?;

    Ok(messages)
}
