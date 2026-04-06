//! 采石矶 (Caishiji) — agentic AI assistant.
//! Entry point — mirrors Claude Code's main.tsx.

mod api;
mod bootstrap;
mod cli;
mod config;
mod context;
mod memory;
mod messages;
mod permissions;
mod query;
mod state;
mod tools;
mod tui;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Commands};
use config::PermissionMode;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing (stderr, so it doesn't corrupt TUI)
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "caishiji=warn".to_string()),
        )
        .init();

    let cli = Cli::parse();

    // ── Fast paths (no TUI) ──────────────────────────────────────────────
    if cli.version {
        println!("csj {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    if let Some(Commands::Config) = &cli.command {
        let settings = config::load_settings()?;
        println!("{}", toml::to_string_pretty(&settings)?);
        return Ok(());
    }

    if let Some(Commands::Memory) = &cli.command {
        println!("{}", memory::memory_dir().display());
        return Ok(());
    }

    // ── Bootstrap ────────────────────────────────────────────────────────
    let mut app_state = bootstrap::initialize(cli.dir).await?;

    // Apply CLI overrides
    if let Some(model) = cli.model {
        app_state.model = model.clone();
        app_state.settings.provider.model = model;
    }
    if let Some(key) = cli.api_key {
        app_state.settings.provider.api_key = Some(key);
    }
    if let Some(mode_str) = cli.permission_mode {
        app_state.settings.permission_mode = match mode_str.as_str() {
            "bypass" => PermissionMode::Bypass,
            "auto" => PermissionMode::Auto,
            _ => PermissionMode::Default,
        };
    }

    if cli.dump_system_prompt {
        let prompt = context::build_system_prompt(
            &app_state.settings,
            &app_state.working_dir,
            &memory::memory_dir(),
        );
        println!("{prompt}");
        return Ok(());
    }

    // ── Debug: single-shot query, no TUI ────────────────────────────────
    if let Some(msg) = cli.print {
        return debug_query(app_state, msg).await;
    }

    // ── Launch REPL ──────────────────────────────────────────────────────
    tui::run_repl(app_state).await?;

    Ok(())
}

/// Non-TUI single-shot query for debugging.
/// Sends one message and prints every SSE event + final response to stdout.
async fn debug_query(app_state: state::AppState, msg: String) -> Result<()> {
    use api::types::{ChatRequest, StreamEvent};
    use messages::{ApiMessage, ContentBlock, Role};

    let provider = api::from_settings(&app_state.settings)?;

    let system_prompt = context::build_system_prompt(
        &app_state.settings,
        &app_state.working_dir,
        &memory::memory_dir(),
    );

    let request = ChatRequest {
        model: app_state.settings.provider.model.clone(),
        max_tokens: app_state.settings.provider.max_tokens,
        system: system_prompt,
        messages: vec![ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::Text { text: msg }],
        }],
        tools: vec![],  // no tools for debug
        stream: true,
    };

    let base = app_state.settings.provider.base_url.as_deref().unwrap_or("https://api.anthropic.com");
    let api_key = app_state.settings.provider.api_key.clone().unwrap_or_default();
    eprintln!("[debug] POST {}/v1/messages", base);
    eprintln!("[debug] model: {}", request.model);
    eprintln!("[debug] request body: {}", serde_json::to_string_pretty(&request).unwrap_or_default());

    // Raw HTTP call to see exact response bytes
    let client = reqwest::Client::new();
    let url = format!("{}/v1/messages", base);
    let resp = client
        .post(&url)
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("User-Agent", "claude-code/1.2.0")
        .json(&request)
        .send()
        .await?;

    eprintln!("[debug] status: {}", resp.status());
    eprintln!("[debug] headers: {:#?}", resp.headers());

    let body = resp.text().await?;
    eprintln!("[debug] raw body (first 2000 chars):\n{}", &body[..body.len().min(2000)]);

    Ok(())
}
