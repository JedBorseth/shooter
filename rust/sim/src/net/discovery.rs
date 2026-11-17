//! Server discovery: how a client finds servers before it joins one.
//!
//! Every dedicated server answers queries on a plain UDP *query port*, next to
//! its ENet game port ([`query_port_for`]). A client finds servers three ways:
//!
//! - **LAN:** it broadcasts a [`Packet::Query`] to the query ports of the
//!   default game ports ([`LAN_QUERY_PORTS`]); every server on the network
//!   answers with a [`Packet::Info`].
//! - **Direct:** it sends the same query to one address (a favorite, or one
//!   typed into the browser).
//! - **Internet:** a self-hosted master server (the `shooter-master` binary)
//!   keeps the list of servers that send it heartbeats. The client asks the
//!   master for that list ([`Packet::ListRequest`]), then queries each server
//!   itself, which also measures the ping.
//!
//! Servers send heartbeats from their query socket, so the master records the
//! address clients must query, even behind NAT.
//!
//! Abuse: every request is padded to [`REQUEST_SIZE`], and the only replies
//! sent without a handshake ([`Packet::Info`], [`Packet::Challenge`]) are
//! smaller than that, so a spoofed request cannot be amplified. The master
//! answers list requests and accepts heartbeats only with a token it handed to
//! that address ([`Packet::Challenge`]), which proves the sender can receive
//! packets there.
//!
//! [`Browser`] is the client side, without sockets: feed it received packets
//! and the time, and send what it puts in its outbox.

use std::net::{Ipv4Addr, SocketAddrV4};
use std::ops::RangeInclusive;

use super::DEFAULT_PORT;
use super::bits::{DecodeError, DecodeResult};

/// Bumped whenever this format changes. Packets of other versions are ignored.
pub const DISCOVERY_VERSION: u8 = 1;
/// Every discovery packet starts with this.
const MAGIC: [u8; 4] = *b"SHTQ";
/// Requests are padded to this many bytes; shorter ones are ignored.
pub const REQUEST_SIZE: usize = 160;
/// UDP port the master server listens on by default.
pub const DEFAULT_MASTER_PORT: u16 = 27010;
/// The query port of the default game port.
pub const DEFAULT_QUERY_PORT: u16 = DEFAULT_PORT + 1;
/// Query ports a LAN scan broadcasts to: game ports 27015, 27017, … 27029
/// (servers on one machine step their game port by 2).
pub const LAN_QUERY_PORTS: RangeInclusive<u16> = DEFAULT_QUERY_PORT..=DEFAULT_QUERY_PORT + 14;
/// Longest server name, map name, and mode name, in bytes.
pub const MAX_NAME_BYTES: usize = 32;
pub const MAX_MAP_BYTES: usize = 24;
pub const MAX_MODE_BYTES: usize = 16;
/// Longest build name (a tag or short commit hash), in bytes.
pub const MAX_BUILD_BYTES: usize = 32;
/// Servers in one [`Packet::List`] (6 bytes each).
pub const MAX_LIST_ENTRIES: usize = 150;

/// A server's query port: the port after its game port.
pub fn query_port_for(game_port: u16) -> u16 {
    game_port.wrapping_add(1)
}

/// What a server says about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerInfo {
    /// Random per server process, so one server reached at two addresses
    /// (localhost and its LAN address) is listed once.
    pub instance: u64,
    /// The game protocol (`protocol::PROTOCOL_VERSION`) the server speaks.
    pub protocol: u16,
    /// ENet port to join on.
    pub game_port: u16,
    pub name: String,
    pub map: String,
    pub mode: String,
    /// Human players connected.
    pub players: u8,
    pub max_players: u8,
    pub bots: u8,
    /// Which build the server runs (a tag or short commit hash). Sent last,
    /// so older clients, which ignore trailing bytes, still read the rest;
    /// empty from servers older than the field.
    pub build: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Packet {
    /// Client → server query port: who are you?
    Query { nonce: u32 },
    /// Server → client: the answer to the query with `nonce`.
    Info { nonce: u32, info: ServerInfo },
    /// Server → master, from the query socket: list me. `token` is the last
    /// one the master sent this address, or 0 to ask for one.
    Heartbeat { token: u64 },
    /// Server → master: I am shutting down.
    Goodbye { token: u64 },
    /// Client → master: send the servers from index `start` on.
    ListRequest { nonce: u32, token: u64, start: u16 },
    /// Master → sender of a heartbeat or list request without a valid token:
    /// repeat it with this token. `nonce` echoes the list request's (0 for a
    /// heartbeat).
    Challenge { nonce: u32, token: u64 },
    /// Master → client: query addresses of servers `start..start + len` of
    /// `total`.
    List {
        nonce: u32,
        start: u16,
        total: u16,
        servers: Vec<SocketAddrV4>,
    },
}

