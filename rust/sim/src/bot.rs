//! The engine-free half of the Team Deathmatch bots: skill levels, how a bot's
//! aim tracks a target (a reaction delay, a turn-rate limit, and an aim error
//! that shrinks the longer it watches), and how it steers along a path.
//!
//! Bots play through the same `PlayerInput` a human client sends, so they move
//! and shoot with exactly the rules players do. The server's bot brain
//! (`rust/game/src/net/bots.rs`) adds navigation and line-of-sight checks.

use glam::{Vec2, Vec3};

use crate::equipment::{self, Collide, Floor, Grenade, Kind};
use crate::{fmath, look, rng};

/// How well a bot plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skill {
    Easy,
    Normal,
    Hard,
}

/// What a skill level means.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkillParams {
    /// Seconds from first sight to the first shot.
    pub reaction: f32,
    /// Fastest the view turns, in radians per second.
    pub turn_rate: f32,
    /// Aim error when a target first appears, in radians.
    pub aim_error: f32,
    /// How fast that error shrinks while the bot keeps watching (per second).
    pub error_decay: f32,
    /// Chance of aiming at the head instead of the chest, per engagement.
    pub head_chance: f32,
    /// Fire only when the aim is within this many radians of the target.
    pub fire_cone: f32,
    /// Seconds of each burst from an automatic weapon, and of the pause after.
    pub burst: f32,
    pub burst_pause: f32,
    /// Field of view, in radians, inside which the bot notices enemies.
    pub fov: f32,
    /// Chance a bot takes a grenade opportunity (enemies bunched up, one
    /// hiding where it was just seen, or one on the objective) when it comes
    /// up. Zero never throws.
    pub grenade_chance: f32,
    /// How long a frag is left to burn after it should have reached the
    /// target, in seconds: the bot cooks it for the rest. `None` never cooks.
    pub cook_margin: Option<f32>,
}

impl Skill {
    /// The name [`Skill::parse`] takes.
    pub fn key(self) -> &'static str {
        match self {
            Skill::Easy => "easy",
            Skill::Normal => "normal",
            Skill::Hard => "hard",
        }
    }

    pub fn parse(name: &str) -> Option<Skill> {
        match name.to_ascii_lowercase().as_str() {
            "easy" => Some(Skill::Easy),
            "normal" => Some(Skill::Normal),
            "hard" => Some(Skill::Hard),
            _ => None,
        }
    }

    pub fn params(self) -> SkillParams {
        let deg = f32::to_radians;
        match self {
            Skill::Easy => SkillParams {
                reaction: 0.65,
                turn_rate: deg(170.0),
                aim_error: deg(7.0),
                error_decay: 1.2,
                head_chance: 0.0,
                fire_cone: deg(3.5),
                burst: 0.3,
                burst_pause: 0.45,
                fov: deg(90.0),
                // Black Ops 2's Recruits throw nothing (`bot_AllowGrenades 0`).
                grenade_chance: 0.0,
                cook_margin: None,
            },
            Skill::Normal => SkillParams {
                reaction: 0.4,
                turn_rate: deg(280.0),
                aim_error: deg(5.0),
                error_decay: 2.0,
                head_chance: 0.15,
                fire_cone: deg(2.5),
                burst: 0.45,
                burst_pause: 0.3,
                fov: deg(110.0),
                grenade_chance: 0.55,
                cook_margin: Some(0.2),
            },
            Skill::Hard => SkillParams {
                reaction: 0.25,
                turn_rate: deg(420.0),
                aim_error: deg(3.5),
                error_decay: 3.2,
                head_chance: 0.35,
                fire_cone: deg(1.8),
                burst: 0.6,
                burst_pause: 0.2,
                fov: deg(120.0),
                grenade_chance: 0.75,
                cook_margin: Some(0.05),
            },
        }
    }
}

