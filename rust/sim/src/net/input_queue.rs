//! Server-side buffer of one client's inputs.
//!
//! The server never invents input. Each tick it applies the next queued input,
//! or none if nothing has arrived yet, so the server runs exactly the inputs the
//! client predicted with, in order. When packets bunch up after jitter, it
//! applies two or three per tick to drain the queue and keep latency low.
//! (Source engine "usercmd" model; no clock synchronisation needed.)
//!
//! Draining is paid for with credit that builds up one input per tick, so
//! over time no client gets more than one input per tick. Without the cap, a
//! tampered client that sends inputs faster than the tick rate would move,
//! fire, and reload faster than everyone else. Honest clients only bank credit
//! while their packets are late, which is exactly the backlog they then drain.

use std::collections::VecDeque;

use crate::player::PlayerInput;

/// A client flooding inputs beyond this many queued ticks loses the oldest.
const MAX_QUEUED: usize = 60;
/// Most inputs a client can bank to drain a backlog: a quarter second of
/// late packets.
pub const MAX_CREDIT: u32 = super::TICK_RATE / 4;

pub struct InputQueue {
    pending: VecDeque<(u32, PlayerInput)>,
    last_applied: u32,
    /// Inputs this client may still apply beyond one per tick.
    credit: u32,
    /// Inputs dropped as duplicates or because the queue was full.
    pub discarded: u32,
    /// Ticks with no input to apply.
    pub starved: u32,
    /// Ticks that applied fewer inputs than queued because the client had
    /// already used its credit (sending faster than the tick rate).
    pub throttled: u32,
}

impl Default for InputQueue {
    fn default() -> Self {
        Self {
            pending: VecDeque::new(),
            last_applied: 0,
            credit: MAX_CREDIT,
            discarded: 0,
            starved: 0,
            throttled: 0,
        }
    }
}

impl InputQueue {
    /// Sequence number of the last input handed out by [`take`](Self::take) (0 for none).
    pub fn last_applied(&self) -> u32 {
        self.last_applied
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Adds a packet of consecutive inputs starting at `first_seq`. Inputs
    /// already queued or applied are ignored, so redundant copies are harmless.
    pub fn receive(&mut self, first_seq: u32, inputs: &[PlayerInput]) {
        let newest = self
            .pending
            .back()
            .map_or(self.last_applied, |(seq, _)| *seq);
        for (offset, input) in inputs.iter().enumerate() {
            let Some(seq) = first_seq.checked_add(offset as u32) else {
                break;
            };
            if seq == 0 || seq <= newest {
                self.discarded += 1;
                continue;
            }
            self.pending.push_back((seq, *input));
        }
        while self.pending.len() > MAX_QUEUED {
            self.pending.pop_front();
            self.discarded += 1;
        }
    }

    /// Inputs to apply this tick, oldest first. Call exactly once per tick.
    pub fn take(&mut self) -> impl Iterator<Item = (u32, PlayerInput)> + '_ {
        // Every tick pays for one input; a tick that applies none banks it.
        self.credit = (self.credit + 1).min(MAX_CREDIT + 1);
        let wanted = match self.pending.len() {
            0 => {
                self.starved += 1;
                0
            }
            1..=2 => 1,
            3..=5 => 2,
            _ => 3,
        };
        let count = wanted.min(self.credit as usize);
        if count < wanted {
            self.throttled += 1;
        }
        self.credit -= count as u32;
        if count > 0 {
            self.last_applied = self.pending[count - 1].0;
        }
        self.pending.drain(..count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(n: usize) -> Vec<PlayerInput> {
        (0..n)
            .map(|i| PlayerInput {
                yaw: i as f32,
                ..Default::default()
            })
            .collect()
    }

    #[test]
    fn redundant_copies_are_applied_once() {
        let mut q = InputQueue::default();
        q.receive(1, &inputs(3));
        q.receive(1, &inputs(4));
        let taken: Vec<u32> = q.take().map(|(s, _)| s).collect();
        assert_eq!(taken, vec![1, 2]);
        let taken: Vec<u32> = q.take().map(|(s, _)| s).collect();
        assert_eq!(taken, vec![3]);
        assert_eq!(q.last_applied(), 3);
        // A stale packet arriving late changes nothing.
        q.receive(1, &inputs(3));
        let taken: Vec<u32> = q.take().map(|(s, _)| s).collect();
        assert_eq!(taken, vec![4]);
        assert_eq!(q.take().count(), 0);
        assert_eq!(q.starved, 1);
        assert_eq!(q.last_applied(), 4);
    }

    #[test]
    fn a_backlog_drains_faster_than_real_time() {
        let mut q = InputQueue::default();
        q.receive(1, &inputs(8));
        let per_tick: Vec<usize> = (0..5).map(|_| q.take().count()).collect();
        assert_eq!(per_tick, vec![3, 2, 2, 1, 0]);
        assert_eq!(q.last_applied(), 8);
    }

    #[test]
    fn gaps_from_lost_packets_are_skipped() {
        let mut q = InputQueue::default();
        q.receive(1, &inputs(1));
        q.receive(5, &inputs(1));
        let taken: Vec<u32> = q.take().map(|(s, _)| s).collect();
        assert_eq!(taken, vec![1]);
        let taken: Vec<u32> = q.take().map(|(s, _)| s).collect();
        assert_eq!(taken, vec![5]);
    }

    #[test]
    fn inputs_faster_than_the_tick_rate_are_throttled() {
        // A tampered client sends two inputs per tick for ten seconds.
        let mut q = InputQueue::default();
        let mut seq = 1;
        let mut applied = 0;
        let ticks = 10 * crate::TICK_RATE;
        for _ in 0..ticks {
            q.receive(seq, &inputs(2));
            seq += 2;
            applied += q.take().count() as u32;
        }
        assert!(
            applied <= ticks + MAX_CREDIT + 1,
            "{applied} inputs in {ticks} ticks"
        );
        assert!(q.throttled > 0);
    }

    #[test]
    fn late_packets_are_drained_at_full_speed() {
        // An honest client at one input per tick whose packets stall for
        // 200 ms, then arrive together.
        let mut q = InputQueue::default();
        let mut seq = 1;
        for _ in 0..600 {
            q.receive(seq, &inputs(1));
            seq += 1;
            q.take().count();
        }
        for _ in 0..24 {
            assert_eq!(q.take().count(), 0, "starved during the stall");
        }
        q.receive(seq, &inputs(24));
        seq += 24;
        let mut ticks = 0;
        // Drained once the queue is back to the one or two inputs in flight.
        while q.len() > 2 {
            q.receive(seq, &inputs(1));
            seq += 1;
            q.take().count();
            ticks += 1;
            assert!(ticks < 40, "the backlog drains");
        }
        assert!(ticks <= 16, "drained in {ticks} ticks");
        assert_eq!(q.throttled, 0, "an honest client is never throttled");
    }

    #[test]
    fn flooding_is_capped() {
        let mut q = InputQueue::default();
        q.receive(1, &inputs(16));
        for start in (17..400).step_by(16) {
            q.receive(start, &inputs(16));
        }
        assert_eq!(q.len(), MAX_QUEUED);
        assert!(q.discarded > 0);
    }
}
