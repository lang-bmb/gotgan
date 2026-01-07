//! Build system for gotgan
//!
//! Wraps the bmb compiler to build, run, check, verify, and test BMB projects.

#![allow(dead_code)]

use crate::config::{ConfigError, Manifest};
use crate::lock::{LockError, LockFile, LOCK_FILE_NAME};
use crate::resolver::{DependencyResolver, ResolveError, ResolvedDep};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use thiserror::Error;

/// Build options
#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub release: bool,
    pub output: Option<PathBuf>,
}

/// Errors that can occur during build operations
#[derive(Error, Debug)]
pub enum BuildError {
    #[error("Not in a gotgan project (gotgan.toml not found)")]
    NotInProject,

    #[error("Config error: {0}")]
    ConfigError(#[from] ConfigError),

    #[error("No source files found in src/")]
    NoSourceFiles,

    #[error("Main entry point not found: src/main.bmb")]
    NoMainFile,

    #[error("Failed to run bmb compiler: {0}")]
    CompilerError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Build failed with exit code: {0}")]
    BuildFailed(i32),

    #[error("bmb compiler not found in PATH")]
    CompilerNotFound,

    #[error("Dependency error: {0}")]
    ResolveError(#[from] ResolveError),

    #[error("Lock file error: {0}")]
    LockError(#[from] LockError),
}

/// Project context loaded from gotgan.toml
pub struct ProjectContext {
    pub root: PathBuf,
    pub manifest: Manifest,
    pub src_dir: PathBuf,
    pub target_dir: PathBuf,
    pub dependencies: Vec<ResolvedDep>,
    /// Whether the lock file was up to date
    pub lock_up_to_date: bool,
}

impl ProjectContext {
    /// Find and load project context from current directory or parents
    pub fn find() -> Result<Self, BuildError> {
        let current = std::env::current_dir()?;
        let root = find_project_root(&current).ok_or(BuildError::NotInProject)?;

        let manifest_path = root.join("gotgan.toml");
        let manifest = Manifest::load(&manifest_path)?;

        let src_dir = root.join("src");
        let target_dir = root.join("target");

        // Resolve local path dependencies
        let mut resolver = DependencyResolver::new(root.clone());
        let dependencies = resolver.resolve(&manifest)?;

        if !dependencies.is_empty() {
            println!(
                "   Resolved {} local {}",
                dependencies.len(),
                if dependencies.len() == 1 { "dependency" } else { "dependencies" }
            );
        }

        // Check lock file
        let lock_path = root.join(LOCK_FILE_NAME);
        let lock_up_to_date = if lock_path.exists() {
            match LockFile::load(&lock_path) {
                Ok(lock) => lock.matches(&dependencies),
                Err(_) => false,
            }
        } else {
            false
        };

        Ok(Self {
            root,
            manifest,
            src_dir,
            target_dir,
            dependencies,
            lock_up_to_date,
        })
    }

    /// Save lock file for current dependencies
    pub fn save_lock_file(&self) -> Result<(), BuildError> {
        let lock = LockFile::from_resolved(&self.dependencies);
        let lock_path = self.root.join(LOCK_FILE_NAME);
        lock.save(&lock_path)?;
        Ok(())
    }

    /// Get the main entry point file
    pub fn main_file(&self) -> PathBuf {
        self.src_dir.join("main.bmb")
    }

    /// Get the library file
    pub fn lib_file(&self) -> PathBuf {
        self.src_dir.join("lib.bmb")
    }

    /// Get all BMB source files (project only)
    pub fn source_files(&self) -> Result<Vec<PathBuf>, BuildError> {
        let mut files = Vec::new();
        collect_bmb_files(&self.src_dir, &mut files)?;
        if files.is_empty() {
            return Err(BuildError::NoSourceFiles);
        }
        Ok(files)
    }

    /// Get all BMB source files including dependencies
    pub fn all_source_files(&self) -> Result<Vec<PathBuf>, BuildError> {
        let mut files = Vec::new();

        // Add dependency source files first (build order)
        for dep in &self.dependencies {
            files.extend(dep.source_files.clone());
        }

        // Then add project source files
        collect_bmb_files(&self.src_dir, &mut files)?;

        if files.is_empty() {
            return Err(BuildError::NoSourceFiles);
        }
        Ok(files)
    }

    /// Get the output binary path
    pub fn output_binary(&self, release: bool, custom_output: Option<&Path>) -> PathBuf {
        if let Some(out) = custom_output {
            out.to_path_buf()
        } else {
            let subdir = if release { "release" } else { "debug" };
            let binary_name = if cfg!(windows) {
                format!("{}.exe", self.manifest.package.name)
            } else {
                self.manifest.package.name.clone()
            };
            self.target_dir.join(subdir).join(binary_name)
        }
    }
}

/// Find project root by searching for gotgan.toml
fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join("gotgan.toml").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Recursively collect .bmb files
fn collect_bmb_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), BuildError> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            collect_bmb_files(&path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "bmb") {
            files.push(path);
        }
    }

