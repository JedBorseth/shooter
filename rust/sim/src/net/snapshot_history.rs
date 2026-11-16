//! Recent snapshots by tick, used as delta-compression baselines.
//!
//! The server keeps every player's view for the last second of ticks; a client
//! keeps the remote players of each snapshot it received. When a client
//! acknowledges tick T, the server encodes against its record for T and the
//! client decodes against its own copy of T, so both sides agree on the baseline.

use std::collections::VecDeque;

use super::Tick;
use super::protocol::PlayerView;

/// One second at 60 Hz. An older acknowledgement falls back to a full snapshot.
pub const HISTORY_TICKS: usize = 64;

#[derive(Default)]
pub struct SnapshotHistory {
    entries: VecDeque<(Tick, Vec<PlayerView>)>,
}

impl SnapshotHistory {
    pub fn insert(&mut self, tick: Tick, players: Vec<PlayerView>) {
        if self.entries.back().is_some_and(|(t, _)| *t >= tick) {
            // Out of order: insert in place to keep the list sorted.
            let index = self.entries.partition_point(|(t, _)| *t < tick);
            if self.entries.get(index).is_some_and(|(t, _)| *t == tick) {
                return;
            }
            self.entries.insert(index, (tick, players));
        } else {
            self.entries.push_back((tick, players));
        }
        while self.entries.len() > HISTORY_TICKS {
            self.entries.pop_front();
        }
    }

    pub fn get(&self, tick: Tick) -> Option<&[PlayerView]> {
        let index = self.entries.partition_point(|(t, _)| *t < tick);
        self.entries
            .get(index)
            .filter(|(t, _)| *t == tick)
            .map(|(_, players)| players.as_slice())
    }

    pub fn newest_tick(&self) -> Option<Tick> {
        self.entries.back().map(|(t, _)| *t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(id: u8) -> Vec<PlayerView> {
        vec![PlayerView {
            id,
            ..Default::default()
        }]
    }

    #[test]
    fn keeps_a_second_of_snapshots() {
        let mut h = SnapshotHistory::default();
        for tick in 0..100 {
            h.insert(tick, view(tick as u8));
        }
        assert!(h.get(10).is_none());
        assert_eq!(h.get(99).unwrap()[0].id, 99);
        assert_eq!(h.get(36).unwrap()[0].id, 36);
        assert!(h.get(35).is_none());
        h.insert(50, view(0));
        assert_eq!(h.get(50).unwrap()[0].id, 50, "duplicates are ignored");
        assert_eq!(h.newest_tick(), Some(99));
    }
}
