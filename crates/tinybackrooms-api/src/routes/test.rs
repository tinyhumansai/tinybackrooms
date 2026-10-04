//! Unit tests for the HTTP routes, driven in-process without a socket.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{Value, json};
use tinybackrooms::{Error, ScriptedConversant};
use tower::ServiceExt;

use super::*;
use crate::DemoConversant;

fn demo_app() -> Router {
    router(Engine::new(Arc::new(DemoConversant)))
}

fn body() -> Value {
    json!({
        "input": {"brief": "name a cafe"},
        "output": {"schema": {"type": "object", "required": ["name"],
            "properties": {"name": {"type": "string"}}}},
        "config": {"variations": 2, "generations": 2, "items_per_variation": 2}
    })
}

async fn call(
    app: Router,
    method: Method,
    path: &str,
    payload: Option<Vec<u8>>,
) -> (StatusCode, String) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(payload.map_or_else(Body::empty, Body::from))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

async fn post(app: Router, path: &str, payload: &Value) -> (StatusCode, String) {
    call(
        app,
        Method::POST,
        path,
        Some(serde_json::to_vec(payload).unwrap()),
    )
    .await
}

#[tokio::test]
async fn health_reports_ok() {
    let (status, text) = call(demo_app(), Method::GET, "/healthz", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap(),
        json!({"status": "ok"})
    );
}

#[tokio::test]
async fn generate_returns_ranked_items() {
    let (status, text) = post(demo_app(), "/v1/generate", &body()).await;
    assert_eq!(status, StatusCode::OK);
    let response: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(response["items"].as_array().unwrap().len(), 4);
    assert!(response["items"][0]["total"].is_number());
    assert!(response.get("transcripts").is_none());
}

#[tokio::test]
async fn generate_rejects_malformed_json() {
    let (status, text) = call(
        demo_app(),
        Method::POST,
        "/v1/generate",
        Some(b"{nope".to_vec()),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let error: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(error["error"]["code"], "invalid_json");
}

#[tokio::test]
async fn generate_rejects_unknown_fields() {
    let mut payload = body();
    payload["config"]["varations"] = json!(3);
    let (status, text) = post(demo_app(), "/v1/generate", &payload).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(text.contains("varations"));
}

#[tokio::test]
async fn generate_maps_validation_errors_to_400() {
    let mut payload = body();
    payload["input"]["brief"] = json!("  ");
    let (status, text) = post(demo_app(), "/v1/generate", &payload).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let error: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(error["error"]["code"], "invalid_request");
    assert_eq!(error["error"]["message"], "brief must not be empty");
}

#[tokio::test]
async fn generate_maps_no_valid_items_to_422() {
    let wrong =
        ScriptedConversant::new(|_| Ok("x".into()), |_| Ok(json!({"items": [{"nope": 1}]})));
    let app = router(Engine::new(Arc::new(wrong)));
    let (status, text) = post(app, "/v1/generate", &body()).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(text.contains("no_valid_items"));
}

#[tokio::test]
async fn generate_maps_backend_failures_to_502() {
    let down = ScriptedConversant::new(
        |_| Err(Error::Conversant("down".into())),
        |_| Err(Error::Conversant("down".into())),
    );
    let (status, text) = post(router(Engine::new(Arc::new(down))), "/v1/generate", &body()).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert!(text.contains("backend_error"));
}

#[test]
fn error_mapping_covers_every_class() {
    let status = |error: Error| ApiError::from(error).status;
    assert_eq!(
        status(Error::InvalidSchema("x".into())),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        status(Error::TooManyVariations { max: 1 }),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(status(Error::Memory("x".into())), StatusCode::BAD_GATEWAY);
    assert_eq!(
        status(Error::NoValidItems),
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[tokio::test]
async fn unknown_paths_and_methods_are_rejected() {
    assert_eq!(
        call(demo_app(), Method::GET, "/nope", None).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(demo_app(), Method::GET, "/v1/generate", None).await.0,
        StatusCode::METHOD_NOT_ALLOWED
    );
}

fn events(text: &str) -> Vec<(String, Value)> {
    text.split("\n\n")
        .filter(|block| !block.trim().is_empty())
        .map(|block| {
            let name = block
                .lines()
                .find_map(|l| l.strip_prefix("event: "))
                .unwrap();
            let data = block
                .lines()
                .find_map(|l| l.strip_prefix("data: "))
                .unwrap();
            (name.to_owned(), serde_json::from_str(data).unwrap())
        })
        .collect()
}

#[tokio::test]
async fn stream_emits_progress_then_the_result() {
    let (status, text) = post(demo_app(), "/v1/generate/stream", &body()).await;
    assert_eq!(status, StatusCode::OK);
    let events = events(&text);
    let names: Vec<&str> = events.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names.iter().filter(|n| **n == "variation_started").count(),
        2
    );
    assert_eq!(names.iter().filter(|n| **n == "turn").count(), 4);
    assert_eq!(names.iter().filter(|n| **n == "extracted").count(), 2);
    assert!(names.contains(&"scored"));
    assert_eq!(names.last(), Some(&"result"));
    let (_, result) = events.last().unwrap();
    assert_eq!(result["items"].as_array().unwrap().len(), 4);
    assert_eq!(events[0].1["type"], "variation_started");
}

#[tokio::test]
async fn stream_rejects_a_bad_request_before_opening() {
    let mut payload = body();
    payload["config"]["variations"] = json!(0);
    let (status, text) = post(demo_app(), "/v1/generate/stream", &payload).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(text.contains("invalid_request"));
    let (status, _) = call(
        demo_app(),
        Method::POST,
        "/v1/generate/stream",
        Some(b"[".to_vec()),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn stream_reports_a_failed_run_as_an_error_event() {
    let down = ScriptedConversant::new(
        |_| Err(Error::Conversant("down".into())),
        |_| Err(Error::Conversant("down".into())),
    );
    let (status, text) = post(
        router(Engine::new(Arc::new(down))),
        "/v1/generate/stream",
        &body(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let events = events(&text);
    let (name, payload) = events.last().unwrap();
    assert_eq!(name, "error");
    assert_eq!(payload["error"]["code"], "backend_error");
    assert!(events.iter().any(|(n, _)| n == "variation_failed"));
}
