//! Real `OpenHuman` agents against a fake OpenAI-compatible provider.
//!
//! `OpenHuman` allows one `Runtime` per process, so every scenario shares one
//! conversant and they run in order inside a single test. The fake provider's
//! behavior is chosen by markers in the last user message: `[[error]]`,
//! `[[slow]]`, `[[no-tool]]`, `[[empty-once]]`, `[[empty-always]]`, and
//! `[[id:N]]`, which it echoes into tool output so concurrent calls can be told
//! apart.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use tinybackrooms::{
    Conversant, Engine, Error, GenerateRequest, Message, Seat, StructuredRequest, TurnRequest,
};
use tinybackrooms_openhuman::{OpenHumanConfig, OpenHumanConversant};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// Answers chat-completions like a model that follows instructions.
#[derive(Clone, Default)]
struct FakeModel {
    seen: Arc<Mutex<Vec<Value>>>,
    next: Arc<AtomicUsize>,
    emptied: Arc<AtomicBool>,
}

fn text_of(row: &Value) -> String {
    match &row["content"] {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts.iter().filter_map(|p| p["text"].as_str()).collect(),
        _ => String::new(),
    }
}

fn completion(message: &Value, finish: &str) -> Value {
    json!({
        "id": "fake", "object": "chat.completion", "created": 1_700_000_000, "model": "fake",
        "choices": [{"index": 0, "message": message, "finish_reason": finish}],
        "usage": {"prompt_tokens": 11, "completion_tokens": 3, "total_tokens": 14},
    })
}

impl Respond for FakeModel {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
        self.seen.lock().unwrap().push(body.clone());
        let n = self.next.fetch_add(1, Ordering::SeqCst);
        let messages = body["messages"].as_array().cloned().unwrap_or_default();
        let last = messages.last().cloned().unwrap_or(Value::Null);
        let last_text = text_of(&last);
        let marker = messages
            .iter()
            .map(text_of)
            .find_map(|t| {
                t.split("[[id:")
                    .nth(1)
                    .map(|r| r.split("]]").next().unwrap_or("").to_owned())
            })
            .unwrap_or_default();

        if last["role"] == "tool" {
            return ResponseTemplate::new(200).set_body_json(completion(
                &json!({"role": "assistant", "content": "Submitted."}),
                "stop",
            ));
        }
        if last_text.contains("[[error]]") {
            return ResponseTemplate::new(400).set_body_string("model rejected the request");
        }
        let empty = last_text.contains("[[empty-always]]")
            || (last_text.contains("[[empty-once]]") && !self.emptied.swap(true, Ordering::SeqCst));
        if empty {
            return ResponseTemplate::new(200).set_body_json(completion(
                &json!({"role": "assistant", "content": ""}),
                "stop",
            ));
        }
        let tool = body["tools"]
            .as_array()
            .and_then(|tools| {
                tools.iter().find_map(|t| {
                    t["function"]["name"]
                        .as_str()
                        .filter(|n| matches!(*n, "submit_items" | "rate_item"))
                })
            })
            .map(str::to_owned);
        let slow = last_text.contains("[[slow]]");
        let template = match tool {
            Some(name) if !last_text.contains("[[no-tool]]") => {
                let args = if name == "rate_item" {
                    json!({"novelty": 6, "usefulness": 5, "boden": "exploratory"})
                } else {
                    json!({"items": [
                        {"name": format!("idea {n} {marker}")},
                        {"name": format!("idea {n}b {marker}")},
                    ]})
                };
                let call = json!({"id": format!("call-{n}"), "type": "function",
                    "function": {"name": name, "arguments": args.to_string()}});
                completion(
                    &json!({"role": "assistant", "content": null, "tool_calls": [call]}),
                    "tool_calls",
                )
            }
            _ => completion(
                &json!({"role": "assistant", "content": format!("turn reply {n}")}),
                "stop",
            ),
        };
        let response = ResponseTemplate::new(200).set_body_json(template);
        if slow {
            response.set_delay(Duration::from_secs(8))
        } else {
            response
        }
    }
}

