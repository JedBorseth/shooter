//! Bandwidth bench: what each bot would cost on the wire if it were a client.
//!
//! The server hands every bot's snapshot and inputs to a [`Probe`], which
//! models a client behind a fixed round trip: it acknowledges the newest
//! snapshot that could have reached it and come back, and each of its input
//! packets carries every input the server has not acknowledged yet, exactly
//! as `prediction::Prediction::unacked` would. The probe encodes both with
//! the real wire format and counts the bytes, so a headless bot match
//! (`godot/tests/bandwidth_test.gd`) measures kilobits per second per client
//! without a client process.

use std::collections::{BTreeMap, VecDeque};

use super::Tick;
use super::protocol::{ClientMessage, InputPacket, MAX_INPUTS_PER_PACKET};
use crate::TICK_RATE;
use crate::player::PlayerInput;

/// Bytes each packet costs beyond its payload: IPv4 (20), UDP (8), and
/// ENet's protocol and unreliable send command headers (12).
pub const WIRE_OVERHEAD_BYTES: u64 = 40;

/// Snapshot ticks kept to look acknowledgements up in (about two seconds).
const SENT_TICKS: usize = 128;

/// Byte counts summed over every measured client.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Totals {
    /// Snapshot payload bytes, and how many of them were the receiver's own
    /// state.
    pub down_bytes: u64,
    pub own_bytes: u64,
    pub down_packets: u64,
    /// Input packet payload bytes.
    pub up_bytes: u64,
    pub up_packets: u64,
    /// Ticks measured, summed over clients (one client for a second is
    /// `TICK_RATE`).
    pub client_ticks: u64,
}

/// Kilobits per second per client.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rates {
    pub down_kbps: f64,
    pub own_kbps: f64,
    pub up_kbps: f64,
    /// With [`WIRE_OVERHEAD_BYTES`] on every packet.
    pub down_wire_kbps: f64,
    pub up_wire_kbps: f64,
    /// Packets per second per client.
    pub down_pps: f64,
    pub up_pps: f64,
}

impl Totals {
    pub fn rates(&self) -> Rates {
        let seconds = self.client_ticks as f64 / f64::from(TICK_RATE);
        if seconds <= 0.0 {
            return Rates::default();
        }
        let kbps = |bytes: u64| bytes as f64 * 8.0 / 1000.0 / seconds;
        Rates {
            down_kbps: kbps(self.down_bytes),
            own_kbps: kbps(self.own_bytes),
            up_kbps: kbps(self.up_bytes),
            down_wire_kbps: kbps(self.down_bytes + self.down_packets * WIRE_OVERHEAD_BYTES),
            up_wire_kbps: kbps(self.up_bytes + self.up_packets * WIRE_OVERHEAD_BYTES),
            down_pps: self.down_packets as f64 / seconds,
            up_pps: self.up_packets as f64 / seconds,
        }
    }
}

#[derive(Default)]
struct ProbeClient {
    /// Inputs by the tick the server applied them, newest last.
    inputs: VecDeque<(Tick, u32, PlayerInput)>,
    next_seq: u32,
}

/// Models clients behind a round trip of `rtt_ticks` and counts their bytes.
pub struct Probe {
    rtt_ticks: Tick,
    sent: VecDeque<Tick>,
    clients: BTreeMap<u8, ProbeClient>,
    /// Counting starts once this is set ([`Probe::start`]).
    measuring: bool,
    pub totals: Totals,
}

impl Probe {
    pub fn new(rtt_ms: f64) -> Self {
        Self {
            rtt_ticks: (rtt_ms.max(0.0) * f64::from(TICK_RATE) / 1000.0).round() as Tick,
            sent: VecDeque::new(),
            clients: BTreeMap::new(),
            measuring: false,
            totals: Totals::default(),
        }
    }

    /// Starts counting (after warm-up), from zero.
    pub fn start(&mut self) {
        self.measuring = true;
        self.totals = Totals::default();
    }

    /// The newest snapshot a client would have acknowledged by `now`: one sent
    /// at least a round trip ago.
    pub fn ack(&self, now: Tick) -> Option<Tick> {
        let limit = now.checked_sub(self.rtt_ticks)?;
        self.sent.iter().rev().find(|t| **t <= limit).copied()
    }

