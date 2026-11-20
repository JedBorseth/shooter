//! `shooter-allocator`: the HTTP service Nakama's match handler calls to get
//! a dedicated server for a match. See the crate docs and docs/online.md.
//!
//! Settings come from the environment (Docker Compose sets them):
//!
//! | Variable | Default |
//! | --- | --- |
//! | `ALLOCATOR_LISTEN` | `0.0.0.0:7400` |
//! | `ALLOCATOR_KEY` | required: callers send `Authorization: Bearer <key>` |
//! | `ALLOCATOR_IMAGE` | `ghcr.io/jedborseth/shooter-server:nightly` |
//! | `ALLOCATOR_PUBLIC_HOST` | `127.0.0.1`: the address players connect to |
//! | `ALLOCATOR_PORTS` | `27100-27198` |
//! | `ALLOCATOR_MAX_SERVERS` | `8` |
//! | `ALLOCATOR_NETWORK` | none: the Docker network servers join |
//! | `ALLOCATOR_ACCOUNTS_URL`, `ALLOCATOR_ACCOUNTS_KEY` | none: make servers ranked |
//! | `ALLOCATOR_EXIT_WHEN_EMPTY` | `120` seconds |
//! | `DOCKER_SOCKET` | `/var/run/docker.sock` |

#![cfg_attr(not(unix), allow(dead_code, unused_imports))]

use std::env;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::json;
#[cfg(unix)]
use shooter_allocator::docker::Docker;
use shooter_allocator::{
    AllocateError, AllocateRequest, Allocation, Config, container_spec, http, pick_port, used_ports,
};

#[cfg(not(unix))]
fn main() -> ExitCode {
    eprintln!("allocator: runs on Linux, next to Docker");
    ExitCode::FAILURE
}

#[cfg(unix)]
struct Service {
    config: Config,
    key: String,
    docker: Docker,
    /// Held while a port is picked and its container created, so two
    /// allocations never take the same port.
    claim: Mutex<()>,
}

#[cfg(unix)]
fn main() -> ExitCode {
    let var = |name: &str, default: &str| env::var(name).unwrap_or_else(|_| default.to_owned());
    let mut config = Config::default();
    config.image = var("ALLOCATOR_IMAGE", &config.image);
    config.public_host = var("ALLOCATOR_PUBLIC_HOST", &config.public_host);
    config.network = var("ALLOCATOR_NETWORK", "");
    config.accounts_url = var("ALLOCATOR_ACCOUNTS_URL", "");
    config.accounts_key = var("ALLOCATOR_ACCOUNTS_KEY", "");
    let ports = var("ALLOCATOR_PORTS", "27100-27198");
    let parsed = ports
        .split_once('-')
        .and_then(|(a, b)| Some((a.trim().parse().ok()?, b.trim().parse().ok()?)));
    let Some((first, last)) = parsed else {
        eprintln!("allocator: ALLOCATOR_PORTS must look like 27100-27198");
        return ExitCode::FAILURE;
    };
    (config.first_port, config.last_port) = (first, last);
    config.max_servers = var("ALLOCATOR_MAX_SERVERS", "8").parse().unwrap_or(8);
    config.exit_when_empty = var("ALLOCATOR_EXIT_WHEN_EMPTY", "120")
        .parse()
        .unwrap_or(120);
    let key = var("ALLOCATOR_KEY", "");
    if key.is_empty() {
        eprintln!("allocator: set ALLOCATOR_KEY");
        return ExitCode::FAILURE;
    }
    let listen = var("ALLOCATOR_LISTEN", "0.0.0.0:7400");
    let listener = match TcpListener::bind(&listen) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("allocator: cannot listen on {listen}: {error}");
            return ExitCode::FAILURE;
        }
    };
    let service = Arc::new(Service {
        docker: Docker {
            socket: PathBuf::from(var("DOCKER_SOCKET", "/var/run/docker.sock")),
        },
        config,
        key,
        claim: Mutex::new(()),
    });
    // The first match should not wait for a download.
    match service.docker.has_image(&service.config.image) {
        Ok(true) => {}
        Ok(false) => {
            println!("allocator: pulling {}", service.config.image);
            if let Err(error) = service.docker.pull(&service.config.image) {
                eprintln!("allocator: {}", error.message());
            }
        }
        Err(error) => eprintln!("allocator: cannot reach Docker: {}", error.message()),
    }
    println!(
        "allocator: listening on {listen}; {} on ports {}-{} for {}",
        service.config.image,
        service.config.first_port,
        service.config.last_port,
        service.config.public_host
    );
    for stream in listener.incoming().flatten() {
        let service = Arc::clone(&service);
        thread::spawn(move || {
            if let Err(error) = serve(&service, stream) {
                eprintln!("allocator: {error}");
            }
        });
    }
    ExitCode::SUCCESS
}

#[cfg(unix)]
fn serve(service: &Service, stream: TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let request = http::read_message(&stream, false)?;
    let (status, body) = match request.method_path() {
        ("GET", "/health") => (200, json!({"ok": true})),
        ("POST", "/allocate") => {
            let authorized = request
                .header("Authorization")
                .and_then(|v| v.strip_prefix("Bearer "))
                .is_some_and(|k| constant_time_eq(k.as_bytes(), service.key.as_bytes()));
            if !authorized {
                (401, json!({"error": "bad key"}))
            } else {
                match serde_json::from_slice::<AllocateRequest>(&request.body) {
                    Err(error) => (400, json!({"error": error.to_string()})),
                    Ok(wanted) => match allocate(service, &wanted) {
                        Ok(allocation) => (200, json!(allocation)),
                        Err(error) => {
                            eprintln!("allocator: {}", error.message());
                            (error.status(), json!({"error": error.message()}))
                        }
                    },
                }
            }
        }
        _ => (404, json!({"error": "not found"})),
    };
    let start = match status {
        200 => "200 OK",
        400 => "400 Bad Request",
        401 => "401 Unauthorized",
        404 => "404 Not Found",
        503 => "503 Service Unavailable",
        _ => "502 Bad Gateway",
    };
    http::write_message(&stream, start, body.to_string().as_bytes())
}

#[cfg(unix)]
fn allocate(service: &Service, wanted: &AllocateRequest) -> Result<Allocation, AllocateError> {
    wanted.validate()?;
    let config = &service.config;
    let (id, port) = {
        let _claim = service.claim.lock().unwrap_or_else(|e| e.into_inner());
        let used = used_ports(&service.docker.list()?);
        let port = pick_port(config, &used).ok_or(AllocateError::Full)?;
        let spec = container_spec(config, wanted, port);
        let id = service
            .docker
            .run(&format!("shooter-match-{port}"), &spec)?;
        (id, port)
    };
    println!(
        "allocator: {} on {} for match {} at {}:{port}",
        wanted.mode, wanted.map, wanted.match_id, config.public_host
    );
    service.docker.wait_healthy(&id, Duration::from_secs(90))?;
    Ok(Allocation {
        host: config.public_host.clone(),
        port,
        container: id.chars().take(12).collect(),
    })
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
