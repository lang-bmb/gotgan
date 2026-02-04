//! Dependency resolution for local path and git dependencies
//! Supports Go-style git URL dependencies (github.com/user/repo)

#![allow(dead_code)]

use crate::config::{ConfigError, Dependency, Manifest};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

/// Errors during dependency resolution
#[derive(Error, Debug)]
pub enum ResolveError {
    #[error("Config error: {0}")]
    ConfigError(#[from] ConfigError),

    #[error("Dependency not found: {name} at path {path}")]
    DependencyNotFound { name: String, path: String },

    #[error("Dependency {name} has no gotgan.toml")]
    NoManifest { name: String },

    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Git clone failed for {url}: {message}")]
    GitCloneFailed { url: String, message: String },

    #[error("Git checkout failed for {url} ref {git_ref}: {message}")]
    GitCheckoutFailed { url: String, git_ref: String, message: String },
}

/// A resolved dependency with its absolute path
#[derive(Debug, Clone)]
pub struct ResolvedDep {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub source_files: Vec<PathBuf>,
}

/// Cache directory for git dependencies (~/.gotgan/cache)
fn get_cache_dir() -> PathBuf {
    // Try HOME (Unix) or USERPROFILE (Windows)
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".gotgan")
        .join("cache")
}

/// Dependency resolver for local path and git dependencies
pub struct DependencyResolver {
    /// Root project path
    root: PathBuf,
    /// Resolved dependencies cache
    resolved: HashMap<String, ResolvedDep>,
    /// Currently resolving (for cycle detection)
    resolving: HashSet<String>,
    /// Cache directory for git dependencies
    cache_dir: PathBuf,
}

