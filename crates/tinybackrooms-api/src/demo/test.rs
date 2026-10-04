//! Unit tests for the demo backend and its schema sampler.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;
use std::sync::Arc;

use tinybackrooms::{Engine, GenerateRequest, Message, Seat};

use super::*;

#[test]
fn samples_each_scalar_type() {
    assert!(
        sample(&json!({"type": "string"}), "s", 0)
            .as_str()
            .unwrap()
            .contains("sample idea s")
    );
    assert_eq!(
        sample(&json!({"type": "integer", "minimum": 3}), "s", 0),
        json!(3)
    );
    assert_eq!(sample(&json!({"type": "integer"}), "s", 0), json!(0));
    assert_eq!(
        sample(&json!({"type": "number", "minimum": 1.5}), "s", 0),
        json!(1.5)
    );
    assert_eq!(sample(&json!({"type": "boolean"}), "s", 0), json!(true));
    assert_eq!(sample(&json!({"type": "null"}), "s", 0), Value::Null);
}

#[test]
fn prefers_const_and_enum() {
    assert_eq!(sample(&json!({"const": 7}), "s", 0), json!(7));
    assert_eq!(sample(&json!({"enum": ["a", "b"]}), "s", 0), json!("a"));
}

#[test]
fn honours_min_length() {
    let text = sample(&json!({"type": "string", "minLength": 40}), "s", 0);
    assert!(text.as_str().unwrap().chars().count() >= 40);
}

#[test]
fn samples_objects_and_arrays_recursively() {
    let schema = json!({
        "type": "object",
        "required": ["title", "tags"],
        "properties": {
            "title": {"type": "string"},
            "tags": {"type": "array", "items": {"type": "string"}, "minItems": 1}
        }
    });
    let value = sample(&schema, "v1", 0);
    assert!(value["title"].as_str().unwrap().contains("v1 title"));
    assert_eq!(value["tags"].as_array().unwrap().len(), 2);
}

#[test]
fn respects_max_items_and_min_items() {
    let capped = sample(
        &json!({"type": "array", "items": {"type": "integer"}, "maxItems": 1}),
        "s",
        0,
    );
    assert_eq!(capped.as_array().unwrap().len(), 1);
    let floor = sample(
        &json!({"type": "array", "items": {"type": "integer"}, "minItems": 4}),
        "s",
        0,
    );
    assert_eq!(floor.as_array().unwrap().len(), 4);
}

#[test]
fn gives_up_on_runaway_nesting() {
    assert_eq!(
        sample(&json!({"type": "string"}), "s", MAX_DEPTH + 1),
        Value::Null
    );
}

#[test]
fn different_salts_give_different_strings() {
    let schema = json!({"type": "string"});
    assert_ne!(sample(&schema, "v0", 0), sample(&schema, "v1", 0));
}

#[tokio::test]
async fn the_demo_backend_answers_both_call_kinds() {
    let seat = Seat {
        run_id: "v2".into(),
        role: "agent_a".into(),
    };
    let turn = DemoConversant
        .turn(TurnRequest {
            seat: seat.clone(),
            model: None,
            temperature: 1.0,
            system: String::new(),
            messages: vec![Message::user("go")],
        })
        .await
        .unwrap();
    assert!(turn.text.starts_with("agent_a (turn 1)"));

    let structured = DemoConversant
        .structured(StructuredRequest {
            seat,
            model: None,
            system: String::new(),
            messages: vec![],
            tool: "submit_items".into(),
            schema: json!({"type": "object", "properties": {
                "items": {"type": "array", "items": {"type": "string"}, "maxItems": 2}}}),
        })
        .await
        .unwrap();
    assert_eq!(structured.value["items"].as_array().unwrap().len(), 2);
    assert_eq!(structured.usage.calls, 1);
}

#[tokio::test]
async fn the_engine_accepts_the_demo_output_for_a_nested_schema() {
    let request: GenerateRequest = serde_json::from_value(json!({
        "input": {"brief": "anything"},
        "output": {"schema": {
            "type": "object",
            "required": ["title", "tags", "stats"],
            "properties": {
                "title": {"type": "string", "minLength": 5},
                "kind": {"enum": ["game", "tool"]},
                "tags": {"type": "array", "items": {"type": "string"}, "minItems": 1},
                "stats": {"type": "object", "required": ["score"],
                    "properties": {"score": {"type": "integer", "minimum": 1}}}
            },
            "additionalProperties": false
        }},
        "config": {"variations": 2, "generations": 2, "items_per_variation": 2}
    }))
    .unwrap();
    let response = Engine::new(Arc::new(DemoConversant))
        .generate(&request)
        .await
        .unwrap();
    assert_eq!(response.items.len(), 4);
    assert_eq!(response.warnings.len(), 0);
}
