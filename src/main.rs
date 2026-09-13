use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

mod rpf;
mod commands;
mod keys;
mod resources;
mod utils;

use commands::{info, list, extract, verify, tree, textures, screenshot, create, search};
use rpf::GtaKeys;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(name = "rpf")]
#[command(about = "A CLI tool for working with RAGE Package Files (RPF)", long_about = None)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// GTA5.exe (or the folder holding it) to read the keys from
    #[arg(long, global = true, value_name = "PATH", env = "GTAV_PATH")]
    exe: Option<PathBuf>,

    /// Directory with keys already written out by `extract-keys`; takes
    /// precedence over --exe
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

        /// Recurse into nested RPF archives, extracting them to loose files
        /// (resource files get a valid RSC7 header, like CodeWalker)
        #[arg(short, long)]
        recursive: bool,
    },

    /// Find files by name, contents or hash, descending into nested archives without extracting
    Search(search::SearchArgs),

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

    /// Export textures from a .ytd/.ydr/.ydd/.yft as PNG/JPG/WebP (or DDS with --dds)
    #[command(alias = "ytd")]
    Textures(textures::TexturesArgs),

    /// Render a .ydr/.ydd/.yft to an image
    Screenshot(screenshot::ScreenshotArgs),

    /// Create an RPF archive from a directory
    Create {
        /// Directory to pack
        input: PathBuf,

        /// Output RPF file path
        #[arg(short, long, value_name = "FILE")]
        output: PathBuf,

        /// RPF version to create (0, 2, 3, 4, 6, 7)
        #[arg(long, default_value = "7")]
        version: u8,

        /// Encryption mode (none, open, ng)
        #[arg(short, long, default_value = "none")]
        encryption: String,
    },

    /// Write the keys out to disk for reuse with --keys
    ExtractKeys {
        /// Directory to save extracted keys into
        #[arg(short, long, value_name = "DIR")]
        output: PathBuf,
    },
}

fn load_keys(exe: Option<&Path>, keys_dir: Option<&Path>) -> Result<Option<GtaKeys>> {
    // Keys win over the executable, so an explicit --keys still works when
    // GTAV_PATH is set in the environment.
    match (keys_dir, exe) {
        (Some(dir), _)  => Ok(Some(GtaKeys::load_from_path(dir)?)),
        (_, Some(exe))  => Ok(Some(keys::from_exe(&keys::resolve_exe(exe)?)?)),
        (None, None)    => Ok(None),
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(if cli.verbose { "debug" } else { "info" })
    ).init();

    let keys = load_keys(cli.exe.as_deref(), cli.keys.as_deref())?;

    match cli.command {
        Commands::Info        { archive }                    => info::run(&archive, keys.as_ref()),
        Commands::List        { archive, pattern, detailed } => list::run(&archive, pattern.as_deref(), detailed, keys.as_ref()),
        Commands::Extract     { archive, output, pattern, recursive } => extract::run(&archive, output.as_deref(), pattern.as_deref(), recursive, keys.as_ref()),
        Commands::Search(args)                               => search::run(&args, keys.as_ref()),
        Commands::Verify      { archive }                    => verify::run(&archive, keys.as_ref()),
        Commands::Tree        { archive, depth }             => tree::run(&archive, depth, keys.as_ref()),
        Commands::Textures(args)                             => textures::run(&args, keys.as_ref()),
        Commands::Screenshot(args)                           => screenshot::run(&args, keys.as_ref()),
        Commands::Create { input, output, version, encryption } => {
            create::run(&input, &output, version, &encryption, keys.as_ref())
        }
        Commands::ExtractKeys { output }                     => {
            let exe = cli.exe.context("--exe is required to extract keys")?;
            keys::extract(&keys::resolve_exe(&exe)?, &output)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    /// Ensures clap's own invariants hold for the CLI definition (e.g. no
    /// duplicate short flags within a subcommand). This catches regressions
    /// like `-v` being claimed by both the global `--verbose` and a
    /// subcommand-local argument before they can panic at runtime.
    #[test]
    fn cli_debug_assert() {
        Cli::command().debug_assert();
    }
}