const KIND_QUERY: u8 = 1;
const KIND_INFO: u8 = 2;
const KIND_HEARTBEAT: u8 = 3;
const KIND_GOODBYE: u8 = 4;
const KIND_LIST_REQUEST: u8 = 5;
const KIND_CHALLENGE: u8 = 6;
const KIND_LIST: u8 = 7;

impl Packet {
    /// Whether this is a request, padded to [`REQUEST_SIZE`].
    pub fn is_request(&self) -> bool {
        matches!(
            self,
            Packet::Query { .. }
                | Packet::Heartbeat { .. }
                | Packet::Goodbye { .. }
                | Packet::ListRequest { .. }
        )
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut w = ByteWriter::default();
        w.bytes(&MAGIC);
        w.u8(DISCOVERY_VERSION);
        match self {
            Packet::Query { nonce } => {
                w.u8(KIND_QUERY);
                w.u32(*nonce);
            }
            Packet::Info { nonce, info } => {
                w.u8(KIND_INFO);
                w.u32(*nonce);
                w.u64(info.instance);
                w.u16(info.protocol);
                w.u16(info.game_port);
                w.text(&info.name, MAX_NAME_BYTES);
                w.text(&info.map, MAX_MAP_BYTES);
                w.text(&info.mode, MAX_MODE_BYTES);
                w.u8(info.players);
                w.u8(info.max_players);
                w.u8(info.bots);
                w.text(&info.build, MAX_BUILD_BYTES);
            }
            Packet::Heartbeat { token } => {
                w.u8(KIND_HEARTBEAT);
                w.u64(*token);
            }
            Packet::Goodbye { token } => {
                w.u8(KIND_GOODBYE);
                w.u64(*token);
            }
            Packet::ListRequest {
                nonce,
                token,
                start,
            } => {
                w.u8(KIND_LIST_REQUEST);
                w.u32(*nonce);
                w.u64(*token);
                w.u16(*start);
            }
            Packet::Challenge { nonce, token } => {
                w.u8(KIND_CHALLENGE);
                w.u32(*nonce);
                w.u64(*token);
            }
            Packet::List {
                nonce,
                start,
                total,
                servers,
            } => {
                w.u8(KIND_LIST);
                w.u32(*nonce);
                w.u16(*start);
                w.u16(*total);
                let servers = &servers[..servers.len().min(MAX_LIST_ENTRIES)];
                w.u8(servers.len() as u8);
                for server in servers {
                    w.bytes(&server.ip().octets());
                    w.u16(server.port());
                }
            }
        }
        let mut bytes = w.0;
        if self.is_request() {
            bytes.resize(REQUEST_SIZE, 0);
        }
        bytes
    }

    pub fn decode(bytes: &[u8]) -> DecodeResult<Packet> {
        let mut r = ByteReader(bytes);
        if r.take(4)? != MAGIC {
            return Err(DecodeError::Invalid("discovery magic"));
        }
        if r.u8()? != DISCOVERY_VERSION {
            return Err(DecodeError::Invalid("discovery version"));
        }
        let kind = r.u8()?;
        let packet = match kind {
            KIND_QUERY => Packet::Query { nonce: r.u32()? },
            KIND_INFO => Packet::Info {
                nonce: r.u32()?,
                info: ServerInfo {
                    instance: r.u64()?,
                    protocol: r.u16()?,
                    game_port: r.u16()?,
                    name: r.text(MAX_NAME_BYTES)?,
                    map: r.text(MAX_MAP_BYTES)?,
                    mode: r.text(MAX_MODE_BYTES)?,
                    players: r.u8()?,
                    max_players: r.u8()?,
                    bots: r.u8()?,
                    build: if r.0.is_empty() {
                        String::new()
                    } else {
                        r.text(MAX_BUILD_BYTES)?
                    },
                },
            },
            KIND_HEARTBEAT => Packet::Heartbeat { token: r.u64()? },
            KIND_GOODBYE => Packet::Goodbye { token: r.u64()? },
            KIND_LIST_REQUEST => Packet::ListRequest {
                nonce: r.u32()?,
                token: r.u64()?,
                start: r.u16()?,
            },
            KIND_CHALLENGE => Packet::Challenge {
                nonce: r.u32()?,
                token: r.u64()?,
            },
            KIND_LIST => {
                let nonce = r.u32()?;
                let start = r.u16()?;
                let total = r.u16()?;
                let count = usize::from(r.u8()?);
                if count > MAX_LIST_ENTRIES {
                    return Err(DecodeError::Invalid("server list length"));
                }
                let mut servers = Vec::with_capacity(count);
                for _ in 0..count {
                    let ip = r.take(4)?;
                    let ip = Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3]);
                    servers.push(SocketAddrV4::new(ip, r.u16()?));
                }
                Packet::List {
                    nonce,
                    start,
                    total,
                    servers,
                }
            }
            _ => return Err(DecodeError::Invalid("discovery packet kind")),
        };
        if packet.is_request() && bytes.len() < REQUEST_SIZE {
            return Err(DecodeError::Invalid("unpadded request"));
        }
        Ok(packet)
    }
}

