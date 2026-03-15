# AgentLab

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)

A Rust-based AI agent orchestration platform for managing and configuring multiple AI agents through a unified web interface and API.

## Features

- **Agent Management** — Create and configure AI agents with customizable identity, personality, and instructions
- **Multi-Agent Rooms** — Orchestrate collaborative conversations where multiple agents interact with humans
- **OpenAPI Skills** — Upload OpenAPI specs as agent tools, enabling agents to call external APIs
- **Streaming Responses** — Real-time conversation streaming via Server-Sent Events (SSE)
- **OpenAI-Compatible** — Works with any OpenAI API-compatible provider (Ollama, cloud LLMs, etc.)
- **Web Admin UI** — Built-in admin interface for managing agents, conversations, and rooms
- **SQLite Storage** — Lightweight persistent storage for agents, conversations, and history

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) (1.75+)
- An OpenAI-compatible API endpoint (e.g., [Ollama](https://ollama.ai/))

### Install & Run

```bash
git clone https://github.com/pttlink/AgentLab.git
cd AgentLab
cargo build --release
./target/release/agentlab
```

### Configuration

Create an `agentlab.toml` in the project root:

```toml
[server]
port = 8080

[provider]
api_url = "http://localhost:11434/v1"   # OpenAI-compatible endpoint
model = "llama3"                        # Model name
# api_key = "your-api-key"             # Optional, for cloud providers
```

Environment variables override the config file:

| Variable | Description |
|----------|-------------|
| `AGENTLAB_API_URL` | API endpoint URL |
| `AGENTLAB_MODEL` | Model name |
| `AGENTLAB_API_KEY` | API key |
| `AGENTLAB_PORT` | Server port |

### Usage

Once running, open `http://localhost:8080/admin/` to access the web UI, or use the REST API:

```bash
# Create an agent via the configuration agent
curl -X POST http://localhost:8080/api/agents \
  -H "Content-Type: application/json" \
  -d '{"message": "Create a helpful coding assistant"}'

# Chat with an agent
curl -X POST http://localhost:8080/api/agents/{id}/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Hello!"}'
```

## Architecture

```
src/
├── agent/       # Agent logic, prompts, room orchestrator
├── provider/    # LLM provider (OpenAI-compatible)
├── openapi/     # OpenAPI spec parsing and execution
├── db/          # SQLite models and queries
├── web/         # Axum REST API and admin UI
├── config.rs    # TOML configuration
├── lib.rs       # Library root
└── main.rs      # Entry point
```

## License

Licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT License](LICENSE-MIT)

at your option.
