//! Domination: two teams fight over three flags, A, B, and C.
//!
//! Played the way Black Ops 2 plays it (`dom.gsc`), with no twists:
//!
//! - A flag is captured by standing in its circle for
//!   [`DominationRules::capture_seconds`] (10 s). Every extra teammate in the
//!   circle adds one more person's worth of speed, so two take 5 s and three
//!   about 3.3 s (the scripts' use rate is the number of claimants).
//! - Taking an enemy flag takes as long as taking a neutral one: there is no
//!   separate neutralise step (that arrived in later games).
//! - With both teams in the circle the flag is contested: the capture stalls
//!   and keeps its progress until one side is left. If the capturing team
//!   walks away, or the owners step back on, the capture starts over.
//! - Every [`DominationRules::point_seconds`] (5 s) each team scores a point
//!   for every flag it owns. The match runs on [`crate::tdm::MatchState`]:
//!   200 points wins, or the team ahead when the clock runs out.
//! - The match is two halves: when half the clock has run, the teams swap
//!   sides, every flag goes back to neutral, and everyone respawns at their
//!   new side; the score carries over ([`halftime_due`]).
//!
//! Players earn points for each capture they stood in for
//! ([`Award::DominationCapture`]), and for kills on or near a flag
//! ([`Award::DominationDefend`] when their team owns it,
//! [`Award::DominationAssault`] otherwise).
//!
//! The server runs this; clients only draw what it sends
//! (`net::protocol::GameEvent::Domination`).

use glam::Vec3;

use crate::score::Award;
use crate::tdm::{TEAM_COUNT, Team, Threat};

/// Most flags a map may have. Domination uses three; the cap only bounds
/// what the wire carries.
pub const MAX_FLAGS: usize = 5;

/// One flag: a circle on the ground that players capture by standing in it.
#[derive(Clone, Debug, PartialEq)]
pub struct FlagPoint {
    /// Its letter ("A").
    pub name: String,
    /// The middle of the circle, `half_height` above its floor.
    pub center: Vec3,
    pub radius: f32,
    pub half_height: f32,
}

impl FlagPoint {
    /// A flag of the usual size with its floor at `floor`.
    pub fn new(name: &str, floor: Vec3) -> Self {
        Self {
            name: name.to_owned(),
            center: floor + Vec3::Y * DEFAULT_HALF_HEIGHT,
            radius: DEFAULT_RADIUS,
            half_height: DEFAULT_HALF_HEIGHT,
        }
    }

    /// Whether feet at `feet` stand in the circle: on its floor (or a
    /// little below, for uneven ground) up to its top.
    pub fn contains(&self, feet: Vec3) -> bool {
        let d = feet - self.center;
        let height = feet.y - self.floor();
        (-FLOOR_SLACK..=self.half_height * 2.0).contains(&height)
            && d.x * d.x + d.z * d.z <= self.radius * self.radius
    }

    /// Whether `feet` are within `margin` metres of the circle across the
    /// ground, at a height that could be in it.
    pub fn near(&self, feet: Vec3, margin: f32) -> bool {
        let d = feet - self.center;
        let reach = self.radius + margin;
        d.y.abs() <= self.half_height + margin.min(2.0) && d.x * d.x + d.z * d.z <= reach * reach
    }

    /// Height of the circle's floor.
    pub fn floor(&self) -> f32 {
        self.center.y - self.half_height
    }
}

/// Radius of a flag's circle, in metres: Black Ops 2's flag triggers are
/// about 120 to 160 units (3 to 4 m) across the ground; this sits in the
/// middle.
pub const DEFAULT_RADIUS: f32 = 3.5;
/// Half the height of a flag's capture volume, in metres.
pub const DEFAULT_HALF_HEIGHT: f32 = 2.0;
/// How far below a flag's floor feet still count as on it, in metres.
const FLOOR_SLACK: f32 = 0.5;

