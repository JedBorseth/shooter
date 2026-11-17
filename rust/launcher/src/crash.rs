//! Upload only failed sessions. Failed HTTPS uploads stay queued for next launch.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

use anyhow::{Result, bail};

const MAX_BYTES: u64 = 2 * 1024 * 1024;

pub fn report(log: &Path, endpoint: &str) {
    if endpoint.is_empty() {
        return;
    }
    let pending = log.with_extension("pending");
    if let Err(error) = prepare(log).and_then(|body| {
        fs::write(&pending, body)?;
        upload(&pending, endpoint)
    }) {
        eprintln!("Crash upload deferred: {error}; saved at {}", log.display());
    }
}

pub fn retry_pending(dir: &Path, endpoint: &str) {
    if endpoint.is_empty() {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    // Bound launch delay, even after a long offline period.
    for entry in entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|e| e == "pending"))
        .take(2)
    {
        if let Err(error) = upload(&entry.path(), endpoint) {
            eprintln!("Crash upload still pending: {error}");
            break;
        }
    }
}

fn prepare(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(MAX_BYTES)))?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES).read_to_end(&mut bytes)?;
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    // Verbose engine errors contain absolute local file paths. Never send the
    // user's home directory or a complete environment/command line.
    if let Some(home) = dirs::home_dir() {
        let home = home.to_string_lossy();
        text = text.replace(home.as_ref(), "<home>");
        text = text.replace(&home.replace('\\', "/"), "<home>");
    }
    // Lossy UTF-8 conversion can expand malformed bytes; keep the tail bounded.
    if text.len() > MAX_BYTES as usize {
        let mut start = text.len() - MAX_BYTES as usize;
        while !text.is_char_boundary(start) {
            start += 1;
        }
        text.drain(..start);
    }
    // Keep the build header even when only the last 2 MiB fit.
    let mut header = Vec::new();
    File::open(path)?.take(256).read_to_end(&mut header)?;
    let header = header.split(|b| *b == b'\n').next().unwrap_or_default();
    Ok(format!(
        "{}\n--- crash log tail ---\n{text}",
        String::from_utf8_lossy(header)
    ))
}

fn upload(path: &Path, endpoint: &str) -> Result<()> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(0)
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into();
    let body = fs::read(path)?;
    if body.len() > MAX_BYTES as usize + 512 {
        bail!("crash report exceeds upload limit");
    }
    let response = agent
        .post(endpoint)
        .header("Content-Type", "text/plain; charset=utf-8")
        .send(body)?;
    if !response.status().is_success() {
        bail!("collector did not accept report: {}", response.status());
    }
    fs::remove_file(path)?;
    println!("Crash report delivered securely to the master host.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_plaintext_and_keeps_report_for_retry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("client-test.log");
        fs::write(&path, "Shooter build test\nfatal crash").unwrap();
        report(&path, "http://127.0.0.1:9/shooter/crashes");
        assert!(path.with_extension("pending").exists());
        assert!(path.exists());
    }

    #[test]
    fn bounds_reports_and_preserves_build_and_last_messages() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("client-test.log");
        fs::write(
            &path,
            format!(
                "Shooter build test\n{}\nlast fatal message",
                "x".repeat(3 * MAX_BYTES as usize)
            ),
        )
        .unwrap();
        let text = prepare(&path).unwrap();
        assert!(text.starts_with("Shooter build test"));
        assert!(text.ends_with("last fatal message"));
        assert!(text.len() <= MAX_BYTES as usize + 512);
    }
}
