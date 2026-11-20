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
//! The attackers carry a charge ([`Charge`]): one of them starts each round
//! with it, a carrier who dies drops it, and any attacker who walks over it
//! picks it up. Held at a site for [`SndRules::plant_time`], it is planted,
//! and the round clock becomes its fuse ([`SndRules::fuse`]): it goes off
//! (the attackers win) unless a defender holds interact over it for
//! [`SndRules::defuse_time`] (the defenders win). Letting go, or dying,
//! starts a plant or a defuse over (no checkpoint). Once it is planted,
//! wiping out the attackers no longer wins the round: the defenders still
//! have to defuse.
//!
//! Each round won is a point for the match ([`MatchState`](crate::tdm::MatchState)
//! keeps the score); the first team to [`SndRules::rounds_to_win`] wins it. The
//! teams swap sides once, at halftime.
//!
//! The server runs this; clients only show what it sends
//! (`net::protocol::GameEvent::Round`).

use glam::Vec3;

use crate::score::Award;
use crate::tdm::{TEAM_COUNT, Team};

pub mod bots;

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
    /// Seconds of holding interact at a site to plant the charge.
    pub plant_time: f32,
    /// Seconds of holding interact over the planted charge to defuse it.
    pub defuse_time: f32,
    /// Seconds from the plant until the charge goes off.
    pub fuse: f32,
    /// How close (horizontally, in metres) an attacker must come to a
    /// dropped charge to pick it up.
    pub pickup_radius: f32,
    /// How close (horizontally) a defender must be to the planted charge to
    /// defuse it.
    pub defuse_radius: f32,
    /// Most height between a player's feet and the charge that still counts
    /// as being at it.
    pub reach_height: f32,
}

impl Default for SndRules {
    fn default() -> Self {
        Self {
            rounds_to_win: 6,
            freeze: 5.0,
            round_time: 105.0,
            round_end: 6.0,
            // Call of Duty arms in 5 s; this project chose 4 s and a 7 s
            // defuse (Call of Duty: 5 s, 7.5 s in competitive settings). The
            // 45 s fuse is Call of Duty's in every era. See
            // docs/search-and-destroy.md.
            plant_time: 4.0,
            defuse_time: 7.0,
            fuse: 45.0,
            pickup_radius: 1.2,
            defuse_radius: 1.6,
            reach_height: 1.6,
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
    /// The charge went off (the attackers win).
    Detonated,
    /// The charge was defused (the defenders win).
    Defused,
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
    /// The charge is planted: `time_left` is its fuse.
    pub planted: bool,
}

impl RoundState {
    /// Round one's freeze, with team 0 attacking.
    pub fn first(rules: &SndRules) -> Self {
        Self {
            number: 1,
            phase: RoundPhase::Freeze,
            time_left: rules.freeze,
            attackers: 0,
            planted: false,
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

    /// The charge was planted: the round clock becomes its fuse. Only in
    /// live play, and only once.
    pub fn plant(&mut self, rules: &SndRules) {
        if self.live() && !self.planted {
            self.planted = true;
            self.time_left = rules.fuse;
        }
    }

    /// The planted charge was defused: the defenders take the round.
    pub fn defuse(&mut self, rules: &SndRules) -> Option<RoundTransition> {
        (self.live() && self.planted).then(|| self.end(rules, self.defenders(), RoundEnd::Defused))
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
                // die in the same moment, the defenders take the round. Once
                // the charge is planted, wiping out the attackers is not
                // enough: it still has to be defused.
                if !self.planted && teams[usize::from(self.attackers)].wiped_out() {
                    return Some(self.end(rules, self.defenders(), RoundEnd::Eliminated));
                }
                if teams[usize::from(self.defenders())].wiped_out() {
                    return Some(self.end(rules, self.attackers, RoundEnd::Eliminated));
                }
                self.time_left -= dt;
                if self.time_left > 0.0 {
                    return None;
                }
                Some(if self.planted {
                    self.end(rules, self.attackers, RoundEnd::Detonated)
                } else {
                    self.end(rules, self.defenders(), RoundEnd::Time)
                })
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
                self.planted = false;
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

/// A bomb site: a box the carrier plants inside.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Site {
    pub center: Vec3,
    pub half_extents: Vec3,
}

impl Site {
    /// Whether a player standing at `feet` is inside the site.
    pub fn contains(&self, feet: Vec3) -> bool {
        let d = (feet + Vec3::Y * 0.1 - self.center).abs();
        d.x <= self.half_extents.x && d.y <= self.half_extents.y && d.z <= self.half_extents.z
    }
}

/// Where the charge is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChargeState {
    /// Carried by attacker `by`.
    Carried { by: u8 },
    /// Lying at `at`, for any attacker to pick up.
    Dropped { at: Vec3 },
    /// Planted at site `site`, standing at `at`, its fuse burning.
    Planted { site: u8, at: Vec3 },
    /// It went off.
    Detonated { site: u8, at: Vec3 },
    /// A defender defused it.
    Defused { site: u8, at: Vec3 },
}

/// What a player holding interact is doing to the charge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseKind {
    Plant,
    Defuse,
}

impl UseKind {
    /// Seconds this takes.
    pub fn duration(self, rules: &SndRules) -> f32 {
        match self {
            Self::Plant => rules.plant_time,
            Self::Defuse => rules.defuse_time,
        }
    }
}

/// A plant or defuse under way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChargeUse {
    pub by: u8,
    pub kind: UseKind,
    /// Seconds held so far.
    pub held: f32,
}

/// What the charge needs to know about a player.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChargePlayer {
    pub id: u8,
    pub team: Team,
    pub alive: bool,
    /// Feet position.
    pub position: Vec3,
    /// Holding the interact button.
    pub interact: bool,
}

/// The round around the charge: the rules, the map's sites, who attacks,
/// and whether play is live (nothing happens to the charge otherwise).
#[derive(Clone, Copy, Debug)]
pub struct ChargeContext<'a> {
    pub rules: &'a SndRules,
    pub sites: &'a [Site],
    pub attackers: Team,
    pub live: bool,
}

