use crate::db::agents::Agent;

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