/// Keeps printable characters only, trimmed, and at most `max_bytes` long
/// (cut on a character boundary). Names come from strangers' servers.
pub fn clean_text(text: &str, max_bytes: usize) -> String {
    let mut out = String::new();
    for c in text.trim().chars() {
        let c = if c.is_control() { ' ' } else { c };
        if out.len() + c.len_utf8() > max_bytes {
            break;
        }
        out.push(c);
    }
    out.trim_end().to_owned()
}

#[derive(Default)]
struct ByteWriter(Vec<u8>);

impl ByteWriter {
    fn bytes(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.bytes(&v.to_be_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.bytes(&v.to_be_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.bytes(&v.to_be_bytes());
    }
    fn text(&mut self, text: &str, max_bytes: usize) {
        let text = clean_text(text, max_bytes);
        self.u8(text.len() as u8);
        self.bytes(text.as_bytes());
    }
}

struct ByteReader<'a>(&'a [u8]);

impl<'a> ByteReader<'a> {
    fn take(&mut self, n: usize) -> DecodeResult<&'a [u8]> {
        if self.0.len() < n {
            return Err(DecodeError::Truncated);
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(head)
    }
    fn array<const N: usize>(&mut self) -> DecodeResult<[u8; N]> {
        let mut out = [0; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }
    fn u8(&mut self) -> DecodeResult<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> DecodeResult<u16> {
        Ok(u16::from_be_bytes(self.array()?))
    }
    fn u32(&mut self) -> DecodeResult<u32> {
        Ok(u32::from_be_bytes(self.array()?))
    }
    fn u64(&mut self) -> DecodeResult<u64> {
        Ok(u64::from_be_bytes(self.array()?))
    }
    fn text(&mut self, max_bytes: usize) -> DecodeResult<String> {
        let len = usize::from(self.u8()?);
        if len > max_bytes {
            return Err(DecodeError::Invalid("text length"));
        }
        let text = std::str::from_utf8(self.take(len)?)
            .map_err(|_| DecodeError::Invalid("text encoding"))?;
        Ok(clean_text(text, max_bytes))
    }
}

// --- Client side -----------------------------------------------------------

/// Seconds between query attempts to a server that has not answered.
pub const RETRY_SECONDS: f64 = 0.8;
/// Query attempts before a server counts as offline.
pub const QUERY_TRIES: u8 = 3;
/// LAN broadcasts per scan (UDP broadcasts are not resent by anything else).
const LAN_SENDS: u8 = 2;
/// Seconds to wait for each page of the master's list.
const MASTER_TIMEOUT: f64 = 2.0;
/// Attempts to reach the master before giving up.
const MASTER_TRIES: u8 = 3;
/// Cap on listed servers, so a hostile master cannot exhaust memory.
pub const MAX_SERVERS: usize = 1024;

/// Where a server in the browser came from. A server can come from several.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Lan,
    Internet,
    Favorite,
    /// An address typed into the browser to connect to.
    Direct,
}

impl Source {
    fn bit(self) -> u8 {
        match self {
            Source::Lan => 1,
            Source::Internet => 2,
            Source::Favorite => 4,
            Source::Direct => 8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryState {
    /// Waiting for an answer.
    Querying,
    /// Answered; `info` and `ping` are set.
    Online,
    /// Never answered.
    Offline,
}

/// One server in the browser.
#[derive(Clone, Debug)]
pub struct Entry {
    /// Where its query port is.
    pub query_addr: SocketAddrV4,
    pub info: Option<ServerInfo>,
    /// Round trip of the last answered query, in seconds.
    pub ping: Option<f64>,
    pub state: QueryState,
    sources: u8,
    nonce: u32,
    sent_at: f64,
    tries: u8,
}

impl Entry {
    pub fn has_source(&self, source: Source) -> bool {
        self.sources & source.bit() != 0
    }

    /// The address to join: the query address's IP and the server's game port.
    pub fn game_addr(&self) -> SocketAddrV4 {
        let port = self.info.as_ref().map_or_else(
            || self.query_addr.port().wrapping_sub(1),
            |info| info.game_port,
        );
        SocketAddrV4::new(*self.query_addr.ip(), port)
    }
}

/// The state of the last request for the master's list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MasterState {
    Idle,
    Requesting,
    /// The whole list arrived.
    Done,
    /// No answer from the master.
    Failed,
}

struct MasterRequest {
    addr: SocketAddrV4,
    nonce: u32,
    token: u64,
    start: u16,
    sent_at: f64,
    tries: u8,
}

/// Client-side discovery, without sockets.
pub struct Browser {
    entries: Vec<Entry>,
    /// Nonce and send time of each broadcast of the current LAN scan. Each
    /// broadcast has its own nonce, so an answer is timed from the right one.
    lan: Vec<(u32, f64)>,
    master: Option<MasterRequest>,
    master_state: MasterState,
    next_nonce: u32,
    outbox: Vec<(SocketAddrV4, Vec<u8>)>,
    changed: bool,
}

impl Browser {
    /// `seed` starts the nonce sequence; use something unpredictable (time,
    /// process id) so another host cannot guess the next nonce.
    pub fn new(seed: u32) -> Self {
        Self {
            entries: Vec::new(),
            lan: Vec::new(),
            master: None,
            master_state: MasterState::Idle,
            next_nonce: seed | 1,
            outbox: Vec::new(),
            changed: false,
        }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn master_state(&self) -> MasterState {
        self.master_state
    }

    /// Whether the list changed since the last call.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// Packets to send, and where.
    pub fn drain_outbox(&mut self) -> std::vec::Drain<'_, (SocketAddrV4, Vec<u8>)> {
        self.outbox.drain(..)
    }

    /// Forgets the servers found through `source` (they stay if another source
    /// also lists them).
    pub fn clear(&mut self, source: Source) {
        let before = self.entries.len();
        for entry in &mut self.entries {
            entry.sources &= !source.bit();
        }
        self.entries.retain(|e| e.sources != 0);
        if source == Source::Lan {
            self.lan.clear();
        }
        if source == Source::Internet {
            self.master = None;
            self.master_state = MasterState::Idle;
        }
        self.changed |= self.entries.len() != before;
    }

    /// Broadcasts a query to every LAN query port, and to the same ports on
    /// this machine (broadcasts do not always loop back).
    pub fn scan_lan(&mut self, now: f64) {
        self.clear(Source::Lan);
        self.send_lan(now);
    }

    fn send_lan(&mut self, now: f64) {
        let nonce = self.nonce();
        let bytes = Packet::Query { nonce }.encode();
        for port in LAN_QUERY_PORTS {
            for ip in [Ipv4Addr::BROADCAST, Ipv4Addr::LOCALHOST] {
                self.outbox
                    .push((SocketAddrV4::new(ip, port), bytes.clone()));
            }
        }
        self.lan.push((nonce, now));
    }

    /// Adds a server by its query address (or marks an existing one with
    /// `source`) and queries it again.
    pub fn add(&mut self, query_addr: SocketAddrV4, source: Source, now: f64) {
        let index = match self.entries.iter().position(|e| e.query_addr == query_addr) {
            Some(index) => index,
            None => {
                if self.entries.len() >= MAX_SERVERS {
                    return;
                }
                self.entries.push(Entry {
                    query_addr,
                    info: None,
                    ping: None,
                    state: QueryState::Querying,
                    sources: 0,
                    nonce: 0,
                    sent_at: now,
                    tries: 0,
                });
                self.entries.len() - 1
            }
        };
        let nonce = self.nonce();
        let entry = &mut self.entries[index];
        entry.sources |= source.bit();
        entry.state = QueryState::Querying;
        entry.tries = 0;
        entry.nonce = nonce;
        Self::send_query(&mut self.outbox, entry, now);
        self.changed = true;
    }

    /// Removes `source` from one server (the favorite star turned off).
    pub fn remove(&mut self, query_addr: SocketAddrV4, source: Source) {
        let before = self.entries.len();
        for entry in &mut self.entries {
            if entry.query_addr == query_addr {
                entry.sources &= !source.bit();
            }
        }
        self.entries.retain(|e| e.sources != 0);
        self.changed |= self.entries.len() != before;
    }

    /// Asks the master at `master` for its list, page by page, and queries
    /// every server on it.
    pub fn request_master_list(&mut self, master: SocketAddrV4, now: f64) {
        self.clear(Source::Internet);
        let nonce = self.nonce();
        self.master = Some(MasterRequest {
            addr: master,
            nonce,
            token: 0,
            start: 0,
            sent_at: now,
            tries: 0,
        });
        self.master_state = MasterState::Requesting;
        self.send_master_request(now);
        self.changed = true;
    }

    fn send_master_request(&mut self, now: f64) {
        let Some(request) = &mut self.master else {
            return;
        };
        let packet = Packet::ListRequest {
            nonce: request.nonce,
            token: request.token,
            start: request.start,
        };
        request.sent_at = now;
        request.tries += 1;
        self.outbox.push((request.addr, packet.encode()));
    }

    /// Handles a packet from `from`. Anything unexpected is ignored.
    pub fn receive(&mut self, from: SocketAddrV4, bytes: &[u8], now: f64) {
        match Packet::decode(bytes) {
            Ok(Packet::Info { nonce, info }) => self.on_info(from, nonce, info, now),
            Ok(Packet::Challenge { nonce, token }) => {
                if let Some(request) = &mut self.master
                    && request.addr == from
                    && request.nonce == nonce
                {
                    request.token = token;
                    request.tries = 0;
                    self.send_master_request(now);
                }
            }
            Ok(Packet::List {
                nonce,
                start,
                total,
                servers,
            }) => self.on_list(from, nonce, start, total, &servers, now),
            _ => {}
        }
    }

    fn on_info(&mut self, from: SocketAddrV4, nonce: u32, info: ServerInfo, now: f64) {
        let lan_sent_at = self
            .lan
            .iter()
            .find(|(lan_nonce, _)| *lan_nonce == nonce)
            .map(|(_, sent_at)| *sent_at);
        let lan = lan_sent_at.is_some();
        if let Some(index) = self.entries.iter().position(|e| e.query_addr == from) {
            let entry = &mut self.entries[index];
            let sent_at = match lan_sent_at {
                Some(sent_at) => sent_at,
                None if entry.nonce == nonce => entry.sent_at,
                None => return,
            };
            entry.ping = Some((now - sent_at).max(0.0));
            entry.info = Some(info);
            entry.state = QueryState::Online;
            if lan {
                entry.sources |= Source::Lan.bit();
            }
            self.changed = true;
            return;
        }
        let Some(sent_at) = lan_sent_at else {
            return;
        };
        if self.entries.len() >= MAX_SERVERS {
            return;
        }
        // The same server already answered at another address (localhost and
        // its LAN address): keep the one that answered first.
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|e| e.info.as_ref().is_some_and(|i| i.instance == info.instance))
        {
            entry.sources |= Source::Lan.bit();
            return;
        }
        self.entries.push(Entry {
            query_addr: from,
            info: Some(info),
            ping: Some((now - sent_at).max(0.0)),
            state: QueryState::Online,
            sources: Source::Lan.bit(),
            nonce,
            sent_at,
            tries: 1,
        });
        self.changed = true;
    }

    fn on_list(
        &mut self,
        from: SocketAddrV4,
        nonce: u32,
        start: u16,
        total: u16,
        servers: &[SocketAddrV4],
        now: f64,
    ) {
        let Some(request) = &self.master else {
            return;
        };
        if request.addr != from || request.nonce != nonce || request.start != start {
            return;
        }
        for &server in servers {
            if !server.ip().is_unspecified() && server.port() != 0 {
                self.add(server, Source::Internet, now);
            }
        }
        let next = usize::from(start) + servers.len();
        if servers.is_empty() || next >= usize::from(total) || next >= MAX_SERVERS {
            self.master = None;
            self.master_state = MasterState::Done;
            self.changed = true;
            return;
        }
        if let Some(request) = &mut self.master {
            request.start = next as u16;
            request.tries = 0;
        }
        self.send_master_request(now);
    }

    /// Resends unanswered queries and gives up on silent servers and masters.
    pub fn update(&mut self, now: f64) {
        if let Some(&(_, last)) = self.lan.last()
            && self.lan.len() < usize::from(LAN_SENDS)
            && now - last >= RETRY_SECONDS / 2.0
        {
            self.send_lan(now);
        }
        for entry in &mut self.entries {
            if entry.state != QueryState::Querying || now - entry.sent_at < RETRY_SECONDS {
                continue;
            }
            if entry.tries >= QUERY_TRIES {
                entry.state = QueryState::Offline;
                entry.ping = None;
                self.changed = true;
            } else {
                Self::send_query(&mut self.outbox, entry, now);
            }
        }
        if let Some(request) = &self.master
            && now - request.sent_at >= MASTER_TIMEOUT
        {
            if request.tries >= MASTER_TRIES {
                self.master = None;
                self.master_state = MasterState::Failed;
                self.changed = true;
            } else {
                self.send_master_request(now);
            }
        }
    }

    fn send_query(outbox: &mut Vec<(SocketAddrV4, Vec<u8>)>, entry: &mut Entry, now: f64) {
        entry.sent_at = now;
        entry.tries += 1;
        outbox.push((
            entry.query_addr,
            Packet::Query { nonce: entry.nonce }.encode(),
        ));
    }

    fn nonce(&mut self) -> u32 {
        // An odd-step Weyl sequence through a mixer: never repeats within 2^32
        // calls, and consecutive nonces look unrelated.
        self.next_nonce = self.next_nonce.wrapping_add(0x9E37_79B9);
        let mut x = self.next_nonce;
        x ^= x >> 16;
        x = x.wrapping_mul(0x7FEB_352D);
        x ^= x >> 15;
        x = x.wrapping_mul(0x846C_A68B);
        x ^= x >> 16;
        x.max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(instance: u64) -> ServerInfo {
        ServerInfo {
            instance,
            protocol: 3,
            game_port: 27015,
            name: "Friday 1v1".into(),
            map: "Arena".into(),
            mode: "Free-for-all".into(),
            players: 1,
            max_players: 12,
            bots: 3,
            build: "abc1234".into(),
        }
    }

    fn addr(last: u8, port: u16) -> SocketAddrV4 {
        SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, last), port)
    }

