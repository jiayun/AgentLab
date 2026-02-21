use std::convert::Infallible;
use std::sync::Arc;

use anyhow::Result;
use axum::response::sse::Event;
use futures_util::stream::Stream;
use serde_json::json;

use crate::config::AppConfig;
use crate::db::agents::Agent;
use crate::db::conversations;
use crate::db::skills;
use crate::db::DbPool;
use crate::openapi::{executor, parser};
use crate::provider::openai_compatible::OpenAiCompatibleProvider;
use crate::provider::traits::*;

const MAX_TOOL_ITERATIONS: usize = 5;

/// Build messages from conversation history
fn build_messages(
    system_prompt: &str,
    db_messages: &[conversations::Message],
) -> Vec<ChatMessage> {
    let mut messages = vec![ChatMessage::system(system_prompt)];
    for m in db_messages {
        messages.push(ChatMessage {
            role: m.role.clone(),
            content: Some(m.content.clone()),
            tool_calls: m
                .tool_calls_json
                .as_deref()
                .and_then(|j| serde_json::from_str(j).ok()),
            tool_call_id: m.tool_call_id.clone(),
        });
    }
    messages
}

/// Non-streaming chat with an agent (supports OpenAPI skill tool calls)
pub async fn run_chat(
    db: &DbPool,
    config: &Arc<AppConfig>,
    http_client: &reqwest::Client,
    agent: &Agent,
    conversation_id: &str,
) -> Result<String> {
    let provider = OpenAiCompatibleProvider::new(&config.provider, http_client.clone());
    let system_prompt = super::prompt::build_agent_system_prompt(agent);

    // TODO: model override per-agent not yet supported in provider.chat()
    // The provider uses the global model from config for now.

    // Load skills and tool definitions
    let agent_skills = skills::list_skills(db, &agent.id)?;
    let mut tool_defs: Vec<ToolDefinition> = Vec::new();
    let mut parsed_ops: Vec<parser::ParsedOperation> = Vec::new();

    for skill in &agent_skills {
        if let Ok(ops) = serde_json::from_str::<Vec<parser::ParsedOperation>>(&skill.parsed_tools_json) {
            for op in &ops {
                tool_defs.push(ToolDefinition::new(
                    &op.operation_id,
                    &op.description,
                    op.parameters_schema.clone(),
                ));
            }
            parsed_ops.extend(ops);
        }
    }

    let tools_ref = if tool_defs.is_empty() {
        None
    } else {
        Some(tool_defs.as_slice())
    };

    // Load conversation history
    let db_messages = conversations::get_messages(db, conversation_id)?;
    let mut messages = build_messages(&system_prompt, &db_messages);

    // Tool call loop (only if we have skills)
    if tools_ref.is_some() {
        for _ in 0..MAX_TOOL_ITERATIONS {
            let resp = provider.chat(&messages, tools_ref, agent.temperature).await?;

            if resp.has_tool_calls() {
                let tc_msg = ChatMessage::assistant_with_tool_calls(
                    resp.text.as_deref(),
                    resp.tool_calls.clone(),
                );
                messages.push(tc_msg);

                let tc_json = serde_json::to_string(&resp.tool_calls)?;
                conversations::add_message(
                    db,
                    conversation_id,
                    "assistant",
                    &resp.text_or_empty(),
                    Some(&tc_json),
                    None,
                )?;

                for tc in &resp.tool_calls {
                    let result = execute_skill_tool(
                        http_client,
                        &agent_skills,
                        &parsed_ops,
                        &tc.function.name,
                        &tc.function.arguments,
                    )
                    .await?;

                    messages.push(ChatMessage::tool_result(&tc.id, &result));
                    conversations::add_message(
                        db,
                        conversation_id,
                        "tool",
                        &result,
                        None,
                        Some(&tc.id),
                    )?;
                }
            } else {
                return Ok(resp.text_or_empty());
            }
        }

        Ok("I've completed the requested operations.".to_string())
    } else {
        // No tools, simple chat
        let resp = provider.chat(&messages, None, agent.temperature).await?;
        Ok(resp.text_or_empty())
    }
}

