//! Master server: the list of dedicated servers that clients browse.
//!
//! Dedicated servers started with `--master HOST[:PORT]` send a heartbeat from
//! their query socket every [`HEARTBEAT_SECONDS`]; the master lists that
//! socket's address until heartbeats stop for [`EXPIRE_SECONDS`] or the server
//! says goodbye. Clients ask for the list a page at a time and query each
//! server themselves for its name, players, and ping. The master never relays
//! game traffic.
//!
//! A heartbeat or list request is honoured only with a token the master gave
//! that address (a [`Packet::Challenge`]): a forged source address never sees
//! its token, so it cannot list someone else's machine or aim a list at it.
//! Tokens are keyed hashes of the address and the time window, so the master
//! stores nothing per challenge. The wire format is in
//! `shooter_sim::net::discovery`.

use std::collections::HashMap;
use std::hash::{BuildHasher, RandomState};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use shooter_sim::net::discovery::{MAX_LIST_ENTRIES, Packet};

pub use shooter_sim::net::discovery::DEFAULT_MASTER_PORT;

/// How often a server sends a heartbeat.
pub const HEARTBEAT_SECONDS: f64 = 30.0;
/// A server that sent no heartbeat for this long is dropped.
pub const EXPIRE_SECONDS: f64 = 2.5 * HEARTBEAT_SECONDS;
/// Tokens are valid for one to two of these windows.
const TOKEN_WINDOW_SECONDS: f64 = 60.0;
/// Cap on listed servers.
pub const MAX_SERVERS: usize = 4096;
/// Cap on listed servers per IP address.
pub const MAX_PER_IP: usize = 32;

/// What changed in the list, for logging.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Added(SocketAddrV4),
    Removed(SocketAddrV4),
}

pub struct Master {
    keys: RandomState,
    /// In registration order, so pages stay stable while a client reads them.
    servers: Vec<(SocketAddrV4, f64)>,
    per_ip: HashMap<Ipv4Addr, usize>,
}

impl Default for Master {
    fn default() -> Self {
        Self::new()
    }
}

impl Master {
    pub fn new() -> Self {
        Self {
            keys: RandomState::new(),
            servers: Vec::new(),
            per_ip: HashMap::new(),
        }
    }

    /// Query addresses of the listed servers.
    pub fn servers(&self) -> impl Iterator<Item = SocketAddrV4> + '_ {
        self.servers.iter().map(|(addr, _)| *addr)
    }

    /// Handles one packet from `from` at `now` (seconds, any epoch) and
    /// returns the reply to send back, if any, and what changed in the list.
    pub fn handle(
        &mut self,
        from: SocketAddr,
        bytes: &[u8],
        now: f64,
    ) -> (Option<Vec<u8>>, Option<Change>) {
        let from = match from {
            SocketAddr::V4(from) => from,
            SocketAddr::V6(from) => match from.ip().to_ipv4_mapped() {
                Some(ip) => SocketAddrV4::new(ip, from.port()),
                None => return (None, None),
            },
        };
        let Ok(packet) = Packet::decode(bytes) else {
            return (None, None);
        };
        match packet {
            Packet::Heartbeat { token } => {
                if !self.token_valid(from, token, now) {
                    return (Some(self.challenge(from, 0, now)), None);
                }
                (None, self.register(from, now))
            }
            Packet::Goodbye { token } => {
                if self.token_valid(from, token, now) {
                    (None, self.unregister(from))
                } else {
                    (None, None)
                }
            }
            Packet::ListRequest {
                nonce,
                token,
                start,
            } => {
                if !self.token_valid(from, token, now) {
                    return (Some(self.challenge(from, nonce, now)), None);
                }
                let start_index = usize::from(start).min(self.servers.len());
                let end = (start_index + MAX_LIST_ENTRIES).min(self.servers.len());
                let page = Packet::List {
                    nonce,
                    start,
                    total: self.servers.len() as u16,
                    servers: self.servers[start_index..end]
                        .iter()
                        .map(|(addr, _)| *addr)
                        .collect(),
                };
                (Some(page.encode()), None)
            }
            _ => (None, None),
        }
    }

    /// Drops servers whose heartbeats stopped.
    pub fn expire(&mut self, now: f64) -> Vec<Change> {
        let mut removed = Vec::new();
        self.servers.retain(|(addr, seen)| {
            let alive = now - seen < EXPIRE_SECONDS;
            if !alive {
                removed.push(Change::Removed(*addr));
            }
            alive
        });
        for change in &removed {
            if let Change::Removed(addr) = change {
                self.release_ip(*addr.ip());
            }
        }
        removed
    }

    fn register(&mut self, from: SocketAddrV4, now: f64) -> Option<Change> {
        if let Some(entry) = self.servers.iter_mut().find(|(addr, _)| *addr == from) {
            entry.1 = now;
            return None;
        }
        let count = self.per_ip.get(from.ip()).copied().unwrap_or(0);
        if self.servers.len() >= MAX_SERVERS || count >= MAX_PER_IP {
            return None;
        }
        self.servers.push((from, now));
        self.per_ip.insert(*from.ip(), count + 1);
        Some(Change::Added(from))
    }

    fn unregister(&mut self, from: SocketAddrV4) -> Option<Change> {
        let index = self.servers.iter().position(|(addr, _)| *addr == from)?;
        self.servers.remove(index);
        self.release_ip(*from.ip());
        Some(Change::Removed(from))
    }

    fn release_ip(&mut self, ip: Ipv4Addr) {
        if let Some(count) = self.per_ip.get_mut(&ip) {
            *count -= 1;
            if *count == 0 {
                self.per_ip.remove(&ip);
            }
        }
    }

    fn challenge(&self, from: SocketAddrV4, nonce: u32, now: f64) -> Vec<u8> {
        let token = self.token(from, window(now));
        Packet::Challenge { nonce, token }.encode()
    }

    fn token(&self, addr: SocketAddrV4, window: u64) -> u64 {
        // Never 0, which means "no token yet".
        self.keys
            .hash_one((addr.ip().octets(), addr.port(), window))
            .max(1)
    }

    fn token_valid(&self, addr: SocketAddrV4, token: u64, now: f64) -> bool {
        let current = window(now);
        token != 0
            && (token == self.token(addr, current)
                || (current > 0 && token == self.token(addr, current - 1)))
    }
}

