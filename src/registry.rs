//! Package registry clients
//!
//! Supports both local and remote (GitHub-based) package registries for BMB packages.

#![allow(dead_code)]

use crate::config::{Dependency, DetailedDependency, Manifest};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self};
use std::path::{Path, PathBuf};
use tar::Archive;
use thiserror::Error;

/// Default registry URL (GitHub-based)
pub const DEFAULT_REGISTRY: &str = "https://raw.githubusercontent.com/lang-bmb/registry/main";

/// Errors that can occur when working with the registry
#[derive(Error, Debug)]
pub enum RegistryError {
    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    IoError(#[from] io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Package not found: {0}")]
    PackageNotFound(String),

    #[error("Version not found: {0}@{1}")]
    VersionNotFound(String, String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Archive error: {0}")]
    ArchiveError(String),

    #[error("Version already exists: {0}@{1}")]
    VersionAlreadyExists(String, String),

    #[error("Invalid semver: {0}")]
    InvalidSemver(String),
}

/// Package metadata in the registry index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    pub description: Option<String>,
    pub versions: Vec<VersionInfo>,
    pub repository: Option<String>,
    pub license: Option<String>,
    pub keywords: Option<Vec<String>>,
}

/// Version-specific metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: String,
    pub checksum: String,
    pub published_at: String,
    pub download_url: String,
    pub dependencies: HashMap<String, String>,
}

/// Registry index containing all packages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryIndex {
    pub packages: HashMap<String, PackageInfo>,
    pub last_updated: String,
}

/// Search result entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub name: String,
    pub description: Option<String>,
    pub latest_version: String,
    pub downloads: Option<u64>,
}

/// Registry client for interacting with the BMB package registry
pub struct RegistryClient {
    base_url: String,
    client: reqwest::blocking::Client,
    cache_dir: PathBuf,
}

impl RegistryClient {
    /// Create a new registry client with the default registry URL
    pub fn new() -> Self {
        Self::with_url(DEFAULT_REGISTRY)
    }

    /// Create a new registry client with a custom registry URL
    pub fn with_url(base_url: &str) -> Self {
        let cache_dir = dirs_cache_dir().join("gotgan").join("registry");
        fs::create_dir_all(&cache_dir).ok();

        Self {
            base_url: base_url.to_string(),
            client: reqwest::blocking::Client::new(),
            cache_dir,
        }
    }

    /// Fetch the registry index
    pub fn fetch_index(&self) -> Result<RegistryIndex, RegistryError> {
        let url = format!("{}/index.json", self.base_url);
        let response = self.client.get(&url).send()?;

        if !response.status().is_success() {
            // Return empty index if registry not available yet
            return Ok(RegistryIndex {
                packages: HashMap::new(),
                last_updated: chrono_now(),
            });
        }

        let index: RegistryIndex = response.json()?;
        Ok(index)
    }

