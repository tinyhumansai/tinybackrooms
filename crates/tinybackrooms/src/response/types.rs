//! Payload types of a generation response.
//!
//! Like the request types, these are wire forms; their field names are pinned by
//! unit tests in the engine.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Transcript, Usage};

/// The answer to a [`GenerateRequest`](crate::GenerateRequest).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenerateResponse {
    /// The selected items, best first. Each satisfies the output schema.
    pub items: Vec<ScoredItem>,
    /// How varied the returned items are as a set.
    pub diversity: Diversity,
    /// The conversations, when `include_transcripts` was set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcripts: Option<Vec<Transcript>>,
    /// Backend calls made across the whole run.
    pub usage: Usage,
    /// Things dropped or degraded: failed variations, items that never passed
    /// the schema, judge calls that failed.
    pub warnings: Vec<String>,
}

/// One returned item with its scores.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoredItem {
    /// The item, conforming to the output schema.
    pub value: Value,
    /// The variation that produced it, such as `v2`.
    pub run_id: String,
    /// Each scorer's score in `[0, 1]`, by scorer name. The `judge` scorer also
    /// reports `judge_disagreement`, the spread between raters.
    pub scores: BTreeMap<String, f64>,
    /// The weighted mean of the scorers that ranked the items, in `[0, 1]`.
    pub total: f64,
    /// The judges' majority view of how the idea is creative, when judged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boden_class: Option<BodenClass>,
}

/// Boden's three kinds of creativity, as labelled by the judge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodenClass {
    /// A new combination of familiar ideas.
    Combinational,
    /// A new move inside an established style or rule space.
    Exploratory,
    /// A change to the rules of the space itself.
    Transformational,
}

/// Set-level diversity of the returned items.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Diversity {
    /// The Vendi score: the effective number of distinct items, between 1 and
    /// the item count (0 for an empty set).
    pub vendi: f64,
    /// Items that stay distinct after merging near-duplicates.
    pub distinct_classes: u32,
    /// How many valid items came back.
    pub fluency: u32,
    /// `distinct_classes / fluency`; 1 means every item is its own idea.
    pub flexibility: f64,
    /// Mean pairwise semantic distance times 100, after the Divergent
    /// Association Task.
    pub dat: f64,
}
