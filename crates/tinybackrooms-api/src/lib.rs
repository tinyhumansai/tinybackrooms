//! The HTTP server for the `tinybackrooms` creativity engine.
//!
//! One request in, ranked typed items out:
//!
//! - `POST /v1/generate` takes a [`GenerateRequest`](tinybackrooms::GenerateRequest)
//!   (`input`, `output`, `config`) and returns a
//!   [`GenerateResponse`](tinybackrooms::GenerateResponse).
//! - `POST /v1/generate/stream` takes the same request and streams progress as
//!   server-sent events, ending with the full response.
//! - `GET /healthz` reports liveness.
//!
//! [`router`] builds the axum router around an [`Engine`],
//! [`serve`] runs it on a listener, and [`run_from_env`] is the whole binary.
//! Two backends are selectable with `TINYBACKROOMS_BACKEND`: `demo`
//! ([`DemoConversant`]), which fabricates schema-valid output so the server
//! works without credentials, and `openhuman` (the `openhuman` Cargo feature),
//! which runs `OpenHuman` agents against an OpenAI-compatible provider. Any other
//! model backend plugs in behind the engine's `Conversant` port.
//!
//! This crate deliberately holds no creative logic: everything about
//! conversations, extraction and scoring lives in `tinybackrooms`.
//!
//! # Example
//!
//! ```
//! use std::sync::Arc;
//! use tinybackrooms::Engine;
//! use tinybackrooms_api::{DemoConversant, router};
//!
//! let app = router(Engine::new(Arc::new(DemoConversant)));
//! # let _ = app;
//! ```

mod config;
mod demo;
mod routes;

use std::future::Future;
use std::io::Write;
use std::process::ExitCode;
use std::sync::Arc;

use tinybackrooms::Engine;
use tokio::net::TcpListener;

pub use config::{BackendKind, Config, ConfigError};
pub use demo::{DemoConversant, sample};
pub use routes::router;

/// Serve `engine` on `listener` until `shutdown` completes.
///
/// # Errors
///
/// Returns the I/O error that stopped the server.
pub async fn serve(
    listener: TcpListener,
    engine: Engine,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(listener, router(engine))
        .with_graceful_shutdown(shutdown)
        .await
}

/// Worker-thread stack for the server runtime. `OpenHuman` agent turns overflow
/// tokio's 2 MiB default, so the whole server runs on larger stacks.
const WORKER_STACK_BYTES: usize = 16 * 1024 * 1024;

/// Read the configuration from the environment, serve until interrupted
/// (`Ctrl-C`), and report how it went as a process exit code.
///
/// Prints `listening on http://ADDR` to stdout once the socket is bound, using
/// the address actually bound, so `TINYBACKROOMS_ADDR=127.0.0.1:0` works.
#[must_use]
pub fn run_from_env() -> ExitCode {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(WORKER_STACK_BYTES)
        .build();
    let outcome = match runtime {
        Ok(runtime) => {
            // Spawned so the server runs on a worker thread, which has the
            // large stack, rather than on this thread, which does not.
            runtime.block_on(async {
                tokio::spawn(run())
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|ran| ran)
            })
        }
        Err(error) => Err(error.to_string()),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("tinybackrooms-api: {message}");
            ExitCode::FAILURE
        }
    }
}

async fn build_engine(backend: BackendKind) -> Result<Engine, String> {
    match backend {
        BackendKind::Demo => Ok(Engine::new(Arc::new(DemoConversant))),
        #[cfg(feature = "openhuman")]
        BackendKind::OpenHuman(settings) => {
            use std::time::Duration;
            use tinybackrooms_openhuman::{OpenHumanConfig, OpenHumanConversant};
            let config = OpenHumanConfig::new(settings.base_url, settings.api_key, settings.model)
                .with_turn_timeout(Duration::from_secs(settings.turn_timeout_secs));
            let conversant = OpenHumanConversant::new(config)
                .await
                .map_err(|e| e.to_string())?;
            Ok(Engine::new(Arc::new(conversant)))
        }
        #[cfg(not(feature = "openhuman"))]
        BackendKind::OpenHuman(_) => {
            Err("the openhuman backend needs a build with `--features openhuman`".to_owned())
        }
    }
}

async fn run() -> Result<(), String> {
    let config = Config::from_env().map_err(|e| e.to_string())?;
    let engine = build_engine(config.backend).await?;
    let listener = TcpListener::bind(config.addr)
        .await
        .map_err(|e| format!("cannot bind {}: {e}", config.addr))?;
    let bound = listener.local_addr().map_err(|e| e.to_string())?;
    let mut stdout = std::io::stdout();
    writeln!(stdout, "listening on http://{bound}")
        .and_then(|()| stdout.flush())
        .map_err(|e| e.to_string())?;
    serve(listener, engine, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod test;
