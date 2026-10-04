//! Scoring, ranking and diversity.
//!
//! Every candidate gets a score in `[0, 1]` from each configured scorer, and the
//! ranking key is their weighted mean:
//!
//! - **`originality`** is the semantic distance to the nearest other candidate
//!   or remembered idea (novelty search), so repeating an earlier run's idea
//!   scores near zero.
//! - **`elaboration`** is the number of distinct words in the item's text,
//!   saturating at 40, after the AUT's elaboration measure.
//! - **`judge`** is the multi-rater `novelty × usefulness` score described in
//!   `judge`.
//!
//! - **`comparative`** is the rank a single reviewer gives the candidate when it
//!   sees all of them side by side, held to the request's `criteria`; see
//!   `comparative`.
//!
//! The returned set is also summarized by [`Diversity`]: the `Vendi` score,
//! distinct classes, and the AUT's fluency and flexibility, plus a DAT-style
//! mean pairwise distance. The embedding-based measures use the [`Embedder`]
//! port and are only as semantic as that embedder is; the default measures
//! surface overlap.
//!
//! Scores of different scorers are not rescaled against each other, so weights
//! are a statement about the raw `[0, 1]` values.

mod comparative;
mod judge;
mod math;

use std::collections::{BTreeMap, HashSet};

use serde_json::Value;

use crate::{
    BodenClass, Conversant, Diversity, Embedder, GenerateRequest, ScoredItem, ScorerKind, Usage,
};

/// Items beyond this many are left out of the `Vendi` score, which is cubic in
/// the item count.
const VENDI_MAX_ITEMS: usize = 128;

/// Distinct words at which `elaboration` saturates.
const ELABORATION_WORDS: f64 = 40.0;

/// An item awaiting a score.
#[derive(Debug, Clone)]
pub(crate) struct Candidate {
    /// The variation that produced it.
    pub(crate) run_id: String,
    /// The schema-valid item.
    pub(crate) value: Value,
    /// The item's text, from [`item_text`].
    pub(crate) text: String,
}

impl Candidate {
    /// Wrap a valid item.
    pub(crate) fn new(run_id: &str, value: Value) -> Self {
        let text = item_text(&value);
        Self {
            run_id: run_id.to_owned(),
            value,
            text,
        }
    }
}

/// The outcome of ranking a pool of candidates.
#[derive(Debug)]
pub(crate) struct Ranked {
    /// Selected items, best first.
    pub(crate) items: Vec<ScoredItem>,
    /// Diversity of the selected set.
    pub(crate) diversity: Diversity,
    /// Judge call costs.
    pub(crate) usage: Usage,
    /// Judge failures worth surfacing.
    pub(crate) warnings: Vec<String>,
}

