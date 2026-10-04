//! The LLM judge: independent raters score novelty and usefulness.
//!
//! Following the consensual assessment technique, each item is rated by several
//! independent calls rather than one. An item's score is the mean over raters of
//! `novelty × usefulness`, each normalized to `[0, 1]`: the product means an
//! idea that is merely strange, or merely useful, cannot rank well, which is the
//! quality gate on novelty. The spread between raters is reported as
//! disagreement. Raters also label the idea with one of Boden's three kinds of
//! creativity, and the majority label is kept.

use futures::future::join_all;
use serde_json::{Value, json};

use super::Candidate;
use super::math::count;
use crate::{BodenClass, Conversant, Message, Seat, StructuredRequest, Usage};

const TOOL: &str = "rate_item";

/// The tool schema raters answer with.
fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "novelty": {"type": "integer", "minimum": 1, "maximum": 7},
            "usefulness": {"type": "integer", "minimum": 1, "maximum": 7},
            "boden": {"type": "string", "enum": ["combinational", "exploratory", "transformational"]}
        },
        "required": ["novelty", "usefulness", "boden"]
    })
}

const SYSTEM: &str = "You are one of several independent raters judging ideas for creativity. \
Rate the idea from 1 to 7 on two scales and call the `rate_item` tool once. \
novelty: 1 means a stock answer anyone would give, 7 means nothing like it exists. \
usefulness: 1 means it cannot serve the problem, 7 means it solves it well. \
boden: combinational if it combines familiar things in a new way, exploratory if it makes a \
new move inside an established style, transformational if it changes the rules of the space.";

/// One judged item.
#[derive(Debug)]
pub(super) struct Judged {
    /// Mean of `novelty × usefulness` over raters, in `[0, 1]`; `0` if no
    /// rater produced a usable rating.
    pub(super) score: f64,
    /// Population standard deviation of the raters' scores.
    pub(super) disagreement: f64,
    /// The majority Boden label.
    pub(super) boden: Option<BodenClass>,
    /// Cost of all rater calls.
    pub(super) usage: Usage,
    /// Raters whose call failed or whose answer was unusable.
    pub(super) failed_raters: u32,
}

struct Rating {
    score: f64,
    boden: Option<BodenClass>,
}

fn parse(value: &Value) -> Option<Rating> {
    let scale = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_f64)
            .filter(|v| (1.0..=7.0).contains(v))
            .map(|v| (v - 1.0) / 6.0)
    };
    Some(Rating {
        score: scale("novelty")? * scale("usefulness")?,
        boden: value
            .get("boden")
            .and_then(|b| serde_json::from_value(b.clone()).ok()),
    })
}

/// Rate every candidate with `raters` independent calls each.
pub(super) async fn judge_all(
    brief: &str,
    candidates: &[Candidate],
    raters: u32,
    model: Option<&str>,
    backend: &dyn Conversant,
) -> Vec<Judged> {
    join_all(
        candidates
            .iter()
            .map(|c| judge_one(brief, c, raters, model, backend)),
    )
    .await
}

async fn judge_one(
    brief: &str,
    candidate: &Candidate,
    raters: u32,
    model: Option<&str>,
    backend: &dyn Conversant,
) -> Judged {
    let prompt = format!("PROBLEM\n{brief}\n\nIDEA\n{}", candidate.text);
    let replies = join_all((0..raters.max(1)).map(|rater| {
        backend.structured(StructuredRequest {
            seat: Seat {
                run_id: candidate.run_id.clone(),
                role: format!("judge_{rater}"),
            },
            model: model.map(str::to_owned),
            system: SYSTEM.to_owned(),
            messages: vec![Message::user(prompt.clone())],
            tool: TOOL.to_owned(),
            schema: schema(),
        })
    }))
    .await;

    let mut usage = Usage::default();
    let mut ratings = Vec::new();
    let mut failed_raters = 0_u32;
    for reply in replies {
        match reply {
            Ok(reply) => {
                usage.add(reply.usage);
                match parse(&reply.value) {
                    Some(rating) => ratings.push(rating),
                    None => failed_raters += 1,
                }
            }
            Err(_) => failed_raters += 1,
        }
    }

    let scores: Vec<f64> = ratings.iter().map(|r| r.score).collect();
    let (score, disagreement) = if scores.is_empty() {
        (0.0, 0.0)
    } else {
        let mean = scores.iter().sum::<f64>() / count(scores.len());
        let variance = scores.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / count(scores.len());
        (mean, variance.sqrt())
    };
    Judged {
        score,
        disagreement,
        boden: majority(&ratings),
        usage,
        failed_raters,
    }
}

/// The most common label; ties go to the earlier kind in Boden's ordering.
fn majority(ratings: &[Rating]) -> Option<BodenClass> {
    [
        BodenClass::Combinational,
        BodenClass::Exploratory,
        BodenClass::Transformational,
    ]
    .into_iter()
    .map(|class| {
        (
            ratings.iter().filter(|r| r.boden == Some(class)).count(),
            class,
        )
    })
    .filter(|(n, _)| *n > 0)
    .max_by(|(a, class_a), (b, class_b)| a.cmp(b).then(class_b.cmp(class_a)))
    .map(|(_, class)| class)
}