/// Tunable Domination rules.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DominationRules {
    /// Seconds one player takes to capture a flag (Black Ops 2: 10).
    pub capture_seconds: f32,
    /// Seconds between team points: each owned flag scores one (5).
    pub point_seconds: f32,
    /// Points for each player in the circle when their team takes a flag.
    pub capture_points: u32,
    /// Bonus for a kill with the killer or the victim on or near a flag.
    pub objective_kill_points: u32,
    /// How far outside a flag's circle, in metres, a kill still counts as
    /// on it.
    pub objective_kill_margin: f32,
}

impl Default for DominationRules {
    fn default() -> Self {
        Self {
            capture_seconds: 10.0,
            point_seconds: 5.0,
            capture_points: 150,
            objective_kill_points: 50,
            objective_kill_margin: 5.0,
        }
    }
}

/// One flag's owner and the capture under way on it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FlagState {
    /// The team that owns it, or `None` while neutral.
    pub owner: Option<Team>,
    /// The team capturing it, if any. It stays through a contest.
    pub claim: Option<Team>,
    /// Seconds of capture the claiming team has banked, out of
    /// [`DominationRules::capture_seconds`] (one player's worth a second).
    pub progress: f32,
    /// Both teams are in the circle.
    pub contested: bool,
    /// The claiming team's players in the circle while it is not contested:
    /// how fast the capture runs. 0 when stalled.
    pub capturers: u32,
}

impl FlagState {
    /// How far along the capture is, from 0 to 1.
    pub fn fraction(&self, rules: &DominationRules) -> f32 {
        (self.progress / rules.capture_seconds.max(0.01)).clamp(0.0, 1.0)
    }

    /// Advances by `dt` with `presence[t]` living players of team `t` in the
    /// circle. Returns the team that captured it this tick, if any.
    fn tick(
        &mut self,
        rules: &DominationRules,
        dt: f32,
        presence: [u32; TEAM_COUNT],
    ) -> Option<Team> {
        let here = |t: usize| presence[t] > 0;
        self.contested = here(0) && here(1);
        if self.contested {
            // A contest freezes the capture where it is.
            self.capturers = 0;
            return None;
        }
        let Some(team) = (0..TEAM_COUNT).find(|t| here(*t)).map(|t| t as Team) else {
            // Nobody on it: any capture starts over.
            self.claim = None;
            self.progress = 0.0;
            self.capturers = 0;
            return None;
        };
        if self.owner == Some(team) {
            // The owners stepping on clear the enemy's capture.
            self.claim = None;
            self.progress = 0.0;
            self.capturers = 0;
            return None;
        }
        if self.claim != Some(team) {
            self.claim = Some(team);
            self.progress = 0.0;
        }
        self.capturers = presence[usize::from(team)];
        self.progress += dt * self.capturers as f32;
        if self.progress < rules.capture_seconds {
            return None;
        }
        self.owner = Some(team);
        self.claim = None;
        self.progress = 0.0;
        self.capturers = 0;
        Some(team)
    }

    /// What clients must hear about: anything but the progress ticking up
    /// at the rate they already know.
    fn key(&self) -> (Option<Team>, Option<Team>, bool, u32) {
        (self.owner, self.claim, self.contested, self.capturers)
    }
}

/// A capture in one [`DominationState::tick`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capture {
    /// Index of the flag.
    pub flag: usize,
    pub team: Team,
    /// Who owned it before: `None` for a neutral flag.
    pub from: Option<Team>,
}

/// What changed in one [`DominationState::tick`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Step {
    /// Points each team scored.
    pub points: [u32; TEAM_COUNT],
    pub captures: Vec<Capture>,
    /// A flag's owner, claim, contest, or capture speed changed, so clients
    /// need the new state.
    pub changed: bool,
}

/// Every flag, and the clock to the next team points.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DominationState {
    pub flags: Vec<FlagState>,
    /// Seconds banked toward the next team points.
    score_clock: f32,
    /// Halftime has passed: the teams play from each other's sides.
    pub swapped: bool,
}

