//! Unit tests for the seeded generator.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

#[test]
fn same_seed_and_stream_repeat() {
    let mut a = Rng::new(7, 1);
    let mut b = Rng::new(7, 1);
    assert_eq!((a.next_u64(), a.next_u64()), (b.next_u64(), b.next_u64()));
}

#[test]
fn different_streams_diverge() {
    assert_ne!(Rng::new(7, 1).next_u64(), Rng::new(7, 2).next_u64());
}

#[test]
fn below_stays_in_range_and_handles_zero() {
    let mut rng = Rng::new(1, 0);
    for _ in 0..100 {
        assert!(rng.below(5) < 5);
    }
    assert_eq!(rng.below(0), 0);
}
