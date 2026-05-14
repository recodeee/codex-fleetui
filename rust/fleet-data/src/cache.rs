//! Tiny TTL cache for the dashboard data loaders.
//!
//! The fleet has four separate dashboard binaries (`fleet-state`,
//! `fleet-watcher`, `fleet-ui`, `fleet-tui-poc`) that each poll account /
//! pane state on a ~250 ms tick. Without a cache, every loader call shells
//! out — `codex-auth list` is hundreds of ms, and `tmux capture-pane`
//! adds one fork per pane. With a small in-process TTL this collapses
//! to one real call per TTL window, regardless of how many widgets in the
//! same binary ask for the data.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Single-slot TTL cache. Holds the most recent `T` plus its capture time;
/// `get_or_refresh` returns the cached value if it's younger than `ttl`,
/// otherwise calls `refresh` and stores the result.
pub struct TtlCache<T> {
    slot: Mutex<Option<(Instant, T)>>,
    ttl: Duration,
}

impl<T: Clone> TtlCache<T> {
    pub const fn new(ttl: Duration) -> Self {
        Self { slot: Mutex::new(None), ttl }
    }

    pub fn get_or_refresh<F, E>(&self, refresh: F) -> Result<T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        {
            let guard = self.slot.lock().unwrap();
            if let Some((ts, val)) = guard.as_ref() {
                if ts.elapsed() < self.ttl {
                    return Ok(val.clone());
                }
            }
        }
        // Drop the lock before the (possibly slow) refresh so concurrent
        // readers of a still-fresh value aren't blocked behind us.
        let fresh = refresh()?;
        let mut guard = self.slot.lock().unwrap();
        *guard = Some((Instant::now(), fresh.clone()));
        Ok(fresh)
    }

    pub fn invalidate(&self) {
        *self.slot.lock().unwrap() = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn caches_within_ttl_and_refreshes_after() {
        let cache: TtlCache<u32> = TtlCache::new(Duration::from_millis(50));
        let calls = AtomicUsize::new(0);
        let load = || -> Result<u32, ()> {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(42)
        };

        assert_eq!(cache.get_or_refresh(load).unwrap(), 42);
        assert_eq!(cache.get_or_refresh(load).unwrap(), 42);
        assert_eq!(calls.load(Ordering::SeqCst), 1, "second call inside TTL must reuse");

        std::thread::sleep(Duration::from_millis(70));
        assert_eq!(cache.get_or_refresh(load).unwrap(), 42);
        assert_eq!(calls.load(Ordering::SeqCst), 2, "call after TTL must refresh");
    }

    #[test]
    fn invalidate_forces_refresh() {
        let cache: TtlCache<u32> = TtlCache::new(Duration::from_secs(60));
        let calls = AtomicUsize::new(0);
        let load = || -> Result<u32, ()> {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(7)
        };

        cache.get_or_refresh(load).unwrap();
        cache.get_or_refresh(load).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        cache.invalidate();
        cache.get_or_refresh(load).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn refresh_error_is_not_cached() {
        let cache: TtlCache<u32> = TtlCache::new(Duration::from_secs(60));
        assert!(cache
            .get_or_refresh(|| -> Result<u32, &'static str> { Err("nope") })
            .is_err());
        let val = cache
            .get_or_refresh(|| -> Result<u32, &'static str> { Ok(99) })
            .unwrap();
        assert_eq!(val, 99, "after an error the next call must run refresh, not return stale");
    }
}