/// Something that happened to the charge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChargeEvent {
    PickedUp {
        by: u8,
    },
    Dropped {
        by: u8,
        at: Vec3,
    },
    /// `by` started planting or defusing.
    Started {
        by: u8,
        kind: UseKind,
    },
    /// `by` let go, moved off, or died before finishing.
    Stopped {
        by: u8,
        kind: UseKind,
    },
    Planted {
        by: u8,
        site: u8,
        at: Vec3,
    },
    Defused {
        by: u8,
        site: u8,
        at: Vec3,
    },
}

/// The charge, for one round. The server runs it; clients only show what it
/// sends (`net::protocol::GameEvent::Charge`) and hold their own player still
/// while [`Charge::spot`] says interact would plant or defuse, as the server
/// does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Charge {
    pub state: ChargeState,
    /// The plant or defuse under way, if any. One at a time: a second
    /// defender holding interact waits.
    pub using: Option<ChargeUse>,
    /// Where it is now: its carrier's feet, or where it lies or stands.
    pub position: Vec3,
}

impl Charge {
    /// A round's charge: carried by `carrier`, or lying at `fallback` (the
    /// attackers' side) when nobody can take it.
    pub fn new(carrier: Option<(u8, Vec3)>, fallback: Vec3) -> Self {
        match carrier {
            Some((by, at)) => Self {
                state: ChargeState::Carried { by },
                using: None,
                position: at,
            },
            None => Self {
                state: ChargeState::Dropped { at: fallback },
                using: None,
                position: fallback,
            },
        }
    }

    /// Who carries it, if anyone.
    pub fn carrier(&self) -> Option<u8> {
        match self.state {
            ChargeState::Carried { by } => Some(by),
            _ => None,
        }
    }

