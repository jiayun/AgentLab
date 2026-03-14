use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::json;
use tokio::sync::broadcast;

use crate::config::AppConfig;
use crate::db::agents;
use crate::db::rooms::{self, RoomParticipant};
use crate::db::DbPool;
use crate::provider::openai_compatible::OpenAiCompatibleProvider;
use crate::provider::traits::*;

use super::prompt;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type")]
pub enum RoomEvent {
    MessageSent {
        sender_alias: String,
        content: String,
        visibility: String,
        target_alias: String,
    },
    AgentResponded {
        agent_alias: String,
        content: String,
        visibility: String,
        target_alias: String,
    },
    WaitingForHuman {
        alias: String,
        question: String,
    },
    TurnAdvanced {
        turn_number: i64,
    },
    SessionEnded {
        summary: String,
    },
    Error {
        message: String,
    },
}

fn orchestrator_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition::new(
            "broadcast_message",
            "Send a public message to all participants in the room. Use for narration, announcements, or public dialogue.",
            json!({
                "type": "object",
                "properties": {
                    "message": {"type": "string", "description": "The message to broadcast"}
                },
                "required": ["message"]
            }),
        ),
        ToolDefinition::new(
            "send_private_message",
            "Send a private message to a specific participant. Only you and the target can see it.",
            json!({
                "type": "object",
                "properties": {
                    "alias": {"type": "string", "description": "The alias of the target participant"},
                    "message": {"type": "string", "description": "The private message"}
                },
                "required": ["alias", "message"]
            }),
        ),
        ToolDefinition::new(
            "ask_agent",
            "Ask a specific participant a question and wait for their reply. This is the core interaction tool.",
            json!({
                "type": "object",
                "properties": {
                    "alias": {"type": "string", "description": "The alias of the participant to ask"},
                    "message": {"type": "string", "description": "The question or prompt to send"},
                    "private": {"type": "boolean", "description": "If true, the exchange is only visible to you and the target"}
                },
                "required": ["alias", "message"]
            }),
        ),
        ToolDefinition::new(
            "ask_all_agents",
            "Ask all participants the same question and collect their replies.",
            json!({
                "type": "object",
                "properties": {
                    "message": {"type": "string", "description": "The question to ask everyone"}
                },
                "required": ["message"]
            }),
        ),
        ToolDefinition::new(
            "get_room_history",
            "Get recent public messages from the room.",
            json!({
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "description": "Number of recent messages to retrieve (default 20)"}
                },
                "required": []
            }),
        ),
        ToolDefinition::new(
            "advance_turn",
            "Advance the turn counter. Call this to mark the progression of rounds/phases.",
            json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        ),
        ToolDefinition::new(
            "end_session",
            "End the room session with a summary.",
            json!({
                "type": "object",
                "properties": {
                    "summary": {"type": "string", "description": "A summary of the session outcome"}
                },
                "required": ["summary"]
            }),
        ),
    ]
}

struct RoomContext {
    db: DbPool,
    config: Arc<AppConfig>,
    http_client: reqwest::Client,
    room_id: String,
    orchestrator_alias: String,
    participants: Vec<RoomParticipant>,
    turn_number: i64,
    tx: broadcast::Sender<RoomEvent>,
    human_replies: Arc<dashmap::DashMap<String, tokio::sync::oneshot::Sender<String>>>,
}

impl RoomContext {
    fn find_participant(&self, alias: &str) -> Option<&RoomParticipant> {
        self.participants.iter().find(|p| p.alias == alias && p.role != "orchestrator")
    }

    fn emit(&self, event: RoomEvent) {
        let _ = self.tx.send(event);
    }
}

