//! Unit tests for the conversant port types and the scripted backend.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;

use super::*;
use crate::Error;

fn seat() -> Seat {
    Seat {
        run_id: "v0".into(),
        role: "agent_a".into(),
    }
}

#[test]
fn builds_messages_with_the_right_role() {
    assert_eq!(Message::user("hi").role, Role::User);
    assert_eq!(Message::assistant("yo").role, Role::Assistant);
}

#[test]
fn adds_usage() {
    let mut total = Usage::default();
    total.add(Usage {
        input_tokens: 3,
        output_tokens: 4,
        calls: 1,
    });
    total.add(Usage {
        input_tokens: 1,
        output_tokens: 1,
        calls: 1,
    });
    assert_eq!(
        total,
        Usage {
            input_tokens: 4,
            output_tokens: 5,
            calls: 2
        }
    );
}

#[tokio::test]
async fn scripted_turn_echoes_the_closure() {
    let backend = ScriptedConversant::new(
        |r| Ok(format!("{} said {}", r.seat.role, r.messages.len())),
        |_| Ok(json!(null)),
    );
    let reply = backend
        .turn(TurnRequest {
            seat: seat(),
            model: None,
            temperature: 1.0,
            system: "s".into(),
            messages: vec![Message::user("x")],
        })
        .await
        .unwrap();
    assert_eq!(reply.text, "agent_a said 1");
    assert_eq!(reply.usage.calls, 1);
}

#[tokio::test]
async fn scripted_structured_returns_the_closure_value() {
    let backend = ScriptedConversant::new(|_| Ok(String::new()), |r| Ok(json!({"tool": r.tool})));
    let reply = backend
        .structured(StructuredRequest {
            seat: seat(),
            model: None,
            system: "s".into(),
            messages: vec![],
            tool: "submit_items".into(),
            schema: json!(true),
        })
        .await
        .unwrap();
    assert_eq!(reply.value, json!({"tool": "submit_items"}));
}

#[tokio::test]
async fn scripted_propagates_backend_errors() {
    let backend = ScriptedConversant::new(
        |_| Err(Error::Conversant("down".into())),
        |_| Err(Error::Conversant("down".into())),
    );
    let turn = backend
        .turn(TurnRequest {
            seat: seat(),
            model: None,
            temperature: 0.0,
            system: String::new(),
            messages: vec![],
        })
        .await;
    assert_eq!(turn, Err(Error::Conversant("down".into())));
    let structured = backend
        .structured(StructuredRequest {
            seat: seat(),
            model: None,
            system: String::new(),
            messages: vec![],
            tool: "t".into(),
            schema: json!(true),
        })
        .await;
    assert_eq!(structured.unwrap_err(), Error::Conversant("down".into()));
}

#[test]
fn debug_does_not_expose_closures() {
    let backend = ScriptedConversant::new(|_| Ok(String::new()), |_| Ok(json!(null)));
    assert!(format!("{backend:?}").starts_with("ScriptedConversant"));
}
