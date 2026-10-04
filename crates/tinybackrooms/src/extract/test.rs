//! Unit tests for extraction, validation and the repair loop.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::*;
use crate::backrooms::{StopReason, Turn};
use crate::{Error, ScriptedConversant};

fn output() -> OutputSpec {
    OutputSpec {
        item_name: "name".into(),
        item_guidance: Some("keep it under four words".into()),
        schema: json!({
            "type": "object",
            "required": ["name"],
            "properties": {"name": {"type": "string"}},
            "additionalProperties": false
        }),
    }
}

fn transcript() -> Transcript {
    Transcript {
        run_id: "v0".into(),
        turns: vec![Turn {
            agent: "agent_a".into(),
            text: "ls".into(),
        }],
        stopped_by: StopReason::Generations,
    }
}

type Requests = Arc<Mutex<Vec<StructuredRequest>>>;

/// A formatter that answers the nth call with `answers[n]` (the last repeats).
fn formatter(answers: Vec<Value>) -> (ScriptedConversant, Requests) {
    let requests: Requests = Arc::default();
    let sink = Arc::clone(&requests);
    let backend = ScriptedConversant::new(
        |_| Ok(String::new()),
        move |request| {
            let mut seen = sink.lock().unwrap();
            seen.push(request.clone());
            Ok(answers[(seen.len() - 1).min(answers.len() - 1)].clone())
        },
    );
    (backend, requests)
}

async fn run_extract(config: &RunConfig, backend: &ScriptedConversant) -> Result<Extraction> {
    let output = output();
    let validator = output.compile().unwrap();
    extract(&transcript(), &output, &validator, config, backend).await
}

fn config(items: u32, repairs: u32) -> RunConfig {
    let mut config = RunConfig {
        items_per_variation: items,
        ..RunConfig::default()
    };
    config.budget.repair_attempts = repairs;
    config
}

#[tokio::test]
async fn accepts_valid_items_and_caps_at_the_requested_count() {
    let (backend, requests) = formatter(vec![json!({"items": [
        {"name": "one"}, {"name": "two"}, {"name": "three"}
    ]})]);
    let extraction = run_extract(&config(2, 2), &backend).await.unwrap();
    assert_eq!(
        extraction.items,
        [json!({"name": "one"}), json!({"name": "two"})]
    );
    assert_eq!(extraction.invalid, 0);
    assert_eq!(extraction.usage.calls, 1);
    assert_eq!(requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn sends_the_transcript_guidance_and_tool_schema() {
    let (backend, requests) = formatter(vec![json!({"items": [{"name": "one"}]})]);
    run_extract(&config(1, 0), &backend).await.unwrap();
    let request = requests.lock().unwrap()[0].clone();
    assert_eq!(request.tool, TOOL);
    assert_eq!(request.seat.role, "formatter");
    assert!(request.system.contains("keep it under four words"));
    assert!(request.messages[0].content.contains("[agent_a]"));
    assert_eq!(request.schema["properties"]["items"]["maxItems"], json!(1));
}

#[tokio::test]
async fn repairs_rejected_items_and_keeps_the_valid_ones() {
    let (backend, requests) = formatter(vec![
        json!({"items": [{"name": "good"}, {"name": 7}]}),
        json!({"items": [{"name": "fixed"}]}),
    ]);
    let extraction = run_extract(&config(2, 2), &backend).await.unwrap();
    assert_eq!(
        extraction.items,
        [json!({"name": "good"}), json!({"name": "fixed"})]
    );
    assert_eq!(extraction.invalid, 0);
    assert_eq!(extraction.usage.calls, 2);

    let repair = &requests.lock().unwrap()[1];
    assert_eq!(repair.messages.len(), 3);
    assert!(repair.messages[2].content.contains("item 1"));
    assert!(repair.messages[1].content.contains("good"));
}

#[tokio::test]
async fn gives_up_after_the_repair_limit_and_reports_what_is_still_invalid() {
    let (backend, _) = formatter(vec![json!({"items": [{"name": "ok"}, {"wrong": 1}]})]);
    let extraction = run_extract(&config(3, 1), &backend).await.unwrap();
    assert_eq!(extraction.items, [json!({"name": "ok"})]);
    assert_eq!(extraction.invalid, 1);
    assert_eq!(extraction.usage.calls, 2);
}

#[tokio::test]
async fn an_answer_without_an_items_array_counts_as_invalid() {
    let (backend, _) = formatter(vec![json!({"nope": true})]);
    let extraction = run_extract(&config(1, 0), &backend).await.unwrap();
    assert_eq!(extraction.items.len(), 0);
    assert_eq!(extraction.invalid, 1);
}

#[tokio::test]
async fn drops_duplicate_items() {
    let (backend, _) = formatter(vec![json!({"items": [{"name": "a"}, {"name": "a"}]})]);
    let extraction = run_extract(&config(3, 0), &backend).await.unwrap();
    assert_eq!(extraction.items, [json!({"name": "a"})]);
}

#[tokio::test]
async fn propagates_a_backend_failure() {
    let failing = ScriptedConversant::new(
        |_| Ok(String::new()),
        |_| Err(Error::Conversant("down".into())),
    );
    let err = run_extract(&config(1, 0), &failing).await.unwrap_err();
    assert_eq!(err, Error::Conversant("down".into()));
}

#[test]
fn hoists_schema_definitions_to_the_tool_schema_root() {
    let output = OutputSpec {
        item_name: "i".into(),
        item_guidance: None,
        schema: json!({
            "$ref": "#/$defs/thing",
            "$defs": {"thing": {"type": "string"}},
            "definitions": {"old": {"type": "integer"}}
        }),
    };
    let schema = tool_schema(&output, 2);
    assert_eq!(schema["$defs"]["thing"], json!({"type": "string"}));
    assert_eq!(schema["definitions"]["old"], json!({"type": "integer"}));
    assert!(
        jsonschema::validator_for(&schema)
            .unwrap()
            .is_valid(&json!({"items": ["x"]}))
    );
}

#[test]
fn the_system_prompt_names_the_item_and_count() {
    let prompt = system_prompt(&output(), 4);
    assert!(prompt.contains("4 distinct") && prompt.contains("name candidates"));
}
