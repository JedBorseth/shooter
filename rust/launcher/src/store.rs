//! The folder of installed builds for one platform:
//!
//! ```text
//! <root>/<platform>/
//!     lock            held while the launcher changes anything here
//!     state.json      the current build and the one before it
//!     downloads/      zips while they download
//!     builds/<build>/ unzipped builds; only complete ones have this name
//! ```

use std::fs::{self, File, TryLockError};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::fetch;
use crate::manifest::{self, Build};

/// An unzipped build.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Installed {
    pub build: String,
    pub exe: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct State {
    current: Option<Installed>,
    previous: Option<Installed>,
}

pub struct Store {
    dir: PathBuf,
    // Held for the life of the store; the OS releases it when the process ends.
    _lock: File,
}

impl Store {
    /// Opens (creating if needed) the store in `dir`. Fails if another
    /// launcher is using it.
    pub fn open(dir: &Path) -> Result<Store> {
        fs::create_dir_all(dir.join("builds"))
            .with_context(|| format!("creating {}", dir.display()))?;
        fs::create_dir_all(dir.join("downloads"))?;
        let lock = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.join("lock"))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                bail!(
                    "another Shooter launcher is already updating {}",
                    dir.display()
                )
            }
            Err(TryLockError::Error(error)) => return Err(error.into()),
        }
        Ok(Store {
            dir: dir.to_owned(),
            _lock: lock,
        })
    }

    pub fn current(&self) -> Option<Installed> {
        self.state().current
    }

    pub fn previous(&self) -> Option<Installed> {
        self.state().previous
    }

    pub fn build_dir(&self, build: &str) -> PathBuf {
        self.dir.join("builds").join(build)
    }

    pub fn exe_path(&self, installed: &Installed) -> PathBuf {
        self.build_dir(&installed.build).join(&installed.exe)
    }

    pub fn downloads_dir(&self) -> PathBuf {
        self.dir.join("downloads")
    }

    fn state(&self) -> State {
        fs::read_to_string(self.dir.join("state.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save_state(&self, state: &State) -> Result<()> {
        let path = self.dir.join("state.json");
        let partial = self.dir.join("state.json.part");
        fs::write(&partial, serde_json::to_string_pretty(state)?)?;
        fs::rename(&partial, &path)?;
        Ok(())
    }

    /// Downloads and unzips `build` as `id`, unless it is already installed.
    /// `manifest_source` is where the manifest came from; the zip sits next to it.
    pub fn install(&self, id: &str, build: &Build, manifest_source: &str) -> Result<Installed> {
        let installed = Installed {
            build: id.to_owned(),
            exe: build.exe.clone(),
        };
        let final_dir = self.build_dir(id);
        if final_dir.is_dir() {
            return Ok(installed);
        }
        let zip_path = self.downloads_dir().join(&build.asset.file);
        let source = manifest::asset_source(manifest_source, &build.asset.file);
        fetch::download(&source, &zip_path, &build.asset, &format!("Build {id}"))?;

        println!("Unpacking build {id}...");
        let partial = self.dir.join("builds").join(format!("{id}.partial"));
        if partial.exists() {
            fs::remove_dir_all(&partial)?;
        }
        let unpacked = unzip(&zip_path, &partial).and_then(|()| {
            let exe = partial.join(&build.exe);
            if !exe.is_file() {
                bail!("build {id} has no {}", build.exe);
            }
            make_executable(&exe)
        });
        let _ = fs::remove_file(&zip_path);
        if let Err(error) = unpacked {
            let _ = fs::remove_dir_all(&partial);
            return Err(error);
        }
        fs::rename(&partial, &final_dir)?;
        Ok(installed)
    }

    /// Makes `installed` the build to start, keeping the old one as the
    /// previous build, and removes every other build.
    pub fn set_current(&self, installed: &Installed) -> Result<()> {
        let mut state = self.state();
        if state.current.as_ref() != Some(installed) {
            state.previous = state.current.take();
            state.current = Some(installed.clone());
            self.save_state(&state)?;
        }
        self.prune(&state);
        Ok(())
    }

    fn prune(&self, state: &State) {
        let keep: Vec<&str> = [&state.current, &state.previous]
            .into_iter()
            .flatten()
            .map(|i| i.build.as_str())
            .collect();
        let Ok(entries) = fs::read_dir(self.dir.join("builds")) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            if keep.iter().any(|k| name.to_str() == Some(k)) {
                continue;
            }
            if let Err(error) = fs::remove_dir_all(entry.path()) {
                eprintln!(
                    "Could not remove old build {}: {error}",
                    name.to_string_lossy()
                );
            }
        }
    }
}