    #[test]
    fn every_packet_round_trips() {
        let packets = [
            Packet::Query { nonce: 7 },
            Packet::Info {
                nonce: 9,
                info: info(42),
            },
            Packet::Heartbeat { token: u64::MAX },
            Packet::Goodbye { token: 5 },
            Packet::ListRequest {
                nonce: 1,
                token: 2,
                start: 300,
            },
            Packet::Challenge { nonce: 3, token: 4 },
            Packet::List {
                nonce: 5,
                start: 0,
                total: 2,
                servers: vec![addr(1, 27016), addr(2, 27018)],
            },
        ];
        for packet in packets {
            assert_eq!(Packet::decode(&packet.encode()), Ok(packet));
        }
    }

    #[test]
    fn requests_are_padded_and_replies_cannot_amplify() {
        for request in [
            Packet::Query { nonce: 1 },
            Packet::Heartbeat { token: 1 },
            Packet::Goodbye { token: 1 },
            Packet::ListRequest {
                nonce: 1,
                token: 1,
                start: 1,
            },
        ] {
            let bytes = request.encode();
            assert_eq!(bytes.len(), REQUEST_SIZE);
            assert!(Packet::decode(&bytes[..REQUEST_SIZE - 1]).is_err());
        }
        let longest = ServerInfo {
            name: "n".repeat(100),
            map: "m".repeat(100),
            mode: "x".repeat(100),
            build: "b".repeat(100),
            ..info(1)
        };
        let reply = Packet::Info {
            nonce: 1,
            info: longest,
        };
        assert!(reply.encode().len() < REQUEST_SIZE);
        assert!(Packet::Challenge { nonce: 1, token: 1 }.encode().len() < REQUEST_SIZE);
    }

