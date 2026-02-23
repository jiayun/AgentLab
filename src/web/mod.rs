pub mod handlers;
pub mod sse;

use std::sync::Arc;

use axum::http::StatusCode;
use axum::Router;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use crate::config::AppConfig;
use crate::db::DbPool;

pub struct AppState {
    pub db: DbPool,
    pub config: Arc<AppConfig>,
    pub http_client: reqwest::Client,
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
