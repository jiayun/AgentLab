use std::convert::Infallible;
use std::sync::Arc;

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Redirect};
use axum::Form;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use super::AppState;
use crate::agent::room_orchestrator::{self, RoomEvent};
use crate::db::{agents, rooms};
use crate::web::handlers::AppError;

// -- Templates --

#[derive(Template)]
#[template(path = "admin/rooms.html")]
struct RoomsTemplate {
    rooms: Vec<RoomWithOrchestrator>,
}

struct RoomWithOrchestrator {
    room: rooms::Room,
    orchestrator_name: String,
}

#[derive(Template)]
#[template(path = "admin/room_new.html")]
struct RoomNewTemplate {
    agents: Vec<agents::Agent>,
}

#[derive(Template)]
#[template(path = "admin/room.html")]
struct RoomViewTemplate {
    room: rooms::Room,
    participants: Vec<rooms::RoomParticipant>,
    messages: Vec<rooms::RoomMessage>,
    viewer_alias: String,
}

#[derive(Deserialize)]
pub struct RoomViewQuery {
    #[serde(rename = "as")]
    pub as_alias: Option<String>,
}

#[derive(Deserialize)]
pub struct StreamQuery {
    #[serde(rename = "as")]
    pub as_alias: Option<String>,
}

// -- Page handlers --

pub async fn rooms_page(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let room_list = rooms::list_rooms(&state.db)?;
    let rooms: Vec<RoomWithOrchestrator> = room_list
        .into_iter()
        .map(|room| {
            let orchestrator_name = agents::get_agent(&state.db, &room.orchestrator_agent_id)
                .ok()
                .flatten()
                .map(|a| a.display_name)
                .unwrap_or_else(|| "(unknown)".to_string());
            RoomWithOrchestrator { room, orchestrator_name }
        })
        .collect();
    let tmpl = RoomsTemplate { rooms };
    Ok(Html(tmpl.render()?))
}

pub async fn room_new_page(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let agents = agents::list_agents(&state.db)?;
    let tmpl = RoomNewTemplate { agents };
    Ok(Html(tmpl.render()?))
}

pub async fn room_view_page(
    State(state): State<Arc<AppState>>,
    Path(room_id): Path<String>,
    Query(query): Query<RoomViewQuery>,
) -> Result<impl IntoResponse, AppError> {
    let room = rooms::get_room(&state.db, &room_id)?
        .ok_or_else(|| anyhow::anyhow!("Room not found"))?;
    let participants = rooms::get_participants(&state.db, &room_id)?;

    // Determine viewer: explicit ?as= param, or auto-detect sole human participant
    let viewer_alias = query.as_alias.or_else(|| {
        let humans: Vec<_> = participants.iter()
            .filter(|p| p.is_human && p.role != "orchestrator")
            .collect();
        if humans.len() == 1 {
            Some(humans[0].alias.clone())
        } else {
            None
        }
    });

    let messages = match &viewer_alias {
        Some(alias) => rooms::get_visible_messages(&state.db, &room_id, alias, 200)?,
        None => rooms::get_room_messages(&state.db, &room_id, 200)?,
    };

    let tmpl = RoomViewTemplate {
        room,
        participants,
        messages,
        viewer_alias: viewer_alias.unwrap_or_default(),
    };
    Ok(Html(tmpl.render()?))
}

// -- API handlers --

#[derive(Deserialize)]
pub struct CreateRoomForm {
    pub name: String,
    pub description: Option<String>,
    pub orchestrator_agent_id: String,
    pub scenario: Option<String>,
    pub max_turns: Option<i64>,
}

pub async fn create_room(
    State(state): State<Arc<AppState>>,
    Form(form): Form<CreateRoomForm>,
) -> Result<impl IntoResponse, AppError> {
    let orchestrator = agents::get_agent(&state.db, &form.orchestrator_agent_id)?
        .ok_or_else(|| anyhow::anyhow!("Orchestrator agent not found"))?;

    let room = rooms::create_room(
        &state.db,
        &form.name,
        form.description.as_deref().unwrap_or(""),
        &form.orchestrator_agent_id,
        form.scenario.as_deref().unwrap_or(""),
        form.max_turns.unwrap_or(100),
    )?;

    // Auto-add orchestrator as participant
    rooms::add_participant(
        &state.db,
        &room.id,
        Some(&form.orchestrator_agent_id),
        "orchestrator",
        &orchestrator.display_name,
        "",
        false,
    )?;

    Ok(Redirect::to(&format!("/admin/rooms/{}", room.id)))
}

pub async fn api_list_rooms(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let rooms = rooms::list_rooms(&state.db)?;
    Ok(axum::Json(json!(rooms)))
}

pub async fn api_get_room(
    State(state): State<Arc<AppState>>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let room = rooms::get_room(&state.db, &room_id)?
        .ok_or_else(|| anyhow::anyhow!("Room not found"))?;
    let participants = rooms::get_participants(&state.db, &room_id)?;
    Ok(axum::Json(json!({
        "room": room,
        "participants": participants,
    })))
}

#[derive(Deserialize)]
pub struct AddParticipantInput {
    pub agent_id: Option<String>,
    pub role: Option<String>,
    pub alias: String,
    pub private_context: Option<String>,
    pub is_human: Option<bool>,
}