async fn execute_room_tool(
    ctx: &mut RoomContext,
    tool_name: &str,
    arguments: &str,
) -> Result<String> {
    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or(json!({}));

    match tool_name {
        "broadcast_message" => {
            let message = args["message"].as_str().unwrap_or("");
            rooms::add_room_message(
                &ctx.db, &ctx.room_id, &ctx.orchestrator_alias,
                "public", "", message, "narration", ctx.turn_number,
            )?;
            ctx.emit(RoomEvent::MessageSent {
                sender_alias: ctx.orchestrator_alias.clone(),
                content: message.to_string(),
                visibility: "public".to_string(),
                target_alias: String::new(),
            });
            Ok("Message broadcast to all participants.".to_string())
        }

        "send_private_message" => {
            let alias = args["alias"].as_str().unwrap_or("");
            let message = args["message"].as_str().unwrap_or("");

            if ctx.find_participant(alias).is_none() {
                return Ok(format!("Participant '{alias}' not found."));
            }

            rooms::add_room_message(
                &ctx.db, &ctx.room_id, &ctx.orchestrator_alias,
                "private", alias, message, "chat", ctx.turn_number,
            )?;
            ctx.emit(RoomEvent::MessageSent {
                sender_alias: ctx.orchestrator_alias.clone(),
                content: message.to_string(),
                visibility: "private".to_string(),
                target_alias: alias.to_string(),
            });
            Ok(format!("Private message sent to {alias}."))
        }

        "ask_agent" => {
            let alias = args["alias"].as_str().unwrap_or("");
            let message = args["message"].as_str().unwrap_or("");
            let is_private = args["private"].as_bool().unwrap_or(false);
            let visibility = if is_private { "private" } else { "public" };

            let participant = match ctx.find_participant(alias) {
                Some(p) => p.clone(),
                None => return Ok(format!("Participant '{alias}' not found.")),
            };

            // Save the orchestrator's question
            rooms::add_room_message(
                &ctx.db, &ctx.room_id, &ctx.orchestrator_alias,
                visibility, alias, message, "chat", ctx.turn_number,
            )?;
            ctx.emit(RoomEvent::MessageSent {
                sender_alias: ctx.orchestrator_alias.clone(),
                content: message.to_string(),
                visibility: visibility.to_string(),
                target_alias: alias.to_string(),
            });

            let reply = if participant.is_human {
                ask_human(ctx, &participant, message).await?
            } else {
                ask_ai(ctx, &participant, message).await?
            };

            // Save reply
            rooms::add_room_message(
                &ctx.db, &ctx.room_id, alias,
                visibility, &ctx.orchestrator_alias, &reply, "chat", ctx.turn_number,
            )?;
            ctx.emit(RoomEvent::AgentResponded {
                agent_alias: alias.to_string(),
                content: reply.clone(),
                visibility: visibility.to_string(),
                target_alias: ctx.orchestrator_alias.clone(),
            });

            Ok(format!("{alias}: {reply}"))
        }

        "ask_all_agents" => {
            let message = args["message"].as_str().unwrap_or("");
            let non_orchestrator: Vec<_> = ctx.participants.iter()
                .filter(|p| p.role != "orchestrator")
                .cloned()
                .collect();

            // Save broadcast question
            rooms::add_room_message(
                &ctx.db, &ctx.room_id, &ctx.orchestrator_alias,
                "public", "", message, "chat", ctx.turn_number,
            )?;
            ctx.emit(RoomEvent::MessageSent {
                sender_alias: ctx.orchestrator_alias.clone(),
                content: message.to_string(),
                visibility: "public".to_string(),
                target_alias: String::new(),
            });

            let mut replies = Vec::new();
            for participant in &non_orchestrator {
                let reply = if participant.is_human {
                    ask_human(ctx, participant, message).await?
                } else {
                    ask_ai(ctx, participant, message).await?
                };

                rooms::add_room_message(
                    &ctx.db, &ctx.room_id, &participant.alias,
                    "public", &ctx.orchestrator_alias, &reply, "chat", ctx.turn_number,
                )?;
                ctx.emit(RoomEvent::AgentResponded {
                    agent_alias: participant.alias.clone(),
                    content: reply.clone(),
                    visibility: "public".to_string(),
                    target_alias: ctx.orchestrator_alias.clone(),
                });

                replies.push(format!("{}: {}", participant.alias, reply));
            }

            Ok(replies.join("\n\n"))
        }

        "get_room_history" => {
            let limit = args["limit"].as_i64().unwrap_or(20);
            let messages = rooms::get_room_messages(&ctx.db, &ctx.room_id, limit)?;
            let history: Vec<String> = messages
                .iter()
                .map(|m| {
                    let vis = if m.visibility == "private" {
                        format!(" [private to {}]", m.target_alias)
                    } else {
                        String::new()
                    };
                    format!("[Turn {}] {}{}: {}", m.turn_number, m.sender_alias, vis, m.content)
                })
                .collect();
            Ok(if history.is_empty() {
                "No messages yet.".to_string()
            } else {
                history.join("\n")
            })
        }

        "advance_turn" => {
            ctx.turn_number += 1;
            ctx.emit(RoomEvent::TurnAdvanced { turn_number: ctx.turn_number });
            Ok(format!("Turn advanced to {}.", ctx.turn_number))
        }

        "end_session" => {
            let summary = args["summary"].as_str().unwrap_or("Session ended.");
            rooms::add_room_message(
                &ctx.db, &ctx.room_id, "system",
                "public", "", summary, "system", ctx.turn_number,
            )?;
            rooms::update_room_status(&ctx.db, &ctx.room_id, "finished")?;
            ctx.emit(RoomEvent::SessionEnded { summary: summary.to_string() });
            Ok(format!("Session ended: {summary}"))
        }

        _ => Ok(format!("Unknown tool: {tool_name}")),
    }
}

