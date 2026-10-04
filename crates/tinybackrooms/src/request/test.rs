//! Unit tests for request validation and the pinned wire form.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use serde_json::json;

fn request() -> GenerateRequest {
    GenerateRequest {
        input: Context {
            brief: "name a lighthouse cafe".into(),
            seeds: vec![],
            constraints: Constraints::default(),
            memory_namespace: None,
        },
        output: OutputSpec {
            item_name: "name".into(),
            item_guidance: None,
            schema: json!({"type": "object", "required": ["name"],
                "properties": {"name": {"type": "string"}}}),
        },
        config: RunConfig::default(),
    }
}

#[test]
fn accepts_a_default_request() {
    request().validate().unwrap();
}

#[test]
fn rejects_a_blank_brief() {
    let mut r = request();
    r.input.brief = "  \n".into();
    assert_eq!(r.validate(), Err(Error::EmptyBrief));
}

#[test]
fn rejects_zero_variations() {
    let mut r = request();
    r.config.variations = 0;
    assert_eq!(r.validate(), Err(Error::NoVariations));
}

#[test]
fn rejects_too_many_variations() {
    let mut r = request();
    r.config.variations = MAX_VARIATIONS + 1;
    assert_eq!(
        r.validate(),
        Err(Error::TooManyVariations {
            max: MAX_VARIATIONS
        })
    );
}

#[test]
fn rejects_zero_generations() {
    let mut r = request();
    r.config.generations = 0;
    assert_eq!(r.validate(), Err(Error::NoGenerations));
}

#[test]
fn rejects_zero_items_per_variation() {
    let mut r = request();
    r.config.items_per_variation = 0;
    assert_eq!(r.validate(), Err(Error::NoItems));
}

#[test]
fn rejects_zero_top_k() {
    let mut r = request();
    r.config.scoring.select_top_k = Some(0);
    assert_eq!(r.validate(), Err(Error::InvalidTopK));
}

#[test]
fn rejects_negative_nan_and_all_zero_weights() {
    for weights in [vec![-1.0], vec![f64::NAN], vec![0.0, 0.0]] {
        let mut r = request();
        r.config.scoring.scorers = weights
            .into_iter()
            .map(|weight| ScorerSpec {
                kind: ScorerKind::Originality,
                weight,
            })
            .collect();
        assert_eq!(r.validate(), Err(Error::InvalidWeights));
    }
}

#[test]
fn rejects_an_uncompilable_schema() {
    let mut r = request();
    r.output.schema = json!({"type": 12});
    assert!(matches!(r.validate(), Err(Error::InvalidSchema(_))));
}

#[test]
fn defaults_to_originality_and_elaboration_when_no_scorers_are_set() {
    let kinds: Vec<_> = ScoringConfig::default()
        .effective_scorers()
        .into_iter()
        .map(|s| s.kind)
        .collect();
    assert_eq!(kinds, [ScorerKind::Originality, ScorerKind::Elaboration]);
}

#[test]
fn keeps_explicit_scorers() {
    let config = ScoringConfig {
        scorers: vec![ScorerSpec {
            kind: ScorerKind::Judge,
            weight: 2.0,
        }],
        ..ScoringConfig::default()
    };
    assert_eq!(config.effective_scorers(), config.scorers);
}

#[test]
fn minimal_json_decodes_with_defaults() {
    let r: GenerateRequest = serde_json::from_value(json!({
        "input": {"brief": "x"},
        "output": {"schema": {"type": "string"}}
    }))
    .unwrap();
    assert_eq!(r.output.item_name, "idea");
    assert_eq!(r.config, RunConfig::default());
    assert_eq!(r.input.seeds.len(), 0);
}

#[test]
fn wire_form_is_pinned() {
    let r: GenerateRequest = serde_json::from_value(json!({
        "input": {
            "brief": "b",
            "seeds": [{"kind": "prior_idea", "text": "t"}],
            "constraints": {"must": ["m"], "must_not": ["n"]},
            "memory_namespace": "ns"
        },
        "output": {"item_name": "i", "item_guidance": "g", "schema": true},
        "config": {
            "variations": 2, "generations": 4, "items_per_variation": 5,
            "mode": "critic_pair", "template": "fugue", "temperature": 0.7,
            "models": {"agent_a": "a", "agent_b": "b", "formatter": "f", "judge": "j"},
            "scoring": {"scorers": [{"kind": "judge", "weight": 2.0},
                    {"kind": "comparative", "weight": 1.0}],
                "select_top_k": 3, "judge_raters": 5, "class_threshold": 0.9,
                "criteria": "the answer must work in practice"},
            "budget": {"max_turns": 8, "wall_clock_secs": 60, "repair_attempts": 1},
            "seed": 42, "include_transcripts": true
        }
    }))
    .unwrap();
    assert_eq!(r.input.seeds[0].kind, SeedKind::PriorIdea);
    assert_eq!(r.config.mode, Mode::CriticPair);
    assert_eq!(r.config.template, Template::Fugue);
    assert_eq!(r.config.scoring.scorers[0].kind, ScorerKind::Judge);
    assert_eq!(r.config.scoring.scorers[1].kind, ScorerKind::Comparative);
    assert_eq!(
        r.config.scoring.criteria.as_deref(),
        Some("the answer must work in practice")
    );
    assert_eq!(r.config.budget.max_turns, Some(8));
    assert_eq!(r.config.models.agent_b.as_deref(), Some("b"));
    let round_trip: GenerateRequest =
        serde_json::from_value(serde_json::to_value(&r).unwrap()).unwrap();
    assert_eq!(round_trip, r);
}

#[test]
fn rejects_unknown_fields() {
    let bad = json!({"input": {"brief": "x", "brif": 1}, "output": {"schema": true}});
    assert!(serde_json::from_value::<GenerateRequest>(bad).is_err());
}
