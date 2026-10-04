//! Payload types for a generation request: input, output spec, and config.
//!
//! These are the wire form of `POST /v1/generate`. Field names and enum
//! spellings are pinned by unit tests, because a client and the server that
//! disagree about one fail at runtime with a decode error. Every struct rejects
//! unknown fields, so a misspelled option is an error rather than a silent
//! default.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The single request the engine accepts: what to be creative about, what shape
/// the answers take, and how hard to try.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerateRequest {
    /// All the context the agents work from.
    pub input: Context,
    /// The shape every returned item must have.
    pub output: OutputSpec,
    /// Variation count, conversation length, scoring, and budgets.
    #[serde(default)]
    pub config: RunConfig,
}

/// Everything the agents are told about the problem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Context {
    /// The creative problem, in the caller's words.
    pub brief: String,
    /// Material to draw on: notes, references, earlier ideas.
    #[serde(default)]
    pub seeds: Vec<Seed>,
    /// Rules every idea must respect.
    #[serde(default)]
    pub constraints: Constraints,
    /// When set, accepted items are stored under this namespace and recalled as
    /// prior ideas on later runs, which also penalizes repeating them.
    #[serde(default)]
    pub memory_namespace: Option<String>,
}

/// One piece of seed material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seed {
    /// What the material is, which decides how the agents are told to use it.
    pub kind: SeedKind,
    /// The material itself.
    pub text: String,
}

/// How a [`Seed`] is presented to the agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeedKind {
    /// Background to build on.
    Text,
    /// An external source to borrow structure or vocabulary from.
    Reference,
    /// An existing idea to move away from.
    PriorIdea,
}

/// Rules the ideas must follow.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Constraints {
    /// Things every idea must do.
    #[serde(default)]
    pub must: Vec<String>,
    /// Things no idea may do.
    #[serde(default)]
    pub must_not: Vec<String>,
}

/// The shape of one returned item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputSpec {
    /// What one item is called in prompts, for example `"product name"`.
    #[serde(default = "default_item_name")]
    pub item_name: String,
    /// Extra instruction for the formatter, for example how long a field is.
    #[serde(default)]
    pub item_guidance: Option<String>,
    /// JSON Schema every returned item satisfies.
    pub schema: Value,
}

fn default_item_name() -> String {
    "idea".to_owned()
}

/// How the run is shaped: breadth, depth, scoring, and limits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct RunConfig {
    /// Independent conversations run in parallel, each with its own lens.
    pub variations: u32,
    /// Conversation turns per variation, counting both agents.
    pub generations: u32,
    /// Items the formatter extracts from each conversation.
    pub items_per_variation: u32,
    /// Who talks to whom.
    pub mode: Mode,
    /// The system-prompt style the agents use.
    pub template: Template,
    /// Sampling temperature for the conversation turns.
    pub temperature: f32,
    /// Per-role model names; unset roles use the backend default.
    pub models: ModelSet,
    /// Which scorers rank the items and how many survive.
    pub scoring: ScoringConfig,
    /// Limits on turns, time, and repair attempts.
    pub budget: Budget,
    /// Seeds the lens draw so a run is reproducible against a fixed backend.
    pub seed: u64,
    /// Return the full conversation transcripts alongside the items.
    pub include_transcripts: bool,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            variations: 3,
            generations: 6,
            items_per_variation: 3,
            mode: Mode::Backrooms,
            template: Template::Cli,
            temperature: 1.0,
            models: ModelSet::default(),
            scoring: ScoringConfig::default(),
            budget: Budget::default(),
            seed: 0,
            include_transcripts: false,
        }
    }
}

/// The conversation structure of one variation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Two agents alternate, each answering the other, with no task anchor.
    Backrooms,
    /// One agent keeps pushing its own previous turn further.
    Brainstorm,
    /// One agent proposes and the other attacks, then the proposer revises.
    CriticPair,
}

/// The system-prompt style of the conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Template {
    /// The Infinite Backrooms command-line simulator framing.
    Cli,
    /// Two voices developing a theme in counterpoint.
    Fugue,
    /// A teacher and a student who keeps asking why.
    Student,
}

/// Model names per role. `None` means the backend picks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ModelSet {
    /// The first conversation agent.
    pub agent_a: Option<String>,
    /// The second conversation agent; a different model from `agent_a`
    /// reduces the same-model attractors the backrooms loop falls into.
    pub agent_b: Option<String>,
    /// The agent that turns a transcript into typed items.
    pub formatter: Option<String>,
    /// The agent that rates items for the `judge` scorer.
    pub judge: Option<String>,
}

/// Scoring and selection settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ScoringConfig {
    /// The weighted per-item scorers. Empty means the defaults.
    pub scorers: Vec<ScorerSpec>,
    /// Keep only the best this many items; `None` keeps all of them, ranked.
    pub select_top_k: Option<usize>,
    /// Independent judge calls per item for the `judge` scorer.
    pub judge_raters: u32,
    /// Cosine similarity at or above which two items count as the same idea.
    pub class_threshold: f64,
}

impl Default for ScoringConfig {
    fn default() -> Self {
        Self {
            scorers: Vec::new(),
            select_top_k: None,
            judge_raters: 3,
            class_threshold: 0.8,
        }
    }
}

/// One weighted per-item scorer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScorerSpec {
    /// Which scorer.
    pub kind: ScorerKind,
    /// Its share of the total score; weights are normalized, so only their
    /// ratio matters.
    #[serde(default = "default_weight")]
    pub weight: f64,
}

fn default_weight() -> f64 {
    1.0
}

/// The per-item scorers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScorerKind {
    /// Semantic distance from every other item and from remembered ideas.
    Originality,
    /// How much concrete detail the item carries.
    Elaboration,
    /// Independent raters score novelty and usefulness; the product is the score.
    Judge,
}

/// Limits that stop a run from spending without bound.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Budget {
    /// Hard cap on conversation turns per variation, below `generations`.
    pub max_turns: Option<u32>,
    /// Stop starting new turns after this many seconds.
    pub wall_clock_secs: Option<u64>,
    /// Extra formatter calls allowed when items fail the schema.
    pub repair_attempts: u32,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_turns: None,
            wall_clock_secs: None,
            repair_attempts: 2,
        }
    }
}