fn window(now: f64) -> u64 {
    (now.max(0.0) / TOKEN_WINDOW_SECONDS) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(last: u8, port: u16) -> SocketAddr {
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, last), port))
    }

    /// Sends `packet` from `from`, answering one challenge, and returns the reply.
    fn send(master: &mut Master, from: SocketAddr, packet: Packet, now: f64) -> Option<Packet> {
        let (reply, _) = master.handle(from, &packet.encode(), now);
        let reply = Packet::decode(&reply?).ok()?;
        let Packet::Challenge { token, .. } = reply else {
            return Some(reply);
        };
        let retry = match packet {
            Packet::Heartbeat { .. } => Packet::Heartbeat { token },
            Packet::ListRequest { nonce, start, .. } => Packet::ListRequest {
                nonce,
                token,
                start,
            },
            other => other,
        };
        let (reply, _) = master.handle(from, &retry.encode(), now);
        reply.and_then(|bytes| Packet::decode(&bytes).ok())
    }

    fn list(master: &mut Master, now: f64) -> Vec<SocketAddrV4> {
        let request = Packet::ListRequest {
            nonce: 5,
            token: 0,
            start: 0,
        };
        match send(master, addr(200, 50000), request, now) {
            Some(Packet::List { servers, nonce, .. }) => {
                assert_eq!(nonce, 5);
                servers
            }
            other => panic!("expected a list, got {other:?}"),
        }
    }

    #[test]
    fn heartbeats_need_a_token() {
        let mut master = Master::new();
        let (reply, change) = master.handle(
            addr(1, 27016),
            &Packet::Heartbeat { token: 0 }.encode(),
            0.0,
        );
        assert!(change.is_none());
        assert!(matches!(
            Packet::decode(&reply.unwrap()),
            Ok(Packet::Challenge { .. })
        ));
        // A made-up token is not accepted either.
        let (_, change) = master.handle(
            addr(1, 27016),
            &Packet::Heartbeat { token: 1234 }.encode(),
            0.0,
        );
        assert!(change.is_none());
        assert_eq!(master.servers().count(), 0);

        assert!(
            send(
                &mut master,
                addr(1, 27016),
                Packet::Heartbeat { token: 0 },
                0.0
            )
            .is_none()
        );
        assert_eq!(
            list(&mut master, 1.0),
            vec![SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 27016)]
        );
    }

    #[test]
    fn a_token_only_works_from_its_own_address() {
        let mut master = Master::new();
        let (reply, _) = master.handle(
            addr(1, 27016),
            &Packet::Heartbeat { token: 0 }.encode(),
            0.0,
        );
        let Ok(Packet::Challenge { token, .. }) = Packet::decode(&reply.unwrap()) else {
            panic!("expected a challenge");
        };
        let (_, change) = master.handle(addr(2, 27016), &Packet::Heartbeat { token }.encode(), 0.0);
        assert!(change.is_none());
        let (_, change) = master.handle(addr(1, 27016), &Packet::Heartbeat { token }.encode(), 0.0);
        assert!(matches!(change, Some(Change::Added(_))));
    }

    #[test]
    fn silent_servers_expire_and_goodbye_removes_at_once() {
        let mut master = Master::new();
        send(
            &mut master,
            addr(1, 27016),
            Packet::Heartbeat { token: 0 },
            0.0,
        );
        send(
            &mut master,
            addr(2, 27016),
            Packet::Heartbeat { token: 0 },
            0.0,
        );
        send(
            &mut master,
            addr(2, 27016),
            Packet::Heartbeat { token: 0 },
            50.0,
        );
        assert!(master.expire(EXPIRE_SECONDS - 1.0).is_empty());
        assert_eq!(master.expire(EXPIRE_SECONDS + 1.0).len(), 1);
        assert_eq!(master.servers().count(), 1);

        let (reply, _) = master.handle(
            addr(2, 27016),
            &Packet::Heartbeat { token: 0 }.encode(),
            80.0,
        );
        let Ok(Packet::Challenge { token, .. }) = Packet::decode(&reply.unwrap()) else {
            panic!("expected a challenge");
        };
        let (_, change) = master.handle(addr(2, 27016), &Packet::Goodbye { token }.encode(), 80.0);
        assert!(matches!(change, Some(Change::Removed(_))));
        assert_eq!(master.servers().count(), 0);
    }

    #[test]
    fn list_requests_need_a_token_and_page() {
        let mut master = Master::new();
        for i in 0..(MAX_PER_IP as u16) {
            send(
                &mut master,
                addr(1, 20000 + i),
                Packet::Heartbeat { token: 0 },
                0.0,
            );
        }
        for i in 0..200u16 {
            let from = addr((2 + i / 30) as u8, 20000 + i);
            send(&mut master, from, Packet::Heartbeat { token: 0 }, 0.0);
        }
        assert_eq!(master.servers().count(), MAX_PER_IP + 200);
        // The 33rd server on one IP is refused.
        send(
            &mut master,
            addr(1, 30000),
            Packet::Heartbeat { token: 0 },
            0.0,
        );
        assert_eq!(master.servers().count(), MAX_PER_IP + 200);

        // Without a token: only a small challenge comes back.
        let request = Packet::ListRequest {
            nonce: 1,
            token: 0,
            start: 0,
        }
        .encode();
        let (reply, _) = master.handle(addr(250, 1), &request, 1.0);
        assert!(reply.unwrap().len() < request.len());

        let first = list(&mut master, 1.0);
        assert_eq!(first.len(), MAX_LIST_ENTRIES);
        let second = send(
            &mut master,
            addr(200, 50000),
            Packet::ListRequest {
                nonce: 2,
                token: 0,
                start: MAX_LIST_ENTRIES as u16,
            },
            1.0,
        );
        let Some(Packet::List { servers, total, .. }) = second else {
            panic!("expected the second page");
        };
        assert_eq!(usize::from(total), MAX_PER_IP + 200);
        assert_eq!(first.len() + servers.len(), usize::from(total));
    }

    #[test]
    fn tokens_outlive_one_window_but_not_two() {
        let master = Master::new();
        let from = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 27016);
        let token = master.token(from, window(10.0));
        assert!(master.token_valid(from, token, 10.0));
        assert!(master.token_valid(from, token, 10.0 + TOKEN_WINDOW_SECONDS));
        assert!(!master.token_valid(from, token, 10.0 + 2.0 * TOKEN_WINDOW_SECONDS));
        assert!(!master.token_valid(from, 0, 10.0));
    }
}
