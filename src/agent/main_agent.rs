use std::convert::Infallible;
use std::sync::Arc;

use anyhow::Result;
use axum::response::sse::Event;
use futures_util::stream::Stream;
use serde_json::json;

use crate::config::AppConfig;
use crate::db::agents::{self, Agent};
use crate::db::conversations;
use crate::db::skills;
use crate::db::DbPool;
use crate::provider::openai_compatible::OpenAiCompatibleProvider;
use crate::provider::traits::*;

const MAX_TOOL_ITERATIONS: usize = 5;

fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition::new(
            "get_agent_config",
            "Read the target agent's complete configuration",
            json!({"type": "object", "properties": {}, "required": []}),
        ),
        ToolDefinition::new(
            "update_agent_soul",
            "Update the agent's core identity and behavior rules (who the agent is, values, principles)",
            json!({
                "type": "object",
                "properties": {
                    "soul": {"type": "string", "description": "Core identity description"}
                },
                "required": ["soul"]
            }),
        ),
        ToolDefinition::new(
            "update_agent_personality",
            "Update the agent's personality traits (MBTI type, tone, attitude, emotional style)",
            json!({
                "type": "object",
                "properties": {
                    "personality": {"type": "string", "description": "Personality description"}
                },
                "required": ["personality"]
            }),
        ),
        ToolDefinition::new(
            "update_agent_communication_style",
            "Update how the agent communicates (formality level, language habits, response length)",
            json!({
                "type": "object",
                "properties": {
                    "style": {"type": "string", "description": "Communication style description"}
                },
                "required": ["style"]
            }),
        ),
        ToolDefinition::new(
            "update_agent_instructions",
            "Update specific task rules, constraints, and guidelines for the agent",
            json!({
                "type": "object",
                "properties": {
                    "instructions": {"type": "string", "description": "Instructions text"}
                },
                "required": ["instructions"]
            }),
        ),
        ToolDefinition::new(
            "update_agent_system_prompt",
            "Set a complete system prompt override. When non-empty, this replaces all structured identity fields.",
            json!({
                "type": "object",
                "properties": {
                    "system_prompt": {"type": "string", "description": "Full system prompt"}
                },
                "required": ["system_prompt"]
            }),
        ),
        ToolDefinition::new(
            "update_agent_model",
            "Update the model used by this agent (empty string = use default)",
            json!({
                "type": "object",
                "properties": {
                    "model": {"type": "string", "description": "Model name"}
                },
                "required": ["model"]
            }),
        ),
        ToolDefinition::new(
            "update_agent_temperature",
            "Update the temperature (creativity) setting. 0.0 = deterministic, 1.0 = creative",
            json!({
                "type": "object",
                "properties": {
                    "temperature": {"type": "number", "description": "Temperature value 0.0-2.0"}
                },
                "required": ["temperature"]
            }),
        ),
        ToolDefinition::new(
            "list_agent_skills",
            "List all OpenAPI skills configured for this agent",
            json!({"type": "object", "properties": {}, "required": []}),
        ),
    ]
}

