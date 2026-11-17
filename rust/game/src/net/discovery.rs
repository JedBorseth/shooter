//! Server discovery on the engine side: the dedicated server's query responder
//! and master heartbeats, and `ServerBrowser`, the client's list of servers.
//!
//! Discovery uses plain UDP sockets from the standard library, next to ENet:
//! ENet owns the game port and cannot hand us raw packets. The wire format and
//! the browser's logic are in `shooter_sim::net::discovery`.

use std::hash::{BuildHasher, RandomState};
use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, ToSocketAddrs, UdpSocket};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Instant;

use godot::prelude::*;
use shooter_sim::net::discovery::{
    Browser, DEFAULT_MASTER_PORT, MasterState, Packet, QueryState, ServerInfo, Source,
    query_port_for,
};
use shooter_sim::net::{DEFAULT_PORT, protocol::PROTOCOL_VERSION};

/// Seconds between heartbeats to the master (matches the master's expectation).
const HEARTBEAT_SECONDS: f64 = 30.0;
/// Seconds before a resolved master host name is looked up again.
const RESOLVE_AGAIN_SECONDS: f64 = 600.0;
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
/// seconds, and must not freeze a frame or a server tick.
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

/// A random 64-bit number, different in every process.
fn random_u64() -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    RandomState::new().hash_one((now, std::process::id()))
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

/// Answers queries on the server's query port and keeps the server listed on a
/// master.
pub struct QueryResponder {
    socket: UdpSocket,
    /// What queries are answered with. The server keeps the counts current.
    pub info: ServerInfo,
    master: Option<MasterLink>,
    start: Instant,
}

struct MasterLink {
    host: String,
    port: u16,
    addr: Option<SocketAddrV4>,
    lookup: Option<Receiver<Option<SocketAddrV4>>>,
    looked_up_at: f64,
    token: u64,
    next_heartbeat: f64,
}

impl QueryResponder {
    /// Binds the query port. `master` is `host[:port]`, or empty for none.
    pub fn bind(query_port: u16, info: ServerInfo, master: &str) -> std::io::Result<Self> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, query_port))?;
        socket.set_nonblocking(true)?;
        let master = parse_host_port(master, DEFAULT_MASTER_PORT).map(|(host, port)| MasterLink {
            host,
            port,
            addr: None,
            lookup: None,
            looked_up_at: f64::NEG_INFINITY,
            token: 0,
            next_heartbeat: 0.0,
        });
        Ok(Self {
            socket,
            info,
            master,
            start: Instant::now(),
        })
    }

    pub fn new_instance_id() -> u64 {
        random_u64()
    }

    /// Answers waiting queries and sends a heartbeat when one is due.
    pub fn poll(&mut self) {
        let now = self.start.elapsed().as_secs_f64();
        let mut buffer = [0u8; 512];
        for _ in 0..MAX_PACKETS_PER_POLL {
            let (len, from) = match self.socket.recv_from(&mut buffer) {
                Ok(received) => received,
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                // Windows reports an earlier send's ICMP "port unreachable" here.
                Err(_) => continue,
            };
            let Some(from) = v4(from) else {
                continue;
            };
            match Packet::decode(&buffer[..len]) {
                Ok(Packet::Query { nonce }) => {
                    let reply = Packet::Info {
                        nonce,
                        info: self.info.clone(),
                    };
                    let _ = self.socket.send_to(&reply.encode(), from);
                }
                Ok(Packet::Challenge { token, .. }) => {
                    if let Some(master) = &mut self.master
                        && master.addr == Some(from)
                    {
                        master.token = token;
                        let heartbeat = Packet::Heartbeat { token }.encode();
                        let _ = self.socket.send_to(&heartbeat, from);
                    }
                }
                _ => {}
            }
        }
        self.update_master(now);
    }

    fn update_master(&mut self, now: f64) {
        let Some(master) = &mut self.master else {
            return;
        };
        if let Some(lookup) = &master.lookup {
            match lookup.try_recv() {
                Ok(addr) => {
                    master.lookup = None;
                    if addr.is_none() {
                        godot_warn!("server: cannot find master server {}", master.host);
                    } else if master.addr != addr {
                        godot_print!(
                            "server: listing on master server {}:{}",
                            master.host,
                            master.port
                        );
                        master.token = 0;
                        master.next_heartbeat = now;
                    }
                    master.addr = addr.or(master.addr);
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => master.lookup = None,
            }
        }
        let stale = now - master.looked_up_at
            >= if master.addr.is_some() {
                RESOLVE_AGAIN_SECONDS
            } else {
                HEARTBEAT_SECONDS
            };
        if master.lookup.is_none() && stale {
            master.lookup = Some(resolve(master.host.clone(), master.port));
            master.looked_up_at = now;
        }
        if let Some(addr) = master.addr
            && now >= master.next_heartbeat
        {
            let heartbeat = Packet::Heartbeat {
                token: master.token,
            };
            let _ = self.socket.send_to(&heartbeat.encode(), addr);
            master.next_heartbeat = now + HEARTBEAT_SECONDS;
        }
    }

    /// Tells the master this server is gone, so browsers stop listing it now
    /// rather than when its heartbeats time out.
    pub fn goodbye(&mut self) {
        if let Some(master) = &self.master
            && let Some(addr) = master.addr
            && master.token != 0
        {
            let goodbye = Packet::Goodbye {
                token: master.token,
            };
            let _ = self.socket.send_to(&goodbye.encode(), addr);
        }
    }
}