    #[test]
    fn info_without_a_build_still_decodes() {
        // Servers older than the build field end the reply after `bots`.
        let packet = Packet::Info {
            nonce: 1,
            info: info(1),
        };
        let mut bytes = packet.encode();
        bytes.truncate(bytes.len() - 1 - "abc1234".len());
        let Ok(Packet::Info { info: old, .. }) = Packet::decode(&bytes) else {
            panic!("an info without a build did not decode");
        };
        assert_eq!(old.build, "");
        assert_eq!(old.name, "Friday 1v1");
    }

    #[test]
    fn text_is_cleaned_and_cut_on_a_char_boundary() {
        assert_eq!(clean_text("  a\nb\u{7}c  ", 32), "a b c");
        assert_eq!(clean_text("ééé", 5), "éé");
        let packet = Packet::Info {
            nonce: 1,
            info: ServerInfo {
                name: "x".repeat(40),
                ..info(1)
            },
        };
        let Ok(Packet::Info { info, .. }) = Packet::decode(&packet.encode()) else {
            panic!("info did not decode");
        };
        assert_eq!(info.name.len(), MAX_NAME_BYTES);
    }

    #[test]
    fn garbage_is_rejected() {
        assert!(Packet::decode(b"").is_err());
        assert!(Packet::decode(b"hello world").is_err());
        let mut bytes = Packet::Query { nonce: 1 }.encode();
        bytes[4] = DISCOVERY_VERSION + 1;
        assert!(Packet::decode(&bytes).is_err());
        let mut list = Packet::List {
            nonce: 1,
            start: 0,
            total: 1,
            servers: vec![addr(1, 1)],
        }
        .encode();
        list[14] = 200; // more servers than fit
        assert!(Packet::decode(&list).is_err());
    }

