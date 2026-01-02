//! gotgan.toml configuration parsing

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use thiserror::Error;

/// Errors that can occur when working with configuration
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub package: Package,

    #[serde(default)]
    pub dependencies: HashMap<String, Dependency>,

    #[serde(default, rename = "dev-dependencies")]
    pub dev_dependencies: HashMap<String, Dependency>,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedDependency {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,

    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
}

impl Manifest {
    /// Create a new manifest with default values
    pub fn new(name: &str) -> Self {
        Self {
            package: Package {
                name: name.to_string(),
                version: "0.1.0".to_string(),
                edition: default_edition(),
                description: None,
                license: None,
                authors: None,
                repository: None,
            },
            dependencies: HashMap::new(),
            dev_dependencies: HashMap::new(),
        }
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
        assert_eq!(manifest.package.name, "test-project");
        assert_eq!(manifest.package.version, "0.1.0");
        assert_eq!(manifest.package.edition, "2025");
    }

    #[test]
    fn test_manifest_serialize() {
        let manifest = Manifest::new("hello");
        let toml = manifest.to_toml().unwrap();
        assert!(toml.contains("name = \"hello\""));
        assert!(toml.contains("version = \"0.1.0\""));
    }
}
