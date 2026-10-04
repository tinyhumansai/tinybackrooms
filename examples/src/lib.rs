//! Helpers shared by the example binaries and their tests: load a request file
//! and run it offline.

use std::path::Path;
use std::sync::Arc;

use tinybackrooms::{Engine, GenerateRequest, GenerateResponse};
use tinybackrooms_api::DemoConversant;

/// Read a request file.
///
/// # Errors
///
/// Returns a message when the file cannot be read or is not a valid request.
pub fn load(path: &Path) -> Result<GenerateRequest, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Run a request against the offline demo backend, which fabricates
/// schema-valid output without calling a model.
///
/// # Errors
///
/// Returns the engine's error as a message.
pub async fn run_offline(request: &GenerateRequest) -> Result<GenerateResponse, String> {
    Engine::new(Arc::new(DemoConversant))
        .generate(request)
        .await
        .map_err(|e| e.to_string())
}

/// A short human-readable summary of a response.
#[must_use]
pub fn summarize(response: &GenerateResponse) -> String {
    let mut lines: Vec<String> = response
        .items
        .iter()
        .enumerate()
        .map(|(rank, item)| {
            format!(
                "{:>2}. [{:.2}] {} ({})",
                rank + 1,
                item.total,
                item.value,
                item.run_id
            )
        })
        .collect();
    let d = &response.diversity;
    lines.push(format!(
        "\n{} items, {} distinct, vendi {:.2}, dat {:.1}, {} backend calls",
        d.fluency, d.distinct_classes, d.vendi, d.dat, response.usage.calls
    ));
    lines.extend(response.warnings.iter().map(|w| format!("warning: {w}")));
    lines.join("\n")
}
