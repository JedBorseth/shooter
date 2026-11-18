//! How a soldier's body moves over the ground: the locomotion blend the
//! third-person character's animation tree plays (idle, walk, jog, sprint, and
//! the crouched walk) and how far the hips turn toward a strafe.
//!
//! The character (`rust/game/src/character/rig.rs`) and [`super::hitboxes`]
//! both read these, so the hit zones blend the same base poses the body is
//! drawn in.
//!
//! Every locomotion loop runs on one shared stride phase (0..1, the left foot
//! furthest forward at 0), so a stride can be sent in a snapshot and both the
//! drawn body and the hit zones show the same moment of it. The server keeps
//! each player's phase with [`advance_stride`]; the character seeks its loops
//! to it, and [`super::hitboxes`] picks the base pose measured at it.

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

/// One locomotion loop's timing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Loop {
    /// The clip, as the animation library names it.
    pub clip: &'static str,
    /// Where it sits in its blend (its natural ground speed, m/s).
    pub position: f32,
    /// Clip length, seconds: one full stride, left foot to left foot.
    pub seconds: f32,
    /// Phase of the clip (0..1) where the left foot is furthest forward,
    /// which the shared stride phase 0 is aligned to.
    pub offset: f32,
}

/// The standing loops the stride blends between, slowest first (measured from
/// the clips; the idle below them is not a stride).
pub const STAND_LOOPS: [Loop; 3] = [
    Loop {
        clip: "ual1/Walk",
        position: WALK_SPEED,
        seconds: 1.3333,
        offset: 0.0,
    },
    Loop {
        clip: "ual1/Jog_Fwd",
        position: JOG_SPEED,
        seconds: 0.9333,
        offset: 0.0,
    },
    Loop {
        clip: "ual1/Sprint",
        position: SPRINT_SPEED,
        seconds: 0.6667,
        offset: 0.86,
    },
];
/// The crouched walk.
pub const CROUCH_LOOP: Loop = Loop {
    clip: "ual1/Crouch_Fwd",
    position: CROUCH_SPEED,
    seconds: 2.0,
    offset: 0.12,
};

/// Where in the standing blend the hit zones' stride poses are measured
/// (`hitbox/poses.rs`): each loop, the points between them a player moves at
/// (an aimed walk, and the run every non-sprinting player settles at,
/// [`RUN_BLEND_LIMIT`]), and the sprint. Each is measured sprinting or not as
/// a body at that point would be.
pub const STRIDE_POSITIONS: [(f32, bool); 5] = [
    (WALK_SPEED, false),
    (2.0, false),
    (JOG_SPEED, false),
    (RUN_BLEND_LIMIT, false),
    (SPRINT_SPEED, true),
];

/// The two measured stride positions around `position` and how far from the
/// first to the second (0..1).
pub fn stride_samples(position: f32) -> (usize, usize, f32) {
    let last = STRIDE_POSITIONS.len() - 1;
    match STRIDE_POSITIONS.iter().position(|(p, _)| *p > position) {
        Some(0) => (0, 0, 0.0),
        None => (last, last, 0.0),
        Some(n) => {
            let (a, b) = (STRIDE_POSITIONS[n - 1].0, STRIDE_POSITIONS[n].0);
            (n - 1, n, (position - a) / (b - a))
        }
    }
}

/// The climb blends in over this much of a mantle's progress, as the body
/// pulls up into it.
pub const CLIMB_BLEND_IN: f32 = 0.15;

/// How much of the climb shows at mantle `progress` (0..1).
pub fn climb_share(progress: f32) -> f32 {
    (progress / CLIMB_BLEND_IN).clamp(0.0, 1.0)
}

/// How far the standing blend has gone from the idle into the walk (0..1).
pub fn walk_share(position: f32) -> f32 {
    (position / WALK_SPEED).clamp(0.0, 1.0)
}

/// The two standing loops around `position` and how far from the first to the
/// second (0..1).
pub fn stand_loops(position: f32) -> (usize, usize, f32) {
    let last = STAND_LOOPS.len() - 1;
    match STAND_LOOPS.iter().position(|l| l.position > position) {
        Some(0) => (0, 0, 0.0),
        None => (last, last, 0.0),
        Some(n) => {
            let (a, b) = (STAND_LOOPS[n - 1].position, STAND_LOOPS[n].position);
            (n - 1, n, (position - a) / (b - a))
        }
    }
}

/// Playback rate of the standing loops (1 at a loop's natural speed).
pub fn stand_rate(speed: f32, position: f32) -> f32 {
    (speed / position.max(WALK_SPEED)).clamp(0.5, 1.8)
}