/// An angle in `[-PI, PI)`.
pub fn wrap_angle(a: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let wrapped = (a + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI;
    if wrapped >= std::f32::consts::PI {
        wrapped - tau
    } else {
        wrapped
    }
}

/// Turns a view (yaw, pitch) toward `goal`, by at most `max_rate * dt` radians
/// on each axis, the short way round.
pub fn turn_toward(view: Vec2, goal: Vec2, max_rate: f32, dt: f32) -> Vec2 {
    let step = max_rate * dt;
    let yaw = wrap_angle(goal.x - view.x).clamp(-step, step);
    let pitch = (goal.y - view.y).clamp(-step, step);
    Vec2::new(wrap_angle(view.x + yaw), view.y + pitch)
}

/// The angle between two views, in radians.
pub fn view_gap(a: Vec2, b: Vec2) -> f32 {
    Vec2::new(wrap_angle(a.x - b.x), a.y - b.y).length()
}

/// The wish vector (x: strafe right, y: forward) that moves a player looking
/// along `yaw` toward horizontal `direction` (any length; zero stands still).
pub fn wish_toward(yaw: f32, direction: Vec3) -> Vec2 {
    let flat = Vec3::new(direction.x, 0.0, direction.z).normalize_or_zero();
    let (forward, right) = crate::movement::basis_from_yaw(yaw);
    Vec2::new(flat.dot(right), flat.dot(forward))
}

/// One bot watching one target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Engagement {
    pub target: u8,
    /// Seconds the target has been in sight.
    pub seen_for: f32,
    /// Where the aim is off by, in radians (yaw, pitch); shrinks over time.
    pub error: Vec2,
    pub aim_head: bool,
}

impl Engagement {
    /// Starts watching `target`. `seed` picks the error's direction and
    /// whether to go for the head.
    pub fn new(target: u8, skill: &SkillParams, seed: u32) -> Self {
        let angle = rng::unit(seed, 1) * std::f32::consts::TAU;
        let (s, c) = fmath::sin_cos(angle);
        // Errors run wider sideways than up and down, as people miss.
        let error = Vec2::new(c, s * 0.5) * skill.aim_error;
        Self {
            target,
            seen_for: 0.0,
            error,
            aim_head: rng::unit(seed, 2) < skill.head_chance,
        }
    }

    pub fn update(&mut self, skill: &SkillParams, dt: f32) {
        self.seen_for += dt;
        self.error *= fmath::exp(-skill.error_decay * dt);
    }

    /// Whether the bot has had time to react.
    pub fn reacted(&self, skill: &SkillParams) -> bool {
        self.seen_for >= skill.reaction
    }
}

/// Time step of a bot's throw planning, in seconds: coarser than the tick,
/// since a plan only has to land near the target.
pub const PLAN_DT: f32 = 1.0 / 30.0;
/// The lowest and highest view pitch a bot tries for a throw, and the step.
const PLAN_PITCH: (f32, f32, f32) = (-0.4, 1.2, 0.05);
/// Pitches above this lob the grenade high (over cover) rather than flat.
pub const HIGH_ARC: f32 = 0.6;
/// The shortest fuse a bot leaves on a cooked frag, in seconds: enough to
/// reach a target at about 8 m and burst over it.
pub const MIN_COOKED_FUSE: f32 = 0.4;
/// A frag that would go off within this many metres of its target is worth
/// throwing: inside the blast's lethal reach (about 5 m).
pub const MAX_MISS: f32 = 3.5;

/// A throw a bot has worked out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThrowPlan {
    /// The view to throw along.
    pub yaw: f32,
    pub pitch: f32,
    /// Seconds to hold the button from the pin (a frag cooks for that long).
    pub hold: f32,
    /// Seconds left on the fuse as it leaves the hand.
    pub fuse: f32,
    /// Where it should go off, and how far that is from the target.
    pub blast: Vec3,
    pub miss: f32,
}

/// Seconds left on `kind`'s fuse as it leaves a hand that let go of the
/// button `hold` seconds after the pin (the wind-up still burns a frag).
pub fn release_fuse(kind: Kind, hold: f32) -> f32 {
    if kind.cooks() {
        kind.fuse() - hold.max(equipment::PIN_TIME) - equipment::WIND_UP
    } else {
        kind.fuse()
    }
}

/// How long to hold the button for a frag to leave with `fuse` seconds left
/// (no less than the pin pull).
pub fn hold_for_fuse(fuse: f32) -> f32 {
    (equipment::FRAG_FUSE - equipment::WIND_UP - fuse).max(equipment::PIN_TIME)
}

/// Flies `grenade` through `world` at [`PLAN_DT`] for `seconds` and returns
/// where it is at each step (the first is where it starts).
pub fn trace_flight(grenade: Grenade, world: &mut impl Collide, seconds: f32) -> Vec<Vec3> {
    let steps = (seconds / PLAN_DT).ceil().max(0.0) as usize;
    let mut g = grenade;
    // The fuse must not end the trace early.
    g.fuse = f32::MAX;
    let mut out = Vec::with_capacity(steps + 1);
    out.push(g.position);
    for _ in 0..steps {
        g.step(world, PLAN_DT);
        out.push(g.position);
    }
    out
}

