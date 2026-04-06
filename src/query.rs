//! Agentic loop — the heart of 采石矶.
//! Mirrors Claude Code's query.ts.
//!
//! One call to `run_query` runs a complete agentic turn:
//!   1. Assemble messages + system prompt
//!   2. Call LLM (streaming)
//!   3. Collect assistant response blocks
//!   4. If stop_reason == tool_use → execute tools, inject results, loop
//!   5. Return final assistant message + accumulated usage

use anyhow::Result;
use futures::StreamExt;
use crate::{
    api::{
        types::{ChatRequest, ContentBlockDelta, ContentBlockStart, StopReason, StreamEvent},
        LLMProvider,
    },
    messages::{ApiMessage, ContentBlock, Message, Role, Usage},
    tools::{
        executor::{results_to_content, run_tool_calls, ToolCall},
        ToolContext, ToolRegistry,
    },
};

/// Callback for streaming partial text to the UI.
pub type TextStreamCallback = Box<dyn Fn(&str) + Send + Sync>;

/// Callback for notifying the UI that a tool call started.
pub type ToolStartCallback = Box<dyn Fn(&str, &str) + Send + Sync>; // (tool_name, id)

/// Callback for notifying the UI that a tool call finished.
pub type ToolDoneCallback = Box<dyn Fn(&str, bool) + Send + Sync>; // (id, is_error)

pub struct QueryCallbacks {
    pub on_text: Option<TextStreamCallback>,
    pub on_tool_start: Option<ToolStartCallback>,
    pub on_tool_done: Option<ToolDoneCallback>,
}

impl Default for QueryCallbacks {
    fn default() -> Self {
        Self {
            on_text: None,
            on_tool_start: None,
            on_tool_done: None,
        }
    }
}

/// Parameters for a single agentic query.
pub struct QueryParams<'a> {
    pub history: &'a [ApiMessage],
    pub new_user_content: Vec<ContentBlock>,
    pub system_prompt: String,
    pub model: String,
    pub max_tokens: u32,
    pub provider: &'a dyn LLMProvider,
    pub tool_registry: &'a ToolRegistry,
    pub tool_ctx: &'a ToolContext,
    pub callbacks: QueryCallbacks,
    /// Maximum number of agentic loop iterations (safety limit)
    pub max_iterations: usize,
}

/// Result of one complete agentic turn.
pub struct QueryResult {
    /// All messages appended during this turn (assistant + tool results)
    pub new_messages: Vec<Message>,
    /// Total token usage across all loop iterations
    pub usage: Usage,
}

