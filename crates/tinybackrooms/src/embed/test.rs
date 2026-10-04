//! Unit tests for the hashing embedder and cosine similarity.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

#[test]
fn identical_text_has_similarity_one() {
    let e = HashingEmbedder::default();
    let v = e.embed("a lighthouse cafe");
    assert!((cosine(&v, &v) - 1.0).abs() < 1e-9);
}

#[test]
fn related_text_is_closer_than_unrelated_text() {
    let e = HashingEmbedder::default();
    let base = e.embed("a cafe inside an old lighthouse");
    let near = e.embed("a cafe inside a lighthouse");
    let far = e.embed("quantum chromodynamics lattice gauge");
    assert!(cosine(&base, &near) > cosine(&base, &far));
}

#[test]
fn embedding_is_case_insensitive_and_normalized() {
    let e = HashingEmbedder::default();
    assert_eq!(e.embed("Moon"), e.embed("moon"));
    let norm: f32 = e.embed("moon").iter().map(|x| x * x).sum();
    assert!((norm - 1.0).abs() < 1e-5);
}

#[test]
fn empty_text_embeds_to_zero_and_is_unrelated_to_everything() {
    let e = HashingEmbedder::default();
    let empty = e.embed("");
    assert!(empty.iter().all(|x| *x == 0.0));
    assert!(cosine(&empty, &e.embed("moon")).abs() < f64::EPSILON);
}

#[test]
fn dimensions_are_clamped_to_a_minimum() {
    assert_eq!(HashingEmbedder::new(1).embed("x").len(), 8);
}

#[test]
fn cosine_of_mismatched_or_zero_vectors_is_zero() {
    assert!(cosine(&[1.0], &[1.0, 2.0]).abs() < f64::EPSILON);
    assert!(cosine(&[0.0, 0.0], &[1.0, 1.0]).abs() < f64::EPSILON);
}

#[test]
fn cosine_of_opposite_vectors_is_minus_one() {
    assert!((cosine(&[1.0, 0.0], &[-1.0, 0.0]) + 1.0).abs() < 1e-9);
}