    /// The site it is planted at and where, while its fuse burns.
    pub fn planted(&self) -> Option<(u8, Vec3)> {
        match self.state {
            ChargeState::Planted { site, at } => Some((site, at)),
            _ => None,
        }
    }

    /// Who is planting or defusing it, held still meanwhile
    /// ([`crate::player::PlayerInput::using_charge`]).
    pub fn user(&self) -> Option<u8> {
        self.using.map(|u| u.by)
    }

    /// What holding interact would do for `p` right now: plant (carrying it,
    /// inside a site), defuse (a defender at the planted charge, while nobody
    /// else is defusing), or nothing.
    pub fn spot(&self, cx: &ChargeContext, p: &ChargePlayer) -> Option<UseKind> {
        if !cx.live || !p.alive {
            return None;
        }
        match self.state {
            ChargeState::Carried { by } if by == p.id && p.team == cx.attackers => cx
                .sites
                .iter()
                .any(|site| site.contains(p.position))
                .then_some(UseKind::Plant),
            ChargeState::Planted { at, .. }
                if p.team != cx.attackers
                    && self.using.is_none_or(|u| u.by == p.id)
                    && reaches(
                        p.position,
                        at,
                        cx.rules.defuse_radius,
                        cx.rules.reach_height,
                    ) =>
            {
                Some(UseKind::Defuse)
            }
            _ => None,
        }
    }

    /// Lies it down at `at` (the floor under where it fell).
    pub fn settle(&mut self, at: Vec3) {
        if let ChargeState::Dropped { .. } = self.state {
            self.state = ChargeState::Dropped { at };
            self.position = at;
        }
    }

    /// The fuse ran out.
    pub fn detonate(&mut self) {
        if let ChargeState::Planted { site, at } = self.state {
            self.state = ChargeState::Detonated { site, at };
            self.using = None;
        }
    }

    /// Advances drops, pickups, and plants or defuses by `dt` seconds.
    /// `players` are everyone in the game; a carrier missing from it has
    /// left, and drops the charge where it last was.
    pub fn tick(
        &mut self,
        cx: &ChargeContext,
        dt: f32,
        players: &[ChargePlayer],
        events: &mut Vec<ChargeEvent>,
    ) {
        let find = |id: u8| players.iter().find(|p| p.id == id);
        // A carrier who dies or leaves drops it.
        if let ChargeState::Carried { by } = self.state {
            match find(by) {
                Some(p) if p.alive => self.position = p.position,
                found => {
                    let at = found.map_or(self.position, |p| p.position);
                    self.state = ChargeState::Dropped { at };
                    self.position = at;
                    events.push(ChargeEvent::Dropped { by, at });
                }
            }
        }
        if !cx.live {
            if let Some(u) = self.using.take() {
                events.push(ChargeEvent::Stopped {
                    by: u.by,
                    kind: u.kind,
                });
            }
            return;
        }
        // Any living attacker who walks over it picks it up: the nearest.
        if let ChargeState::Dropped { at } = self.state
            && let Some(p) = players
                .iter()
                .filter(|p| {
                    p.alive
                        && p.team == cx.attackers
                        && reaches(
                            p.position,
                            at,
                            cx.rules.pickup_radius,
                            cx.rules.reach_height,
                        )
                })
                .min_by(|a, b| flat(a.position - at).total_cmp(&flat(b.position - at)))
        {
            self.state = ChargeState::Carried { by: p.id };
            self.position = p.position;
            events.push(ChargeEvent::PickedUp { by: p.id });
        }
        // A plant or defuse keeps going while its player holds interact
        // where it counts; anything else starts it over.
        if let Some(u) = self.using {
            let held = find(u.by).is_some_and(|p| p.interact && self.spot(cx, p) == Some(u.kind));
            if !held {
                self.using = None;
                events.push(ChargeEvent::Stopped {
                    by: u.by,
                    kind: u.kind,
                });
            }
        }
        if self.using.is_none()
            && let Some((p, kind)) = players
                .iter()
                .filter(|p| p.interact)
                .find_map(|p| self.spot(cx, p).map(|kind| (p, kind)))
        {
            self.using = Some(ChargeUse {
                by: p.id,
                kind,
                held: 0.0,
            });
            events.push(ChargeEvent::Started { by: p.id, kind });
        }
        let Some(mut u) = self.using else {
            return;
        };
        u.held += dt;
        self.using = Some(u);
        // A little slack, so a whole number of ticks finishes on time.
        if u.held + 1e-4 < u.kind.duration(cx.rules) {
            return;
        }
        self.using = None;
        let Some(p) = find(u.by) else {
            return;
        };
        match (u.kind, self.state) {
            (UseKind::Plant, ChargeState::Carried { .. }) => {
                let site = cx
                    .sites
                    .iter()
                    .position(|s| s.contains(p.position))
                    .unwrap_or(0) as u8;
                let at = p.position;
                self.state = ChargeState::Planted { site, at };
                self.position = at;
                events.push(ChargeEvent::Planted { by: u.by, site, at });
            }
            (UseKind::Defuse, ChargeState::Planted { site, at }) => {
                self.state = ChargeState::Defused { site, at };
                events.push(ChargeEvent::Defused { by: u.by, site, at });
            }
            _ => {}
        }
    }
}

