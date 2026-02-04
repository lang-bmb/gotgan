//! gotgan.toml configuration parsing

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use thiserror::Error;

/// Errors that can occur when working with configuration
#[allow(clippy::enum_variant_names)]
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),

    #[error("Failed to parse config file: {0}")]
    ParseError(#[from] toml::de::Error),

    #[error("Failed to serialize config: {0}")]
    SerializeError(#[from] toml::ser::Error),
}

/// BMB project manifest (gotgan.toml)
/// Can be either a package manifest or a workspace manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<Package>,

    /// Workspace configuration (for monorepo support)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<Workspace>,

    #[serde(default)]
    pub dependencies: HashMap<String, Dependency>,

    #[serde(default, rename = "dev-dependencies")]
    pub dev_dependencies: HashMap<String, Dependency>,
}

/// Workspace configuration for monorepo support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    /// Member package paths (glob patterns supported)
    /// Example: ["packages/*", "crates/core"]
    pub members: Vec<String>,

    /// Packages to exclude from workspace
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,

    /// Shared dependencies for all workspace members
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub dependencies: HashMap<String, Dependency>,
}

/// Package metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,

    #[serde(default = "default_edition")]
    pub edition: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub authors: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
}

fn default_edition() -> String {
    "2025".to_string()
}

/// Dependency specification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Dependency {
    /// Simple version string: "0.1.0"
    Simple(String),

    /// Detailed dependency specification
    Detailed(DetailedDependency),
}

/// Detailed dependency with optional features
/// Supports Go-style git URL dependencies:
/// - git = "github.com/user/repo" (default branch)
/// - git = "github.com/user/repo", tag = "v1.0.0"
/// - git = "github.com/user/repo", branch = "main"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedDependency {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// Git repository URL (Go-style: "github.com/user/repo")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<String>,

    /// Git branch to use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,

    /// Git tag to use (preferred over branch for releases)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,

    /// Git commit hash (for exact version pinning)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,

    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
}

impl Manifest {
    /// Create a new package manifest with default values
    pub fn new(name: &str) -> Self {
        Self {
            package: Some(Package {
                name: name.to_string(),
                version: "0.1.0".to_string(),
                edition: default_edition(),
                description: None,
                license: None,
                authors: None,
                repository: None,
            }),
            workspace: None,
            dependencies: HashMap::new(),
            dev_dependencies: HashMap::new(),
        }
    }

    /// Create a new workspace manifest
    pub fn new_workspace(members: Vec<String>) -> Self {
        Self {
            package: None,
            workspace: Some(Workspace {
                members,
                exclude: Vec::new(),
                dependencies: HashMap::new(),
            }),
            dependencies: HashMap::new(),
            dev_dependencies: HashMap::new(),
        }
    }

    /// Check if this is a workspace manifest
    pub fn is_workspace(&self) -> bool {
        self.workspace.is_some()
    }

    /// Check if this is a package manifest
    pub fn is_package(&self) -> bool {
        self.package.is_some()
    }

    /// Get package info (panics if workspace manifest)
    pub fn package(&self) -> &Package {
        self.package.as_ref().expect("Expected package manifest, got workspace manifest")
    }

    /// Load manifest from file
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let content = fs::read_to_string(path)?;
        let manifest: Manifest = toml::from_str(&content)?;
        Ok(manifest)
    }

    /// Save manifest to file
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Generate a formatted TOML string
    pub fn to_toml(&self) -> Result<String, ConfigError> {
        Ok(toml::to_string_pretty(self)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_manifest() {
        let manifest = Manifest::new("test-project");
        assert_eq!(manifest.package().name, "test-project");
        assert_eq!(manifest.package().version, "0.1.0");
        assert_eq!(manifest.package().edition, "2025");
    }

    #[test]
    fn test_manifest_serialize() {
        let manifest = Manifest::new("hello");
        let toml = manifest.to_toml().unwrap();
        assert!(toml.contains("name = \"hello\""));
        assert!(toml.contains("version = \"0.1.0\""));
    }

    #[test]
    fn test_workspace_manifest() {
        let manifest = Manifest::new_workspace(vec!["packages/*".to_string()]);
        assert!(manifest.is_workspace());
        assert!(!manifest.is_package());
        let workspace = manifest.workspace.as_ref().unwrap();
        assert_eq!(workspace.members, vec!["packages/*"]);
    }
}
