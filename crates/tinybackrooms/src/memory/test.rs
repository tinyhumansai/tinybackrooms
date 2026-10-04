//! Unit tests for the in-process memory.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

#[tokio::test]
async fn recalls_nothing_from_an_unknown_namespace() {
    let memory = InMemoryMemory::new();
    assert_eq!(memory.recall("none", "q", 5).await.unwrap().len(), 0);
}

#[tokio::test]
async fn stores_and_recalls_the_most_recent_entries() {
    let memory = InMemoryMemory::new();
    memory
        .store("ns", vec!["a".into(), "b".into()])
        .await
        .unwrap();
    memory.store("ns", vec!["c".into()]).await.unwrap();
    assert_eq!(memory.recall("ns", "q", 2).await.unwrap(), ["b", "c"]);
    assert_eq!(memory.recall("ns", "q", 10).await.unwrap(), ["a", "b", "c"]);
}

#[tokio::test]
async fn namespaces_are_isolated() {
    let memory = InMemoryMemory::new();
    memory.store("one", vec!["x".into()]).await.unwrap();
    assert_eq!(memory.recall("two", "q", 5).await.unwrap().len(), 0);
}

#[tokio::test]
async fn reports_a_poisoned_lock_as_a_memory_error() {
    let memory = std::sync::Arc::new(InMemoryMemory::new());
    let held = std::sync::Arc::clone(&memory);
    let _ = std::thread::spawn(move || {
        let _guard = held.entries.lock().unwrap();
        panic!("poison the lock");
    })
    .join();
    assert!(matches!(
        memory.recall("ns", "q", 1).await,
        Err(Error::Memory(_))
    ));
    assert!(matches!(
        memory.store("ns", vec![]).await,
        Err(Error::Memory(_))
    ));
}
