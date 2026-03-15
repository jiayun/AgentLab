# Werewolf Game Guide

This guide walks through setting up and running a Werewolf (Mafia) game using AgentLab's Room system. AI agents play the villager roles while a human can join as a player.

## How Rooms Work

A **Room** is a multi-agent conversation managed by an **orchestrator** — a special agent that controls the flow. The orchestrator has 7 tools:

| Tool | Description |
|------|-------------|
| `broadcast_message` | Send a public message to all participants |
| `send_private_message` | Send a private message to one participant |
| `ask_agent` | Ask a specific participant and wait for their reply (supports `private` flag) |
| `ask_all_agents` | Ask all participants the same question and collect replies |
| `get_room_history` | Retrieve recent public messages |
| `advance_turn` | Advance the turn/round counter |
| `end_session` | End the session with a summary |

Participants can be **AI agents** (backed by an agent configuration) or **humans** (flagged with `is_human`). Human participants have a 5-minute response timeout.

Message visibility types: **public** (everyone sees it), **private** (only sender and target), **system** (administrative).

## Setup

### Step 1: Create the Orchestrator Agent

Create an agent that will serve as the game master. Via the Admin UI at `/admin/agents/new`, or via API:

```bash
curl -X POST http://localhost:8080/api/agents \
  -H "Content-Type: application/json" \
  -d '{
    "message": "Create a Werewolf game master agent. It should manage a Werewolf/Mafia party game: assign roles secretly, run day/night cycles, facilitate discussion and voting, and announce results. It should be dramatic and entertaining."
  }'
```

Note the returned agent ID (e.g., `ORCHESTRATOR_ID`).

### Step 2: Create Player Agents

Create AI agents for each role. Example for a villager:

```bash
curl -X POST http://localhost:8080/api/agents \
  -H "Content-Type: application/json" \
  -d '{
    "message": "Create a villager character for a Werewolf game. Name: Alice. Personality: analytical and cautious, tends to observe before speaking. She tries to find logical inconsistencies in other players statements."
  }'
```

Repeat for other characters — a bold villager, a sneaky werewolf, a wise seer, etc. You need at least 5-6 players for a good game.

### Step 3: Create the Room

Via Admin UI at `/admin/rooms/new`, or via API:

```bash
curl -X POST http://localhost:8080/admin/rooms \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "name=Werewolf Night&orchestrator_agent_id=ORCHESTRATOR_ID&scenario=Run a Werewolf game with these players. Assign roles secretly: 2 werewolves, 1 seer, rest are villagers. Run night/day cycles. During night, use private messages for werewolf kills and seer investigations. During day, facilitate public discussion and voting. Use advance_turn between phases.&max_turns=100"
```

### Step 4: Add Participants

Add each AI agent as a participant:

```bash
# Add AI player
curl -X POST http://localhost:8080/api/rooms/ROOM_ID/participants \
  -H "Content-Type: application/json" \
  -d '{
    "agent_id": "AGENT_ID",
    "alias": "Alice",
    "role": "participant"
  }'
```

To join as a human player:

```bash
curl -X POST http://localhost:8080/api/rooms/ROOM_ID/participants \
  -H "Content-Type: application/json" \
  -d '{
    "alias": "You",
    "is_human": true,
    "role": "participant"
  }'
```

### Step 5: Start the Game

```bash
curl -X POST http://localhost:8080/api/rooms/ROOM_ID/start
```

## Playing the Game

### Via Admin UI

Open `/admin/rooms/ROOM_ID` in your browser. If you are a human participant, add `?as=You` (your alias) to see only messages visible to you and get prompted when it's your turn.

The UI auto-updates via Server-Sent Events (SSE) — you'll see messages appear in real time.

### Via API (Human Player)

**Watch the game stream:**

```bash
curl -N http://localhost:8080/api/rooms/ROOM_ID/stream?as=You
```

When you see a `WaitingForHuman` event, submit your reply:

```bash
curl -X POST http://localhost:8080/api/rooms/ROOM_ID/reply \
  -H "Content-Type: application/json" \
  -d '{"alias": "You", "content": "I think Bob is suspicious because he was too quiet last night."}'
```

### Key Game Mechanics

1. **Night Phase** — The orchestrator uses `send_private_message` and `ask_agent` with `private: true` to handle werewolf kills and seer investigations secretly.
2. **Day Phase** — The orchestrator uses `broadcast_message` for announcements and `ask_all_agents` to gather each player's discussion input.
3. **Voting** — The orchestrator asks all players to vote publicly, then tallies results.
4. **Turn Progression** — `advance_turn` marks the transition between night and day phases.
5. **Game End** — The orchestrator calls `end_session` with a summary when a win condition is met.

### Stopping a Game

```bash
curl -X POST http://localhost:8080/api/rooms/ROOM_ID/stop
```

## API Reference

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/rooms` | List all rooms |
| GET | `/api/rooms/{room_id}` | Get room details and participants |
| POST | `/api/rooms/{room_id}/participants` | Add a participant |
| POST | `/api/rooms/{room_id}/start` | Start the room session |
| POST | `/api/rooms/{room_id}/stop` | Stop the room session |
| POST | `/api/rooms/{room_id}/reply` | Submit a human player's reply |
| POST | `/api/rooms/{room_id}/intervene` | Inject a system message |
| GET | `/api/rooms/{room_id}/messages` | Get all room messages |
| GET | `/api/rooms/{room_id}/stream` | SSE stream of room events |
| DELETE | `/api/rooms/{room_id}` | Delete a room |

### SSE Event Types

```json
{"type": "MessageSent", "sender_alias": "GM", "content": "...", "visibility": "public", "target_alias": ""}
{"type": "AgentResponded", "agent_alias": "Alice", "content": "...", "visibility": "public", "target_alias": "GM"}
{"type": "WaitingForHuman", "alias": "You", "question": "Who do you want to vote for?"}
{"type": "TurnAdvanced", "turn_number": 3}
{"type": "SessionEnded", "summary": "The villagers win! All werewolves have been eliminated."}
```

The `?as=ALIAS` query parameter on the stream endpoint filters events to only those visible to the specified participant — essential for hiding private werewolf communications from villager players.
