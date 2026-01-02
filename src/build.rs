//! Build system for gotgan
//!
//! Wraps the bmb compiler to build, run, check, verify, and test BMB projects.

use crate::config::{ConfigError, Manifest};
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
}

/// Project context loaded from gotgan.toml
pub struct ProjectContext {
    pub root: PathBuf,
    pub manifest: Manifest,
    pub src_dir: PathBuf,
    pub target_dir: PathBuf,
    pub dependencies: Vec<ResolvedDep>,
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

        Ok(Self {
            root,
            manifest,
            src_dir,
            target_dir,
            dependencies,
        })
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
        } else if path.extension().map_or(false, |ext| ext == "bmb") {
            files.push(path);
        }
    }

    Ok(())
}

/// Find bmb compiler in PATH or common locations
fn find_bmb_compiler() -> Option<PathBuf> {
    // Try PATH first
    if let Ok(output) = Command::new("bmb").arg("--version").output() {
        if output.status.success() {
            return Some(PathBuf::from("bmb"));
        }
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
        if let Some(name) = file.file_name() {
            if name.to_string_lossy().starts_with("test_") {
                test_files.push(file);
            }
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