    /// The server sent snapshots for `tick`.
    pub fn on_snapshot(&mut self, tick: Tick) {
        self.sent.push_back(tick);
        while self.sent.len() > SENT_TICKS {
            self.sent.pop_front();
        }
    }

    /// The sequence number of the last input of `id` the server had applied
    /// by `tick` (0 for none), as a snapshot's `last_input`.
    pub fn last_input(&self, id: u8, tick: Tick) -> u32 {
        self.clients
            .get(&id)
            .and_then(|c| c.inputs.iter().rev().find(|(t, _, _)| *t <= tick))
            .map_or(0, |(_, seq, _)| *seq)
    }

    /// A snapshot went to `id`: `bytes` in all, `own_bytes` of them its own
    /// state.
    pub fn count_snapshot(&mut self, bytes: usize, own_bytes: usize) {
        if self.measuring {
            self.totals.down_bytes += bytes as u64;
            self.totals.own_bytes += own_bytes as u64;
            self.totals.down_packets += 1;
        }
    }

    /// The server applied `input` for `id` on tick `now`. Encodes the packet
    /// that delivered it: every input since the newest snapshot the client
    /// has, as the client resends them.
    pub fn on_input(&mut self, id: u8, now: Tick, mut input: PlayerInput) {
        let ack = self.ack(now);
        // A client's attacking inputs name the tick it draws others at:
        // half a round trip and about 35 ms of interpolation behind.
        if input.fire || input.melee {
            let behind = f64::from(self.rtt_ticks) / 2.0 + 4.3;
            input.view_tick = Some((f64::from(now) - behind).max(0.0));
        }
        let client = self.clients.entry(id).or_default();
        client.next_seq += 1;
        client.inputs.push_back((now, client.next_seq, input));
        // Inputs the acknowledged snapshot already confirmed are not resent.
        let acked = ack.unwrap_or(0);
        while client.inputs.len() > 1
            && (client.inputs.len() > MAX_INPUTS_PER_PACKET * 2
                || client.inputs.front().is_some_and(|(t, _, _)| *t <= acked))
        {
            client.inputs.pop_front();
        }
        let pending: Vec<(Tick, u32, PlayerInput)> = client
            .inputs
            .iter()
            .filter(|(t, _, _)| ack.is_none_or(|a| *t > a))
            .copied()
            .collect();
        let Some(&(_, first_seq, _)) = pending.first() else {
            return;
        };
        let bytes = ClientMessage::Input(InputPacket {
            ack_snapshot: ack,
            first_seq,
            inputs: pending.iter().map(|(_, _, i)| *i).collect(),
        })
        .encode();
        if self.measuring {
            self.totals.up_bytes += bytes.len() as u64;
            self.totals.up_packets += 1;
        }
    }

    /// One tick of `clients` clients measured.
    pub fn on_tick(&mut self, clients: usize) {
        if self.measuring {
            self.totals.client_ticks += clients as u64;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acknowledges_a_snapshot_a_round_trip_old() {
        // 100 ms at 120 Hz is 12 ticks.
        let mut probe = Probe::new(100.0);
        for tick in (2..=40).step_by(2) {
            probe.on_snapshot(tick);
        }
        assert_eq!(probe.ack(40), Some(28));
        assert_eq!(probe.ack(41), Some(28));
        assert_eq!(probe.ack(13), None);
    }

    #[test]
    fn input_packets_carry_every_unacknowledged_input() {
        let mut probe = Probe::new(50.0);
        probe.start();
        for tick in 0..240 {
            if tick % 2 == 0 {
                probe.on_snapshot(tick);
            }
            probe.on_input(7, tick, PlayerInput::default());
            probe.on_tick(1);
        }
        let rates = probe.totals.rates();
        assert_eq!(probe.totals.up_packets, 240);
        assert!((rates.up_pps - 120.0).abs() < 1e-9);
        // Idle inputs repeat, so each costs a bit after the first.
        assert!(rates.up_kbps > 0.0 && rates.up_kbps < 20.0, "{rates:?}");
        assert_eq!(probe.last_input(7, 239), 240);
    }
}
