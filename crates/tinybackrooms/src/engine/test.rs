//! Unit tests for the engine pipeline.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Mutex;

use serde_json::json;

use super::*;
use crate::{
    BoxFuture, Constraints, Context, OutputSpec, RunConfig, ScorerKind, ScorerSpec,
    ScriptedConversant,
};

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
        config: RunConfig {
            variations: 2,
            generations: 2,
            items_per_variation: 2,
            ..RunConfig::default()
        },
    }
}

/// Conversation turns echo their seat; the formatter names two items per run.
fn backend() -> ScriptedConversant {
    ScriptedConversant::new(
        |r| Ok(format!("{} on {}", r.seat.role, r.seat.run_id)),
        |r| {
            Ok(json!({"items": [
                {"name": format!("{} lamp keeper", r.seat.run_id)},
                {"name": format!("{} salt and tide", r.seat.run_id)},
            ]}))
        },
    )
}

fn engine(backend: ScriptedConversant) -> Engine {
    Engine::new(Arc::new(backend))
}

#[derive(Default)]
struct Recorder(Mutex<Vec<Event>>);

impl Observer for Recorder {
    fn event(&self, event: Event) {
        self.0.lock().unwrap().push(event);
    }
}

#[tokio::test]
async fn runs_the_whole_pipeline() {
    let response = engine(backend()).generate(&request()).await.unwrap();
    assert_eq!(response.items.len(), 4);
    assert_eq!(response.warnings.len(), 0);
    assert!(response.transcripts.is_none());
    // 2 variations × (2 turns + 1 formatter call).
    assert_eq!(response.usage.calls, 6);
    assert_eq!(response.diversity.fluency, 4);
    for item in &response.items {
        assert!(item.scores.contains_key("originality"));
        assert!(item.scores.contains_key("elaboration"));
        assert!((0.0..=1.0).contains(&item.total));
    }
    let totals: Vec<f64> = response.items.iter().map(|i| i.total).collect();
    assert!(
        totals.windows(2).all(|w| w[0] >= w[1]),
        "not ranked: {totals:?}"
    );
}

#[tokio::test]
async fn is_deterministic_for_a_fixed_backend_and_seed() {
    let first = engine(backend()).generate(&request()).await.unwrap();
    let second = engine(backend()).generate(&request()).await.unwrap();
    assert_eq!(first, second);
}

#[tokio::test]
async fn includes_transcripts_on_request() {
    let mut r = request();
    r.config.include_transcripts = true;
    let response = engine(backend()).generate(&r).await.unwrap();
    let transcripts = response.transcripts.unwrap();
    assert_eq!(transcripts.len(), 2);
    assert_eq!(transcripts[0].turns[0].text, "agent_a on v0");
}

#[tokio::test]
async fn selects_the_top_k() {
    let mut r = request();
    r.config.scoring.select_top_k = Some(3);
    let response = engine(backend()).generate(&r).await.unwrap();
    assert_eq!(response.items.len(), 3);
}

#[tokio::test]
async fn refuses_an_invalid_request_before_calling_the_backend() {
    let calls = Arc::new(Mutex::new(0_u32));
    let counter = Arc::clone(&calls);
    let counting = ScriptedConversant::new(
        move |_| {
            *counter.lock().unwrap() += 1;
            Ok(String::new())
        },
        |_| Ok(json!({"items": []})),
    );
    let mut r = request();
    r.input.brief = " ".into();
    assert_eq!(
        engine(counting).generate(&r).await.unwrap_err(),
        Error::EmptyBrief
    );
    assert_eq!(*calls.lock().unwrap(), 0);
}

#[tokio::test]
async fn drops_a_failed_variation_with_a_warning() {
    let flaky = ScriptedConversant::new(
        |r| {
            if r.seat.run_id == "v1" {
                Err(Error::Conversant("rate limited".into()))
            } else {
                Ok("fine".into())
            }
        },
        |r| Ok(json!({"items": [{"name": format!("{} idea", r.seat.run_id)}]})),
    );
    let response = engine(flaky).generate(&request()).await.unwrap();
    assert_eq!(response.items.len(), 1);
    assert_eq!(response.items[0].run_id, "v0");
    assert_eq!(
        response.warnings,
        ["v1 failed: conversant failed: rate limited"]
    );
}

#[tokio::test]
async fn returns_the_backend_error_when_every_variation_fails() {
    let down = ScriptedConversant::new(
        |_| Err(Error::Conversant("down".into())),
        |_| Err(Error::Conversant("down".into())),
    );
    let err = engine(down).generate(&request()).await.unwrap_err();
    assert_eq!(err, Error::Conversant("down".into()));
}

#[tokio::test]
async fn errors_when_no_item_ever_satisfies_the_schema() {
    let wrong =
        ScriptedConversant::new(|_| Ok("x".into()), |_| Ok(json!({"items": [{"nope": 1}]})));
    let err = engine(wrong).generate(&request()).await.unwrap_err();
    assert_eq!(err, Error::NoValidItems);
}

#[tokio::test]
async fn warns_about_items_that_never_passed_the_schema() {
    let mixed = ScriptedConversant::new(
        |_| Ok("x".into()),
        |r| Ok(json!({"items": [{"name": r.seat.run_id.clone()}, {"nope": 1}]})),
    );
    let mut r = request();
    r.config.budget.repair_attempts = 0;
    let response = engine(mixed).generate(&r).await.unwrap();
    assert_eq!(response.items.len(), 2);
    assert!(response.warnings[0].contains("1 item(s) still failed the schema after 0"));
}

