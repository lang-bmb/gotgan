//! Gotgan - BMB Package Manager
//!
//! Create and manage BMB projects with Rust ecosystem support.

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

mod build;
mod config;
mod error;
mod lock;
mod project;
mod resolver;

use build::{run_build, run_check, run_clean, run_run, run_test, run_tree, run_update, run_verify, BuildOptions};
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
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::FAILURE
        }
    }
}
