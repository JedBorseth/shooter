//! `latest.json`: what the CI workflow published last, and where to get it.
//!
//! The workflow writes the manifest next to the build zips (see
//! `.github/scripts/write-manifest.sh`). File names in it are relative to the
//! manifest's own address, so the whole set can be served from any static host.

use std::collections::BTreeMap;
use std::path::{Component, Path};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// The newest manifest layout this launcher understands.
pub const SCHEMA: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Manifest {
    pub schema: u32,
    /// Short commit hash; names the build.
    pub build: String,
    pub commit: String,
    /// The game protocol the build speaks (`PROTOCOL_VERSION`).
    pub protocol: u16,
    /// When the workflow published it (RFC 3339).
    pub published: String,
    /// Game builds by platform: `linux-x86_64`, `windows-x86_64`,
    /// `macos-universal`, `server-linux-x86_64`.
    pub builds: BTreeMap<String, Build>,
    /// Launcher binaries by platform, for the launcher to update itself.
    #[serde(default)]
    pub launchers: BTreeMap<String, Asset>,
}

/// A file next to the manifest.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Asset {
    pub file: String,
    pub size: u64,
    /// Lowercase hex SHA-256 of the file.
    pub sha256: String,
}

/// A zipped game build.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Build {
    #[serde(flatten)]
    pub asset: Asset,
    /// The executable to start, relative to the unzipped build.
    pub exe: String,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Manifest> {
        let manifest: Manifest = serde_json::from_str(text).context("reading latest.json")?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Rejects names that could escape the install folder or the host.
    fn validate(&self) -> Result<()> {
        check_name(&self.build)?;
        for (platform, build) in &self.builds {
            check_asset(&build.asset).with_context(|| format!("build for {platform}"))?;
            check_relative(&build.exe).with_context(|| format!("build for {platform}"))?;
        }
        for (platform, launcher) in &self.launchers {
            check_asset(launcher).with_context(|| format!("launcher for {platform}"))?;
        }
        Ok(())
    }

    pub fn build_for(&self, platform: &str) -> Result<&Build> {
        if self.schema > SCHEMA {
            bail!(
                "the latest build needs a newer launcher; download it again from the releases page"
            );
        }
        self.builds
            .get(platform)
            .with_context(|| format!("the latest build has nothing for {platform}"))
    }
}

/// The address of `file`, which sits next to the manifest at `manifest_source`
/// (a URL or a local path).
pub fn asset_source(manifest_source: &str, file: &str) -> String {
    match manifest_source.rsplit_once(['/', '\\']) {
        Some((base, _)) => format!("{base}/{file}"),
        None => file.to_owned(),
    }
}

fn check_asset(asset: &Asset) -> Result<()> {
    check_name(&asset.file)?;
    if asset.sha256.len() != 64 || !asset.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("bad sha256 for {}", asset.file);
    }
    Ok(())
}

/// A plain file name: letters, digits, `.`, `_`, `-`, and not `.` or `..`.
fn check_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if !ok {
        bail!("bad file name {name:?}");
    }
    Ok(())
}

fn check_relative(path: &str) -> Result<()> {
    let ok = !path.is_empty()
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)));
    if !ok {
        bail!("bad executable path {path:?}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "schema": 1,
        "build": "aebe7ae",
        "commit": "aebe7ae0000000000000000000000000000000000",
        "protocol": 10,
        "published": "2026-11-17T01:30:00Z",
        "builds": {
            "windows-x86_64": {
                "file": "shooter-aebe7ae-windows-x86_64.zip",
                "size": 300,
                "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                "exe": "shooter.exe"
            },
            "macos-universal": {
                "file": "shooter-aebe7ae-macos-universal.zip",
                "size": 300,
                "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                "exe": "Shooter.app/Contents/MacOS/Shooter"
            }
        },
        "launchers": {
            "windows-x86_64": {
                "file": "shooter-launcher-windows-x86_64.exe",
                "size": 5,
                "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            }
        }
    }"#;

    #[test]
    fn parses_the_published_layout() {
        let manifest = Manifest::parse(SAMPLE).unwrap();
        assert_eq!(manifest.build, "aebe7ae");
        assert_eq!(manifest.protocol, 10);
        let mac = manifest.build_for("macos-universal").unwrap();
        assert_eq!(mac.exe, "Shooter.app/Contents/MacOS/Shooter");
        assert_eq!(mac.asset.file, "shooter-aebe7ae-macos-universal.zip");
        assert!(manifest.build_for("linux-x86_64").is_err());
    }

    #[test]
    fn rejects_paths_that_escape() {
        let escaping_file = SAMPLE.replace("shooter-aebe7ae-windows-x86_64.zip", "../evil.zip");
        assert!(Manifest::parse(&escaping_file).is_err());
        let escaping_exe = SAMPLE.replace("\"shooter.exe\"", "\"../../bin/sh\"");
        assert!(Manifest::parse(&escaping_exe).is_err());
        let absolute_exe = SAMPLE.replace("\"shooter.exe\"", "\"/bin/sh\"");
        assert!(Manifest::parse(&absolute_exe).is_err());
        let bad_hash = SAMPLE.replacen("0123456789abcdef0123", "not-a-hash", 1);
        assert!(Manifest::parse(&bad_hash).is_err());
    }

    #[test]
    fn newer_schema_asks_for_a_new_launcher() {
        let newer = SAMPLE.replace("\"schema\": 1", "\"schema\": 2");
        let manifest = Manifest::parse(&newer).unwrap();
        let error = manifest.build_for("windows-x86_64").unwrap_err();
        assert!(error.to_string().contains("newer launcher"));
    }

    #[test]
    fn assets_sit_next_to_the_manifest() {
        assert_eq!(
            asset_source(
                "https://example.com/releases/download/nightly/latest.json",
                "a.zip"
            ),
            "https://example.com/releases/download/nightly/a.zip"
        );
        assert_eq!(
            asset_source("/srv/builds/latest.json", "a.zip"),
            "/srv/builds/a.zip"
        );
        assert_eq!(asset_source("latest.json", "a.zip"), "a.zip");
    }
}
