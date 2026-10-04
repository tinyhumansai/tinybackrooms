//! Unit tests for scoring, ranking, diversity and the judge.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;

use super::judge::{Judged, judge_all};
use super::math::{dat, distinct_classes, nearest_distance, vendi};
use super::*;
use crate::{
    Constraints, Context, Error, HashingEmbedder, OutputSpec, RunConfig, ScorerSpec, ScoringConfig,
    ScriptedConversant,
};

/// Embeds a text as the one-hot vector of its first letter, so identical first
/// letters are identical ideas and different ones are orthogonal.
struct LetterEmbedder;

impl Embedder for LetterEmbedder {
    fn embed(&self, text: &str) -> Vec<f32> {
        let mut v = vec![0.0; 26];
        if let Some(c) = text.chars().next().filter(char::is_ascii_lowercase) {
            v[(c as u8 - b'a') as usize] = 1.0;
        }
        v
    }
}

fn request(scorers: Vec<ScorerSpec>, top_k: Option<usize>) -> GenerateRequest {
    GenerateRequest {
        input: Context {
            brief: "brief".into(),
            seeds: vec![],
            constraints: Constraints::default(),
            memory_namespace: None,
        },
        output: OutputSpec {
            item_name: "i".into(),
            item_guidance: None,
            schema: json!(true),
        },
        config: RunConfig {
            scoring: ScoringConfig {
                scorers,
                select_top_k: top_k,
                ..ScoringConfig::default()
            },
            ..RunConfig::default()
        },
    }
}

fn spec(kind: ScorerKind, weight: f64) -> ScorerSpec {
    ScorerSpec { kind, weight }
}

fn candidates(words: &[&str]) -> Vec<Candidate> {
    words
        .iter()
        .enumerate()
        .map(|(i, w)| Candidate::new(&format!("v{i}"), json!({"name": w})))
        .collect()
}

fn idle() -> ScriptedConversant {
    ScriptedConversant::new(
        |_| Ok(String::new()),
        |_| Err(Error::Conversant("no judge".into())),
    )
}

fn rater(novelty: u32, usefulness: u32, boden: &str) -> ScriptedConversant {
    let boden = boden.to_owned();
    ScriptedConversant::new(
        |_| Ok(String::new()),
        move |_| Ok(json!({"novelty": novelty, "usefulness": usefulness, "boden": boden})),
    )
}

#[test]
fn item_text_joins_every_string_in_order() {
    let value = json!({"a": "one", "b": [{"c": "two"}, 3, true, null], "d": "three"});
    assert_eq!(item_text(&value), "one\ntwo\nthree");
    assert_eq!(item_text(&json!("solo")), "solo");
    assert_eq!(item_text(&json!(7)), "");
}