pub async fn api_add_participant(
    State(state): State<Arc<AppState>>,
    Path(room_id): Path<String>,
    axum::Json(input): axum::Json<AddParticipantInput>,
) -> Result<impl IntoResponse, AppError> {
    // Verify room exists
    rooms::get_room(&state.db, &room_id)?
        .ok_or_else(|| anyhow::anyhow!("Room not found"))?;

    let is_human = input.is_human.unwrap_or(false);
    let participant = rooms::add_participant(
        &state.db,
        &room_id,
        input.agent_id.as_deref(),
        input.role.as_deref().unwrap_or("participant"),
        &input.alias,
        input.private_context.as_deref().unwrap_or(""),
        is_human,
    )?;

    Ok(axum::Json(json!(participant)))
}

pub async fn api_start_room(
    State(state): State<Arc<AppState>>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let room = rooms::get_room(&state.db, &room_id)?
        .ok_or_else(|| anyhow::anyhow!("Room not found"))?;

    if room.status == "running" {
        return Ok(axum::Json(json!({"error": "Room is already running"})));
    }

    // Create broadcast channel for SSE
    let (tx, _) = broadcast::channel::<RoomEvent>(256);
    state.room_channels.insert(room_id.clone(), tx.clone());

    let db = state.db.clone();
    let config = state.config.clone();
    let http_client = state.http_client.clone();
    let human_replies = state.room_human_replies.clone();
    let rid = room_id.clone();

    tokio::spawn(async move {
        if let Err(e) = room_orchestrator::run_room_session(
            db, config, http_client, rid.clone(), tx, human_replies,
        ).await {
            tracing::error!(room_id = %rid, error = %e, "Room session error");
        }
    });

    Ok(axum::Json(json!({"status": "started"})))
}

pub async fn api_stop_room(
    State(state): State<Arc<AppState>>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    rooms::update_room_status(&state.db, &room_id, "stopped")?;
    // Emit event if channel exists
    if let Some(tx) = state.room_channels.get(&room_id) {
        let _ = tx.send(RoomEvent::SessionEnded {
            summary: "Session stopped by user.".to_string(),
        });
    }
    Ok(axum::Json(json!({"status": "stopped"})))
}

#[derive(Deserialize)]
pub struct HumanReplyInput {
    pub alias: String,
    pub content: String,
}

pub async fn api_reply(
    State(state): State<Arc<AppState>>,
    Path(room_id): Path<String>,
    axum::Json(input): axum::Json<HumanReplyInput>,
) -> Result<impl IntoResponse, AppError> {
    let key = format!("{}:{}", room_id, input.alias);

    if let Some((_, tx)) = state.room_human_replies.remove(&key) {
        let _ = tx.send(input.content.clone());
        Ok(axum::Json(json!({"status": "replied"})))
    } else {
        Ok(axum::Json(json!({"error": "No pending question for this participant"})))
    }
}

#[derive(Deserialize)]
pub struct InterveneInput {
    pub content: String,
}

pub async fn api_intervene(
    State(state): State<Arc<AppState>>,
    Path(room_id): Path<String>,
    axum::Json(input): axum::Json<InterveneInput>,
) -> Result<impl IntoResponse, AppError> {
    rooms::add_room_message(
        &state.db, &room_id, "system",
        "system", "", &input.content, "system", 0,
    )?;
    Ok(axum::Json(json!({"status": "intervened"})))
}

pub async fn api_messages(
    State(state): State<Arc<AppState>>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let messages = rooms::get_room_messages(&state.db, &room_id, 500)?;
    Ok(axum::Json(json!(messages)))
}

pub async fn api_delete_room(
    State(state): State<Arc<AppState>>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    rooms::delete_room(&state.db, &room_id)?;
    state.room_channels.remove(&room_id);
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn room_stream(
    State(state): State<Arc<AppState>>,
    Path(room_id): Path<String>,
    Query(query): Query<StreamQuery>,
) -> Result<impl IntoResponse, AppError> {
    // Get or create broadcast channel
    let rx = if let Some(tx) = state.room_channels.get(&room_id) {
        tx.subscribe()
    } else {
        let (tx, rx) = broadcast::channel::<RoomEvent>(256);
        state.room_channels.insert(room_id.clone(), tx);
        rx
    };

    let viewer = query.as_alias;

    let stream = BroadcastStream::new(rx).filter_map(move |result| match result {
        Ok(event) => {
            if let Some(ref alias) = viewer {
                let visible = match &event {
                    RoomEvent::MessageSent { visibility, target_alias, sender_alias, .. } => {
                        visibility == "public" || visibility == "system"
                            || target_alias == alias || sender_alias == alias
                    }
                    RoomEvent::AgentResponded { visibility, target_alias, agent_alias, .. } => {
                        visibility == "public"
                            || target_alias == alias || agent_alias == alias
                    }
                    RoomEvent::WaitingForHuman { alias: waiting_alias, .. } => {
                        waiting_alias == alias
                    }
                    _ => true,
                };
                if !visible {
                    return None;
                }
            }
            let data = serde_json::to_string(&event).unwrap_or_default();
            Some(Ok::<_, Infallible>(Event::default().data(data)))
        }
        Err(_) => None,
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
