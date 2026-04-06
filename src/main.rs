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

    // ── Launch REPL ──────────────────────────────────────────────────────
    tui::run_repl(app_state).await?;

    Ok(())
}
