//! Keeps the launcher itself current: when the manifest lists a different
//! launcher binary for this platform, download it, swap it in, and restart.

use std::env;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::fetch;
use crate::manifest::{self, Manifest};

/// Set on the restarted launcher, so a bad manifest cannot cause a loop.
const RESTARTED: &str = "SHOOTER_LAUNCHER_RESTARTED";
/// Set to anything to keep the launcher as it is.
const DISABLED: &str = "SHOOTER_LAUNCHER_NO_SELF_UPDATE";

/// Replaces the running launcher if the manifest has a different one.
/// Returns whether it did; the caller then calls [`restart`]. Problems are
/// reported and skipped: an old launcher still starts the game.
pub fn update(manifest: &Manifest, manifest_source: &str, platform: &str, scratch: &Path) -> bool {
    // Debug builds come from `cargo run`, not from a release.
    if cfg!(debug_assertions) || env::var_os(DISABLED).is_some() || env::var_os(RESTARTED).is_some()
    {
        return false;
    }
    let Some(wanted) = manifest.launchers.get(platform) else {
        return false;
    };
    match try_update(wanted, manifest_source, scratch) {
        Ok(updated) => updated,
        Err(error) => {
            eprintln!("Could not update the launcher: {error:#}");
            false
        }
    }
}

fn try_update(wanted: &manifest::Asset, manifest_source: &str, scratch: &Path) -> Result<bool> {
    let exe = env::current_exe().context("finding the launcher")?;
    if fetch::file_sha256(&exe)?.eq_ignore_ascii_case(&wanted.sha256) {
        return Ok(false);
    }
    println!("Updating the launcher...");
    let new = scratch.join(&wanted.file);
    let source = manifest::asset_source(manifest_source, &wanted.file);
    fetch::download(&source, &new, wanted, "Launcher")?;
    let replaced = self_replace::self_replace(&new).context("replacing the launcher");
    let _ = fs::remove_file(&new);
    replaced?;
    Ok(true)
}

/// Starts the updated launcher with the same arguments. On Unix it takes over
/// this process; on Windows this process waits for it and passes on its exit
/// code.
pub fn restart(exe: &Path) -> Result<()> {
    let mut command = std::process::Command::new(exe);
    command.args(env::args_os().skip(1)).env(RESTARTED, "1");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(command.exec()).context("restarting the launcher")
    }
    #[cfg(not(unix))]
    {
        let status = command.status().context("restarting the launcher")?;
        std::process::exit(status.code().unwrap_or(1));
    }
}
