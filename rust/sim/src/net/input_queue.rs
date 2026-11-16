//! Server-side buffer of one client's inputs.
//!
//! The server never invents input. Each tick it applies the next queued input,
//! or none if nothing has arrived yet, so the server runs exactly the inputs the
//! client predicted with, in order. When packets bunch up after jitter, it
//! applies two or three per tick to drain the queue and keep latency low.
//! (Source engine "usercmd" model; no clock synchronisation needed.)

use std::collections::VecDeque;

use crate::player::PlayerInput;

/// A client flooding inputs beyond this many queued ticks loses the oldest.
const MAX_QUEUED: usize = 60;

#[derive(Default)]
pub struct InputQueue {
    pending: VecDeque<(u32, PlayerInput)>,
    last_applied: u32,
    /// Inputs dropped as duplicates or because the queue was full.
    pub discarded: u32,
    /// Ticks with no input to apply.
    pub starved: u32,
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

    /// Inputs to apply this tick, oldest first.
    pub fn take(&mut self) -> impl Iterator<Item = (u32, PlayerInput)> + '_ {
        let count = match self.pending.len() {
            0 => {
                self.starved += 1;
                0
            }
            1..=2 => 1,
            3..=5 => 2,
            _ => 3,
        };
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
