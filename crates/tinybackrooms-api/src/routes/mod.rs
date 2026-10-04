//! The HTTP routes.
//!
//! | Method | Path | Behavior |
//! | --- | --- | --- |
//! | `GET` | `/healthz` | Liveness: `{"status": "ok"}`. |
//! | `POST` | `/v1/generate` | A `GenerateRequest` in, a `GenerateResponse` out. |
//! | `POST` | `/v1/generate/stream` | The same request, answered as server-sent events. |
//!
//! Bodies are decoded here rather than by an axum extractor so every failure,
//! including malformed JSON, uses the single error shape in `error`.

mod error;
mod stream;

use axum::body::Bytes;
use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};
use tinybackrooms::{Engine, GenerateRequest, GenerateResponse};

use error::ApiError;

/// Build the router serving `engine`.
pub fn router(engine: Engine) -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/v1/generate", post(generate))
        .route("/v1/generate/stream", post(stream::stream))
        .with_state(engine)
}

fn parse(body: &Bytes) -> Result<GenerateRequest, ApiError> {
    serde_json::from_slice(body).map_err(|error| ApiError::invalid_json(error.to_string()))
}

async fn health() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

async fn generate(
    State(engine): State<Engine>,
    body: Bytes,
) -> Result<Json<GenerateResponse>, ApiError> {
    let request = parse(&body)?;
    Ok(Json(engine.generate(&request).await?))
}

#[cfg(test)]
mod test;