// --- Client side -----------------------------------------------------------

/// A host name being looked up for the browser.
struct Lookup {
    /// What the player typed (`host[:port]`), for favorites and direct connects.
    label: String,
    kind: LookupKind,
    rx: Receiver<Option<SocketAddrV4>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LookupKind {
    Server(Source),
    Master,
}

/// The client's server list: LAN broadcasts, the master's list, favorites, and
/// direct queries, with each server's name, map, players, and ping.
///
/// Call `refresh_lan`, `refresh_internet`, `add_favorite`, or `query_address`,
/// then read `servers()` whenever `servers_changed` fires.
#[derive(GodotClass)]
#[class(base = Node)]
pub struct ServerBrowser {
    base: Base<Node>,
    socket: Option<UdpSocket>,
    browser: Browser,
    start: Instant,
    lookups: Vec<Lookup>,
    /// What the player typed for each favorite or direct address, by query address.
    labels: Vec<(SocketAddrV4, String)>,
    /// Favorites and direct addresses whose host name did not resolve.
    unresolved: Vec<(String, Source)>,
    master_lookup_failed: bool,
}

#[godot_api]
impl INode for ServerBrowser {
    fn init(base: Base<Node>) -> Self {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).and_then(|socket| {
            socket.set_nonblocking(true)?;
            socket.set_broadcast(true)?;
            Ok(socket)
        });
        let socket = match socket {
            Ok(socket) => Some(socket),
            Err(error) => {
                godot_error!("server browser: cannot open a UDP socket: {error}");
                None
            }
        };
        Self {
            base,
            socket,
            browser: Browser::new(random_u64() as u32),
            start: Instant::now(),
            lookups: Vec::new(),
            labels: Vec::new(),
            unresolved: Vec::new(),
            master_lookup_failed: false,
        }
    }

    fn process(&mut self, _delta: f64) {
        let now = self.now();
        self.receive(now);
        let mut changed = self.finish_lookups(now);
        self.browser.update(now);
        self.send();
        changed |= self.browser.take_changed();
        if changed {
            self.signals().servers_changed().emit();
        }
    }
}

#[godot_api]
impl ServerBrowser {
    /// The list or a server in it changed.
    #[signal]
    fn servers_changed();

    /// Scans the local network again.
    #[func]
    fn refresh_lan(&mut self) {
        let now = self.now();
        self.browser.scan_lan(now);
        self.send();
    }

    /// Asks the master at `master` (`host[:port]`) for its list again.
    /// Returns false if the address is not valid.
    #[func]
    fn refresh_internet(&mut self, master: GString) -> bool {
        self.browser.clear(Source::Internet);
        self.lookups.retain(|l| l.kind != LookupKind::Master);
        self.master_lookup_failed = false;
        let Some((host, port)) = parse_host_port(&master.to_string(), DEFAULT_MASTER_PORT) else {
            return false;
        };
        self.lookups.push(Lookup {
            label: master.to_string(),
            kind: LookupKind::Master,
            rx: resolve(host, port),
        });
        true
    }

