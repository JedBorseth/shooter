//! Recent snapshots by tick, used as delta-compression baselines.
//!
//! The server keeps every player's view for the last second of ticks; a client
//! keeps the remote players of each snapshot it received. When a client
//! acknowledges tick T, the server encodes against its record for T and the
//! client decodes against its own copy of T, so both sides agree on the baseline.
//! Each entry also keeps players' own full states: the server every
//! player's, a client its own, so own states are delta-encoded the same way.

use std::collections::VecDeque;

use super::Tick;
use super::protocol::{PlayerView, Predicted, ZombieView};

/// About a second of 60 Hz snapshots. An older acknowledgement falls back to a full snapshot.
pub const HISTORY_TICKS: usize = 64;

#[derive(Default)]
pub struct SnapshotHistory {
    entries: VecDeque<(Tick, Vec<PlayerView>)>,
    /// Own states by tick, beside `entries` (sorted the same way).
    owns: VecDeque<(Tick, Vec<(u8, Predicted)>)>,
    /// Zombies: the horde of each snapshot that carried it.
    hordes: VecDeque<(Tick, Vec<ZombieView>)>,
}

impl SnapshotHistory {
    /// Records a snapshot's players and the own states sent with it.
    pub fn insert_with_own(
        &mut self,
        tick: Tick,
        players: Vec<PlayerView>,
        owns: Vec<(u8, Predicted)>,
    ) {
        self.insert(tick, players);
        if self.owns.back().is_some_and(|(t, _)| *t >= tick) {
            let index = self.owns.partition_point(|(t, _)| *t < tick);
            if self.owns.get(index).is_some_and(|(t, _)| *t == tick) {
                return;
            }
            self.owns.insert(index, (tick, owns));
        } else {
            self.owns.push_back((tick, owns));
        }
        while self.owns.len() > HISTORY_TICKS {
            self.owns.pop_front();
        }
    }

    /// Records the horde a snapshot carried.
    pub fn insert_zombies(&mut self, tick: Tick, zombies: Vec<ZombieView>) {
        if self.hordes.back().is_some_and(|(t, _)| *t >= tick) {
            let index = self.hordes.partition_point(|(t, _)| *t < tick);
            if self.hordes.get(index).is_some_and(|(t, _)| *t == tick) {
                return;
            }
            self.hordes.insert(index, (tick, zombies));
        } else {
            self.hordes.push_back((tick, zombies));
        }
        // Hordes go out at most every other snapshot; keep as long a reach.
        while self.hordes.len() > HISTORY_TICKS {
            self.hordes.pop_front();
        }
    }

    /// The horde the snapshot for `tick` carried, if it carried one.
    pub fn zombies(&self, tick: Tick) -> Option<&[ZombieView]> {
        let index = self.hordes.partition_point(|(t, _)| *t < tick);
        self.hordes
            .get(index)
            .filter(|(t, _)| *t == tick)
            .map(|(_, z)| z.as_slice())
    }

    /// The newest horde sent at or before `tick`, and its tick.
    pub fn zombies_by(&self, tick: Tick) -> Option<(Tick, &[ZombieView])> {
        let index = self.hordes.partition_point(|(t, _)| *t <= tick);
        let (t, z) = self.hordes.get(index.checked_sub(1)?)?;
        Some((*t, z.as_slice()))
    }

    /// Player `id`'s own state as sent with the snapshot for `tick`.
    pub fn own(&self, tick: Tick, id: u8) -> Option<&Predicted> {
        let index = self.owns.partition_point(|(t, _)| *t < tick);
        let (t, owns) = self.owns.get(index)?;
        if *t != tick {
            return None;
        }
        owns.iter().find(|(i, _)| *i == id).map(|(_, p)| p)
    }

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

    #[test]
    fn keeps_own_states_by_tick_and_player() {
        let mut h = SnapshotHistory::default();
        for tick in 0..100 {
            let own = Predicted {
                state: crate::player::PlayerState::default(),
                position: glam::Vec3::new(tick as f32, 0.0, 0.0),
                on_floor: true,
            };
            h.insert_with_own(tick, view(1), vec![(3, own)]);
        }
        assert_eq!(h.own(99, 3).unwrap().position.x, 99.0);
        assert_eq!(h.own(40, 3).unwrap().position.x, 40.0);
        assert!(h.own(40, 4).is_none());
        assert!(h.own(10, 3).is_none());
        assert_eq!(h.get(40).unwrap()[0].id, 1);
    }

    #[test]
    fn keeps_hordes_by_tick() {
        let mut h = SnapshotHistory::default();
        for tick in (0..400).step_by(4) {
            h.insert_zombies(
                tick,
                vec![ZombieView::new(tick as u8, glam::Vec3::ZERO, 0.0, true)],
            );
        }
        assert_eq!(h.zombies(396).unwrap()[0].id, 396u32 as u8);
        assert!(h.zombies(397).is_none());
        assert_eq!(h.zombies_by(399).unwrap().0, 396);
        assert_eq!(h.zombies_by(150).unwrap().0, 148);
        assert!(h.zombies(0).is_none(), "older than the history");
    }
}
