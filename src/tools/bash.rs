//! BashTool — runs shell commands.
//! Mirrors Claude Code's tools/BashTool/.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

use crate::permissions::Decision;
use super::{Tool, ToolContext, ToolResult};

pub struct BashTool;

#[derive(Deserialize)]
struct BashInput {
    command: String,
    #[serde(default)]
    description: Option<String>,
    /// Timeout in milliseconds (default 120 000)
    #[serde(default)]
    timeout_ms: Option<u64>,
}

const DEFAULT_TIMEOUT_MS: u64 = 120_000;
const MAX_OUTPUT_BYTES: usize = 100_000;

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute a shell command and return its stdout/stderr. \
         Use for running tests, building projects, file operations that need a shell, \
         package management, git commands, etc. \
         Prefer specific file tools (Read, Write, Edit, Glob, Grep) when applicable."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to execute"
                },
                "description": {
                    "type": "string",
                    "description": "Short description of what the command does (shown in permission prompt)"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Timeout in milliseconds (default 120000)"
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
        let parsed: BashInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return ToolResult::err(tool_use_id, format!("Invalid input: {e}")),
        };

        let summary = parsed
            .description
            .as_deref()
            .unwrap_or(&parsed.command);

        // Permission check
        match ctx.permissions.check("bash", summary, false) {
            Decision::Deny { reason } => {
                return ToolResult::err(
                    tool_use_id,
                    format!("Permission denied: {reason}"),
                );
            }
            Decision::Ask { prompt: _ } => {
                // Non-interactive path: deny by default
                // The TUI layer intercepts Ask decisions before calling tool.call()
            }
            Decision::Allow => {}
        }

        let timeout_dur =
            Duration::from_millis(parsed.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS));

        let output_result = timeout(timeout_dur, async {
            Command::new(&ctx.shell)
                .arg("-c")
                .arg(&parsed.command)
                .current_dir(&ctx.working_dir)
                .output()
                .await
        })
        .await;

        match output_result {
            Err(_) => ToolResult::err(
                tool_use_id,
                format!(
                    "Command timed out after {}ms",
                    parsed.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS)
                ),
            ),
            Ok(Err(e)) => ToolResult::err(tool_use_id, format!("Failed to execute command: {e}")),
            Ok(Ok(output)) => {
                let mut combined = String::new();

                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if !stdout.is_empty() {
                    combined.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !combined.is_empty() {
                        combined.push('\n');
                    }
                    combined.push_str("STDERR:\n");
                    combined.push_str(&stderr);
                }

                // Truncate very large outputs
                if combined.len() > MAX_OUTPUT_BYTES {
                    combined.truncate(MAX_OUTPUT_BYTES);
                    combined.push_str("\n\n[Output truncated]");
                }

                let is_error = !output.status.success();

                if is_error && combined.is_empty() {
                    combined = format!(
                        "Command exited with status {}",
                        output.status.code().unwrap_or(-1)
                    );
                }

                ToolResult {
                    tool_use_id,
                    content: combined,
                    is_error,
                }
            }
        }
    }
}