    /// Lists a favorite server by its game address (`host[:port]`) and queries
    /// it. Returns false if the address is not valid.
    #[func]
    fn add_favorite(&mut self, address: GString) -> bool {
        self.add_address(&address.to_string(), Source::Favorite)
    }

    /// Stops listing a favorite.
    #[func]
    fn remove_favorite(&mut self, address: GString) {
        let address = address.to_string();
        self.unresolved
            .retain(|(label, source)| !(*source == Source::Favorite && *label == address));
        self.lookups
            .retain(|l| !(l.kind == LookupKind::Server(Source::Favorite) && l.label == address));
        let matching: Vec<SocketAddrV4> = self
            .browser
            .entries()
            .iter()
            .filter(|e| {
                self.label_of(e.query_addr) == Some(address.as_str())
                    || e.game_addr().to_string() == address
            })
            .map(|e| e.query_addr)
            .collect();
        for query_addr in matching {
            self.browser.remove(query_addr, Source::Favorite);
        }
        self.signals().servers_changed().emit();
    }

    /// Queries one server by its game address (`host[:port]`), for a direct
    /// connect. Returns false if the address is not valid.
    #[func]
    fn query_address(&mut self, address: GString) -> bool {
        self.browser.clear(Source::Direct);
        self.unresolved
            .retain(|(_, source)| *source != Source::Direct);
        self.lookups
            .retain(|l| l.kind != LookupKind::Server(Source::Direct));
        self.add_address(&address.to_string(), Source::Direct)
    }

    /// Forgets the servers from `source`: "lan", "internet", "favorite", or "direct".
    #[func]
    fn clear(&mut self, source: GString) {
        let Some(source) = source_from_name(&source.to_string()) else {
            return;
        };
        self.browser.clear(source);
        self.unresolved.retain(|(_, s)| *s != source);
        self.lookups
            .retain(|l| l.kind != LookupKind::Server(source));
        self.signals().servers_changed().emit();
    }

    /// The state of the last master list request: "idle", "resolving",
    /// "unresolved", "requesting", "done", or "failed".
    #[func]
    fn master_status(&self) -> GString {
        if self.lookups.iter().any(|l| l.kind == LookupKind::Master) {
            return "resolving".into();
        }
        if self.master_lookup_failed {
            return "unresolved".into();
        }
        match self.browser.master_state() {
            MasterState::Idle => "idle",
            MasterState::Requesting => "requesting",
            MasterState::Done => "done",
            MasterState::Failed => "failed",
        }
        .into()
    }

