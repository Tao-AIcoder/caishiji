//! Retry logic for transient API errors.
//! Mirrors Claude Code's services/api/withRetry.ts.

use anyhow::Result;
use std::time::Duration;
use tokio::time::sleep;

#[allow(dead_code)]
/// Exponential back-off parameters
const MAX_ATTEMPTS: u32 = 4;
#[allow(dead_code)] const BASE_DELAY_MS: u64 = 500;
#[allow(dead_code)] const MAX_DELAY_MS: u64 = 10_000;

/// HTTP status codes that are worth retrying
fn is_retryable_status(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503 | 529)
}

/// Run `f` with exponential back-off on transient failures.
pub async fn with_retry<F, Fut, T>(mut f: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut attempt = 0u32;
    loop {
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                attempt += 1;
                if attempt >= MAX_ATTEMPTS || !is_retryable_error(&e) {
                    return Err(e);
                }
                let delay = std::cmp::min(
                    BASE_DELAY_MS * 2u64.pow(attempt - 1),
                    MAX_DELAY_MS,
                );
                tracing::warn!(
                    attempt,
                    delay_ms = delay,
                    error = %e,
                    "Retrying after transient error"
                );
                sleep(Duration::from_millis(delay)).await;
            }
        }
    }
}

fn is_retryable_error(err: &anyhow::Error) -> bool {
    // Check reqwest HTTP status embedded in error message (simple heuristic)
    let msg = err.to_string();
    for status in [429u16, 500, 502, 503, 529] {
        if msg.contains(&status.to_string()) {
            return true;
        }
    }
    // Network-level errors are always retryable
    if msg.contains("connection") || msg.contains("timeout") || msg.contains("reset") {
        return true;
    }
    false
}
