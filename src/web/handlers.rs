use std::sync::Arc;

use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Form;
use serde::Deserialize;

use crate::db::agents::{self, Agent};
use crate::db::skills;

use super::AppState;

// -- Error handling --

pub struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!("Error: {:?}", self.0);
        (StatusCode::INTERNAL_SERVER_ERROR, self.0.to_string()).into_response()
    }
}

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

// -- Templates --

#[derive(Template)]
#[template(path = "admin/index.html")]
struct IndexTemplate {
    agents: Vec<Agent>,
}

#[derive(Template)]
#[template(path = "admin/agent_create.html")]
struct AgentCreateTemplate;

#[derive(Template)]
#[template(path = "admin/agent_edit.html")]
struct AgentEditTemplate {
    agent: Agent,
}

#[derive(Template)]
#[template(path = "admin/agent_chat.html")]
struct AgentChatTemplate {
    agent: Agent,
}

// -- Page handlers --

pub async fn index(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let agents = agents::list_agents(&state.db)?;
    let tmpl = IndexTemplate { agents };
    Ok(Html(tmpl.render()?))
}

pub async fn agent_create_page() -> Result<impl IntoResponse, AppError> {
    let tmpl = AgentCreateTemplate;
    Ok(Html(tmpl.render()?))
}

#[derive(Deserialize)]
pub struct CreateAgentForm {
    pub name: String,
    pub display_name: String,
}

pub async fn agent_create(
    State(state): State<Arc<AppState>>,
    Form(form): Form<CreateAgentForm>,
) -> Result<impl IntoResponse, AppError> {
    let _agent = agents::create_agent(
        &state.db,
        &agents::CreateAgent {
            name: form.name,
            display_name: form.display_name,
        },
    )?;
    Ok(Redirect::to("/admin/"))
}

pub async fn agent_edit_page(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let agent = agents::get_agent(&state.db, &id)?
        .ok_or_else(|| anyhow::anyhow!("Agent not found"))?;
    let tmpl = AgentEditTemplate { agent };
    Ok(Html(tmpl.render()?))
}

pub async fn agent_chat_page(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let agent = agents::get_agent(&state.db, &id)?
        .ok_or_else(|| anyhow::anyhow!("Agent not found"))?;
    let tmpl = AgentChatTemplate { agent };
    Ok(Html(tmpl.render()?))
}

// -- API handlers --

pub async fn health() -> impl IntoResponse {
    axum::Json(serde_json::json!({"status": "ok"}))
}

#[derive(Deserialize)]
pub struct ChatInput {
    pub message: String,
    pub session_id: Option<String>,
}

pub async fn api_configure(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    axum::Json(input): axum::Json<ChatInput>,
) -> Result<impl IntoResponse, AppError> {
    use crate::agent::main_agent;
    use crate::db::conversations;

    let agent = agents::get_agent(&state.db, &id)?
        .ok_or_else(|| anyhow::anyhow!("Agent not found"))?;

    let session_id = input.session_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let conv = conversations::get_or_create_conversation(&state.db, &id, &session_id)?;

    // Save user message
    conversations::add_message(&state.db, &conv.id, "user", &input.message, None, None)?;

    // Run main agent
    let response = main_agent::run_configure(
        &state.db,
        &state.config,
        &state.http_client,
        &agent,
        &conv.id,
    )
    .await?;

    // Save assistant response
    conversations::add_message(&state.db, &conv.id, "assistant", &response, None, None)?;

    Ok(axum::Json(serde_json::json!({
        "response": response,
        "session_id": session_id,
        "conversation_id": conv.id,
    })))
}

pub async fn api_chat(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    axum::Json(input): axum::Json<ChatInput>,
) -> Result<impl IntoResponse, AppError> {
    use crate::agent::chat_agent;
    use crate::db::conversations;

    let agent = agents::get_agent(&state.db, &id)?
        .ok_or_else(|| anyhow::anyhow!("Agent not found"))?;

    let session_id = input.session_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let conv = conversations::get_or_create_conversation(&state.db, &id, &session_id)?;

    // Save user message
    conversations::add_message(&state.db, &conv.id, "user", &input.message, None, None)?;

    // Run chat agent
    let response = chat_agent::run_chat(
        &state.db,
        &state.config,
        &state.http_client,
        &agent,
        &conv.id,
    )
    .await?;

    // Save assistant response
    conversations::add_message(&state.db, &conv.id, "assistant", &response, None, None)?;

    Ok(axum::Json(serde_json::json!({
        "response": response,
        "session_id": session_id,
        "conversation_id": conv.id,
    })))
}

pub async fn api_get_config(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let agent = agents::get_agent(&state.db, &id)?
        .ok_or_else(|| anyhow::anyhow!("Agent not found"))?;
    Ok(axum::Json(agent))
}

#[derive(Deserialize)]
pub struct UploadSkillInput {
    pub name: String,
    pub description: Option<String>,
    pub openapi_spec: String,
    pub base_url: Option<String>,
    pub auth_header: Option<String>,
    pub auth_value: Option<String>,
}

pub async fn api_upload_skill(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    axum::Json(input): axum::Json<UploadSkillInput>,
) -> Result<impl IntoResponse, AppError> {
    use crate::openapi::parser;

    // Verify agent exists
    agents::get_agent(&state.db, &id)?
        .ok_or_else(|| anyhow::anyhow!("Agent not found"))?;

    // Parse OpenAPI spec
    let parsed_tools = parser::parse_openapi_spec(&input.openapi_spec)?;
    let parsed_tools_json = serde_json::to_string(&parsed_tools)?;

    let skill = skills::create_skill(
        &state.db,
        &id,
        &input.name,
        input.description.as_deref().unwrap_or(""),
        &input.openapi_spec,
        &parsed_tools_json,
        input.base_url.as_deref().unwrap_or(""),
        input.auth_header.as_deref(),
        input.auth_value.as_deref(),
    )?;

    Ok(axum::Json(skill))
}

pub async fn api_delete_skill(
    State(state): State<Arc<AppState>>,
    Path((id, skill_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    skills::delete_skill(&state.db, &id, &skill_id)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn api_delete_agent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    agents::delete_agent(&state.db, &id)?;
    Ok(StatusCode::NO_CONTENT)
}
