//! Configuration — mirrors Claude Code's settings system (utils/settings/).
//!
//! Loaded from (in priority order):
//!   1. CLI flags
//!   2. ~/.config/caishiji/config.toml
//!   3. Built-in defaults

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ─── Permission mode ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    /// Ask before running potentially-destructive tools
    #[default]
    Default,
    /// Auto-approve all tools (dangerous — for scripting)
    Bypass,
    /// Auto-approve read-only tools; ask for write/exec
    Auto,
}

// ─── Provider config ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    #[serde(default = "default_provider_name")]
    pub name: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

fn default_provider_name() -> String { "anthropic".to_string() }
fn default_model() -> String { "claude-sonnet-4-6".to_string() }
fn default_max_tokens() -> u32 { 8192 }

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            name: "anthropic".to_string(),
            base_url: None,
            api_key: None,
            model: "claude-sonnet-4-6".to_string(),
            max_tokens: 8192,
        }
    }
}

// ─── Main settings ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub provider: ProviderConfig,
    #[serde(default)]
    pub permission_mode: PermissionMode,
    #[serde(default)]
    pub allowed_dirs: Vec<PathBuf>,
    #[serde(default = "default_show_cost")]
    pub show_cost: bool,
    #[serde(default = "default_shell")]
    pub shell: String,
}

fn default_show_cost() -> bool { true }

impl Default for Settings {
    fn default() -> Self {
        Self {
            provider: ProviderConfig::default(),
            permission_mode: PermissionMode::Default,
            allowed_dirs: Vec::new(),
            show_cost: true,
            shell: default_shell(),
        }
    }
}

fn default_shell() -> String {
    if cfg!(target_os = "windows") {
        "powershell".to_string()
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
    }
}

// ─── Load / save ─────────────────────────────────────────────────────────────

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("caishiji")
        .join("config.toml")
}

pub fn load_settings() -> Result<Settings> {
    let path = config_path();

    // Env-var overlay runs after file load
    let mut settings = if path.exists() {
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("Reading config file: {}", path.display()))?;
        toml::from_str::<Settings>(&raw)
            .with_context(|| format!("Parsing config file: {}", path.display()))?
    } else {
        Settings::default()
    };

    // Env overrides
    if let Ok(key) = std::env::var("CAISHIJI_API_KEY")
        .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
    {
        settings.provider.api_key = Some(key);
    }
    if let Ok(model) = std::env::var("CAISHIJI_MODEL") {
        settings.provider.model = model;
    }
    if let Ok(url) = std::env::var("CAISHIJI_BASE_URL") {
        settings.provider.base_url = Some(url);
    }

    Ok(settings)
}

pub fn save_settings(settings: &Settings) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let toml_str = toml::to_string_pretty(settings)?;
    std::fs::write(&path, toml_str)?;
    Ok(())
}
