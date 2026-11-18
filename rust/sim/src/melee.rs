//! Melee: a knife on its own button, with a lunge.
//!
//! Pressing melee swings the knife, whatever weapon is in hand. When an enemy
//! stands within [`LUNGE_RANGE`] in front of the player, the client that
//! pressed it sends the enemy's offset with the input
//! ([`PlayerInput::lunge`](crate::player::PlayerInput::lunge)), and movement
//! throws the player at them, stopping [`LUNGE_STOP`] short. The strike lands
//! when the lunge ends (or [`STRIKE_DELAY`] after the press without one) and
//! kills a full-health player.
//!
//! The lunge travels with the input, so prediction and the server move the
//! attacker identically. The server only checks that an enemy really was where
//! the input points (`NetServer`), so a tampered client cannot use the lunge
//! as a dash. Who the strike hits is decided like a shot: online by the server
//! against players rewound to what the attacker saw, offline by the client.

use glam::{Vec2, Vec3};

use crate::movement::basis_from_yaw;

/// Horizontal distance between feet that a strike reaches.
pub const REACH: f32 = 1.5;
/// Extra reach the server allows, for the small differences between what the
/// attacker drew and the rewound hitboxes.
pub const SERVER_SLACK: f32 = 0.5;
/// How far away (feet to feet, horizontally) an enemy can be and still be
/// lunged at.
pub const LUNGE_RANGE: f32 = 3.2;
/// The lunge stops this far from the target's feet: about two bodies apart,
/// in arm's reach.
pub const LUNGE_STOP: f32 = 0.9;
/// Speed of the lunge, in metres a second. A full-range lunge takes about
/// 0.16 s.
pub const LUNGE_SPEED: f32 = 14.0;
/// Horizontal speed left once a lunge ends, so the attacker stops at the
/// target instead of sliding through them.
pub const LUNGE_CARRY_SPEED: f32 = 2.0;
/// Seconds from the press to the strike without a lunge.
pub const STRIKE_DELAY: f32 = 0.1;
/// Seconds from the strike until the weapon is back up and can fire.
pub const RECOVERY: f32 = 0.45;
/// Damage of a strike: more than a full-health player has.
pub const DAMAGE: f32 = 150.0;
/// Half-angle in front of the player, in degrees, in which an enemy can be
/// lunged at.
pub const LUNGE_CONE_DEGREES: f32 = 45.0;
/// Half-angle in front of the player, in degrees, that a strike covers.
pub const STRIKE_CONE_DEGREES: f32 = 70.0;
/// Height above the feet that must be in sight for a strike to land (below a
/// crouched player's head).
pub const CHEST_HEIGHT: f32 = 1.0;
/// Most the target's feet may be above or below the attacker's.
pub const MAX_RISE: f32 = 1.3;
/// Lunges longer than this never come from an honest client, and are dropped
/// from the input before they reach the simulation.
pub const MAX_LUNGE_OFFSET: f32 = 2.0 * LUNGE_RANGE;

/// How far a lunge at a target `offset` away (horizontal, `x` and `z`) carries
/// the player: up to [`LUNGE_STOP`] short of it, and never more than a lunge
/// from the edge of [`LUNGE_RANGE`].
pub fn lunge_travel(offset: Vec2) -> f32 {
    if !offset.is_finite() {
        return 0.0;
    }
    (offset.length() - LUNGE_STOP).clamp(0.0, LUNGE_RANGE - LUNGE_STOP)
}

/// Seconds the lunge at `offset` lasts.
pub fn lunge_time(offset: Vec2) -> f32 {
    lunge_travel(offset) / LUNGE_SPEED
}

/// Seconds from the press to the strike: the end of the lunge, but never
/// sooner than [`STRIKE_DELAY`].
pub fn strike_delay(lunge: Option<Vec2>) -> f32 {
    lunge.map_or(STRIKE_DELAY, |offset| lunge_time(offset).max(STRIKE_DELAY))
}

/// Whether a lunge at `offset` moves the player at all.
pub fn lunges(offset: Vec2) -> bool {
    lunge_travel(offset) > 0.01
}

