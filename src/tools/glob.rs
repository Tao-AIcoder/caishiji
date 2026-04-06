//! GlobTool — fast file-pattern matching.
//! Mirrors Claude Code's tools/GlobTool/.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use walkdir::WalkDir;

use super::{Tool, ToolContext, ToolResult};

pub struct GlobTool;

#[derive(Deserialize)]
struct GlobInput {
    pattern: String,
    /// Directory to search (default: cwd)
    #[serde(default)]
    path: Option<String>,
}

const MAX_RESULTS: usize = 1000;

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Fast file-pattern matching. Supports glob patterns like **/*.rs or src/**/*.ts. \
         Returns matching file paths sorted by modification time."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["pattern"],
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern, e.g. **/*.rs or src/**/*.ts"
                },
                "path": {
                    "type": "string",
                    "description": "Directory to search (default: current working directory)"
                }
            }
        })
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrent_safe(&self) -> bool {
        true
    }

    async fn call(
        &self,
        tool_use_id: String,
        input: Value,
        ctx: &ToolContext,
    ) -> ToolResult {
        let parsed: GlobInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return ToolResult::err(tool_use_id, format!("Invalid input: {e}")),
        };

        let base = parsed
            .path
            .map(|p| resolve_path(&ctx.working_dir, &p))
            .unwrap_or_else(|| ctx.working_dir.clone());

        // Build glob from base + pattern
        let full_pattern = base.join(&parsed.pattern).to_string_lossy().to_string();

        let glob_pat = match glob::Pattern::new(&full_pattern) {
            Ok(p) => p,
            Err(e) => return ToolResult::err(tool_use_id, format!("Invalid glob pattern: {e}")),
        };

        let mut matches: Vec<(std::time::SystemTime, PathBuf)> = WalkDir::new(&base)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| glob_pat.matches_path(e.path()))
            .filter_map(|e| {
                let mtime = e.metadata().ok()?.modified().ok()?;
                Some((mtime, e.into_path()))
            })
            .take(MAX_RESULTS + 1)
            .collect();

        let truncated = matches.len() > MAX_RESULTS;
        matches.truncate(MAX_RESULTS);

        // Sort newest first
        matches.sort_by(|a, b| b.0.cmp(&a.0));

        let mut lines: Vec<String> = matches
            .iter()
            .map(|(_, p)| p.to_string_lossy().to_string())
            .collect();

        if truncated {
            lines.push(format!("\n[Results truncated at {MAX_RESULTS}]"));
        }

        if lines.is_empty() {
            ToolResult::ok(tool_use_id, "No files matched.".to_string())
        } else {
            ToolResult::ok(tool_use_id, lines.join("\n"))
        }
    }
}

fn resolve_path(cwd: &PathBuf, path_str: &str) -> PathBuf {
    let p = PathBuf::from(path_str);
    if p.is_absolute() { p } else { cwd.join(p) }
}
