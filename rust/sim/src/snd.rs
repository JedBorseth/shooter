//! Search and Destroy: rounds with one life each.
//!
//! Inside a match's [`Phase::Playing`](crate::tdm::Phase), the server plays
//! rounds. One team attacks and the other defends; each round goes:
//!
//! - [`RoundPhase::Freeze`]: everyone is back at their side with full health.
//!   They can look around and change their loadout, but cannot move or fire.
//! - [`RoundPhase::Live`]: nobody respawns. The round ends when a team is
//!   wiped out, or when the clock runs out, which the defenders win.
//! - [`RoundPhase::Over`]: the result, then the next round's freeze.
//!
//! Each round won is a point for the match ([`MatchState`](crate::tdm::MatchState)
//! keeps the score); the first team to [`SndRules::rounds_to_win`] wins it. The
//! teams swap sides once, at halftime.
//!
//! The server runs this; clients only show what it sends
//! (`net::protocol::GameEvent::Round`).

use crate::tdm::{TEAM_COUNT, Team};

/// Tunable round rules.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SndRules {
    /// Rounds a team needs to win the match.
    pub rounds_to_win: u32,
    /// Seconds of freeze before each round.
    pub freeze: f32,
    /// Seconds of play in a round before the defenders win it.
    pub round_time: f32,
    /// Seconds the round's result stays up before the next freeze.
    pub round_end: f32,
}

impl Default for SndRules {
    fn default() -> Self {
        Self {
            rounds_to_win: 6,
            freeze: 5.0,
            round_time: 105.0,
            round_end: 6.0,
        }
    }
}

impl SndRules {
    /// Rounds played before the teams swap sides: one short of the rounds to
    /// win, so a 6-round match swaps after round 5 and both teams attack
    /// five times before a decider.
    pub fn halftime_after(&self) -> u32 {
        self.rounds_to_win.saturating_sub(1).max(1)
    }
}

/// How a round was won.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoundEnd {
    /// The losing team was wiped out.
    Eliminated,
    /// The clock ran out on the attackers.
    Time,
}

/// Where a round is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoundPhase {
    Freeze,
    Live,
    Over { winner: Team, reason: RoundEnd },
}

/// What changed in one [`RoundState::tick`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoundTransition {
    /// The freeze is over.
    Live,
    /// The round is decided: score it for `winner`.
    Over { winner: Team, reason: RoundEnd },
    /// The next round begins with its freeze: put everyone back at their
    /// side. `swapped` when the teams just changed sides (halftime).
    Next { swapped: bool },
}

/// Players on one team, and how many of them are alive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Headcount {
    pub players: u32,
    pub alive: u32,
}

