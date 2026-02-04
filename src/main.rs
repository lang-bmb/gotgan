//! Gotgan - BMB Package Manager
//!
//! Create and manage BMB projects with Rust ecosystem support.

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

mod bmbx;
mod build;
mod cache;
mod config;
mod error;
mod lock;
mod project;
mod registry;
mod resolver;

use bmbx::{run_bundle, run_compat_check, run_explore};
use build::{run_add, run_build, run_check, run_clean, run_publish, run_run, run_search, run_test, run_tree, run_update, run_verify, BuildOptions};
use error::GotganError;
use project::{create_project, init_project};

#[derive(Parser)]
#[command(name = "gotgan")]
#[command(author, version, about = "BMB Package Manager", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new BMB project
    New {
        /// Project name
        name: String,

        /// Project directory (defaults to project name)
        #[arg(short, long)]
        path: Option<PathBuf>,

        /// Use library template instead of binary
        #[arg(long)]
        lib: bool,
    },

    /// Initialize a BMB project in the current directory
    Init {
        /// Project name (defaults to directory name)
        #[arg(short, long)]
        name: Option<String>,

        /// Use library template instead of binary
        #[arg(long)]
        lib: bool,
    },

    /// Build the project
    Build {
        /// Build in release mode with optimizations
        #[arg(long)]
        release: bool,

        /// Output directory for build artifacts
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Build and run the project
    Run {
        /// Build in release mode
        #[arg(long)]
        release: bool,

        /// Arguments to pass to the program
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// Type-check the project without building
    Check,

    /// Verify contracts using SMT solver
    Verify {
        /// Specific file to verify (defaults to all)
        #[arg(short, long)]
        file: Option<PathBuf>,
    },

    /// Run tests
    Test {
        /// Filter tests by pattern
        #[arg(short, long)]
        filter: Option<String>,

        /// Show verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Remove build artifacts (target directory)
    Clean,

    /// Display dependency tree
    Tree {
        /// Show all transitive dependencies
        #[arg(short, long)]
        all: bool,
    },

    /// Update dependencies and regenerate lock file
    Update,

    /// Add a dependency to the project
    Add {
        /// Dependency name
        name: String,

        /// Path to local dependency
        #[arg(short, long)]
        path: Option<String>,

        /// Version constraint (e.g., "1.0", "^2.0")
        #[arg(short, long)]
        version: Option<String>,

        /// Add as dev dependency
        #[arg(long)]
        dev: bool,

        /// Use local registry (~/.gotgan/registry/)
        #[arg(long)]
        local: bool,
    },

    /// Search for packages in the registry
    Search {
        /// Search query
        query: String,

        /// Maximum number of results
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },

    /// Publish a package to the registry
    Publish {
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,

        /// Dry run (don't actually publish)
        #[arg(long)]
        dry_run: bool,

        /// Publish to local registry (~/.gotgan/registry/)
        #[arg(long)]
        local: bool,
    },

    /// Create BMBX bundle with contracts, symbols, and types
    Bundle {
        /// Output directory for BMBX files
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Create single-file bundle (.bmbx)
        #[arg(long)]
        single_file: bool,
    },

    /// Explore package symbols and contracts (AI-Native discovery)
    Explore {
        /// Package path (defaults to current directory)
        #[arg(short, long)]
        path: Option<PathBuf>,

        /// Show symbols index
        #[arg(long)]
        symbols: bool,

        /// Show contracts
        #[arg(long)]
        contracts: bool,

        /// Show types
        #[arg(long)]
        types: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Filter by function/type name pattern
        #[arg(short, long)]
        filter: Option<String>,
    },

    /// Check contract compatibility between versions
    Compat {
        /// Path to old contracts.json (or use saved target/bmbx/contracts.json)
        #[arg(long)]
        old: Option<PathBuf>,

        /// Path to new contracts.json (or generate from current project)
        #[arg(long)]
        new: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result: Result<(), GotganError> = match cli.command {
        Commands::New { name, path, lib } => {
            let project_path = path.unwrap_or_else(|| PathBuf::from(&name));
            create_project(&name, &project_path, lib).map_err(GotganError::from)
        }
        Commands::Init { name, lib } => {
            let current_dir = std::env::current_dir().expect("Failed to get current directory");
            let project_name = name.unwrap_or_else(|| {
                current_dir
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unnamed".to_string())
            });
            init_project(&project_name, &current_dir, lib).map_err(GotganError::from)
        }
        Commands::Build { release, output } => {
            let opts = BuildOptions { release, output };
            run_build(opts).map_err(GotganError::from)
        }
        Commands::Run { release, args } => {
            let opts = BuildOptions {
                release,
                output: None,
            };
            run_run(opts, args).map_err(GotganError::from)
        }
        Commands::Check => run_check().map_err(GotganError::from),
        Commands::Verify { file } => run_verify(file).map_err(GotganError::from),
        Commands::Test { filter, verbose } => run_test(filter, verbose).map_err(GotganError::from),
        Commands::Clean => run_clean().map_err(GotganError::from),
        Commands::Tree { all } => run_tree(all).map_err(GotganError::from),
        Commands::Update => run_update().map_err(GotganError::from),
        Commands::Add { name, path, version, dev, local } => run_add(&name, path, version, dev, local).map_err(GotganError::from),
        Commands::Search { query, limit } => run_search(&query, limit).map_err(GotganError::from),
        Commands::Publish { yes, dry_run, local } => run_publish(yes, dry_run, local).map_err(GotganError::from),
        Commands::Bundle { output, single_file } => run_bundle(output, single_file),
        Commands::Explore { path, symbols, contracts, types, json, filter } => {
            run_explore(path, symbols, contracts, types, json, filter)
        }
        Commands::Compat { old, new } => run_compat_check(old, new),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::FAILURE
        }
    }
}