/// Where a traced flight is after `seconds`.
pub fn at_time(trace: &[Vec3], seconds: f32) -> Vec3 {
    let i = (seconds / PLAN_DT).round().max(0.0) as usize;
    trace[i.min(trace.len().saturating_sub(1))]
}

/// The fuse a throw leaves with, for a flight that comes closest to the
/// target `reach` seconds out: the whole fuse without cooking, otherwise
/// the reach plus `cook_margin` (a frag only).
fn planned_fuse(kind: Kind, reach: f32, cook_margin: Option<f32>) -> f32 {
    let longest = release_fuse(kind, 0.0);
    match cook_margin {
        Some(margin) if kind.cooks() => (reach + margin).clamp(MIN_COOKED_FUSE, longest),
        _ => longest,
    }
}

/// The best throws of `kind` from `hand` (where it leaves the hand) at
/// `target` (a point on the ground), over a flat floor at the target's
/// height: the best flat throw and the best lob, nearest first. Engine
/// free and cheap (no rays); the caller checks the one it picks against the
/// real map. A frag is cooked by `cook_margin` (see
/// [`SkillParams::cook_margin`]).
pub fn plan_throw(
    kind: Kind,
    hand: Vec3,
    target: Vec3,
    cook_margin: Option<f32>,
) -> Vec<ThrowPlan> {
    let yaw = look::bearing(hand, target).x;
    let longest = release_fuse(kind, 0.0);
    let (low, high, step) = PLAN_PITCH;
    let mut best: [Option<ThrowPlan>; 2] = [None, None];
    let count = ((high - low) / step).round() as usize;
    for k in 0..=count {
        let pitch = low + k as f32 * step;
        let grenade = Grenade::thrown(kind, hand, yaw, pitch, Vec3::ZERO, longest);
        let trace = trace_flight(grenade, &mut Floor(target.y), longest);
        let reach = trace
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.distance(target).total_cmp(&b.1.distance(target)))
            .map_or(0.0, |(i, _)| i as f32 * PLAN_DT);
        let fuse = planned_fuse(kind, reach, cook_margin);
        let blast = at_time(&trace, fuse);
        let plan = ThrowPlan {
            yaw,
            pitch,
            hold: if kind.cooks() {
                hold_for_fuse(fuse)
            } else {
                equipment::PIN_TIME
            },
            fuse,
            blast,
            miss: blast.distance(target),
        };
        let slot = usize::from(pitch > HIGH_ARC);
        if best[slot].is_none_or(|b| plan.miss < b.miss) {
            best[slot] = Some(plan);
        }
    }
    let mut out: Vec<ThrowPlan> = best.into_iter().flatten().collect();
    out.sort_by(|a, b| a.miss.total_cmp(&b.miss));
    out
}

/// A frag this close (metres) that can reach a bot sends it running.
/// Kills land out to about 5 m ([`equipment::frag_damage`]).
pub const DANGER_RADIUS: f32 = 6.5;

/// Where a bot standing at `feet` runs to from a frag at `grenade`: straight
/// away, far enough to clear the lethal part of the blast. A grenade right
/// at its feet sends it the way `seed` picks.
pub fn flee_point(feet: Vec3, grenade: Vec3, seed: u32) -> Vec3 {
    let away = Vec3::new(feet.x - grenade.x, 0.0, feet.z - grenade.z);
    let distance = away.length();
    let direction = if distance > 0.2 {
        away / distance
    } else {
        let (s, c) = fmath::sin_cos(rng::unit(seed, 3) * std::f32::consts::TAU);
        Vec3::new(c, 0.0, s)
    };
    feet + direction * (DANGER_RADIUS + 1.5 - distance).max(2.0)
}

/// Share of a flash's length ([`equipment::flash_seconds`]) a bot sees
/// nothing for. Players' screens hold white for about a third of it, then
/// fade; a bot gets its sight back about where a player could pick out a
/// target through the fade.
pub const FLASH_BLIND_SHARE: f32 = 0.75;

/// Seconds a flash of `amount` (0 to 1) leaves a bot blind.
pub fn blinded_for(amount: f32) -> f32 {
    equipment::flash_seconds(amount) * FLASH_BLIND_SHARE
}