    Ok(())
}

/// Find bmb compiler in PATH or common locations
fn find_bmb_compiler() -> Option<PathBuf> {
    // Try PATH first
    if let Ok(output) = Command::new("bmb").arg("--version").output()
        && output.status.success() {
            return Some(PathBuf::from("bmb"));
        }

    // Try common locations
    let candidates = [
        PathBuf::from("./target/release/bmb"),
        PathBuf::from("./target/debug/bmb"),
        PathBuf::from("../bmb/target/release/bmb"),
        PathBuf::from("../bmb/target/debug/bmb"),
    ];

    for candidate in candidates {
        let candidate = if cfg!(windows) {
            candidate.with_extension("exe")
        } else {
            candidate
        };

        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

/// Run bmb compiler with given arguments
fn run_bmb(args: &[&str]) -> Result<(), BuildError> {
    let bmb = find_bmb_compiler().ok_or(BuildError::CompilerNotFound)?;

    println!("Running: bmb {}", args.join(" "));

    let status = Command::new(&bmb)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(BuildError::BuildFailed(status.code().unwrap_or(-1)))
    }
}

/// Build the project
pub fn run_build(opts: BuildOptions) -> Result<(), BuildError> {
    let ctx = ProjectContext::find()?;

    println!(
        "   Building {} v{}",
        ctx.manifest.package.name, ctx.manifest.package.version
    );

    // Print dependency info
    for dep in &ctx.dependencies {
        println!(
            "   Compiling {} v{} ({})",
            dep.name,
            dep.version,
            dep.path.display()
        );
    }

    // Check for main.bmb (binary project)
    let main_file = ctx.main_file();
    if !main_file.exists() {
        // Check for lib.bmb (library project)
        let lib_file = ctx.lib_file();
        if !lib_file.exists() {
            return Err(BuildError::NoMainFile);
        }
        // Library: just type-check
        println!("   Library project detected, running type check...");
        return run_check();
    }

    // Create target directory
    let output = ctx.output_binary(opts.release, opts.output.as_deref());
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Build with bmb build
    let main_str = main_file.to_string_lossy();
    let output_str = output.to_string_lossy();

    let mut args = vec!["build", &main_str, "-o", &output_str];
    if opts.release {
        args.push("-O3");
    }

    run_bmb(&args)?;

    // Update lock file if needed
    if !ctx.lock_up_to_date && !ctx.dependencies.is_empty() {
        ctx.save_lock_file()?;
        println!("   Updated {}", LOCK_FILE_NAME);
    }

    println!("   Finished {} target", if opts.release { "release" } else { "debug" });
    println!("   Binary: {}", output.display());

    Ok(())
}

/// Build and run the project
pub fn run_run(opts: BuildOptions, _program_args: Vec<String>) -> Result<(), BuildError> {
    let ctx = ProjectContext::find()?;

    // Check for main.bmb
    let main_file = ctx.main_file();
    if !main_file.exists() {
        return Err(BuildError::NoMainFile);
    }

    println!(
        "     Running {} v{}",
        ctx.manifest.package.name, ctx.manifest.package.version
    );

    // Use bmb run (interpreter) for now
    // TODO: When LLVM is available, use build + execute for --release
    let main_str = main_file.to_string_lossy();

    if opts.release {
        // Build and run native binary
        run_build(opts.clone())?;

        let binary = ctx.output_binary(opts.release, opts.output.as_deref());
        println!("     Running `{}`", binary.display());

        let status = Command::new(&binary)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()?;

        if status.success() {
            Ok(())
        } else {
            Err(BuildError::BuildFailed(status.code().unwrap_or(-1)))
        }
    } else {
        // Use interpreter for debug mode
        run_bmb(&["run", &main_str])
    }
}

/// Type-check the project
pub fn run_check() -> Result<(), BuildError> {
    let ctx = ProjectContext::find()?;

    println!(
        "    Checking {} v{}",
        ctx.manifest.package.name, ctx.manifest.package.version
    );

    // Get all source files
    let sources = ctx.source_files()?;

    // Check each file
    for source in &sources {
        let source_str = source.to_string_lossy();
        run_bmb(&["check", &source_str])?;
    }

    println!("    Finished type checking");
    Ok(())
}

/// Verify contracts in the project
pub fn run_verify(file: Option<PathBuf>) -> Result<(), BuildError> {
    let ctx = ProjectContext::find()?;

    println!(
        "   Verifying {} v{}",
        ctx.manifest.package.name, ctx.manifest.package.version
    );

    if let Some(specific_file) = file {
        // Verify specific file
        let file_str = specific_file.to_string_lossy();
        run_bmb(&["verify", &file_str])?;
    } else {
        // Verify all source files
        let sources = ctx.source_files()?;
        for source in &sources {
            let source_str = source.to_string_lossy();
            println!("   Verifying {}", source.display());
            run_bmb(&["verify", &source_str])?;
        }
    }

    println!("   Finished verification");
    Ok(())
}

/// Run tests in the project
pub fn run_test(filter: Option<String>, verbose: bool) -> Result<(), BuildError> {
    let ctx = ProjectContext::find()?;

    println!(
        "    Testing {} v{}",
        ctx.manifest.package.name, ctx.manifest.package.version
    );

    // Look for test files in tests/ directory
    let tests_dir = ctx.root.join("tests");
    let mut test_files = Vec::new();

    if tests_dir.exists() {
        collect_bmb_files(&tests_dir, &mut test_files)?;
    }

    // Also look for test_ prefix files in src/
    let src_files = ctx.source_files()?;
    for file in src_files {
        if let Some(name) = file.file_name()
            && name.to_string_lossy().starts_with("test_") {
                test_files.push(file);
            }
    }

    if test_files.is_empty() {
        println!("   No test files found");
        return Ok(());
    }

    // Run tests
    for test_file in &test_files {
        let file_str = test_file.to_string_lossy();

        let mut args = vec!["test", &file_str];

        let filter_str;
        if let Some(ref f) = filter {
            filter_str = f.clone();
            args.push("--filter");
            args.push(&filter_str);
        }

        if verbose {
            args.push("-v");
        }

        run_bmb(&args)?;
    }

    println!("   Finished testing");
    Ok(())
}

/// Clean build artifacts
pub fn run_clean() -> Result<(), BuildError> {
    let ctx = ProjectContext::find()?;

    println!(
        "    Cleaning {} v{}",
        ctx.manifest.package.name, ctx.manifest.package.version
    );

    if ctx.target_dir.exists() {
        std::fs::remove_dir_all(&ctx.target_dir)?;
        println!("   Removed {}", ctx.target_dir.display());
    } else {
        println!("   Nothing to clean (target/ does not exist)");
    }

    Ok(())
}

/// Display dependency tree
pub fn run_tree(show_all: bool) -> Result<(), BuildError> {
    let ctx = ProjectContext::find()?;

    println!(
        "{} v{}",
        ctx.manifest.package.name, ctx.manifest.package.version
    );

    if ctx.dependencies.is_empty() {
        println!("└── (no dependencies)");
        return Ok(());
    }

    let dep_count = ctx.dependencies.len();
    for (i, dep) in ctx.dependencies.iter().enumerate() {
        let is_last = i == dep_count - 1;
        let prefix = if is_last { "└──" } else { "├──" };

        if show_all {
            println!(
                "{} {} v{} ({})",
                prefix,
                dep.name,
                dep.version,
                dep.path.display()
            );
            // Show source files count
            let child_prefix = if is_last { "    " } else { "│   " };
            println!(
                "{}└── {} source file(s)",
                child_prefix,
                dep.source_files.len()
            );
        } else {
            println!("{} {} v{}", prefix, dep.name, dep.version);
        }
    }

    Ok(())
}

/// Update dependencies and regenerate lock file
pub fn run_update() -> Result<(), BuildError> {
    let ctx = ProjectContext::find()?;

    println!(
        "   Updating {} v{}",
        ctx.manifest.package.name, ctx.manifest.package.version
    );

    if ctx.dependencies.is_empty() {
        println!("   No dependencies to lock");
        return Ok(());
    }

    // Always regenerate lock file
    ctx.save_lock_file()?;

    println!(
        "   Locked {} {}",
        ctx.dependencies.len(),
        if ctx.dependencies.len() == 1 {
            "dependency"
        } else {
            "dependencies"
        }
    );

    for dep in &ctx.dependencies {
        println!("   - {} v{}", dep.name, dep.version);
    }

    println!("   Updated {}", LOCK_FILE_NAME);
    Ok(())
}

/// Add a dependency to the project
pub fn run_add(name: &str, path: Option<String>, version: Option<String>, dev: bool, local: bool) -> Result<(), BuildError> {
    use crate::config::{Dependency, DetailedDependency};
    use crate::registry::{LocalRegistry, RegistryClient};

    let current = std::env::current_dir()?;
    let root = find_project_root(&current).ok_or(BuildError::NotInProject)?;

    let manifest_path = root.join("gotgan.toml");
    let mut manifest = Manifest::load(&manifest_path)?;

    // Check if dependency already exists
    let deps = if dev {
        &manifest.dev_dependencies
    } else {
        &manifest.dependencies
    };

    if deps.contains_key(name) {
        eprintln!(
            "   Warning: {} '{}' already exists, updating",
            if dev { "Dev dependency" } else { "Dependency" },
            name
        );
    }

    // Create dependency entry
    let (dep, version_str) = if let Some(ref dep_path) = path {
        // Local path dependency
        (Dependency::Detailed(DetailedDependency {
            version: None,
            path: Some(dep_path.clone()),
            git: None,
            branch: None,
            features: Vec::new(),
            optional: false,
        }), None)
    } else if local {
        // Local registry dependency
        let local_registry = LocalRegistry::new();

        let resolved_version = match &version {
            Some(v) => {
                // Resolve version constraint from local registry
                match local_registry.resolve_version(name, v) {
                    Ok(resolved) => resolved,
                    Err(_) => {
                        eprintln!("   Error: Package '{}' version '{}' not found in local registry", name, v);
                        return Err(BuildError::CompilerError(format!(
                            "Package '{}' version '{}' not found in local registry", name, v
                        )));
                    }
                }
            }
            None => {
                // Get latest version from local registry
                match local_registry.get_latest_version(name) {
                    Ok(v) => v,
                    Err(_) => {
                        eprintln!("   Error: Package '{}' not found in local registry", name);
                        return Err(BuildError::CompilerError(format!(
                            "Package '{}' not found in local registry", name
                        )));
                    }
                }
            }
        };

        // Get package path from local registry
        let package_path = local_registry.get_package_path(name, &resolved_version)
            .map_err(|e| BuildError::CompilerError(e.to_string()))?;

        (Dependency::Detailed(DetailedDependency {
            version: Some(resolved_version.clone()),
            path: Some(package_path.to_string_lossy().to_string()),
            git: None,
            branch: None,
            features: Vec::new(),
            optional: false,
        }), Some(resolved_version))
    } else {
        // Remote registry dependency
        let registry = RegistryClient::new();

        let resolved_version = match &version {
            Some(v) => v.clone(),
            None => {
                // Try to get latest version from registry
                match registry.get_latest_version(name) {
                    Ok(v) => v,
                    Err(_) => {
                        // If registry lookup fails, use a placeholder
                        eprintln!("   Note: Package '{}' not found in registry, using version '*'", name);
                        "*".to_string()
                    }
                }
            }
        };

        (Dependency::Detailed(DetailedDependency {
            version: Some(resolved_version.clone()),
            path: None,
            git: None,
            branch: None,
            features: Vec::new(),
            optional: false,
        }), Some(resolved_version))
    };

    // Add to manifest
    if dev {
        manifest.dev_dependencies.insert(name.to_string(), dep);
    } else {
        manifest.dependencies.insert(name.to_string(), dep);
    }

    // Save manifest
    manifest.save(&manifest_path)?;

    let dep_type = if dev { "dev-dependency" } else { "dependency" };
    let registry_type = if local { " (local)" } else { "" };
    println!("      Adding {} as {}{}", name, dep_type, registry_type);

    if let Some(ref p) = path {
        println!("        Path: {}", p);
    } else if let Some(v) = version_str {
        println!("        Version: {}", v);
    }

    Ok(())
}

/// Search for packages in the registry
pub fn run_search(query: &str, limit: usize) -> Result<(), BuildError> {
    use crate::registry::RegistryClient;

    println!("    Searching for '{}'...", query);

    let registry = RegistryClient::new();
    let results = registry.search(query).map_err(|e| BuildError::CompilerError(e.to_string()))?;

    if results.is_empty() {
        println!("   No packages found matching '{}'", query);
        println!();
        println!("   Note: The BMB package registry is still being set up.");
        println!("         Once available, packages will be listed here.");
        return Ok(());
    }

    println!();
    println!("   Found {} package(s):", results.len().min(limit));
    println!();

    for result in results.iter().take(limit) {
        println!("   {} ({})", result.name, result.latest_version);
        if let Some(ref desc) = result.description {
            println!("       {}", desc);
        }
        println!();
    }

    Ok(())
}

/// Publish a package to the registry
pub fn run_publish(skip_confirm: bool, dry_run: bool, local: bool) -> Result<(), BuildError> {
    use crate::registry::{create_package_archive, generate_package_info, LocalRegistry};
    use std::io::{self, Write};

    let ctx = ProjectContext::find()?;
    let manifest = &ctx.manifest;

    let registry_type = if local { "local registry" } else { "registry" };
    println!(
        "   Publishing {} v{} to {}",
        manifest.package.name, manifest.package.version, registry_type
    );

    // Validate package metadata
    if manifest.package.description.is_none() {
        eprintln!("   Warning: 'description' is recommended for published packages");
    }
    if manifest.package.license.is_none() {
        eprintln!("   Warning: 'license' is recommended for published packages");
    }
    if manifest.package.repository.is_none() && !local {
        eprintln!("   Warning: 'repository' is recommended for published packages");
    }

    // Create package archive
    let archive_name = format!("{}-{}.tar.gz", manifest.package.name, manifest.package.version);
    let archive_path = ctx.target_dir.join(&archive_name);

    std::fs::create_dir_all(&ctx.target_dir)?;

    println!("   Creating archive: {}", archive_name);
    create_package_archive(&ctx.root, &archive_path)
        .map_err(|e| BuildError::CompilerError(e.to_string()))?;

    // Calculate checksum
    let archive_bytes = std::fs::read(&archive_path)?;
    let checksum = simple_sha256_hex(&archive_bytes);
    println!("   Checksum: {}", &checksum[..16]);

    // Generate package info for registry
    let package_info = generate_package_info(manifest, &checksum);
    let package_json = serde_json::to_string_pretty(&package_info)
        .map_err(|e| BuildError::CompilerError(e.to_string()))?;

    if dry_run {
        println!();
        println!("   [DRY RUN] Would publish with the following metadata:");
        println!("{}", package_json);
        println!();
        println!("   Archive created at: {}", archive_path.display());
        return Ok(());
    }

    // Confirm with user
    if !skip_confirm {
        print!("   Publish {} v{} to {}? [y/N]: ", manifest.package.name, manifest.package.version, registry_type);
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("   Cancelled");
            return Ok(());
        }
    }

    if local {
        // Publish to local registry
        let local_registry = LocalRegistry::new();

        match local_registry.publish(&ctx.root, manifest, &checksum) {
            Ok(package_dir) => {
                println!();
                println!("   ✓ Published to local registry:");
                println!("     {}", package_dir.display());
                println!();
                println!("   To use this package:");
                println!("     gotgan add {} --local", manifest.package.name);
            }
            Err(e) => {
                return Err(BuildError::CompilerError(format!("Failed to publish: {}", e)));
            }
        }
    } else {
        // For now, show instructions for manual publishing to remote registry
        println!();
        println!("   To publish to the BMB registry:");
        println!();
        println!("   1. Create a GitHub release in lang-bmb/registry");
        println!("   2. Upload: {}", archive_path.display());
        println!("   3. Add to registry index.json:");
        println!();
        println!("{}", package_json);
        println!();
        println!("   Note: Automated publishing will be available in a future version.");
    }

    Ok(())
}