fn execute_tool(
    db: &DbPool,
    agent: &Agent,
    tool_name: &str,
    arguments: &str,
) -> Result<String> {
    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or(json!({}));

    match tool_name {
        "get_agent_config" => {
            let current = agents::get_agent(db, &agent.id)?
                .ok_or_else(|| anyhow::anyhow!("Agent not found"))?;
            Ok(serde_json::to_string_pretty(&json!({
                "name": current.name,
                "display_name": current.display_name,
                "soul": current.soul,
                "personality": current.personality,
                "communication_style": current.communication_style,
                "instructions": current.instructions,
                "system_prompt": current.system_prompt,
                "model": current.model,
                "temperature": current.temperature,
            }))?)
        }
        "update_agent_soul" => {
            let val = args["soul"].as_str().unwrap_or("");
            agents::update_agent_field(db, &agent.id, "soul", val)?;
            Ok(format!("Updated soul successfully."))
        }
        "update_agent_personality" => {
            let val = args["personality"].as_str().unwrap_or("");
            agents::update_agent_field(db, &agent.id, "personality", val)?;
            Ok(format!("Updated personality successfully."))
        }
        "update_agent_communication_style" => {
            let val = args["style"].as_str().unwrap_or("");
            agents::update_agent_field(db, &agent.id, "communication_style", val)?;
            Ok(format!("Updated communication style successfully."))
        }
        "update_agent_instructions" => {
            let val = args["instructions"].as_str().unwrap_or("");
            agents::update_agent_field(db, &agent.id, "instructions", val)?;
            Ok(format!("Updated instructions successfully."))
        }
        "update_agent_system_prompt" => {
            let val = args["system_prompt"].as_str().unwrap_or("");
            agents::update_agent_field(db, &agent.id, "system_prompt", val)?;
            Ok(format!("Updated system prompt successfully."))
        }
        "update_agent_model" => {
            let val = args["model"].as_str().unwrap_or("");
            agents::update_agent_field(db, &agent.id, "model", val)?;
            Ok(format!("Updated model to '{val}'."))
        }
        "update_agent_temperature" => {
            let val = args["temperature"].as_f64().unwrap_or(0.7);
            agents::update_agent_temperature(db, &agent.id, val)?;
            Ok(format!("Updated temperature to {val}."))
        }
        "list_agent_skills" => {
            let agent_skills = skills::list_skills(db, &agent.id)?;
            if agent_skills.is_empty() {
                Ok("No skills configured.".to_string())
            } else {
                let list: Vec<_> = agent_skills
                    .iter()
                    .map(|s| json!({"name": s.name, "description": s.description}))
                    .collect();
                Ok(serde_json::to_string_pretty(&list)?)
            }
        }
        _ => Ok(format!("Unknown tool: {tool_name}")),
    }
}

/// Non-streaming configure — runs the tool loop and returns final text
pub async fn run_configure(
    db: &DbPool,
    config: &Arc<AppConfig>,
    http_client: &reqwest::Client,
    agent: &Agent,
    conversation_id: &str,
) -> Result<String> {
    let provider = OpenAiCompatibleProvider::new(&config.provider, http_client.clone());
    let system_prompt = super::prompt::build_main_agent_system_prompt(agent);
    let tools = tool_definitions();

    // Load conversation history
    let db_messages = conversations::get_messages(db, conversation_id)?;
    let mut messages = vec![ChatMessage::system(&system_prompt)];
    for m in &db_messages {
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

    // Tool call loop
    for _ in 0..MAX_TOOL_ITERATIONS {
        let resp = provider.chat(&messages, Some(&tools), 0.7).await?;

        if resp.has_tool_calls() {
            // Add assistant message with tool calls
            let tc_msg = ChatMessage::assistant_with_tool_calls(
                resp.text.as_deref(),
                resp.tool_calls.clone(),
            );
            messages.push(tc_msg.clone());

            // Save to DB
            let tc_json = serde_json::to_string(&resp.tool_calls)?;
            conversations::add_message(
                db,
                conversation_id,
                "assistant",
                &resp.text_or_empty(),
                Some(&tc_json),
                None,
            )?;

            // Execute each tool call
            for tc in &resp.tool_calls {
                let result = execute_tool(db, agent, &tc.function.name, &tc.function.arguments)?;
                let tool_msg = ChatMessage::tool_result(&tc.id, &result);
                messages.push(tool_msg);

                // Save tool result to DB
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

    Ok("I've made the requested changes. Please check the configuration panel to verify.".to_string())
}

/// Streaming configure — returns an SSE event stream
pub fn run_configure_stream(
    db: DbPool,
    config: Arc<AppConfig>,
    http_client: reqwest::Client,
    agent: Agent,
    conversation_id: String,
) -> impl Stream<Item = Result<Event, Infallible>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(100);

    tokio::spawn(async move {
        // Run the non-streaming version and stream the result
        match run_configure(&db, &config, &http_client, &agent, &conversation_id).await {
            Ok(response) => {
                // Send response in chunks for a streaming feel
                for chunk in response.chars().collect::<Vec<_>>().chunks(3) {
                    let text: String = chunk.iter().collect();
                    let _ = tx
                        .send(Ok(Event::default().data(json!({"delta": text}).to_string())))
                        .await;
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                }
                // Save assistant response
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

        let _ = tx
            .send(Ok(Event::default().data(json!({"done": true}).to_string())))
            .await;
    });

    tokio_stream::wrappers::ReceiverStream::new(rx)
}
