//! Unit tests for agent identity.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

fn valid(id: &str) -> bool {
    let mut chars = id.chars();
    id.len() <= 64
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

#[test]
fn the_same_inputs_give_the_same_id() {
    assert_eq!(
        agent_id("agent_a", &["sys", "m"]),
        agent_id("agent_a", &["sys", "m"])
    );
}

#[test]
fn a_different_prompt_or_model_gives_a_different_id() {
    let base = agent_id("agent_a", &["sys", "m"]);
    assert_ne!(base, agent_id("agent_a", &["other", "m"]));
    assert_ne!(base, agent_id("agent_a", &["sys", "n"]));
    assert_ne!(base, agent_id("agent_b", &["sys", "m"]));
}

#[test]
fn ids_satisfy_openhumans_id_rule_whatever_the_role() {
    for role in ["agent_a", "judge_0", "Weird Role!", &"x".repeat(200)] {
        let id = agent_id(role, &["s"]);
        assert!(valid(&id), "{id}");
    }
}

#[test]
fn specs_carry_the_id() {
    assert_eq!(chat_spec("tbr-x-1", "talk", 1.0).id(), "tbr-x-1");
    let def = ToolDef {
        name: "t".into(),
        schema: serde_json::json!({}),
    };
    assert_eq!(
        structured_spec("tbr-y-2", "fmt", &def, Arc::default()).id(),
        "tbr-y-2"
    );
}

#[test]
fn temperatures_that_round_alike_share_a_key() {
    assert_eq!(temperature_key(1.0), "1.00");
    assert_eq!(temperature_key(0.999), temperature_key(1.0));
    assert_ne!(temperature_key(0.7), temperature_key(1.0));
}
