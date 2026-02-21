use std::sync::Arc;

use anyhow::Result;
use axum::extract::Request;
use axum::ServiceExt;
use tower::Layer;
use tower_http::normalize_path::NormalizePathLayer;
use tracing_subscriber::EnvFilter;

use agentlab::config::AppConfig;
use agentlab::db;
use agentlab::web::{self, AppState};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // Load configuration
    let config = AppConfig::load()?;
    let port = config.port();

    tracing::info!(
        "Provider: {} (model: {})",
        config.provider.api_url,
        config.provider.model
    );

    // Initialize database
    let db = db::init_db()?;

    // Build app state
    let state = Arc::new(AppState {
        db,
        config: Arc::new(config),
        http_client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()?,
    });

    // Create router with NormalizePathLayer to trim trailing slashes
    let app = NormalizePathLayer::trim_trailing_slash().layer(web::create_router(state));

    // Start server
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    tracing::info!("AgentLab running on http://localhost:{port}");
    tracing::info!("Admin panel: http://localhost:{port}/admin/");

    axum::serve(listener, ServiceExt::<Request>::into_make_service(app)).await?;

    Ok(())
}