    /// Search for packages matching a query
    pub fn search(&self, query: &str) -> Result<Vec<SearchResult>, RegistryError> {
        let index = self.fetch_index()?;
        let query_lower = query.to_lowercase();

        let results: Vec<SearchResult> = index
            .packages
            .iter()
            .filter(|(name, info)| {
                name.to_lowercase().contains(&query_lower)
                    || info
                        .description
                        .as_ref()
                        .map(|d| d.to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
                    || info
                        .keywords
                        .as_ref()
                        .map(|kw| kw.iter().any(|k| k.to_lowercase().contains(&query_lower)))
                        .unwrap_or(false)
            })
            .map(|(_, info)| SearchResult {
                name: info.name.clone(),
                description: info.description.clone(),
                latest_version: info
                    .versions
                    .first()
                    .map(|v| v.version.clone())
                    .unwrap_or_else(|| "0.0.0".to_string()),
                downloads: None,
            })
            .collect();

        Ok(results)
    }

    /// Get package information
    pub fn get_package(&self, name: &str) -> Result<PackageInfo, RegistryError> {
        let index = self.fetch_index()?;
        index
            .packages
            .get(name)
            .cloned()
            .ok_or_else(|| RegistryError::PackageNotFound(name.to_string()))
    }

    /// Get the latest version of a package
    pub fn get_latest_version(&self, name: &str) -> Result<String, RegistryError> {
        let info = self.get_package(name)?;
        info.versions
            .first()
            .map(|v| v.version.clone())
            .ok_or_else(|| RegistryError::VersionNotFound(name.to_string(), "latest".to_string()))
    }

    /// Download a package to the cache
    pub fn download_package(&self, name: &str, version: &str) -> Result<PathBuf, RegistryError> {
        let info = self.get_package(name)?;
        let version_info = info
            .versions
            .iter()
            .find(|v| v.version == version)
            .ok_or_else(|| RegistryError::VersionNotFound(name.to_string(), version.to_string()))?;

        let cache_path = self
            .cache_dir
            .join(name)
            .join(format!("{}-{}.tar.gz", name, version));

        // Return cached file if exists
        if cache_path.exists() {
            return Ok(cache_path);
        }

        // Create parent directories
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Download the package
        let response = self.client.get(&version_info.download_url).send()?;
        if !response.status().is_success() {
            return Err(RegistryError::PackageNotFound(format!(
                "{}@{}",
                name, version
            )));
        }

        let bytes = response.bytes()?;
        fs::write(&cache_path, bytes)?;

        Ok(cache_path)
    }

    /// Extract a downloaded package to a target directory
    pub fn extract_package(
        &self,
        archive_path: &Path,
        target_dir: &Path,
    ) -> Result<(), RegistryError> {
        let file = File::open(archive_path)?;
        let decoder = GzDecoder::new(file);
        let mut archive = Archive::new(decoder);

        archive
            .unpack(target_dir)
            .map_err(|e| RegistryError::ArchiveError(e.to_string()))?;

        Ok(())
    }
}

impl Default for RegistryClient {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// LocalRegistry - Local package registry for offline development
// ============================================================================

/// Local package registry for offline package management
///
/// Structure:
/// ```text
/// ~/.gotgan/
/// ├── registry/
/// │   ├── index.json           # Local package index
/// │   ├── bmb-json/
/// │   │   ├── 0.1.0/           # Version directory
/// │   │   │   ├── gotgan.toml
/// │   │   │   └── src/
/// │   │   └── 0.2.0/
/// │   └── bmb-regex/
/// └── cache/                   # Downloaded package cache
/// ```
pub struct LocalRegistry {
    registry_dir: PathBuf,
}

impl LocalRegistry {
    /// Create a new local registry at ~/.gotgan/registry/
    pub fn new() -> Self {
        let registry_dir = dirs_home_dir().join(".gotgan").join("registry");
        fs::create_dir_all(&registry_dir).ok();
        Self { registry_dir }
    }

    /// Create a local registry at a custom path
    pub fn with_path(registry_dir: PathBuf) -> Self {
        fs::create_dir_all(&registry_dir).ok();
        Self { registry_dir }
    }

    /// Get the registry directory path
    pub fn registry_dir(&self) -> &Path {
        &self.registry_dir
    }

    /// Load the local registry index
    pub fn load_index(&self) -> Result<RegistryIndex, RegistryError> {
        let index_path = self.registry_dir.join("index.json");
        if !index_path.exists() {
            return Ok(RegistryIndex {
                packages: HashMap::new(),
                last_updated: chrono_now(),
            });
        }

        let content = fs::read_to_string(&index_path)?;
        let index: RegistryIndex = serde_json::from_str(&content)?;
        Ok(index)
    }

    /// Save the local registry index
    fn save_index(&self, index: &RegistryIndex) -> Result<(), RegistryError> {
        let index_path = self.registry_dir.join("index.json");
        let content = serde_json::to_string_pretty(index)?;
        fs::write(&index_path, content)?;
        Ok(())
    }