impl DominationState {
    /// `count` neutral flags, at the start of a match.
    pub fn new(count: usize) -> Self {
        Self {
            flags: vec![FlagState::default(); count.min(MAX_FLAGS)],
            score_clock: 0.0,
            swapped: false,
        }
    }

    /// Back to neutral flags for the second half, sides swapped. The score
    /// clock starts over too.
    pub fn halftime(&mut self) {
        let count = self.flags.len();
        *self = Self::new(count);
        self.swapped = true;
    }

    /// The flags team `team` owns.
    pub fn owned_by(&self, team: Team) -> usize {
        self.flags.iter().filter(|f| f.owner == Some(team)).count()
    }

    /// Advances by `dt`. `presence[i][t]` counts team `t`'s living players
    /// in flag `i`'s circle. `scoring`: team points are paid (not in the
    /// lobby's warm-up, where flags still change hands for practice).
    pub fn tick(
        &mut self,
        rules: &DominationRules,
        dt: f32,
        presence: &[[u32; TEAM_COUNT]],
        scoring: bool,
    ) -> Step {
        let mut step = Step::default();
        for (i, flag) in self.flags.iter_mut().enumerate() {
            let before = *flag;
            let presence = presence.get(i).copied().unwrap_or_default();
            if let Some(team) = flag.tick(rules, dt, presence) {
                step.captures.push(Capture {
                    flag: i,
                    team,
                    from: before.owner,
                });
            }
            step.changed |= flag.key() != before.key();
        }
        if !scoring {
            self.score_clock = 0.0;
            return step;
        }
        self.score_clock += dt;
        let interval = rules.point_seconds.max(0.01);
        while self.score_clock >= interval {
            self.score_clock -= interval;
            for flag in &self.flags {
                if let Some(team) = flag.owner {
                    step.points[usize::from(team.min(1))] += 1;
                }
            }
        }
        step
    }

    /// Seconds until the next team points.
    pub fn seconds_to_points(&self, rules: &DominationRules) -> f32 {
        (rules.point_seconds - self.score_clock).max(0.0)
    }

    /// The objective bonus for a kill by `killer_team` with the killer at
    /// `killer` and the victim at `victim`: defence if the nearest flag
    /// either stood on or near is the killer's team's, assault otherwise.
    pub fn objective_kill(
        &self,
        rules: &DominationRules,
        points: &[FlagPoint],
        killer_team: Team,
        killer: Vec3,
        victim: Vec3,
    ) -> Option<Award> {
        let margin = rules.objective_kill_margin;
        let (index, _) = points
            .iter()
            .enumerate()
            .filter(|(_, p)| p.near(killer, margin) || p.near(victim, margin))
            .map(|(i, p)| {
                let d = p.center.distance(killer).min(p.center.distance(victim));
                (i, d)
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))?;
        let owner = self.flags.get(index).and_then(|f| f.owner);
        Some(if owner == Some(killer_team) {
            Award::DominationDefend
        } else {
            Award::DominationAssault
        })
    }

    /// What a spawn for `team` should weigh besides the players: each flag
    /// the team owns stands in for [`OWNED_FLAG_PULL`] teammates at its
    /// circle and each enemy flag for [`ENEMY_FLAG_PUSH`] enemies, so players
    /// spawn behind their own flags and away from the enemy's. Taking the
    /// enemy's home flag flips the spawns, as in Call of Duty.
    pub fn spawn_threats(&self, points: &[FlagPoint], team: Team) -> Vec<Threat> {
        let mut threats = Vec::new();
        for (flag, point) in self.flags.iter().zip(points) {
            let (enemy, count) = match flag.owner {
                Some(owner) if owner == team => (false, OWNED_FLAG_PULL),
                Some(_) => (true, ENEMY_FLAG_PUSH),
                None => continue,
            };
            threats.extend((0..count).map(|_| Threat {
                position: point.center,
                enemy,
            }));
        }
        threats
    }
}

/// What a bot should do in Domination ([`bot_goals`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BotGoal {
    /// Go and stand in flag `i` until the team owns it.
    Capture(usize),
    /// Guard flag `i`, which the team owns, standing in its circle so an
    /// enemy capture is contested.
    Defend(usize),
}

