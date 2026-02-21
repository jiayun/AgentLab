pub mod agents;
pub mod conversations;
pub mod skills;

use anyhow::{Context, Result};
use parking_lot::Mutex;
use rusqlite::Connection;
use std::sync::Arc;

pub type DbPool = Arc<Mutex<Connection>>;

pub fn init_db() -> Result<DbPool> {
    let conn = Connection::open("agentlab.db").context("Failed to open SQLite database")?;

    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;

    run_migrations(&conn)?;
    seed_main_agent(&conn)?;

    Ok(Arc::new(Mutex::new(conn)))
}

fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS agents (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            display_name TEXT NOT NULL,
            soul TEXT NOT NULL DEFAULT '',
            personality TEXT NOT NULL DEFAULT '',
            communication_style TEXT NOT NULL DEFAULT '',
            instructions TEXT NOT NULL DEFAULT '',
            system_prompt TEXT NOT NULL DEFAULT '',
            model TEXT NOT NULL DEFAULT '',
            temperature REAL NOT NULL DEFAULT 0.7,
            is_main_agent INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS conversations (
            id TEXT PRIMARY KEY,
            agent_id TEXT NOT NULL REFERENCES agents(id),
            session_id TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            conversation_id TEXT NOT NULL REFERENCES conversations(id),
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            tool_calls_json TEXT,
            tool_call_id TEXT,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS agent_skills (
            id TEXT PRIMARY KEY,
            agent_id TEXT NOT NULL REFERENCES agents(id),
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            openapi_spec TEXT NOT NULL,
            parsed_tools_json TEXT NOT NULL,
            base_url TEXT NOT NULL DEFAULT '',
            auth_header TEXT,
            auth_value TEXT,
            created_at TEXT NOT NULL,
            UNIQUE(agent_id, name)
        );
        ",
    )
    .context("Failed to run migrations")?;

    Ok(())
}

fn seed_main_agent(conn: &Connection) -> Result<()> {
    let count: i64 =
        conn.query_row("SELECT COUNT(*) FROM agents WHERE is_main_agent = 1", [], |row| {
            row.get(0)
        })?;

    if count == 0 {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO agents (id, name, display_name, soul, is_main_agent, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6)",
            rusqlite::params![
                uuid::Uuid::new_v4().to_string(),
                "__main_agent__",
                "Main Agent",
                "You are the main configuration agent for AgentLab. You help users configure and customize other AI agents through natural conversation.",
                &now,
                &now,
            ],
        )?;
        tracing::info!("Seeded main agent");
    }

    Ok(())
}
