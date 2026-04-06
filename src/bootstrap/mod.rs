//! Bootstrap — initialization sequence.
//! Mirrors Claude Code's bootstrap/ and entrypoints/init.ts.

use anyhow::Result;
use std::path::PathBuf;

use crate::{
    config::load_settings,
    memory::{ensure_memory_dir, memory_dir},
    state::AppState,
};

/// Full initialization. Returns a ready-to-use AppState.
pub async fn initialize(working_dir: Option<PathBuf>) -> Result<AppState> {
    // 1. Resolve working directory
    let cwd = working_dir.unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    });

    // 2. Load settings (file + env vars)
    let settings = load_settings()?;

    // 3. Ensure memory directory exists
    let mem_dir = memory_dir();
    ensure_memory_dir(&mem_dir)?;

    // 4. Validate API key is present
    if settings.provider.api_key.is_none() {
        return Err(anyhow::anyhow!(
            "No API key found.\n\
             Set the ANTHROPIC_API_KEY environment variable, or add:\n\
             \n\
             [provider]\n\
             api_key = \"your-key\"\n\
             \n\
             to ~/.config/caishiji/config.toml"
        ));
    }

    // 5. Build app state
    let state = AppState::new(settings, cwd);

    Ok(state)
}
