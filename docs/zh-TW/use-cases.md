# 應用場景

AgentLab 的多 Agent Room 系統和單一 Agent 對話可應用於多種場景。以下是實際範例與設定說明。

## 客服團隊模擬

多個專業 agent 協作處理客戶詢問，各自具備領域專長。

### 設定

1. **建立專家 agents** — 例如帳務專家、技術支援 agent、通用接待員。
2. **建立 orchestrator agent** — 指示它將客戶問題導向正確的專家，整合回答，確保客戶得到完整的回應。
3. **建立 room**，scenario 設為：*「客服團隊。將客戶問題轉給對應的專家，客戶為人類參與者。」*
4. **加入 agents 和一個人類參與者**（客戶）。
5. **啟動 room**，透過 Admin UI 或 API 以客戶身份互動。

### Agent Prompt 範例

**Orchestrator：** *「你管理一個客服團隊。當客戶提問時，判斷由哪位專家回答。使用 ask_agent 取得專家回覆，再用 broadcast_message 向客戶發送整理後的答案。」*

**帳務專家：** *「你是帳務專家。回答關於發票、付款方式、退款和訂閱方案的問題。回答要精確，必要時引用政策。」*

**技術支援：** *「你是技術支援工程師。協助處理安裝問題、錯誤訊息、整合問題和效能調校。當問題不明確時主動追問。」*

### 運作方式

- Orchestrator 透過 `ask_agent`（人類參與者）接收客戶訊息。
- 使用 `ask_agent` 私下諮詢對應的專家。
- 使用 `broadcast_message` 將最終回答傳達給客戶。

---

## 技術面試模擬

AI 面試官對人類候選人進行結構化面試。

### 設定

1. **建立面試官 agent**，設定面試格式、主題和評估標準。
2. **建立 room**，scenario 描述面試結構（例如：自我介紹 → 程式題 → 系統設計 → 行為問題 → 結尾）。
3. **加入面試官 agent** 和 **人類參與者**（候選人）。
4. **啟動 room**。

### Scenario 範例

```
進行一場 30 分鐘的後端工程師面試。結構：
1. 簡短自我介紹（1 回合）
2. 兩道中等難度程式題（使用 ask_agent 向候選人提問）
3. 一道系統設計題
4. 關於團隊合作的行為問題
5. 總結並使用 end_session 提供評估回饋

評估思路清晰度、問題解決方法和溝通能力。
各段落之間使用 advance_turn。
```

Orchestrator 驅動對話流程，使用 `ask_agent` 提問和追問，`advance_turn` 切換段落，最後以 `end_session` 附上評估結果。

---

## 腦力激盪 Room

多個 agent 從不同角度探討主題，產生多元想法。

### 設定

1. **建立不同觀點的 agents** — 例如樂觀者、質疑者、技術專家、創意思考者。
2. **建立 orchestrator** 來主持結構化腦力激盪：定義問題 → 個別提案 → 討論 → 綜合。
3. **建立 room**，以腦力激盪主題作為 scenario。
4. **加入所有 agents**（可選加入人類主持人）。

### Scenario 範例

```
針對「如何改善 SaaS 產品的使用者引導流程」進行腦力激盪。
第 1 輪：請每位參與者提出初始想法（ask_all_agents）。
第 2 輪：公開分享所有想法，請每位參與者評論並延伸他人的想法。
第 3 輪：請所有參與者票選前 3 名想法。
綜合結果，以排名清單結束會話。
```

核心機制是交替使用 `ask_all_agents`（平行產生想法）和 `broadcast_message`（分享結果），以 `advance_turn` 標記每一輪。

---

## OpenAPI Skills 整合

Agent 可透過上傳 OpenAPI 規格作為技能來呼叫外部 API。

### Skills 運作方式

1. 在 Admin UI 的 Agent 詳情頁面。
2. 上傳 OpenAPI 規格（JSON 或 YAML）作為 **skill**。
3. Agent 的工具列表自動包含規格中的 API 操作。
4. 對話中，agent 可以 tool call 的形式叫用這些操作。

### 範例：天氣查詢助手

1. **建立 agent** — 一個能查詢天氣的助手。
2. **上傳 OpenAPI 規格** — 天氣 API 的規格（例如 OpenWeatherMap）。
3. **與 agent 對話** — 問「東京現在天氣如何？」，agent 會呼叫天氣 API 並以真實數據回應。

### 範例：結合 Room 與外部 API

將 Room 與 Skills 結合實現強大的工作流程：

1. **資料分析師 agent** — 具備查詢資料庫 API 的 skill。
2. **報告撰寫 agent** — 從數據產生格式化報告。
3. **Orchestrator** — 請分析師撈取數據，將結果傳給撰寫者，然後廣播最終報告。

此模式讓 agent 能取得真實數據、處理並交付結果 — 全部在 Room 會話中自動協調完成。

---

## 單一 Agent 對話

並非每個場景都需要 Room。簡單互動可使用直接對話 API：

```bash
# 建立 agent
curl -X POST http://localhost:8080/api/agents \
  -H "Content-Type: application/json" \
  -d '{"message": "建立一個程式碼審查助手，專注於安全性和效能"}'

# 對話（透過 SSE 串流）
curl -N http://localhost:8080/api/agents/AGENT_ID/chat/stream \
  -H "Content-Type: application/json" \
  -d '{"message": "審查這個函數：function login(user, pass) { return db.query(`SELECT * FROM users WHERE name=${user} AND pass=${pass}`) }"}'
```

Agent 自動維護對話歷史。使用 Admin UI（`/admin/agents/AGENT_ID`）可取得互動式對話介面。
