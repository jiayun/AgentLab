use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::sse::{KeepAlive, Sse};
use axum::response::IntoResponse;
use serde::Deserialize;

use super::AppState;
use crate::agent::{chat_agent, main_agent};
use crate::db::{agents, conversations};
use crate::web::handlers::AppError;

#[derive(Deserialize)]
pub struct StreamQuery {
    pub session_id: Option<String>,
    pub message: Option<String>,
}

pub async fn configure_stream(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<StreamQuery>,
) -> Result<impl IntoResponse, AppError> {
    let agent = agents::get_agent(&state.db, &id)?
        .ok_or_else(|| anyhow::anyhow!("Agent not found"))?;

    let session_id = query
        .session_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let conv = conversations::get_or_create_conversation(&state.db, &id, &session_id)?;

    // If message provided, save it first
    if let Some(msg) = &query.message {
        conversations::add_message(&state.db, &conv.id, "user", msg, None, None)?;
    }

    tracing::info!(session_id = %session_id, agent_name = %agent.name, "SSE configure_stream started");

    let db = state.db.clone();
    let config = state.config.clone();
    let http_client = state.http_client.clone();
    let conv_id = conv.id.clone();

    let stream = main_agent::run_configure_stream(db, config, http_client, agent, conv_id);

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

pub async fn chat_stream(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<StreamQuery>,
) -> Result<impl IntoResponse, AppError> {
    let agent = agents::get_agent(&state.db, &id)?
        .ok_or_else(|| anyhow::anyhow!("Agent not found"))?;

    let session_id = query
        .session_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let conv = conversations::get_or_create_conversation(&state.db, &id, &session_id)?;

    // If message provided, save it first
    if let Some(msg) = &query.message {
        conversations::add_message(&state.db, &conv.id, "user", msg, None, None)?;
    }

    tracing::info!(session_id = %session_id, agent_name = %agent.name, "SSE chat_stream started");

    let db = state.db.clone();
    let config = state.config.clone();
    let http_client = state.http_client.clone();
    let conv_id = conv.id.clone();

    let stream = chat_agent::run_chat_stream(db, config, http_client, agent, conv_id);

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