#[test]
fn elaboration_saturates_at_forty_distinct_words() {
    assert!(elaboration("") < f64::EPSILON);
    assert!((elaboration("a b c d") - 0.1).abs() < 1e-9);
    assert!((elaboration("a A a a") - 0.025).abs() < 1e-9);
    let long = (0..80)
        .map(|i| format!("w{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    assert!((elaboration(&long) - 1.0).abs() < f64::EPSILON);
}

#[test]
fn vendi_is_one_for_identical_items_and_n_for_orthogonal_ones() {
    let same = vec![vec![1.0, 0.0]; 3];
    assert!((vendi(&same) - 1.0).abs() < 1e-6);
    let orthogonal = vec![
        vec![1.0, 0.0, 0.0],
        vec![0.0, 1.0, 0.0],
        vec![0.0, 0.0, 1.0],
    ];
    assert!((vendi(&orthogonal) - 3.0).abs() < 1e-6);
    assert!(vendi(&[]).abs() < f64::EPSILON);
}

#[test]
fn vendi_handles_partially_similar_items() {
    let items = vec![vec![1.0, 0.0], vec![0.6, 0.8], vec![0.0, 1.0]];
    let score = vendi(&items);
    assert!(score > 1.0 && score < 3.0, "{score}");
}

#[test]
fn dat_is_the_scaled_mean_pairwise_distance() {
    assert!(dat(&[vec![1.0, 0.0]]).abs() < f64::EPSILON);
    assert!((dat(&[vec![1.0, 0.0], vec![0.0, 1.0]]) - 100.0).abs() < 1e-9);
    assert!(dat(&[vec![1.0, 0.0], vec![1.0, 0.0]]).abs() < 1e-9);
}

#[test]
fn distinct_classes_merges_near_duplicates() {
    let items = vec![vec![1.0, 0.0], vec![1.0, 0.0], vec![0.0, 1.0]];
    assert_eq!(distinct_classes(&items, 0.8), 2);
    assert_eq!(distinct_classes(&items, 2.0), 3);
}

#[test]
fn nearest_distance_counts_remembered_ideas() {
    let items = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
    let recalled = vec![vec![1.0, 0.0]];
    assert!(nearest_distance(0, &items, &recalled).abs() < 1e-9);
    assert!((nearest_distance(1, &items, &recalled) - 1.0).abs() < 1e-9);
    assert!((nearest_distance(0, &items[..1], &[]) - 0.5).abs() < f64::EPSILON);
}

#[tokio::test]
async fn ranks_the_more_original_item_first() {
    // "a1" and "a2" share a first letter so they are identical to the embedder;
    // "b" is distinct and therefore more original.
    let ranked = rank(
        candidates(&["a1", "a2", "b"]),
        &request(vec![spec(ScorerKind::Originality, 1.0)], None),
        &[],
        &LetterEmbedder,
        &idle(),
    )
    .await;
    assert_eq!(ranked.items[0].value, json!({"name": "b"}));
    assert!((ranked.items[0].total - 1.0).abs() < 1e-9);
    assert!(ranked.items[1].total.abs() < 1e-9);
    // Ties keep their original order.
    assert_eq!(ranked.items[1].run_id, "v0");
    assert_eq!(ranked.items[2].run_id, "v1");
}

#[tokio::test]
async fn remembered_ideas_push_repeats_down() {
    let ranked = rank(
        candidates(&["a", "b"]),
        &request(vec![spec(ScorerKind::Originality, 1.0)], None),
        &["an earlier a".to_owned()],
        &LetterEmbedder,
        &idle(),
    )
    .await;
    assert_eq!(ranked.items[0].value, json!({"name": "b"}));
    assert!(ranked.items[1].scores["originality"].abs() < 1e-9);
}

#[tokio::test]
async fn weights_decide_between_scorers() {
    let long = (0..40)
        .map(|i| format!("w{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    let pool = vec![
        Candidate::new("v0", json!({"t": "a"})),
        Candidate::new("v1", json!({"t": long})),
    ];
    let by_elaboration = rank(
        pool.clone(),
        &request(
            vec![
                spec(ScorerKind::Elaboration, 1.0),
                spec(ScorerKind::Originality, 0.0),
            ],
            None,
        ),
        &[],
        &HashingEmbedder::default(),
        &idle(),
    )
    .await;
    assert_eq!(by_elaboration.items[0].run_id, "v1");
}

#[tokio::test]
async fn keeps_only_the_top_k() {
    let ranked = rank(
        candidates(&["a", "b", "c"]),
        &request(vec![], Some(2)),
        &[],
        &LetterEmbedder,
        &idle(),
    )
    .await;
    assert_eq!(ranked.items.len(), 2);
    assert_eq!(ranked.diversity.fluency, 2);
}

#[tokio::test]
async fn an_empty_pool_yields_empty_diversity() {
    let ranked = rank(
        vec![],
        &request(vec![], None),
        &[],
        &LetterEmbedder,
        &idle(),
    )
    .await;
    assert_eq!(ranked.items.len(), 0);
    assert_eq!(ranked.diversity.fluency, 0);
    assert!(ranked.diversity.flexibility.abs() < f64::EPSILON);
}

#[tokio::test]
async fn reports_set_diversity_of_the_selected_items() {
    let ranked = rank(
        candidates(&["a1", "a2", "b"]),
        &request(vec![], None),
        &[],
        &LetterEmbedder,
        &idle(),
    )
    .await;
    assert_eq!(ranked.diversity.fluency, 3);
    assert_eq!(ranked.diversity.distinct_classes, 2);
    assert!((ranked.diversity.flexibility - 2.0 / 3.0).abs() < 1e-9);
    assert!(ranked.diversity.vendi > 1.0 && ranked.diversity.vendi < 3.0);
    assert!(ranked.diversity.dat > 0.0);
}

#[tokio::test]
async fn the_judge_gates_novelty_by_usefulness() {
    // novelty 7 × usefulness 1 is worthless; 4 × 4 beats it.
    let backend = ScriptedConversant::new(
        |_| Ok(String::new()),
        |request| {
            let strange = request.messages[0].content.contains("strange");
            Ok(if strange {
                json!({"novelty": 7, "usefulness": 1, "boden": "transformational"})
            } else {
                json!({"novelty": 4, "usefulness": 4, "boden": "combinational"})
            })
        },
    );
    let pool = vec![
        Candidate::new("v0", json!({"name": "strange"})),
        Candidate::new("v1", json!({"name": "solid"})),
    ];
    let ranked = rank(
        pool,
        &request(vec![spec(ScorerKind::Judge, 1.0)], None),
        &[],
        &LetterEmbedder,
        &backend,
    )
    .await;
    assert_eq!(ranked.items[0].run_id, "v1");
    assert_eq!(ranked.items[0].boden_class, Some(BodenClass::Combinational));
    assert!((ranked.items[0].scores["judge"] - 0.25).abs() < 1e-9);
    assert!(ranked.items[0].scores["judge_disagreement"].abs() < 1e-9);
    assert_eq!(ranked.usage.calls, 6);
    assert_eq!(ranked.warnings.len(), 0);
}

#[tokio::test]
async fn a_failing_judge_scores_zero_and_warns_instead_of_failing_the_run() {
    let ranked = rank(
        candidates(&["a"]),
        &request(vec![spec(ScorerKind::Judge, 1.0)], None),
        &[],
        &LetterEmbedder,
        &idle(),
    )
    .await;
    assert_eq!(ranked.items.len(), 1);
    assert!(ranked.items[0].scores["judge"].abs() < f64::EPSILON);
    assert_eq!(ranked.items[0].boden_class, None);
    assert!(ranked.warnings[0].contains("3 judge rating(s)"));
}

#[tokio::test]
async fn raters_disagreeing_produces_a_spread_and_the_majority_label() {
    let calls = std::sync::atomic::AtomicU32::new(0);
    let backend = ScriptedConversant::new(
        |_| Ok(String::new()),
        move |_| {
            let n = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(match n % 3 {
                0 => json!({"novelty": 7, "usefulness": 7, "boden": "exploratory"}),
                1 => json!({"novelty": 1, "usefulness": 1, "boden": "exploratory"}),
                _ => json!({"novelty": 4, "usefulness": 4, "boden": "transformational"}),
            })
        },
    );
    let judged: Vec<Judged> = judge_all(
        "brief",
        &candidates(&["x"]),
        3,
        Some("judge-model"),
        &backend,
    )
    .await;
    assert!((judged[0].score - (1.0 + 0.0 + 0.25) / 3.0).abs() < 1e-9);
    assert!(judged[0].disagreement > 0.3);
    assert_eq!(judged[0].boden, Some(BodenClass::Exploratory));
    assert_eq!(judged[0].failed_raters, 0);
}

#[tokio::test]
async fn out_of_range_ratings_count_as_failed_raters() {
    let backend = rater(9, 3, "exploratory");
    let judged = judge_all("brief", &candidates(&["x"]), 2, None, &backend).await;
    assert_eq!(judged[0].failed_raters, 2);
    assert!(judged[0].score.abs() < f64::EPSILON);
    assert_eq!(judged[0].boden, None);
}

#[tokio::test]
async fn an_unknown_boden_label_is_ignored_but_the_rating_counts() {
    let backend = rater(7, 7, "mystical");
    let judged = judge_all("brief", &candidates(&["x"]), 1, None, &backend).await;
    assert!((judged[0].score - 1.0).abs() < 1e-9);
    assert_eq!(judged[0].boden, None);
}

#[tokio::test]
async fn at_least_one_rater_always_runs() {
    let backend = rater(7, 7, "combinational");
    let judged = judge_all("brief", &candidates(&["x"]), 0, None, &backend).await;
    assert_eq!(judged[0].usage.calls, 1);
}
