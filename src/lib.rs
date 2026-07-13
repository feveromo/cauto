#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! Repository-aware routing and native launch support for the Codex CLI.

#[doc(hidden)]
pub mod app_server;
#[doc(hidden)]
pub mod application;
#[doc(hidden)]
pub mod cache;
#[doc(hidden)]
pub mod classifier;
#[doc(hidden)]
pub mod cli;
#[doc(hidden)]
pub mod codex;
#[doc(hidden)]
pub mod config;
#[doc(hidden)]
pub mod context;
#[doc(hidden)]
pub mod error;
#[doc(hidden)]
pub mod output;
#[doc(hidden)]
pub mod paths;
/// Deterministic routing types and pure selection functions.
pub mod routing;
#[doc(hidden)]
pub mod state;

use std::process::ExitCode;

pub use error::AppError;

/// Runs cauto from an already parsed command line.
pub fn run(cli: cli::Cli) -> Result<ExitCode, AppError> {
    application::run(cli)
}