impl DependencyResolver {
    /// Create a new resolver for the given project root
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            resolved: HashMap::new(),
            resolving: HashSet::new(),
            cache_dir: get_cache_dir(),
        }
    }

    /// Resolve all dependencies for the project
    pub fn resolve(&mut self, manifest: &Manifest) -> Result<Vec<ResolvedDep>, ResolveError> {
        let mut deps = Vec::new();

        for (name, dep) in &manifest.dependencies {
            if let Some(resolved) = self.resolve_dependency(name, dep)? {
                deps.push(resolved);
            }
        }

        Ok(deps)
    }

    /// Resolve a single dependency
    fn resolve_dependency(
        &mut self,
        name: &str,
        dep: &Dependency,
    ) -> Result<Option<ResolvedDep>, ResolveError> {
        // Check if already resolved
        if let Some(resolved) = self.resolved.get(name) {
            return Ok(Some(resolved.clone()));
        }

        // Extract path if it's a path dependency
        let dep_path = match dep {
            Dependency::Simple(_version) => {
                // Version-only dependencies are not supported yet (requires registry)
                // Just skip them with a warning
                eprintln!(
                    "Warning: Registry dependencies not yet supported, skipping '{}'",
                    name
                );
                return Ok(None);
            }
            Dependency::Detailed(detailed) => {
                if let Some(git_url) = &detailed.git {
                    // Go-style git dependency
                    let cache_path = self.resolve_git_dependency(
                        name,
                        git_url,
                        detailed.tag.as_deref(),
                        detailed.branch.as_deref(),
                        detailed.rev.as_deref(),
                    )?;
                    // If path is specified, it's a monorepo - append the subpath
                    if let Some(subpath) = &detailed.path {
                        cache_path.join(subpath).to_string_lossy().to_string()
                    } else {
                        cache_path.to_string_lossy().to_string()
                    }
                } else if let Some(path) = &detailed.path {
                    // Local path dependency (no git)
                    path.clone()
                } else if detailed.version.is_some() {
                    eprintln!(
                        "Warning: Registry dependencies not yet supported, skipping '{}'",
                        name
                    );
                    return Ok(None);
                } else {
                    return Err(ResolveError::DependencyNotFound {
                        name: name.to_string(),
                        path: "no path specified".to_string(),
                    });
                }
            }
        };

        // Resolve path relative to project root
        let abs_path = if Path::new(&dep_path).is_absolute() {
            PathBuf::from(&dep_path)
        } else {
            self.root.join(&dep_path)
        };

        // Canonicalize path
        let abs_path = abs_path.canonicalize().map_err(|_| ResolveError::DependencyNotFound {
            name: name.to_string(),
            path: dep_path.clone(),
        })?;

        // Check for circular dependencies
        if self.resolving.contains(name) {
            return Err(ResolveError::CircularDependency(name.to_string()));
        }
        self.resolving.insert(name.to_string());

        // Load dependency's manifest
        let manifest_path = abs_path.join("gotgan.toml");
        if !manifest_path.exists() {
            return Err(ResolveError::NoManifest {
                name: name.to_string(),
            });
        }

        let dep_manifest = Manifest::load(&manifest_path)?;

        // Recursively resolve transitive dependencies
        let mut sub_resolver = DependencyResolver::new(abs_path.clone());
        sub_resolver.resolved = self.resolved.clone();
        sub_resolver.resolving = self.resolving.clone();

        let transitive_deps = sub_resolver.resolve(&dep_manifest)?;

        // Add transitive deps to our resolved set
        for dep in transitive_deps {
            self.resolved.insert(dep.name.clone(), dep);
        }

        // Collect source files
        let source_files = collect_source_files(&abs_path.join("src"))?;

        let resolved = ResolvedDep {
            name: name.to_string(),
            version: dep_manifest.package().version.clone(),
            path: abs_path,
            source_files,
        };

        // Cache and return
        self.resolved.insert(name.to_string(), resolved.clone());
        self.resolving.remove(name);

        Ok(Some(resolved))
    }

    /// Get all resolved dependencies in build order (dependencies first)
    pub fn build_order(&self) -> Vec<&ResolvedDep> {
        // For now, just return all deps (simple topological order not needed for path deps)
        self.resolved.values().collect()
    }

    /// Resolve a git dependency (Go-style: github.com/user/repo)
    /// Returns the local path where the dependency is cached
    fn resolve_git_dependency(
        &self,
        name: &str,
        git_url: &str,
        tag: Option<&str>,
        branch: Option<&str>,
        rev: Option<&str>,
    ) -> Result<PathBuf, ResolveError> {
        // Normalize URL: add https:// if not present
        let full_url = if git_url.starts_with("http://") || git_url.starts_with("https://") {
            git_url.to_string()
        } else {
            format!("https://{}", git_url)
        };

        // Determine git ref (priority: rev > tag > branch > default)
        let git_ref = rev.or(tag).or(branch).unwrap_or("HEAD");

        // Create cache directory path
        // ~/.gotgan/cache/github.com/user/repo@v1.0.0
        let safe_url = git_url
            .replace("https://", "")
            .replace("http://", "")
            .replace('/', "_");
        let cache_name = format!("{}@{}", safe_url, git_ref);
        let cache_path = self.cache_dir.join(&cache_name);

        // Check if already cached
        if cache_path.exists() && cache_path.join("gotgan.toml").exists() {
            eprintln!("   Using cached: {} @ {}", name, git_ref);
            return Ok(cache_path);
        }

        // Create cache directory
        std::fs::create_dir_all(&self.cache_dir).map_err(|e| ResolveError::IoError(e))?;

        eprintln!("   Fetching: {} from {} @ {}", name, git_url, git_ref);

        // Clone the repository
        let clone_result = Command::new("git")
            .args(["clone", "--depth", "1"])
            .args(if let Some(b) = branch {
                vec!["--branch", b]
            } else if let Some(t) = tag {
                vec!["--branch", t]
            } else {
                vec![]
            })
            .arg(&full_url)
            .arg(&cache_path)
            .output();

        match clone_result {
            Ok(output) if output.status.success() => {
                // If rev is specified, checkout that specific commit
                if let Some(r) = rev {
                    let checkout_result = Command::new("git")
                        .args(["checkout", r])
                        .current_dir(&cache_path)
                        .output();

                    if let Ok(output) = checkout_result {
                        if !output.status.success() {
                            return Err(ResolveError::GitCheckoutFailed {
                                url: git_url.to_string(),
                                git_ref: r.to_string(),
                                message: String::from_utf8_lossy(&output.stderr).to_string(),
                            });
                        }
                    }
                }
                Ok(cache_path)
            }
            Ok(output) => {
                Err(ResolveError::GitCloneFailed {
                    url: git_url.to_string(),
                    message: String::from_utf8_lossy(&output.stderr).to_string(),
                })
            }
            Err(e) => {
                Err(ResolveError::GitCloneFailed {
                    url: git_url.to_string(),
                    message: e.to_string(),
                })
            }
        }
    }
}

/// Collect all .bmb source files in a directory
fn collect_source_files(dir: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut files = Vec::new();

    if !dir.exists() {
        return Ok(files);
    }

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            files.extend(collect_source_files(&path)?);
        } else if path.extension().is_some_and(|ext| ext == "bmb") {
            files.push(path);
        }
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_collect_source_files() {
        let temp = std::env::temp_dir().join("gotgan_test_sources");
        let _ = fs::remove_dir_all(&temp);

        let src = temp.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("lib.bmb"), "fn add(a: i64, b: i64) = a + b;").unwrap();
        fs::write(src.join("utils.bmb"), "fn helper() = 42;").unwrap();

        let files = collect_source_files(&src).unwrap();
        assert_eq!(files.len(), 2);

        // Cleanup
        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_resolver_empty_deps() {
        let temp = std::env::temp_dir().join("gotgan_test_resolver");
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();

        let manifest = Manifest::new("test");
        let mut resolver = DependencyResolver::new(temp.clone());
        let deps = resolver.resolve(&manifest).unwrap();

        assert!(deps.is_empty());

        // Cleanup
        let _ = fs::remove_dir_all(&temp);
    }
}
