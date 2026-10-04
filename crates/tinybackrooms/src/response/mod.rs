//! The generation response.
//!
//! A [`GenerateResponse`] carries the ranked, schema-valid items; set-level
//! diversity of what came back; optionally the conversations that produced
//! them; the cost; and any warnings about things that were dropped or degraded
//! along the way. A run that loses a variation or a judge call still returns
//! what it has, and says so in `warnings`.

mod types;

pub use types::{BodenClass, Diversity, GenerateResponse, ScoredItem};