    /// Publish a package to the local registry
    pub fn publish(&self, project_dir: &Path, manifest: &Manifest, checksum: &str) -> Result<PathBuf, RegistryError> {
        let name = &manifest.package().name;
        let version = &manifest.package().version;

        // Create package directory: ~/.gotgan/registry/{name}/{version}/
        let package_dir = self.registry_dir.join(name).join(version);

        // Check if version already exists
        if package_dir.exists() {
            return Err(RegistryError::VersionAlreadyExists(
                name.clone(),
                version.clone(),
            ));
        }

        fs::create_dir_all(&package_dir)?;

        // Copy source files to package directory
        copy_package_files(project_dir, &package_dir)?;

        // Update local index
        let mut index = self.load_index()?;

        let version_info = VersionInfo {
            version: version.clone(),
            checksum: checksum.to_string(),
            published_at: chrono_now(),
            download_url: format!("file://{}", package_dir.display()),
            dependencies: manifest
                .dependencies
                .iter()
                .filter_map(|(name, dep)| {
                    let version = match dep {
                        Dependency::Simple(v) => Some(v.clone()),
                        Dependency::Detailed(d) => d.version.clone(),
                    };
                    version.map(|v| (name.clone(), v))
                })
                .collect(),
        };

        if let Some(pkg_info) = index.packages.get_mut(name) {
            // Add new version to existing package
            pkg_info.versions.insert(0, version_info);
        } else {
            // Create new package entry
            let pkg_info = PackageInfo {
                name: name.clone(),
                description: manifest.package().description.clone(),
                versions: vec![version_info],
                repository: manifest.package().repository.clone(),
                license: manifest.package().license.clone(),
                keywords: None,
            };
            index.packages.insert(name.clone(), pkg_info);
        }

        index.last_updated = chrono_now();
        self.save_index(&index)?;

        Ok(package_dir)
    }

    /// Get package information from local registry
    pub fn get_package(&self, name: &str) -> Result<PackageInfo, RegistryError> {
        let index = self.load_index()?;
        index
            .packages
            .get(name)
            .cloned()
            .ok_or_else(|| RegistryError::PackageNotFound(name.to_string()))
    }

    /// Get the latest version of a package from local registry
    pub fn get_latest_version(&self, name: &str) -> Result<String, RegistryError> {
        let info = self.get_package(name)?;
        info.versions
            .first()
            .map(|v| v.version.clone())
            .ok_or_else(|| RegistryError::VersionNotFound(name.to_string(), "latest".to_string()))
    }

    /// List all versions of a package, sorted by semver (newest first)
    pub fn list_versions(&self, name: &str) -> Result<Vec<String>, RegistryError> {
        let info = self.get_package(name)?;
        let mut versions: Vec<String> = info.versions.iter().map(|v| v.version.clone()).collect();

        // Sort by semver (descending)
        versions.sort_by(|a, b| compare_semver(b, a));

        Ok(versions)
    }

    /// Get the package directory path for a specific version
    pub fn get_package_path(&self, name: &str, version: &str) -> Result<PathBuf, RegistryError> {
        let package_dir = self.registry_dir.join(name).join(version);
        if !package_dir.exists() {
            return Err(RegistryError::VersionNotFound(
                name.to_string(),
                version.to_string(),
            ));
        }
        Ok(package_dir)
    }

