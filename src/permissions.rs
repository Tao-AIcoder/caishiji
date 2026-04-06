//! Permission system — mirrors Claude Code's useCanUseTool hook.
//!
//! Three outcomes for any tool call: Allow, Deny, Ask.
//! In Default mode: read-only tools are auto-allowed; destructive tools
//! prompt the user (or deny when non-interactive).

use crate::config::PermissionMode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Allow,
    Deny { reason: String },
    Ask { prompt: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    /// Glob pattern matched against "<tool_name>:<arg_summary>"
    pub pattern: String,
    pub action: RuleAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RuleAction {
    Allow,
    Deny,
    Ask,
}

pub struct PermissionChecker {
    mode: PermissionMode,
    always_allow: Vec<Rule>,
    always_deny: Vec<Rule>,
}

impl PermissionChecker {
    pub fn new(mode: PermissionMode) -> Self {
        Self {
            mode,
            always_allow: Vec::new(),
            always_deny: Vec::new(),
        }
    }

    pub fn add_allow(&mut self, pattern: impl Into<String>) {
        self.always_allow.push(Rule {
            pattern: pattern.into(),
            action: RuleAction::Allow,
        });
    }

    pub fn add_deny(&mut self, pattern: impl Into<String>) {
        self.always_deny.push(Rule {
            pattern: pattern.into(),
            action: RuleAction::Deny,
        });
    }

    /// Decide whether a tool call is permitted.
    ///
    /// `tool_name` — e.g. "bash"
    /// `summary`   — short human-readable description of what it will do
    /// `read_only` — tool self-declares it only reads
    pub fn check(&self, tool_name: &str, summary: &str, read_only: bool) -> Decision {
        let key = format!("{}:{}", tool_name, summary);

        // Static deny rules always win
        for rule in &self.always_deny {
            if glob_match(&rule.pattern, &key) {
                return Decision::Deny {
                    reason: format!("Matched deny rule: {}", rule.pattern),
                };
            }
        }

        // Static allow rules
        for rule in &self.always_allow {
            if glob_match(&rule.pattern, &key) {
                return Decision::Allow;
            }
        }

        match &self.mode {
            PermissionMode::Bypass => Decision::Allow,
            PermissionMode::Auto if read_only => Decision::Allow,
            PermissionMode::Auto => Decision::Ask {
                prompt: format!("Allow {} to run `{}`?", tool_name, summary),
            },
            PermissionMode::Default if read_only => Decision::Allow,
            PermissionMode::Default => Decision::Ask {
                prompt: format!("Allow {} to run `{}`?", tool_name, summary),
            },
        }
    }
}

/// Minimal glob matching (only * wildcard supported).
fn glob_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return pattern == text;
    }
    let parts: Vec<&str> = pattern.splitn(2, '*').collect();
    if parts.len() != 2 {
        return pattern == text;
    }
    text.starts_with(parts[0]) && text.ends_with(parts[1])
}
