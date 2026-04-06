//! Application state — single source of truth for a session.
//! Mirrors Claude Code's state/AppStateStore.ts.

use std::path::PathBuf;

use crate::{
    config::Settings,
    messages::{Message, Usage},
};

/// The mutable state of one REPL session.
#[derive(Debug)]
pub struct AppState {
    /// Conversation history (UI-level messages)
    pub messages: Vec<Message>,
    /// Current working directory
    pub working_dir: PathBuf,
    /// Accumulated token usage for this session
    pub session_usage: Usage,
    /// Estimated session cost in USD
    pub session_cost_usd: f64,
    /// Whether the agent is currently processing
    pub is_loading: bool,
    /// Most recent error message (shown in status bar)
    pub last_error: Option<String>,
    /// The model being used
    pub model: String,
    /// Session settings (can be mutated during session)
    pub settings: Settings,
}

impl AppState {
    pub fn new(settings: Settings, working_dir: PathBuf) -> Self {
        let model = settings.provider.model.clone();
        Self {
            messages: Vec::new(),
            working_dir,
            session_usage: Usage::default(),
            session_cost_usd: 0.0,
            is_loading: false,
            last_error: None,
            model,
            settings,
        }
    }

    pub fn push_message(&mut self, msg: Message) {
        self.messages.push(msg);
    }

    pub fn add_usage(&mut self, usage: &Usage) {
        let cost = estimate_cost(
            &self.model,
            usage.input_tokens,
            usage.output_tokens,
            usage.cache_read_input_tokens.unwrap_or(0),
            usage.cache_creation_input_tokens.unwrap_or(0),
        );
        self.session_cost_usd += cost;
        self.session_usage += usage.clone();
    }

    /// Messages formatted for the API (strips System messages).
    pub fn api_messages(&self) -> Vec<crate::messages::ApiMessage> {
        self.messages
            .iter()
            .filter_map(|m| m.to_api())
            .collect()
    }
}

// ─── Cost estimation (USD) ────────────────────────────────────────────────────
// Rates as of 2025 — update when Anthropic changes pricing.

fn estimate_cost(
    model: &str,
    input: u32,
    output: u32,
    cache_read: u32,
    cache_write: u32,
) -> f64 {
    let (in_rate, out_rate, cr_rate, cw_rate) = model_rates(model);
    (input as f64 / 1_000_000.0) * in_rate
        + (output as f64 / 1_000_000.0) * out_rate
        + (cache_read as f64 / 1_000_000.0) * cr_rate
        + (cache_write as f64 / 1_000_000.0) * cw_rate
}

/// Returns (input, output, cache_read, cache_write) rates per million tokens.
fn model_rates(model: &str) -> (f64, f64, f64, f64) {
    if model.contains("opus") {
        (15.0, 75.0, 1.5, 18.75)
    } else if model.contains("sonnet") {
        (3.0, 15.0, 0.3, 3.75)
    } else if model.contains("haiku") {
        (0.8, 4.0, 0.08, 1.0)
    } else {
        // Default: sonnet pricing
        (3.0, 15.0, 0.3, 3.75)
    }
}
