//! `shooter-query [HOST:]PORT [--timeout SECONDS]`: asks one dedicated server's
//! query port who it is, prints its answer as one JSON line, and exits 0; exits
//! 1 when it does not answer in time.
//!
//! The Docker image's health check runs it against the server in the same
//! container, and scripts use it to wait for a server to come up. It speaks the
//! same discovery packets as the in-game server browser (`net::discovery`).

use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use shooter_sim::net::discovery::{Packet, ServerInfo};

const USAGE: &str = "usage: shooter-query [HOST:]PORT [--timeout SECONDS]
  [HOST:]PORT     the server's query port (its game port + 1); HOST defaults to 127.0.0.1
  --timeout S     give up after S seconds (default 2)";

fn main() -> ExitCode {
    let mut target = None;
    let mut timeout = Duration::from_secs(2);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--timeout" => match args.next().and_then(|v| v.parse::<f64>().ok()) {
                Some(seconds) if seconds > 0.0 && seconds.is_finite() => {
                    timeout = Duration::from_secs_f64(seconds);
                }
                _ => return usage(),
            },
            _ if target.is_none() => target = Some(arg),
            _ => return usage(),
        }
    }
    let Some(target) = target else {
        return usage();
    };
    let target = if target.contains(':') {
        target
    } else {
        format!("127.0.0.1:{target}")
    };
    let Some(addr) = target
        .to_socket_addrs()
        .ok()
        .and_then(|mut addrs| addrs.find(SocketAddr::is_ipv4))
    else {
        eprintln!("shooter-query: cannot resolve {target}");
        return ExitCode::FAILURE;
    };
    match query(addr, timeout) {
        Ok(info) => {
            println!("{}", to_json(&info));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("shooter-query: {addr}: {error}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::FAILURE
}

/// Sends a query every 250 ms until the server answers or `timeout` passes.
fn query(addr: SocketAddr, timeout: Duration) -> std::io::Result<ServerInfo> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    let nonce = std::process::id() ^ 0x5348_5451;
    let request = Packet::Query { nonce }.encode();
    let deadline = Instant::now() + timeout;
    let mut buffer = [0u8; 1500];
    loop {
        // A refused send (nothing listening yet) is retried like a lost one.
        let _ = socket.send_to(&request, addr);
        let resend = (Instant::now() + Duration::from_millis(250)).min(deadline);
        loop {
            let now = Instant::now();
            if now >= resend {
                break;
            }
            socket.set_read_timeout(Some(resend - now))?;
            let Ok((len, from)) = socket.recv_from(&mut buffer) else {
                continue;
            };
            if from != addr {
                continue;
            }
            if let Ok(Packet::Info { nonce: n, info }) = Packet::decode(&buffer[..len])
                && n == nonce
            {
                return Ok(info);
            }
        }
        if Instant::now() >= deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "no answer",
            ));
        }
    }
}

fn to_json(info: &ServerInfo) -> String {
    format!(
        "{{\"name\":{},\"map\":{},\"mode\":{},\"players\":{},\"max_players\":{},\"bots\":{},\"protocol\":{},\"game_port\":{},\"build\":{}}}",
        json_string(&info.name),
        json_string(&info.map),
        json_string(&info.mode),
        info.players,
        info.max_players,
        info.bots,
        info.protocol,
        info.game_port,
        json_string(&info.build),
    )
}

fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_from_a_server_on_loopback() {
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let mut buffer = [0u8; 1500];
            let (len, from) = server.recv_from(&mut buffer).unwrap();
            let Ok(Packet::Query { nonce }) = Packet::decode(&buffer[..len]) else {
                panic!("expected a query");
            };
            let info = ServerInfo {
                instance: 1,
                protocol: 7,
                game_port: addr.port() - 1,
                name: "Box \"one\"".into(),
                map: "Slamcity".into(),
                mode: "tdm".into(),
                players: 2,
                max_players: 12,
                bots: 4,
                build: "abc1234".into(),
            };
            server
                .send_to(&Packet::Info { nonce, info }.encode(), from)
                .unwrap();
        });
        let info = query(addr, Duration::from_secs(2)).unwrap();
        handle.join().unwrap();
        assert_eq!(info.map, "Slamcity");
        assert!(to_json(&info).contains("\"name\":\"Box \\\"one\\\"\""));
    }

    #[test]
    fn times_out_when_nobody_answers() {
        let silent = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = silent.local_addr().unwrap();
        assert!(query(addr, Duration::from_millis(300)).is_err());
    }
}