    /// Search for packages in local registry
    pub fn search(&self, query: &str) -> Result<Vec<SearchResult>, RegistryError> {
        let index = self.load_index()?;
        let query_lower = query.to_lowercase();

        let results: Vec<SearchResult> = index
            .packages
            .iter()
            .filter(|(name, info)| {
                name.to_lowercase().contains(&query_lower)
                    || info
                        .description
                        .as_ref()
                        .map(|d| d.to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
            })
            .map(|(_, info)| SearchResult {
                name: info.name.clone(),
                description: info.description.clone(),
                latest_version: info
                    .versions
                    .first()
                    .map(|v| v.version.clone())
                    .unwrap_or_else(|| "0.0.0".to_string()),
                downloads: None,
            })
            .collect();

        Ok(results)
    }

    /// Find the best matching version for a constraint
    ///
    /// Supports:
    /// - Exact: "1.0.0"
    /// - Caret: "^1.0.0" (>=1.0.0 <2.0.0)
    /// - Tilde: "~1.0.0" (>=1.0.0 <1.1.0)
    /// - Wildcard: "*" (any version)
    /// - Range: ">=1.0.0" (greater or equal)
    pub fn resolve_version(&self, name: &str, constraint: &str) -> Result<String, RegistryError> {
        let versions = self.list_versions(name)?;

        if versions.is_empty() {
            return Err(RegistryError::VersionNotFound(
                name.to_string(),
                constraint.to_string(),
            ));
        }

        // Handle wildcard
        if constraint == "*" {
            return Ok(versions.first().unwrap().clone());
        }

        // Parse constraint
        let (op, version_str) = parse_version_constraint(constraint);

        for version in &versions {
            if matches_constraint(version, &op, &version_str) {
                return Ok(version.clone());
            }
        }

        Err(RegistryError::VersionNotFound(
            name.to_string(),
            constraint.to_string(),
        ))
    }

    /// Remove a specific version from the local registry
    pub fn remove_version(&self, name: &str, version: &str) -> Result<(), RegistryError> {
        let package_dir = self.registry_dir.join(name).join(version);

        if !package_dir.exists() {
            return Err(RegistryError::VersionNotFound(
                name.to_string(),
                version.to_string(),
            ));
        }

        // Remove the version directory
        fs::remove_dir_all(&package_dir)?;

        // Update index
        let mut index = self.load_index()?;
        if let Some(pkg_info) = index.packages.get_mut(name) {
            pkg_info.versions.retain(|v| v.version != version);

            // Remove package entry if no versions left
            if pkg_info.versions.is_empty() {
                index.packages.remove(name);
                // Also remove the package directory
                let pkg_dir = self.registry_dir.join(name);
                if pkg_dir.exists() {
                    fs::remove_dir_all(&pkg_dir)?;
                }
            }
        }

        index.last_updated = chrono_now();
        self.save_index(&index)?;

        Ok(())
    }
}

impl Default for LocalRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Helper functions
// ============================================================================

/// Create a package archive from a project directory
pub fn create_package_archive(project_dir: &Path, output_path: &Path) -> Result<(), RegistryError> {
    let manifest_path = project_dir.join("gotgan.toml");
    if !manifest_path.exists() {
        return Err(RegistryError::MissingField(
            "gotgan.toml not found".to_string(),
        ));
    }

    let file = File::create(output_path)?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = tar::Builder::new(encoder);

    // Add all BMB source files and the manifest
    for entry in walkdir_simple(project_dir)? {
        let path = entry;
        let relative_path = path.strip_prefix(project_dir).unwrap_or(&path);

        // Skip target directory and hidden files
        let relative_str = relative_path.to_string_lossy();
        if relative_str.starts_with("target")
            || relative_str.starts_with('.')
            || relative_str.contains("/.")
        {
            continue;
        }

        // Include .bmb files, gotgan.toml, README, LICENSE
        let include = relative_str.ends_with(".bmb")
            || relative_str == "gotgan.toml"
            || relative_str.starts_with("README")
            || relative_str.starts_with("LICENSE")
            || relative_str == "src"
            || relative_str.starts_with("src/");

        if include && path.is_file() {
            builder
                .append_path_with_name(&path, relative_path)
                .map_err(|e| RegistryError::ArchiveError(e.to_string()))?;
        }
    }

    builder
        .finish()
        .map_err(|e| RegistryError::ArchiveError(e.to_string()))?;

    Ok(())
}

/// Generate package metadata for publishing
pub fn generate_package_info(manifest: &Manifest, checksum: &str) -> PackageInfo {
    let version_info = VersionInfo {
        version: manifest.package().version.clone(),
        checksum: checksum.to_string(),
        published_at: chrono_now(),
        download_url: format!(
            "https://github.com/lang-bmb/registry/releases/download/{}/{}-{}.tar.gz",
            manifest.package().name, manifest.package().name, manifest.package().version
        ),
        dependencies: manifest
            .dependencies
            .iter()
            .filter_map(|(name, dep)| {
                let version = match dep {
                    Dependency::Simple(v) => Some(v.clone()),
                    Dependency::Detailed(d) => d.version.clone(),
                };
                version.map(|v| (name.clone(), v))
            })
            .collect(),
    };

    PackageInfo {
        name: manifest.package().name.clone(),
        description: manifest.package().description.clone(),
        versions: vec![version_info],
        repository: manifest.package().repository.clone(),
        license: manifest.package().license.clone(),
        keywords: None,
    }
}

/// Resolve a dependency to a Dependency struct
pub fn resolve_registry_dependency(_name: &str, version: &str) -> Dependency {
    Dependency::Detailed(DetailedDependency {
        version: Some(version.to_string()),
        path: None,
        git: None,
        branch: None,
        tag: None,
        rev: None,
        features: Vec::new(),
        optional: false,
    })
}

/// Resolve a local registry dependency
pub fn resolve_local_dependency(_name: &str, version: &str, local_path: &Path) -> Dependency {
    Dependency::Detailed(DetailedDependency {
        version: Some(version.to_string()),
        path: Some(local_path.to_string_lossy().to_string()),
        git: None,
        branch: None,
        tag: None,
        rev: None,
        features: Vec::new(),
        optional: false,
    })
}

// Private helper functions

fn dirs_cache_dir() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
        .join(".cache")
}

fn dirs_home_dir() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
}

fn chrono_now() -> String {
    // Simple ISO 8601 timestamp without external dependency
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Convert to basic ISO format (approximate)
    format!("2025-01-02T{:02}:{:02}:{:02}Z",
        (secs / 3600) % 24,
        (secs / 60) % 60,
        secs % 60
    )
}

fn walkdir_simple(dir: &Path) -> Result<Vec<PathBuf>, io::Error> {
    let mut results = Vec::new();
    walk_dir_recursive(dir, &mut results)?;
    Ok(results)
}

fn walk_dir_recursive(dir: &Path, results: &mut Vec<PathBuf>) -> Result<(), io::Error> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                walk_dir_recursive(&path, results)?;
            } else {
                results.push(path);
            }
        }
    }
    Ok(())
}