/// Execute a skill tool call via HTTP
async fn execute_skill_tool(
    http_client: &reqwest::Client,
    agent_skills: &[skills::AgentSkill],
    parsed_ops: &[parser::ParsedOperation],
    tool_name: &str,
    arguments: &str,
) -> Result<String> {
    // Find the matching operation
    let op = parsed_ops
        .iter()
        .find(|o| o.operation_id == tool_name)
        .ok_or_else(|| anyhow::anyhow!("Unknown tool: {tool_name}"))?;

    // Find the skill that contains this operation (for base_url and auth)
    let skill = agent_skills.iter().find(|s| {
        serde_json::from_str::<Vec<parser::ParsedOperation>>(&s.parsed_tools_json)
            .map(|ops| ops.iter().any(|o| o.operation_id == tool_name))
            .unwrap_or(false)
    });

    let base_url = skill.map(|s| s.base_url.as_str()).unwrap_or("");
    let auth_header = skill.and_then(|s| s.auth_header.as_deref());
    let auth_value = skill.and_then(|s| s.auth_value.as_deref());

    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or(json!({}));

    executor::execute_operation(http_client, op, base_url, &args, auth_header, auth_value).await
}

/// Streaming chat — returns an SSE event stream
pub fn run_chat_stream(
    db: DbPool,
    config: Arc<AppConfig>,
    http_client: reqwest::Client,
    agent: Agent,
    conversation_id: String,
) -> impl Stream<Item = Result<Event, Infallible>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(100);

    tokio::spawn(async move {
        let provider = OpenAiCompatibleProvider::new(&config.provider, http_client.clone());
        let system_prompt = super::prompt::build_agent_system_prompt(&agent);

        // Check if agent has skills
        let has_skills = skills::list_skills(&db, &agent.id)
            .map(|s| !s.is_empty())
            .unwrap_or(false);

        if has_skills {
            // Use non-streaming path for tool calling, then stream the result
            match run_chat(&db, &config, &http_client, &agent, &conversation_id).await {
                Ok(response) => {
                    for chunk in response.chars().collect::<Vec<_>>().chunks(3) {
                        let text: String = chunk.iter().collect();
                        let _ = tx
                            .send(Ok(
                                Event::default().data(json!({"delta": text}).to_string())
                            ))
                            .await;
                        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                    }
                    let _ = conversations::add_message(
                        &db,
                        &conversation_id,
                        "assistant",
                        &response,
                        None,
                        None,
                    );
                }
                Err(e) => {
                    let _ = tx
                        .send(Ok(
                            Event::default().data(json!({"error": e.to_string()}).to_string())
                        ))
                        .await;
                }
            }
        } else {
            // Pure streaming — no tools
            let db_messages = match conversations::get_messages(&db, &conversation_id) {
                Ok(m) => m,
                Err(e) => {
                    let _ = tx
                        .send(Ok(
                            Event::default().data(json!({"error": e.to_string()}).to_string())
                        ))
                        .await;
                    let _ = tx
                        .send(Ok(
                            Event::default().data(json!({"done": true}).to_string())
                        ))
                        .await;
                    return;
                }
            };

            let messages = build_messages(&system_prompt, &db_messages);

            match provider
                .stream_chat(&messages, agent.temperature, None)
                .await
            {
                Ok(mut stream_rx) => {
                    let mut full_response = String::new();

                    while let Some(chunk) = stream_rx.recv().await {
                        if chunk.is_final {
                            break;
                        }
                        full_response.push_str(&chunk.delta);
                        let _ = tx
                            .send(Ok(
                                Event::default()
                                    .data(json!({"delta": chunk.delta}).to_string()),
                            ))
                            .await;
                    }

                    // Save full response
                    let _ = conversations::add_message(
                        &db,
                        &conversation_id,
                        "assistant",
                        &full_response,
                        None,
                        None,
                    );
                }
                Err(e) => {
                    let _ = tx
                        .send(Ok(
                            Event::default().data(json!({"error": e.to_string()}).to_string())
                        ))
                        .await;
                }
            }
        }

        let _ = tx
            .send(Ok(Event::default().data(json!({"done": true}).to_string())))
            .await;
    });

    tokio_stream::wrappers::ReceiverStream::new(rx)
}