async fn ask_ai(ctx: &RoomContext, participant: &RoomParticipant, question: &str) -> Result<String> {
    let agent_id = participant.agent_id.as_deref()
        .context("AI participant has no agent_id")?;
    let agent = agents::get_agent(&ctx.db, agent_id)?
        .context("Participant agent not found")?;

    let room = rooms::get_room(&ctx.db, &ctx.room_id)?
        .context("Room not found")?;

    // Build participant's view of the conversation
    let visible_messages = rooms::get_visible_messages(&ctx.db, &ctx.room_id, &participant.alias, 50)?;
    let system_prompt = prompt::build_room_participant_prompt(&agent, &room, participant, &visible_messages);

    let messages = vec![
        ChatMessage::system(&system_prompt),
        ChatMessage::user(question),
    ];

    let provider = OpenAiCompatibleProvider::new(&ctx.config.provider, ctx.http_client.clone());
    let model_override = if agent.model.is_empty() { None } else { Some(agent.model.as_str()) };

    // Use provider with agent's model if set, otherwise default
    let temperature = agent.temperature;
    let resp = if let Some(model) = model_override {
        // Create a temporary config with model override
        let mut provider_config = ctx.config.provider.clone();
        provider_config.model = model.to_string();
        let provider = OpenAiCompatibleProvider::new(&provider_config, ctx.http_client.clone());
        provider.chat(&messages, None, temperature).await?
    } else {
        provider.chat(&messages, None, temperature).await?
    };

    Ok(resp.text_or_empty())
}

async fn ask_human(ctx: &RoomContext, participant: &RoomParticipant, question: &str) -> Result<String> {
    let key = format!("{}:{}", ctx.room_id, participant.alias);

    ctx.emit(RoomEvent::WaitingForHuman {
        alias: participant.alias.clone(),
        question: question.to_string(),
    });

    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    ctx.human_replies.insert(key.clone(), tx);

    // Wait with timeout (5 minutes)
    match tokio::time::timeout(Duration::from_secs(300), rx).await {
        Ok(Ok(reply)) => Ok(reply),
        Ok(Err(_)) => {
            ctx.human_replies.remove(&key);
            Ok("[Player did not respond]".to_string())
        }
        Err(_) => {
            ctx.human_replies.remove(&key);
            Ok("[Player timed out]".to_string())
        }
    }
}

