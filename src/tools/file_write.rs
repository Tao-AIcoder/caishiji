//! FileWriteTool — create or overwrite a file.
//! Mirrors Claude Code's tools/WriteFileTool/.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::permissions::Decision;
use super::{Tool, ToolContext, ToolResult};

pub struct FileWriteTool;

#[derive(Deserialize)]
struct WriteInput {
    file_path: String,
    content: String,
}

#[async_trait]
impl Tool for FileWriteTool {
    fn name(&self) -> &str {
        "write"
    }

    fn description(&self) -> &str {
        "Write content to a file, creating it if it does not exist \
         or overwriting it completely if it does. \
         Prefer Edit for modifying existing files — Write is best for new files \
         or full rewrites."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path", "content"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "Full content to write to the file"
                }
            }
        })
    }

    fn is_read_only(&self) -> bool {
        false
    }

    fn is_concurrent_safe(&self) -> bool {
        false
    }

    async fn call(
        &self,
        tool_use_id: String,
        input: Value,
        ctx: &ToolContext,
    ) -> ToolResult {
        let parsed: WriteInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return ToolResult::err(tool_use_id, format!("Invalid input: {e}")),
        };

        let path = resolve_path(&ctx.working_dir, &parsed.file_path);
        let summary = format!("write {}", path.display());

        match ctx.permissions.check("write", &summary, false) {
            Decision::Deny { reason } => {
                return ToolResult::err(tool_use_id, format!("Permission denied: {reason}"))
            }
            Decision::Ask { .. } => {
                // Handled by TUI before tool.call(); fall through
            }
            Decision::Allow => {}
        }

        // Create parent dirs if needed
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return ToolResult::err(
                    tool_use_id,
                    format!("Cannot create directory {}: {e}", parent.display()),
                );
            }
        }

        let existed = path.exists();
        match std::fs::write(&path, &parsed.content) {
            Ok(()) => {
                let verb = if existed { "Updated" } else { "Created" };
                ToolResult::ok(
                    tool_use_id,
                    format!("{} {}", verb, path.display()),
                )
            }
            Err(e) => ToolResult::err(tool_use_id, format!("Write failed: {e}")),
        }
    }
}

fn resolve_path(cwd: &PathBuf, path_str: &str) -> PathBuf {
    let p = PathBuf::from(path_str);
    if p.is_absolute() { p } else { cwd.join(p) }
}
