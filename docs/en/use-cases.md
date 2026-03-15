# Use Cases

AgentLab's multi-agent Room system and single-agent chat can be applied to a variety of scenarios. Below are practical examples with setup instructions.

## Customer Support Team Simulation

Multiple specialist agents collaborate to handle customer inquiries, each with domain expertise.

### Setup

1. **Create specialist agents** — e.g., a billing expert, a technical support agent, and a general greeter.
2. **Create an orchestrator agent** — instructs it to route customer questions to the right specialist, synthesize answers, and ensure the customer gets a coherent response.
3. **Create a room** with the scenario: *"A customer support team. Route incoming questions to the appropriate specialist. The customer is a human participant."*
4. **Add agents and a human participant** (the customer) to the room.
5. **Start the room** and interact as the customer via the Admin UI or API.

### Example Agent Prompts

**Orchestrator:** *"You manage a customer support team. When a customer asks a question, determine which specialist should answer. Use ask_agent to get the specialist's response, then broadcast a polished answer to the customer."*

**Billing Specialist:** *"You are a billing expert. Answer questions about invoices, payment methods, refunds, and subscription plans. Be precise and reference policy when relevant."*

**Technical Support:** *"You are a technical support engineer. Help with setup issues, error messages, integration problems, and performance tuning. Ask clarifying questions when the problem is ambiguous."*

### How It Works

- The orchestrator receives the customer's message via `ask_agent` (human participant).
- It uses `ask_agent` to consult the appropriate specialist privately.
- It uses `broadcast_message` to deliver the final answer to the customer.

---

## Technical Interview Simulation

An AI interviewer conducts a structured interview with a human candidate.

### Setup

1. **Create an interviewer agent** with instructions covering the interview format, topics, and evaluation criteria.
2. **Create a room** with the scenario describing the interview structure (e.g., intro → coding questions → system design → behavioral → wrap-up).
3. **Add the interviewer agent** and **a human participant** (the candidate).
4. **Start the room**.

### Example Scenario

```
Conduct a 30-minute backend engineering interview. Structure:
1. Brief introduction (1 turn)
2. Two coding questions of medium difficulty (ask_agent with the candidate)
3. One system design question
4. Behavioral questions about teamwork
5. Wrap up and provide feedback using end_session

Evaluate clarity of thought, problem-solving approach, and communication.
Advance turns between sections.
```

The orchestrator drives the conversation, using `ask_agent` to pose questions and follow-ups, `advance_turn` between sections, and `end_session` with evaluation notes.

---

## Brainstorming Room

Multiple agents approach a topic from different perspectives, generating diverse ideas.

### Setup

1. **Create perspective agents** — e.g., an optimist, a skeptic, a technical expert, and a creative thinker.
2. **Create an orchestrator** that facilitates structured brainstorming: problem definition → individual ideas → discussion → synthesis.
3. **Create a room** with the brainstorming topic as the scenario.
4. **Add all agents** (and optionally a human facilitator).

### Example Scenario

```
Brainstorm ideas for improving user onboarding in a SaaS product.
Round 1: Ask each participant for their initial ideas (ask_all_agents).
Round 2: Share all ideas publicly, then ask each participant to critique and build on others' ideas.
Round 3: Ask all participants to vote on the top 3 ideas.
Synthesize the results and end the session with a ranked list.
```

The key mechanism is alternating between `ask_all_agents` (parallel idea generation) and `broadcast_message` (sharing results), with `advance_turn` marking each round.

---

## OpenAPI Skills Integration

Agents can call external APIs by uploading OpenAPI specifications as skills.

### How Skills Work

1. Go to the Agent detail page in the Admin UI.
2. Upload an OpenAPI spec (JSON or YAML) as a **skill**.
3. The agent's tool list automatically includes the API operations from the spec.
4. During conversation, the agent can invoke these operations as tool calls.

### Example: Weather-Aware Assistant

1. **Create an agent** — a helpful assistant that can check weather.
2. **Upload an OpenAPI spec** for a weather API (e.g., OpenWeatherMap).
3. **Chat with the agent** — ask "What's the weather like in Tokyo?" and the agent will call the weather API and respond with real data.

### Example: Multi-Agent with External APIs

Combine Rooms with Skills for powerful workflows:

1. **Data Analyst agent** — has a skill for querying a database API.
2. **Report Writer agent** — generates formatted reports from data.
3. **Orchestrator** — asks the analyst to pull data, passes results to the writer, then broadcasts the final report.

This pattern enables agents to fetch real data, process it, and deliver results — all orchestrated automatically within a Room session.

---

## Single-Agent Chat

Not every use case requires a Room. For simple interactions, use the direct chat API:

```bash
# Create an agent
curl -X POST http://localhost:8080/api/agents \
  -H "Content-Type: application/json" \
  -d '{"message": "Create a code review assistant that focuses on security and performance"}'

# Chat (streaming via SSE)
curl -N http://localhost:8080/api/agents/AGENT_ID/chat/stream \
  -H "Content-Type: application/json" \
  -d '{"message": "Review this function: function login(user, pass) { return db.query(`SELECT * FROM users WHERE name=${user} AND pass=${pass}`) }"}'
```

The agent maintains conversation history automatically. Use the Admin UI at `/admin/agents/AGENT_ID` for an interactive chat interface.
