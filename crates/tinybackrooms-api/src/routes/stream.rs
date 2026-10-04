//! `POST /v1/generate/stream`: the same request as `/v1/generate`, answered as
//! server-sent events.
//!
//! The stream carries one event per engine progress event, named by its `type`
//! (`variation_started`, `turn`, `extracted`, `variation_failed`, `scored`),
//! then a final `result` event holding the full response, or an `error` event
//! holding the usual error body. Request validation happens before the stream
//! opens, so a bad request is an ordinary `400`, not an `error` event.
//!
//! Dropping the connection aborts the run: the engine future is cancelled
//! rather than left spending model calls nobody will read.

use std::convert::Infallible;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::response::sse::{Event as Sse, KeepAlive};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use tinybackrooms::{Engine, Event, GenerateResponse, Observer, Result as EngineResult};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::UnboundedReceiverStream;

use super::error::ApiError;
use super::parse;

/// What travels from the run to the response stream.
enum Message {
    Progress(Event),
    Done(EngineResult<GenerateResponse>),
}

/// Forwards engine events into the channel feeding the stream.
struct ChannelObserver(UnboundedSender<Message>);

impl Observer for ChannelObserver {
    fn event(&self, event: Event) {
        // A closed channel means the client left; the run is about to be aborted.
        let _ = self.0.send(Message::Progress(event));
    }
}

/// Aborts the run when the response stream is dropped.
struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn sse<T: Serialize>(name: &str, payload: &T) -> Sse {
    Sse::default()
        .event(name)
        .json_data(payload)
        .unwrap_or_else(|_| Sse::default().event("error").data("unserializable event"))
}

fn render(message: &Message) -> Sse {
    match message {
        Message::Progress(event) => {
            let name = serde_json::to_value(event)
                .ok()
                .and_then(|v| v.get("type").and_then(|t| t.as_str().map(str::to_owned)))
                .unwrap_or_else(|| "progress".to_owned());
            sse(&name, event)
        }
        Message::Done(Ok(response)) => sse("result", response),
        Message::Done(Err(error)) => sse("error", &ApiError::from(error.clone()).body()),
    }
}

pub(super) async fn stream(
    State(engine): State<Engine>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request = parse(&body)?;
    request.validate()?;
    let (sender, receiver) = unbounded_channel();
    let observer: Arc<dyn Observer> = Arc::new(ChannelObserver(sender.clone()));
    let run = tokio::spawn(async move {
        let result = engine.with_observer(observer).generate(&request).await;
        let _ = sender.send(Message::Done(result));
    });
    let guard = AbortOnDrop(run);
    let events = UnboundedReceiverStream::new(receiver).map(move |message| {
        let _keep_alive_until_dropped = &guard;
        Ok::<_, Infallible>(render(&message))
    });
    Ok(axum::response::Sse::new(events)
        .keep_alive(KeepAlive::default())
        .into_response())
}