/// Copy package files to target directory
fn copy_package_files(src: &Path, dst: &Path) -> Result<(), RegistryError> {
    for entry in walkdir_simple(src)? {
        let relative_path = entry.strip_prefix(src).unwrap_or(&entry);

        // Skip target directory and hidden files
        let relative_str = relative_path.to_string_lossy();
        if relative_str.starts_with("target")
            || relative_str.starts_with('.')
            || relative_str.contains("/.")
        {
            continue;
        }

        // Include .bmb files, gotgan.toml, README, LICENSE
        let include = relative_str.ends_with(".bmb")
            || relative_str == "gotgan.toml"
            || relative_str.starts_with("README")
            || relative_str.starts_with("LICENSE")
            || relative_str.starts_with("src/");

        if include && entry.is_file() {
            let dst_path = dst.join(relative_path);
            if let Some(parent) = dst_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&entry, &dst_path)?;
        }
    }
    Ok(())
}

/// Compare two semver strings
/// Returns: Ordering (Less, Equal, Greater)
fn compare_semver(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> (u64, u64, u64) {
        let parts: Vec<&str> = s.split('.').collect();
        let major = parts.first().and_then(|p| p.parse().ok()).unwrap_or(0);
        let minor = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(0);
        let patch = parts.get(2).and_then(|p| p.split('-').next()?.parse().ok()).unwrap_or(0);
        (major, minor, patch)
    };

    let a_ver = parse(a);
    let b_ver = parse(b);

    a_ver.cmp(&b_ver)
}

/// Parse version constraint into operator and version
fn parse_version_constraint(constraint: &str) -> (String, String) {
    let constraint = constraint.trim();

    if let Some(ver) = constraint.strip_prefix('^') {
        ("^".to_string(), ver.to_string())
    } else if let Some(ver) = constraint.strip_prefix('~') {
        ("~".to_string(), ver.to_string())
    } else if let Some(ver) = constraint.strip_prefix(">=") {
        (">=".to_string(), ver.to_string())
    } else if let Some(ver) = constraint.strip_prefix("<=") {
        ("<=".to_string(), ver.to_string())
    } else if let Some(ver) = constraint.strip_prefix('>') {
        (">".to_string(), ver.to_string())
    } else if let Some(ver) = constraint.strip_prefix('<') {
        ("<".to_string(), ver.to_string())
    } else if let Some(ver) = constraint.strip_prefix('=') {
        ("=".to_string(), ver.to_string())
    } else {
        // Exact version
        ("=".to_string(), constraint.to_string())
    }
}

