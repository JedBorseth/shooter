//! `serve`: runs the dedicated server and keeps it on the latest build.
//!
//! Every few minutes it reads the manifest. When a newer server build is out,
//! it downloads and unpacks it while the old server keeps running, then waits
//! until no human player is connected (asking the server through its query
//! port, like the server browser does), stops the old server and starts the
//! new one. A server that exits on its own is started again.

use std::ffi::OsString;
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use shooter_sim::net::discovery::{self, Packet};

use crate::platform::SERVER;
use crate::store::{Installed, Store};
use crate::{fetch, self_update};

pub struct Options {
    pub dir: PathBuf,
    pub manifest: String,
    pub check_every: Duration,
    pub server_args: Vec<OsString>,
}

/// How often the loop wakes to look at the server.
const TICK: Duration = Duration::from_secs(5);
/// How often to ask a server with a pending update whether it is empty.
const EMPTY_POLL: Duration = Duration::from_secs(15);
/// A server that does not answer this many polls in a row is restarted
/// anyway: something is wrong with it.
const MAX_SILENT_POLLS: u32 = 8;
/// A server that ran this long before exiting is restarted at once;
/// one that keeps dying is restarted more and more slowly.
const HEALTHY_RUN: Duration = Duration::from_secs(300);
const MAX_BACKOFF: Duration = Duration::from_secs(120);

struct Running {
    child: Child,
    installed: Installed,
    started: Instant,
}

pub fn run(options: Options, launcher_exe: &Path) -> Result<()> {
    let store = Store::open(&options.dir.join(SERVER))?;
    let query_port = query_port(&options.server_args);
    println!(
        "Serving the latest build from {} (query port {query_port})",
        options.manifest
    );

    let mut pending: Option<Installed> = None;
    let mut next_check = Instant::now();
    let mut next_poll = Instant::now();
    let mut silent_polls = 0;
    let mut backoff = Duration::ZERO;
    let mut restart_at = Instant::now();
    let mut running: Option<Running> = None;

    loop {
        if Instant::now() >= next_check {
            next_check = Instant::now() + options.check_every;
            match check(
                &store,
                &options.manifest,
                running.as_ref(),
                pending.as_ref(),
                launcher_exe,
            ) {
                Ok(Some(newer)) if running.is_none() => store.set_current(&newer)?,
                Ok(Some(newer)) => {
                    if pending.as_ref() != Some(&newer) {
                        println!(
                            "Build {} is ready; it starts once the server is empty",
                            newer.build
                        );
                    }
                    pending = Some(newer);
                }
                Ok(None) => {}
                Err(error) => eprintln!("Could not check for a new build: {error:#}"),
            }
        }

        if let Some(server) = &mut running
            && let Some(status) = server.child.try_wait()?
        {
            let ran = server.started.elapsed();
            eprintln!(
                "Build {} exited ({status}) after {}s",
                server.installed.build,
                ran.as_secs()
            );
            backoff = if ran >= HEALTHY_RUN {
                Duration::ZERO
            } else {
                (backoff * 2).clamp(Duration::from_secs(5), MAX_BACKOFF)
            };
            restart_at = Instant::now() + backoff;
            running = None;
        }

        let empty = running.is_some() && pending.is_some() && Instant::now() >= next_poll && {
            next_poll = Instant::now() + EMPTY_POLL;
            match players(query_port) {
                Some(players) => {
                    silent_polls = 0;
                    players == 0
                }
                None => {
                    silent_polls += 1;
                    silent_polls >= MAX_SILENT_POLLS
                }
            }
        };
        if empty && let (Some(mut server), Some(newer)) = (running.take(), pending.take()) {
            println!(
                "Switching from build {} to {}",
                server.installed.build, newer.build
            );
            stop(&mut server.child);
            store.set_current(&newer)?;
            silent_polls = 0;
            restart_at = Instant::now();
            // The server is down, so this is the moment to swap the launcher too.
            if let Ok(manifest) = fetch::manifest(&options.manifest)
                && self_update::update(
                    &manifest,
                    &options.manifest,
                    SERVER_LAUNCHER,
                    &store.downloads_dir(),
                )
            {
                drop(store);
                return self_update::restart(launcher_exe);
            }
        }

        if running.is_none() && Instant::now() >= restart_at {
            // A server that stopped while an update waited comes back on the new build.
            if let Some(newer) = pending.take() {
                store.set_current(&newer)?;
            }
            match store.current() {
                Some(installed) => match start(&store, &installed, &options.server_args) {
                    Ok(child) => {
                        println!("Started build {} (pid {})", installed.build, child.id());
                        running = Some(Running {
                            child,
                            installed,
                            started: Instant::now(),
                        });
                    }
                    Err(error) => {
                        eprintln!("Could not start build {}: {error:#}", installed.build);
                        restart_at = Instant::now() + MAX_BACKOFF;
                    }
                },
                None => {
                    // Nothing installed yet and the first check failed; try again soon.
                    restart_at = Instant::now() + Duration::from_secs(30);
                    next_check = next_check.min(restart_at);
                }
            }
        }

        thread::sleep(TICK);
    }
}

