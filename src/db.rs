use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

/// Every key and value. Bytes, not String: Redis strings are binary safe.
pub(crate) type Store = HashMap<Vec<u8>, Vec<u8>>;

/// The keyspace shared by every connection: one store behind one lock.
///
/// Cloning is cheap: it copies the `Arc` (a reference count), not the store.
///
/// Rules this design relies on:
/// - The lock is held for exactly one command and never across an `.await`. The compiler
///   enforces the second part: `MutexGuard` is not `Send`, so a task holding it across an
///   `.await` cannot be passed to `tokio::spawn`.
/// - Commands see only `&mut Store`, so changing the locking scheme does not touch them.
///
/// Revisit this when either of these happens:
/// - With a multi-threaded runtime, throughput stops growing as worker threads are added
///   (measure with many connections, not `scripts/cpubench.sh` alone): the single lock is
///   the bottleneck. Next step is sharding, at the cost of locking several shards for
///   multi-key commands and for MULTI/EXEC (S09).
/// - Blocking commands (S07) need waiters that a `Mutex` plus `Notify` cannot express
///   cleanly: move to one task that owns the store and receives commands over a channel.
#[derive(Clone, Default)]
pub(crate) struct Db {
    store: Arc<Mutex<Store>>,
}

impl Db {
    /// Locks the store for one command.
    pub(crate) fn lock(&self) -> MutexGuard<'_, Store> {
        // A panic while holding the lock "poisons" it. No production path panics, and a
        // single HashMap operation cannot leave the map half-updated, so keep serving
        // rather than failing every later command on every connection.
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
