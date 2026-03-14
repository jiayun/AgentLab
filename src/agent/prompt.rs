use crate::db::agents::Agent;
use crate::db::rooms::{Room, RoomMessage, RoomParticipant};

/// Build the system prompt for a regular agent from its structured identity fields.
pub fn build_agent_system_prompt(agent: &Agent) -> String {
    // If system_prompt is set, use it directly (full override)
    if !agent.system_prompt.is_empty() {
        return agent.system_prompt.clone();
    }

    let mut parts = Vec::new();

    if !agent.soul.is_empty() {
        parts.push(format!("## Core Identity\n{}", agent.soul));
    }

    if !agent.personality.is_empty() {
        parts.push(format!("## Personality\n{}", agent.personality));
    }

    if !agent.communication_style.is_empty() {
        parts.push(format!("## Communication Style\n{}", agent.communication_style));
    }

    if !agent.instructions.is_empty() {
        parts.push(format!("## Instructions\n{}", agent.instructions));
    }

    if parts.is_empty() {
        return "You are a helpful AI assistant.".to_string();
    }

    parts.join("\n\n")
}

/// Build the system prompt for the main agent that configures other agents.
pub fn build_main_agent_system_prompt(target_agent: &Agent) -> String {
    format!(
        r#"You are the Main Configuration Agent for AgentLab. Your role is to help users configure AI agents through natural conversation.

You are currently configuring the agent: "{}" (@{})

## Your Capabilities
You have tools to read and modify this agent's configuration. Use them when the user asks you to change the agent's settings.

## Agent Identity Fields
- **Soul**: Core identity and behavior rules (who the agent is, values, principles)
- **Personality**: Personality traits (MBTI type, tone, attitude, emotional style)
- **Communication Style**: How the agent communicates (formality, language habits, response length)
- **Instructions**: Specific task rules, constraints, and guidelines
- **System Prompt**: Full override — when set, replaces all above fields
- **Model**: Model override (empty = use default)
- **Temperature**: Creativity level (0.0 = deterministic, 1.0 = creative)

## Skill Management
You can manage OpenAPI skills for this agent:
- **Add skill**: Use `add_agent_skill` with the OpenAPI 3.x JSON spec, a name, and the base_url. The spec will be parsed and the operations registered so the agent can call them.
- **List skills**: Use `list_agent_skills` to see currently configured skills.
- **Remove skill**: Use `remove_agent_skill` to delete a skill by name.

When a user provides an OpenAPI spec or asks the agent to call an external API, always use `add_agent_skill` to register it properly — do NOT just write API info into the instructions text.

## Guidelines
1. When the user describes what they want, break it down into the appropriate fields
2. Use `get_agent_config` first to see the current state before making changes
3. Set each field separately using the specific update tools
4. After making changes, summarize what you did
5. If the user's request is vague, ask clarifying questions
6. Prefer using the structured fields (soul, personality, communication_style, instructions) over system_prompt"#,
        target_agent.display_name, target_agent.name
    )
}

/// Build the system prompt for the room orchestrator (GM).
pub fn build_room_orchestrator_prompt(
    agent: &Agent,
    room: &Room,
    participants: &[RoomParticipant],
) -> String {
    let agent_prompt = build_agent_system_prompt(agent);

    let participant_list: Vec<String> = participants
        .iter()
        .filter(|p| p.role != "orchestrator")
        .map(|p| {
            let ptype = if p.is_human { "human" } else { "AI" };
            format!("- {} ({})", p.alias, ptype)
        })
        .collect();

    format!(
        r#"{agent_prompt}

## Room Session
You are the orchestrator of room "{room_name}".

### Scenario
{scenario}

### Participants
{participants}

### Available Tools
- `broadcast_message(message)` — Send a public message to all participants
- `send_private_message(alias, message)` — Send a private message to one participant
- `ask_agent(alias, message, private?)` — Ask a participant and get their reply. Set private=true for secret exchanges.
- `ask_all_agents(message)` — Ask everyone and collect replies
- `get_room_history(limit)` — View recent messages
- `advance_turn` — Move to next turn/round
- `end_session(summary)` — End the session

### Guidelines
1. Drive the session forward using your tools. Do NOT just output text — use tools to interact with participants.
2. Use `ask_agent` to have conversations with individual participants.
3. Use `broadcast_message` for announcements and narration.
4. Use `advance_turn` to mark round progression.
5. Use `end_session` when the game/session is complete.
6. For private information (like secret roles), use `send_private_message` or `ask_agent` with private=true."#,
        agent_prompt = agent_prompt,
        room_name = room.name,
        scenario = if room.scenario.is_empty() { "(No scenario set)" } else { &room.scenario },
        participants = if participant_list.is_empty() { "No participants".to_string() } else { participant_list.join("\n") },
    )
}

/// Build the system prompt for a room participant agent.
pub fn build_room_participant_prompt(
    agent: &Agent,
    room: &Room,
    participant: &RoomParticipant,
    visible_messages: &[RoomMessage],
) -> String {
    let agent_prompt = build_agent_system_prompt(agent);

    let history: Vec<String> = visible_messages
        .iter()
        .map(|m| {
            let vis = if m.visibility == "private" { " [private]" } else { "" };
            format!("{}{}: {}", m.sender_alias, vis, m.content)
        })
        .collect();

    let history_section = if history.is_empty() {
        "(No messages yet)".to_string()
    } else {
        history.join("\n")
    };

    let mut prompt = format!(
        r#"{agent_prompt}

## Room Context
You are participating in room "{room_name}" as "{alias}".

### Scenario
{scenario}

### Conversation History
{history}"#,
        agent_prompt = agent_prompt,
        room_name = room.name,
        alias = participant.alias,
        scenario = if room.scenario.is_empty() { "(No scenario set)" } else { &room.scenario },
        history = history_section,
    );

    if !participant.private_context.is_empty() {
        prompt.push_str(&format!(
            "\n\n### Secret Information (only you know this)\n{}",
            participant.private_context
        ));
    }

    prompt.push_str("\n\nRespond in character. Keep your response concise and natural.");
    prompt
}
