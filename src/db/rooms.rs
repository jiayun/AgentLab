use anyhow::{Context, Result};
use serde::Serialize;

use super::DbPool;

#[derive(Debug, Clone, Serialize)]
pub struct Room {
    pub id: String,
    pub name: String,
    pub description: String,
    pub orchestrator_agent_id: String,
    pub status: String,
    pub scenario: String,
    pub max_turns: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomParticipant {
    pub id: String,
    pub room_id: String,
    pub agent_id: Option<String>,
    pub role: String,
    pub alias: String,
    pub private_context: String,
    pub is_human: bool,
    pub joined_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomMessage {
    pub id: String,
    pub room_id: String,
    pub sender_alias: String,
    pub visibility: String,
    pub target_alias: String,
    pub content: String,
    pub message_type: String,
    pub turn_number: i64,
    pub created_at: String,
}

pub fn create_room(
    db: &DbPool,
    name: &str,
    description: &str,
    orchestrator_agent_id: &str,
    scenario: &str,
    max_turns: i64,
) -> Result<Room> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let conn = db.lock();
    conn.execute(
        "INSERT INTO rooms (id, name, description, orchestrator_agent_id, scenario, max_turns, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![&id, name, description, orchestrator_agent_id, scenario, max_turns, &now, &now],
    )
    .context("Failed to create room")?;
    drop(conn);

    get_room(db, &id)?.context("Room just created but not found")
}

pub fn get_room(db: &DbPool, id: &str) -> Result<Option<Room>> {
    let conn = db.lock();
    let result = conn
        .query_row(
            "SELECT id, name, description, orchestrator_agent_id, status, scenario, max_turns, created_at, updated_at
             FROM rooms WHERE id = ?1",
            rusqlite::params![id],
            |row| {
                Ok(Room {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    orchestrator_agent_id: row.get(3)?,
                    status: row.get(4)?,
                    scenario: row.get(5)?,
                    max_turns: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            },
        )
        .optional()
        .context("Failed to get room")?;
    Ok(result)
}

pub fn list_rooms(db: &DbPool) -> Result<Vec<Room>> {
    let conn = db.lock();
    let mut stmt = conn.prepare(
        "SELECT id, name, description, orchestrator_agent_id, status, scenario, max_turns, created_at, updated_at
         FROM rooms ORDER BY created_at DESC",
    )?;

    let rooms = stmt
        .query_map([], |row| {
            Ok(Room {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                orchestrator_agent_id: row.get(3)?,
                status: row.get(4)?,
                scenario: row.get(5)?,
                max_turns: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to list rooms")?;

    Ok(rooms)
}

pub fn update_room_status(db: &DbPool, id: &str, status: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let conn = db.lock();
    let affected = conn.execute(
        "UPDATE rooms SET status = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![status, &now, id],
    )?;
    if affected == 0 {
        anyhow::bail!("Room not found");
    }
    Ok(())
}

pub fn add_participant(
    db: &DbPool,
    room_id: &str,
    agent_id: Option<&str>,
    role: &str,
    alias: &str,
    private_context: &str,
    is_human: bool,
) -> Result<RoomParticipant> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let conn = db.lock();
    conn.execute(
        "INSERT INTO room_participants (id, room_id, agent_id, role, alias, private_context, is_human, joined_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![&id, room_id, agent_id, role, alias, private_context, is_human as i32, &now],
    )
    .context("Failed to add participant")?;

    Ok(RoomParticipant {
        id,
        room_id: room_id.to_string(),
        agent_id: agent_id.map(String::from),
        role: role.to_string(),
        alias: alias.to_string(),
        private_context: private_context.to_string(),
        is_human,
        joined_at: now,
    })
}

pub fn get_participants(db: &DbPool, room_id: &str) -> Result<Vec<RoomParticipant>> {
    let conn = db.lock();
    let mut stmt = conn.prepare(
        "SELECT id, room_id, agent_id, role, alias, private_context, is_human, joined_at
         FROM room_participants WHERE room_id = ?1 ORDER BY joined_at",
    )?;

    let participants = stmt
        .query_map(rusqlite::params![room_id], |row| {
            Ok(RoomParticipant {
                id: row.get(0)?,
                room_id: row.get(1)?,
                agent_id: row.get(2)?,
                role: row.get(3)?,
                alias: row.get(4)?,
                private_context: row.get(5)?,
                is_human: row.get::<_, i32>(6)? != 0,
                joined_at: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to get participants")?;

    Ok(participants)
}

pub fn add_room_message(
    db: &DbPool,
    room_id: &str,
    sender_alias: &str,
    visibility: &str,
    target_alias: &str,
    content: &str,
    message_type: &str,
    turn_number: i64,
) -> Result<RoomMessage> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let conn = db.lock();
    conn.execute(
        "INSERT INTO room_messages (id, room_id, sender_alias, visibility, target_alias, content, message_type, turn_number, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![&id, room_id, sender_alias, visibility, target_alias, content, message_type, turn_number, &now],
    )
    .context("Failed to add room message")?;

    Ok(RoomMessage {
        id,
        room_id: room_id.to_string(),
        sender_alias: sender_alias.to_string(),
        visibility: visibility.to_string(),
        target_alias: target_alias.to_string(),
        content: content.to_string(),
        message_type: message_type.to_string(),
        turn_number,
        created_at: now,
    })
}

pub fn get_room_messages(db: &DbPool, room_id: &str, limit: i64) -> Result<Vec<RoomMessage>> {
    let conn = db.lock();
    let mut stmt = conn.prepare(
        "SELECT id, room_id, sender_alias, visibility, target_alias, content, message_type, turn_number, created_at
         FROM room_messages WHERE room_id = ?1
         ORDER BY created_at DESC LIMIT ?2",
    )?;

    let mut messages = stmt
        .query_map(rusqlite::params![room_id, limit], |row| {
            Ok(RoomMessage {
                id: row.get(0)?,
                room_id: row.get(1)?,
                sender_alias: row.get(2)?,
                visibility: row.get(3)?,
                target_alias: row.get(4)?,
                content: row.get(5)?,
                message_type: row.get(6)?,
                turn_number: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to get room messages")?;

    messages.reverse(); // chronological order
    Ok(messages)
}

/// Get messages visible to a specific alias: public + system + private where they are sender or target
pub fn get_visible_messages(
    db: &DbPool,
    room_id: &str,
    alias: &str,
    limit: i64,
) -> Result<Vec<RoomMessage>> {
    let conn = db.lock();
    let mut stmt = conn.prepare(
        "SELECT id, room_id, sender_alias, visibility, target_alias, content, message_type, turn_number, created_at
         FROM room_messages
         WHERE room_id = ?1
           AND (visibility = 'public' OR visibility = 'system'
                OR (visibility = 'private' AND (sender_alias = ?2 OR target_alias = ?2)))
         ORDER BY created_at DESC LIMIT ?3",
    )?;

    let mut messages = stmt
        .query_map(rusqlite::params![room_id, alias, limit], |row| {
            Ok(RoomMessage {
                id: row.get(0)?,
                room_id: row.get(1)?,
                sender_alias: row.get(2)?,
                visibility: row.get(3)?,
                target_alias: row.get(4)?,
                content: row.get(5)?,
                message_type: row.get(6)?,
                turn_number: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to get visible messages")?;

    messages.reverse();
    Ok(messages)
}

pub fn delete_room(db: &DbPool, id: &str) -> Result<()> {
    let conn = db.lock();
    conn.execute("DELETE FROM room_messages WHERE room_id = ?1", rusqlite::params![id])?;
    conn.execute("DELETE FROM room_participants WHERE room_id = ?1", rusqlite::params![id])?;
    conn.execute("DELETE FROM rooms WHERE id = ?1", rusqlite::params![id])?;
    Ok(())
}

// rusqlite optional helper
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
