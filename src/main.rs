use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

mod rpf;
mod commands;
mod utils;

use commands::{info, list, extract, verify, tree};
use rpf::GtaKeys;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(name = "rpf")]
#[command(about = "A CLI tool for working with RAGE Package Files (RPF)", long_about = None)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Directory with extracted GTA V keys (gtav_aes_key.dat, gtav_ng_key.dat, gtav_ng_decrypt_tables.dat)
    #[arg(long, global = true, value_name = "DIR")]
    keys: Option<PathBuf>,

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
        pattern: Option<String>,
    },

    /// Verify integrity of an RPF archive
    Verify {
        /// Path to the RPF archive
        archive: PathBuf,
    },

    /// Display archive contents in tree format
    Tree {
        /// Path to the RPF archive
        archive: PathBuf,

        /// Maximum depth to display
        #[arg(short, long)]
        depth: Option<usize>,
    },

    /// Extract AES/NG keys from a GTA5.exe binary
    ExtractKeys {
        /// Path to GTA5.exe
        #[arg(long, value_name = "FILE")]
        exe: PathBuf,

        /// Directory to save extracted keys into
        #[arg(short, long, value_name = "DIR")]
        output: PathBuf,
    },
}

fn load_keys(path: Option<&Path>) -> Result<Option<GtaKeys>> {
    match path {
        Some(p) => Ok(Some(GtaKeys::load_from_path(p)?)),
        None    => Ok(None),
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(if cli.verbose { "debug" } else { "info" })
    ).init();

    let keys = load_keys(cli.keys.as_deref())?;

    match cli.command {
        Commands::Info        { archive }                    => info::run(&archive, keys.as_ref()),
        Commands::List        { archive, pattern, detailed } => list::run(&archive, pattern.as_deref(), detailed, keys.as_ref()),
        Commands::Extract     { archive, output, pattern }   => extract::run(&archive, output.as_deref(), pattern.as_deref(), keys.as_ref()),
        Commands::Verify      { archive }                    => verify::run(&archive, keys.as_ref()),
        Commands::Tree        { archive, depth }             => tree::run(&archive, depth, keys.as_ref()),
        Commands::ExtractKeys { exe, output }                => {
            GtaKeys::extract_from_exe(&exe, Some(&output))?;
            Ok(())
        }
    }
}
