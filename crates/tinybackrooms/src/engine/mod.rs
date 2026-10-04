//! The engine: one call from request to response.
//!
//! [`Engine::generate`] runs the whole pipeline:
//!
//! 1. validate the request, so nothing is spent on one that cannot succeed;
//! 2. recall earlier ideas from memory when a namespace is set;
//! 3. run every variation concurrently: a free-text conversation, then typed
//!    extraction of its items;
//! 4. score, rank and select the pooled items;
//! 5. store the selected items back to memory.
//!
//! The engine tolerates partial failure. A variation whose backend fails is
//! dropped with a warning; a memory failure costs the recall or the store, not
//! the run. Only a run that ends with no valid item at all is an error.
//!
//! Everything it touches comes through a port: the [`Conversant`] for models,
//! [`Memory`] for recall, [`Embedder`] for semantic scoring, and an
//! [`Observer`] for progress. The wall-clock budget is checked between
//! conversation turns, not inside a backend call, so backends enforce their own
//! per-turn timeouts.

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::future::join_all;
use jsonschema::Validator;
use serde_json::Value;

use crate::backrooms::{self, Conversation, Transcript};
use crate::extract;
use crate::progress::{Event, NoopObserver, Observer};
use crate::score::{self, Candidate, item_text};
use crate::{
    Conversant, Embedder, Error, GenerateRequest, GenerateResponse, HashingEmbedder,
    InMemoryMemory, Memory, Result, Usage,
};

/// Entries recalled from memory at the start of a run.
const RECALL_LIMIT: usize = 8;

/// Runs generation requests against a model backend.
///
/// Cloning is cheap and shares the backends, so a server holds one engine and
/// hands each request a clone with its own [`Observer`].
#[derive(Clone)]
pub struct Engine {
    conversant: Arc<dyn Conversant>,
    memory: Arc<dyn Memory>,
    embedder: Arc<dyn Embedder>,
    observer: Arc<dyn Observer>,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine").finish_non_exhaustive()
    }
}

/// What one variation produced.
struct Variation {
    transcript: Transcript,
    items: Vec<Value>,
    invalid: u32,
    usage: Usage,
}

impl Engine {
    /// An engine over `conversant`, with process-local memory, the hashing
    /// embedder and no progress reporting.
    #[must_use]
    pub fn new(conversant: Arc<dyn Conversant>) -> Self {
        Self {
            conversant,
            memory: Arc::new(InMemoryMemory::new()),
            embedder: Arc::new(HashingEmbedder::default()),
            observer: Arc::new(NoopObserver),
        }
    }

    /// Use `memory` for recall and storage.
    #[must_use]
    pub fn with_memory(mut self, memory: Arc<dyn Memory>) -> Self {
        self.memory = memory;
        self
    }

    /// Use `embedder` for semantic scoring.
    #[must_use]
    pub fn with_embedder(mut self, embedder: Arc<dyn Embedder>) -> Self {
        self.embedder = embedder;
        self
    }

    /// Report progress to `observer`.
    #[must_use]
    pub fn with_observer(mut self, observer: Arc<dyn Observer>) -> Self {
        self.observer = observer;
        self
    }

    /// Run a request to completion.
    ///
    /// # Errors
    ///
    /// Returns a validation error from [`GenerateRequest::validate`] before any
    /// backend call. Once running, returns the backend's error when every
    /// variation failed, or [`Error::NoValidItems`] when the variations ran but
    /// no item satisfied the output schema.
    pub async fn generate(&self, request: &GenerateRequest) -> Result<GenerateResponse> {
        request.validate()?;
        let validator = request.output.compile()?;
        let config = &request.config;
        let mut warnings = Vec::new();

        let namespace = request.input.memory_namespace.as_deref();
        let recalled = self.recall(request, &mut warnings).await;
        let deadline = config
            .budget
            .wall_clock_secs
            .map(|secs| Instant::now() + Duration::from_secs(secs));
        let outcomes = join_all(
            (0..config.variations)
                .map(|index| self.run_variation(request, &validator, &recalled, deadline, index)),
        )
        .await;

        let Pool {
            candidates,
            transcripts,
            mut usage,
        } = pool(outcomes, request, &mut warnings)?;
        let ranked = score::rank(
            candidates,
            request,
            &recalled,
            &*self.embedder,
            &*self.conversant,
        )
        .await;
        usage.add(ranked.usage);
        warnings.extend(ranked.warnings);
        self.observer.event(Event::Scored {
            items: u32::try_from(ranked.items.len()).unwrap_or(u32::MAX),
        });

        if let Some(namespace) = namespace {
            let entries = ranked
                .items
                .iter()
                .map(|item| item_text(&item.value))
                .collect();
            if let Err(error) = self.memory.store(namespace, entries).await {
                warnings.push(format!("memory store skipped: {error}"));
            }
        }

        Ok(GenerateResponse {
            items: ranked.items,
            diversity: ranked.diversity,
            transcripts: config.include_transcripts.then_some(transcripts),
            usage,
            warnings,
        })
    }

