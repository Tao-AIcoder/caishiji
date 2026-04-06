//! Memory system — persistent context across sessions.
//! Mirrors Claude Code's memdir/ module.
//!
//! Layout:
//!   ~/.local/share/caishiji/memory/
//!   ├── MEMORY.md          # Index (max 200 lines / 25 KB)
//!   ├── user_role.md       # type: user
//!   ├── feedback_xxx.md    # type: feedback
//!   ├── project_xxx.md     # type: project
//!   └── reference_xxx.md   # type: reference

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const ENTRYPOINT_NAME: &str = "MEMORY.md";
pub const MAX_ENTRYPOINT_LINES: usize = 200;
pub const MAX_ENTRYPOINT_BYTES: usize = 25_000;

#[derive(Debug, Clone, PartialEq)]
pub enum MemoryType {
    User,
    Feedback,
    Project,
    Reference,
}

impl MemoryType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "user" => Some(Self::User),
            "feedback" => Some(Self::Feedback),
            "project" => Some(Self::Project),
            "reference" => Some(Self::Reference),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemoryFile {
    pub path: PathBuf,
    pub name: String,
    pub description: String,
    pub mem_type: Option<MemoryType>,
    pub body: String,
}

/// Root directory for this session's memory files.
pub fn memory_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("caishiji")
        .join("memory")
}

/// Load and truncate the MEMORY.md index, ready for injection.
pub fn load_memory_index(dir: &Path) -> Result<Option<String>> {
    let index_path = dir.join(ENTRYPOINT_NAME);
    if !index_path.exists() {
        return Ok(None);
    }

    let raw = std::fs::read_to_string(&index_path)
        .with_context(|| format!("Reading {}", index_path.display()))?;

    Ok(Some(truncate_index(&raw)))
}

/// Truncate to MAX_ENTRYPOINT_LINES lines and MAX_ENTRYPOINT_BYTES bytes.
pub fn truncate_index(raw: &str) -> String {
    let trimmed = raw.trim();
    let lines: Vec<&str> = trimmed.lines().collect();

    let line_capped: String = if lines.len() > MAX_ENTRYPOINT_LINES {
        let mut s = lines[..MAX_ENTRYPOINT_LINES].join("\n");
        s.push_str("\n\n[MEMORY.md truncated at line limit]");
        s
    } else {
        trimmed.to_string()
    };

    if line_capped.len() > MAX_ENTRYPOINT_BYTES {
        // Byte-truncate at last newline before the cap
        let cap = MAX_ENTRYPOINT_BYTES;
        let truncation_point = line_capped[..cap]
            .rfind('\n')
            .unwrap_or(cap);
        let mut s = line_capped[..truncation_point].to_string();
        s.push_str("\n\n[MEMORY.md truncated at byte limit]");
        s
    } else {
        line_capped
    }
}

/// Parse a memory file's YAML-style frontmatter.
///
/// Expected format:
/// ```
/// ---
/// name: ...
/// description: ...
/// type: user|feedback|project|reference
/// ---
///
/// body content
/// ```
pub fn parse_memory_file(path: &Path) -> Result<Option<MemoryFile>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Reading memory file: {}", path.display()))?;

    let Some(after_open) = content.strip_prefix("---\n") else {
        return Ok(None); // no frontmatter
    };

    let Some(close_pos) = after_open.find("\n---") else {
        return Ok(None);
    };

    let frontmatter = &after_open[..close_pos];
    let body = after_open[close_pos + 4..].trim().to_string();

    let mut name = String::new();
    let mut description = String::new();
    let mut mem_type: Option<MemoryType> = None;

    for line in frontmatter.lines() {
        if let Some(v) = line.strip_prefix("name:") {
            name = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("description:") {
            description = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("type:") {
            mem_type = MemoryType::from_str(v.trim());
        }
    }

    Ok(Some(MemoryFile {
        path: path.to_path_buf(),
        name,
        description,
        mem_type,
        body,
    }))
}

/// Ensure the memory directory exists.
pub fn ensure_memory_dir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("Creating memory dir: {}", dir.display()))
}

/// Write (or overwrite) a memory file.
pub fn write_memory_file(dir: &Path, filename: &str, content: &str) -> Result<PathBuf> {
    ensure_memory_dir(dir)?;
    let path = dir.join(filename);
    std::fs::write(&path, content)
        .with_context(|| format!("Writing memory file: {}", path.display()))?;
    Ok(path)
}

/// Build the memory section injected into the system prompt.
pub fn build_memory_prompt(dir: &Path) -> String {
    match load_memory_index(dir) {
        Ok(Some(index)) => format!(
            "\n\n# Your persistent memory\n\
             The following is your auto-memory index (MEMORY.md).\n\
             Use the read tool to load individual memory files when relevant.\n\n{index}"
        ),
        Ok(None) => String::new(),
        Err(e) => {
            tracing::warn!("Failed to load memory index: {e}");
            String::new()
        }
    }
}
