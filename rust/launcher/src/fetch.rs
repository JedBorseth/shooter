//! Reads the manifest and downloads files, from `http(s)://` URLs or local
//! paths (a folder on a LAN share works as a build host too).

use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Digest, Sha256};

use crate::manifest::{Asset, Manifest};

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .user_agent(concat!("shooter-launcher/", env!("CARGO_PKG_VERSION")))
            .build()
            .into()
    })
}

fn is_url(source: &str) -> bool {
    source.starts_with("http://") || source.starts_with("https://")
}

fn open(source: &str) -> Result<(Box<dyn Read>, Option<u64>)> {
    if is_url(source) {
        let response = agent()
            .get(source)
            .call()
            .with_context(|| format!("downloading {source}"))?;
        let length = response.body().content_length();
        Ok((Box::new(response.into_body().into_reader()), length))
    } else {
        let file = File::open(source).with_context(|| format!("opening {source}"))?;
        let length = file.metadata().ok().map(|m| m.len());
        Ok((Box::new(file), length))
    }
}

pub fn manifest(source: &str) -> Result<Manifest> {
    let (reader, _) = open(source)?;
    let mut text = String::new();
    // A manifest is a few kilobytes; anything near this is not one.
    reader
        .take(1 << 20)
        .read_to_string(&mut text)
        .with_context(|| format!("reading {source}"))?;
    Manifest::parse(&text)
}

/// Downloads `source` to `dest`, checking its size and SHA-256 against
/// `expected`. Writes to `dest.part` first, so `dest` only ever holds a whole,
/// verified file.
pub fn download(source: &str, dest: &Path, expected: &Asset, label: &str) -> Result<()> {
    let partial = dest.with_extension("part");
    let result = download_to(source, &partial, expected, label);
    match result {
        Ok(()) => fs::rename(&partial, dest).with_context(|| format!("saving {}", dest.display())),
        Err(error) => {
            let _ = fs::remove_file(&partial);
            Err(error)
        }
    }
}

fn download_to(source: &str, partial: &Path, expected: &Asset, label: &str) -> Result<()> {
    let (mut reader, length) = open(source)?;
    let bar = progress_bar(length.unwrap_or(expected.size), label);
    let mut out = BufWriter::new(
        File::create(partial).with_context(|| format!("creating {}", partial.display()))?,
    );
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    let mut buffer = vec![0u8; 256 * 1024];
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error).with_context(|| format!("downloading {source}")),
        };
        total += read as u64;
        if total > expected.size {
            bail!("{} is larger than the manifest says", expected.file);
        }
        hasher.update(&buffer[..read]);
        out.write_all(&buffer[..read])?;
        bar.set_position(total);
    }
    out.into_inner()
        .map_err(io::IntoInnerError::into_error)?
        .sync_all()?;
    bar.finish_and_clear();
    if total != expected.size {
        bail!(
            "{} stopped after {total} of {} bytes",
            expected.file,
            expected.size
        );
    }
    let actual = hex(&hasher.finalize());
    if !actual.eq_ignore_ascii_case(&expected.sha256) {
        bail!("{} is damaged (SHA-256 {actual})", expected.file);
    }
    Ok(())
}

pub fn file_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => hasher.update(&buffer[..read]),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn progress_bar(length: u64, label: &str) -> ProgressBar {
    let bar = ProgressBar::new(length);
    bar.set_style(
        ProgressStyle::with_template(
            "{msg} [{bar:30}] {bytes}/{total_bytes} {bytes_per_sec}, {eta} left",
        )
        .expect("valid template")
        .progress_chars("=> "),
    );
    bar.set_message(label.to_owned());
    bar
}
