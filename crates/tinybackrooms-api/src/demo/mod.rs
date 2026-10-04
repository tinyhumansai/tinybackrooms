//! A credential-free backend for demos and plumbing checks.
//!
//! [`DemoConversant`] answers conversation turns with canned text and answers
//! typed requests by building a value that satisfies the requested JSON Schema.
//! Nothing here is creative: it exists so the server, the streaming endpoint
//! and client code can be exercised end to end without a model. Real runs use
//! a real backend.

use serde_json::{Map, Value, json};
use tinybackrooms::{
    BoxFuture, Conversant, Result, StructuredReply, StructuredRequest, TurnReply, TurnRequest,
    Usage,
};

/// How many array elements a generated value holds, unless the schema asks
/// for more or fewer.
const MAX_ARRAY: usize = 2;

/// Deepest schema nesting the generator follows; deeper values become `null`.
const MAX_DEPTH: usize = 8;

/// A [`Conversant`] that fabricates schema-valid output.
#[derive(Debug, Clone, Copy, Default)]
pub struct DemoConversant;

impl Conversant for DemoConversant {
    fn turn(&self, request: TurnRequest) -> BoxFuture<'_, Result<TurnReply>> {
        let text = format!(
            "{} (turn {}) wanders a corridor of half-formed ideas",
            request.seat.role,
            request.messages.len()
        );
        Box::pin(async move {
            Ok(TurnReply {
                text,
                usage: Usage {
                    calls: 1,
                    ..Usage::default()
                },
            })
        })
    }

    fn structured(&self, request: StructuredRequest) -> BoxFuture<'_, Result<StructuredReply>> {
        let value = sample(&request.schema, &request.seat.run_id, 0);
        Box::pin(async move {
            Ok(StructuredReply {
                value,
                usage: Usage {
                    calls: 1,
                    ..Usage::default()
                },
            })
        })
    }
}

/// Build a value satisfying `schema`. Strings carry `salt` so different
/// variations produce different items.
#[must_use]
pub fn sample(schema: &Value, salt: &str, depth: usize) -> Value {
    if depth > MAX_DEPTH {
        return Value::Null;
    }
    if let Some(constant) = schema.get("const") {
        return constant.clone();
    }
    if let Some(first) = schema
        .get("enum")
        .and_then(Value::as_array)
        .and_then(|e| e.first())
    {
        return first.clone();
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("object") => sample_object(schema, salt, depth),
        Some("array") => sample_array(schema, salt, depth),
        Some("integer") => json!(schema.get("minimum").and_then(Value::as_i64).unwrap_or(0)),
        Some("number") => json!(schema.get("minimum").and_then(Value::as_f64).unwrap_or(0.0)),
        Some("boolean") => json!(true),
        Some("null") => Value::Null,
        _ => sample_string(schema, salt),
    }
}

fn sample_string(schema: &Value, salt: &str) -> Value {
    let min = schema.get("minLength").and_then(Value::as_u64).unwrap_or(0);
    let mut text = format!("sample idea {salt}");
    while u64::try_from(text.chars().count()).unwrap_or(u64::MAX) < min {
        text.push('!');
    }
    Value::String(text)
}

fn sample_object(schema: &Value, salt: &str, depth: usize) -> Value {
    let mut out = Map::new();
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (name, property) in properties {
            out.insert(
                name.clone(),
                sample(property, &format!("{salt} {name}"), depth + 1),
            );
        }
    }
    Value::Object(out)
}

fn sample_array(schema: &Value, salt: &str, depth: usize) -> Value {
    let min = schema.get("minItems").and_then(Value::as_u64).unwrap_or(0);
    let max = schema
        .get("maxItems")
        .and_then(Value::as_u64)
        .unwrap_or(u64::MAX);
    let length = usize::try_from(u64::try_from(MAX_ARRAY).unwrap_or(2).max(min).min(max))
        .unwrap_or(MAX_ARRAY);
    let items = schema.get("items").cloned().unwrap_or(Value::Null);
    Value::Array(
        (0..length)
            .map(|index| sample(&items, &format!("{salt} #{index}"), depth + 1))
            .collect(),
    )
}

#[cfg(test)]
mod test;