/// Bots this many or fewer answer an enemy capture of one of the team's flags.
const RESPONDERS: usize = 2;

/// Where each of team `team`'s bots (`(id, feet)`) should go, the way Call
/// of Duty's bots play Domination: the nearest bots run back to stop an
/// enemy capture of a flag the team owns, one bot in four guards an owned
/// flag, and the rest take the flags the team does not own, most of them the
/// nearest and every third the next nearest, so the team splits over two
/// flags. With every flag owned, everyone guards.
pub fn bot_goals(
    points: &[FlagPoint],
    state: &DominationState,
    team: Team,
    bots: &[(u8, Vec3)],
) -> Vec<(u8, BotGoal)> {
    let mut bots: Vec<(u8, Vec3)> = bots.to_vec();
    bots.sort_unstable_by_key(|(id, _)| *id);
    let count = points.len().min(state.flags.len());
    let owned: Vec<usize> = (0..count)
        .filter(|i| state.flags[*i].owner == Some(team))
        .collect();
    let targets: Vec<usize> = (0..count)
        .filter(|i| state.flags[*i].owner != Some(team))
        .collect();
    let mut out: Vec<(u8, BotGoal)> = Vec::new();
    // The nearest bots answer an enemy capture of a flag we own.
    for &flag in &owned {
        if state.flags[flag].claim.is_none_or(|t| t == team) {
            continue;
        }
        let mut by_distance: Vec<(f32, u8)> = bots
            .iter()
            .filter(|(id, _)| !out.iter().any(|(o, _)| o == id))
            .map(|(id, feet)| (feet.distance(points[flag].center), *id))
            .collect();
        by_distance.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        for (_, id) in by_distance.into_iter().take(RESPONDERS) {
            out.push((id, BotGoal::Defend(flag)));
        }
    }
    let mut guards = 0;
    let mut attackers = 0;
    for (i, (id, feet)) in bots.iter().enumerate() {
        if out.iter().any(|(o, _)| o == id) {
            continue;
        }
        if !owned.is_empty() && (targets.is_empty() || i % 4 == 3) {
            out.push((*id, BotGoal::Defend(owned[guards % owned.len()])));
            guards += 1;
            continue;
        }
        let mut nearest = targets.clone();
        nearest.sort_unstable_by(|a, b| {
            feet.distance(points[*a].center)
                .total_cmp(&feet.distance(points[*b].center))
        });
        let pick = if attackers % 3 == 2 && nearest.len() >= 2 {
            nearest[1]
        } else {
            nearest[0]
        };
        attackers += 1;
        out.push((*id, BotGoal::Capture(pick)));
    }
    out
}

/// Teammates an owned flag counts as when choosing a spawn.
pub const OWNED_FLAG_PULL: usize = 3;
/// Enemies an enemy flag counts as when choosing a spawn.
pub const ENEMY_FLAG_PUSH: usize = 1;

/// Whether the second half starts now: half the match clock has run and the
/// sides have not swapped yet. `time_left` and `time_limit` are the match
/// clock's.
pub fn halftime_due(time_left: f32, time_limit: Option<f32>, swapped: bool) -> bool {
    !swapped && time_limit.is_some_and(|limit| time_left <= limit * 0.5)
}