/// Simple SHA-256 hash (placeholder - uses basic hash for now)
fn simple_sha256_hex(data: &[u8]) -> String {
    // Simple hash for demonstration (not cryptographically secure)
    // In production, use sha2 crate
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in data {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}{:016x}{:016x}{:016x}", hash, hash.rotate_left(16), hash.rotate_left(32), hash.rotate_left(48))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;
    use std::fs;

    #[test]
    fn test_find_project_root() {
        let temp = temp_dir().join("gotgan_test_root");
        let _ = fs::remove_dir_all(&temp);

        // Create nested project
        let project = temp.join("project");
        let subdir = project.join("src").join("submodule");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(project.join("gotgan.toml"), "[package]\nname=\"test\"\nversion=\"0.1.0\"").unwrap();

        // Find from subdir
        let found = find_project_root(&subdir);
        assert_eq!(found, Some(project.clone()));

        // Find from project root
        let found = find_project_root(&project);
        assert_eq!(found, Some(project.clone()));

        // Cleanup
        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_collect_bmb_files() {
        let temp = temp_dir().join("gotgan_test_collect");
        let _ = fs::remove_dir_all(&temp);

        let src = temp.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("main.bmb"), "fn main() = 0;").unwrap();
        fs::write(src.join("lib.bmb"), "fn add(a: i64, b: i64) = a + b;").unwrap();
        fs::write(src.join("readme.txt"), "Not a BMB file").unwrap();

        let mut files = Vec::new();
        collect_bmb_files(&src, &mut files).unwrap();

        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|f| f.ends_with("main.bmb")));
        assert!(files.iter().any(|f| f.ends_with("lib.bmb")));

        // Cleanup
        let _ = fs::remove_dir_all(&temp);
    }
}
