//! GrepTool — regex content search across files.
//! Mirrors Claude Code's tools/GrepTool/.

use async_trait::async_trait;
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use walkdir::WalkDir;

use super::{Tool, ToolContext, ToolResult};

pub struct GrepTool;

#[derive(Deserialize)]
struct GrepInput {
    pattern: String,
    /// Directory or file to search
    #[serde(default)]
    path: Option<String>,
    /// Glob to filter files (e.g. "*.rs")
    #[serde(default)]
    glob: Option<String>,
    /// Lines of context before match
    #[serde(default)]
    context_before: Option<usize>,
    /// Lines of context after match
    #[serde(default)]
    context_after: Option<usize>,
    /// Case-insensitive
    #[serde(default)]
    case_insensitive: bool,
}

const MAX_MATCHES: usize = 500;

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Search file contents using a regular expression. \
         Returns matching lines with file paths and line numbers. \
         Use glob to restrict to specific file types."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["pattern"],
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regular expression to search for"
                },
                "path": {
                    "type": "string",
                    "description": "Directory or file to search (default: cwd)"
                },
                "glob": {
                    "type": "string",
                    "description": "Glob filter for file names (e.g. *.rs)"
                },
                "context_before": {
                    "type": "integer",
                    "description": "Lines of context before each match"
                },
                "context_after": {
                    "type": "integer",
                    "description": "Lines of context after each match"
                },
                "case_insensitive": {
                    "type": "boolean",
                    "description": "Case-insensitive search (default false)"
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
        let parsed: GrepInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return ToolResult::err(tool_use_id, format!("Invalid input: {e}")),
        };

        let pattern_str = if parsed.case_insensitive {
            format!("(?i){}", parsed.pattern)
        } else {
            parsed.pattern.clone()
        };

        let re = match Regex::new(&pattern_str) {
            Ok(r) => r,
            Err(e) => return ToolResult::err(tool_use_id, format!("Invalid regex: {e}")),
        };

        let base = parsed
            .path
            .as_deref()
            .map(|p| resolve_path(&ctx.working_dir, p))
            .unwrap_or_else(|| ctx.working_dir.clone());

        let glob_filter = parsed.glob.as_deref().map(|g| {
            // Build full glob from just filename pattern
            glob::Pattern::new(g).ok()
        }).flatten();

        let ctx_before = parsed.context_before.unwrap_or(0);
        let ctx_after = parsed.context_after.unwrap_or(0);

        let mut all_matches: Vec<String> = Vec::new();
        let mut total_matches = 0usize;
        let mut truncated = false;

        let walker = if base.is_file() {
            WalkDir::new(&base).max_depth(1)
        } else {
            WalkDir::new(&base)
        };

        'outer: for entry in walker
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            // Apply glob filter to file name
            if let Some(ref gp) = glob_filter {
                let fname = entry.file_name().to_string_lossy();
                if !gp.matches(&fname) {
                    continue;
                }
            }

            // Skip binary-ish files by extension
            if is_likely_binary(entry.path()) {
                continue;
            }

            let content = match std::fs::read_to_string(entry.path()) {
                Ok(s) => s,
                Err(_) => continue,
            };

            let lines: Vec<&str> = content.lines().collect();

            for (i, line) in lines.iter().enumerate() {
                if re.is_match(line) {
                    if total_matches >= MAX_MATCHES {
                        truncated = true;
                        break 'outer;
                    }
                    total_matches += 1;

                    let start = i.saturating_sub(ctx_before);
                    let end = (i + ctx_after + 1).min(lines.len());

                    for j in start..end {
                        let marker = if j == i { ">" } else { " " };
                        all_matches.push(format!(
                            "{}:{}:{} {}",
                            entry.path().display(),
                            j + 1,
                            marker,
                            lines[j]
                        ));
                    }
                    if ctx_before > 0 || ctx_after > 0 {
                        all_matches.push("--".to_string());
                    }
                }
            }
        }

        if truncated {
            all_matches.push(format!("\n[Results truncated at {MAX_MATCHES} matches]"));
        }

        if all_matches.is_empty() {
            ToolResult::ok(tool_use_id, "No matches found.".to_string())
        } else {
            ToolResult::ok(tool_use_id, all_matches.join("\n"))
        }
    }
}

fn is_likely_binary(path: &std::path::Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some(
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "ico" | "webp"
            | "pdf" | "zip" | "tar" | "gz" | "bz2" | "xz"
            | "exe" | "dll" | "so" | "dylib"
            | "wasm" | "bin" | "o" | "a"
            | "mp3" | "mp4" | "mov" | "avi"
            | "ttf" | "otf" | "woff" | "woff2"
        )
    )
}

fn resolve_path(cwd: &PathBuf, path_str: &str) -> PathBuf {
    let p = PathBuf::from(path_str);
    if p.is_absolute() { p } else { cwd.join(p) }
}