    /// Answers every query in the outbox as `info` from the address it went to.
    fn answer(browser: &mut Browser, now: f64, reply: impl Fn(SocketAddrV4) -> Option<u64>) {
        let sent: Vec<_> = browser.drain_outbox().collect();
        for (to, bytes) in sent {
            if let Ok(Packet::Query { nonce }) = Packet::decode(&bytes)
                && let Some(instance) = reply(to)
            {
                let mut info = info(instance);
                info.game_port = to.port() - 1;
                let bytes = Packet::Info { nonce, info }.encode();
                browser.receive(to, &bytes, now);
            }
        }
    }

    #[test]
    fn lan_scan_lists_each_server_once_with_its_ping() {
        let mut browser = Browser::new(1);
        browser.scan_lan(10.0);
        let sent: Vec<_> = browser.drain_outbox().collect();
        assert_eq!(sent.len(), LAN_QUERY_PORTS.count() * 2);
        let Ok(Packet::Query { nonce }) = Packet::decode(&sent[0].1) else {
            panic!("not a query");
        };
        // The same server answers on localhost and at its LAN address.
        for from in [
            SocketAddrV4::new(Ipv4Addr::LOCALHOST, 27016),
            addr(5, 27016),
        ] {
            let bytes = Packet::Info {
                nonce,
                info: info(77),
            }
            .encode();
            browser.receive(from, &bytes, 10.03);
        }
        assert_eq!(browser.entries().len(), 1);
        let entry = &browser.entries()[0];
        assert_eq!(entry.state, QueryState::Online);
        assert!((entry.ping.unwrap() - 0.03).abs() < 1e-9);
        assert_eq!(entry.game_addr().port(), 27015);
        assert!(entry.has_source(Source::Lan));
    }

