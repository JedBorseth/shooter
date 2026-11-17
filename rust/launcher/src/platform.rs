//! Platform names, as the Build workflow uses them in file names and in the
//! manifest.

/// The client build for the machine the launcher runs on.
pub fn client() -> Option<&'static str> {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some("windows-x86_64")
    } else if cfg!(target_os = "macos") {
        Some("macos-universal")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("linux-x86_64")
    } else {
        None
    }
}

/// The headless dedicated server build.
pub const SERVER: &str = "server-linux-x86_64";

/// Where builds go unless `--dir` says otherwise: `%LOCALAPPDATA%\Shooter` on
/// Windows, `~/Library/Application Support/Shooter` on macOS, and
/// `~/.local/share/shooter` on Linux.
pub fn default_dir() -> Option<std::path::PathBuf> {
    let name = if cfg!(any(target_os = "windows", target_os = "macos")) {
        "Shooter"
    } else {
        "shooter"
    };
    dirs::data_local_dir().map(|dir| dir.join(name))
}
