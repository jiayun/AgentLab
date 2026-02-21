use anyhow::{Context, Result};
use serde::Serialize;

use super::DbPool;

#[derive(Debug, Clone, Serialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub soul: String,
    pub personality: String,
    pub communication_style: String,
    pub instructions: String,
    pub system_prompt: String,
    pub model: String,
    pub temperature: f64,
    pub is_main_agent: bool,
    pub created_at: String,
    pub updated_at: String,
}

pub fn list_agents(db: &DbPool) -> Result<Vec<Agent>> {
    let conn = db.lock();
    let mut stmt = conn.prepare(
        "SELECT id, name, display_name, soul, personality, communication_style,
                instructions, system_prompt, model, temperature, is_main_agent,
                created_at, updated_at
         FROM agents WHERE is_main_agent = 0 ORDER BY created_at DESC",
    )?;

    let agents = stmt
        .query_map([], |row| {
            Ok(Agent {
                id: row.get(0)?,
                name: row.get(1)?,
                display_name: row.get(2)?,
                soul: row.get(3)?,
                personality: row.get(4)?,
                communication_style: row.get(5)?,
                instructions: row.get(6)?,
                system_prompt: row.get(7)?,
                model: row.get(8)?,
                temperature: row.get(9)?,
                is_main_agent: row.get::<_, i32>(10)? != 0,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to list agents")?;

    Ok(agents)
}

pub fn get_agent(db: &DbPool, id: &str) -> Result<Option<Agent>> {
    let conn = db.lock();
    let mut stmt = conn.prepare(
        "SELECT id, name, display_name, soul, personality, communication_style,
                instructions, system_prompt, model, temperature, is_main_agent,
                created_at, updated_at
         FROM agents WHERE id = ?1",
    )?;

    let agent = stmt
        .query_row(rusqlite::params![id], |row| {
            Ok(Agent {
                id: row.get(0)?,
                name: row.get(1)?,
                display_name: row.get(2)?,
                soul: row.get(3)?,
                personality: row.get(4)?,
                communication_style: row.get(5)?,
                instructions: row.get(6)?,
                system_prompt: row.get(7)?,
                model: row.get(8)?,
                temperature: row.get(9)?,
                is_main_agent: row.get::<_, i32>(10)? != 0,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        })
        .optional()
        .context("Failed to get agent")?;

    Ok(agent)
}

pub fn get_main_agent(db: &DbPool) -> Result<Agent> {
    let conn = db.lock();
    conn.query_row(
        "SELECT id, name, display_name, soul, personality, communication_style,
                instructions, system_prompt, model, temperature, is_main_agent,
                created_at, updated_at
         FROM agents WHERE is_main_agent = 1 LIMIT 1",
        [],
        |row| {
            Ok(Agent {
                id: row.get(0)?,
                name: row.get(1)?,
                display_name: row.get(2)?,
                soul: row.get(3)?,
                personality: row.get(4)?,
                communication_style: row.get(5)?,
                instructions: row.get(6)?,
                system_prompt: row.get(7)?,
                model: row.get(8)?,
                temperature: row.get(9)?,
                is_main_agent: row.get::<_, i32>(10)? != 0,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        },
    )
    .context("Main agent not found")
}

pub struct CreateAgent {
    pub name: String,
    pub display_name: String,
}

pub fn create_agent(db: &DbPool, input: &CreateAgent) -> Result<Agent> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let conn = db.lock();
    conn.execute(
        "INSERT INTO agents (id, name, display_name, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![&id, &input.name, &input.display_name, &now, &now],
    )
    .context("Failed to create agent")?;

    drop(conn);
    get_agent(db, &id)?.context("Agent just created but not found")
}

pub fn update_agent_field(db: &DbPool, id: &str, field: &str, value: &str) -> Result<()> {
    let allowed_fields = [
        "soul",
        "personality",
        "communication_style",
        "instructions",
        "system_prompt",
        "model",
    ];
    if !allowed_fields.contains(&field) {
        anyhow::bail!("Invalid field: {field}");
    }

    let now = chrono::Utc::now().to_rfc3339();
    let conn = db.lock();
    let sql = format!("UPDATE agents SET {field} = ?1, updated_at = ?2 WHERE id = ?3 AND is_main_agent = 0");
    let affected = conn.execute(&sql, rusqlite::params![value, &now, id])?;

    if affected == 0 {
        anyhow::bail!("Agent not found or is main agent");
    }
    Ok(())
}

pub fn update_agent_temperature(db: &DbPool, id: &str, temperature: f64) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let conn = db.lock();
    let affected = conn.execute(
        "UPDATE agents SET temperature = ?1, updated_at = ?2 WHERE id = ?3 AND is_main_agent = 0",
        rusqlite::params![temperature, &now, id],
    )?;

    if affected == 0 {
        anyhow::bail!("Agent not found or is main agent");
    }
    Ok(())
}

pub fn delete_agent(db: &DbPool, id: &str) -> Result<()> {
    let conn = db.lock();

    // Check not main agent
    let is_main: bool = conn.query_row(
        "SELECT is_main_agent FROM agents WHERE id = ?1",
        rusqlite::params![id],
        |row| row.get::<_, i32>(0).map(|v| v != 0),
    )?;

    if is_main {
        anyhow::bail!("Cannot delete main agent");
    }

    // Delete related data
    conn.execute(
        "DELETE FROM messages WHERE conversation_id IN (SELECT id FROM conversations WHERE agent_id = ?1)",
        rusqlite::params![id],
    )?;
    conn.execute(
        "DELETE FROM conversations WHERE agent_id = ?1",
        rusqlite::params![id],
    )?;
    conn.execute(
        "DELETE FROM agent_skills WHERE agent_id = ?1",
        rusqlite::params![id],
    )?;
    conn.execute("DELETE FROM agents WHERE id = ?1", rusqlite::params![id])?;

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
