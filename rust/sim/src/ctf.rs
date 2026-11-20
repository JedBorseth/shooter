//! Capture the Flag: where each team's flag is, and who has it.
//!
//! Each team has a flag at its base. The rules are the classic ones:
//!
//! - Walk over the enemy's flag, at their base or wherever it was dropped, to
//!   take it.
//! - Carry it to your own flag's base to capture it and score a point. Your
//!   own flag must be at home for that, so a team has to get its flag back
//!   before it can score.
//! - A carrier who dies, or leaves, drops the flag where they were.
//! - Walk over your own dropped flag to send it home at once. Left alone, a
//!   dropped flag goes home by itself after [`FlagRules::return_time`].
//!
//! The match is played in two halves, as in Black Ops 2: at half the clock
//! the teams swap bases ([`Flags::set_sides`]), both flags go home, and the
//! score carries over ([`halftime_due`]). A draw at the end goes to one
//! overtime round from the original sides, as long as a half, where the
//! next capture wins ([`crate::tdm::MatchState::on_capture`]); with no
//! capture it stays a draw.
//!
//! The match clock, the score limit, and the team scores are in
//! [`crate::tdm`]; this is only the flags. The server runs it; clients show
//! what it sends (`net::protocol::GameEvent::Flags`).

use glam::Vec3;

use crate::tdm::{TEAM_COUNT, Team};

/// Tunable flag rules.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlagRules {
    /// How close (horizontally, in metres) a player must come to a flag to
    /// take or return it.
    pub touch_radius: f32,
    /// How close to their own base a carrier must come to capture.
    pub capture_radius: f32,
    /// Most height difference between a player's feet and a flag that still
    /// counts as touching it.
    pub touch_height: f32,
    /// Seconds a dropped flag waits before it goes home by itself.
    pub return_time: f32,
}

impl Default for FlagRules {
    fn default() -> Self {
        Self {
            touch_radius: 1.4,
            capture_radius: 2.0,
            touch_height: 1.6,
            return_time: 30.0,
        }
    }
}

/// Where a flag is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FlagState {
    /// At its base.
    Home,
    /// Carried by player `by`.
    Carried { by: u8 },
    /// Lying at `at`, going home by itself in `return_in` seconds.
    Dropped { at: Vec3, return_in: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flag {
    /// Whose flag it is.
    pub team: Team,
    /// Where its base is.
    pub home: Vec3,
    pub state: FlagState,
    /// Where the flag is now: its base, its carrier's feet, or where it lies.
    pub position: Vec3,
}

/// What a flag needs to know about a player.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlagPlayer {
    pub id: u8,
    pub team: Team,
    pub alive: bool,
    /// Feet position.
    pub position: Vec3,
}

/// Something that happened to a flag. `flag` is the team whose flag it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FlagEvent {
    /// `by` took the flag, from its base or from where it lay.
    Taken { flag: Team, by: u8, from_base: bool },
    /// Its carrier `by` died or left, and dropped it at `at`.
    Dropped { flag: Team, by: u8, at: Vec3 },
    /// The flag went home: touched by `by`, a player of its own team, or
    /// after lying alone too long (`None`).
    Returned { flag: Team, by: Option<u8> },
    /// `by` carried it to their own base and scored.
    Captured { flag: Team, by: u8 },
}

/// Both teams' flags.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flags {
    pub flags: [Flag; TEAM_COUNT],
    /// The map's two bases: the first half's homes, by team.
    bases: [Vec3; TEAM_COUNT],
}

impl Flags {
    /// Both flags at home. `homes[t]` is team `t`'s base.
    pub fn new(homes: [Vec3; TEAM_COUNT]) -> Self {
        let flag = |team: usize| Flag {
            team: team as Team,
            home: homes[team],
            state: FlagState::Home,
            position: homes[team],
        };
        Self {
            flags: [flag(0), flag(1)],
            bases: homes,
        }
    }

