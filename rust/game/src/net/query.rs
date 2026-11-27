//! Server queries on the engine side: the dedicated server's query responder,
//! and `ServerQuery`, which asks one server who it is before the game joins.
//!
//! Queries use plain UDP sockets from the standard library, next to ENet:
//! ENet owns the game port and cannot hand us raw packets. The wire format and
//! the client's logic are in `shooter_sim::net::query`.

use std::hash::{BuildHasher, RandomState};
use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, ToSocketAddrs, UdpSocket};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Instant;

use godot::prelude::*;
use shooter_sim::net::query::{Packet, Query, QueryState, ServerInfo, query_port_for};
use shooter_sim::net::{DEFAULT_PORT, protocol::PROTOCOL_VERSION};

/// Packets handled per poll, so a flood cannot stall a tick.
const MAX_PACKETS_PER_POLL: usize = 64;

/// Splits `host`, `host:port`, or `[host]:port` into a host and port.
pub fn parse_host_port(text: &str, default_port: u16) -> Option<(String, u16)> {
    let text = text.trim();
    if text.is_empty() || text.contains(char::is_whitespace) {
        return None;
    }
    let (host, port) = match text.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => (host, port.parse().ok()?),
        _ => (text, default_port),
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    (!host.is_empty() && port != 0).then(|| (host.to_owned(), port))
}

/// Looks up `host` on a worker thread: name lookups block, sometimes for
/// seconds, and must not freeze a frame.
fn resolve(host: String, port: u16) -> Receiver<Option<SocketAddrV4>> {
    let (tx, rx) = mpsc::channel();
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        let _ = tx.send(Some(SocketAddrV4::new(ip, port)));
        return rx;
    }
    std::thread::spawn(move || {
        let addr = (host.as_str(), port)
            .to_socket_addrs()
            .ok()
            .and_then(|mut addrs| {
                addrs.find_map(|addr| match addr {
                    SocketAddr::V4(v4) => Some(v4),
                    SocketAddr::V6(_) => None,
                })
            });
        let _ = tx.send(addr);
    });
    rx
}

/// A random 32-bit number, different in every process and call.
fn random_u32() -> u32 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    RandomState::new().hash_one((now, std::process::id())) as u32
}

fn v4(addr: SocketAddr) -> Option<SocketAddrV4> {
    match addr {
        SocketAddr::V4(v4) => Some(v4),
        SocketAddr::V6(v6) => v6
            .ip()
            .to_ipv4_mapped()
            .map(|ip| SocketAddrV4::new(ip, v6.port())),
    }
}

// --- Server side -----------------------------------------------------------

/// Answers queries on the server's query port.
pub struct QueryResponder {
    socket: UdpSocket,
    /// What queries are answered with. The server keeps the count current.
    pub info: ServerInfo,
}

impl QueryResponder {
    /// Binds the query port.
    pub fn bind(query_port: u16, info: ServerInfo) -> std::io::Result<Self> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, query_port))?;
        socket.set_nonblocking(true)?;
        Ok(Self { socket, info })
    }

    /// Answers waiting queries.
    pub fn poll(&mut self) {
        let mut buffer = [0u8; 512];
        for _ in 0..MAX_PACKETS_PER_POLL {
            let (len, from) = match self.socket.recv_from(&mut buffer) {
                Ok(received) => received,
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                // Windows reports an earlier send's ICMP "port unreachable" here.
                Err(_) => continue,
            };
            if let Ok(Packet::Query { nonce }) = Packet::decode(&buffer[..len]) {
                let reply = Packet::Info {
                    nonce,
                    info: self.info.clone(),
                };
                let _ = self.socket.send_to(&reply.encode(), from);
            }
        }
    }
}

// --- Client side -----------------------------------------------------------

/// Where the current question stands.
enum Asking {
    Idle,
    /// The typed host name is being looked up.
    Resolving(Receiver<Option<SocketAddrV4>>),
    /// Waiting for the server at this query address to answer.
    Querying(SocketAddrV4, Query),
}

/// Asks one server, by the address a player typed (`host[:port]`, the game
/// port), who it is: its name, map, players, and version. Call `ask`, then
/// wait for `finished`.
#[derive(GodotClass)]
#[class(base = Node)]
pub struct ServerQuery {
    base: Base<Node>,
    socket: Option<UdpSocket>,
    start: Instant,
    /// What the player typed, for the result.
    address: String,
    asking: Asking,
}

