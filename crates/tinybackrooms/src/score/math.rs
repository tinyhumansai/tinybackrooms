//! Numeric building blocks for the scorers: distances, set diversity and the
//! `Vendi` score.

use crate::cosine;

/// Cosine distance clamped to `[0, 1]`.
pub(super) fn distance(a: &[f32], b: &[f32]) -> f64 {
    (1.0 - cosine(a, b)).clamp(0.0, 1.0)
}

/// A small count as `f64`.
pub(super) fn count(n: usize) -> f64 {
    f64::from(u32::try_from(n).unwrap_or(u32::MAX))
}

/// Distance from item `index` to its nearest neighbour among the other items
/// and the remembered ones; `0.5` when there is nothing to compare against.
pub(super) fn nearest_distance(index: usize, items: &[Vec<f32>], recalled: &[Vec<f32>]) -> f64 {
    let others = items
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != index)
        .map(|(_, v)| v)
        .chain(recalled);
    others
        .map(|other| distance(&items[index], other))
        .min_by(f64::total_cmp)
        .unwrap_or(0.5)
}

/// Mean pairwise distance times 100, after the Divergent Association Task.
pub(super) fn dat(items: &[Vec<f32>]) -> f64 {
    let mut total = 0.0;
    let mut pairs = 0_u32;
    for (i, a) in items.iter().enumerate() {
        for b in &items[i + 1..] {
            total += distance(a, b);
            pairs += 1;
        }
    }
    if pairs == 0 {
        0.0
    } else {
        100.0 * total / f64::from(pairs)
    }
}

/// Greedy leader clustering: an item joins the first class whose leader it
/// matches at `threshold` or above, otherwise it starts a new class.
pub(super) fn distinct_classes(items: &[Vec<f32>], threshold: f64) -> usize {
    let mut leaders: Vec<&Vec<f32>> = Vec::new();
    for item in items {
        if !leaders
            .iter()
            .any(|leader| cosine(leader, item) >= threshold)
        {
            leaders.push(item);
        }
    }
    leaders.len()
}

/// The `Vendi` score: `exp` of the Shannon entropy of the eigenvalues of the
/// similarity matrix divided by `n`. It is the effective number of distinct
/// items: 1 when all items are identical, `n` when they are all orthogonal.
pub(super) fn vendi(items: &[Vec<f32>]) -> f64 {
    let n = items.len();
    if n == 0 {
        return 0.0;
    }
    let scale = count(n);
    let matrix: Vec<Vec<f64>> = items
        .iter()
        .map(|a| {
            items
                .iter()
                .map(|b| cosine(a, b).max(0.0) / scale)
                .collect()
        })
        .collect();
    let entropy: f64 = eigenvalues(matrix)
        .into_iter()
        .filter(|lambda| *lambda > 1e-12)
        .map(|lambda| -lambda * lambda.ln())
        .sum();
    entropy.exp()
}

/// Eigenvalues of a symmetric matrix by cyclic Jacobi rotations.
fn eigenvalues(mut matrix: Vec<Vec<f64>>) -> Vec<f64> {
    let size = matrix.len();
    for _sweep in 0..64 {
        let off_diagonal: f64 = matrix
            .iter()
            .enumerate()
            .flat_map(|(i, row)| row.iter().enumerate().filter(move |(j, _)| *j != i))
            .map(|(_, value)| value * value)
            .sum();
        if off_diagonal < 1e-18 {
            break;
        }
        for p in 0..size {
            for q in p + 1..size {
                rotate(&mut matrix, p, q);
            }
        }
    }
    matrix.iter().enumerate().map(|(i, row)| row[i]).collect()
}

/// Zero the `(p, q)` entry with a Jacobi rotation, updating columns then rows.
fn rotate(matrix: &mut [Vec<f64>], p: usize, q: usize) {
    if matrix[p][q].abs() < 1e-300 {
        return;
    }
    let theta = (matrix[q][q] - matrix[p][p]) / (2.0 * matrix[p][q]);
    let tangent = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
    let cosine = 1.0 / (tangent * tangent + 1.0).sqrt();
    let sine = tangent * cosine;
    for row in matrix.iter_mut() {
        let (x, y) = (row[p], row[q]);
        row[p] = cosine * x - sine * y;
        row[q] = sine * x + cosine * y;
    }
    let (head, tail) = matrix.split_at_mut(q);
    for (x, y) in head[p].iter_mut().zip(tail[0].iter_mut()) {
        let (px, qy) = (*x, *y);
        *x = cosine * px - sine * qy;
        *y = sine * px + cosine * qy;
    }
}
