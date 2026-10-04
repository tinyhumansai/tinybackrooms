//! Integration tests against the public API only: the regression suite for the
//! request/response contract.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use serde_json::{Value, json};
use tinybackrooms::{Engine, Error, GenerateRequest, GenerateResponse, ScriptedConversant};

fn backend() -> ScriptedConversant {
    ScriptedConversant::new(
        |turn| Ok(format!("{}: ls ideas/", turn.seat.role)),
        |call| match call.tool.as_str() {
            "rate_item" => Ok(json!({"novelty": 6, "usefulness": 5, "boden": "exploratory"})),
            _ => Ok(json!({"items": [
                {"title": format!("{} drifting archive", call.seat.run_id), "tags": ["a", "b"]},
                {"title": format!("{} tidal ledger", call.seat.run_id), "tags": ["c"]},
            ]})),
        },
    )
}

fn request(config: &Value) -> GenerateRequest {
    serde_json::from_value(json!({
        "input": {
            "brief": "invent a board game about maintaining a lighthouse",
            "seeds": [{"kind": "reference", "text": "Dixit, Hanabi"}],
            "constraints": {"must": ["playable in 30 minutes"], "must_not": ["use dice"]}
        },
        "output": {
            "item_name": "game concept",
            "schema": {
                "type": "object",
                "required": ["title", "tags"],
                "properties": {
                    "title": {"type": "string", "minLength": 3},
                    "tags": {"type": "array", "items": {"type": "string"}, "minItems": 1}
                },
                "additionalProperties": false
            }
        },
        "config": config
    }))
    .unwrap()
}

#[tokio::test]
async fn returns_ranked_items_that_satisfy_the_schema() {
    let engine = Engine::new(Arc::new(backend()));
    let response = engine
        .generate(&request(
            &json!({"variations": 3, "generations": 4, "items_per_variation": 2}),
        ))
        .await
        .unwrap();
    assert_eq!(response.items.len(), 6);
    for item in &response.items {
        assert!(item.value["title"].as_str().unwrap().len() >= 3);
    }
}

#[tokio::test]
async fn judge_scoring_labels_items_with_a_boden_class() {
    let engine = Engine::new(Arc::new(backend()));
    let response = engine
        .generate(&request(&json!({
            "variations": 1, "generations": 2, "items_per_variation": 2,
            "scoring": {"scorers": [{"kind": "judge", "weight": 1.0}], "select_top_k": 1}
        })))
        .await
        .unwrap();
    assert_eq!(response.items.len(), 1);
    let item = serde_json::to_value(&response.items[0]).unwrap();
    assert_eq!(item["boden_class"], json!("exploratory"));
    assert!((item["scores"]["judge"].as_f64().unwrap() - (5.0 / 6.0) * (4.0 / 6.0)).abs() < 1e-9);
}

#[tokio::test]
async fn the_response_survives_a_json_round_trip() {
    let engine = Engine::new(Arc::new(backend()));
    let response = engine
        .generate(&request(
            &json!({"include_transcripts": true, "variations": 2, "generations": 2}),
        ))
        .await
        .unwrap();
    let wire = serde_json::to_string(&response).unwrap();
    let back: GenerateResponse = serde_json::from_str(&wire).unwrap();
    assert_eq!(back, response);
    assert_eq!(back.transcripts.unwrap().len(), 2);
}

#[tokio::test]
async fn rejects_a_request_whose_schema_is_invalid() {
    let mut bad = request(&json!({}));
    bad.output.schema = json!({"type": "not-a-type"});
    let err = Engine::new(Arc::new(backend()))
        .generate(&bad)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::InvalidSchema(_)));
}
