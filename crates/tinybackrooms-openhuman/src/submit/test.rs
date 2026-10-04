//! Unit tests for the submit tool and its slots.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;

use super::*;

fn def() -> ToolDef {
    ToolDef {
        name: "submit_items".into(),
        schema: json!({"type": "object"}),
    }
}

#[test]
fn slots_open_get_and_close() {
    let slots = Slots::default();
    assert!(slots.get("s1").is_none());
    let slot = slots.open("s1");
    assert!(Arc::ptr_eq(&slot, &slots.get("s1").unwrap()));
    slots.close("s1");
    assert!(slots.get("s1").is_none());
}

#[test]
fn slots_for_different_sessions_do_not_share_state() {
    let slots = Slots::default();
    let (a, b) = (slots.open("a"), slots.open("b"));
    *a.lock().unwrap() = Some(json!(1));
    assert!(b.lock().unwrap().is_none());
}

#[tokio::test]
async fn the_tool_records_its_arguments() {
    let slots = Slots::default();
    let slot = slots.open("s");
    let tool = SubmitTool::new(def(), slots.get("s"));
    assert_eq!(tool.name(), "submit_items");
    assert_eq!(tool.parameters_schema(), json!({"type": "object"}));
    assert!(tool.description().contains("final answer"));
    let result = tool.execute(json!({"items": [1]})).await.unwrap();
    assert!(!result.is_error);
    assert_eq!(slot.lock().unwrap().clone(), Some(json!({"items": [1]})));
}

#[tokio::test]
async fn the_tool_refuses_when_nothing_is_waiting() {
    let result = SubmitTool::new(def(), None)
        .execute(json!({}))
        .await
        .unwrap();
    assert!(result.is_error);
}
