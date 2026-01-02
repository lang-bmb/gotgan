//! Gotgan - BMB Package Manager
//!
//! Create and manage BMB projects with Rust ecosystem support.

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

mod config;
mod project;

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
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::New { name, path, lib } => {
            let project_path = path.unwrap_or_else(|| PathBuf::from(&name));
            create_project(&name, &project_path, lib)
        }
        Commands::Init { name, lib } => {
            let current_dir = std::env::current_dir().expect("Failed to get current directory");
            let project_name = name.unwrap_or_else(|| {
                current_dir
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unnamed".to_string())
            });
            init_project(&project_name, &current_dir, lib)
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::FAILURE
        }
    }
}