#[tokio::test]
async fn an_expired_wall_clock_budget_still_extracts_from_what_exists() {
    let mut r = request();
    r.config.budget.wall_clock_secs = Some(0);
    r.config.include_transcripts = true;
    let response = engine(backend()).generate(&r).await.unwrap();
    assert!(
        response
            .transcripts
            .unwrap()
            .iter()
            .all(|t| t.turns.is_empty())
    );
    assert_eq!(response.usage.calls, 2);
}

#[tokio::test]
async fn stores_selected_items_and_penalizes_repeats_on_the_next_run() {
    let memory = Arc::new(InMemoryMemory::new());
    let mut r = request();
    r.input.memory_namespace = Some("cafe".into());
    r.config.scoring.scorers = vec![ScorerSpec {
        kind: ScorerKind::Originality,
        weight: 1.0,
    }];
    let engine = engine(backend()).with_memory(memory.clone());

    let first = engine.generate(&r).await.unwrap();
    let stored = memory.recall("cafe", "", 10).await.unwrap();
    assert_eq!(stored.len(), first.items.len());

    // The same backend now proposes exactly what is remembered.
    let second = engine.generate(&r).await.unwrap();
    assert!(
        second.items.iter().all(|i| i.scores["originality"] < 1e-9),
        "repeats should score zero: {:?}",
        second
            .items
            .iter()
            .map(|i| i.scores["originality"])
            .collect::<Vec<_>>()
    );
}

struct BrokenMemory;

impl Memory for BrokenMemory {
    fn recall(&self, _: &str, _: &str, _: usize) -> BoxFuture<'_, Result<Vec<String>>> {
        Box::pin(async { Err(Error::Memory("offline".into())) })
    }

    fn store(&self, _: &str, _: Vec<String>) -> BoxFuture<'_, Result<()>> {
        Box::pin(async { Err(Error::Memory("offline".into())) })
    }
}

#[tokio::test]
async fn a_failing_memory_degrades_to_warnings() {
    let mut r = request();
    r.input.memory_namespace = Some("cafe".into());
    let response = engine(backend())
        .with_memory(Arc::new(BrokenMemory))
        .generate(&r)
        .await
        .unwrap();
    assert_eq!(response.items.len(), 4);
    assert_eq!(
        response.warnings,
        [
            "memory recall skipped: memory failed: offline",
            "memory store skipped: memory failed: offline"
        ]
    );
}

#[tokio::test]
async fn reports_progress_events() {
    let recorder = Arc::new(Recorder::default());
    let observer: Arc<dyn Observer> = recorder.clone();
    engine(backend())
        .with_observer(observer)
        .generate(&request())
        .await
        .unwrap();
    let events = recorder.0.lock().unwrap();
    let count = |f: fn(&Event) -> bool| events.iter().filter(|e| f(e)).count();
    assert_eq!(count(|e| matches!(e, Event::VariationStarted { .. })), 2);
    assert_eq!(count(|e| matches!(e, Event::Turn { .. })), 4);
    assert_eq!(
        count(|e| matches!(
            e,
            Event::Extracted {
                valid: 2,
                invalid: 0,
                ..
            }
        )),
        2
    );
    assert_eq!(count(|e| matches!(e, Event::Scored { items: 4 })), 1);
    assert!(matches!(events.last(), Some(Event::Scored { .. })));
}

#[tokio::test]
async fn reports_a_failed_variation_event() {
    let recorder = Arc::new(Recorder::default());
    let observer: Arc<dyn Observer> = recorder.clone();
    let down = ScriptedConversant::new(
        |_| Err(Error::Conversant("down".into())),
        |_| Ok(json!(null)),
    );
    let _ = engine(down)
        .with_observer(observer)
        .generate(&request())
        .await;
    let failures = recorder
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|e| matches!(e, Event::VariationFailed { .. }))
        .count();
    assert_eq!(failures, 2);
}

#[tokio::test]
async fn uses_a_custom_embedder() {
    struct Constant;
    impl Embedder for Constant {
        fn embed(&self, _: &str) -> Vec<f32> {
            vec![1.0, 0.0]
        }
    }
    let response = engine(backend())
        .with_embedder(Arc::new(Constant))
        .generate(&request())
        .await
        .unwrap();
    // Everything embeds identically, so nothing is original and all collapse to one class.
    assert!(
        response
            .items
            .iter()
            .all(|i| i.scores["originality"] < 1e-9)
    );
    assert_eq!(response.diversity.distinct_classes, 1);
}

#[tokio::test]
async fn the_response_wire_form_is_pinned() {
    let response = engine(backend()).generate(&request()).await.unwrap();
    let wire = serde_json::to_value(&response).unwrap();
    for key in ["items", "diversity", "usage", "warnings"] {
        assert!(wire.get(key).is_some(), "missing {key}");
    }
    assert!(wire.get("transcripts").is_none());
    let item = &wire["items"][0];
    for key in ["value", "run_id", "scores", "total"] {
        assert!(item.get(key).is_some(), "missing item.{key}");
    }
    assert!(item.get("boden_class").is_none());
    for key in ["vendi", "distinct_classes", "fluency", "flexibility", "dat"] {
        assert!(
            wire["diversity"].get(key).is_some(),
            "missing diversity.{key}"
        );
    }
    let back: GenerateResponse = serde_json::from_value(wire).unwrap();
    assert_eq!(back, response);
}

#[test]
fn debug_hides_the_backends() {
    assert!(format!("{:?}", engine(backend())).starts_with("Engine"));
}