/// Playback rate of the crouched walk.
pub fn crouch_rate(speed: f32) -> f32 {
    (speed / CROUCH_SPEED).clamp(0.6, 2.2)
}

/// Strides per second (negative walking backward) for a body moving at
/// `velocity` with view `yaw`: the loops' own cadence, blended and sped up as
/// the character's animation tree plays them. Zero standing still, sliding,
/// or off the ground, where no loop plays.
pub fn stride_rate(
    velocity: Vec3,
    yaw: f32,
    stance: crate::movement::Stance,
    sprinting: bool,
    on_floor: bool,
    backward: bool,
) -> f32 {
    use crate::movement::Stance;
    let (right, forward) = local_velocity(velocity, yaw);
    let speed = (right * right + forward * forward).sqrt();
    if speed <= MOVING_SPEED || !on_floor {
        return 0.0;
    }
    let direction = if backward { -1.0 } else { 1.0 };
    match stance {
        Stance::Sliding => 0.0,
        Stance::Crouching => crouch_rate(speed) / CROUCH_LOOP.seconds * direction,
        Stance::Standing => {
            let position = stand_position(speed, sprinting);
            let (a, b, t) = stand_loops(position);
            let cadence =
                (1.0 / STAND_LOOPS[a].seconds) * (1.0 - t) + (1.0 / STAND_LOOPS[b].seconds) * t;
            stand_rate(speed, position) * cadence * direction
        }
    }
}

/// The stride phase `dt` seconds on at `rate` strides per second.
pub fn advance_stride(phase: f32, rate: f32, dt: f32) -> f32 {
    (phase + rate * dt).rem_euclid(1.0)
}

/// Where a loop's clip is (0..1) at stride `phase`.
pub fn clip_phase(l: &Loop, phase: f32) -> f32 {
    (phase + l.offset).rem_euclid(1.0)
}

/// Shortest-way blend between two stride phases.
pub fn lerp_phase(a: f32, b: f32, t: f32) -> f32 {
    let d = (b - a + 0.5).rem_euclid(1.0) - 0.5;
    (a + d * t).rem_euclid(1.0)
}

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
    fn stride_cadence_follows_the_loops() {
        use crate::movement::Stance;
        let forward = |v: f32| Vec3::new(0.0, 0.0, -v);
        // At a loop's natural speed its clip plays at its own length.
        let walk = stride_rate(
            forward(WALK_SPEED),
            0.0,
            Stance::Standing,
            false,
            true,
            false,
        );
        assert!((walk - 1.0 / 1.3333).abs() < 1e-4, "{walk}");
        let sprint = stride_rate(
            forward(SPRINT_SPEED),
            0.0,
            Stance::Standing,
            true,
            true,
            false,
        );
        assert!((sprint - 1.0 / 0.6667).abs() < 1e-3, "{sprint}");
        // Between loops the cadence blends.
        let between = stride_rate(forward(2.0), 0.0, Stance::Standing, false, true, false);
        assert!(between > walk && between < 1.0 / 0.9333 * 1.0, "{between}");
        // Backward runs the stride the other way; still, sliding, or in the air
        // it holds.
        assert!(stride_rate(forward(1.5), 0.0, Stance::Standing, false, true, true) < 0.0);
        assert_eq!(
            stride_rate(forward(0.2), 0.0, Stance::Standing, false, true, false),
            0.0
        );
        assert_eq!(
            stride_rate(forward(7.0), 0.0, Stance::Sliding, false, true, false),
            0.0
        );
        assert_eq!(
            stride_rate(forward(4.0), 0.0, Stance::Standing, false, false, false),
            0.0
        );
        let crouch = stride_rate(
            forward(CROUCH_SPEED),
            0.0,
            Stance::Crouching,
            false,
            true,
            false,
        );
        assert!((crouch - 0.5).abs() < 1e-4, "{crouch}");
    }

    #[test]
    fn phases_wrap() {
        assert!((advance_stride(0.9, 1.0, 0.2) - 0.1).abs() < 1e-5);
        assert!((advance_stride(0.1, -1.0, 0.2) - 0.9).abs() < 1e-5);
        assert!((lerp_phase(0.9, 0.1, 0.5) - 0.0).abs() < 1e-5);
        assert!((lerp_phase(0.2, 0.4, 0.5) - 0.3).abs() < 1e-5);
        assert!((clip_phase(&STAND_LOOPS[2], 0.5) - 0.36).abs() < 1e-5);
    }

    #[test]
    fn blend_positions() {
        assert_eq!(stand_position(0.2, false), 0.0);
        assert_eq!(stand_position(2.0, false), 2.0);
        assert_eq!(stand_position(5.5, false), RUN_BLEND_LIMIT);
        assert_eq!(stand_position(2.0, true), JOG_SPEED);
    }
}