fn seat(role: &str) -> Seat {
    Seat {
        run_id: "v0".into(),
        role: role.into(),
    }
}

fn extract_schema() -> Value {
    json!({"type": "object", "required": ["items"], "properties": {"items": {
        "type": "array", "items": {"type": "object", "required": ["name"],
        "properties": {"name": {"type": "string"}}}}}})
}

fn structured(marker: &str) -> StructuredRequest {
    StructuredRequest {
        seat: seat("formatter"),
        model: None,
        system: "You turn conversations into items.".into(),
        messages: vec![Message::user(format!("Transcript. [[id:{marker}]]"))],
        tool: "submit_items".into(),
        schema: extract_schema(),
    }
}

fn turn(messages: Vec<Message>) -> TurnRequest {
    TurnRequest {
        seat: seat("agent_a"),
        model: None,
        temperature: 1.0,
        system: "SYSTEM-A you are a terminal".into(),
        messages,
    }
}

async fn scenarios() {
    let model = FakeModel::default();
    let provider = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(model.clone())
        .mount(&provider)
        .await;
    let config = OpenHumanConfig::new(
        format!("{}/v1", provider.uri()),
        "fixture-key",
        "fake-model",
    )
    .with_turn_timeout(Duration::from_secs(4));
    let conversant = Arc::new(OpenHumanConversant::new(config.clone()).await.unwrap());

    // A second runtime in the same process is refused, and the failure says so.
    let second = OpenHumanConversant::new(config).await.unwrap_err();
    assert!(
        matches!(&second, Error::Conversant(m) if m.contains("OpenHuman runtime")),
        "{second}"
    );

    turn_scenario(&model, &conversant).await;
    structured_scenarios(&model, &conversant).await;
    failure_scenarios(&conversant).await;
    engine_scenario(&conversant).await;
}

