//! `shooter-master [--port N] [--bind IP] [--public-ip IP]`: runs the master server that lists
//! dedicated servers for the in-game server browser. See the crate docs.

use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use shooter_master::{Change, DEFAULT_MASTER_PORT, Master};
use shooter_sim::net::discovery::Packet;

const USAGE: &str = "usage: shooter-master [--port N] [--bind IP] [--public-ip IP]
  --port N   UDP port to listen on (default 27010)
  --bind IP  IPv4 address to listen on (default 0.0.0.0, every interface)
  --public-ip IP  advertise this IPv4 address for servers registered from localhost";

fn main() -> ExitCode {
    let mut port = DEFAULT_MASTER_PORT;
    let mut bind = Ipv4Addr::UNSPECIFIED;
    let mut public_ip = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = match arg.as_str() {
            "--port" | "--bind" | "--public-ip" => args.next(),
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            _ => None,
        };
        let parsed = match (arg.as_str(), value) {
            ("--port", Some(v)) => v.parse().map(|p| port = p).is_ok(),
            ("--bind", Some(v)) => v.parse().map(|ip| bind = ip).is_ok(),
            ("--public-ip", Some(v)) => v.parse().map(|ip| public_ip = Some(ip)).is_ok(),
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
                let _ = socket.send_to(&advertise_local(reply, public_ip), from);
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

/// A co-hosted game server registers over loopback; clients need its public IP.
/// Keep the observed query port and remote registrations unchanged.
fn advertise_local(reply: Vec<u8>, public_ip: Option<Ipv4Addr>) -> Vec<u8> {
    let Some(public_ip) = public_ip else {
        return reply;
    };
    if let Ok(Packet::List {
        nonce,
        start,
        total,
        mut servers,
    }) = Packet::decode(&reply)
    {
        for server in &mut servers {
            if server.ip().is_loopback() {
                server.set_ip(public_ip);
            }
        }
        Packet::List {
            nonce,
            start,
            total,
            servers,
        }
        .encode()
    } else {
        reply
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_listing_rewrites_only_loopback_and_keeps_query_ports() {
        let local = "127.0.0.1:27016".parse().unwrap();
        let remote = "192.0.2.10:27022".parse().unwrap();
        let packet = Packet::List {
            nonce: 42,
            start: 0,
            total: 2,
            servers: vec![local, remote],
        };
        let reply = advertise_local(packet.encode(), Some(Ipv4Addr::new(203, 0, 113, 5)));
        assert_eq!(
            Packet::decode(&reply).unwrap(),
            Packet::List {
                nonce: 42,
                start: 0,
                total: 2,
                servers: vec!["203.0.113.5:27016".parse().unwrap(), remote],
            }
        );
        assert_eq!(advertise_local(packet.encode(), None), packet.encode());
    }

    #[test]
    fn public_address_does_not_change_handshake_tokens() {
        let packet = Packet::Challenge {
            nonce: 42,
            token: 12345,
        }
        .encode();
        assert_eq!(
            advertise_local(packet.clone(), Some(Ipv4Addr::LOCALHOST)),
            packet
        );
    }
}
