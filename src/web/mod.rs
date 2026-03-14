pub mod handlers;
pub mod room_handlers;
pub mod sse;

use std::sync::Arc;

use axum::http::StatusCode;
use axum::Router;
use tokio::sync::{broadcast, oneshot};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use crate::agent::room_orchestrator::RoomEvent;
use crate::config::AppConfig;
use crate::db::DbPool;

pub struct AppState {
    pub db: DbPool,
    pub config: Arc<AppConfig>,
    pub http_client: reqwest::Client,
    pub room_channels: dashmap::DashMap<String, broadcast::Sender<RoomEvent>>,
    pub room_human_replies: Arc<dashmap::DashMap<String, oneshot::Sender<String>>>,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    let api_routes = Router::new()
        .route(
            "/agents/{id}/configure",
            axum::routing::post(handlers::api_configure),
        )
        .route(
            "/agents/{id}/configure/stream",
            axum::routing::get(sse::configure_stream),
        )
        .route(
            "/agents/{id}/chat",
            axum::routing::post(handlers::api_chat),
        )
        .route(
            "/agents/{id}/chat/stream",
            axum::routing::get(sse::chat_stream),
        )
        .route(
            "/agents/{id}/config",
            axum::routing::get(handlers::api_get_config),
        )
        .route(
            "/agents/{id}/skills",
            axum::routing::get(handlers::api_list_skills)
                .post(handlers::api_upload_skill),
        )
        .route(
            "/agents/{id}/skills/{skill_id}",
            axum::routing::delete(handlers::api_delete_skill),
        )
        .route(
            "/agents/{id}",
            axum::routing::delete(handlers::api_delete_agent),
        )
        .route(
            "/agents",
            axum::routing::get(handlers::api_list_agents),
        )
        // Room API routes
        .route(
            "/rooms",
            axum::routing::get(room_handlers::api_list_rooms),
        )
        .route(
            "/rooms/{room_id}",
            axum::routing::get(room_handlers::api_get_room)
                .delete(room_handlers::api_delete_room),
        )
        .route(
            "/rooms/{room_id}/participants",
            axum::routing::post(room_handlers::api_add_participant),
        )
        .route(
            "/rooms/{room_id}/start",
            axum::routing::post(room_handlers::api_start_room),
        )
        .route(
            "/rooms/{room_id}/stop",
            axum::routing::post(room_handlers::api_stop_room),
        )
        .route(
            "/rooms/{room_id}/reply",
            axum::routing::post(room_handlers::api_reply),
        )
        .route(
            "/rooms/{room_id}/intervene",
            axum::routing::post(room_handlers::api_intervene),
        )
        .route(
            "/rooms/{room_id}/messages",
            axum::routing::get(room_handlers::api_messages),
        )
        .route(
            "/rooms/{room_id}/stream",
            axum::routing::get(room_handlers::room_stream),
        );

    let admin_routes = Router::new()
        .route("/", axum::routing::get(handlers::index))
        .route("/agents/new", axum::routing::get(handlers::agent_create_page))
        .route(
            "/agents",
            axum::routing::post(handlers::agent_create),
        )
        .route(
            "/agents/{id}",
            axum::routing::get(handlers::agent_edit_page),
        )
        .route(
            "/agents/{id}/chat",
            axum::routing::get(handlers::agent_chat_page),
        )
        // Room page routes
        .route("/rooms", axum::routing::get(room_handlers::rooms_page))
        .route("/rooms/new", axum::routing::get(room_handlers::room_new_page))
        .route(
            "/rooms/create",
            axum::routing::post(room_handlers::create_room),
        )
        .route(
            "/rooms/{room_id}",
            axum::routing::get(room_handlers::room_view_page),
        )
        .nest("/api", api_routes);

    Router::new()
        .nest("/admin", admin_routes)
        .route("/health", axum::routing::get(handlers::health))
        .nest_service("/static", ServeDir::new("static"))
        .fallback(fallback)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn fallback(uri: axum::http::Uri) -> (StatusCode, String) {
    tracing::warn!("No route matched: {}", uri);
    (StatusCode::NOT_FOUND, format!("Not found: {uri}"))
}