    #[test]
    fn the_second_lan_broadcast_is_timed_from_its_own_send() {
        let mut browser = Browser::new(1);
        browser.scan_lan(0.0);
        let first: Vec<_> = browser.drain_outbox().collect();
        let Ok(Packet::Query { nonce: first_nonce }) = Packet::decode(&first[0].1) else {
            panic!("not a query");
        };
        let from = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 27016);
        let reply = |nonce| {
            Packet::Info {
                nonce,
                info: info(1),
            }
            .encode()
        };
        browser.receive(from, &reply(first_nonce), 0.01);
        browser.update(0.5);
        let second: Vec<_> = browser.drain_outbox().collect();
        assert!(!second.is_empty(), "a second broadcast goes out");
        let Ok(Packet::Query {
            nonce: second_nonce,
        }) = Packet::decode(&second[0].1)
        else {
            panic!("not a query");
        };
        assert_ne!(first_nonce, second_nonce);
        browser.receive(from, &reply(second_nonce), 0.52);
        let ping = browser.entries()[0].ping.unwrap();
        assert!((ping - 0.02).abs() < 1e-9, "ping {ping}");
        browser.update(5.0);
        assert_eq!(browser.drain_outbox().count(), 0, "only two broadcasts");
    }

    #[test]
    fn answers_with_the_wrong_nonce_are_ignored() {
        let mut browser = Browser::new(1);
        browser.add(addr(1, 27016), Source::Favorite, 0.0);
        browser.drain_outbox().for_each(drop);
        let forged = Packet::Info {
            nonce: 12345,
            info: info(1),
        }
        .encode();
        browser.receive(addr(1, 27016), &forged, 0.1);
        browser.receive(addr(9, 27016), &forged, 0.1);
        assert_eq!(browser.entries().len(), 1);
        assert_eq!(browser.entries()[0].state, QueryState::Querying);
    }

