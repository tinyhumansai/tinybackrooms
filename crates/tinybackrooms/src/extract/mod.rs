//! Turning a conversation into typed items.
//!
//! Extraction is the second pass of a variation. The formatter agent reads the
//! finished transcript and answers by calling one typed tool,
//! `submit_items`, whose arguments are `{"items": [<item schema>, ...]}`. The
//! caller's schema is the single source of truth: every returned item is
//! validated against it here, whatever the backend claims to enforce, and items
//! that fail are sent back for a bounded number of repair attempts with the
//! validator's message. Valid items accumulate across attempts.
//!
//! Keeping this pass separate from the conversation is deliberate: exploring
//! under a format constraint costs a model quality, while extracting after the
//! fact costs almost nothing.

use jsonschema::Validator;
use serde_json::{Value, json};

use crate::backrooms::Transcript;
use crate::{Conversant, Message, OutputSpec, Result, RunConfig, Seat, StructuredRequest, Usage};

/// The tool the formatter is asked to call.
pub(crate) const TOOL: &str = "submit_items";

/// The outcome of extracting one variation.
#[derive(Debug)]
pub(crate) struct Extraction {
    /// Items that satisfy the schema, at most `items_per_variation`.
    pub(crate) items: Vec<Value>,
    /// Items still failing the schema after the last repair attempt.
    pub(crate) invalid: u32,
    /// What the formatter calls cost.
    pub(crate) usage: Usage,
}

/// Build the tool schema: an object holding an array of the caller's schema.
///
/// `$defs` and `definitions` are copied to the root so a root-relative `$ref`
/// inside the item schema still resolves once it is nested.
pub(crate) fn tool_schema(output: &OutputSpec, count: u32) -> Value {
    let mut schema = json!({
        "type": "object",
        "properties": {
            "items": {"type": "array", "items": output.schema, "maxItems": count}
        },
        "required": ["items"]
    });
    for key in ["$defs", "definitions"] {
        if let Some(defs) = output.schema.get(key) {
            schema[key] = defs.clone();
        }
    }
    schema
}

fn system_prompt(output: &OutputSpec, count: u32) -> String {
    let mut prompt = format!(
        "You read a conversation between two agents and turn it into {count} distinct, concrete \
         {name} candidates. Prefer the strongest and strangest ones the conversation actually \
         produced or implied; never pad with generic ones and never repeat one. Answer by \
         calling the `{TOOL}` tool exactly once.",
        name = output.item_name,
    );
    if let Some(guidance) = &output.item_guidance {
        prompt.push_str("\n\n");
        prompt.push_str(guidance.trim());
    }
    prompt
}

/// Extract the items of one transcript.
///
/// # Errors
///
/// Returns whatever the [`Conversant`] returns when a formatter call fails.
pub(crate) async fn extract(
    transcript: &Transcript,
    output: &OutputSpec,
    validator: &Validator,
    config: &RunConfig,
    backend: &dyn Conversant,
) -> Result<Extraction> {
    let wanted = usize::try_from(config.items_per_variation).unwrap_or(usize::MAX);
    let schema = tool_schema(output, config.items_per_variation);
    let system = system_prompt(output, config.items_per_variation);
    let mut messages = vec![Message::user(transcript.render())];
    let mut items: Vec<Value> = Vec::new();
    let mut usage = Usage::default();
    let mut rejected: Vec<String> = Vec::new();

    for _attempt in 0..=config.budget.repair_attempts {
        let reply = backend
            .structured(StructuredRequest {
                seat: Seat {
                    run_id: transcript.run_id.clone(),
                    role: "formatter".to_owned(),
                },
                model: config.models.formatter.clone(),
                system: system.clone(),
                messages: messages.clone(),
                tool: TOOL.to_owned(),
                schema: schema.clone(),
            })
            .await?;
        usage.add(reply.usage);

        rejected.clear();
        match reply.value.get("items").and_then(Value::as_array) {
            Some(candidates) => {
                for (position, candidate) in candidates.iter().enumerate() {
                    match validator.iter_errors(candidate).next() {
                        None if !items.contains(candidate) => items.push(candidate.clone()),
                        None => {}
                        Some(error) => rejected.push(format!("item {position}: {error}")),
                    }
                }
            }
            None => rejected.push("the answer must be an object with an `items` array".to_owned()),
        }
        if rejected.is_empty() || items.len() >= wanted {
            break;
        }
        messages.push(Message::assistant(reply.value.to_string()));
        messages.push(Message::user(format!(
            "These items were rejected:\n{}\nCall `{TOOL}` again with corrected versions of the \
             rejected items only.",
            rejected.join("\n")
        )));
    }

    items.truncate(wanted);
    let invalid = if items.len() >= wanted {
        0
    } else {
        rejected.len()
    };
    let invalid = u32::try_from(invalid).unwrap_or(u32::MAX);
    Ok(Extraction {
        items,
        invalid,
        usage,
    })
}

#[cfg(test)]
mod test;
