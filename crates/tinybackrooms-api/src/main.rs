//! The `tinybackrooms-api` binary: configuration comes from the environment, see
//! [`tinybackrooms_api::Config`].

use std::process::ExitCode;

fn main() -> ExitCode {
    tinybackrooms_api::run_from_env()
}