    /// Every listed server, as dictionaries:
    ///
    /// - `key`: stable id (the query address, or the typed address if unresolved)
    /// - `address`: `ip:port` to join, `label`: what the player typed, if anything
    /// - `state`: "querying", "online", "offline", or "resolving"
    /// - `name`, `map`, `mode`, `players`, `max_players`, `bots` (once online)
    /// - `ping_ms` (-1 until answered), `compatible` (same game protocol)
    /// - `lan`, `internet`, `favorite`, `direct`: where it was found
    #[func]
    fn servers(&self) -> Array<VarDictionary> {
        let mut out = Array::new();
        for entry in self.browser.entries() {
            let mut d = VarDictionary::new();
            let state = match entry.state {
                QueryState::Querying => "querying",
                QueryState::Online => "online",
                QueryState::Offline => "offline",
            };
            d.set("key", entry.query_addr.to_string());
            d.set("address", entry.game_addr().to_string());
            d.set("label", self.label_of(entry.query_addr).unwrap_or(""));
            d.set("state", state);
            d.set("ping_ms", entry.ping.map_or(-1.0, |p| p * 1000.0));
            if let Some(info) = &entry.info {
                d.set("name", info.name.as_str());
                d.set("map", info.map.as_str());
                d.set("mode", info.mode.as_str());
                d.set("players", i32::from(info.players));
                d.set("max_players", i32::from(info.max_players));
                d.set("bots", i32::from(info.bots));
                d.set("build", info.build.as_str());
                d.set("compatible", info.protocol == PROTOCOL_VERSION);
                // A newer server means this game is out of date: the launcher fixes that.
                d.set("newer", info.protocol > PROTOCOL_VERSION);
            } else {
                d.set("compatible", true);
            }
            d.set("lan", entry.has_source(Source::Lan));
            d.set("internet", entry.has_source(Source::Internet));
            d.set("favorite", entry.has_source(Source::Favorite));
            d.set("direct", entry.has_source(Source::Direct));
            out.push(&d);
        }
        let pending = self.lookups.iter().filter_map(|l| match l.kind {
            LookupKind::Server(source) => Some((l.label.as_str(), source, "resolving")),
            LookupKind::Master => None,
        });
        let unresolved = self
            .unresolved
            .iter()
            .map(|(label, source)| (label.as_str(), *source, "offline"));
        for (label, source, state) in pending.chain(unresolved) {
            let mut d = VarDictionary::new();
            d.set("key", label);
            d.set("address", label);
            d.set("label", label);
            d.set("state", state);
            d.set("ping_ms", -1.0);
            d.set("compatible", true);
            d.set("lan", false);
            d.set("internet", false);
            d.set("favorite", source == Source::Favorite);
            d.set("direct", source == Source::Direct);
            out.push(&d);
        }
        out
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

impl ServerBrowser {
    fn now(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    fn label_of(&self, query_addr: SocketAddrV4) -> Option<&str> {
        self.labels
            .iter()
            .find(|(addr, _)| *addr == query_addr)
            .map(|(_, label)| label.as_str())
    }

    fn add_address(&mut self, address: &str, source: Source) -> bool {
        let Some((host, port)) = parse_host_port(address, DEFAULT_PORT) else {
            return false;
        };
        self.unresolved
            .retain(|(label, s)| !(*s == source && label == address));
        self.lookups.push(Lookup {
            label: address.to_owned(),
            kind: LookupKind::Server(source),
            rx: resolve(host, query_port_for(port)),
        });
        // An IP address resolves at once; show it right away.
        let now = self.now();
        self.finish_lookups(now);
        self.send();
        self.signals().servers_changed().emit();
        true
    }

    fn finish_lookups(&mut self, now: f64) -> bool {
        let mut changed = false;
        let mut index = 0;
        while index < self.lookups.len() {
            let result = match self.lookups[index].rx.try_recv() {
                Ok(addr) => addr,
                Err(TryRecvError::Empty) => {
                    index += 1;
                    continue;
                }
                Err(TryRecvError::Disconnected) => None,
            };
            let lookup = self.lookups.swap_remove(index);
            changed = true;
            match (lookup.kind, result) {
                (LookupKind::Master, Some(addr)) => self.browser.request_master_list(addr, now),
                (LookupKind::Master, None) => self.master_lookup_failed = true,
                (LookupKind::Server(source), Some(addr)) => {
                    self.labels.retain(|(a, _)| *a != addr);
                    self.labels.push((addr, lookup.label));
                    self.browser.add(addr, source, now);
                }
                (LookupKind::Server(source), None) => {
                    self.unresolved.push((lookup.label, source));
                }
            }
        }
        changed
    }

    fn receive(&mut self, now: f64) {
        let Some(socket) = &self.socket else {
            return;
        };
        let mut buffer = [0u8; 1500];
        for _ in 0..256 {
            match socket.recv_from(&mut buffer) {
                Ok((len, from)) => {
                    if let Some(from) = v4(from) {
                        self.browser.receive(from, &buffer[..len], now);
                    }
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                // Windows reports an earlier query's ICMP "port unreachable" here.
                Err(_) => {}
            }
        }
    }

    fn send(&mut self) {
        let Some(socket) = &self.socket else {
            self.browser.drain_outbox().for_each(drop);
            return;
        };
        for (to, bytes) in self.browser.drain_outbox() {
            // Broadcasts fail on machines without a network; that is fine.
            let _ = socket.send_to(&bytes, to);
        }
    }
}

fn source_from_name(name: &str) -> Option<Source> {
    match name {
        "lan" => Some(Source::Lan),
        "internet" => Some(Source::Internet),
        "favorite" => Some(Source::Favorite),
        "direct" => Some(Source::Direct),
        _ => None,
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
