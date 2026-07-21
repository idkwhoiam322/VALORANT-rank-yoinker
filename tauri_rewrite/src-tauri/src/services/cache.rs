//! Shared TTL + LRU cache used by the per-entity services (rank, stats, …).
//!
//! Previously every service re-implemented the same `Mutex<LruCache<K, (V, Instant)>>`
//! + TTL-expiry pattern by hand. This module centralises it so
//! the expiry logic lives in exactly one place.

use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use lru::LruCache;
use tokio::sync::Mutex;

/// An LRU-bounded cache whose entries also expire after `ttl`. Wrapped in a
/// `tokio::sync::Mutex` so it can be shared across async tasks.
pub(crate) struct TtlLruCache<K, V> {
    inner: Mutex<LruCache<K, (V, Instant)>>,
    ttl: Duration,
}

impl<K, V> TtlLruCache<K, V>
where
    K: std::hash::Hash + Eq + Clone,
    V: Clone,
{
    pub fn new(capacity: usize, ttl: Duration) -> Self {
        let cap = NonZeroUsize::new(capacity.max(1)).expect("cache capacity must be > 0");
        TtlLruCache {
            inner: Mutex::new(LruCache::new(cap)),
            ttl,
        }
    }

    /// Fetch `key` if present and not expired. Returns the remaining TTL (in
    /// seconds) alongside the value so callers can report cache-hit telemetry.
    pub async fn get<Q>(&self, key: &Q) -> Option<(V, u64)>
    where
        K: std::borrow::Borrow<Q>,
        Q: std::hash::Hash + Eq + ?Sized,
    {
        let mut guard = self.inner.lock().await;
        let (value, ts) = guard.get(key)?;
        if ts.elapsed() < self.ttl {
            let ttl_left = self.ttl.as_secs().saturating_sub(ts.elapsed().as_secs());
            Some((value.clone(), ttl_left))
        } else {
            None
        }
    }

    /// Insert (or refresh) a value for `key` with a fresh timestamp.
    pub async fn insert(&self, key: K, value: V) {
        let mut guard = self.inner.lock().await;
        guard.put(key, (value, Instant::now()));
    }

    /// Drop every entry.
    pub async fn clear(&self) {
        self.inner.lock().await.clear();
    }
}
