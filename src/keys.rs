//! Finding the game executable that the keys are recovered from.
//!
//! The recovery itself lives in rpf-archive, which carries the magic data the
//! NG keys come out of.

use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

use crate::rpf::GtaKeys;

/// Derive the keys straight from a game executable, ready to use.
pub fn from_exe(exe_path: &Path) -> Result<GtaKeys> {
    GtaKeys::extract_from_exe(exe_path, None)
}

/// Recover the keys for `exe_path` and write them into `out_dir` as the
/// `gtav_*.dat` files that `--keys` reads back.
pub fn extract(exe_path: &Path, out_dir: &Path) -> Result<()> {
    GtaKeys::extract_from_exe(exe_path, Some(out_dir))?;
    Ok(())
}

/// Accept either the executable itself or the folder holding it.
pub fn resolve_exe(path: &Path) -> Result<PathBuf> {
    if path.is_dir() {
        for name in ["GTA5.exe", "GTA5_Enhanced.exe"] {
            let candidate = path.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        bail!("no GTA5.exe found in {}", path.display());
    }
    Ok(path.to_path_buf())
}
