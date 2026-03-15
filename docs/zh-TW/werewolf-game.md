# 狼人殺遊戲指南

本指南說明如何使用 AgentLab 的 Room 系統來進行狼人殺遊戲。AI agents 扮演玩家角色，人類也可以加入遊戲。

## Room 運作原理

**Room** 是由 **orchestrator**（主持人 agent）管理的多 agent 對話。Orchestrator 擁有 7 個工具：

| 工具 | 說明 |
|------|------|
| `broadcast_message` | 向所有參與者發送公開訊息 |
| `send_private_message` | 向特定參與者發送私訊 |
| `ask_agent` | 向特定參與者提問並等待回覆（支援 `private` 旗標） |
| `ask_all_agents` | 向所有參與者提出相同問題並收集回覆 |
| `get_room_history` | 取得近期公開訊息 |
| `advance_turn` | 推進回合計數器 |
| `end_session` | 結束會話並附上摘要 |

參與者可以是 **AI agent**（由 agent 設定驅動）或 **人類**（標記 `is_human`）。人類參與者的回應逾時為 5 分鐘。

訊息可見度類型：**public**（所有人可見）、**private**（僅發送者與目標可見）、**system**（系統管理訊息）。

## 設定步驟

### 步驟 1：建立 Orchestrator Agent

建立一個擔任遊戲主持人的 agent。可透過 Admin UI（`/admin/agents/new`）或 API：

```bash
curl -X POST http://localhost:8080/api/agents \
  -H "Content-Type: application/json" \
  -d '{
    "message": "建立一個狼人殺遊戲主持人 agent。它負責管理狼人殺遊戲：秘密分配角色、執行日夜循環、主持討論和投票、宣布結果。風格要戲劇化且有趣。"
  }'
```

記下回傳的 agent ID（例如 `ORCHESTRATOR_ID`）。

### 步驟 2：建立玩家 Agents

為每個角色建立 AI agent。村民範例：

```bash
curl -X POST http://localhost:8080/api/agents \
  -H "Content-Type: application/json" \
  -d '{
    "message": "建立一個狼人殺遊戲的村民角色。名字：小明。個性：分析型且謹慎，傾向先觀察再發言，會試圖找出其他玩家言論中的邏輯矛盾。"
  }'
```

重複建立其他角色 — 大膽的村民、狡猾的狼人、智慧的預言家等。建議至少 5-6 個玩家。

### 步驟 3：建立 Room

透過 Admin UI（`/admin/rooms/new`）或 API：

```bash
curl -X POST http://localhost:8080/admin/rooms \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "name=狼人殺之夜&orchestrator_agent_id=ORCHESTRATOR_ID&scenario=進行一場狼人殺遊戲。秘密分配角色：2 個狼人、1 個預言家、其餘為村民。執行日夜循環。夜晚時使用私訊處理狼人殺人和預言家查驗。白天時主持公開討論和投票。使用 advance_turn 切換階段。&max_turns=100"
```

### 步驟 4：加入參與者

加入 AI agent 參與者：

```bash
# 加入 AI 玩家
curl -X POST http://localhost:8080/api/rooms/ROOM_ID/participants \
  -H "Content-Type: application/json" \
  -d '{
    "agent_id": "AGENT_ID",
    "alias": "小明",
    "role": "participant"
  }'
```

以人類玩家身份加入：

```bash
curl -X POST http://localhost:8080/api/rooms/ROOM_ID/participants \
  -H "Content-Type: application/json" \
  -d '{
    "alias": "玩家",
    "is_human": true,
    "role": "participant"
  }'
```

### 步驟 5：開始遊戲

```bash
curl -X POST http://localhost:8080/api/rooms/ROOM_ID/start
```

## 進行遊戲

### 透過 Admin UI

在瀏覽器開啟 `/admin/rooms/ROOM_ID`。若你是人類參與者，加上 `?as=玩家`（你的別名）即可只看到你可見的訊息，並在輪到你時收到提示。

UI 透過 Server-Sent Events (SSE) 自動更新 — 訊息會即時出現。

### 透過 API（人類玩家）

**監看遊戲串流：**

```bash
curl -N http://localhost:8080/api/rooms/ROOM_ID/stream?as=玩家
```

當看到 `WaitingForHuman` 事件時，提交你的回覆：

```bash
curl -X POST http://localhost:8080/api/rooms/ROOM_ID/reply \
  -H "Content-Type: application/json" \
  -d '{"alias": "玩家", "content": "我覺得小紅很可疑，因為她昨晚太安靜了。"}'
```

### 關鍵遊戲機制

1. **夜晚階段** — Orchestrator 使用 `send_private_message` 和 `ask_agent`（`private: true`）秘密處理狼人殺人和預言家查驗。
2. **白天階段** — Orchestrator 使用 `broadcast_message` 發布公告，`ask_all_agents` 收集每位玩家的討論發言。
3. **投票** — Orchestrator 要求所有玩家公開投票，然後統計結果。
4. **回合推進** — `advance_turn` 標記夜晚與白天階段的切換。
5. **遊戲結束** — 當勝利條件達成時，orchestrator 呼叫 `end_session` 並附上摘要。

### 停止遊戲

```bash
curl -X POST http://localhost:8080/api/rooms/ROOM_ID/stop
```

## API 參考

| 方法 | 端點 | 說明 |
|------|------|------|
| GET | `/api/rooms` | 列出所有 rooms |
| GET | `/api/rooms/{room_id}` | 取得 room 詳情與參與者 |
| POST | `/api/rooms/{room_id}/participants` | 加入參與者 |
| POST | `/api/rooms/{room_id}/start` | 啟動 room 會話 |
| POST | `/api/rooms/{room_id}/stop` | 停止 room 會話 |
| POST | `/api/rooms/{room_id}/reply` | 提交人類玩家的回覆 |
| POST | `/api/rooms/{room_id}/intervene` | 注入系統訊息 |
| GET | `/api/rooms/{room_id}/messages` | 取得所有 room 訊息 |
| GET | `/api/rooms/{room_id}/stream` | Room 事件的 SSE 串流 |
| DELETE | `/api/rooms/{room_id}` | 刪除 room |

### SSE 事件類型

```json
{"type": "MessageSent", "sender_alias": "GM", "content": "...", "visibility": "public", "target_alias": ""}
{"type": "AgentResponded", "agent_alias": "小明", "content": "...", "visibility": "public", "target_alias": "GM"}
{"type": "WaitingForHuman", "alias": "玩家", "question": "你想投票給誰？"}
{"type": "TurnAdvanced", "turn_number": 3}
{"type": "SessionEnded", "summary": "村民獲勝！所有狼人已被淘汰。"}
```

串流端點的 `?as=ALIAS` 查詢參數會過濾事件，只顯示該參與者可見的內容 — 這對於向村民玩家隱藏狼人私訊至關重要。
