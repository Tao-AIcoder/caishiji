//! TUI layer — ratatui-based REPL.
//! Mirrors Claude Code's screens/REPL.tsx + ink/ directory.

pub mod input;
pub mod renderer;
pub mod repl;

pub use repl::run_repl;
