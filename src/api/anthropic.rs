//! Anthropic Messages API provider with SSE streaming.
//! Mirrors Claude Code's services/api/claude.ts.

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use reqwest::Client;
use std::time::Duration;

use super::{types::{ChatRequest, StreamEvent}, LLMProvider};

pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    base_url: String,
}

impl AnthropicProvider {
    pub fn new(api_key: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            api_key,
            base_url: "https://api.anthropic.com".to_string(),
        }
    }

    pub fn with_base_url(mut self, url: String) -> Self {
        self.base_url = url;
        self
    }
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Result<BoxStream<'static, Result<StreamEvent>>> {
        let url = format!("{}/v1/messages", self.base_url);

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .header("User-Agent", "claude-code/1.2.0")
            .json(&request)
            .send()
            .await
            .context("Sending request to Anthropic API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("Anthropic API error {}: {}", status, body));
        }

        // Parse the SSE byte stream into StreamEvent items
        let byte_stream = response.bytes_stream();

        let event_stream = byte_stream
            .map(|chunk_result| {
                chunk_result.context("Reading SSE chunk")
            })
            .flat_map(|chunk_result| {
                match chunk_result {
                    Err(e) => stream::iter(vec![Err(e)]),
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes).into_owned();
                        let events: Vec<Result<StreamEvent>> = text
                            .lines()
                            .filter_map(parse_sse_line)
                            .collect();
                        stream::iter(events)
                    }
                }
            });

        Ok(Box::pin(event_stream))
    }
}

/// Parse a single SSE text line into a StreamEvent.
/// Returns None for comment lines, empty lines, and `[DONE]`.
fn parse_sse_line(line: &str) -> Option<Result<StreamEvent>> {
    let data = line.strip_prefix("data: ")?;
    if data.trim() == "[DONE]" {
        return None;
    }
    Some(serde_json::from_str::<StreamEvent>(data).context("Parsing SSE event"))
}