    /// Puts each team's flag at its own base (`swapped` false) or at the
    /// other team's (true, the second half), and sends both home without
    /// events.
    pub fn set_sides(&mut self, swapped: bool) {
        for (team, flag) in self.flags.iter_mut().enumerate() {
            flag.home = self.bases[team ^ usize::from(swapped)];
        }
        self.reset();
    }

    /// Whether the teams play from each other's bases.
    pub fn swapped(&self) -> bool {
        self.flags[0].home == self.bases[1] && self.bases[0] != self.bases[1]
    }

    /// Sends both flags home without events, for the start of a match.
    pub fn reset(&mut self) {
        for flag in &mut self.flags {
            flag.state = FlagState::Home;
            flag.position = flag.home;
        }
    }

    pub fn flag(&self, team: Team) -> &Flag {
        &self.flags[usize::from(team.min(1))]
    }

    /// The team whose flag player `id` carries, if any.
    pub fn carried_by(&self, id: u8) -> Option<Team> {
        self.flags
            .iter()
            .find(|f| f.state == FlagState::Carried { by: id })
            .map(|f| f.team)
    }

    /// Moves a dropped flag, for the server to put it on the floor under
    /// where its carrier died.
    pub fn settle(&mut self, team: Team, at: Vec3) {
        let flag = &mut self.flags[usize::from(team.min(1))];
        if let FlagState::Dropped { return_in, .. } = flag.state {
            flag.state = FlagState::Dropped { at, return_in };
            flag.position = at;
        }
    }

    /// Advances the flags one step of `dt` seconds. `players` is everyone in
    /// the game. With `live` false (no match being played), carried flags
    /// still drop and dropped flags still go home, but nobody can take or
    /// capture one. Events are appended to `events` in the order they
    /// happened.
    pub fn tick(
        &mut self,
        rules: &FlagRules,
        dt: f32,
        live: bool,
        players: &[FlagPlayer],
        events: &mut Vec<FlagEvent>,
    ) {
        // Carriers who died or left drop the flag; the rest carry it along.
        for flag in &mut self.flags {
            let FlagState::Carried { by } = flag.state else {
                continue;
            };
            match players.iter().find(|p| p.id == by) {
                Some(carrier) if carrier.alive && live => flag.position = carrier.position,
                carrier => {
                    let at = carrier.map_or(flag.position, |c| c.position);
                    flag.state = FlagState::Dropped {
                        at,
                        return_in: rules.return_time,
                    };
                    flag.position = at;
                    events.push(FlagEvent::Dropped {
                        flag: flag.team,
                        by,
                        at,
                    });
                }
            }
        }
        // Dropped flags left alone go home.
        for flag in &mut self.flags {
            if let FlagState::Dropped { at, return_in } = flag.state {
                let left = return_in - dt;
                if left > 0.0 && live {
                    flag.state = FlagState::Dropped {
                        at,
                        return_in: left,
                    };
                } else {
                    flag.state = FlagState::Home;
                    flag.position = flag.home;
                    events.push(FlagEvent::Returned {
                        flag: flag.team,
                        by: None,
                    });
                }
            }
        }
        if !live {
            return;
        }
        // Lowest id first, so ties always go the same way.
        let mut order: Vec<&FlagPlayer> = players.iter().filter(|p| p.alive).collect();
        order.sort_unstable_by_key(|p| p.id);
        // Returns first, so a carrier waiting at home can capture on the tick
        // a teammate brings their flag back.
        for player in &order {
            let flag = &mut self.flags[usize::from(player.team.min(1))];
            if let FlagState::Dropped { at, .. } = flag.state
                && touches(rules, rules.touch_radius, player.position, at)
            {
                flag.state = FlagState::Home;
                flag.position = flag.home;
                events.push(FlagEvent::Returned {
                    flag: flag.team,
                    by: Some(player.id),
                });
            }
        }
        for player in &order {
            if self.carried_by(player.id).is_some() {
                continue;
            }
            let enemy = &mut self.flags[usize::from(1 - player.team.min(1))];
            let (spot, from_base) = match enemy.state {
                FlagState::Home => (enemy.home, true),
                FlagState::Dropped { at, .. } => (at, false),
                FlagState::Carried { .. } => continue,
            };
            if touches(rules, rules.touch_radius, player.position, spot) {
                enemy.state = FlagState::Carried { by: player.id };
                enemy.position = player.position;
                events.push(FlagEvent::Taken {
                    flag: enemy.team,
                    by: player.id,
                    from_base,
                });
            }
        }
        for player in &order {
            let own = usize::from(player.team.min(1));
            let enemy = 1 - own;
            if self.flags[enemy].state != (FlagState::Carried { by: player.id })
                || self.flags[own].state != FlagState::Home
                || !touches(
                    rules,
                    rules.capture_radius,
                    player.position,
                    self.flags[own].home,
                )
            {
                continue;
            }
            let flag = &mut self.flags[enemy];
            flag.state = FlagState::Home;
            flag.position = flag.home;
            events.push(FlagEvent::Captured {
                flag: flag.team,
                by: player.id,
            });
        }
    }
}

