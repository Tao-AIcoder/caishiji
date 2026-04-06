//! Tool layer — Tool trait, ToolRegistry, and all built-in tool implementations.
//! Mirrors Claude Code's Tool.ts + tools/ directory.

pub mod bash;
pub mod executor;
pub mod file_edit;
pub mod file_read;
pub mod file_write;
pub mod glob;
pub mod grep;

use async_trait::async_trait;
use serde_json::Value;
use std::{path::PathBuf, sync::Arc};

use crate::{api::types::ToolDefinition, permissions::PermissionChecker};

// ─── Tool result ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub tool_use_id: String,
    pub content: String,
    pub is_error: bool,
}

impl ToolResult {
    pub fn ok(tool_use_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            tool_use_id: tool_use_id.into(),
            content: content.into(),
            is_error: false,
        }
    }

    pub fn err(tool_use_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            tool_use_id: tool_use_id.into(),
            content: message.into(),
            is_error: true,
        }
    }
}

// ─── Tool execution context ───────────────────────────────────────────────────

#[derive(Clone)]
pub struct ToolContext {
    pub working_dir: PathBuf,
    pub permissions: Arc<PermissionChecker>,
    /// Shell path used by BashTool
    pub shell: String,
}

// ─── Tool trait ───────────────────────────────────────────────────────────────

#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique tool name (snake_case)
    fn name(&self) -> &str;

    /// Human-readable description injected into the system prompt
    fn description(&self) -> &str;

    /// JSON Schema for the tool's `input` object
    fn input_schema(&self) -> Value;

    /// Whether this tool only reads — auto-allows in non-bypass modes
    fn is_read_only(&self) -> bool {
        false
    }

    /// Whether two calls of this tool can safely run in parallel
    fn is_concurrent_safe(&self) -> bool {
        false
    }

    /// Execute the tool.
    async fn call(
        &self,
        tool_use_id: String,
        input: Value,
        ctx: &ToolContext,
    ) -> ToolResult;
}

pub type DynTool = Arc<dyn Tool>;

// ─── Tool registry ────────────────────────────────────────────────────────────

pub struct ToolRegistry {
    tools: Vec<DynTool>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    pub fn register(&mut self, tool: impl Tool + 'static) {
        self.tools.push(Arc::new(tool));
    }

    pub fn get(&self, name: &str) -> Option<&DynTool> {
        self.tools.iter().find(|t| t.name() == name)
    }

    pub fn all(&self) -> &[DynTool] {
        &self.tools
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .iter()
            .map(|t| ToolDefinition {
                name: t.name().to_string(),
                description: t.description().to_string(),
                input_schema: t.input_schema(),
            })
            .collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Build the default registry with all v1 built-in tools.
pub fn default_registry() -> ToolRegistry {
    let mut r = ToolRegistry::new();
    r.register(bash::BashTool);
    r.register(file_read::FileReadTool);
    r.register(file_write::FileWriteTool);
    r.register(file_edit::FileEditTool);
    r.register(glob::GlobTool);
    r.register(grep::GrepTool);
    r
}
