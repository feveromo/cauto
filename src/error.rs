use std::path::PathBuf;

use thiserror::Error;

/// Top-level typed failure categories and stable exit-code mapping.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid arguments: {0}")]
    InvalidArguments(String),
    #[error(
        "no task prompt was supplied; use a positional prompt, --prompt, --prompt-file, or --stdin"
    )]
    PromptMissing,
    #[error("failed to read configuration {path}: {source}")]
    ConfigRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse configuration {path}: {message}")]
    ConfigParse { path: PathBuf, message: String },
    #[error(
        "invalid configuration {path} at {toml_path}: {value}; expected {expected}. {suggestion}"
    )]
    ConfigValidation {
        path: PathBuf,
        toml_path: String,
        value: String,
        expected: String,
        suggestion: String,
    },
    #[error("repository discovery failed for {path}: {message}")]
    RepositoryDiscovery { path: PathBuf, message: String },
    #[error("Codex was not found; use --codex-bin or set CODEX_BIN")]
    CodexNotFound,
    #[error("the selected Codex executable resolves back to cauto: {0}")]
    CodexRecursion(PathBuf),
    #[error("failed to inspect Codex version: {0}")]
    CodexVersion(String),
    #[error("model catalog discovery failed: {0}")]
    CatalogDiscovery(String),
    #[error("model catalog could not be parsed: {0}")]
    CatalogParse(String),
    #[error("requested preset is unavailable: {0}")]
    PresetUnavailable(String),
    #[error("explicit downgrade refused: {0}; pass --allow-downgrade to permit a fallback")]
    ExplicitDowngradeRefused(String),
    #[error("classifier failed: {0}")]
    Classifier(String),
    #[error("Codex App Server failure: {0}")]
    AppServer(String),
    #[error("cache failure at {path}: {message}")]
    Cache { path: PathBuf, message: String },
    #[error("state failure at {path}: {message}")]
    State { path: PathBuf, message: String },
    #[error("failed to launch native Codex {path}: {source}")]
    LaunchFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("I/O failure for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("serialization failed: {0}")]
    Serialization(String),
}

impl AppError {
    /// Returns the stable process exit code for this failure category.
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::InvalidArguments(_) | Self::PromptMissing => 2,
            Self::ConfigRead { .. } | Self::ConfigParse { .. } | Self::ConfigValidation { .. } => 3,
            Self::CodexNotFound
            | Self::CodexRecursion(_)
            | Self::CodexVersion(_)
            | Self::CatalogDiscovery(_)
            | Self::CatalogParse(_) => 4,
            Self::PresetUnavailable(_) | Self::ExplicitDowngradeRefused(_) => 5,
            Self::Classifier(_) => 6,
            Self::AppServer(_) => 8,
            Self::Cache { .. } | Self::State { .. } | Self::Serialization(_) => 7,
            Self::LaunchFailed { .. } => 8,
            Self::RepositoryDiscovery { .. } | Self::Io { .. } => 3,
        }
    }
}