    #[test]
    fn silent_servers_are_retried_then_marked_offline() {
        let mut browser = Browser::new(1);
        browser.add(addr(1, 27016), Source::Favorite, 0.0);
        let mut queries = browser.drain_outbox().count();
        let mut now = 0.0;
        while now < 5.0 {
            now += 0.1;
            browser.update(now);
            queries += browser.drain_outbox().count();
        }
        assert_eq!(queries, usize::from(QUERY_TRIES));
        assert_eq!(browser.entries()[0].state, QueryState::Offline);

        // A later answer to the last query still counts.
        browser.add(addr(1, 27016), Source::Favorite, 6.0);
        answer(&mut browser, 6.05, |_| Some(3));
        assert_eq!(browser.entries()[0].state, QueryState::Online);
        assert!(browser.entries()[0].has_source(Source::Favorite));
    }

    #[test]
    fn master_list_handshake_pages_and_queries() {
        let master = addr(100, DEFAULT_MASTER_PORT);
        let servers: Vec<_> = (0..200u16).map(|i| addr(1, 20000 + i * 2)).collect();
        let mut browser = Browser::new(9);
        browser.request_master_list(master, 0.0);
        let mut pages = 0;
        for _ in 0..10 {
            let sent: Vec<_> = browser.drain_outbox().collect();
            let mut replies = Vec::new();
            for (to, bytes) in sent {
                if to != master {
                    continue;
                }
                let Ok(Packet::ListRequest {
                    nonce,
                    token,
                    start,
                }) = Packet::decode(&bytes)
                else {
                    panic!("expected a list request");
                };
                if token != 99 {
                    replies.push(Packet::Challenge { nonce, token: 99 });
                    continue;
                }
                pages += 1;
                let start_index = usize::from(start);
                let end = (start_index + MAX_LIST_ENTRIES).min(servers.len());
                replies.push(Packet::List {
                    nonce,
                    start,
                    total: servers.len() as u16,
                    servers: servers[start_index..end].to_vec(),
                });
            }
            for reply in replies {
                browser.receive(master, &reply.encode(), 0.1);
            }
        }
        assert_eq!(pages, 2);
        assert_eq!(browser.master_state(), MasterState::Done);
        assert_eq!(browser.entries().len(), servers.len());
        assert!(
            browser
                .entries()
                .iter()
                .all(|e| e.has_source(Source::Internet))
        );
    }

    #[test]
    fn a_silent_master_fails() {
        let mut browser = Browser::new(1);
        browser.request_master_list(addr(100, DEFAULT_MASTER_PORT), 0.0);
        let mut now = 0.0;
        while now < 10.0 {
            now += 0.25;
            browser.update(now);
        }
        assert_eq!(browser.master_state(), MasterState::Failed);
    }

    #[test]
    fn clearing_a_source_keeps_servers_other_sources_list() {
        let mut browser = Browser::new(1);
        browser.add(addr(1, 27016), Source::Favorite, 0.0);
        browser.add(addr(1, 27016), Source::Internet, 0.0);
        browser.add(addr(2, 27016), Source::Internet, 0.0);
        browser.clear(Source::Internet);
        assert_eq!(browser.entries().len(), 1);
        assert!(browser.entries()[0].has_source(Source::Favorite));
        browser.remove(addr(1, 27016), Source::Favorite);
        assert!(browser.entries().is_empty());
    }
}
