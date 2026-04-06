//! CLI argument parsing.
//! Mirrors Claude Code's entrypoints/cli.tsx + main.tsx option handling.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "csj",
    about = "采石矶 — an agentic AI assistant",
    version,
    long_about = "采石矶 (Caishiji) is a Claude Code-style agentic AI assistant written in Rust.\n\
                  Configure your LLM provider in ~/.config/caishiji/config.toml\n\
                  or via environment variables (ANTHROPIC_API_KEY, CAISHIJI_MODEL, etc.)."
)]
pub struct Cli {
    /// Working directory (default: current directory)
    #[arg(short = 'C', long, value_name = "DIR")]
    pub dir: Option<PathBuf>,

    /// Model to use (overrides config)
    #[arg(short = 'm', long, env = "CAISHIJI_MODEL")]
    pub model: Option<String>,

    /// API key (overrides config and ANTHROPIC_API_KEY)
    #[arg(long, env = "CAISHIJI_API_KEY", hide_env_values = true)]
    pub api_key: Option<String>,

    /// Permission mode: default | auto | bypass
    #[arg(long, default_value = "default")]
    pub permission_mode: Option<String>,

    /// Show version information and exit
    #[arg(long)]
    pub version: bool,

    /// Print the system prompt and exit (debug)
    #[arg(long, hide = true)]
    pub dump_system_prompt: bool,

    /// Send a single message and print raw response (no TUI, for debugging)
    #[arg(long, value_name = "MESSAGE")]
    pub print: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Print current configuration
    Config,
    /// Show memory directory path
    Memory,
}