/// Run the agentic loop for a single user turn.
///
/// This is the core of 采石矶 — equivalent to Claude Code's `query()`.
pub async fn run_query(params: QueryParams<'_>) -> Result<QueryResult> {
    let mut accumulated_messages: Vec<ApiMessage> = params.history.to_vec();
    accumulated_messages.push(ApiMessage {
        role: Role::User,
        content: params.new_user_content,
    });

    let mut new_messages: Vec<Message> = Vec::new();
    let mut total_usage = Usage::default();
    let mut iterations = 0;

    loop {
        iterations += 1;
        if iterations > params.max_iterations {
            tracing::warn!("Agentic loop hit max_iterations ({})", params.max_iterations);
            break;
        }

        let request = ChatRequest {
            model: params.model.clone(),
            max_tokens: params.max_tokens,
            system: params.system_prompt.clone(),
            messages: accumulated_messages.clone(),
            tools: params.tool_registry.definitions(),
            stream: true,
        };

        // ── Stream the API response ──────────────────────────────────────
        let mut stream = params.provider.chat_stream(request).await?;

        let mut text_blocks: Vec<(usize, String)> = Vec::new(); // (index, text)
        let mut tool_use_blocks: Vec<(usize, String, String, String)> = Vec::new(); // (index, id, name, partial_json)
        let mut stop_reason: Option<StopReason> = None;
        let mut turn_usage = Usage::default();

        while let Some(event_result) = stream.next().await {
            match event_result? {
                StreamEvent::MessageStart { message } => {
                    turn_usage += message.usage;
                }
                StreamEvent::ContentBlockStart { index, content_block } => {
                    match content_block {
                        ContentBlockStart::Text { .. } => {
                            text_blocks.push((index, String::new()));
                        }
                        ContentBlockStart::ToolUse { id, name } => {
                            tool_use_blocks.push((index, id, name, String::new()));
                        }
                    }
                }
                StreamEvent::ContentBlockDelta { index, delta } => {
                    match delta {
                        ContentBlockDelta::TextDelta { text } => {
                            if let Some((_, buf)) = text_blocks.iter_mut().find(|(i, _)| *i == index) {
                                buf.push_str(&text);
                                if let Some(cb) = &params.callbacks.on_text {
                                    cb(&text);
                                }
                            }
                        }
                        ContentBlockDelta::InputJsonDelta { partial_json } => {
                            if let Some((_, _, _, buf)) =
                                tool_use_blocks.iter_mut().find(|(i, _, _, _)| *i == index)
                            {
                                buf.push_str(&partial_json);
                            }
                        }
                    }
                }
                StreamEvent::MessageDelta { delta, usage } => {
                    stop_reason = delta.stop_reason;
                    if let Some(u) = usage {
                        turn_usage.output_tokens += u.output_tokens;
                    }
                }
                StreamEvent::MessageStop | StreamEvent::ContentBlockStop { .. }
                | StreamEvent::Ping | StreamEvent::Unknown => {}
            }
        }

        total_usage += turn_usage;

        // ── Build assistant ContentBlocks ────────────────────────────────
        let mut assistant_content: Vec<ContentBlock> = Vec::new();

        // Text blocks (in index order)
        let mut all_indices: Vec<usize> = text_blocks.iter().map(|(i, _)| *i)
            .chain(tool_use_blocks.iter().map(|(i, _, _, _)| *i))
            .collect();
        all_indices.sort_unstable();
        all_indices.dedup();

        for idx in &all_indices {
            if let Some((_, text)) = text_blocks.iter().find(|(i, _)| i == idx) {
                if !text.is_empty() {
                    assistant_content.push(ContentBlock::Text { text: text.clone() });
                }
            } else if let Some((_, id, name, json)) =
                tool_use_blocks.iter().find(|(i, _, _, _)| i == idx)
            {
                let input = serde_json::from_str::<serde_json::Value>(json)
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                assistant_content.push(ContentBlock::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input,
                });
            }
        }

        let assistant_msg = Message::assistant(assistant_content.clone(), total_usage.clone());
        new_messages.push(assistant_msg);

        accumulated_messages.push(ApiMessage {
            role: Role::Assistant,
            content: assistant_content.clone(),
        });

        // ── Check stop reason ────────────────────────────────────────────
        if stop_reason != Some(StopReason::ToolUse) {
            break;
        }

        // ── Execute tools ────────────────────────────────────────────────
        let calls: Vec<ToolCall> = tool_use_blocks
            .iter()
            .map(|(_, id, name, json)| {
                let input = serde_json::from_str::<serde_json::Value>(json)
                    .unwrap_or_default();
                ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    input,
                }
            })
            .collect();

        for call in &calls {
            if let Some(cb) = &params.callbacks.on_tool_start {
                cb(&call.name, &call.id);
            }
        }

        let results =
            run_tool_calls(&calls, params.tool_registry, params.tool_ctx).await;

        for r in &results {
            if let Some(cb) = &params.callbacks.on_tool_done {
                cb(&r.tool_use_id, r.is_error);
            }
        }

        // Build tool_result user message and continue the loop
        let tool_result_content = results_to_content(&results);

        // Attach tool results as a User message with tool_result blocks
        let tool_result_msg = Message::user(tool_result_content.clone());
        new_messages.push(tool_result_msg);

        accumulated_messages.push(ApiMessage {
            role: Role::User,
            content: tool_result_content,
        });
    }

    Ok(QueryResult {
        new_messages,
        usage: total_usage,
    })
}
