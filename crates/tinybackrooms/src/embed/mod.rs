//! Text embeddings and similarity.
//!
//! The semantic scorers (originality, diversity, novelty against memory) only
//! need vectors and a similarity measure, so the engine takes them through the
//! [`Embedder`] port. [`HashingEmbedder`] is the built-in, dependency-free
//! implementation: it hashes character trigrams into a fixed-size vector. It
//! measures surface and morphological overlap rather than meaning, which makes
//! it cheap and deterministic but weaker than a learned model. Plug a learned
//! embedder in behind the same trait for real semantic distance.

/// Turns text into a vector whose cosine similarity approximates relatedness.
pub trait Embedder: Send + Sync {
    /// Embed one text. The result need not be normalized.
    fn embed(&self, text: &str) -> Vec<f32>;
}

/// The default embedder: hashed character trigrams, L2-normalized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HashingEmbedder {
    dims: usize,
}

impl HashingEmbedder {
    /// An embedder with `dims` buckets, raised to at least 8.
    #[must_use]
    pub fn new(dims: usize) -> Self {
        Self { dims: dims.max(8) }
    }
}

impl Default for HashingEmbedder {
    fn default() -> Self {
        Self::new(256)
    }
}

impl Embedder for HashingEmbedder {
    fn embed(&self, text: &str) -> Vec<f32> {
        let mut vector = vec![0.0_f32; self.dims];
        let padded: Vec<char> = std::iter::once(' ')
            .chain(text.chars().flat_map(char::to_lowercase))
            .chain(std::iter::once(' '))
            .collect();
        for window in padded.windows(3) {
            let hash = fnv1a(window);
            let bucket =
                usize::try_from(hash % u64::try_from(self.dims).unwrap_or(u64::MAX)).unwrap_or(0);
            vector[bucket] += 1.0;
        }
        let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut vector {
                *x /= norm;
            }
        }
        vector
    }
}

fn fnv1a(chars: &[char]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for c in chars {
        hash ^= u64::from(u32::from(*c));
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// Cosine similarity of two vectors, in `[-1, 1]`; `0` when either is zero or
/// the lengths differ.
#[must_use]
pub fn cosine(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() {
        return 0.0;
    }
    let dot: f64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| f64::from(*x) * f64::from(*y))
        .sum();
    let na: f64 = a.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>().sqrt();
    let nb: f64 = b.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        (dot / (na * nb)).clamp(-1.0, 1.0)
    }
}

#[cfg(test)]
mod test;
