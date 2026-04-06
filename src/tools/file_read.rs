//! FileReadTool — read file contents with optional line range.
//! Mirrors Claude Code's tools/ReadTool/.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;

use super::{Tool, ToolContext, ToolResult};

pub struct FileReadTool;

#[derive(Deserialize)]
struct ReadInput {
    file_path: String,
    /// 1-based start line (inclusive)
    #[serde(default)]
    offset: Option<usize>,
    /// Max lines to return
    #[serde(default)]
    limit: Option<usize>,
}

const DEFAULT_LIMIT: usize = 2000;
const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024; // 10 MB

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "read"
    }

    fn description(&self) -> &str {
        "Read a file from the local filesystem. \
         Returns file contents with line numbers. \
         Use offset and limit for large files."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to read"
                },
                "offset": {
                    "type": "integer",
                    "description": "1-based line number to start reading from"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to return (default 2000)"
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
        let parsed: ReadInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return ToolResult::err(tool_use_id, format!("Invalid input: {e}")),
        };

        let path = resolve_path(&ctx.working_dir, &parsed.file_path);

        match std::fs::metadata(&path) {
            Err(e) => return ToolResult::err(tool_use_id, format!("Cannot access {}: {e}", path.display())),
            Ok(meta) if meta.is_dir() => {
                return ToolResult::err(
                    tool_use_id,
                    format!("{} is a directory, not a file", path.display()),
                )
            }
            Ok(meta) if meta.len() > MAX_FILE_BYTES => {
                return ToolResult::err(
                    tool_use_id,
                    format!(
                        "File too large ({} MB). Use offset/limit to read portions.",
                        meta.len() / 1024 / 1024
                    ),
                )
            }
            Ok(_) => {}
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => return ToolResult::err(tool_use_id, format!("Read error: {e}")),
        };

        let limit = parsed.limit.unwrap_or(DEFAULT_LIMIT);
        let offset = parsed.offset.unwrap_or(1).saturating_sub(1); // convert to 0-based

        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();
        let end = (offset + limit).min(total);

        let numbered: Vec<String> = lines[offset..end]
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{}\t{}", offset + i + 1, line))
            .collect();

        let mut result = numbered.join("\n");

        if end < total {
            result.push_str(&format!(
                "\n\n[Showing lines {}-{} of {}. Use offset={} to continue.]",
                offset + 1,
                end,
                total,
                end + 1
            ));
        }

        ToolResult::ok(tool_use_id, result)
    }
}

fn resolve_path(cwd: &PathBuf, path_str: &str) -> PathBuf {
    let p = PathBuf::from(path_str);
    if p.is_absolute() {
        p
    } else {
        cwd.join(p)
    }
}
