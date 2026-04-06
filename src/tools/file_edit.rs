//! FileEditTool — exact string replacement in a file.
//! Mirrors Claude Code's tools/EditTool/.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::permissions::Decision;
use super::{Tool, ToolContext, ToolResult};

pub struct FileEditTool;

#[derive(Deserialize)]
struct EditInput {
    file_path: String,
    old_string: String,
    new_string: String,
    #[serde(default)]
    replace_all: bool,
}

#[async_trait]
impl Tool for FileEditTool {
    fn name(&self) -> &str {
        "edit"
    }

    fn description(&self) -> &str {
        "Perform exact string replacement in a file. \
         `old_string` must appear exactly once (unless replace_all=true). \
         Use Write for creating new files. Use Read first to confirm the exact text."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path", "old_string", "new_string"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to edit"
                },
                "old_string": {
                    "type": "string",
                    "description": "Exact text to find and replace"
                },
                "new_string": {
                    "type": "string",
                    "description": "Replacement text"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "Replace all occurrences (default false — fails if >1 match)"
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
        let parsed: EditInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return ToolResult::err(tool_use_id, format!("Invalid input: {e}")),
        };

        if parsed.old_string == parsed.new_string {
            return ToolResult::err(tool_use_id, "old_string and new_string are identical — nothing to change".to_string());
        }

        let path = resolve_path(&ctx.working_dir, &parsed.file_path);
        let summary = format!("edit {}", path.display());

        match ctx.permissions.check("edit", &summary, false) {
            Decision::Deny { reason } => {
                return ToolResult::err(tool_use_id, format!("Permission denied: {reason}"))
            }
            _ => {}
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => return ToolResult::err(tool_use_id, format!("Cannot read {}: {e}", path.display())),
        };

        let occurrences = content.matches(&parsed.old_string).count();

        if occurrences == 0 {
            return ToolResult::err(
                tool_use_id,
                format!(
                    "old_string not found in {}. \
                     Use Read first to confirm the exact text.",
                    path.display()
                ),
            );
        }

        if occurrences > 1 && !parsed.replace_all {
            return ToolResult::err(
                tool_use_id,
                format!(
                    "old_string appears {} times in {}. \
                     Use replace_all=true to replace all, or provide more context to make it unique.",
                    occurrences,
                    path.display()
                ),
            );
        }

        let new_content = if parsed.replace_all {
            content.replace(&parsed.old_string, &parsed.new_string)
        } else {
            content.replacen(&parsed.old_string, &parsed.new_string, 1)
        };

        match std::fs::write(&path, &new_content) {
            Ok(()) => ToolResult::ok(
                tool_use_id,
                format!(
                    "Replaced {} occurrence(s) in {}",
                    occurrences,
                    path.display()
                ),
            ),
            Err(e) => ToolResult::err(tool_use_id, format!("Write failed: {e}")),
        }
    }
}

fn resolve_path(cwd: &PathBuf, path_str: &str) -> PathBuf {
    let p = PathBuf::from(path_str);
    if p.is_absolute() { p } else { cwd.join(p) }
}