    /// Recall earlier ideas, or nothing when no namespace is set or memory fails.
    async fn recall(&self, request: &GenerateRequest, warnings: &mut Vec<String>) -> Vec<String> {
        let Some(namespace) = request.input.memory_namespace.as_deref() else {
            return Vec::new();
        };
        self.memory
            .recall(namespace, &request.input.brief, RECALL_LIMIT)
            .await
            .unwrap_or_else(|error| {
                warnings.push(format!("memory recall skipped: {error}"));
                Vec::new()
            })
    }

    /// One variation: converse, then extract. Never fails the run on its own.
    async fn run_variation(
        &self,
        request: &GenerateRequest,
        validator: &Validator,
        recalled: &[String],
        deadline: Option<Instant>,
        index: u32,
    ) -> (String, Result<Variation>) {
        let run_id = format!("v{index}");
        self.observer.event(Event::VariationStarted {
            run_id: run_id.clone(),
        });
        let outcome = self
            .converse_and_extract(request, validator, recalled, deadline, index, &run_id)
            .await;
        if let Err(error) = &outcome {
            self.observer.event(Event::VariationFailed {
                run_id: run_id.clone(),
                reason: error.to_string(),
            });
        }
        (run_id, outcome)
    }

    async fn converse_and_extract(
        &self,
        request: &GenerateRequest,
        validator: &Validator,
        recalled: &[String],
        deadline: Option<Instant>,
        index: u32,
        run_id: &str,
    ) -> Result<Variation> {
        let conversation = Conversation {
            run_id,
            index: u64::from(index),
            context: &request.input,
            recalled,
            config: &request.config,
            deadline,
        };
        let (transcript, mut usage) =
            backrooms::run(&conversation, &*self.conversant, &*self.observer).await?;
        let extraction = extract::extract(
            &transcript,
            &request.output,
            validator,
            &request.config,
            &*self.conversant,
        )
        .await?;
        usage.add(extraction.usage);
        self.observer.event(Event::Extracted {
            run_id: run_id.to_owned(),
            valid: u32::try_from(extraction.items.len()).unwrap_or(u32::MAX),
            invalid: extraction.invalid,
        });
        Ok(Variation {
            transcript,
            items: extraction.items,
            invalid: extraction.invalid,
            usage,
        })
    }
}

/// The surviving variations, pooled for scoring.
struct Pool {
    candidates: Vec<Candidate>,
    transcripts: Vec<Transcript>,
    usage: Usage,
}

/// Merge variation outcomes, turning failures and leftovers into warnings.
fn pool(
    outcomes: Vec<(String, Result<Variation>)>,
    request: &GenerateRequest,
    warnings: &mut Vec<String>,
) -> Result<Pool> {
    let mut pool = Pool {
        candidates: Vec::new(),
        transcripts: Vec::new(),
        usage: Usage::default(),
    };
    let mut first_error = None;
    for (run_id, outcome) in outcomes {
        match outcome {
            Ok(variation) => {
                pool.usage.add(variation.usage);
                if variation.invalid > 0 {
                    warnings.push(format!(
                        "{run_id}: {} item(s) still failed the schema after {} repair attempt(s)",
                        variation.invalid, request.config.budget.repair_attempts
                    ));
                }
                pool.candidates.extend(
                    variation
                        .items
                        .into_iter()
                        .map(|item| Candidate::new(&run_id, item)),
                );
                pool.transcripts.push(variation.transcript);
            }
            Err(error) => {
                warnings.push(format!("{run_id} failed: {error}"));
                first_error.get_or_insert(error);
            }
        }
    }
    if pool.transcripts.is_empty() {
        return Err(first_error.unwrap_or(Error::NoValidItems));
    }
    if pool.candidates.is_empty() {
        return Err(Error::NoValidItems);
    }
    Ok(pool)
}

#[cfg(test)]
mod test;
