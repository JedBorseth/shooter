//! `shooter-master [--port N] [--bind IP]`: runs the master server that lists
//! dedicated servers for the in-game server browser. See the crate docs.

use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use shooter_master::{Change, DEFAULT_MASTER_PORT, Master};

const USAGE: &str = "usage: shooter-master [--port N] [--bind IP]
  --port N   UDP port to listen on (default 27010)
  --bind IP  IPv4 address to listen on (default 0.0.0.0, every interface)";

fn main() -> ExitCode {
    let mut port = DEFAULT_MASTER_PORT;
    let mut bind = Ipv4Addr::UNSPECIFIED;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = match arg.as_str() {
            "--port" | "--bind" => args.next(),
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            _ => None,
        };
        let parsed = match (arg.as_str(), value) {
            ("--port", Some(v)) => v.parse().map(|p| port = p).is_ok(),
            ("--bind", Some(v)) => v.parse().map(|ip| bind = ip).is_ok(),
            _ => false,
        };
        if !parsed {
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
        }
    }

    let socket = match UdpSocket::bind((bind, port)) {
        Ok(socket) => socket,
        Err(error) => {
            eprintln!("master: cannot listen on UDP {bind}:{port}: {error}");
            return ExitCode::FAILURE;
        }
    };
    // Wake at least once a second to expire silent servers.
    if let Err(error) = socket.set_read_timeout(Some(Duration::from_secs(1))) {
        eprintln!("master: {error}");
        return ExitCode::FAILURE;
    }
    println!("master: listening on UDP {bind}:{port}");

    let start = Instant::now();
    let mut master = Master::new();
    let mut buffer = [0u8; 1500];
    loop {
        let received = socket.recv_from(&mut buffer);
        let now = start.elapsed().as_secs_f64();
        if let Ok((len, from)) = received {
            let (reply, change) = master.handle(from, &buffer[..len], now);
            if let Some(reply) = reply {
                // A failed send to one address must not stop the master.
                let _ = socket.send_to(&reply, from);
            }
            if let Some(change) = change {
                log(change, &master);
            }
        }
        for change in master.expire(now) {
            log(change, &master);
        }
    }
}

fn log(change: Change, master: &Master) {
    let count = master.servers().count();
    match change {
        Change::Added(addr) => {
            println!("master: listed {} ({count} servers)", SocketAddr::V4(addr))
        }
        Change::Removed(addr) => {
            println!("master: dropped {} ({count} servers)", SocketAddr::V4(addr));
        }
    }
}
