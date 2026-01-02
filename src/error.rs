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
}