/// The launcher binary that runs servers (the server zip is Linux only).
const SERVER_LAUNCHER: &str = "linux-x86_64";

/// Installs the manifest's server build if it differs from the running one.
fn check(
    store: &Store,
    manifest_source: &str,
    running: Option<&Running>,
    pending: Option<&Installed>,
    launcher_exe: &Path,
) -> Result<Option<Installed>> {
    let manifest = fetch::manifest(manifest_source)?;
    if running.is_none()
        && self_update::update(
            &manifest,
            manifest_source,
            SERVER_LAUNCHER,
            &store.downloads_dir(),
        )
    {
        // Nothing to interrupt, so restart into the new launcher now.
        return self_update::restart(launcher_exe).map(|()| None);
    }
    let build = manifest.build_for(SERVER)?;
    if let Some(pending) = pending
        && pending.build == manifest.build
    {
        return Ok(Some(pending.clone()));
    }
    let current = running
        .map(|r| &r.installed)
        .cloned()
        .or_else(|| store.current());
    if current.is_some_and(|c| c.build == manifest.build) {
        return Ok(None);
    }
    println!("Downloading server build {}...", manifest.build);
    Ok(Some(store.install(
        &manifest.build,
        build,
        manifest_source,
    )?))
}

fn start(store: &Store, installed: &Installed, server_args: &[OsString]) -> Result<Child> {
    let exe = store.exe_path(installed);
    let mut command = Command::new(&exe);
    command.current_dir(store.build_dir(&installed.build));
    if !server_args.is_empty() {
        command.arg("--").args(server_args);
    }
    command
        .spawn()
        .with_context(|| format!("starting {}", exe.display()))
}

/// Asks the server to quit (so it can tell the master server it is going),
/// then kills it if it has not gone after a few seconds.
fn stop(child: &mut Child) {
    #[cfg(unix)]
    {
        use rustix::process::{Pid, Signal, kill_process};
        let _ = kill_process(Pid::from_child(child), Signal::TERM);
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return;
            }
            thread::sleep(Duration::from_millis(200));
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Human players on the local server, or `None` if it did not answer.
fn players(query_port: u16) -> Option<u8> {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).ok()?;
    socket.set_read_timeout(Some(Duration::from_secs(1))).ok()?;
    let server = SocketAddrV4::new(Ipv4Addr::LOCALHOST, query_port);
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1, |t| t.subsec_nanos() | 1);
    let query = Packet::Query { nonce }.encode();
    let mut buffer = [0u8; 1500];
    for _ in 0..3 {
        socket.send_to(&query, server).ok()?;
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            let Ok((len, _)) = socket.recv_from(&mut buffer) else {
                break;
            };
            if let Ok(Packet::Info { nonce: n, info }) = Packet::decode(&buffer[..len])
                && n == nonce
            {
                return Some(info.players);
            }
        }
    }
    None
}

/// The query port the server will use, from its `--query-port` or `--port`
/// option, as the game reads them.
pub fn query_port(server_args: &[OsString]) -> u16 {
    let value = |name: &str| {
        server_args
            .iter()
            .position(|a| a == name)
            .and_then(|i| server_args.get(i + 1))
            .and_then(|v| v.to_str())
            .and_then(|v| v.parse::<u16>().ok())
    };
    value("--query-port")
        .or_else(|| value("--port").map(discovery::query_port_for))
        .unwrap_or(discovery::DEFAULT_QUERY_PORT)
}

/// Fails early, with a clear message, on a machine that cannot run servers.
pub fn check_platform() -> Result<()> {
    if !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        bail!("dedicated server builds are Linux x86_64 only");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn finds_the_query_port() {
        assert_eq!(query_port(&args(&["--name", "Borseth"])), 27016);
        assert_eq!(
            query_port(&args(&["--port", "27017", "--bots", "3"])),
            27018
        );
        assert_eq!(
            query_port(&args(&["--port", "27017", "--query-port", "28000"])),
            28000
        );
    }

    #[test]
    fn reads_the_player_count_from_a_server() {
        let server = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = server.local_addr().unwrap().port();
        let answer = thread::spawn(move || {
            let mut buffer = [0u8; 1500];
            let (len, from) = server.recv_from(&mut buffer).unwrap();
            let Ok(Packet::Query { nonce }) = Packet::decode(&buffer[..len]) else {
                panic!("expected a query");
            };
            let info = discovery::ServerInfo {
                instance: 1,
                protocol: 10,
                game_port: port - 1,
                name: "Test".into(),
                map: "arena".into(),
                mode: "tdm".into(),
                players: 2,
                max_players: 12,
                bots: 3,
                build: String::new(),
            };
            server
                .send_to(&Packet::Info { nonce, info }.encode(), from)
                .unwrap();
        });
        assert_eq!(players(port), Some(2));
        answer.join().unwrap();
    }

    #[test]
    fn a_silent_server_has_no_player_count() {
        // Bound but never answers.
        let server = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        assert_eq!(players(server.local_addr().unwrap().port()), None);
    }
}
