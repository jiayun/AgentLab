use anyhow::{Context, Result};
use serde::Serialize;

use super::DbPool;

#[derive(Debug, Clone, Serialize)]
pub struct AgentSkill {
    pub id: String,
    pub agent_id: String,
    pub name: String,
    pub description: String,
    pub openapi_spec: String,
    pub parsed_tools_json: String,
    pub base_url: String,
    pub auth_header: Option<String>,
    pub auth_value: Option<String>,
    pub created_at: String,
}

pub fn list_skills(db: &DbPool, agent_id: &str) -> Result<Vec<AgentSkill>> {
    let conn = db.lock();
    let mut stmt = conn.prepare(
        "SELECT id, agent_id, name, description, openapi_spec, parsed_tools_json,
                base_url, auth_header, auth_value, created_at
         FROM agent_skills WHERE agent_id = ?1 ORDER BY created_at ASC",
    )?;

    let skills = stmt
        .query_map(rusqlite::params![agent_id], |row| {
            Ok(AgentSkill {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                openapi_spec: row.get(4)?,
                parsed_tools_json: row.get(5)?,
                base_url: row.get(6)?,
                auth_header: row.get(7)?,
                auth_value: row.get(8)?,
                created_at: row.get(9)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to list skills")?;

    Ok(skills)
}

pub fn create_skill(
    db: &DbPool,
    agent_id: &str,
    name: &str,
    description: &str,
    openapi_spec: &str,
    parsed_tools_json: &str,
    base_url: &str,
    auth_header: Option<&str>,
    auth_value: Option<&str>,
) -> Result<AgentSkill> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let conn = db.lock();
    conn.execute(
        "INSERT INTO agent_skills (id, agent_id, name, description, openapi_spec, parsed_tools_json, base_url, auth_header, auth_value, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![&id, agent_id, name, description, openapi_spec, parsed_tools_json, base_url, auth_header, auth_value, &now],
    )
    .context("Failed to create skill")?;

    Ok(AgentSkill {
        id,
        agent_id: agent_id.to_string(),
        name: name.to_string(),
        description: description.to_string(),
        openapi_spec: openapi_spec.to_string(),
        parsed_tools_json: parsed_tools_json.to_string(),
        base_url: base_url.to_string(),
        auth_header: auth_header.map(String::from),
        auth_value: auth_value.map(String::from),
        created_at: now,
    })
}

pub fn delete_skill(db: &DbPool, agent_id: &str, skill_id: &str) -> Result<()> {
    let conn = db.lock();
    let affected = conn.execute(
        "DELETE FROM agent_skills WHERE id = ?1 AND agent_id = ?2",
        rusqlite::params![skill_id, agent_id],
    )?;

    if affected == 0 {
        anyhow::bail!("Skill not found");
    }
    Ok(())
}
