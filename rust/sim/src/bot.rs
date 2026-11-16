//! The engine-free half of the Team Deathmatch bots: skill levels, how a bot's
//! aim tracks a target (a reaction delay, a turn-rate limit, and an aim error
//! that shrinks the longer it watches), and how it steers along a path.
//!
//! Bots play through the same `PlayerInput` a human client sends, so they move
//! and shoot with exactly the rules players do. The server's bot brain
//! (`rust/game/src/net/bots.rs`) adds navigation and line-of-sight checks.

use glam::{Vec2, Vec3};

use crate::{fmath, rng};

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
}

impl Skill {
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
        assert!(e.turn_rate < n.turn_rate && n.turn_rate < h.turn_rate);
        assert_eq!(Skill::parse("HARD"), Some(Skill::Hard));
        assert_eq!(Skill::parse("insane"), None);
    }
}
