//! Tool execution orchestrator — batches and runs tools.
//! Mirrors Claude Code's services/tools/toolOrchestration.ts.
//!
//! Execution strategy (mirrors Claude Code):
//!   - Non-concurrent-safe tools run alone in serial batches
//!   - Concurrent-safe tools in a batch run in parallel
//!   - Batches themselves are run serially

use std::sync::Arc;
use tokio::task::JoinSet;

use crate::messages::ContentBlock;

use super::{ToolContext, ToolRegistry, ToolResult};

/// A pending tool invocation parsed from an assistant message.
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// Execute all tool calls, respecting concurrency constraints.
/// Returns one ToolResult per ToolCall in the same order.
pub async fn run_tool_calls(
    calls: &[ToolCall],
    registry: &ToolRegistry,
    ctx: &ToolContext,
) -> Vec<ToolResult> {
    if calls.is_empty() {
        return Vec::new();
    }

    // Split into batches: each batch is either
    //   [one non-concurrent-safe tool]  or
    //   [N >= 1 concurrent-safe tools]
    let batches = build_batches(calls, registry);

    let mut results: Vec<Option<ToolResult>> = vec![None; calls.len()];

    for batch in batches {
        if batch.len() == 1 {
            let (orig_idx, call) = &batch[0];
            let result = execute_one(call, registry, ctx).await;
            results[*orig_idx] = Some(result);
        } else {
            // Concurrent batch
            let mut set = JoinSet::new();
            for (orig_idx, call) in batch {
                let call = call.clone();
                let registry_arc: Vec<(String, Arc<dyn super::Tool>)> = registry
                    .all()
                    .iter()
                    .map(|t| (t.name().to_string(), Arc::clone(t)))
                    .collect();
                let ctx = ctx.clone();
                set.spawn(async move {
                    let tool = registry_arc
                        .iter()
                        .find(|(name, _)| name == &call.name)
                        .map(|(_, t)| Arc::clone(t));

                    let result = if let Some(t) = tool {
                        t.call(call.id.clone(), call.input.clone(), &ctx).await
                    } else {
                        ToolResult::err(call.id.clone(), format!("Unknown tool: {}", call.name))
                    };
                    (orig_idx, result)
                });
            }
            while let Some(join_result) = set.join_next().await {
                if let Ok((idx, result)) = join_result {
                    results[idx] = Some(result);
                }
            }
        }
    }

    results
        .into_iter()
        .map(|r| r.expect("All results should be populated"))
        .collect()
}

/// Convert ToolResults to ContentBlocks for the next API message.
pub fn results_to_content(results: &[ToolResult]) -> Vec<ContentBlock> {
    results
        .iter()
        .map(|r| ContentBlock::ToolResult {
            tool_use_id: r.tool_use_id.clone(),
            content: r.content.clone(),
            is_error: if r.is_error { Some(true) } else { None },
        })
        .collect()
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

type IndexedCall<'a> = (usize, &'a ToolCall);

fn build_batches<'a>(
    calls: &'a [ToolCall],
    registry: &ToolRegistry,
) -> Vec<Vec<IndexedCall<'a>>> {
    let mut batches: Vec<Vec<IndexedCall<'a>>> = Vec::new();
    let mut concurrent_batch: Vec<IndexedCall<'a>> = Vec::new();

    for (i, call) in calls.iter().enumerate() {
        let concurrent_safe = registry
            .get(&call.name)
            .map(|t| t.is_concurrent_safe())
            .unwrap_or(false);

        if concurrent_safe {
            concurrent_batch.push((i, call));
        } else {
            // Flush pending concurrent batch first
            if !concurrent_batch.is_empty() {
                batches.push(std::mem::take(&mut concurrent_batch));
            }
            // Non-concurrent tool gets its own single-item batch
            batches.push(vec![(i, call)]);
        }
    }

    if !concurrent_batch.is_empty() {
        batches.push(concurrent_batch);
    }

    batches
}

async fn execute_one(
    call: &ToolCall,
    registry: &ToolRegistry,
    ctx: &ToolContext,
) -> ToolResult {
    match registry.get(&call.name) {
        Some(tool) => tool.call(call.id.clone(), call.input.clone(), ctx).await,
        None => ToolResult::err(call.id.clone(), format!("Unknown tool: {}", call.name)),
    }
}
