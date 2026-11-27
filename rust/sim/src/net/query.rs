//! Server queries: what a client asks a server before it joins.
//!
//! Every dedicated server answers queries on a plain UDP *query port*, next to
//! its ENet game port ([`query_port_for`]). A client joining by address sends
//! a [`Packet::Query`] there first and reads the [`Packet::Info`] reply: the
//! server's name, map, players, game protocol, and build. The game loads that
//! map before it connects, and turns away a server of another version. The
//! launcher's `serve` asks its own server the same way, to see whether anyone
//! is playing.
//!
//! Abuse: every query is padded to [`REQUEST_SIZE`], and the reply is smaller
//! than that, so a spoofed query cannot be amplified.
//!
//! [`Query`] is the client side, without sockets: send what [`Query::poll`]
//! returns, and feed it the replies and the time.

use super::DEFAULT_PORT;
use super::bits::{DecodeError, DecodeResult};

/// Bumped whenever this format changes. Packets of other versions are ignored.
pub const QUERY_VERSION: u8 = 2;
/// Every query packet starts with this.
const MAGIC: [u8; 4] = *b"SHTQ";
/// Queries are padded to this many bytes; shorter ones are ignored.
pub const REQUEST_SIZE: usize = 160;
/// The query port of the default game port.
pub const DEFAULT_QUERY_PORT: u16 = DEFAULT_PORT + 1;
/// Longest server name and map name, in bytes.
pub const MAX_NAME_BYTES: usize = 32;
pub const MAX_MAP_BYTES: usize = 24;
/// Longest build name (a tag or short commit hash), in bytes.
pub const MAX_BUILD_BYTES: usize = 32;

/// A server's query port: the port after its game port.
pub fn query_port_for(game_port: u16) -> u16 {
    game_port.wrapping_add(1)
}

/// What a server says about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerInfo {
    /// The game protocol (`protocol::PROTOCOL_VERSION`) the server speaks.
    pub protocol: u16,
    pub name: String,
    pub map: String,
    /// Human players connected.
    pub players: u8,
    pub max_players: u8,
    /// Which build the server runs (a tag or short commit hash).
    pub build: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Packet {
    /// Client → server query port: who are you?
    Query { nonce: u32 },
    /// Server → client: the answer to the query with `nonce`.
    Info { nonce: u32, info: ServerInfo },
}

const KIND_QUERY: u8 = 1;
const KIND_INFO: u8 = 2;

