//! Run request files against the offline demo backend.
//!
//! ```sh
//! cargo run --manifest-path examples/Cargo.toml --bin offline
//! cargo run --manifest-path examples/Cargo.toml --bin offline -- examples/requests/product-naming.json
//! ```
//!
//! With no arguments every file in `examples/requests/` runs. The demo backend
//! fabricates schema-valid output, so this shows the request/response shape and
//! the scoring plumbing, not creativity; point the API server at a real model
//! backend for that.

use std::path::PathBuf;
use std::process::ExitCode;

use tinybackrooms_examples::{load, run_offline, summarize};

fn request_files() -> Vec<PathBuf> {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if !args.is_empty() {
        return args;
    }
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("requests");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    files
}

#[tokio::main]
async fn main() -> ExitCode {
    let mut failed = false;
    for path in request_files() {
        println!("== {}", path.display());
        let result = match load(&path) {
            Ok(request) => run_offline(&request).await,
            Err(message) => Err(message),
        };
        match result {
            Ok(response) => println!("{}", summarize(&response)),
            Err(message) => {
                eprintln!("error: {message}");
                failed = true;
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
