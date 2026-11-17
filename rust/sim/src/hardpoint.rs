//! Hardpoint: two teams fight over one zone at a time.
//!
//! A map lists its zones in a fixed order. One is live; every
//! [`HardpointRules::zone_seconds`] the next one in the list takes over,
//! wrapping around. A team with players in the live zone and no enemy in it
//! holds the zone and scores a point every [`HardpointRules::point_seconds`].
//! With both teams inside, the zone is contested and nobody scores; empty, it
//! is neutral. The match runs on [`crate::tdm::MatchState`]: the first team to
//! the score limit wins, or the team ahead when the clock runs out.
//!
//! Players earn points too: for taking the zone ([`Award::HardpointCapture`]),
//! for every few seconds spent holding it ([`Award::HardpointSecure`]), and for
//! kills on or near it ([`Award::HardpointDefend`] when their team held it
//! last, [`Award::HardpointAssault`] otherwise).
//!
//! The server runs this; clients only draw what it sends
//! (`net::protocol::GameEvent::Hardpoint`).

use glam::Vec3;

use crate::score::Award;
use crate::tdm::{TEAM_COUNT, Team};

/// One zone: an axis-aligned box. A player is in it when their feet are.
#[derive(Clone, Debug, PartialEq)]
pub struct Zone {
    /// The name the HUD shows ("Skate Bowl").
    pub name: String,
    pub center: Vec3,
    pub half_extents: Vec3,
}

impl Zone {
    /// Whether feet at `feet` stand in the zone.
    pub fn contains(&self, feet: Vec3) -> bool {
        let d = (feet - self.center).abs();
        d.x <= self.half_extents.x && d.y <= self.half_extents.y && d.z <= self.half_extents.z
    }

    /// Whether `feet` are within `margin` metres of the zone across the
    /// ground, at a height that could be in it.
    pub fn near(&self, feet: Vec3, margin: f32) -> bool {
        let d = (feet - self.center).abs();
        d.x <= self.half_extents.x + margin
            && d.z <= self.half_extents.z + margin
            && d.y <= self.half_extents.y + margin.min(2.0)
    }

    /// Height of the zone's floor.
    pub fn floor(&self) -> f32 {
        self.center.y - self.half_extents.y
    }
}

/// Who has the live zone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Holder {
    /// Nobody is in it.
    #[default]
    Neutral,
    /// Only this team is in it, and it scores.
    Held(Team),
    /// Both teams are in it.
    Contested,
}

impl Holder {
    /// Who holds a zone with `presence[t]` living players of team `t` in it.
    pub fn from_presence(presence: [u32; TEAM_COUNT]) -> Self {
        match (presence[0] > 0, presence[1] > 0) {
            (false, false) => Holder::Neutral,
            (true, false) => Holder::Held(0),
            (false, true) => Holder::Held(1),
            (true, true) => Holder::Contested,
        }
    }

    pub fn team(self) -> Option<Team> {
        match self {
            Holder::Held(team) => Some(team),
            _ => None,
        }
    }
}

/// Tunable Hardpoint rules.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HardpointRules {
    /// Seconds each zone stays live.
    pub zone_seconds: f32,
    /// Seconds of holding per team point.
    pub point_seconds: f32,
    /// A player earns [`Award::HardpointSecure`] for each this many seconds
    /// in a zone their team holds.
    pub secure_seconds: f32,
    pub secure_points: u32,
    /// Points for each player in the zone when their team takes it.
    pub capture_points: u32,
    /// Bonus for a kill with the killer or the victim on or near the zone.
    pub objective_kill_points: u32,
    /// How far outside the zone, in metres, a kill still counts as on it.
    pub objective_kill_margin: f32,
}

impl Default for HardpointRules {
    fn default() -> Self {
        Self {
            zone_seconds: 60.0,
            point_seconds: 1.0,
            secure_seconds: 5.0,
            secure_points: 50,
            capture_points: 100,
            objective_kill_points: 50,
            objective_kill_margin: 5.0,
        }
    }
}

/// What changed in one [`HardpointState::tick`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Step {
    /// Points each team scored.
    pub points: [u32; TEAM_COUNT],
    /// The next zone went live.
    pub moved: bool,
    /// The holder changed (including to neutral when the zone moved).
    pub holder_changed: bool,
    /// This team took the zone: it holds it now, and was not the last team
    /// to hold it since it went live.
    pub captured: Option<Team>,
}

/// The live zone and who holds it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HardpointState {
    /// Index of the live zone in the map's list.
    pub active: usize,
    /// Seconds until the next zone goes live.
    pub time_left: f32,
    pub holder: Holder,
    /// The last team to hold this zone alone. It stays through a contest, so
    /// winning a contest back is not a new capture.
    pub owner: Option<Team>,
    /// Seconds the holder has held toward its next point.
    progress: f32,
}

