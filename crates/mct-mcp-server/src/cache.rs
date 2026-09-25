//! Exact-match fast-path cache for MCP tool results.
//!
//! Keyed by a SHA-256 hash of (tool name, canonicalized argument string,
//! index generation) — a byte-for-byte repeat of the same call against the
//! same index state is served from memory instead of re-running the query
//! and re-rendering the response. There is no similarity/semantic path:
//! only an identical call hits the cache, which is what
//! [issue #16](https://github.com/Zubiarka8/mini-consumes-tokens/issues/16)
//! calls the "SHA-256 fast path for exact match".
//!
//! Invalidation is generation-based, not time-based: `Index::generation()`
//! is bumped on every `reindex()` call (see `mct-index`'s doc comment on the
//! field), and every cache entry embeds the generation it was computed
//! under. The first cache access after a reindex sees a new generation and
//! drops every entry from the previous one — no explicit "invalidate on
//! repo changes" call is needed, and a stale entry can never be served.

use std::collections::HashMap;

use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

struct CacheState {
    generation: u64,
    entries: HashMap<[u8; 32], String>,
}

/// Shared across every clone of `MctServer` (wrap in `Arc`) so all callers
/// in a session see the same cache.
pub struct QueryCache {
    state: Mutex<CacheState>,
}

impl QueryCache {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(CacheState {
                generation: 0,
                entries: HashMap::new(),
            }),
        }
    }

    fn key(tool: &str, request: &str) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(tool.as_bytes());
        hasher.update(b"\0");
        hasher.update(request.as_bytes());
        hasher.finalize().into()
    }

    /// Returns the cached response for (`tool`, `request`) if one was stored
    /// under the current `generation`. `generation` moving *forward* (a
    /// reindex happened) drops every entry from the old generation at once.
    /// `generation` being *behind* the cache's own (a caller that read the
    /// index's generation just before a concurrent reindex bumped it) is
    /// just an ordinary miss — it must never roll the cache backward and
    /// resurrect entries a newer, already-cached generation shadows.
    pub async fn get(&self, tool: &str, request: &str, generation: u64) -> Option<String> {
        let mut state = self.state.lock().await;
        match generation.cmp(&state.generation) {
            std::cmp::Ordering::Greater => {
                state.generation = generation;
                state.entries.clear();
                None
            }
            std::cmp::Ordering::Less => None,
            std::cmp::Ordering::Equal => state.entries.get(&Self::key(tool, request)).cloned(),
        }
    }

    /// Stores `response` for (`tool`, `request`) under `generation`. If the
    /// cache is no longer at that generation — moved on by a reindex, or
    /// (see `get`) never reached it because a newer call arrived first — the
    /// entry is dropped instead of stored, since keeping it would either
    /// resurrect a stale answer or wrongly advance the cache's generation as
    /// a side effect of a write.
    pub async fn put(&self, tool: &str, request: &str, generation: u64, response: String) {
        let mut state = self.state.lock().await;
        if generation != state.generation {
            return;
        }
        state.entries.insert(Self::key(tool, request), response);
    }
}

impl Default for QueryCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_miss_returns_none_and_a_stored_value_is_returned_on_the_same_generation() {
        let cache = QueryCache::new();
        assert_eq!(cache.get("list_symbols", "path=src", 0).await, None);
        cache
            .put("list_symbols", "path=src", 0, "hit".to_string())
            .await;
        assert_eq!(
            cache.get("list_symbols", "path=src", 0).await,
            Some("hit".to_string())
        );
    }

    #[tokio::test]
    async fn different_tools_or_requests_never_collide() {
        let cache = QueryCache::new();
        cache.put("find_symbol", "name=foo", 0, "a".to_string()).await;
        cache.put("find_symbol", "name=bar", 0, "b".to_string()).await;
        cache.put("list_symbols", "name=foo", 0, "c".to_string()).await;
        assert_eq!(
            cache.get("find_symbol", "name=foo", 0).await,
            Some("a".to_string())
        );
        assert_eq!(
            cache.get("find_symbol", "name=bar", 0).await,
            Some("b".to_string())
        );
        assert_eq!(
            cache.get("list_symbols", "name=foo", 0).await,
            Some("c".to_string())
        );
    }

    #[tokio::test]
    async fn a_forward_generation_move_drops_every_entry_from_the_old_one() {
        let cache = QueryCache::new();
        cache
            .put("list_symbols", "path=src", 0, "stale".to_string())
            .await;
        assert_eq!(cache.get("list_symbols", "path=src", 1).await, None);
        // The stale entry is gone even under the old generation number —
        // the whole map was cleared, not just shadowed.
        cache
            .put("list_symbols", "path=src", 1, "fresh".to_string())
            .await;
        assert_eq!(
            cache.get("list_symbols", "path=src", 1).await,
            Some("fresh".to_string())
        );
    }

    #[tokio::test]
    async fn a_lookup_behind_the_cache_s_current_generation_is_a_plain_miss_and_never_rolls_back() {
        let cache = QueryCache::new();
        // Move the cache to generation 1 first, then populate it there.
        assert_eq!(cache.get("list_symbols", "path=src", 1).await, None);
        cache
            .put("list_symbols", "path=src", 1, "fresh".to_string())
            .await;
        // A caller that read generation 0 (e.g. just before a concurrent
        // reindex bumped it to 1) must see a miss, not resurrect "fresh" nor
        // wipe it from under a caller still on generation 1.
        assert_eq!(cache.get("list_symbols", "path=src", 0).await, None);
        assert_eq!(
            cache.get("list_symbols", "path=src", 1).await,
            Some("fresh".to_string())
        );
    }

    #[tokio::test]
    async fn put_under_a_stale_generation_is_dropped_not_stored() {
        let cache = QueryCache::new();
        // Move the cache to generation 1 first.
        assert_eq!(cache.get("list_symbols", "path=src", 1).await, None);
        // A caller that read generation 0 before the bump tries to store
        // its (now-stale) result — it must not resurrect generation 0 data.
        cache
            .put("list_symbols", "path=src", 0, "stale".to_string())
            .await;
        assert_eq!(cache.get("list_symbols", "path=src", 1).await, None);
    }
}
