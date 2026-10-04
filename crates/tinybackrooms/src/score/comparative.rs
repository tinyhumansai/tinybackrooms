//! The comparative scorer: one reviewer ranks all candidates side by side.
//!
//! Rating each candidate alone lets a plausible-sounding flaw through, because
//! nothing shows what a better answer looks like. Seeing the whole pool, a
//! reviewer can compare them against the request's criteria and against each
//! other. On a benchmark with checkable answers this recovered every correct
//! answer that the pool contained, where per-item rating did not.
//!
//! The reviewer writes a short assessment first and the ranking second, which
//! gives it room to reason before it commits. Candidates are shown in a
//! shuffle fixed by the request seed, so position bias is spread evenly and a
//! fixed backend still gives a fixed ranking. A candidate's score is its rank
//! mapped onto `[0, 1]`: the best scores `1`, the worst `0`. A candidate the
//! reviewer leaves out is placed after the ones it ranked.

use serde_json::{Value, json};

use super::Candidate;
use super::math::count;
use crate::rng::Rng;
use crate::{Conversant, Message, Seat, StructuredRequest, Usage};

const TOOL: &str = "rank_items";

/// Distinguishes the shuffle from the other uses of the request seed.
const SHUFFLE_STREAM: u64 = 0x5c0_7e;

const SYSTEM: &str = "You are a strict reviewer comparing candidate answers to the same problem. \
Check each candidate against the problem and the criteria: does it really work, does it rely on \
anything the problem does not give, and is it better or worse than the others? Penalise claims \
the problem does not support. In `assessment`, note in one sentence per candidate the most \
serious flaw, or that it has none. Then call the `rank_items` tool once with `ranking`: every \
candidate number, best first.";

const DEFAULT_CRITERIA: &str = "How well the candidate serves the problem.";

/// The tool schema the reviewer answers with. `assessment` comes first so the
/// reasoning precedes the verdict.
fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "assessment": {"type": "string"},
            "ranking": {"type": "array", "items": {"type": "integer", "minimum": 1}}
        },
        "required": ["assessment", "ranking"]
    })
}

/// The outcome of comparing a pool.
#[derive(Debug)]
pub(super) struct Comparison {
    /// One score per candidate, in the candidates' own order.
    pub(super) scores: Vec<f64>,
    /// Cost of the comparison.
    pub(super) usage: Usage,
    /// The reviewer call failed or its ranking was unusable.
    pub(super) failed: bool,
}

/// Rank `candidates` against each other and the optional `criteria`.
///
/// A pool of fewer than two has nothing to compare and costs no call.
pub(super) async fn compare_all(
    brief: &str,
    criteria: Option<&str>,
    candidates: &[Candidate],
    model: Option<&str>,
    seed: u64,
    backend: &dyn Conversant,
) -> Comparison {
    let n = candidates.len();
    if n < 2 {
        return Comparison {
            scores: vec![1.0; n],
            usage: Usage::default(),
            failed: false,
        };
    }
    let shown = shuffled(n, seed);
    let blocks: Vec<String> = shown
        .iter()
        .enumerate()
        .map(|(slot, index)| format!("[{}]\n{}", slot + 1, candidates[*index].text))
        .collect();
    let prompt = format!(
        "PROBLEM\n{brief}\n\nCRITERIA\n{}\n\nCANDIDATES\n\n{}",
        criteria.unwrap_or(DEFAULT_CRITERIA),
        blocks.join("\n\n")
    );
    let reply = backend
        .structured(StructuredRequest {
            seat: Seat {
                run_id: "compare".to_owned(),
                role: "reviewer".to_owned(),
            },
            model: model.map(str::to_owned),
            system: SYSTEM.to_owned(),
            messages: vec![Message::user(prompt)],
            tool: TOOL.to_owned(),
            schema: schema(),
        })
        .await;
    let Ok(reply) = reply else {
        return failed(n);
    };
    let Some(ranking) = reply.value.get("ranking").and_then(Value::as_array) else {
        return failed_with(n, reply.usage);
    };

    let mut order: Vec<usize> = Vec::with_capacity(n);
    for slot in ranking.iter().filter_map(Value::as_u64) {
        let candidate = usize::try_from(slot)
            .ok()
            .and_then(|slot| slot.checked_sub(1))
            .and_then(|slot| shown.get(slot).copied());
        if let Some(index) = candidate.filter(|index| !order.contains(index)) {
            order.push(index);
        }
    }
    order.extend((0..n).filter(|index| !order.contains(index)));

    let mut scores = vec![0.0; n];
    for (place, index) in order.into_iter().enumerate() {
        scores[index] = 1.0 - count(place) / count(n - 1);
    }
    Comparison {
        scores,
        usage: reply.usage,
        failed: false,
    }
}

fn failed(n: usize) -> Comparison {
    failed_with(n, Usage::default())
}

fn failed_with(n: usize, usage: Usage) -> Comparison {
    Comparison {
        scores: vec![0.0; n],
        usage,
        failed: true,
    }
}

/// `0..n` in an order fixed by `seed` (Fisher-Yates).
fn shuffled(n: usize, seed: u64) -> Vec<usize> {
    let mut order: Vec<usize> = (0..n).collect();
    let mut rng = Rng::new(seed, SHUFFLE_STREAM);
    for i in (1..n).rev() {
        order.swap(i, rng.below(i + 1));
    }
    order
}