fn unzip(zip_path: &Path, dest: &Path) -> Result<()> {
    let file = File::open(zip_path)?;
    let mut archive =
        zip::ZipArchive::new(file).with_context(|| format!("opening {}", zip_path.display()))?;
    archive
        .extract(dest)
        .with_context(|| format!("unpacking {}", zip_path.display()))?;
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(permissions.mode() | 0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::manifest::Asset;

    /// Writes a zip holding `files` and returns the matching manifest entry.
    fn make_build(dir: &Path, name: &str, files: &[(&str, &[u8])], exe: &str) -> Build {
        let path = dir.join(name);
        let mut writer = zip::ZipWriter::new(File::create(&path).unwrap());
        for (file, contents) in files {
            writer
                .start_file(*file, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(contents).unwrap();
        }
        writer.finish().unwrap();
        Build {
            asset: Asset {
                file: name.to_owned(),
                size: fs::metadata(&path).unwrap().len(),
                sha256: fetch::file_sha256(&path).unwrap(),
            },
            exe: exe.to_owned(),
        }
    }

    #[test]
    fn installs_switches_and_prunes() {
        let host = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let manifest = host.path().join("latest.json");
        let manifest = manifest.to_str().unwrap();
        let store = Store::open(home.path()).unwrap();
        assert_eq!(store.current(), None);

        let mut installed = Vec::new();
        for id in ["aaaaaaa", "bbbbbbb", "ccccccc"] {
            let build = make_build(
                host.path(),
                &format!("shooter-{id}-linux-x86_64.zip"),
                &[("shooter.x86_64", id.as_bytes()), ("shooter.pck", b"pck")],
                "shooter.x86_64",
            );
            let build = store.install(id, &build, manifest).unwrap();
            store.set_current(&build).unwrap();
            assert_eq!(
                fs::read_to_string(store.exe_path(&build)).unwrap(),
                id,
                "the new build is unpacked"
            );
            installed.push(build);
        }
        assert_eq!(store.current().as_ref(), Some(&installed[2]));
        assert_eq!(store.previous().as_ref(), Some(&installed[1]));
        assert!(
            !store.build_dir("aaaaaaa").exists(),
            "older builds are removed"
        );
        assert!(
            store.build_dir("bbbbbbb").exists(),
            "the previous build is kept"
        );
        let leftovers = fs::read_dir(store.downloads_dir()).unwrap().count();
        assert_eq!(leftovers, 0, "zips are deleted once unpacked");
    }

    #[test]
    fn a_damaged_download_installs_nothing() {
        let host = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let manifest = host.path().join("latest.json");
        let store = Store::open(home.path()).unwrap();
        let mut build = make_build(
            host.path(),
            "shooter-ddddddd-linux-x86_64.zip",
            &[("shooter.x86_64", b"game")],
            "shooter.x86_64",
        );
        build.asset.sha256 = "0".repeat(64);
        let error = store
            .install("ddddddd", &build, manifest.to_str().unwrap())
            .unwrap_err();
        assert!(error.to_string().contains("damaged"), "{error}");
        assert!(!store.build_dir("ddddddd").exists());
        assert_eq!(fs::read_dir(store.downloads_dir()).unwrap().count(), 0);
    }

    #[test]
    fn a_build_without_its_executable_is_rejected() {
        let host = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let manifest = host.path().join("latest.json");
        let store = Store::open(home.path()).unwrap();
        let build = make_build(
            host.path(),
            "shooter-eeeeeee-linux-x86_64.zip",
            &[("other", b"x")],
            "shooter.x86_64",
        );
        assert!(
            store
                .install("eeeeeee", &build, manifest.to_str().unwrap())
                .is_err()
        );
        assert!(!store.build_dir("eeeeeee").exists());
    }

    #[test]
    fn one_launcher_at_a_time() {
        let home = tempfile::tempdir().unwrap();
        let _first = Store::open(home.path()).unwrap();
        assert!(Store::open(home.path()).is_err());
    }
}