/// How far a blinded bot's aim has wandered `t` seconds into the flash, in
/// radians (yaw, pitch): a slow, seeded drift, wider sideways, as a player
/// sweeping blind.
pub fn blind_wander(seed: u32, t: f32) -> Vec2 {
    let phase = |k: u32| rng::unit(seed, k) * std::f32::consts::TAU;
    let yaw = 0.3 * fmath::sin(t * 2.1 + phase(10)) + 0.12 * fmath::sin(t * 5.3 + phase(11));
    let pitch = 0.08 * fmath::sin(t * 1.7 + phase(12)) + 0.04 * fmath::sin(t * 4.1 + phase(13));
    Vec2::new(yaw, pitch)
}

/// Enemies at least this close together count as a group worth a frag.
pub const CLUSTER_RADIUS: f32 = 4.5;

/// The middle of the biggest group of `points` within [`CLUSTER_RADIUS`] of
/// one of them, and how many are in it, if any two are.
pub fn cluster(points: &[Vec3]) -> Option<(Vec3, usize)> {
    let mut best: Option<(Vec3, usize)> = None;
    for p in points {
        let near: Vec<Vec3> = points
            .iter()
            .copied()
            .filter(|q| q.distance(*p) <= CLUSTER_RADIUS)
            .collect();
        if near.len() >= 2 && best.is_none_or(|(_, n)| near.len() > n) {
            let center = near.iter().copied().sum::<Vec3>() / near.len() as f32;
            best = Some((center, near.len()));
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn angles_wrap_the_short_way() {
        assert!((wrap_angle(3.0 * PI / 2.0) + PI / 2.0).abs() < 1e-5);
        let v = turn_toward(
            Vec2::new(PI - 0.1, 0.0),
            Vec2::new(-PI + 0.1, 0.0),
            10.0,
            1.0,
        );
        assert!((wrap_angle(v.x - (-PI + 0.1))).abs() < 1e-5, "{v}");
        let slow = turn_toward(Vec2::ZERO, Vec2::new(1.0, -1.0), 0.5, 1.0);
        assert_eq!(slow, Vec2::new(0.5, -0.5));
        assert!((view_gap(Vec2::new(PI - 0.1, 0.0), Vec2::new(-PI + 0.1, 0.0)) - 0.2).abs() < 1e-5);
    }

    #[test]
    fn wish_moves_toward_the_direction() {
        // Looking down -Z (yaw 0): moving toward -Z is forward, +X is right.
        assert!(
            (wish_toward(0.0, Vec3::new(0.0, 0.0, -3.0)) - Vec2::new(0.0, 1.0)).length() < 1e-5
        );
        assert!((wish_toward(0.0, Vec3::new(2.0, 5.0, 0.0)) - Vec2::new(1.0, 0.0)).length() < 1e-5);
        // Looking toward +X (yaw -PI/2): +X is forward.
        let wish = wish_toward(-PI / 2.0, Vec3::X);
        assert!((wish - Vec2::new(0.0, 1.0)).length() < 1e-5, "{wish}");
        assert_eq!(wish_toward(1.0, Vec3::ZERO), Vec2::ZERO);
    }

    #[test]
    fn aim_error_shrinks_and_reactions_take_time() {
        for skill in [Skill::Easy, Skill::Normal, Skill::Hard] {
            let p = skill.params();
            let mut e = Engagement::new(3, &p, 42);
            let start = e.error.length();
            assert!(start > 0.0 && start <= p.aim_error + 1e-6);
            assert!(!e.reacted(&p));
            for _ in 0..120 {
                e.update(&p, 1.0 / 120.0);
            }
            assert!(e.reacted(&p));
            assert!(e.error.length() < start * 0.35, "{skill:?}");
        }
    }

    #[test]
    fn harder_bots_react_faster_and_turn_quicker() {
        let (e, n, h) = (
            Skill::Easy.params(),
            Skill::Normal.params(),
            Skill::Hard.params(),
        );
        assert!(e.reaction > n.reaction && n.reaction > h.reaction);
        assert_eq!(e.grenade_chance, 0.0, "Easy throws nothing, as Recruits");
        assert!(n.grenade_chance < h.grenade_chance);
        assert!(e.turn_rate < n.turn_rate && n.turn_rate < h.turn_rate);
        assert_eq!(Skill::parse("HARD"), Some(Skill::Hard));
        assert_eq!(Skill::parse("insane"), None);
    }

    #[test]
    fn a_planned_throw_lands_on_the_target_over_flat_ground() {
        let hand = Vec3::new(0.0, 1.55, 0.0);
        for distance in [8.0f32, 11.0, 14.0, 18.0, 22.0, 27.0] {
            let target = Vec3::new(distance * 0.6, 0.0, -distance * 0.8);
            for margin in [None, Some(0.05), Some(0.2)] {
                let plans = plan_throw(Kind::Frag, hand, target, margin);
                let plan = plans.first().expect("a plan");
                // Thrown at once, a frag runs on past all but a far target,
                // and a late cook (normal bots) overshoots a near one.
                if (margin.is_none() && distance < 20.0) || (margin == Some(0.2) && distance < 10.0)
                {
                    continue;
                }
                assert!(
                    plan.miss < MAX_MISS,
                    "{distance} m {margin:?}: missed by {}",
                    plan.miss
                );
                // The real tick-rate flight agrees with the plan.
                let g = Grenade::thrown(
                    Kind::Frag,
                    hand,
                    plan.yaw,
                    plan.pitch,
                    Vec3::ZERO,
                    plan.fuse,
                );
                let mut real = g;
                let mut floor = Floor(0.0);
                while real.fuse > 0.0 {
                    if real.step(&mut floor, crate::TICK_DT).exploded {
                        break;
                    }
                }
                assert!(
                    real.position.distance(target) < MAX_MISS + 0.5,
                    "{distance} m: real flight ends {}",
                    real.position
                );
            }
        }
    }

    #[test]
    fn cooking_shortens_the_fuse_for_short_throws_only_so_far() {
        let hand = Vec3::new(0.0, 1.55, 0.0);
        let near = plan_throw(Kind::Frag, hand, Vec3::new(0.0, 0.0, -10.0), Some(0.05))[0];
        let raw = plan_throw(Kind::Frag, hand, Vec3::new(0.0, 0.0, -10.0), None)[0];
        assert!(
            near.fuse < raw.fuse - 0.5,
            "cooked {} vs {}",
            near.fuse,
            raw.fuse
        );
        assert!(near.fuse >= MIN_COOKED_FUSE);
        assert!((raw.fuse - release_fuse(Kind::Frag, 0.0)).abs() < 1e-6);
        // Holding for the plan leaves the planned fuse.
        assert!((release_fuse(Kind::Frag, near.hold) - near.fuse).abs() < 1e-4);
        // Tacticals do not cook.
        let smoke = plan_throw(Kind::Smoke, hand, Vec3::new(0.0, 0.0, -8.0), Some(0.35))[0];
        assert_eq!(smoke.fuse, equipment::SMOKE_FUSE);
        assert_eq!(smoke.hold, equipment::PIN_TIME);
    }

    #[test]
    fn bots_run_clear_of_a_frag() {
        let feet = Vec3::new(2.0, 0.0, 0.0);
        let to = flee_point(feet, Vec3::ZERO, 1);
        assert!(to.x > feet.x, "away from it");
        assert!(to.distance(Vec3::ZERO) >= DANGER_RADIUS + 1.0);
        // At its feet: somewhere, the same way every time.
        let a = flee_point(Vec3::ZERO, Vec3::ZERO, 5);
        assert_eq!(a, flee_point(Vec3::ZERO, Vec3::ZERO, 5));
        assert!(a.length() >= 2.0);
    }

    #[test]
    fn flashes_blind_bots_longer_the_harder_they_land() {
        assert_eq!(blinded_for(0.0), 0.0);
        assert!(blinded_for(1.0) > blinded_for(0.3));
        assert!(blinded_for(1.0) <= equipment::FLASH_MAX_SECONDS);
        let w = blind_wander(9, 1.3);
        assert!(w.x.abs() < 0.45 && w.y.abs() < 0.15);
        assert_eq!(w, blind_wander(9, 1.3));
    }

    #[test]
    fn groups_of_enemies_are_found() {
        let lone = [Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0)];
        assert_eq!(cluster(&lone), None);
        let group = [
            Vec3::ZERO,
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 2.0),
            Vec3::new(20.0, 0.0, 0.0),
        ];
        let (center, n) = cluster(&group).expect("a group");
        assert_eq!(n, 3);
        assert!(center.distance(Vec3::new(1.0, 0.0, 2.0 / 3.0)) < 1e-4);
    }
}
