//! The generation request and its validation.
//!
//! A [`GenerateRequest`] is `input` (what to be creative about), `output` (the
//! JSON Schema each item satisfies) and `config` (how many variations and
//! turns, which scorers, what budget). [`GenerateRequest::validate`] refuses a
//! request that could not produce anything before any model call is spent.

mod types;

pub use types::{
    Budget, Constraints, Context, GenerateRequest, Mode, ModelSet, OutputSpec, RunConfig,
    ScorerKind, ScorerSpec, ScoringConfig, Seed, SeedKind, Template,
};

use crate::{Error, Result};

/// The largest `variations` the engine accepts in one request.
pub const MAX_VARIATIONS: u32 = 64;

impl GenerateRequest {
    /// Check the request is runnable.
    ///
    /// # Errors
    ///
    /// Returns [`Error::EmptyBrief`] for a blank brief, [`Error::NoVariations`],
    /// [`Error::TooManyVariations`], [`Error::NoGenerations`] or
    /// [`Error::NoItems`] for out-of-range counts, [`Error::InvalidWeights`] or
    /// [`Error::InvalidTopK`] for bad scoring settings, and
    /// [`Error::InvalidSchema`] when the output schema does not compile.
    pub fn validate(&self) -> Result<()> {
        if self.input.brief.trim().is_empty() {
            return Err(Error::EmptyBrief);
        }
        let config = &self.config;
        if config.variations == 0 {
            return Err(Error::NoVariations);
        }
        if config.variations > MAX_VARIATIONS {
            return Err(Error::TooManyVariations {
                max: MAX_VARIATIONS,
            });
        }
        if config.generations == 0 {
            return Err(Error::NoGenerations);
        }
        if config.items_per_variation == 0 {
            return Err(Error::NoItems);
        }
        config.scoring.validate()?;
        self.output.compile()?;
        Ok(())
    }
}

impl ScoringConfig {
    fn validate(&self) -> Result<()> {
        if self.select_top_k == Some(0) {
            return Err(Error::InvalidTopK);
        }
        let weights = self.scorers.iter().map(|spec| spec.weight);
        if weights.clone().any(|w| !w.is_finite() || w < 0.0)
            || (!self.scorers.is_empty() && weights.sum::<f64>() <= 0.0)
        {
            return Err(Error::InvalidWeights);
        }
        Ok(())
    }

    /// The scorers to run: the configured list, or the defaults when empty.
    #[must_use]
    pub fn effective_scorers(&self) -> Vec<ScorerSpec> {
        if self.scorers.is_empty() {
            vec![
                ScorerSpec {
                    kind: ScorerKind::Originality,
                    weight: 1.0,
                },
                ScorerSpec {
                    kind: ScorerKind::Elaboration,
                    weight: 0.5,
                },
            ]
        } else {
            self.scorers.clone()
        }
    }
}

impl OutputSpec {
    /// Compile the schema into a reusable validator.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidSchema`] when the schema is not valid JSON Schema.
    pub(crate) fn compile(&self) -> Result<jsonschema::Validator> {
        jsonschema::validator_for(&self.schema).map_err(|e| Error::InvalidSchema(e.to_string()))
    }
}

#[cfg(test)]
mod test;
