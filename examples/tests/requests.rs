//! Every bundled request file must parse, validate, and run offline.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::path::PathBuf;

use tinybackrooms_examples::{load, run_offline, summarize};

fn files() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("requests");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    files
}

#[test]
fn ships_the_documented_examples() {
    let names: Vec<_> = files()
        .iter()
        .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        [
            "game-mechanics",
            "product-naming",
            "research-hypotheses",
            "story-premises"
        ]
    );
}

#[tokio::test]
async fn every_request_validates_and_runs() {
    for path in files() {
        let request = load(&path).unwrap();
        request
            .validate()
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let response = run_offline(&request)
            .await
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(!response.items.is_empty(), "{}", path.display());
        let expected = request.config.scoring.select_top_k.unwrap_or(usize::MAX);
        assert!(response.items.len() <= expected);
        assert!(summarize(&response).contains("backend calls"));
    }
}

#[tokio::test]
async fn the_story_example_returns_transcripts() {
    let path = files()
        .into_iter()
        .find(|p| p.ends_with("story-premises.json"))
        .unwrap();
    let response = run_offline(&load(&path).unwrap()).await.unwrap();
    assert_eq!(response.transcripts.unwrap().len(), 3);
}

#[test]
fn loading_reports_the_file_on_failure() {
    let err = load(std::path::Path::new("/nonexistent/request.json")).unwrap_err();
    assert!(err.contains("/nonexistent/request.json"));
}