impl Headcount {
    /// Everyone on the team is dead. A team with nobody on it is never wiped
    /// out, so a round with one side empty runs to the clock.
    pub fn wiped_out(self) -> bool {
        self.players > 0 && self.alive == 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundState {
    /// The current round, from 1.
    pub number: u32,
    pub phase: RoundPhase,
    /// Seconds left in the current phase.
    pub time_left: f32,
    /// The team attacking this round.
    pub attackers: Team,
}

impl RoundState {
    /// Round one's freeze, with team 0 attacking.
    pub fn first(rules: &SndRules) -> Self {
        Self {
            number: 1,
            phase: RoundPhase::Freeze,
            time_left: rules.freeze,
            attackers: 0,
        }
    }

    pub fn defenders(&self) -> Team {
        1 - self.attackers
    }

    /// The side of the map `team` plays from this round: 0 for the attackers'
    /// side, 1 for the defenders'.
    pub fn side_of(&self, team: Team) -> Team {
        u8::from(team != self.attackers)
    }

    /// Players may move and fire (not during the freeze).
    pub fn can_move(&self) -> bool {
        self.phase != RoundPhase::Freeze
    }

    /// Shots hurt only while the round is live.
    pub fn live(&self) -> bool {
        self.phase == RoundPhase::Live
    }

    /// Advances the clock and checks whether a team has been wiped out.
    pub fn tick(
        &mut self,
        rules: &SndRules,
        dt: f32,
        teams: [Headcount; TEAM_COUNT],
    ) -> Option<RoundTransition> {
        match self.phase {
            RoundPhase::Freeze => {
                self.time_left -= dt;
                if self.time_left > 0.0 {
                    return None;
                }
                self.phase = RoundPhase::Live;
                self.time_left = rules.round_time;
                Some(RoundTransition::Live)
            }
            RoundPhase::Live => {
                // The attackers must make something happen: if both teams
                // die in the same moment, the defenders take the round.
                if teams[usize::from(self.attackers)].wiped_out() {
                    return Some(self.end(rules, self.defenders(), RoundEnd::Eliminated));
                }
                if teams[usize::from(self.defenders())].wiped_out() {
                    return Some(self.end(rules, self.attackers, RoundEnd::Eliminated));
                }
                self.time_left -= dt;
                if self.time_left > 0.0 {
                    return None;
                }
                Some(self.end(rules, self.defenders(), RoundEnd::Time))
            }
            RoundPhase::Over { .. } => {
                self.time_left -= dt;
                if self.time_left > 0.0 {
                    return None;
                }
                let swapped = self.number == rules.halftime_after();
                if swapped {
                    self.attackers = self.defenders();
                }
                self.number += 1;
                self.phase = RoundPhase::Freeze;
                self.time_left = rules.freeze;
                Some(RoundTransition::Next { swapped })
            }
        }
    }

    fn end(&mut self, rules: &SndRules, winner: Team, reason: RoundEnd) -> RoundTransition {
        self.phase = RoundPhase::Over { winner, reason };
        self.time_left = rules.round_end;
        RoundTransition::Over { winner, reason }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> SndRules {
        SndRules {
            rounds_to_win: 3,
            freeze: 1.0,
            round_time: 10.0,
            round_end: 2.0,
        }
    }

    fn team(players: u32, alive: u32) -> Headcount {
        Headcount { players, alive }
    }

    const FULL: [Headcount; 2] = [
        Headcount {
            players: 3,
            alive: 3,
        },
        Headcount {
            players: 3,
            alive: 3,
        },
    ];

    fn run(state: &mut RoundState, seconds: f32, teams: [Headcount; 2]) -> Vec<RoundTransition> {
        let mut out = Vec::new();
        for _ in 0..(seconds * 10.0).round() as u32 {
            out.extend(state.tick(&rules(), 0.1, teams));
        }
        out
    }

    #[test]
    fn a_round_freezes_then_goes_live() {
        let mut state = RoundState::first(&rules());
        assert!(!state.can_move());
        assert!(!state.live());
        assert_eq!(run(&mut state, 1.05, FULL), vec![RoundTransition::Live]);
        assert!(state.can_move());
        assert!(state.live());
        assert!(state.time_left > 9.8 && state.time_left <= 10.0);
    }

    #[test]
    fn wiping_out_a_team_wins_the_round() {
        let mut state = RoundState::first(&rules());
        run(&mut state, 1.05, FULL);
        let over = state.tick(&rules(), 0.1, [team(3, 1), team(3, 0)]);
        assert_eq!(
            over,
            Some(RoundTransition::Over {
                winner: 0,
                reason: RoundEnd::Eliminated
            })
        );
        assert!(!state.live());
        // Nothing more happens to a decided round, whatever the headcount.
        assert!(run(&mut state, 1.0, [team(3, 0), team(3, 0)]).is_empty());
    }

    #[test]
    fn the_defenders_win_on_time_and_on_a_double_wipe() {
        let mut state = RoundState::first(&rules());
        run(&mut state, 1.05, FULL);
        assert_eq!(
            run(&mut state, 10.05, FULL),
            vec![RoundTransition::Over {
                winner: 1,
                reason: RoundEnd::Time
            }]
        );

        let mut state = RoundState::first(&rules());
        run(&mut state, 1.05, FULL);
        assert_eq!(
            state.tick(&rules(), 0.1, [team(2, 0), team(2, 0)]),
            Some(RoundTransition::Over {
                winner: 1,
                reason: RoundEnd::Eliminated
            })
        );
    }

    #[test]
    fn an_empty_team_is_never_wiped_out() {
        let mut state = RoundState::first(&rules());
        run(&mut state, 1.05, FULL);
        assert!(run(&mut state, 5.0, [team(2, 2), team(0, 0)]).is_empty());
    }

    #[test]
    fn rounds_follow_each_other_and_swap_sides_at_halftime() {
        let rules = rules();
        assert_eq!(rules.halftime_after(), 2);
        let mut state = RoundState::first(&rules);
        assert_eq!(state.side_of(0), 0);
        assert_eq!(state.side_of(1), 1);
        let mut swaps = Vec::new();
        for _ in 0..3 {
            run(&mut state, 1.05, FULL);
            state.tick(&rules, 0.1, [team(3, 0), team(3, 3)]);
            for transition in run(&mut state, 2.05, FULL) {
                if let RoundTransition::Next { swapped } = transition {
                    swaps.push((state.number, swapped));
                }
            }
        }
        assert_eq!(swaps, vec![(2, false), (3, true), (4, false)]);
        assert_eq!(state.attackers, 1);
        assert_eq!(state.side_of(1), 0);
        assert_eq!(state.phase, RoundPhase::Freeze);
    }

    #[test]
    fn halftime_is_never_round_zero() {
        let rules = SndRules {
            rounds_to_win: 1,
            ..SndRules::default()
        };
        assert_eq!(rules.halftime_after(), 1);
    }
}
