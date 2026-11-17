//! Names the build, so menus and the dedicated server can say which one runs.
//!
//! Sets `SHOOTER_BUILD` (a release tag such as `v0.3.0`, or the short commit
//! hash that also names the CI build zips) and `SHOOTER_BUILD_DATE` (the
//! commit's date, `YYYY-MM-DD`, or empty). GitHub Actions supplies the commit
//! and tag through its environment; elsewhere they come from git. A build
//! outside a git checkout is called `dev`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    for var in ["GITHUB_SHA", "GITHUB_REF_TYPE", "GITHUB_REF_NAME"] {
        println!("cargo:rerun-if-env-changed={var}");
    }
    watch_git_head();

    let env = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    let commit = env("GITHUB_SHA")
        .or_else(|| git(&["rev-parse", "HEAD"]))
        .map(|sha| sha.chars().take(7).collect::<String>());
    let tag = env("GITHUB_REF_NAME").filter(|_| env("GITHUB_REF_TYPE").as_deref() == Some("tag"));
    let build = tag.or(commit).unwrap_or_else(|| "dev".to_owned());
    let date = git(&["show", "-s", "--format=%cs", "HEAD"]).unwrap_or_default();

    println!("cargo:rustc-env=SHOOTER_BUILD={build}");
    println!("cargo:rustc-env=SHOOTER_BUILD_DATE={date}");
}

/// Runs git in this crate's directory; `None` if it fails or prints nothing.
fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!text.is_empty()).then_some(text)
}

/// Reruns this script when HEAD moves: a new commit, a checkout, or a branch
/// switch. Only files that exist are watched, because a missing one makes
/// Cargo rerun the script (and rebuild the crate) on every build.
fn watch_git_head() {
    let Some(git_dir) = git(&["rev-parse", "--absolute-git-dir"]).map(PathBuf::from) else {
        return;
    };
    let common_dir = git(&["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .map_or_else(|| git_dir.clone(), PathBuf::from);
    let head = git_dir.join("HEAD");
    let mut watched = vec![head.clone(), common_dir.join("packed-refs")];
    if let Ok(text) = std::fs::read_to_string(&head)
        && let Some(reference) = text.strip_prefix("ref:")
    {
        watched.push(common_dir.join(reference.trim()));
    }
    for path in watched.iter().filter(|p| Path::exists(p)) {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}