/// The readable text of an item: every string in it, in order, one per line.
pub(crate) fn item_text(value: &Value) -> String {
    fn walk(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::String(s) => out.push(s.clone()),
            Value::Array(items) => items.iter().for_each(|v| walk(v, out)),
            Value::Object(map) => map.values().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    let mut parts = Vec::new();
    walk(value, &mut parts);
    parts.join("\n")
}

fn elaboration(text: &str) -> f64 {
    let distinct: HashSet<String> = text.split_whitespace().map(str::to_lowercase).collect();
    (math::count(distinct.len()) / ELABORATION_WORDS).min(1.0)
}

/// Score, rank and select `candidates`.
///
/// A judge call that fails never fails the run: that rater is dropped, and an
/// item with no usable rating scores `0` on `judge` with a warning.
pub(crate) async fn rank(
    candidates: Vec<Candidate>,
    request: &GenerateRequest,
    recalled: &[String],
    embedder: &dyn Embedder,
    backend: &dyn Conversant,
) -> Ranked {
    let config = &request.config.scoring;
    let scorers = config.effective_scorers();
    let vectors: Vec<Vec<f32>> = candidates.iter().map(|c| embedder.embed(&c.text)).collect();
    let recalled_vectors: Vec<Vec<f32>> = recalled.iter().map(|t| embedder.embed(t)).collect();

    let mut usage = Usage::default();
    let mut warnings = Vec::new();
    let judged = if scorers.iter().any(|s| s.kind == ScorerKind::Judge) {
        let judged = judge::judge_all(
            &request.input.brief,
            &candidates,
            config.judge_raters,
            request.config.models.judge.as_deref(),
            backend,
        )
        .await;
        for (candidate, result) in candidates.iter().zip(&judged) {
            usage.add(result.usage);
            if result.failed_raters > 0 {
                warnings.push(format!(
                    "{}: {} judge rating(s) failed or were unusable",
                    candidate.run_id, result.failed_raters
                ));
            }
        }
        Some(judged)
    } else {
        None
    };

    let compared = if scorers.iter().any(|s| s.kind == ScorerKind::Comparative) {
        let result = comparative::compare_all(
            &request.input.brief,
            config.criteria.as_deref(),
            &candidates,
            request.config.models.judge.as_deref(),
            request.config.seed,
            backend,
        )
        .await;
        usage.add(result.usage);
        if result.failed {
            warnings.push("comparison failed or was unusable; candidates score 0".to_owned());
        }
        Some(result.scores)
    } else {
        None
    };

    let weight_sum: f64 = scorers.iter().map(|s| s.weight).sum();
    let mut scored: Vec<ScoredItem> = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        let mut by_scorer = BTreeMap::new();
        let mut boden_class: Option<BodenClass> = None;
        let mut total = 0.0;
        for spec in &scorers {
            let (name, value) = match spec.kind {
                ScorerKind::Originality => (
                    "originality",
                    math::nearest_distance(index, &vectors, &recalled_vectors),
                ),
                ScorerKind::Elaboration => ("elaboration", elaboration(&candidate.text)),
                ScorerKind::Judge => {
                    let judged = judged.as_ref().and_then(|all| all.get(index));
                    if let Some(judged) = judged {
                        by_scorer.insert("judge_disagreement".to_owned(), judged.disagreement);
                        boden_class = judged.boden;
                    }
                    ("judge", judged.map_or(0.0, |j| j.score))
                }
                ScorerKind::Comparative => (
                    "comparative",
                    compared
                        .as_ref()
                        .and_then(|all| all.get(index))
                        .copied()
                        .unwrap_or(0.0),
                ),
            };
            by_scorer.insert(name.to_owned(), value);
            total += spec.weight * value;
        }
        scored.push(ScoredItem {
            value: candidate.value.clone(),
            run_id: candidate.run_id.clone(),
            scores: by_scorer,
            total: total / weight_sum,
            boden_class,
        });
    }

    // Stable, so equal totals keep their variation order and a fixed backend
    // gives a fixed ranking.
    let mut order: Vec<usize> = (0..scored.len()).collect();
    order.sort_by(|a, b| scored[*b].total.total_cmp(&scored[*a].total));
    order.truncate(config.select_top_k.unwrap_or(usize::MAX));
    let selected: Vec<usize> = order;
    let selected_vectors: Vec<Vec<f32>> = selected.iter().map(|i| vectors[*i].clone()).collect();
    let items: Vec<ScoredItem> = selected.into_iter().map(|i| scored[i].clone()).collect();

    let diversity = diversity(&selected_vectors, config.class_threshold);
    Ranked {
        items,
        diversity,
        usage,
        warnings,
    }
}

fn diversity(vectors: &[Vec<f32>], threshold: f64) -> Diversity {
    let classes = math::distinct_classes(vectors, threshold);
    let fluency = vectors.len();
    let capped = &vectors[..vectors.len().min(VENDI_MAX_ITEMS)];
    Diversity {
        vendi: math::vendi(capped),
        distinct_classes: u32::try_from(classes).unwrap_or(u32::MAX),
        fluency: u32::try_from(fluency).unwrap_or(u32::MAX),
        flexibility: if fluency == 0 {
            0.0
        } else {
            math::count(classes) / math::count(fluency)
        },
        dat: math::dat(vectors),
    }
}

#[cfg(test)]
mod test;
