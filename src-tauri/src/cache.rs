//! Session-scoped read caches.
//!
//! Commands are stateless by design (install + mod path per call, no global
//! state), which taken literally meant every command re-read and re-parsed its
//! whole domain from disk — decoding the 34MB provinces.bmp and parsing ~4k
//! province history files per map render was the app-wide lag root cause.
//! This module generalizes `loc.rs`'s process-level memo: expensive
//! derivations are cached keyed by the session's (install, mod) pair and
//! served until invalidated.
//!
//! Invalidation contract: [`invalidate_all`] drops every cache (loc included).
//! It runs after any project write (`edits::apply_queue`) and when a session
//! (re)opens (the `invalidate_caches` command, called by MapView on mount), so
//! within a session a cache can never outlive the disk state it mirrors.
//! External edits *during* a session (e.g. a git checkout while the app is
//! open) are not detected — same tradeoff `loc::store` already made.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex, OnceLock};

use crate::vfs::Vfs;

/// The (install, mod) pair identifying a session's disk state. Mirrors
/// `loc::CacheKey`.
pub type SessionKey = (String, Option<String>);

/// The cache key for the session `vfs` reads from.
pub fn session_key(vfs: &Vfs) -> SessionKey {
    (
        vfs.base_dir().to_string_lossy().into_owned(),
        vfs.mod_dir().map(|p| p.to_string_lossy().into_owned()),
    )
}

/// A keyed memo store for one derivation. Declared as a `static` in the module
/// that owns the derivation; values are shared out as `Arc`s so readers never
/// hold the lock while using them.
pub struct Store<K, T> {
    cell: OnceLock<Mutex<HashMap<K, Arc<T>>>>,
}

impl<K: Eq + Hash + Clone, T> Store<K, T> {
    pub const fn new() -> Self {
        Self {
            cell: OnceLock::new(),
        }
    }

    fn map(&self) -> &Mutex<HashMap<K, Arc<T>>> {
        self.cell.get_or_init(|| Mutex::new(HashMap::new()))
    }

    /// Cached value for `key`, building it on a miss. The lock is not held
    /// during the build, so concurrent misses may build twice — both results
    /// are equivalent and the last insert wins (same policy as `loc::store`).
    pub fn get_or_build(&self, key: K, build: impl FnOnce() -> T) -> Arc<T> {
        if let Some(hit) = self.map().lock().unwrap().get(&key) {
            return hit.clone();
        }
        let built = Arc::new(build());
        self.map().lock().unwrap().insert(key, built.clone());
        built
    }

    /// Fallible variant of [`Store::get_or_build`]; errors are returned to the
    /// caller and never cached.
    pub fn get_or_try_build<E>(
        &self,
        key: K,
        build: impl FnOnce() -> Result<T, E>,
    ) -> Result<Arc<T>, E> {
        if let Some(hit) = self.map().lock().unwrap().get(&key) {
            return Ok(hit.clone());
        }
        let built = Arc::new(build()?);
        self.map().lock().unwrap().insert(key, built.clone());
        Ok(built)
    }

    /// Number of cached entries — lets an owner bound growth (e.g. the render
    /// cache clears itself past a cap instead of evicting piecemeal).
    pub fn len(&self) -> usize {
        self.map().lock().unwrap().len()
    }

    pub fn clear(&self) {
        self.map().lock().unwrap().clear();
    }
}

/// Drops every session cache in the process, `loc` included. Wholesale rather
/// than per-key: writes are rare (explicit saves), a full rebuild after one is
/// cheap, and clear-all can never miss a derived-data dependency.
pub fn invalidate_all() {
    crate::game_data::invalidate_caches();
    crate::map_renderer::invalidate_caches();
    crate::trigger_eval::invalidate_caches();
    crate::goods_spawn::invalidate_caches();
    crate::loc::invalidate_all();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_memoizes_and_clears() {
        static STORE: Store<(String, Option<String>), u32> = Store::new();
        let key = ("install".to_string(), None);
        let mut builds = 0;
        let a = STORE.get_or_build(key.clone(), || {
            builds += 1;
            7
        });
        let b = STORE.get_or_build(key.clone(), || {
            builds += 1;
            9
        });
        assert_eq!((*a, *b, builds), (7, 7, 1));
        STORE.clear();
        let c = STORE.get_or_build(key, || 9);
        assert_eq!(*c, 9);
    }

    #[test]
    fn try_build_never_caches_errors() {
        static STORE: Store<u32, u32> = Store::new();
        let err: Result<Arc<u32>, String> = STORE.get_or_try_build(1, || Err("boom".into()));
        assert!(err.is_err());
        let ok = STORE.get_or_try_build(1, || Ok::<u32, String>(5)).unwrap();
        assert_eq!(*ok, 5);
    }
}
