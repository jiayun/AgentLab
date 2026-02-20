# Agent Lab 功能計畫

使用 Rust 語言

主要參考 @/Users/jiayun/projects/zeroclaw

## 功能

### 串接 API

1. 能夠串接 OpenAI 相容 API。
2. 我會先使用 Ollama Cloud （可相容 OpenAI）來測試，之後再串接其他 API。

### Agent 區分

1. 基本功能：建立一個 Agent Lab，能夠創建和管理多個 AI 代理（agents）。
2. 主要 Agent 專門用來協助 AI 代理的建立和修改，主要 Agent 本身設定不可被自己修改。
3. 每個被建立的 AI 代理都可以有自己的設定和功能或 Skill 設定，AI 代理不可動自己或別人的設定，只能被主要 Agent 調整。

### Web UI

預設本機 8080 port 在 http://localhost:8080/admin/ URL

1. 有 AI 代理管理列表（一開始當然沒有），可新增、修改、刪除（刪除先不提供 UI 操作，有 API 可處理即可）。
2. 新增只是簡單新增一個 AI 代理名稱，之後再提供修改功能來調整設定。
3. 修改功能則是和主要 Agent 互動（對話，可用瀏覽器 session 決定此次對話紀錄即可），主要 Agent 會根據使用者需求來調整 AI 代理的設定（SOUL, IDENTITY 等等）。
4. 使用者可以提供 openapi spec doc 來設定 AI 代理的 Skill。
5. AI 代理被設定過後，會在列表提供對話按鈕，讓使用者和設定好的 AI 代理對話並測試它。