impl Packet {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = ByteWriter::default();
        w.bytes(&MAGIC);
        w.u8(QUERY_VERSION);
        match self {
            Packet::Query { nonce } => {
                w.u8(KIND_QUERY);
                w.u32(*nonce);
                w.0.resize(REQUEST_SIZE, 0);
            }
            Packet::Info { nonce, info } => {
                w.u8(KIND_INFO);
                w.u32(*nonce);
                w.u16(info.protocol);
                w.text(&info.name, MAX_NAME_BYTES);
                w.text(&info.map, MAX_MAP_BYTES);
                w.u8(info.players);
                w.u8(info.max_players);
                w.text(&info.build, MAX_BUILD_BYTES);
            }
        }
        w.0
    }

    pub fn decode(bytes: &[u8]) -> DecodeResult<Packet> {
        let mut r = ByteReader(bytes);
        if r.take(4)? != MAGIC {
            return Err(DecodeError::Invalid("query magic"));
        }
        if r.u8()? != QUERY_VERSION {
            return Err(DecodeError::Invalid("query version"));
        }
        match r.u8()? {
            KIND_QUERY => {
                if bytes.len() < REQUEST_SIZE {
                    return Err(DecodeError::Invalid("unpadded query"));
                }
                Ok(Packet::Query { nonce: r.u32()? })
            }
            KIND_INFO => Ok(Packet::Info {
                nonce: r.u32()?,
                info: ServerInfo {
                    protocol: r.u16()?,
                    name: r.text(MAX_NAME_BYTES)?,
                    map: r.text(MAX_MAP_BYTES)?,
                    players: r.u8()?,
                    max_players: r.u8()?,
                    build: r.text(MAX_BUILD_BYTES)?,
                },
            }),
            _ => Err(DecodeError::Invalid("query packet kind")),
        }
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
/// Query attempts before a server counts as silent.
pub const QUERY_TRIES: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryState {
    /// Waiting for an answer.
    Asking,
    /// Answered; `info` and `ping` are set.
    Answered,
    /// Never answered.
    Silent,
}

/// One server being asked who it is, without sockets.
#[derive(Clone, Debug)]
pub struct Query {
    nonce: u32,
    sent_at: f64,
    tries: u8,
    pub state: QueryState,
    pub info: Option<ServerInfo>,
    /// Round trip of the answered query, in seconds.
    pub ping: Option<f64>,
}

impl Query {
    /// `nonce` should be unpredictable (random), so another host cannot forge
    /// the answer.
    pub fn new(nonce: u32) -> Self {
        Self {
            nonce,
            sent_at: f64::NEG_INFINITY,
            tries: 0,
            state: QueryState::Asking,
            info: None,
            ping: None,
        }
    }

    /// The query to send now, if one is due: at once, then again every
    /// [`RETRY_SECONDS`] until [`QUERY_TRIES`] went unanswered.
    pub fn poll(&mut self, now: f64) -> Option<Vec<u8>> {
        if self.state != QueryState::Asking || now - self.sent_at < RETRY_SECONDS {
            return None;
        }
        if self.tries >= QUERY_TRIES {
            self.state = QueryState::Silent;
            return None;
        }
        self.sent_at = now;
        self.tries += 1;
        Some(Packet::Query { nonce: self.nonce }.encode())
    }

    /// Handles a packet from the server's query address. Anything but the
    /// answer to this query is ignored; a late answer still counts. Returns
    /// whether it was the answer.
    pub fn receive(&mut self, bytes: &[u8], now: f64) -> bool {
        match Packet::decode(bytes) {
            Ok(Packet::Info { nonce, info }) if nonce == self.nonce => {
                self.ping = Some((now - self.sent_at).max(0.0));
                self.info = Some(info);
                self.state = QueryState::Answered;
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> ServerInfo {
        ServerInfo {
            protocol: 3,
            name: "Friday 1v1".into(),
            map: "Arena".into(),
            players: 1,
            max_players: 12,
            build: "abc1234".into(),
        }
    }

    #[test]
    fn every_packet_round_trips() {
        for packet in [
            Packet::Query { nonce: 7 },
            Packet::Info {
                nonce: 9,
                info: info(),
            },
        ] {
            assert_eq!(Packet::decode(&packet.encode()), Ok(packet));
        }
    }

    #[test]
    fn queries_are_padded_and_replies_cannot_amplify() {
        let bytes = Packet::Query { nonce: 1 }.encode();
        assert_eq!(bytes.len(), REQUEST_SIZE);
        assert!(Packet::decode(&bytes[..REQUEST_SIZE - 1]).is_err());
        let longest = ServerInfo {
            name: "n".repeat(100),
            map: "m".repeat(100),
            build: "b".repeat(100),
            ..info()
        };
        let reply = Packet::Info {
            nonce: 1,
            info: longest,
        };
        assert!(reply.encode().len() < REQUEST_SIZE);
    }

    #[test]
    fn text_is_cleaned_and_cut_on_a_char_boundary() {
        assert_eq!(clean_text("  a\nb\u{7}c  ", 32), "a b c");
        assert_eq!(clean_text("ééé", 5), "éé");
        let packet = Packet::Info {
            nonce: 1,
            info: ServerInfo {
                name: "x".repeat(40),
                ..info()
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
        bytes[4] = QUERY_VERSION + 1;
        assert!(Packet::decode(&bytes).is_err());
    }

    /// The answer to `query`'s last packet, as the server would send it.
    fn answer(sent: &[u8]) -> Vec<u8> {
        let Ok(Packet::Query { nonce }) = Packet::decode(sent) else {
            panic!("not a query");
        };
        Packet::Info {
            nonce,
            info: info(),
        }
        .encode()
    }

    #[test]
    fn an_answer_is_timed_from_its_query() {
        let mut query = Query::new(41);
        let sent = query.poll(10.0).expect("a query goes out at once");
        assert_eq!(query.poll(10.1), None, "no resend before the retry time");
        assert!(query.receive(&answer(&sent), 10.03));
        assert_eq!(query.state, QueryState::Answered);
        assert_eq!(query.info, Some(info()));
        assert!((query.ping.unwrap() - 0.03).abs() < 1e-9);
        assert_eq!(query.poll(20.0), None, "nothing more to send");
    }

    #[test]
    fn answers_with_the_wrong_nonce_are_ignored() {
        let mut query = Query::new(41);
        query.poll(0.0);
        let forged = Packet::Info {
            nonce: 12345,
            info: info(),
        }
        .encode();
        assert!(!query.receive(&forged, 0.1));
        assert_eq!(query.state, QueryState::Asking);
    }

    #[test]
    fn a_silent_server_is_asked_again_then_given_up_on() {
        let mut query = Query::new(41);
        let mut sent = Vec::new();
        let mut now = 0.0;
        while now < 5.0 {
            sent.extend(query.poll(now));
            now += 0.1;
        }
        assert_eq!(sent.len(), usize::from(QUERY_TRIES));
        assert_eq!(query.state, QueryState::Silent);

        // A late answer to the last query still counts.
        assert!(query.receive(&answer(&sent[0]), 6.0));
        assert_eq!(query.state, QueryState::Answered);
    }
}