pub async fn run_room_session(
    db: DbPool,
    config: Arc<AppConfig>,
    http_client: reqwest::Client,
    room_id: String,
    tx: broadcast::Sender<RoomEvent>,
    human_replies: Arc<dashmap::DashMap<String, tokio::sync::oneshot::Sender<String>>>,
) -> Result<()> {
    let room = rooms::get_room(&db, &room_id)?
        .context("Room not found")?;
    let participants = rooms::get_participants(&db, &room_id)?;

    let orchestrator_participant = participants.iter()
        .find(|p| p.role == "orchestrator")
        .context("No orchestrator participant in room")?;
    let orchestrator_alias = orchestrator_participant.alias.clone();

    let orchestrator_agent = agents::get_agent(&db, &room.orchestrator_agent_id)?
        .context("Orchestrator agent not found")?;

    rooms::update_room_status(&db, &room_id, "running")?;

    let mut ctx = RoomContext {
        db: db.clone(),
        config: config.clone(),
        http_client: http_client.clone(),
        room_id: room_id.clone(),
        orchestrator_alias: orchestrator_alias.clone(),
        participants: participants.clone(),
        turn_number: 0,
        tx: tx.clone(),
        human_replies,
    };

    let system_prompt = prompt::build_room_orchestrator_prompt(&orchestrator_agent, &room, &participants);
    let tools = orchestrator_tools();
    let provider = OpenAiCompatibleProvider::new(&config.provider, http_client.clone());

    let mut messages = vec![
        ChatMessage::system(&system_prompt),
        ChatMessage::user("The session has started. Begin."),
    ];

    let max_iterations = room.max_turns as usize;

    for iteration in 0..max_iterations {
        tracing::info!(room_id = %room_id, iteration, "Room orchestrator iteration");

        let resp = match provider.chat(&messages, Some(&tools), orchestrator_agent.temperature).await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(error = %e, "Room orchestrator LLM error");
                ctx.emit(RoomEvent::Error { message: e.to_string() });
                break;
            }
        };

        if resp.has_tool_calls() {
            let tc_msg = ChatMessage::assistant_with_tool_calls(
                resp.text.as_deref(),
                resp.tool_calls.clone(),
            );
            messages.push(tc_msg);

            let mut should_end = false;
            for tc in &resp.tool_calls {
                tracing::info!(tool = %tc.function.name, room_id = %room_id, "Executing room tool");
                let result = match execute_room_tool(&mut ctx, &tc.function.name, &tc.function.arguments).await {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::error!(error = ?e, tool = %tc.function.name, args = %tc.function.arguments, "Room tool error");
                        format!("Error: {e}")
                    }
                };

                if tc.function.name == "end_session" {
                    should_end = true;
                }

                messages.push(ChatMessage::tool_result(&tc.id, &result));
            }

            if should_end {
                break;
            }
        } else {
            // LLM responded with text only — treat as narration broadcast
            let text = resp.text_or_empty();
            if !text.is_empty() {
                rooms::add_room_message(
                    &ctx.db, &ctx.room_id, &orchestrator_alias,
                    "public", "", &text, "narration", ctx.turn_number,
                )?;
                ctx.emit(RoomEvent::MessageSent {
                    sender_alias: orchestrator_alias.clone(),
                    content: text.clone(),
                    visibility: "public".to_string(),
                    target_alias: String::new(),
                });
            }
            messages.push(ChatMessage::assistant(&text));
            // After a pure text response, prompt the orchestrator to continue
            messages.push(ChatMessage::user("Continue the session. Use your tools to interact with participants."));
        }

        // Check if room status changed externally (e.g., stopped by user)
        if let Ok(Some(current_room)) = rooms::get_room(&db, &room_id) {
            if current_room.status == "stopped" || current_room.status == "finished" {
                break;
            }
        }
    }

    // Ensure room is marked finished
    if let Ok(Some(current_room)) = rooms::get_room(&db, &room_id) {
        if current_room.status == "running" {
            rooms::update_room_status(&db, &room_id, "finished")?;
            ctx.emit(RoomEvent::SessionEnded {
                summary: "Session reached maximum turns.".to_string(),
            });
        }
    }

    Ok(())
}