#[godot_api]
impl INode for ServerQuery {
    fn init(base: Base<Node>) -> Self {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).and_then(|socket| {
            socket.set_nonblocking(true)?;
            Ok(socket)
        });
        let socket = match socket {
            Ok(socket) => Some(socket),
            Err(error) => {
                godot_error!("server query: cannot open a UDP socket: {error}");
                None
            }
        };
        Self {
            base,
            socket,
            start: Instant::now(),
            address: String::new(),
            asking: Asking::Idle,
        }
    }

    fn process(&mut self, _delta: f64) {
        let now = self.start.elapsed().as_secs_f64();
        if let Asking::Resolving(rx) = &self.asking {
            match rx.try_recv() {
                Ok(Some(addr)) => self.asking = Asking::Querying(addr, Query::new(random_u32())),
                Ok(None) | Err(TryRecvError::Disconnected) => {
                    self.asking = Asking::Idle;
                    return self.finish("unresolved", None);
                }
                Err(TryRecvError::Empty) => return,
            }
        }
        let Asking::Querying(addr, query) = &mut self.asking else {
            return;
        };
        let addr = *addr;
        if let Some(socket) = &self.socket {
            let mut buffer = [0u8; 1500];
            for _ in 0..MAX_PACKETS_PER_POLL {
                match socket.recv_from(&mut buffer) {
                    Ok((len, from)) => {
                        if v4(from) == Some(addr) {
                            query.receive(&buffer[..len], now);
                        }
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                    // Windows reports an earlier query's ICMP "port unreachable" here.
                    Err(_) => {}
                }
            }
            if let Some(bytes) = query.poll(now) {
                let _ = socket.send_to(&bytes, addr);
            }
        } else {
            query.state = QueryState::Silent;
        }
        match query.state {
            QueryState::Asking => {}
            QueryState::Answered => {
                let query = query.clone();
                self.asking = Asking::Idle;
                self.finish("answered", Some(&query));
            }
            QueryState::Silent => {
                self.asking = Asking::Idle;
                self.finish("silent", None);
            }
        }
    }
}

#[godot_api]
impl ServerQuery {
    /// The server answered, or did not. `server` has `address` (as typed)
    /// and `state`: "answered", "silent" (no answer on the query port), or
    /// "unresolved" (the host name was not found). Answered, it also has
    /// `name`, `map`, `players`, `max_players`, `build`, `ping_ms`,
    /// `compatible` (same game protocol), and `newer` (the server's is
    /// newer than ours).
    #[signal]
    fn finished(server: VarDictionary);

    /// Asks the server at `address` (`host[:port]`, its game port), dropping
    /// any question still open. Returns false if the address is not valid.
    #[func]
    fn ask(&mut self, address: GString) -> bool {
        let address = address.to_string();
        let Some((host, port)) = parse_host_port(&address, DEFAULT_PORT) else {
            return false;
        };
        self.address = address;
        self.asking = Asking::Resolving(resolve(host, query_port_for(port)));
        true
    }

    /// Drops the question still open, if any.
    #[func]
    fn cancel(&mut self) {
        self.asking = Asking::Idle;
    }

    /// The game protocol version this build speaks.
    #[func]
    fn protocol_version() -> i32 {
        i32::from(PROTOCOL_VERSION)
    }

    /// The game port a typed address points at, or -1 if it is not valid.
    #[func]
    fn port_of(address: GString) -> i32 {
        parse_host_port(&address.to_string(), DEFAULT_PORT).map_or(-1, |(_, port)| i32::from(port))
    }
}

impl ServerQuery {
    fn finish(&mut self, state: &str, query: Option<&Query>) {
        let mut d = VarDictionary::new();
        d.set("address", self.address.as_str());
        d.set("state", state);
        if let Some(query) = query
            && let Some(info) = &query.info
        {
            d.set("name", info.name.as_str());
            d.set("map", info.map.as_str());
            d.set("players", i32::from(info.players));
            d.set("max_players", i32::from(info.max_players));
            d.set("build", info.build.as_str());
            d.set("ping_ms", query.ping.map_or(-1.0, |p| p * 1000.0));
            d.set("compatible", info.protocol == PROTOCOL_VERSION);
            // A newer server means this game is out of date: the launcher fixes that.
            d.set("newer", info.protocol > PROTOCOL_VERSION);
        }
        self.signals().finished().emit(&d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_parse_with_and_without_ports() {
        let parse = |s| parse_host_port(s, 27015);
        assert_eq!(parse("1.2.3.4"), Some(("1.2.3.4".into(), 27015)));
        assert_eq!(
            parse(" host.example:27999 "),
            Some(("host.example".into(), 27999))
        );
        assert_eq!(parse("[host]:5"), Some(("host".into(), 5)));
        assert_eq!(parse(""), None);
        assert_eq!(parse("host:notaport"), None);
        assert_eq!(parse("host:0"), None);
        assert_eq!(parse("two words"), None);
        assert_eq!(parse(":27015"), None);
    }
}
