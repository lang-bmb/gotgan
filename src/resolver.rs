//! Dependency resolution for local path dependencies

use crate::config::{ConfigError, Dependency, Manifest};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
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
}

/// A resolved dependency with its absolute path
#[derive(Debug, Clone)]
pub struct ResolvedDep {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub source_files: Vec<PathBuf>,
}

/// Dependency resolver for local path dependencies
pub struct DependencyResolver {
    /// Root project path
    root: PathBuf,
    /// Resolved dependencies cache
    resolved: HashMap<String, ResolvedDep>,
    /// Currently resolving (for cycle detection)
    resolving: HashSet<String>,
}

impl DependencyResolver {
    /// Create a new resolver for the given project root
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            resolved: HashMap::new(),
            resolving: HashSet::new(),
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
                if let Some(path) = &detailed.path {
                    path.clone()
                } else if detailed.git.is_some() {
                    eprintln!(
                        "Warning: Git dependencies not yet supported, skipping '{}'",
                        name
                    );
                    return Ok(None);
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
            version: dep_manifest.package.version,
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
        } else if path.extension().map_or(false, |ext| ext == "bmb") {
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
