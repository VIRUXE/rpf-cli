//! Shared per-user directory layout.

use std::path::PathBuf;

/// `~/.rpf-cli` — the per-user directory holding the key cache and the
/// update-check stamp. The home directory is `HOME` first, then
/// `USERPROFILE`, so this also matches whatever shell (Git Bash/MSYS
/// included) the process is running under.
pub fn config_root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)?;
    Some(home.join(".rpf-cli"))
}
