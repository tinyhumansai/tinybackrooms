//! The memory port: what earlier runs remember.
//!
//! When a request names a `memory_namespace`, the engine recalls earlier
//! accepted items before the conversation starts, tells the agents to move away
//! from them, and scores new items for distance from them. After selection it
//! stores the accepted items back. A backend decides what "recall" means: the
//! `OpenHuman` adapter uses its retrieval, while [`InMemoryMemory`] just returns
//! the most recent entries.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::{BoxFuture, Error, Result};

/// Durable recall and storage for accepted items, scoped by namespace.
pub trait Memory: Send + Sync {
    /// Up to `limit` stored entries most relevant to `query`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Memory`] when the backend fails.
    fn recall(
        &self,
        namespace: &str,
        query: &str,
        limit: usize,
    ) -> BoxFuture<'_, Result<Vec<String>>>;

    /// Append `entries` to the namespace.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Memory`] when the backend fails.
    fn store(&self, namespace: &str, entries: Vec<String>) -> BoxFuture<'_, Result<()>>;
}

/// A process-local [`Memory`]. Recall ignores the query and returns the most
/// recently stored entries, oldest first.
#[derive(Debug, Default)]
pub struct InMemoryMemory {
    entries: Mutex<HashMap<String, Vec<String>>>,
}

impl InMemoryMemory {
    /// An empty memory.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

fn poisoned<T>(_: T) -> Error {
    Error::Memory("memory lock poisoned".to_owned())
}

impl Memory for InMemoryMemory {
    fn recall(
        &self,
        namespace: &str,
        _query: &str,
        limit: usize,
    ) -> BoxFuture<'_, Result<Vec<String>>> {
        let result = self.entries.lock().map_err(poisoned).map(|map| {
            let all = map.get(namespace).map_or(&[][..], Vec::as_slice);
            all[all.len().saturating_sub(limit)..].to_vec()
        });
        Box::pin(async move { result })
    }

    fn store(&self, namespace: &str, entries: Vec<String>) -> BoxFuture<'_, Result<()>> {
        let result = self.entries.lock().map_err(poisoned).map(|mut map| {
            map.entry(namespace.to_owned()).or_default().extend(entries);
        });
        Box::pin(async move { result })
    }
}

#[cfg(test)]
mod test;
