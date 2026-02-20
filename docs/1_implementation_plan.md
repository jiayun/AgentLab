# AgentLab 實作計畫

## Context

在空的 Rust repo 中實作一個 Agent Lab 系統，能夠建立和管理多個 AI 代理。使用者透過 Web UI 和「主要 Agent」對話來設定其他 AI 代理，設定好的代理可以直接對話測試。參考 `/Users/jiayun/projects/zeroclaw` 的架構模式。

### 確認的需求
- **Frontend**: htmx + Askama 模板（Rust 端渲染）
- **Storage**: SQLite (rusqlite)
- **Streaming**: SSE 逐字輸出
- **Provider**: 遠端 Ollama（OpenAI 相容 API），可設定 base URL
- **OpenAPI Skill**: 完整解析 OpenAPI 3.x spec，轉成 agent 可呼叫的 tools
- **主要 Agent**: 對話式修改其他 agent 的設定

---

## 專案結構

單一 crate（非 workspace），簡化管理。

```
AgentLab/
├── Cargo.toml
├── agentlab.toml              # 設定檔（provider URL, model, port）
├── src/
│   ├── main.rs                # 啟動 Axum server
│   ├── lib.rs
│   ├── config.rs              # 設定載入（TOML + env vars）
│   ├── db/
│   │   ├── mod.rs             # SQLite 初始化 + migrations
│   │   ├── agents.rs          # Agent CRUD
│   │   ├── conversations.rs   # 對話/訊息 CRUD
│   │   └── skills.rs          # Skill 儲存
│   ├── provider/
│   │   ├── mod.rs
│   │   ├── traits.rs          # Provider trait, ChatMessage, ToolCall, StreamChunk
│   │   └── openai_compatible.rs  # OpenAI 相容 provider（參考 zeroclaw）
│   ├── agent/
│   │   ├── mod.rs
│   │   ├── main_agent.rs      # 主要 Agent（tool calling 修改其他 agent）
│   │   ├── chat_agent.rs      # 一般 agent 對話（含 OpenAPI skill 呼叫）
│   │   └── prompt.rs          # System prompt 組建
│   ├── openapi/
│   │   ├── mod.rs
│   │   ├── parser.rs          # OpenAPI 3.x → ToolDefinition
│   │   └── executor.rs        # 執行 HTTP 呼叫
│   └── web/
│       ├── mod.rs             # Axum router + AppState
│       ├── handlers.rs        # HTTP handlers（頁面 + API）
│       └── sse.rs             # SSE streaming handler
├── templates/
│   ├── base.html              # 共用 layout（含 htmx CDN）
│   └── admin/
│       ├── index.html         # Agent 列表
│       ├── agent_create.html  # 建立 agent 表單
│       ├── agent_edit.html    # 和主要 Agent 對話來設定
│       └── agent_chat.html    # 和 agent 對話測試
└── static/                    # CSS 等靜態檔案
```

## 關鍵依賴

```toml
tokio, axum, tower-http          # Web + Async
reqwest                          # HTTP client（LLM API + OpenAPI 執行）
rusqlite = { features = ["bundled"] }  # SQLite
askama, askama_axum              # HTML 模板
serde, serde_json, toml          # 序列化
futures-util, tokio-stream       # SSE streaming
async-trait                      # Provider trait
uuid, chrono, anyhow, thiserror  # 工具
tracing, tracing-subscriber      # Logging
parking_lot                      # Mutex for SQLite
```

## Database Schema

