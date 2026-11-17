//! Shooter launcher: keeps the latest build of the game installed and starts it.
//!
//! Every push to `main` publishes a build to the `nightly` GitHub release with
//! a `latest.json` manifest. The launcher reads the manifest, downloads the
//! build for this machine if it is new, checks its SHA-256, unpacks it next to
//! the previous build, and starts it. Offline, it starts what it has.
//!
//! `shooter-launcher serve` does the same for a dedicated server and keeps it
//! running, switching builds whenever the server is empty.

mod fetch;
mod manifest;
mod platform;
mod self_update;
mod serve;
mod store;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use crate::store::{Installed, Store};

const DEFAULT_MANIFEST: &str =
    "https://github.com/JedBorseth/shooter/releases/download/nightly/latest.json";

#[derive(Parser)]
#[command(version, about = "Keeps Shooter up to date and starts it.")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
    #[command(flatten)]
    common: Common,
    #[command(flatten)]
    play: PlayArgs,
}

#[derive(clap::Args)]
struct Common {
    /// Where the latest build is described (a URL or a local path to latest.json).
    #[arg(long, global = true, env = "SHOOTER_MANIFEST", default_value = DEFAULT_MANIFEST)]
    manifest: String,
    /// Folder that holds the installed builds.
    #[arg(long, global = true, env = "SHOOTER_DIR")]
    dir: Option<PathBuf>,
}

#[derive(clap::Args)]
struct PlayArgs {
    /// Start the installed build without checking for a new one.
    #[arg(long)]
    offline: bool,
    /// Start the build before the current one (also skips the update check).
    #[arg(long)]
    previous: bool,
    /// Game options, after `--`: `shooter-launcher -- --connect 10.0.0.5`.
    #[arg(last = true)]
    game_args: Vec<OsString>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a dedicated server, switching to each new build when it is empty.
    Serve {
        /// Minutes between checks for a new build.
        #[arg(long, default_value_t = 5)]
        check_minutes: u64,
        /// Server options, after `--`: `serve -- --name "EU 1" --bots 3`.
        #[arg(last = true)]
        server_args: Vec<OsString>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error:#}");
            pause_on_windows();
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    // Read before a self-update can rename the file away.
    let launcher_exe = std::env::current_exe().context("finding the launcher")?;
    let dir = match cli.common.dir {
        Some(dir) => dir,
        None => platform::default_dir().context("no folder for game data on this machine")?,
    };
    match cli.command {
        Some(Commands::Serve {
            check_minutes,
            server_args,
        }) => {
            serve::check_platform()?;
            serve::run(
                serve::Options {
                    dir,
                    manifest: cli.common.manifest,
                    check_every: Duration::from_secs(check_minutes.max(1) * 60),
                    server_args,
                },
                &launcher_exe,
            )
        }
        None => play(&dir, &cli.common.manifest, cli.play, &launcher_exe),
    }
}

fn play(dir: &Path, manifest_source: &str, args: PlayArgs, launcher_exe: &Path) -> Result<()> {
    let platform = platform::client().context("there is no Shooter build for this machine")?;
    let store = Store::open(&dir.join(platform))?;

    if !args.offline
        && !args.previous
        && let Err(error) = update(&store, manifest_source, platform, launcher_exe)
    {
        if store.current().is_none() {
            return Err(error);
        }
        eprintln!("Could not update: {error:#}");
        eprintln!("Starting the installed build instead.");
    }

    let installed = if args.previous {
        store.previous().context("there is no previous build")?
    } else {
        store
            .current()
            .context("no build is installed yet; run the launcher once while online")?
    };
    let exe = store.exe_path(&installed);
    let build_dir = store.build_dir(&installed.build);
    // Let the next launcher run update while this game is open.
    drop(store);
    launch(&installed, &exe, &build_dir, &args.game_args)
}

/// Installs the newest build (and launcher) if this machine does not have them.
fn update(store: &Store, manifest_source: &str, platform: &str, launcher_exe: &Path) -> Result<()> {
    println!("Checking for a new build...");
    let manifest = fetch::manifest(manifest_source)?;
    if self_update::update(&manifest, manifest_source, platform, &store.downloads_dir()) {
        return self_update::restart(launcher_exe);
    }
    let build = manifest.build_for(platform)?;
    if store.current().is_some_and(|c| c.build == manifest.build) {
        println!("Build {} is up to date.", manifest.build);
        return Ok(());
    }
    println!(
        "Downloading build {} ({} MB)...",
        manifest.build,
        build.asset.size.div_ceil(1 << 20)
    );
    let installed = store.install(&manifest.build, build, manifest_source)?;
    store.set_current(&installed)
}

fn launch(
    installed: &Installed,
    exe: &Path,
    build_dir: &Path,
    game_args: &[OsString],
) -> Result<()> {
    if !exe.is_file() {
        bail!(
            "build {} is missing {}; delete {} and run the launcher again",
            installed.build,
            installed.exe,
            build_dir.display()
        );
    }
    println!("Starting build {}.", installed.build);
    let mut command = Command::new(exe);
    command.current_dir(build_dir);
    if !game_args.is_empty() {
        command.arg("--").args(game_args);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(command.exec()).with_context(|| format!("starting {}", exe.display()))
    }
    #[cfg(not(unix))]
    {
        command
            .spawn()
            .with_context(|| format!("starting {}", exe.display()))?;
        Ok(())
    }
}

/// Double-clicking the launcher on Windows opens a console that closes as soon
/// as it exits; keep it open long enough to read the error.
fn pause_on_windows() {
    if cfg!(windows) {
        eprintln!("Press Enter to close.");
        let _ = std::io::stdin().read_line(&mut String::new());
    }
}
