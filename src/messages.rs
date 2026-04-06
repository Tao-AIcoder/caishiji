//! Message types — mirrors Claude Code's message model (types/, query.ts)
//!
//! Two layers:
//!   - `ApiMessage` / `ContentBlock` — what the LLM API speaks
//!   - `Message` — richer UI-level envelope used inside the REPL

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

// ─── API layer ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

/// A single block inside an API message `content` array.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

/// A message in the format the Anthropic Messages API expects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiMessage {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

// ─── Token usage ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u32>,
}

impl std::ops::AddAssign for Usage {
    fn add_assign(&mut self, rhs: Self) {
        self.input_tokens += rhs.input_tokens;
        self.output_tokens += rhs.output_tokens;
        self.cache_read_input_tokens = add_opt(self.cache_read_input_tokens, rhs.cache_read_input_tokens);
        self.cache_creation_input_tokens =
            add_opt(self.cache_creation_input_tokens, rhs.cache_creation_input_tokens);
    }
}

fn add_opt(a: Option<u32>, b: Option<u32>) -> Option<u32> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x + y),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    }
}

// ─── UI-level messages ────────────────────────────────────────────────────────

/// Rich message type used in the REPL / conversation history.
/// Mirrors Claude Code's `Message` union type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    User {
        uuid: String,
        timestamp: DateTime<Utc>,
        content: Vec<ContentBlock>,
    },
    Assistant {
        uuid: String,
        timestamp: DateTime<Utc>,
        content: Vec<ContentBlock>,
        usage: Usage,
        #[serde(skip_serializing_if = "Option::is_none")]
        api_error: Option<String>,
    },
    /// UI-only notice (not sent to API)
    System {
        uuid: String,
        timestamp: DateTime<Utc>,
        text: String,
    },
}

impl Message {
    pub fn user(content: Vec<ContentBlock>) -> Self {
        Self::User {
            uuid: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            content,
        }
    }

    pub fn user_text(text: impl Into<String>) -> Self {
        Self::user(vec![ContentBlock::Text { text: text.into() }])
    }

    pub fn assistant(content: Vec<ContentBlock>, usage: Usage) -> Self {
        Self::Assistant {
            uuid: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            content,
            usage,
            api_error: None,
        }
    }

    pub fn system(text: impl Into<String>) -> Self {
        Self::System {
            uuid: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            text: text.into(),
        }
    }

    pub fn uuid(&self) -> &str {
        match self {
            Message::User { uuid, .. }
            | Message::Assistant { uuid, .. }
            | Message::System { uuid, .. } => uuid,
        }
    }

    /// Convert to the API format; `System` messages are filtered out.
    pub fn to_api(&self) -> Option<ApiMessage> {
        match self {
            Message::User { content, .. } => Some(ApiMessage {
                role: Role::User,
                content: content.clone(),
            }),
            Message::Assistant { content, .. } => Some(ApiMessage {
                role: Role::Assistant,
                content: content.clone(),
            }),
            Message::System { .. } => None,
        }
    }

    /// Extract plain text from a User or System message.
    pub fn text_preview(&self, max_len: usize) -> String {
        let raw = match self {
            Message::User { content, .. } => content
                .iter()
                .filter_map(|b| {
                    if let ContentBlock::Text { text } = b {
                        Some(text.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(" "),
            Message::Assistant { content, .. } => content
                .iter()
                .filter_map(|b| {
                    if let ContentBlock::Text { text } = b {
                        Some(text.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(" "),
            Message::System { text, .. } => text.clone(),
        };
        if raw.len() > max_len {
            format!("{}…", &raw[..max_len])
        } else {
            raw
        }
    }
}