```sql
-- agents: AI 代理設定
CREATE TABLE agents (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    -- 結構化 identity（參考 zeroclaw 的 SOUL.md + IDENTITY.md 概念）
    soul TEXT NOT NULL DEFAULT '',                -- 核心行為規範（你是誰、核心價值觀、行為準則）
    personality TEXT NOT NULL DEFAULT '',         -- 人格特質（MBTI、語氣、態度）
    communication_style TEXT NOT NULL DEFAULT '', -- 溝通風格（正式/非正式、用語習慣、回應長度）
    instructions TEXT NOT NULL DEFAULT '',        -- 其他指示（特定任務規則、限制條件）
    system_prompt TEXT NOT NULL DEFAULT '',       -- 完整 system prompt 覆寫（非空時忽略上面四欄）
    model TEXT NOT NULL DEFAULT '',               -- 模型覆寫（空 = 用預設）
    temperature REAL NOT NULL DEFAULT 0.7,
    is_main_agent INTEGER NOT NULL DEFAULT 0,    -- 主要 Agent 標記，不可被修改
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- conversations: 對話紀錄（以 browser session 區分）
CREATE TABLE conversations (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL REFERENCES agents(id),
    session_id TEXT NOT NULL,
    created_at TEXT NOT NULL
);

-- messages: 對話訊息
CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversations(id),
    role TEXT NOT NULL,            -- system/user/assistant/tool
    content TEXT NOT NULL,
    tool_calls_json TEXT,          -- assistant 的 tool calls (JSON)
    tool_call_id TEXT,             -- tool result 對應的 call id
    created_at TEXT NOT NULL
);

-- agent_skills: OpenAPI 技能
CREATE TABLE agent_skills (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL REFERENCES agents(id),
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    openapi_spec TEXT NOT NULL,         -- 原始 OpenAPI JSON
    parsed_tools_json TEXT NOT NULL,    -- 解析後的 tool definitions
    base_url TEXT NOT NULL DEFAULT '',
    auth_header TEXT,
    auth_value TEXT,
    created_at TEXT NOT NULL,
    UNIQUE(agent_id, name)
);
```

## API 端點

```
# 頁面（htmx 渲染）
GET  /admin/                        → Agent 列表
GET  /admin/agents/new              → 建立表單
POST /admin/agents                  → 建立 agent（表單提交）
GET  /admin/agents/:id              → Agent 設定頁（和主要 Agent 對話）
GET  /admin/agents/:id/chat         → Agent 對話測試頁

# API（JSON，htmx 呼叫）
POST   /admin/api/agents/:id/configure       → 傳訊息給主要 Agent
GET    /admin/api/agents/:id/configure/stream → SSE 主要 Agent 回應
POST   /admin/api/agents/:id/chat            → 傳訊息給 agent
GET    /admin/api/agents/:id/chat/stream     → SSE agent 回應
GET    /admin/api/agents/:id/config          → 取得 agent 設定 JSON
POST   /admin/api/agents/:id/skills          → 上傳 OpenAPI spec
DELETE /admin/api/agents/:id/skills/:sid     → 移除 skill
DELETE /admin/api/agents/:id                 → 刪除 agent
GET    /health                               → 健康檢查
```

## 主要 Agent 設計

主要 Agent 有固定的 system prompt + 管理工具，透過 function calling 修改目標 agent。

**Tool definitions:**
| Tool | 參數 | 說明 |
|------|------|------|
| `get_agent_config` | — | 讀取目標 agent 完整設定 |
| `update_agent_soul` | `{soul: str}` | 更新核心行為規範（你是誰、核心價值觀） |
| `update_agent_personality` | `{personality: str}` | 更新人格特質（MBTI、語氣、態度） |
| `update_agent_communication_style` | `{style: str}` | 更新溝通風格（正式度、用語、回應長度） |
| `update_agent_instructions` | `{instructions: str}` | 更新其他指示（任務規則、限制） |
| `update_agent_system_prompt` | `{system_prompt: str}` | 完整覆寫 system prompt（進階用） |
| `update_agent_model` | `{model: str}` | 更新模型 |
| `update_agent_temperature` | `{temperature: f64}` | 更新溫度 |
| `list_agent_skills` | — | 列出技能 |

**執行流程:**
1. 載入對話歷史
2. 組建含目標 agent 資訊的 system prompt
3. 呼叫 provider.chat() 帶 tools
4. 若 response 有 tool_calls → 執行 DB 操作 → 把結果餵回 → 繼續迴圈（max 5 輪）
5. 若 response 是純文字 → streaming 回傳給使用者

## OpenAPI Skill 設計

**Parser** (`openapi/parser.rs`):
- 解析 OpenAPI 3.x JSON spec
- 解析 `$ref` 引用
- 每個 path+method → `ParsedOperation`（operation_id, method, path, parameters, request_body）
- 合併所有參數成單一 JSON Schema 作為 tool parameters

**Executor** (`openapi/executor.rs`):
- 從 tool call arguments 組建 HTTP request（替換 path params, 加 query params, 設 request body）
- 設定 auth header
- 執行 HTTP 請求，回傳 response body