/// Whether the second half starts now: half the regulation clock has run in
/// the first half (`time_left` of `time_limit` seconds left).
pub fn halftime_due(time_left: f32, time_limit: Option<f32>, first_half: bool) -> bool {
    first_half && time_limit.is_some_and(|limit| time_left <= limit * 0.5)
}

fn touches(rules: &FlagRules, radius: f32, feet: Vec3, spot: Vec3) -> bool {
    let flat = Vec3::new(feet.x - spot.x, 0.0, feet.z - spot.z).length();
    flat <= radius && (feet.y - spot.y).abs() <= rules.touch_height
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 120.0;
    const HOME_0: Vec3 = Vec3::new(-50.0, 0.0, 0.0);
    const HOME_1: Vec3 = Vec3::new(50.0, 0.0, 0.0);

    fn player(id: u8, team: Team, x: f32) -> FlagPlayer {
        FlagPlayer {
            id,
            team,
            alive: true,
            position: Vec3::new(x, 0.0, 0.0),
        }
    }

    fn step(flags: &mut Flags, players: &[FlagPlayer]) -> Vec<FlagEvent> {
        let mut events = Vec::new();
        flags.tick(&FlagRules::default(), DT, true, players, &mut events);
        events
    }

    #[test]
    fn walking_over_the_enemy_flag_takes_it_but_not_your_own() {
        let mut flags = Flags::new([HOME_0, HOME_1]);
        // A team 0 player at their own base takes nothing.
        assert!(step(&mut flags, &[player(1, 0, -50.0)]).is_empty());
        assert_eq!(flags.flag(0).state, FlagState::Home);
        // At the enemy's base they take the enemy flag.
        let events = step(&mut flags, &[player(1, 0, 49.5)]);
        assert_eq!(
            events,
            [FlagEvent::Taken {
                flag: 1,
                by: 1,
                from_base: true
            }]
        );
        assert_eq!(flags.carried_by(1), Some(1));
        // The flag follows its carrier.
        step(&mut flags, &[player(1, 0, 20.0)]);
        assert_eq!(flags.flag(1).position, Vec3::new(20.0, 0.0, 0.0));
    }

    #[test]
    fn the_dead_and_the_far_away_take_nothing() {
        let mut flags = Flags::new([HOME_0, HOME_1]);
        let mut dead = player(1, 0, 50.0);
        dead.alive = false;
        let above = FlagPlayer {
            position: Vec3::new(50.0, 4.0, 0.0),
            ..player(2, 0, 50.0)
        };
        assert!(step(&mut flags, &[dead, above, player(3, 0, 47.0)]).is_empty());
        assert_eq!(flags.flag(1).state, FlagState::Home);
    }

    #[test]
    fn carrying_the_flag_home_captures_it() {
        let mut flags = Flags::new([HOME_0, HOME_1]);
        step(&mut flags, &[player(1, 0, 50.0)]);
        let events = step(&mut flags, &[player(1, 0, -49.0)]);
        assert_eq!(events, [FlagEvent::Captured { flag: 1, by: 1 }]);
        assert_eq!(flags.flag(1).state, FlagState::Home);
        assert_eq!(flags.flag(1).position, HOME_1);
        assert_eq!(flags.carried_by(1), None);
    }

    #[test]
    fn a_team_cannot_capture_while_its_own_flag_is_away() {
        let mut flags = Flags::new([HOME_0, HOME_1]);
        // Both teams take each other's flag.
        step(&mut flags, &[player(1, 0, 50.0), player(2, 1, -50.0)]);
        assert_eq!(flags.carried_by(1), Some(1));
        assert_eq!(flags.carried_by(2), Some(0));
        // Player 1 waits at home, but their flag is with player 2.
        assert!(step(&mut flags, &[player(1, 0, -50.0), player(2, 1, 0.0)]).is_empty());
        assert_eq!(flags.carried_by(1), Some(1));
        // Player 2 dies; the flag drops in the middle.
        let mut dead = player(2, 1, 0.0);
        dead.alive = false;
        let events = step(&mut flags, &[player(1, 0, -50.0), dead]);
        assert_eq!(
            events,
            [FlagEvent::Dropped {
                flag: 0,
                by: 2,
                at: Vec3::ZERO
            }]
        );
        // A teammate returns it, and player 1 captures on the same tick.
        let events = step(&mut flags, &[player(1, 0, -50.0), player(3, 0, 0.5), dead]);
        assert_eq!(
            events,
            [
                FlagEvent::Returned {
                    flag: 0,
                    by: Some(3)
                },
                FlagEvent::Captured { flag: 1, by: 1 }
            ]
        );
    }

    #[test]
    fn a_dropped_flag_can_be_retaken_and_goes_home_alone() {
        let mut flags = Flags::new([HOME_0, HOME_1]);
        step(&mut flags, &[player(1, 0, 50.0)]);
        // The carrier leaves the game: the flag drops where they were.
        step(&mut flags, &[player(1, 0, 10.0)]);
        let events = step(&mut flags, &[]);
        assert_eq!(
            events,
            [FlagEvent::Dropped {
                flag: 1,
                by: 1,
                at: Vec3::new(10.0, 0.0, 0.0)
            }]
        );
        // Another attacker picks it up where it lies.
        let events = step(&mut flags, &[player(4, 0, 10.5)]);
        assert_eq!(
            events,
            [FlagEvent::Taken {
                flag: 1,
                by: 4,
                from_base: false
            }]
        );
        // Dropped again and left alone, it goes home after the return time.
        let mut dead = player(4, 0, 10.5);
        dead.alive = false;
        step(&mut flags, &[dead]);
        let mut events = Vec::new();
        let rules = FlagRules::default();
        let ticks = (rules.return_time / DT) as usize;
        for _ in 0..ticks - 2 {
            flags.tick(&rules, DT, true, &[], &mut events);
        }
        assert!(events.is_empty(), "{events:?}");
        assert!(matches!(flags.flag(1).state, FlagState::Dropped { .. }));
        for _ in 0..4 {
            flags.tick(&rules, DT, true, &[], &mut events);
        }
        assert_eq!(events, [FlagEvent::Returned { flag: 1, by: None }]);
        assert_eq!(flags.flag(1).position, HOME_1);
    }

    #[test]
    fn touching_your_own_dropped_flag_returns_it() {
        let mut flags = Flags::new([HOME_0, HOME_1]);
        step(&mut flags, &[player(2, 1, -50.0)]);
        let mut dead = player(2, 1, -20.0);
        dead.alive = false;
        step(&mut flags, &[dead]);
        // The enemy walks over it again first: they retake it, not return it.
        let events = step(&mut flags, &[player(5, 1, -20.0)]);
        assert!(matches!(events[..], [FlagEvent::Taken { flag: 0, .. }]));
        let mut dead = player(5, 1, -20.0);
        dead.alive = false;
        step(&mut flags, &[dead]);
        let events = step(&mut flags, &[player(1, 0, -19.0)]);
        assert_eq!(
            events,
            [FlagEvent::Returned {
                flag: 0,
                by: Some(1)
            }]
        );
        assert_eq!(flags.flag(0).state, FlagState::Home);
    }

    #[test]
    fn outside_play_nobody_takes_a_flag_and_carried_ones_go_home() {
        let mut flags = Flags::new([HOME_0, HOME_1]);
        let mut events = Vec::new();
        let rules = FlagRules::default();
        flags.tick(&rules, DT, false, &[player(1, 0, 50.0)], &mut events);
        assert!(events.is_empty());
        flags.tick(&rules, DT, true, &[player(1, 0, 50.0)], &mut events);
        events.clear();
        // The match ends with the flag carried: it drops and goes home.
        flags.tick(&rules, DT, false, &[player(1, 0, 0.0)], &mut events);
        assert!(matches!(
            events[..],
            [
                FlagEvent::Dropped { flag: 1, .. },
                FlagEvent::Returned { flag: 1, by: None }
            ]
        ));
        assert_eq!(flags.flag(1).state, FlagState::Home);
    }

    #[test]
    fn halftime_swaps_the_bases_and_sends_the_flags_home() {
        let mut flags = Flags::new([Vec3::ZERO, Vec3::new(50.0, 0.0, 0.0)]);
        assert!(!flags.swapped());
        // Blue takes red's flag, then halftime comes.
        step(&mut flags, &[player(1, 0, 50.0)]);
        assert_eq!(flags.carried_by(1), Some(1));
        flags.set_sides(true);
        assert!(flags.swapped());
        assert_eq!(flags.flag(0).home, Vec3::new(50.0, 0.0, 0.0));
        assert_eq!(flags.flag(1).home, Vec3::ZERO);
        assert_eq!(flags.carried_by(1), None);
        assert_eq!(flags.flag(1).state, FlagState::Home);
        assert_eq!(flags.flag(1).position, Vec3::ZERO);
        // Blue now captures at its new base, x = 50, with red's flag from x = 0.
        step(&mut flags, &[player(1, 0, 0.0)]);
        let events = step(&mut flags, &[player(1, 0, 50.0)]);
        assert!(events.contains(&FlagEvent::Captured { flag: 1, by: 1 }));
        // Overtime plays from the original bases.
        flags.set_sides(false);
        assert!(!flags.swapped());
        assert_eq!(flags.flag(0).home, Vec3::ZERO);
    }

    #[test]
    fn halftime_comes_once_at_half_the_clock() {
        assert!(!halftime_due(301.0, Some(600.0), true));
        assert!(halftime_due(300.0, Some(600.0), true));
        assert!(!halftime_due(100.0, Some(600.0), false));
        assert!(!halftime_due(0.0, None, true));
    }

    #[test]
    fn settle_moves_only_a_dropped_flag() {
        let mut flags = Flags::new([HOME_0, HOME_1]);
        flags.settle(1, Vec3::ONE);
        assert_eq!(flags.flag(1).position, HOME_1);
        step(&mut flags, &[player(1, 0, 50.0)]);
        let mut dead = player(1, 0, 5.0);
        dead.alive = false;
        dead.position.y = 1.2;
        step(&mut flags, &[dead]);
        flags.settle(1, Vec3::new(5.0, 0.0, 0.0));
        assert_eq!(flags.flag(1).position, Vec3::new(5.0, 0.0, 0.0));
        assert!(matches!(
            flags.flag(1).state,
            FlagState::Dropped { at, .. } if at == Vec3::new(5.0, 0.0, 0.0)
        ));
    }
}