/// What happened in a round, for its medals: who started it, who killed
/// whom, and who was left alone alive on a team (the server keeps it).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoundRecord {
    /// Everyone alive when the round went live, with their team.
    pub roster: Vec<(u8, Team)>,
    /// Kills this round: (killer, victim).
    pub kills: Vec<(u8, u8)>,
    /// Per team: the player who was left the last one alive while an enemy
    /// still lived, if that happened.
    pub last_alive: [Option<u8>; TEAM_COUNT],
}

/// Enemies an Ace needs: killing a one- or two-player team alone is common.
pub const ACE_MIN_ENEMIES: usize = 3;

impl RoundRecord {
    /// A record for a round going live with `roster` (living players and
    /// their teams).
    pub fn start(roster: Vec<(u8, Team)>) -> Self {
        Self {
            roster,
            ..Self::default()
        }
    }

    pub fn on_kill(&mut self, killer: u8, victim: u8) {
        if killer != victim {
            self.kills.push((killer, victim));
        }
    }

    /// Notes who is alive (`alive`: everyone's id, team, and whether they
    /// live): a team down to one player while an enemy lives has its last
    /// one standing, if the team started the round with more than one.
    pub fn on_headcount(&mut self, alive: &[(u8, Team, bool)]) {
        for team in 0..TEAM_COUNT as Team {
            if self.last_alive[usize::from(team)].is_some() {
                continue;
            }
            let started = self.roster.iter().filter(|(_, t)| *t == team).count();
            let mut living = alive.iter().filter(|(_, t, a)| *t == team && *a);
            let enemies = alive.iter().filter(|(_, t, a)| *t != team && *a).count();
            if let (Some((id, _, _)), None) = (living.next(), living.next())
                && started >= 2
                && enemies >= 1
            {
                self.last_alive[usize::from(team)] = Some(*id);
            }
        }
    }

    /// The round's medals once `winner` takes it: Clutch for the winning
    /// team's last one standing, and Ace for a player who killed every enemy
    /// who started the round (at least [`ACE_MIN_ENEMIES`]).
    pub fn medals(&self, winner: Team) -> Vec<(u8, Award)> {
        let mut out = Vec::new();
        if let Some(id) = self.last_alive[usize::from(winner)] {
            out.push((id, Award::Clutch));
        }
        for &(id, team) in &self.roster {
            let enemies: Vec<u8> = self
                .roster
                .iter()
                .filter(|(_, t)| *t != team)
                .map(|(e, _)| *e)
                .collect();
            if enemies.len() >= ACE_MIN_ENEMIES
                && enemies.iter().all(|e| self.kills.contains(&(id, *e)))
            {
                out.push((id, Award::Ace));
            }
        }
        out
    }
}

/// Horizontal distance squared.
fn flat(v: Vec3) -> f32 {
    v.x * v.x + v.z * v.z
}