impl Default for HardpointState {
    fn default() -> Self {
        Self::new(&HardpointRules::default())
    }
}

impl HardpointState {
    /// The first zone, live for a full rotation.
    pub fn new(rules: &HardpointRules) -> Self {
        Self {
            active: 0,
            time_left: rules.zone_seconds,
            holder: Holder::Neutral,
            owner: None,
            progress: 0.0,
        }
    }

    /// The zone after the live one, out of `zone_count`.
    pub fn next(&self, zone_count: usize) -> usize {
        if zone_count == 0 {
            0
        } else {
            (self.active + 1) % zone_count
        }
    }

    /// Advances by `dt` seconds. `presence[t]` counts team `t`'s living
    /// players in the live zone. Returns the points scored and what changed.
    pub fn tick(
        &mut self,
        rules: &HardpointRules,
        zone_count: usize,
        dt: f32,
        presence: [u32; TEAM_COUNT],
    ) -> Step {
        let mut step = Step::default();
        if zone_count == 0 {
            return step;
        }
        let holder = Holder::from_presence(presence);
        if holder != self.holder {
            step.holder_changed = true;
            self.holder = holder;
            self.progress = 0.0;
            if let Holder::Held(team) = holder
                && self.owner != Some(team)
            {
                self.owner = Some(team);
                step.captured = Some(team);
            }
        }
        if let Holder::Held(team) = self.holder {
            self.progress += dt;
            let per_point = rules.point_seconds.max(0.01);
            while self.progress >= per_point {
                self.progress -= per_point;
                step.points[usize::from(team)] += 1;
            }
        }
        self.time_left -= dt;
        if self.time_left <= 0.0 {
            self.active = self.next(zone_count);
            self.time_left += rules.zone_seconds;
            step.moved = true;
            if self.holder != Holder::Neutral {
                step.holder_changed = true;
            }
            self.holder = Holder::Neutral;
            self.owner = None;
            self.progress = 0.0;
        }
        step
    }

    /// The objective bonus for a kill by `killer_team` with the killer at
    /// `killer` and the victim at `victim`, if either was on or near `zone`.
    pub fn objective_kill(
        &self,
        rules: &HardpointRules,
        zone: &Zone,
        killer_team: Team,
        killer: Vec3,
        victim: Vec3,
    ) -> Option<Award> {
        let margin = rules.objective_kill_margin;
        if !zone.near(killer, margin) && !zone.near(victim, margin) {
            return None;
        }
        Some(if self.owner == Some(killer_team) {
            Award::HardpointDefend
        } else {
            Award::HardpointAssault
        })
    }
}

/// One player's time on the hill, for [`Award::HardpointSecure`] and the
/// scoreboard.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HillTime {
    /// Seconds spent in the live zone this match.
    pub seconds: f32,
    /// Seconds held toward the next secure award.
    secure: f32,
}

impl HillTime {
    /// Advances by `dt`. `in_zone`: the player is alive in the live zone;
    /// `holding`: their team holds it. Returns the points of a secure award
    /// earned this tick, if any.
    pub fn tick(
        &mut self,
        rules: &HardpointRules,
        dt: f32,
        in_zone: bool,
        holding: bool,
    ) -> Option<u32> {
        if !in_zone {
            return None;
        }
        self.seconds += dt;
        if !holding {
            return None;
        }
        self.secure += dt;
        if self.secure < rules.secure_seconds {
            return None;
        }
        self.secure -= rules.secure_seconds;
        Some(rules.secure_points)
    }

