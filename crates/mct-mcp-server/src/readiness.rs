//! Whether the index has finished the indexing the server started with.
//!
//! The server answers `initialize` and `tools/list` right away and indexes in
//! the background, so a tool must never read a half-built index as if it were
//! complete. The state is explicit: a tool call waits a bounded time for
//! [`IndexState::Ready`] and otherwise reports why it cannot answer yet.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::watch;

/// What the startup indexing has done so far.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IndexState {
    /// The index is complete; tools answer from it.
    Ready,
    /// Startup indexing is still running since `since`.
    Indexing { since: Instant },
    /// Startup indexing failed with this diagnostic. Tools keep refusing
    /// until the `reindex` tool succeeds.
    Failed(String),
}

/// Shared handle on the [`IndexState`]; clones observe the same state.
#[derive(Clone, Debug)]
pub struct Readiness {
    state: Arc<watch::Sender<IndexState>>,
}

impl Readiness {
    /// A server that was handed an already-indexed index (every caller but
    /// `mct-mcp-server`'s own startup).
    pub fn ready() -> Self {
        Self {
            state: Arc::new(watch::Sender::new(IndexState::Ready)),
        }
    }

    pub fn state(&self) -> IndexState {
        self.state.borrow().clone()
    }

    pub fn begin_indexing(&self) {
        self.state.send_replace(IndexState::Indexing {
            since: Instant::now(),
        });
    }

    pub fn mark_ready(&self) {
        self.state.send_replace(IndexState::Ready);
    }

    pub fn mark_failed(&self, diagnostic: impl Into<String>) {
        self.state
            .send_replace(IndexState::Failed(diagnostic.into()));
    }

    /// Waits, without a limit, until the state is [`IndexState::Ready`]: the
    /// way out of a failed startup is a successful `reindex`.
    pub async fn until_ready(&self) {
        let mut rx = self.state.subscribe();
        let _ = rx.wait_for(|s| matches!(s, IndexState::Ready)).await;
    }

    /// Waits up to `limit` for the state to leave [`IndexState::Indexing`]
    /// and returns the state it ends in: `Indexing` only when `limit` ran out.
    pub async fn settled(&self, limit: Duration) -> IndexState {
        let mut rx = self.state.subscribe();
        let waited = tokio::time::timeout(
            limit,
            rx.wait_for(|s| !matches!(s, IndexState::Indexing { .. })),
        )
        .await;
        let settled = match waited {
            Ok(Ok(state)) => Some(state.clone()),
            // Timed out, or the sender is gone: report what is current.
            _ => None,
        };
        settled.unwrap_or_else(|| self.state())
    }
}

impl Default for Readiness {
    fn default() -> Self {
        Self::ready()
    }
}

/// The message a tool returns while it cannot be answered from the index.
/// `None` when the index is ready.
pub fn refusal(state: &IndexState) -> Option<String> {
    match state {
        IndexState::Ready => None,
        IndexState::Indexing { since } => Some(format!(
            "index_state: indexing\nThe index is still being built ({}s so far). No partial results \
             are served: retry shortly, or call get_indexing_status.",
            since.elapsed().as_secs()
        )),
        IndexState::Failed(diagnostic) => Some(format!(
            "index_state: failed\nStartup indexing failed: {diagnostic}\nNo results are served from \
             an incomplete index. Fix the cause and call reindex, or restart the server."
        )),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[tokio::test]
    async fn a_ready_state_settles_immediately() {
        let readiness = Readiness::ready();
        assert_eq!(
            readiness.settled(Duration::from_secs(5)).await,
            IndexState::Ready
        );
    }

    #[tokio::test]
    async fn indexing_times_out_without_pretending_to_be_ready() {
        let readiness = Readiness::ready();
        readiness.begin_indexing();
        let state = readiness.settled(Duration::from_millis(20)).await;
        assert!(matches!(state, IndexState::Indexing { .. }));
        assert!(refusal(&state).is_some_and(|m| m.contains("indexing")));
    }

    #[tokio::test]
    async fn a_waiter_wakes_when_indexing_finishes() {
        let readiness = Readiness::ready();
        readiness.begin_indexing();
        let waiter = {
            let readiness = readiness.clone();
            tokio::spawn(async move { readiness.settled(Duration::from_secs(30)).await })
        };
        tokio::task::yield_now().await;
        readiness.mark_ready();
        assert_eq!(waiter.await.unwrap_or(IndexState::Ready), IndexState::Ready);
    }

    #[tokio::test]
    async fn until_ready_returns_once_a_failed_state_recovers() {
        let readiness = Readiness::ready();
        readiness.begin_indexing();
        readiness.mark_failed("disk full");
        let waiter = {
            let readiness = readiness.clone();
            tokio::spawn(async move { readiness.until_ready().await })
        };
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished());
        readiness.mark_ready();
        waiter.await.unwrap();
    }

    #[tokio::test]
    async fn a_failure_is_reported_with_its_diagnostic() {
        let readiness = Readiness::ready();
        readiness.begin_indexing();
        readiness.mark_failed("disk full");
        let state = readiness.settled(Duration::from_secs(1)).await;
        assert!(refusal(&state).is_some_and(|m| m.contains("disk full")));
    }
}