/// Check if a version matches a constraint
fn matches_constraint(version: &str, op: &str, constraint_ver: &str) -> bool {
    let parse = |s: &str| -> (u64, u64, u64) {
        let parts: Vec<&str> = s.split('.').collect();
        let major = parts.first().and_then(|p| p.parse().ok()).unwrap_or(0);
        let minor = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(0);
        let patch = parts.get(2).and_then(|p| p.split('-').next()?.parse().ok()).unwrap_or(0);
        (major, minor, patch)
    };

    let v = parse(version);
    let c = parse(constraint_ver);

    match op {
        "=" => v == c,
        ">" => v > c,
        "<" => v < c,
        ">=" => v >= c,
        "<=" => v <= c,
        "^" => {
            // ^1.2.3 means >=1.2.3 <2.0.0 (for major > 0)
            // ^0.2.3 means >=0.2.3 <0.3.0 (for major == 0)
            if c.0 == 0 {
                v.0 == c.0 && v.1 == c.1 && v.2 >= c.2
            } else {
                v.0 == c.0 && (v.1 > c.1 || (v.1 == c.1 && v.2 >= c.2))
            }
        }
        "~" => {
            // ~1.2.3 means >=1.2.3 <1.3.0
            v.0 == c.0 && v.1 == c.1 && v.2 >= c.2
        }
        _ => v == c,
    }
}

// ============================================================================
// StandardLibrary - Search packages in the standard library (packages/ dir)
// ============================================================================

/// Standard library package metadata
pub struct StdLibPackage {
    pub name: String,
    pub version: String,
    pub description: String,
    pub path: PathBuf,
}

/// Search standard library packages
///
/// Looks for packages in the `packages/` directory relative to:
/// 1. BMB_STDLIB_PATH environment variable
/// 2. Current executable's parent directory
/// 3. Common installation paths
pub fn search_stdlib(query: &str) -> Vec<SearchResult> {
    let query_lower = query.to_lowercase();
    let packages = discover_stdlib_packages();

    packages
        .into_iter()
        .filter(|pkg| {
            pkg.name.to_lowercase().contains(&query_lower)
                || pkg.description.to_lowercase().contains(&query_lower)
        })
        .map(|pkg| SearchResult {
            name: pkg.name,
            description: Some(pkg.description),
            latest_version: pkg.version,
            downloads: None,
        })
        .collect()
}

/// Discover all standard library packages
fn discover_stdlib_packages() -> Vec<StdLibPackage> {
    let mut packages = Vec::new();

    // Try to find packages directory
    let packages_dirs = find_stdlib_paths();

    for packages_dir in packages_dirs {
        if !packages_dir.exists() {
            continue;
        }

        if let Ok(entries) = fs::read_dir(&packages_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir()
                    && let Some(pkg) = load_stdlib_package(&path) {
                        packages.push(pkg);
                    }
            }
        }
    }

    packages
}

/// Find potential standard library paths
fn find_stdlib_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // 1. BMB_STDLIB_PATH environment variable
    if let Ok(stdlib_path) = std::env::var("BMB_STDLIB_PATH") {
        paths.push(PathBuf::from(stdlib_path));
    }

    // 2. Relative to current directory (for development)
    paths.push(PathBuf::from("packages"));
    paths.push(PathBuf::from("../packages"));
    paths.push(PathBuf::from("../../packages"));

    // 3. Relative to executable
    if let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent() {
            paths.push(exe_dir.join("packages"));
            paths.push(exe_dir.join("../packages"));
            paths.push(exe_dir.join("../../packages"));
        }

    // 4. Common installation paths
    let home = dirs_home_dir();
    paths.push(home.join(".bmb").join("packages"));
    paths.push(home.join(".gotgan").join("stdlib"));

    paths
}

