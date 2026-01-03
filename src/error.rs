//! Unified error type for gotgan

use thiserror::Error;

/// Unified error type for all gotgan operations
#[derive(Error, Debug)]
pub enum GotganError {
    #[error("Project error: {0}")]
    Project(#[from] crate::project::ProjectError),

    #[error("Build error: {0}")]
    Build(#[from] crate::build::BuildError),

    #[error("Config error: {0}")]
    Config(#[from] crate::config::ConfigError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("Bundle error: {0}")]
    Bundle(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
