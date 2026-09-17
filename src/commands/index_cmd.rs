// The `rpf index` subcommand: build, inspect, or clear the cached game-wide
// texture index that `screenshot` uses to resolve external texture
// dictionaries. See `crate::index` for what the index actually holds.

use anyhow::{bail, Context, Result};

use crate::index::GameIndex;
use crate::rpf::GtaKeys;

#[derive(clap::Args)]
pub struct IndexArgs {
    #[command(subcommand)]
    pub command: IndexCommand,
}

#[derive(clap::Subcommand)]
pub enum IndexCommand {
    /// Build the texture index and write it to the cache (rebuilds even if
    /// a cache entry already exists)
    Build,
    /// Show where the cache for this game build would live, and how big it
    /// is if built
    Info,
    /// Delete the cached index for this game build
    Clear,
}

pub fn run(args: &IndexArgs, keys: Option<&GtaKeys>, exe: Option<&std::path::Path>) -> Result<()> {
    let exe = exe.context("--exe or GTAV_PATH is required for `rpf index`")?;
    let exe_path = crate::keys::resolve_exe(exe)?;
    let game_root = exe_path.parent().context("--exe has no parent directory")?.to_path_buf();
    let cache_path = GameIndex::cache_path(&exe_path);

    match &args.command {
        IndexCommand::Build => {
            println!("Building texture index for {}...", game_root.display());
            let index = GameIndex::build(&game_root, keys)?;
            let (ytds, archetypes, resident) = index.len();
            println!("{ytds} dictionaries, {archetypes} archetypes, {resident} resident textures");

            let path = cache_path.context("no cache directory available (no HOME/USERPROFILE?)")?;
            index.save_cached(&path)?;
            println!("Wrote {}", path.display());
            Ok(())
        }
        IndexCommand::Info => {
            let Some(path) = cache_path else {
                println!("No cache directory available (no HOME/USERPROFILE?)");
                return Ok(());
            };
            println!("Cache path: {}", path.display());
            if !path.is_file() {
                println!("Not built yet — run `rpf index build`.");
                return Ok(());
            }
            let size = std::fs::metadata(&path)?.len();
            let index = GameIndex::load_cached(&path)?;
            let (ytds, archetypes, resident) = index.len();
            println!("Size: {size} bytes");
            println!("{ytds} dictionaries, {archetypes} archetypes, {resident} resident textures");
            Ok(())
        }
        IndexCommand::Clear => {
            let Some(path) = cache_path else {
                bail!("no cache directory available (no HOME/USERPROFILE?)");
            };
            if path.is_file() {
                std::fs::remove_file(&path)?;
                println!("Removed {}", path.display());
            } else {
                println!("Nothing to remove at {}", path.display());
            }
            Ok(())
        }
    }
}
