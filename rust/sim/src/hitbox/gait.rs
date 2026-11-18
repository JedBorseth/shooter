//! How a soldier's body moves over the ground: the locomotion blend the
//! third-person character's animation tree plays (idle, walk, jog, sprint, and
//! the crouched walk) and how far the hips turn toward a strafe.
//!
//! The character (`rust/game/src/character/rig.rs`) and [`super::hitboxes`]
//! both read these, so the hit zones blend the same base poses the body is
//! drawn in.

use crate::Vec3;
use crate::fmath;

/// Natural ground speeds (m/s) of the in-place loops, measured from the
/// planted foot: each loop's place in the standing blend.
pub const WALK_SPEED: f32 = 1.06;
pub const JOG_SPEED: f32 = 3.0;
pub const SPRINT_SPEED: f32 = 6.0;
/// The crouched walk's place in the crouched blend.
pub const CROUCH_SPEED: f32 = 0.72;
/// Non-sprinting runs blend no further than this toward the sprint, then speed up.
pub const RUN_BLEND_LIMIT: f32 = 4.0;
/// Slower than this (m/s) the body stands still.
pub const MOVING_SPEED: f32 = 0.4;
/// The legs turn at most this far from the aim (radians) when strafing.
pub const MAX_LEG_TURN: f32 = 75.0 * std::f32::consts::PI / 180.0;
/// Share of the hips' turn the spine, chest, and upper chest give back, so
/// the chest faces the aim.
pub const TWIST_SHARES: [f32; 3] = [0.35, 0.35, 0.3];
/// Moving this far back (the cosine of the angle from forward) starts the
/// backward walk; moving less far back than [`BACKWARD_LEAVE`] ends it.
pub const BACKWARD_ENTER: f32 = -0.35;
pub const BACKWARD_LEAVE: f32 = -0.1;

/// Horizontal velocity in the body's frame: (right, forward), m/s.
pub fn local_velocity(velocity: Vec3, yaw: f32) -> (f32, f32) {
    let (s, c) = fmath::sin_cos(yaw);
    let right = velocity.x * c - velocity.z * s;
    let back = velocity.x * s + velocity.z * c;
    (right, -back)
}

/// Where the standing blend sits for a ground speed: 0 idle, then the loops'
/// natural speeds. Sprinting blends from the jog to the sprint.
pub fn stand_position(speed: f32, sprinting: bool) -> f32 {
    if speed <= MOVING_SPEED {
        0.0
    } else if sprinting {
        speed.clamp(JOG_SPEED, SPRINT_SPEED)
    } else {
        speed.min(RUN_BLEND_LIMIT)
    }
}

/// Whether the legs walk backward (the loop played in reverse) rather than
/// turning all the way round. `was` is the previous answer, for hysteresis;
/// the hit zones, which have no history, pass `false`.
pub fn backward(right: f32, forward: f32, sprinting: bool, was: bool) -> bool {
    let speed = (right * right + forward * forward).sqrt();
    if speed <= MOVING_SPEED || sprinting {
        return false;
    }
    let cos = forward / speed;
    cos < if was { BACKWARD_LEAVE } else { BACKWARD_ENTER }
}

/// How far the legs turn from the aim toward the movement, radians (positive
/// toward the right). Walking backward the legs face away from the movement.
pub fn leg_turn(right: f32, forward: f32, backward: bool) -> f32 {
    let angle = fmath::atan2(right, forward);
    let legs = if backward {
        let a = angle - std::f32::consts::PI;
        if a <= -std::f32::consts::PI {
            a + std::f32::consts::TAU
        } else {
            a
        }
    } else {
        angle
    };
    legs.clamp(-MAX_LEG_TURN, MAX_LEG_TURN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn velocity_in_the_body_frame() {
        // Facing -Z (yaw 0), moving -Z is forward.
        let (r, f) = local_velocity(Vec3::new(0.0, 0.0, -2.0), 0.0);
        assert!(r.abs() < 1e-6 && (f - 2.0).abs() < 1e-6);
        // Facing +X (yaw -90°), moving +Z is to the right.
        let (r, f) = local_velocity(Vec3::new(0.0, 0.0, 1.0), -std::f32::consts::FRAC_PI_2);
        assert!((r - 1.0).abs() < 1e-5 && f.abs() < 1e-5, "{r} {f}");
    }

    #[test]
    fn strafing_turns_the_legs_and_backpedalling_does_not() {
        assert!((leg_turn(3.0, 0.0, false) - MAX_LEG_TURN).abs() < 1e-6);
        assert!(backward(0.0, -2.0, false, false));
        assert!(leg_turn(0.0, -2.0, true).abs() < 1e-5);
        assert!(!backward(0.0, -2.0, true, false), "no backward sprint");
        // Hysteresis: a little back of sideways keeps walking backward.
        assert!(backward(1.0, -0.2, false, true));
        assert!(!backward(1.0, -0.2, false, false));
    }

    #[test]
    fn blend_positions() {
        assert_eq!(stand_position(0.2, false), 0.0);
        assert_eq!(stand_position(2.0, false), 2.0);
        assert_eq!(stand_position(5.5, false), RUN_BLEND_LIMIT);
        assert_eq!(stand_position(2.0, true), JOG_SPEED);
    }
}
