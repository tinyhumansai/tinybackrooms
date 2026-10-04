//! A tiny seeded random number generator.
//!
//! The engine only needs reproducible choices, such as which lens a variation
//! draws, so a `SplitMix64` generator is enough and avoids a dependency.

/// A `SplitMix64` generator. Not suitable for anything security-sensitive.
#[derive(Debug, Clone)]
pub(crate) struct Rng {
    state: u64,
}

impl Rng {
    /// A generator for `seed`, mixed with `stream` so sibling variations differ.
    pub(crate) fn new(seed: u64, stream: u64) -> Self {
        let mut rng = Self {
            state: seed ^ stream.wrapping_mul(0x9e37_79b9_7f4a_7c15),
        };
        rng.next_u64();
        rng
    }

    /// The next 64 random bits.
    pub(crate) fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A value in `0..n`, or `0` when `n` is `0`.
    pub(crate) fn below(&mut self, n: usize) -> usize {
        let bound = u64::try_from(n).unwrap_or(u64::MAX).max(1);
        usize::try_from(self.next_u64() % bound).unwrap_or(0)
    }
}

#[cfg(test)]
mod test;
