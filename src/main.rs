use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod rpf;
mod commands;
mod utils;

use commands::{info, list, extract, verify, tree};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(name = "rpf")]
#[command(about = "A CLI tool for working with RAGE Package Files (RPF)", long_about = None)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Display information about an RPF archive
    Info {
        /// Path to the RPF archive
        archive: PathBuf,
    },
    
    /// List files in an RPF archive
	/// If you want to search for specific files
    List {
        /// Path to the RPF archive
        archive: PathBuf,
        
        /// Pattern to filter files (e.g., "*.xml")
        pattern: Option<String>,
        
        /// Show detailed information
        #[arg(short, long)]
        detailed: bool,
    },
    
    /// Extract files from an RPF archive
    Extract {
        /// Path to the RPF archive
        archive: PathBuf,
        
        /// Output directory (defaults to archive name without extension)
        #[arg(short, long, value_name = "DIR")]
        output: Option<PathBuf>,
        
        /// Specific file or pattern to extract
        pattern: Option<String>, // TODO: Do some more testing on this
    },
    
    /// Verify integrity of an RPF archive
    Verify {
        /// Path to the RPF archive
        archive: PathBuf,
    },
    
    /// Display archive contents in tree format
	/// Good for feeding an LLM for example
    Tree {
        /// Path to the RPF archive
        archive: PathBuf,
        
        /// Maximum depth to display
        #[arg(short, long)]
        depth: Option<usize>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(if cli.verbose { "debug" } else { "info" })).init();
    
    match cli.command {
        Commands::Info { archive } => info::run(&archive),
        Commands::List { archive, pattern, detailed } => list::run(&archive, pattern.as_deref(), detailed),
        Commands::Extract { archive, output, pattern } => extract::run(&archive, output.as_deref(), pattern.as_deref()),
        Commands::Verify { archive } => verify::run(&archive),
        Commands::Tree { archive, depth } => tree::run(&archive, depth),
    }
} 