/// Three flags for maps without their own, on the line between the teams'
/// sides (`side0` and `side1`, at floor height): A near team 0's spawn, B in
/// the middle, and C near team 1's, as Domination maps lay them out.
pub fn fallback_points(side0: Vec3, side1: Vec3) -> Vec<FlagPoint> {
    [("A", 0.22), ("B", 0.5), ("C", 0.78)]
        .into_iter()
        .map(|(name, t)| FlagPoint::new(name, side0.lerp(side1, t)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> DominationRules {
        DominationRules::default()
    }

    /// Runs `seconds` in steps of 0.1 s with the same presence on every flag.
    fn run(
        state: &mut DominationState,
        seconds: f32,
        presence: &[[u32; TEAM_COUNT]],
        scoring: bool,
    ) -> Vec<Step> {
        (0..(seconds * 10.0).round() as u32)
            .map(|_| state.tick(&rules(), 0.1, presence, scoring))
            .collect()
    }

    fn points(steps: &[Step]) -> [u32; TEAM_COUNT] {
        steps.iter().fold([0, 0], |acc, s| {
            [acc[0] + s.points[0], acc[1] + s.points[1]]
        })
    }

    fn captures(steps: &[Step]) -> Vec<Capture> {
        steps.iter().flat_map(|s| s.captures.clone()).collect()
    }

    #[test]
    fn flags_are_circles_on_the_ground() {
        let flag = FlagPoint::new("A", Vec3::ZERO);
        assert!(flag.contains(Vec3::new(3.4, 0.0, 0.0)));
        assert!(flag.contains(Vec3::new(2.4, 0.5, 2.4)));
        // The box's corner is outside the circle.
        assert!(!flag.contains(Vec3::new(3.0, 0.0, 3.0)));
        assert!(!flag.contains(Vec3::new(0.0, 4.5, 0.0)));
        // Feet resting on the floor, or just below it on uneven ground.
        assert!(flag.contains(Vec3::new(0.0, -0.0004, 0.0)));
        assert!(flag.contains(Vec3::new(0.0, -0.4, 0.0)));
        assert!(!flag.contains(Vec3::new(0.0, -0.6, 0.0)));
        assert!(flag.near(Vec3::new(8.0, 0.0, 0.0), 5.0));
        assert!(!flag.near(Vec3::new(9.0, 0.0, 0.0), 5.0));
        assert_eq!(flag.floor(), 0.0);
    }

    #[test]
    fn one_player_captures_in_ten_seconds() {
        let mut state = DominationState::new(3);
        let presence = [[1, 0], [0, 0], [0, 0]];
        assert!(captures(&run(&mut state, 9.9, &presence, true)).is_empty());
        assert!((state.flags[0].fraction(&rules()) - 0.99).abs() < 0.01);
        let steps = run(&mut state, 0.15, &presence, true);
        assert_eq!(
            captures(&steps),
            [Capture {
                flag: 0,
                team: 0,
                from: None
            }]
        );
        assert_eq!(state.flags[0].owner, Some(0));
        assert_eq!(state.flags[0].claim, None);
    }

    #[test]
    fn more_capturers_capture_faster() {
        for (count, seconds) in [(2u32, 5.0f32), (3, 10.0 / 3.0)] {
            let mut state = DominationState::new(1);
            let presence = [[0, count]];
            assert!(captures(&run(&mut state, seconds - 0.2, &presence, true)).is_empty());
            assert_eq!(state.flags[0].capturers, count);
            assert_eq!(captures(&run(&mut state, 0.3, &presence, true)).len(), 1);
            assert_eq!(state.flags[0].owner, Some(1));
        }
    }

    #[test]
    fn a_contest_stalls_the_capture_and_keeps_its_progress() {
        let mut state = DominationState::new(1);
        run(&mut state, 4.0, &[[1, 0]], true);
        let banked = state.flags[0].progress;
        let steps = run(&mut state, 20.0, &[[2, 1]], true);
        assert!(captures(&steps).is_empty());
        assert!(state.flags[0].contested);
        assert_eq!(state.flags[0].capturers, 0);
        assert_eq!(state.flags[0].progress, banked);
        // The defender dies: the capture picks up where it stopped.
        assert_eq!(captures(&run(&mut state, 3.1, &[[2, 0]], true)).len(), 1);
    }

    #[test]
    fn walking_away_or_a_new_claim_starts_the_capture_over() {
        let mut state = DominationState::new(1);
        run(&mut state, 6.0, &[[1, 0]], true);
        run(&mut state, 0.1, &[[0, 0]], true);
        assert_eq!(state.flags[0].progress, 0.0);
        assert_eq!(state.flags[0].claim, None);
        run(&mut state, 6.0, &[[1, 0]], true);
        // The other team alone on it now: their own capture, from nothing.
        run(&mut state, 0.1, &[[0, 1]], true);
        assert_eq!(state.flags[0].claim, Some(1));
        assert!(state.flags[0].progress < 0.2);
    }

    #[test]
    fn an_enemy_flag_takes_as_long_as_a_neutral_one_and_owners_clear_it() {
        let mut state = DominationState::new(1);
        run(&mut state, 10.05, &[[1, 0]], true);
        assert_eq!(state.flags[0].owner, Some(0));
        run(&mut state, 5.0, &[[0, 1]], true);
        // An owner stepping on alone wipes the enemy's capture.
        run(&mut state, 0.1, &[[1, 0]], true);
        assert_eq!((state.flags[0].claim, state.flags[0].progress), (None, 0.0));
        let steps = run(&mut state, 10.05, &[[0, 1]], true);
        assert_eq!(
            captures(&steps),
            [Capture {
                flag: 0,
                team: 1,
                from: Some(0)
            }]
        );
    }

    #[test]
    fn each_owned_flag_scores_a_point_every_five_seconds() {
        let mut state = DominationState::new(3);
        state.flags[0].owner = Some(0);
        state.flags[1].owner = Some(0);
        state.flags[2].owner = Some(1);
        assert_eq!(points(&run(&mut state, 20.05, &[], true)), [8, 4]);
        assert!(state.seconds_to_points(&rules()) > 4.8);
        // No points in the lobby's warm-up.
        assert_eq!(points(&run(&mut state, 20.0, &[], false)), [0, 0]);
    }

    #[test]
    fn steps_report_changes_clients_must_hear_about() {
        let mut state = DominationState::new(1);
        assert!(state.tick(&rules(), 0.1, &[[1, 0]], true).changed);
        // Progress alone is not news: clients run it at the known rate.
        assert!(!state.tick(&rules(), 0.1, &[[1, 0]], true).changed);
        assert!(state.tick(&rules(), 0.1, &[[2, 0]], true).changed);
        assert!(state.tick(&rules(), 0.1, &[[2, 1]], true).changed);
    }

    #[test]
    fn halftime_comes_once_at_half_the_clock() {
        assert!(!halftime_due(301.0, Some(600.0), false));
        assert!(halftime_due(300.0, Some(600.0), false));
        assert!(!halftime_due(100.0, Some(600.0), true));
        assert!(!halftime_due(0.0, None, false));
        let mut state = DominationState::new(3);
        state.flags[1].owner = Some(1);
        state.halftime();
        assert!(state.swapped);
        assert!(state.flags.iter().all(|f| *f == FlagState::default()));
    }

    #[test]
    fn kills_near_a_flag_are_defence_or_assault() {
        let points = fallback_points(Vec3::ZERO, Vec3::new(0.0, 0.0, 100.0));
        let mut state = DominationState::new(3);
        state.flags[0].owner = Some(0);
        let at = |i: usize| points[i].center - Vec3::Y * DEFAULT_HALF_HEIGHT;
        let far = Vec3::new(500.0, 0.0, 0.0);
        assert_eq!(state.objective_kill(&rules(), &points, 0, far, far), None);
        assert_eq!(
            state.objective_kill(&rules(), &points, 0, far, at(0)),
            Some(Award::DominationDefend)
        );
        assert_eq!(
            state.objective_kill(&rules(), &points, 1, at(0), far),
            Some(Award::DominationAssault)
        );
        // A neutral flag is always an assault.
        assert_eq!(
            state.objective_kill(&rules(), &points, 0, at(1), far),
            Some(Award::DominationAssault)
        );
    }

    #[test]
    fn spawns_lean_toward_owned_flags_and_away_from_enemy_flags() {
        use crate::tdm::{SpawnPoint, SpawnRules, choose_spawn};
        let side0 = Vec3::ZERO;
        let side1 = Vec3::new(0.0, 0.0, 80.0);
        let points = fallback_points(side0, side1);
        let spawns: Vec<SpawnPoint> = [0.1, 0.9]
            .iter()
            .map(|t| SpawnPoint {
                position: side0.lerp(side1, *t),
                yaw: 0.0,
                team: None,
            })
            .collect();
        let spawn_rules = SpawnRules::default();
        let pick = |state: &DominationState| {
            let threats = state.spawn_threats(&points, 0);
            choose_spawn(&spawns, 0, &threats, &[], &spawn_rules, |_| 0, 1)
        };
        let mut state = DominationState::new(3);
        state.flags[0].owner = Some(0);
        state.flags[2].owner = Some(1);
        assert_eq!(pick(&state), Some(0));
        // Team 0 takes C and loses A: its spawns flip to C's end.
        state.flags[0].owner = Some(1);
        state.flags[2].owner = Some(0);
        assert_eq!(pick(&state), Some(1));
        assert!(state.spawn_threats(&points, 0).iter().any(|t| t.enemy));
    }

    fn bots_at(feet: &[Vec3]) -> Vec<(u8, Vec3)> {
        feet.iter()
            .enumerate()
            .map(|(i, f)| (200 + i as u8, *f))
            .collect()
    }

    #[test]
    fn bots_split_over_the_nearest_flags_at_the_start() {
        let points = fallback_points(Vec3::ZERO, Vec3::new(0.0, 0.0, 100.0));
        let state = DominationState::new(3);
        let bots = bots_at(&[Vec3::ZERO; 6]);
        let goals = bot_goals(&points, &state, 0, &bots);
        assert_eq!(goals.len(), 6);
        let on = |g: BotGoal| goals.iter().filter(|(_, x)| *x == g).count();
        // Nobody owns anything yet: everyone attacks, most at A, some at B.
        assert_eq!(on(BotGoal::Capture(0)), 4);
        assert_eq!(on(BotGoal::Capture(1)), 2);
    }

    #[test]
    fn bots_guard_what_they_own_and_answer_a_capture() {
        let points = fallback_points(Vec3::ZERO, Vec3::new(0.0, 0.0, 100.0));
        let mut state = DominationState::new(3);
        state.flags[0].owner = Some(0);
        state.flags[1].owner = Some(0);
        let bots = bots_at(&[Vec3::new(0.0, 0.0, 60.0); 8]);
        let goals = bot_goals(&points, &state, 0, &bots);
        let defending = goals
            .iter()
            .filter(|(_, g)| matches!(g, BotGoal::Defend(_)))
            .count();
        assert_eq!(defending, 2, "one in four guards: {goals:?}");
        assert!(goals.iter().any(|(_, g)| *g == BotGoal::Capture(2)));
        // The enemy starts taking A: the two nearest bots go back for it.
        state.flags[0].claim = Some(1);
        let mut bots = bots;
        bots[5].1 = Vec3::new(0.0, 0.0, 25.0);
        bots[6].1 = Vec3::new(0.0, 0.0, 30.0);
        let goals = bot_goals(&points, &state, 0, &bots);
        let answering: Vec<u8> = goals
            .iter()
            .filter(|(_, g)| *g == BotGoal::Defend(0))
            .map(|(id, _)| *id)
            .collect();
        assert!(
            answering.contains(&205) && answering.contains(&206),
            "{goals:?}"
        );
        // Holding all three, everyone guards.
        state.flags[0].claim = None;
        state.flags[2].owner = Some(0);
        let goals = bot_goals(&points, &state, 0, &bots);
        assert!(goals.iter().all(|(_, g)| matches!(g, BotGoal::Defend(_))));
    }

    #[test]
    fn maps_without_flags_get_three_between_the_sides() {
        let points = fallback_points(Vec3::ZERO, Vec3::new(0.0, 0.0, 100.0));
        let names: Vec<&str> = points.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["A", "B", "C"]);
        assert!((points[1].center.z - 50.0).abs() < 0.01);
        assert!(points[0].center.z < points[1].center.z && points[1].center.z < points[2].center.z);
        assert_eq!(points[0].floor(), 0.0);
    }
}