**Agent 對話時:**
- 載入 agent 的所有 skills → 合併 tool definitions
- LLM 回傳 tool call → 匹配到對應的 ParsedOperation → executor 執行 → 結果餵回 LLM

## SSE Streaming 設計

使用 Axum 的 `Sse` response type + `tokio_stream`:

```
event: message
data: {"delta": "text chunk"}

event: message
data: {"done": true}
```

前端使用 htmx SSE extension 或原生 `EventSource` 接收。

## 參考 zeroclaw 的關鍵檔案

| AgentLab 模組 | 參考 zeroclaw 檔案 | 重用內容 |
|---|---|---|
| `provider/openai_compatible.rs` | `src/providers/compatible.rs` | OpenAI 相容 API 呼叫、SSE 解析、tool call 序列化 |
| `provider/traits.rs` | `src/providers/traits.rs` | Provider trait、ChatMessage/ChatResponse/ToolCall 型別 |
| `agent/chat_agent.rs` | `src/agent/agent.rs` | Agent turn loop、tool call 執行、history 管理 |
| `agent/prompt.rs` | `src/agent/prompt.rs` | SystemPromptBuilder 模式 |
| `web/mod.rs` | `src/gateway/mod.rs` | Axum Router、AppState、middleware 設定 |

---

## 實作階段

### Phase 1: 基礎骨架
**目標**: Server 啟動、顯示空列表、可建立 agent

1. `Cargo.toml` + 所有 dependencies
2. `config.rs` — 載入 `agentlab.toml`（provider base_url, model, port）
3. `db/` — SQLite 初始化 + migration + agent CRUD
4. `web/mod.rs` — Axum router + AppState
5. `web/handlers.rs` — 列表頁、建立頁、建立 API
6. Askama templates: `base.html`, `index.html`, `agent_create.html`
7. `main.rs` — 啟動 server on `:8080`

### Phase 2: Provider + 基本對話
**目標**: 和 agent 對話，SSE streaming 回應

1. `provider/traits.rs` — 核心型別
2. `provider/openai_compatible.rs` — streaming chat
3. `db/conversations.rs` — 對話/訊息 CRUD
4. `agent/chat_agent.rs` — 簡單對話迴圈（無 tools）
5. `web/sse.rs` — SSE endpoint
6. `agent_chat.html` template + htmx SSE
7. 測試：建立 agent → 對話 → 看到 streaming 回應

### Phase 3: 主要 Agent + Tool Calling
**目標**: 用對話修改 agent 設定

1. `agent/main_agent.rs` — system prompt + tool definitions + 執行迴圈
2. `agent/prompt.rs` — prompt builder
3. Configure endpoints（POST + SSE）
4. `agent_edit.html` template — 對話 UI + 設定顯示面板
5. 測試：和主要 Agent 說「讓這個 agent 成為客服」→ 看到 identity 被更新

### Phase 4: OpenAPI Skills
**目標**: 上傳 OpenAPI spec，agent 可呼叫外部 API

1. `openapi/parser.rs` — 解析 OpenAPI 3.x → tool definitions
2. `openapi/executor.rs` — 組建 + 執行 HTTP request
3. `db/skills.rs` — skill CRUD
4. Skill 上傳 handler + UI
5. 更新 `chat_agent.rs` — 載入 skills → 多輪 tool call loop
6. 測試：上傳 Petstore spec → 問 agent「列出寵物」→ 呼叫 API → 回傳結果

### Phase 5: 收尾
**目標**: 穩定可用

1. 錯誤處理和 provider timeout 處理
2. 對話歷史載入
3. CSS 美化
4. Agent 刪除 API
5. 設定檔文件

## 驗證方式

1. `cargo build` 確認編譯通過
2. `cargo test` 執行單元測試
3. 啟動本機 Ollama 或遠端 Ollama，設定 `agentlab.toml` 中的 `api_url`
4. 瀏覽 `http://localhost:8080/admin/` → 確認列表頁正常
5. 建立一個 agent → 確認出現在列表
6. 點 Configure → 和主要 Agent 對話 → 確認設定被修改
7. 點 Chat → 和設定好的 agent 對話 → 確認 streaming 正常
8. 上傳 OpenAPI spec → 對話中觸發 tool call → 確認外部 API 被呼叫
