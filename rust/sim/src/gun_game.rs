//! Gun Game: every player for themselves, up a fixed ladder of weapons.
//!
//! Everyone starts with the first weapon on [`LADDER`] and nothing else.
//! Kills with it move a player up to the next weapon, in the middle of a life:
//! the new weapon comes up at once with a full magazine. The first player to
//! score their kills with the last weapon wins. Dying by your own hand (a
//! fall), or to an enemy's knife, moves you back down one weapon; a knife
//! kill does not move the killer up. If the clock runs out first, the
//! player furthest up the ladder wins, and of players level with each other,
//! the one who got there first.
//!
//! The server keeps each player's [`Progress`] and swaps their weapon
//! (`CombatState::rearm`); clients learn every player's level from the
//! scoreboard.

use crate::weapon::WeaponId;

/// The weapons in the order players climb them: the forgiving ones first, the
/// sniper and the pistol last.
pub const LADDER: [&str; 8] = ["lsw6", "ak47", "m4", "br3", "pdw9", "m870", "m40", "m1911"];

/// Number of weapons on the ladder.
pub const LEVELS: u8 = LADDER.len() as u8;

/// Tunable Gun Game rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    /// Kills a player needs with each weapon to move on (at least 1).
    pub kills_per_weapon: u8,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            kills_per_weapon: 2,
        }
    }
}

/// The weapon for a ladder level (0 is the first). Levels past the end get the
/// last weapon.
pub fn weapon(level: u8) -> WeaponId {
    let index = usize::from(level).min(LADDER.len() - 1);
    WeaponId::expect(LADDER[index])
}

/// One player's place on the ladder.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    /// Index into [`LADDER`].
    pub level: u8,
    /// Kills with the current weapon so far.
    pub kills: u8,
    /// Server tick of the last change, to break ties: earlier is ahead.
    pub since: u32,
}

/// What a kill did for the killer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Counted toward the current weapon; not enough to move on yet.
    Counted,
    /// Moved up to the next weapon.
    Promoted,
    /// The last weapon's last kill: the match is won.
    Won,
}

impl Progress {
    /// Counts a kill with the current weapon, at server tick `tick`.
    pub fn on_kill(&mut self, rules: &Rules, tick: u32) -> Step {
        self.since = tick;
        self.kills = self.kills.saturating_add(1);
        if self.kills < rules.kills_per_weapon.max(1) {
            return Step::Counted;
        }
        if self.level + 1 >= LEVELS {
            return Step::Won;
        }
        self.level += 1;
        self.kills = 0;
        Step::Promoted
    }

    /// A suicide at server tick `tick`: back down one weapon, with its kills
    /// to score again. Returns whether the player lost a level (not at the
    /// bottom).
    pub fn on_suicide(&mut self, tick: u32) -> bool {
        self.since = tick;
        self.kills = 0;
        if self.level == 0 {
            return false;
        }
        self.level -= 1;
        true
    }

    /// Kills still needed with the current weapon.
    pub fn kills_to_go(&self, rules: &Rules) -> u8 {
        rules.kills_per_weapon.max(1).saturating_sub(self.kills)
    }

    /// Orders players by how far up the ladder they are, then by who got
    /// there first.
    fn rank(&self) -> Rank {
        (self.level, self.kills, std::cmp::Reverse(self.since))
    }
}

/// Level, kills on it, and the tick they got there (earlier ranks higher).
type Rank = (u8, u8, std::cmp::Reverse<u32>);

/// The player furthest up the ladder, and of players level with each other
/// the one who got there first; `None` for a dead heat (or no players).
/// Decides the winner when the clock runs out.
pub fn leader(players: impl IntoIterator<Item = (u8, Progress)>) -> Option<u8> {
    let mut best: Option<(u8, Rank)> = None;
    let mut tied = false;
    for (id, progress) in players {
        let rank = progress.rank();
        match best {
            Some((_, top)) if rank < top => {}
            Some((_, top)) if rank == top => tied = true,
            _ => {
                best = Some((id, rank));
                tied = false;
            }
        }
    }
    best.filter(|_| !tied).map(|(id, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapon::Slot;

    #[test]
    fn the_ladder_names_real_weapons_and_ends_on_the_pistol() {
        for level in 0..LEVELS {
            let def = weapon(level).def();
            assert!(!def.beam, "the laser stays secret");
            assert!(def.fitted.is_empty(), "{} is a base weapon", def.key);
        }
        assert_eq!(weapon(LEVELS - 1).def().slot, Slot::Secondary);
        assert_eq!(weapon(200), weapon(LEVELS - 1));
        // Every weapon appears once.
        let mut keys = LADDER.to_vec();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), LADDER.len());
    }

    #[test]
    fn kills_climb_the_ladder_and_the_last_one_wins() {
        let rules = Rules {
            kills_per_weapon: 2,
        };
        let mut progress = Progress::default();
        assert_eq!(progress.kills_to_go(&rules), 2);
        assert_eq!(progress.on_kill(&rules, 10), Step::Counted);
        assert_eq!(progress.kills_to_go(&rules), 1);
        assert_eq!(progress.on_kill(&rules, 20), Step::Promoted);
        assert_eq!(
            progress,
            Progress {
                level: 1,
                kills: 0,
                since: 20
            }
        );
        let mut steps = Vec::new();
        for tick in 0..u32::from(LEVELS - 1) * 2 {
            steps.push(progress.on_kill(&rules, 30 + tick));
        }
        assert_eq!(steps.last(), Some(&Step::Won));
        assert_eq!(
            steps.iter().filter(|s| **s == Step::Promoted).count(),
            usize::from(LEVELS) - 2
        );
        assert_eq!(progress.level, LEVELS - 1);
    }

    #[test]
    fn one_kill_per_weapon_is_the_minimum() {
        let rules = Rules {
            kills_per_weapon: 0,
        };
        let mut progress = Progress::default();
        assert_eq!(progress.on_kill(&rules, 1), Step::Promoted);
    }

    #[test]
    fn a_suicide_costs_a_weapon_and_the_kills_on_it() {
        let mut progress = Progress {
            level: 3,
            kills: 1,
            since: 5,
        };
        assert!(progress.on_suicide(9));
        assert_eq!(
            progress,
            Progress {
                level: 2,
                kills: 0,
                since: 9
            }
        );
        let mut bottom = Progress {
            level: 0,
            kills: 1,
            since: 5,
        };
        assert!(!bottom.on_suicide(0));
        assert_eq!(bottom, Progress::default());
    }

    #[test]
    fn the_leader_is_furthest_up_and_ties_have_none() {
        let p = |level, kills, since| Progress {
            level,
            kills,
            since,
        };
        assert_eq!(
            leader([(1, p(2, 0, 0)), (2, p(3, 0, 0)), (3, p(1, 1, 0))]),
            Some(2)
        );
        assert_eq!(leader([(1, p(3, 1, 9)), (2, p(3, 0, 1))]), Some(1));
        // Level with each other: whoever got there first.
        assert_eq!(leader([(1, p(3, 1, 50)), (2, p(3, 1, 40))]), Some(2));
        assert_eq!(leader([(1, p(3, 1, 40)), (2, p(3, 1, 40))]), None);
        assert_eq!(
            leader([(1, p(3, 1, 4)), (2, p(3, 1, 4)), (3, p(4, 0, 9))]),
            Some(3)
        );
        assert_eq!(leader([]), None);
    }
}
