use anyhow::{Context, Result};
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::sync::mpsc;

use super::traits::*;
use crate::config::ProviderConfig;

pub struct OpenAiCompatibleProvider {
    base_url: String,
    model: String,
    api_key: Option<String>,
    client: reqwest::Client,
}

impl OpenAiCompatibleProvider {
    pub fn new(config: &ProviderConfig, client: reqwest::Client) -> Self {
        let base_url = config.api_url.trim_end_matches('/').to_string();
        Self {
            base_url,
            model: config.model.clone(),
            api_key: config.api_key.clone(),
            client,
        }
    }

    fn chat_url(&self) -> String {
        if self.base_url.ends_with("/chat/completions") {
            self.base_url.clone()
        } else {
            format!("{}/chat/completions", self.base_url)
        }
    }

    fn build_request(&self, body: &serde_json::Value) -> reqwest::RequestBuilder {
        let mut req = self.client.post(self.chat_url()).json(body);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        req
    }

    /// Non-streaming chat with optional tools
    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolDefinition]>,
        temperature: f64,
    ) -> Result<ChatResponse> {
        let mut body = serde_json::json!({
            "model": &self.model,
            "messages": messages,
            "temperature": temperature,
            "stream": false,
        });

        if let Some(tools) = tools {
            if !tools.is_empty() {
                body["tools"] = serde_json::to_value(tools)?;
            }
        }

        let resp = self
            .build_request(&body)
            .send()
            .await
            .context("Failed to call LLM API")?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("LLM API error {status}: {text}");
        }

        let api_resp: ApiChatResponse = resp.json().await.context("Failed to parse LLM response")?;
        let choice = api_resp
            .choices
            .into_iter()
            .next()
            .context("No choices in LLM response")?;

        Ok(ChatResponse {
            text: choice.message.content,
            tool_calls: choice.message.tool_calls.unwrap_or_default(),
        })
    }

    /// Streaming chat (no tools) — returns a channel receiver of StreamChunks
    pub async fn stream_chat(
        &self,
        messages: &[ChatMessage],
        temperature: f64,
        model_override: Option<&str>,
    ) -> Result<mpsc::Receiver<StreamChunk>> {
        let model = model_override.unwrap_or(&self.model);
        let body = serde_json::json!({
            "model": model,
            "messages": messages,
            "temperature": temperature,
            "stream": true,
        });

        let resp = self
            .build_request(&body)
            .header("Accept", "text/event-stream")
            .send()
            .await
            .context("Failed to call LLM API for streaming")?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("LLM API error {status}: {text}");
        }

        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move {
            let mut stream = resp.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk) = stream.next().await {
                let bytes = match chunk {
                    Ok(b) => b,
                    Err(e) => {
                        tracing::error!("Stream error: {e}");
                        break;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&bytes));

                // Process complete SSE lines
                while let Some(pos) = buffer.find('\n') {
                    let line = buffer[..pos].trim().to_string();
                    buffer = buffer[pos + 1..].to_string();

                    if line.is_empty() || line.starts_with(':') {
                        continue;
                    }

                    if let Some(data) = line.strip_prefix("data:") {
                        let data = data.trim();
                        if data == "[DONE]" {
                            let _ = tx.send(StreamChunk::final_chunk()).await;
                            return;
                        }

                        if let Ok(chunk_resp) =
                            serde_json::from_str::<StreamChunkResponse>(data)
                        {
                            if let Some(choice) = chunk_resp.choices.first() {
                                if let Some(content) = &choice.delta.content {
                                    if !content.is_empty() {
                                        let _ =
                                            tx.send(StreamChunk::delta(content)).await;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let _ = tx.send(StreamChunk::final_chunk()).await;
        });

        Ok(rx)
    }
}

// -- API types --

#[derive(Deserialize)]
struct ApiChatResponse {
    choices: Vec<ApiChoice>,
}

#[derive(Deserialize)]
struct ApiChoice {
    message: ApiMessage,
}

#[derive(Deserialize)]
struct ApiMessage {
    content: Option<String>,
    tool_calls: Option<Vec<ToolCallMessage>>,
}

#[derive(Deserialize)]
struct StreamChunkResponse {
    choices: Vec<StreamChoice>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,
}
