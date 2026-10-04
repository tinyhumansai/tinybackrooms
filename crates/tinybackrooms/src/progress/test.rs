//! Unit tests for the progress event wire form.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;

use super::*;

#[test]
fn events_are_tagged_by_type() {
    let event = Event::Turn {
        run_id: "v0".into(),
        index: 1,
        agent: "agent_b".into(),
        text: "hi".into(),
    };
    assert_eq!(
        serde_json::to_value(&event).unwrap(),
        json!({"type": "turn", "run_id": "v0", "index": 1, "agent": "agent_b", "text": "hi"})
    );
}

#[test]
fn every_event_round_trips() {
    let events = [
        Event::VariationStarted {
            run_id: "v0".into(),
        },
        Event::Extracted {
            run_id: "v0".into(),
            valid: 2,
            invalid: 1,
        },
        Event::VariationFailed {
            run_id: "v0".into(),
            reason: "x".into(),
        },
        Event::Scored { items: 3 },
    ];
    for event in events {
        let back: Event = serde_json::from_value(serde_json::to_value(&event).unwrap()).unwrap();
        assert_eq!(back, event);
    }
}

#[test]
fn the_noop_observer_accepts_events() {
    NoopObserver.event(Event::Scored { items: 0 });
}