/// Free text, with the engine's history and the seat's system prompt.
async fn turn_scenario(model: &FakeModel, conversant: &Arc<OpenHumanConversant>) {
    let reply = conversant
        .turn(turn(vec![
            Message::user("hello"),
            Message::assistant("hi there"),
            Message::user("go on"),
        ]))
        .await
        .unwrap();
    assert!(reply.text.starts_with("turn reply"), "{}", reply.text);
    assert_eq!(reply.usage.calls, 1);
    let sent = model.seen.lock().unwrap().last().cloned().unwrap();
    let rows: Vec<String> = sent["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(text_of)
        .collect();
    // The seat's system prompt is the whole system message: OpenHuman's own
    // persona (SOUL.md, ROLE.md, ...) must not be wrapped around it.
    let system = &sent["messages"][0];
    assert_eq!(system["role"], "system");
    assert_eq!(text_of(system), "SYSTEM-A you are a terminal");
    assert!(
        rows.iter()
            .all(|t| !t.contains("SOUL.md") && !t.contains("Master Agent")),
        "{rows:?}"
    );
    // The per-turn temperature is honoured, not the harness default of 0.4.
    assert!(
        (sent["temperature"].as_f64().unwrap() - 1.0).abs() < 1e-6,
        "{}",
        sent["temperature"]
    );
    let position = |needle: &str| rows.iter().position(|t| t.contains(needle)).unwrap();
    assert!(position("hello") < position("hi there") && position("hi there") < position("go on"));
    assert_eq!(sent["model"], "fake-model");
    assert!(
        sent["tools"]
            .as_array()
            .is_none_or(|t| t.iter().all(|x| x["function"]["name"] != "submit_items"))
    );
}

/// The tool's arguments come back, its schema went on the wire, and concurrent
/// typed calls never see each other's answers.
async fn structured_scenarios(model: &FakeModel, conversant: &Arc<OpenHumanConversant>) {
    let answer = conversant.structured(structured("solo")).await.unwrap();
    assert_eq!(answer.value["items"].as_array().unwrap().len(), 2);
    let sent = model
        .seen
        .lock()
        .unwrap()
        .iter()
        .find(|b| b["tools"].is_array())
        .cloned()
        .unwrap();
    let wire = sent["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["function"]["name"] == "submit_items")
        .unwrap();
    assert_eq!(wire["function"]["parameters"]["required"], json!(["items"]));

    let calls = (0..4).map(|i| {
        let conversant = Arc::clone(conversant);
        tokio::spawn(async move {
            let marker = format!("c{i}");
            (
                marker.clone(),
                conversant.structured(structured(&marker)).await.unwrap(),
            )
        })
    });
    for call in calls {
        let (marker, reply) = call.await.unwrap();
        for item in reply.value["items"].as_array().unwrap() {
            assert!(
                item["name"].as_str().unwrap().ends_with(&marker),
                "{item} vs {marker}"
            );
        }
    }
}

/// Each way a turn can fail is reported as a backend error.
async fn failure_scenarios(conversant: &Arc<OpenHumanConversant>) {
    let refused = conversant
        .structured(structured("[[no-tool]]"))
        .await
        .unwrap_err();
    assert!(
        matches!(&refused, Error::Conversant(m) if m.contains("never called")),
        "{refused}"
    );
    let broken = conversant
        .turn(turn(vec![Message::user("[[error]]")]))
        .await
        .unwrap_err();
    assert!(
        matches!(&broken, Error::Conversant(m) if m.contains("turn failed")),
        "{broken}"
    );
    let slow = conversant
        .turn(turn(vec![Message::user("[[slow]]")]))
        .await
        .unwrap_err();
    assert!(
        matches!(&slow, Error::Conversant(m) if m.contains("timed out")),
        "{slow}"
    );
    // A provider that answers with nothing once is retried; one that always
    // does is reported after the retries run out.
    let recovered = conversant
        .turn(turn(vec![Message::user("[[empty-once]]")]))
        .await
        .unwrap();
    assert!(
        recovered.text.starts_with("turn reply"),
        "{}",
        recovered.text
    );
    let hopeless = conversant
        .turn(turn(vec![Message::user("[[empty-always]]")]))
        .await
        .unwrap_err();
    assert!(
        matches!(&hopeless, Error::Conversant(m) if m.contains("empty response")),
        "{hopeless}"
    );
    let empty = conversant.turn(turn(vec![])).await.unwrap_err();
    assert!(
        matches!(&empty, Error::Conversant(m) if m.contains("at least one message")),
        "{empty}"
    );
}

/// The whole engine on `OpenHuman` agents.
async fn engine_scenario(conversant: &Arc<OpenHumanConversant>) {
    let request: GenerateRequest = serde_json::from_value(json!({
        "input": {"brief": "name a lighthouse cafe"},
        "output": {"item_name": "name", "schema": {"type": "object", "required": ["name"],
            "properties": {"name": {"type": "string"}}}},
        "config": {"variations": 2, "generations": 2, "items_per_variation": 2,
            "scoring": {"scorers": [{"kind": "judge", "weight": 1.0}], "judge_raters": 1}}
    }))
    .unwrap();
    let backend: Arc<dyn Conversant> = conversant.clone();
    let response = Engine::new(backend).generate(&request).await.unwrap();
    assert_eq!(response.items.len(), 4, "{:?}", response.warnings);
    assert!(response.warnings.is_empty(), "{:?}", response.warnings);
    assert!(response.usage.input_tokens > 0);
    assert!(response.items.iter().all(|i| i.boden_class.is_some()));
}

#[test]
fn openhuman_agents_run_the_engine() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(16 * 1024 * 1024)
        .build()
        .unwrap();
    runtime.block_on(async { tokio::spawn(scenarios()).await.unwrap() });
}
