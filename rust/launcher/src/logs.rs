//! Per-launch output, kept outside build folders so updates preserve crash evidence.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

const KEEP_LOGS: usize = 20;

pub struct SessionLog {
    pub path: PathBuf,
    file: File,
}

impl SessionLog {
    pub fn open(dir: &Path, build: &str) -> Result<Self> {
        fs::create_dir_all(dir).context("creating the client log folder")?;
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = dir.join(format!("client-{stamp}-{}.log", std::process::id()));
        let mut file = OpenOptions::new()
            .create_new(true)
            .append(true)
            .open(&path)
            .context("creating the client log")?;
        writeln!(
            file,
            "Shooter build {build}; session {stamp} (Unix nanoseconds)"
        )?;
        file.flush()?;
        // Best effort: inability to remove an older log must not prevent play.
        let _ = prune(dir);
        println!("Client log: {}", path.display());
        Ok(Self { path, file })
    }

    pub fn capture(&self, command: &mut Command) -> Result<()> {
        // Cloned handles share an append-only file, with no pipe reader that
        // could lose the last messages or block the client when it crashes.
        command.stdout(self.file.try_clone()?);
        command.stderr(self.file.try_clone()?);
        Ok(())
    }

    pub fn finish(&mut self, status: ExitStatus) -> Result<()> {
        writeln!(self.file, "\nShooter exited: {status}")?;
        self.file.flush()?;
        Ok(())
    }
}

fn prune(dir: &Path) -> Result<()> {
    let mut logs: Vec<_> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("client-") && name.ends_with(".log")
        })
        .collect();
    logs.sort_by_key(|entry| entry.file_name());
    let excess = logs.len().saturating_sub(KEEP_LOGS);
    for entry in logs.into_iter().take(excess) {
        let _ = fs::remove_file(entry.path().with_extension("pending"));
        let _ = fs::remove_file(entry.path());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retains_recent_sessions_and_preserves_unrelated_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("other.log"), "keep").unwrap();
        for i in 0..25 {
            fs::write(dir.path().join(format!("client-{i:020}.log")), "old").unwrap();
        }
        let log = SessionLog::open(dir.path(), "test-build").unwrap();
        assert!(
            fs::read_to_string(&log.path)
                .unwrap()
                .contains("test-build")
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), KEEP_LOGS + 1);
        assert!(!dir.path().join("client-00000000000000000000.log").exists());
        assert!(dir.path().join("other.log").exists());
        let other = SessionLog::open(dir.path(), "second-build").unwrap();
        assert_ne!(log.path, other.path);
        assert!(log.path.exists());
    }

    #[test]
    fn captures_both_streams_and_nonzero_exit() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = SessionLog::open(dir.path(), "crashing-build").unwrap();
        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new("cmd");
            command.args(["/C", "echo output & echo error 1>&2 & exit /b 7"]);
            command
        };
        #[cfg(not(windows))]
        let mut command = {
            let mut command = Command::new("sh");
            command.args(["-c", "echo output; echo error >&2; exit 7"]);
            command
        };
        log.capture(&mut command).unwrap();
        let status = command.status().unwrap();
        assert_eq!(status.code(), Some(7));
        log.finish(status).unwrap();
        let text = fs::read_to_string(&log.path).unwrap();
        for expected in ["crashing-build", "output", "error", "Shooter exited:"] {
            assert!(text.contains(expected), "missing {expected}: {text}");
        }
    }
}
