//! API layer — LLMProvider trait + provider implementations.
//! Mirrors Claude Code's services/api/ directory.

pub mod anthropic;
pub mod retry;
pub mod types;

use anyhow::Result;
use async_trait::async_trait;
use futures::stream::BoxStream;

use types::{ChatRequest, StreamEvent};

/// Pluggable LLM backend.
/// Implement this to add Anthropic, OpenAI, Ollama, etc.
#[async_trait]
pub trait LLMProvider: Send + Sync {
    /// Human-readable provider name (e.g. "anthropic")
    fn name(&self) -> &str;

    /// Stream a chat completion.
    /// Each yielded item is one parsed SSE event.
    async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Result<BoxStream<'static, Result<StreamEvent>>>;
}

/// Build a provider from the current [crate::config::Settings].
pub fn from_settings(settings: &crate::config::Settings) -> Result<Box<dyn LLMProvider>> {
    let api_key = settings
        .provider
        .api_key
        .clone()
        .ok_or_else(|| anyhow::anyhow!(
            "No API key configured. Set ANTHROPIC_API_KEY or add api_key in ~/.config/caishiji/config.toml"
        ))?;

    let mut provider = anthropic::AnthropicProvider::new(api_key);

    if let Some(base_url) = &settings.provider.base_url {
        provider = provider.with_base_url(base_url.clone());
    }

    Ok(Box::new(provider))
}