/// Whether feet at `feet` are within `radius` (horizontally) and `height` of
/// `at`.
fn reaches(feet: Vec3, at: Vec3, radius: f32, height: f32) -> bool {
    flat(feet - at) <= radius * radius && (feet.y - at.y).abs() <= height
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
            ..SndRules::default()
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
    #[test]
    fn a_planted_charge_turns_the_clock_into_its_fuse() {
        let rules = rules();
        let mut state = RoundState::first(&rules);
        run(&mut state, 1.05, FULL);
        state.plant(&rules);
        assert!(state.planted);
        assert_eq!(state.time_left, rules.fuse);
        // Wiping out the attackers no longer wins: the charge still burns.
        assert!(run(&mut state, 1.0, [team(3, 0), team(3, 2)]).is_empty());
        // It goes off when the fuse runs out, for the attackers.
        let out = run(&mut state, rules.fuse, [team(3, 0), team(3, 2)]);
        assert_eq!(
            out,
            vec![RoundTransition::Over {
                winner: 0,
                reason: RoundEnd::Detonated
            }]
        );
    }

    #[test]
    fn a_defuse_or_a_wiped_out_defence_settles_a_planted_round() {
        let rules = rules();
        let mut state = RoundState::first(&rules);
        run(&mut state, 1.05, FULL);
        assert_eq!(state.defuse(&rules), None, "nothing to defuse yet");
        state.plant(&rules);
        assert_eq!(
            state.defuse(&rules),
            Some(RoundTransition::Over {
                winner: 1,
                reason: RoundEnd::Defused
            })
        );

        let mut state = RoundState::first(&rules);
        run(&mut state, 1.05, FULL);
        state.plant(&rules);
        assert_eq!(
            state.tick(&rules, 0.1, [team(3, 1), team(3, 0)]),
            Some(RoundTransition::Over {
                winner: 0,
                reason: RoundEnd::Eliminated
            })
        );
        // The next round starts unplanted.
        run(&mut state, 2.05, FULL);
        assert!(!state.planted);
    }

    #[test]
    fn no_plant_during_the_freeze() {
        let rules = rules();
        let mut state = RoundState::first(&rules);
        state.plant(&rules);
        assert!(!state.planted);
    }

    // --- The charge -------------------------------------------------------------

    const TICK: f32 = 1.0 / 120.0;
    const SITE_A: Site = Site {
        center: Vec3::new(10.0, 1.5, 0.0),
        half_extents: Vec3::new(2.0, 1.5, 2.0),
    };
    const SITE_B: Site = Site {
        center: Vec3::new(-10.0, 1.5, 0.0),
        half_extents: Vec3::new(2.0, 1.5, 2.0),
    };
    const SITES: [Site; 2] = [SITE_A, SITE_B];

    fn cx(rules: &SndRules) -> ChargeContext<'_> {
        ChargeContext {
            rules,
            sites: &SITES,
            attackers: 0,
            live: true,
        }
    }

    fn player(id: u8, team: Team, x: f32, interact: bool) -> ChargePlayer {
        ChargePlayer {
            id,
            team,
            alive: true,
            position: Vec3::new(x, 0.0, 0.0),
            interact,
        }
    }

    /// Ticks the charge for `seconds`, returning its events.
    fn hold(
        charge: &mut Charge,
        cx: &ChargeContext,
        seconds: f32,
        players: &[ChargePlayer],
    ) -> Vec<ChargeEvent> {
        let mut events = Vec::new();
        for _ in 0..(seconds / TICK).round() as u32 {
            charge.tick(cx, TICK, players, &mut events);
        }
        events
    }

    fn carried_by(id: u8, x: f32) -> Charge {
        Charge::new(Some((id, Vec3::new(x, 0.0, 0.0))), Vec3::ZERO)
    }

    #[test]
    fn the_carrier_plants_by_holding_interact_at_a_site_for_the_plant_time() {
        let rules = SndRules::default();
        let cx = cx(&rules);
        let mut charge = carried_by(1, 10.0);
        let planter = player(1, 0, 10.0, true);
        assert_eq!(charge.spot(&cx, &planter), Some(UseKind::Plant));
        let events = hold(&mut charge, &cx, rules.plant_time - 0.1, &[planter]);
        assert_eq!(
            events,
            vec![ChargeEvent::Started {
                by: 1,
                kind: UseKind::Plant
            }]
        );
        assert_eq!(charge.user(), Some(1));
        assert!(charge.planted().is_none());
        let events = hold(&mut charge, &cx, 0.1, &[planter]);
        assert_eq!(
            events,
            vec![ChargeEvent::Planted {
                by: 1,
                site: 0,
                at: Vec3::new(10.0, 0.0, 0.0)
            }]
        );
        assert_eq!(charge.planted(), Some((0, Vec3::new(10.0, 0.0, 0.0))));
        assert_eq!(charge.user(), None);
    }

    #[test]
    fn the_plant_takes_exactly_its_ticks() {
        let rules = SndRules::default();
        let cx = cx(&rules);
        let mut charge = carried_by(1, -10.0);
        let planter = player(1, 0, -10.0, true);
        let mut events = Vec::new();
        let ticks = (rules.plant_time * 120.0).round() as u32;
        for _ in 0..ticks - 1 {
            charge.tick(&cx, TICK, &[planter], &mut events);
        }
        assert!(charge.planted().is_none());
        charge.tick(&cx, TICK, &[planter], &mut events);
        assert_eq!(charge.planted().map(|(site, _)| site), Some(1));
    }

    #[test]
    fn letting_go_starts_a_plant_over_and_nothing_plants_outside_a_site() {
        let rules = SndRules::default();
        let cx = cx(&rules);
        let mut charge = carried_by(1, 10.0);
        hold(&mut charge, &cx, 3.0, &[player(1, 0, 10.0, true)]);
        let events = hold(&mut charge, &cx, 0.1, &[player(1, 0, 10.0, false)]);
        assert_eq!(
            events,
            vec![ChargeEvent::Stopped {
                by: 1,
                kind: UseKind::Plant
            }]
        );
        hold(&mut charge, &cx, 3.0, &[player(1, 0, 10.0, true)]);
        assert!(
            charge.planted().is_none(),
            "the earlier hold does not count"
        );

        let mut charge = carried_by(1, 0.0);
        let outside = player(1, 0, 0.0, true);
        assert_eq!(charge.spot(&cx, &outside), None);
        assert!(hold(&mut charge, &cx, 5.0, &[outside]).is_empty());
        assert!(charge.planted().is_none());
    }

    #[test]
    fn only_the_carrier_plants_and_only_in_live_play() {
        let rules = SndRules::default();
        let mut charge = carried_by(1, 10.0);
        let teammate = player(2, 0, 10.0, true);
        assert_eq!(charge.spot(&cx(&rules), &teammate), None);
        let frozen = ChargeContext {
            live: false,
            ..cx(&rules)
        };
        assert_eq!(charge.spot(&frozen, &player(1, 0, 10.0, true)), None);
        hold(&mut charge, &frozen, 5.0, &[player(1, 0, 10.0, true)]);
        assert_eq!(charge.carrier(), Some(1));
    }

    #[test]
    fn a_dead_carrier_drops_it_and_only_attackers_pick_it_up() {
        let rules = SndRules::default();
        let cx = cx(&rules);
        let mut charge = carried_by(1, 3.0);
        let mut carrier = player(1, 0, 3.0, false);
        hold(&mut charge, &cx, 0.1, &[carrier]);
        carrier.alive = false;
        let events = hold(&mut charge, &cx, TICK, &[carrier]);
        let at = Vec3::new(3.0, 0.0, 0.0);
        assert_eq!(events, vec![ChargeEvent::Dropped { by: 1, at }]);
        assert_eq!(charge.state, ChargeState::Dropped { at });
        // A defender walks over it and nothing happens.
        assert!(hold(&mut charge, &cx, 0.5, &[carrier, player(5, 1, 3.0, false)]).is_empty());
        // An attacker does, and takes it.
        let events = hold(&mut charge, &cx, TICK, &[carrier, player(2, 0, 3.5, false)]);
        assert_eq!(events, vec![ChargeEvent::PickedUp { by: 2 }]);
        assert_eq!(charge.carrier(), Some(2));
    }

    #[test]
    fn a_carrier_who_leaves_drops_it_where_it_was() {
        let rules = SndRules::default();
        let cx = cx(&rules);
        let mut charge = carried_by(1, 3.0);
        hold(&mut charge, &cx, 0.1, &[player(1, 0, 4.0, false)]);
        let events = hold(&mut charge, &cx, TICK, &[]);
        assert_eq!(
            events,
            vec![ChargeEvent::Dropped {
                by: 1,
                at: Vec3::new(4.0, 0.0, 0.0)
            }]
        );
    }

    #[test]
    fn a_carrier_killed_mid_plant_drops_it_and_the_plant_stops() {
        let rules = SndRules::default();
        let cx = cx(&rules);
        let mut charge = carried_by(1, 10.0);
        let mut planter = player(1, 0, 10.0, true);
        hold(&mut charge, &cx, 2.0, &[planter]);
        planter.alive = false;
        let events = hold(&mut charge, &cx, TICK, &[planter]);
        assert_eq!(
            events,
            vec![
                ChargeEvent::Dropped {
                    by: 1,
                    at: Vec3::new(10.0, 0.0, 0.0)
                },
                ChargeEvent::Stopped {
                    by: 1,
                    kind: UseKind::Plant
                }
            ]
        );
        assert!(charge.planted().is_none());
    }

    fn planted_at_a() -> Charge {
        let rules = SndRules::default();
        let mut charge = carried_by(1, 10.0);
        hold(
            &mut charge,
            &cx(&rules),
            rules.plant_time,
            &[player(1, 0, 10.0, true)],
        );
        assert!(charge.planted().is_some());
        charge
    }

    #[test]
    fn a_defender_defuses_by_holding_interact_at_the_charge_for_the_defuse_time() {
        let rules = SndRules::default();
        let cx = cx(&rules);
        let mut charge = planted_at_a();
        let attacker = player(1, 0, 10.0, true);
        assert_eq!(charge.spot(&cx, &attacker), None, "attackers cannot defuse");
        let far = player(5, 1, 12.0, true);
        assert_eq!(charge.spot(&cx, &far), None, "out of reach");
        let defuser = player(5, 1, 11.0, true);
        let events = hold(&mut charge, &cx, rules.defuse_time - 0.1, &[defuser]);
        assert_eq!(
            events,
            vec![ChargeEvent::Started {
                by: 5,
                kind: UseKind::Defuse
            }]
        );
        let events = hold(&mut charge, &cx, 0.1, &[defuser]);
        assert_eq!(
            events,
            vec![ChargeEvent::Defused {
                by: 5,
                site: 0,
                at: Vec3::new(10.0, 0.0, 0.0)
            }]
        );
        assert!(matches!(charge.state, ChargeState::Defused { site: 0, .. }));
    }

    #[test]
    fn an_interrupted_defuse_starts_over_with_no_checkpoint() {
        let rules = SndRules::default();
        let cx = cx(&rules);
        let mut charge = planted_at_a();
        hold(&mut charge, &cx, 6.5, &[player(5, 1, 11.0, true)]);
        let events = hold(&mut charge, &cx, TICK, &[player(5, 1, 11.0, false)]);
        assert_eq!(
            events,
            vec![ChargeEvent::Stopped {
                by: 5,
                kind: UseKind::Defuse
            }]
        );
        hold(&mut charge, &cx, 6.5, &[player(5, 1, 11.0, true)]);
        assert!(charge.planted().is_some(), "6.5 s again is still short");
        hold(&mut charge, &cx, 0.5, &[player(5, 1, 11.0, true)]);
        assert!(matches!(charge.state, ChargeState::Defused { .. }));
    }

    #[test]
    fn one_defender_defuses_at_a_time() {
        let rules = SndRules::default();
        let cx = cx(&rules);
        let mut charge = planted_at_a();
        let first = player(5, 1, 11.0, true);
        let second = player(6, 1, 9.0, true);
        hold(&mut charge, &cx, 1.0, &[first, second]);
        assert_eq!(charge.user(), Some(5));
        assert_eq!(charge.spot(&cx, &second), None, "the second waits");
        // The first lets go: the second takes over from nothing.
        hold(&mut charge, &cx, TICK, &[player(5, 1, 11.0, false), second]);
        assert_eq!(charge.user(), Some(6));
        assert!(charge.using.is_some_and(|u| u.held <= TICK * 1.5));
    }

    #[test]
    fn a_planted_charge_detonates_once() {
        let mut charge = planted_at_a();
        charge.detonate();
        assert!(matches!(
            charge.state,
            ChargeState::Detonated { site: 0, .. }
        ));
        let before = charge;
        charge.detonate();
        assert_eq!(charge, before);
    }

    #[test]
    fn nobody_to_carry_it_leaves_it_on_the_attackers_side() {
        let charge = Charge::new(None, Vec3::new(-30.0, 0.0, 0.0));
        assert_eq!(
            charge.state,
            ChargeState::Dropped {
                at: Vec3::new(-30.0, 0.0, 0.0)
            }
        );
    }
    // --- Round medals -----------------------------------------------------------

    fn roster() -> Vec<(u8, Team)> {
        vec![(1, 0), (2, 0), (3, 0), (11, 1), (12, 1), (13, 1)]
    }

    #[test]
    fn the_last_one_standing_who_wins_gets_a_clutch() {
        let mut record = RoundRecord::start(roster());
        record.on_headcount(&[
            (1, 0, true),
            (2, 0, true),
            (3, 0, false),
            (11, 1, true),
            (12, 1, true),
            (13, 1, true),
        ]);
        assert_eq!(record.last_alive, [None, None]);
        record.on_headcount(&[
            (1, 0, true),
            (2, 0, false),
            (3, 0, false),
            (11, 1, true),
            (12, 1, true),
            (13, 1, false),
        ]);
        assert_eq!(record.last_alive, [Some(1), None]);
        assert_eq!(record.medals(0), vec![(1, Award::Clutch)]);
        assert!(record.medals(1).is_empty(), "no clutch for a round lost");
    }

    #[test]
    fn a_team_of_one_never_clutches_and_nobody_clutches_without_an_enemy_left() {
        let mut record = RoundRecord::start(vec![(1, 0), (11, 1), (12, 1)]);
        record.on_headcount(&[(1, 0, true), (11, 1, true), (12, 1, true)]);
        assert_eq!(record.last_alive[0], None);
        let mut record = RoundRecord::start(roster());
        record.on_headcount(&[
            (1, 0, true),
            (2, 0, false),
            (3, 0, false),
            (11, 1, false),
            (12, 1, false),
            (13, 1, false),
        ]);
        assert_eq!(record.last_alive[0], None);
    }

    #[test]
    fn killing_the_whole_enemy_team_alone_is_an_ace() {
        let mut record = RoundRecord::start(roster());
        for victim in [11, 12, 13] {
            record.on_kill(1, victim);
        }
        assert_eq!(record.medals(0), vec![(1, Award::Ace)]);
        // One of them killed by someone else: no ace.
        let mut record = RoundRecord::start(roster());
        record.on_kill(1, 11);
        record.on_kill(1, 12);
        record.on_kill(2, 13);
        assert!(record.medals(0).is_empty());
        // Two enemies are not enough.
        let mut record = RoundRecord::start(vec![(1, 0), (11, 1), (12, 1)]);
        record.on_kill(1, 11);
        record.on_kill(1, 12);
        assert!(record.medals(0).is_empty());
    }
}