    /// Whole seconds on the hill, for the scoreboard.
    pub fn whole_seconds(&self) -> u32 {
        self.seconds.max(0.0) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zone() -> Zone {
        Zone {
            name: "Bowl".to_owned(),
            center: Vec3::new(0.0, 1.0, 0.0),
            half_extents: Vec3::new(4.0, 2.0, 3.0),
        }
    }

    fn run(
        state: &mut HardpointState,
        seconds: f32,
        presence: [u32; TEAM_COUNT],
        zones: usize,
    ) -> Vec<Step> {
        let rules = HardpointRules::default();
        (0..(seconds * 10.0).round() as u32)
            .map(|_| state.tick(&rules, zones, 0.1, presence))
            .collect()
    }

    fn points(steps: &[Step]) -> [u32; TEAM_COUNT] {
        steps.iter().fold([0, 0], |acc, s| {
            [acc[0] + s.points[0], acc[1] + s.points[1]]
        })
    }

    #[test]
    fn zones_hold_feet_inside_their_box() {
        let zone = zone();
        assert!(zone.contains(Vec3::new(3.9, -0.9, -2.9)));
        assert!(!zone.contains(Vec3::new(4.1, 0.0, 0.0)));
        assert!(!zone.contains(Vec3::new(0.0, 3.2, 0.0)));
        assert!(zone.near(Vec3::new(8.0, 0.0, 0.0), 5.0));
        assert!(!zone.near(Vec3::new(10.0, 0.0, 0.0), 5.0));
        assert_eq!(zone.floor(), -1.0);
    }

    #[test]
    fn a_team_alone_in_the_zone_scores_a_point_a_second() {
        let mut state = HardpointState::new(&HardpointRules::default());
        let steps = run(&mut state, 10.05, [2, 0], 4);
        assert_eq!(points(&steps), [10, 0]);
        assert_eq!(state.holder, Holder::Held(0));
        assert_eq!(steps[0].captured, Some(0));
        assert!(steps[1..].iter().all(|s| s.captured.is_none()));
    }

    #[test]
    fn contested_and_empty_zones_score_nothing() {
        let mut state = HardpointState::new(&HardpointRules::default());
        assert_eq!(points(&run(&mut state, 5.0, [1, 3], 4)), [0, 0]);
        assert_eq!(state.holder, Holder::Contested);
        assert_eq!(points(&run(&mut state, 5.0, [0, 0], 4)), [0, 0]);
        assert_eq!(state.holder, Holder::Neutral);
    }

    #[test]
    fn winning_a_contest_back_is_not_a_new_capture() {
        let mut state = HardpointState::new(&HardpointRules::default());
        let captured =
            |steps: Vec<Step>| steps.iter().filter_map(|s| s.captured).collect::<Vec<_>>();
        assert_eq!(captured(run(&mut state, 2.0, [1, 0], 4)), [0]);
        assert!(captured(run(&mut state, 2.0, [1, 1], 4)).is_empty());
        assert!(captured(run(&mut state, 2.0, [1, 0], 4)).is_empty());
        // The enemy taking it is a capture, and so is taking it back.
        assert_eq!(captured(run(&mut state, 2.0, [0, 1], 4)), [1]);
        assert_eq!(captured(run(&mut state, 2.0, [1, 0], 4)), [0]);
    }

    #[test]
    fn the_zone_rotates_and_wraps_around() {
        let rules = HardpointRules::default();
        let mut state = HardpointState::new(&rules);
        let steps = run(&mut state, rules.zone_seconds + 0.05, [1, 0], 3);
        assert_eq!(steps.iter().filter(|s| s.moved).count(), 1);
        assert_eq!(state.active, 1);
        // Moving drops the hold: the new zone starts neutral and unowned.
        assert_eq!(state.holder, Holder::Neutral);
        assert_eq!(state.owner, None);
        // The team still standing in the old zone scored for the whole minute.
        assert_eq!(points(&steps), [60, 0]);
        run(&mut state, rules.zone_seconds * 2.0, [0, 0], 3);
        assert_eq!(state.active, 0);
        assert_eq!(state.next(3), 1);
    }

    #[test]
    fn no_zones_means_no_hardpoint() {
        let mut state = HardpointState::new(&HardpointRules::default());
        assert_eq!(points(&run(&mut state, 100.0, [1, 0], 0)), [0, 0]);
        assert_eq!(state.next(0), 0);
    }

    #[test]
    fn kills_near_the_zone_are_defense_or_assault() {
        let rules = HardpointRules::default();
        let mut state = HardpointState::new(&rules);
        let zone = zone();
        let on = Vec3::new(1.0, 0.0, 0.0);
        let far = Vec3::new(40.0, 0.0, 0.0);
        assert_eq!(state.objective_kill(&rules, &zone, 0, far, far), None);
        assert_eq!(
            state.objective_kill(&rules, &zone, 0, far, on),
            Some(Award::HardpointAssault)
        );
        state.tick(&rules, 4, 0.1, [1, 0]);
        assert_eq!(
            state.objective_kill(&rules, &zone, 0, on, far),
            Some(Award::HardpointDefend)
        );
        assert_eq!(
            state.objective_kill(&rules, &zone, 1, on, on),
            Some(Award::HardpointAssault)
        );
    }

    #[test]
    fn holding_pays_a_secure_award_every_few_seconds() {
        let rules = HardpointRules::default();
        let mut time = HillTime::default();
        let mut paid = 0;
        for _ in 0..120 {
            paid += time.tick(&rules, 0.1, true, true).unwrap_or(0);
        }
        // Twelve seconds held: two awards, with two seconds toward the third.
        assert_eq!(paid, 2 * rules.secure_points);
        // A contest counts as time on the hill but pays nothing.
        for _ in 0..100 {
            assert_eq!(time.tick(&rules, 0.1, true, false), None);
        }
        assert_eq!(time.tick(&rules, 1.0, false, false), None);
        assert!((time.seconds - 22.0).abs() < 0.01, "{}", time.seconds);
    }
}