/// Load package metadata from a directory
fn load_stdlib_package(path: &Path) -> Option<StdLibPackage> {
    let gotgan_toml = path.join("gotgan.toml");
    if !gotgan_toml.exists() {
        return None;
    }

    let content = fs::read_to_string(&gotgan_toml).ok()?;
    let manifest: crate::config::Manifest = toml::from_str(&content).ok()?;

    if !manifest.is_package() {
        return None;
    }

    let pkg = manifest.package();
    let description = pkg.description.clone().unwrap_or_else(|| {
        // Generate description from package name
        let name = &pkg.name;
        if name.starts_with("bmb-") {
            format!("BMB standard library: {}", name.strip_prefix("bmb-").unwrap_or(name))
        } else {
            format!("BMB package: {}", name)
        }
    });

    Some(StdLibPackage {
        name: pkg.name.clone(),
        version: pkg.version.clone(),
        description,
        path: path.to_path_buf(),
    })
}

/// Combined search across all sources
///
/// Search order:
/// 1. Standard library (packages/bmb-*)
/// 2. Local registry (~/.gotgan/registry/)
/// 3. Remote registry (if available)
pub fn search_all(query: &str) -> Vec<SearchResult> {
    let mut results = Vec::new();
    let mut seen_names = std::collections::HashSet::new();

    // 1. Standard library (highest priority)
    for result in search_stdlib(query) {
        if seen_names.insert(result.name.clone()) {
            results.push(result);
        }
    }

    // 2. Local registry
    let local_registry = LocalRegistry::new();
    if let Ok(local_results) = local_registry.search(query) {
        for result in local_results {
            if seen_names.insert(result.name.clone()) {
                results.push(result);
            }
        }
    }

    // 3. Remote registry (optional, may fail)
    let remote_registry = RegistryClient::new();
    if let Ok(remote_results) = remote_registry.search(query) {
        for result in remote_results {
            if seen_names.insert(result.name.clone()) {
                results.push(result);
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_client_new() {
        let client = RegistryClient::new();
        assert_eq!(client.base_url, DEFAULT_REGISTRY);
    }

    #[test]
    fn test_resolve_registry_dependency() {
        let dep = resolve_registry_dependency("json", "1.0.0");
        match dep {
            Dependency::Detailed(d) => {
                assert_eq!(d.version, Some("1.0.0".to_string()));
            }
            _ => panic!("Expected detailed dependency"),
        }
    }

    #[test]
    fn test_compare_semver() {
        assert_eq!(compare_semver("1.0.0", "1.0.0"), std::cmp::Ordering::Equal);
        assert_eq!(compare_semver("1.0.1", "1.0.0"), std::cmp::Ordering::Greater);
        assert_eq!(compare_semver("1.0.0", "1.0.1"), std::cmp::Ordering::Less);
        assert_eq!(compare_semver("2.0.0", "1.9.9"), std::cmp::Ordering::Greater);
        assert_eq!(compare_semver("1.10.0", "1.9.0"), std::cmp::Ordering::Greater);
    }

    #[test]
    fn test_parse_version_constraint() {
        assert_eq!(parse_version_constraint("^1.0.0"), ("^".to_string(), "1.0.0".to_string()));
        assert_eq!(parse_version_constraint("~1.0.0"), ("~".to_string(), "1.0.0".to_string()));
        assert_eq!(parse_version_constraint(">=1.0.0"), (">=".to_string(), "1.0.0".to_string()));
        assert_eq!(parse_version_constraint("1.0.0"), ("=".to_string(), "1.0.0".to_string()));
    }

    #[test]
    fn test_matches_constraint() {
        // Exact match
        assert!(matches_constraint("1.0.0", "=", "1.0.0"));
        assert!(!matches_constraint("1.0.1", "=", "1.0.0"));

        // Caret (^)
        assert!(matches_constraint("1.0.0", "^", "1.0.0"));
        assert!(matches_constraint("1.2.3", "^", "1.0.0"));
        assert!(!matches_constraint("2.0.0", "^", "1.0.0"));

        // Tilde (~)
        assert!(matches_constraint("1.0.0", "~", "1.0.0"));
        assert!(matches_constraint("1.0.5", "~", "1.0.0"));
        assert!(!matches_constraint("1.1.0", "~", "1.0.0"));

        // Greater/Less
        assert!(matches_constraint("1.1.0", ">=", "1.0.0"));
        assert!(matches_constraint("0.9.0", "<", "1.0.0"));
    }

    #[test]
    fn test_local_registry_new() {
        let registry = LocalRegistry::new();
        assert!(registry.registry_dir().ends_with("registry"));
    }
}