/// The candidate a melee from `feet`, looking along `yaw`, goes for: within
/// `range` (horizontal, feet to feet), inside a cone of `cone_degrees` either
/// side of the view, and within [`MAX_RISE`] up or down. The nearest wins,
/// with targets off to the side counting as a little farther.
///
/// Candidates closer than [`LUNGE_STOP`] count wherever they are in front of
/// the player, since at arm's length a body fills the view.
pub fn pick<T: Copy>(
    feet: Vec3,
    yaw: f32,
    range: f32,
    cone_degrees: f32,
    candidates: impl IntoIterator<Item = (T, Vec3)>,
) -> Option<(T, Vec3)> {
    let (forward, _) = basis_from_yaw(yaw);
    let forward = Vec2::new(forward.x, forward.z);
    let cos_cone = crate::fmath::cos(cone_degrees.to_radians());
    let mut best: Option<(f32, T, Vec3)> = None;
    for (id, at) in candidates {
        let offset = at - feet;
        if offset.y.abs() > MAX_RISE || !offset.is_finite() {
            continue;
        }
        let flat = Vec2::new(offset.x, offset.z);
        let distance = flat.length();
        if distance > range {
            continue;
        }
        let cos = if distance > 1e-3 {
            flat.dot(forward) / distance
        } else {
            1.0
        };
        let inside = cos >= cos_cone || (distance < LUNGE_STOP && cos > 0.0);
        if !inside {
            continue;
        }
        let score = distance + (1.0 - cos) * 1.5;
        if best.is_none_or(|(s, ..)| score < s) {
            best = Some((score, id, at));
        }
    }
    best.map(|(_, id, at)| (id, at))
}

/// The lunge offset an input carries toward a target at `target` (feet), from
/// a player at `feet`: horizontal only.
pub fn lunge_offset(feet: Vec3, target: Vec3) -> Vec2 {
    Vec2::new(target.x - feet.x, target.z - feet.z)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lunge_stops_short_of_the_target_and_is_capped_at_the_range() {
        assert_eq!(lunge_travel(Vec2::new(0.0, -0.5)), 0.0, "already in reach");
        let travel = lunge_travel(Vec2::new(0.0, -3.0));
        assert!((travel - (3.0 - LUNGE_STOP)).abs() < 1e-5);
        let far = lunge_travel(Vec2::new(0.0, -50.0));
        assert!((far - (LUNGE_RANGE - LUNGE_STOP)).abs() < 1e-5);
        assert_eq!(lunge_travel(Vec2::new(f32::NAN, 1.0)), 0.0);
        assert!(
            lunge_time(Vec2::new(0.0, -LUNGE_RANGE)) < 0.2,
            "a quick lunge"
        );
    }

    #[test]
    fn the_strike_waits_for_the_lunge() {
        assert_eq!(strike_delay(None), STRIKE_DELAY);
        assert_eq!(strike_delay(Some(Vec2::new(0.0, -1.0))), STRIKE_DELAY);
        let long = strike_delay(Some(Vec2::new(0.0, -3.0)));
        assert!((long - lunge_time(Vec2::new(0.0, -3.0))).abs() < 1e-6);
        assert!(long > STRIKE_DELAY);
    }

    #[test]
    fn the_nearest_target_in_front_is_picked() {
        // Yaw 0 looks down -Z.
        let feet = Vec3::ZERO;
        let candidates = [
            (1, Vec3::new(0.0, 0.0, -2.5)),
            (2, Vec3::new(0.3, 0.0, -1.8)),
            // Behind, and closer: never picked.
            (3, Vec3::new(0.0, 0.0, 1.0)),
            // Out of range.
            (4, Vec3::new(0.0, 0.0, -3.5)),
            // Too far above.
            (5, Vec3::new(0.0, 2.0, -1.0)),
        ];
        let picked = pick(feet, 0.0, LUNGE_RANGE, LUNGE_CONE_DEGREES, candidates);
        assert_eq!(picked.map(|(id, _)| id), Some(2));
        let only_behind = [(3, Vec3::new(0.0, 0.0, 1.0))];
        assert!(pick(feet, 0.0, LUNGE_RANGE, LUNGE_CONE_DEGREES, only_behind).is_none());
    }

    #[test]
    fn targets_outside_the_cone_are_skipped_unless_at_arms_length() {
        let feet = Vec3::ZERO;
        // 60 degrees to the right at 2 m: outside the lunge cone.
        let side = Vec3::new(
            2.0 * 60f32.to_radians().sin(),
            0.0,
            -2.0 * 60f32.to_radians().cos(),
        );
        assert!(pick(feet, 0.0, LUNGE_RANGE, LUNGE_CONE_DEGREES, [(1, side)]).is_none());
        // The same angle at arm's length still counts.
        let close = side.normalize() * 0.6;
        assert!(pick(feet, 0.0, LUNGE_RANGE, LUNGE_CONE_DEGREES, [(1, close)]).is_some());
        // Turning toward it brings it into the cone.
        let yaw = -60f32.to_radians();
        assert!(pick(feet, yaw, LUNGE_RANGE, LUNGE_CONE_DEGREES, [(1, side)]).is_some());
    }
}
