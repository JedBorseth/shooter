//! How a soldier's body moves over the ground: the locomotion blend the
//! third-person character's animation tree plays (idle, walk, jog, sprint, and
//! the crouched walk), each loop authored in eight directions (forward,
//! strafes, backpedals, and the diagonals between; `tools/build_gait_animations.py`),
//! and the sprint in the forward diagonals it drifts between.
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
/// A sprint drifts sideways at most about 27° (the movement's
/// `sprint_strafe`), between its forward diagonals: its direction is held
/// within them (radians), so it always blends loops that have the sprint.
pub const MAX_SPRINT_DIRECTION: f32 = std::f32::consts::FRAC_PI_4;
/// Share of the hips' turn the spine, chest, and upper chest give back, so
/// the chest faces the aim (turning in place).
pub const TWIST_SHARES: [f32; 3] = [0.35, 0.35, 0.3];
/// The directions each loop was authored in, 45° apart clockwise from
/// forward, as the animation library names them (`gait/<Loop>_<Dir>`; forward is
/// the source library's own loop).
pub const DIRECTIONS: [&str; 8] = ["Fwd", "FR", "R", "BR", "B", "BL", "L", "FL"];
/// Sideways the stride is this much quicker (and each step this much
/// shorter): cadence `1 + LATERAL_CADENCE * |sin(direction)|`. The loops
/// were authored with it.
pub const LATERAL_CADENCE: f32 = 0.2;

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
/// the clips; the idle below them is not a stride). Only forward and its
/// diagonals have the sprint: the other directions blend the walk and the jog
/// ([`loops_in`]).
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
/// How many of [`STAND_LOOPS`] the directions without a sprint have.
pub const DIRECTION_LOOPS: usize = 2;
/// The [`DIRECTIONS`] with a sprint: forward and its diagonals (`FR`, `FL`).
pub const SPRINT_DIRECTIONS: [usize; 3] = [0, 1, 7];

/// How many of [`STAND_LOOPS`] authored direction `index` (of
/// [`DIRECTIONS`]) has.
pub fn loops_in(index: usize) -> usize {
    if SPRINT_DIRECTIONS.contains(&index) {
        STAND_LOOPS.len()
    } else {
        DIRECTION_LOOPS
    }
}
/// The crouched walk.
pub const CROUCH_LOOP: Loop = Loop {
    clip: "ual1/Crouch_Fwd",
    position: CROUCH_SPEED,
    seconds: 2.0,
    offset: 0.12,
};

/// The low crawl prone (`gait/Prone_Crawl`, `tools/build_gait_animations.py`):
/// one stride, each knee drawn up and pushed back once, over its one-second
/// timeline, with the left knee drawn up at phase 0.
pub const PRONE_LOOP: Loop = Loop {
    clip: "gait/Prone_Crawl",
    position: PRONE_STRIDE,
    seconds: 1.0,
    offset: 0.0,
};
/// Metres crawled per stride of the low crawl (the tool's `CRAWL_STRIDE`).
pub const PRONE_STRIDE: f32 = 0.7;
/// The crawl is authored forward, to each side, and backward
/// (`gait/Prone_Crawl`, `_R`, `_B`, `_L`, as the IW engine's prone has a
/// crawl each way); the hit zones' crawl poses are measured in those and the
/// diagonals between, where the drawn blend of two different leg patterns
/// is furthest from a straight line between them: every 45° clockwise from
/// forward.
pub const PRONE_POSE_DIRECTIONS: usize = 8;

/// The two of the [`PRONE_POSE_DIRECTIONS`] around `direction` (radians, as
/// [`direction`]) and how far from the first to the second (0..1).
pub fn prone_direction_samples(direction: f32) -> (usize, usize, f32) {
    let n = PRONE_POSE_DIRECTIONS as f32;
    let at = direction.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU * n;
    let i = (at.floor() as usize) % PRONE_POSE_DIRECTIONS;
    (i, (i + 1) % PRONE_POSE_DIRECTIONS, at - at.floor())
}

/// Where in the standing blend the hit zones' stride poses are measured
/// (`hitbox/poses.rs`): each loop, the points between them a player moves at
/// (an aimed walk, the run every non-sprinting player settles at,
/// [`RUN_BLEND_LIMIT`], and halfway up a sprint, whose high knee is far from
/// a straight line between the run and the sprint), and the sprint. Each is
/// measured sprinting or not as a body at that point would be.
pub const STRIDE_POSITIONS: [(f32, bool); 6] = [
    (WALK_SPEED, false),
    (2.0, false),
    (JOG_SPEED, false),
    (RUN_BLEND_LIMIT, false),
    (5.0, true),
    (SPRINT_SPEED, true),
];

/// How many of [`STRIDE_POSITIONS`] the directions without a sprint are
/// measured at: up to the jog, where their blend ends. Forward and the
/// forward diagonals ([`SPRINT_POSE_DIRECTIONS`]) are measured at all of them.
pub const DIRECTION_STRIDE_POSITIONS: usize = 3;

/// The two measured stride positions around `position` and how far from the
/// first to the second (0..1).
pub fn stride_samples(position: f32) -> (usize, usize, f32) {
    samples_in(&STRIDE_POSITIONS, position)
}

/// [`stride_samples`] for a direction other than forward, whose blend ends
/// at the jog.
pub fn direction_stride_samples(position: f32) -> (usize, usize, f32) {
    samples_in(&STRIDE_POSITIONS[..DIRECTION_STRIDE_POSITIONS], position)
}

fn samples_in(positions: &[(f32, bool)], position: f32) -> (usize, usize, f32) {
    let last = positions.len() - 1;
    match positions.iter().position(|(p, _)| *p > position) {
        Some(0) => (0, 0, 0.0),
        None => (last, last, 0.0),
        Some(n) => {
            let (a, b) = (positions[n - 1].0, positions[n].0);
            (n - 1, n, (position - a) / (b - a))
        }
    }
}

/// The direction of travel (radians from the aim, positive to the right,
/// −π..π) for body-frame velocity (`right`, `forward`), as the loops blend
/// it. A sprint's is held within its forward diagonals
/// ([`MAX_SPRINT_DIRECTION`]).
pub fn direction(right: f32, forward: f32, sprinting: bool) -> f32 {
    if right == 0.0 && forward == 0.0 {
        0.0
    } else if sprinting {
        fmath::atan2(right, forward).clamp(-MAX_SPRINT_DIRECTION, MAX_SPRINT_DIRECTION)
    } else {
        fmath::atan2(right, forward)
    }
}

/// The directions the hit zones' stride poses are measured in (degrees
/// clockwise from forward, 0 to 360): the authored ones, and halfway between
/// those behind the strafes and either side of forward, where the drawn blend
/// of two quite different leg poses (a strafe and a back diagonal; a sprint
/// and its diagonal, the knee high) is too far from a straight line between
/// them.
pub const POSE_DIRECTIONS: [f32; 14] = [
    0.0, 22.5, 45.0, 90.0, 112.5, 135.0, 157.5, 180.0, 202.5, 225.0, 247.5, 270.0, 315.0, 337.5,
];
/// The [`POSE_DIRECTIONS`] besides forward measured at every one of
/// [`STRIDE_POSITIONS`], sprint included: those within the forward
/// diagonals, where a sprint runs.
pub const SPRINT_POSE_DIRECTIONS: [usize; 4] = [1, 2, 12, 13];

/// The two of [`POSE_DIRECTIONS`] around `direction` (radians, as
/// [`direction`]) and how far from the first to the second (0..1).
pub fn pose_direction_samples(direction: f32) -> (usize, usize, f32) {
    let degrees = direction.to_degrees().rem_euclid(360.0);
    let n = POSE_DIRECTIONS.len();
    let i = POSE_DIRECTIONS
        .iter()
        .rposition(|d| *d <= degrees)
        .unwrap_or(0);
    let (from, to) = (
        POSE_DIRECTIONS[i],
        POSE_DIRECTIONS.get(i + 1).copied().unwrap_or(360.0),
    );
    (
        i,
        (i + 1) % n,
        ((degrees - from) / (to - from)).clamp(0.0, 1.0),
    )
}

/// How much quicker the authored stride is in `direction` than forward (each
/// step that much shorter).
pub fn cadence(direction: f32) -> f32 {
    1.0 + LATERAL_CADENCE * fmath::sin_cos(direction).0.abs()
}

/// How much quicker the stride runs in `direction` than forward, as drawn:
/// the loops of the two authored directions around it ([`DIRECTIONS`], 45°
/// apart) blend by weight, and so do their foot paths, two vectors 45°
/// apart whose blend is shorter than either (by up to 8% halfway, more where
/// one is shortened by [`cadence`]). The stride runs faster by as much, so a
/// planted foot keeps pace with the ground instead of sliding along with the
/// body. In an authored direction it is [`cadence`].
pub fn blend_cadence(direction: f32) -> f32 {
    use std::f32::consts::FRAC_PI_4;
    let at = direction / FRAC_PI_4;
    let from = at.floor();
    let t = at - from;
    let (a, b) = (from * FRAC_PI_4, (from + 1.0) * FRAC_PI_4);
    // Each loop's foot path, relative to forward's: the first along x, the
    // second 45° round from it.
    let (la, lb) = ((1.0 - t) / cadence(a), t / cadence(b));
    let half = std::f32::consts::FRAC_1_SQRT_2;
    let (x, y) = (la + lb * half, lb * half);
    1.0 / (x * x + y * y).sqrt()
}

/// The furthest the standing blend goes without sprinting in `direction`:
/// [`RUN_BLEND_LIMIT`] straight ahead, down to the jog (where the other
/// directions' blends end) from 45° off.
pub fn run_limit(direction: f32) -> f32 {
    let off = (direction.abs() / std::f32::consts::FRAC_PI_4).min(1.0);
    RUN_BLEND_LIMIT + (JOG_SPEED - RUN_BLEND_LIMIT) * off
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

/// Strides per second for a body moving at `velocity` with view `yaw`: the
/// loops' own cadence, blended and sped up as the character's animation tree
/// plays them, quicker sideways and between two authored directions
/// ([`blend_cadence`]). Zero standing still, sliding,
/// or off the ground, where no loop plays.
pub fn stride_rate(
    velocity: Vec3,
    yaw: f32,
    stance: crate::movement::Stance,
    sprinting: bool,
    on_floor: bool,
) -> f32 {
    use crate::movement::Stance;
    let (right, forward) = local_velocity(velocity, yaw);
    let speed = (right * right + forward * forward).sqrt();
    if speed <= MOVING_SPEED || !on_floor {
        return 0.0;
    }
    let direction = direction(right, forward, sprinting);
    let cadence = blend_cadence(direction);
    match stance {
        Stance::Sliding => 0.0,
        // The crawl has one stride for every `PRONE_STRIDE` crawled, in any
        // direction.
        Stance::Prone => speed / PRONE_STRIDE,
        Stance::Crouching => crouch_rate(speed) / CROUCH_LOOP.seconds * cadence,
        Stance::Standing => {
            let position = stand_position(speed, sprinting, direction);
            let (a, b, t) = stand_loops(position);
            let loops =
                (1.0 / STAND_LOOPS[a].seconds) * (1.0 - t) + (1.0 / STAND_LOOPS[b].seconds) * t;
            stand_rate(speed, position) * loops * cadence
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

/// Where the standing blend sits for a ground speed in `direction` (see
/// [`direction`]): 0 idle, then the loops' natural speeds. Sprinting blends
/// from the jog to the sprint; otherwise the blend stops at [`run_limit`]
/// and the loops play faster.
pub fn stand_position(speed: f32, sprinting: bool, direction: f32) -> f32 {
    if speed <= MOVING_SPEED {
        0.0
    } else if sprinting {
        speed.clamp(JOG_SPEED, SPRINT_SPEED)
    } else {
        speed.min(run_limit(direction))
    }
}

/// Standing still, the aim may turn this far from the feet (radians) before
/// they step round to it, as a person turns their upper body first.
pub const TURN_START: f32 = 60.0 * std::f32::consts::PI / 180.0;
/// The feet step round at this rate (radians a second) until they are within
/// [`TURN_STOP`] of the aim.
pub const TURN_RATE: f32 = 360.0 * std::f32::consts::PI / 180.0;
pub const TURN_STOP: f32 = 5.0 * std::f32::consts::PI / 180.0;
/// The aim never gets further than this from the feet: a faster turn drags
/// them round.
pub const MAX_TURN: f32 = 90.0 * std::f32::consts::PI / 180.0;

/// Wraps an angle to `[-π, π)`.
pub fn wrap_angle(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// One step of turning in place: the feet's yaw `feet` (world, as the aim's
/// `yaw`), whether they are stepping round (`turning`), after `dt` seconds.
/// A body that is not standing still (`still` false) has its feet under the
/// aim. Returns the new feet yaw and whether they are still turning.
pub fn turn_feet(feet: f32, yaw: f32, still: bool, turning: bool, dt: f32) -> (f32, bool) {
    if !still {
        return (yaw, false);
    }
    let offset = wrap_angle(yaw - feet);
    let turning = turning || offset.abs() > TURN_START;
    let mut feet = feet;
    if turning {
        let step = TURN_RATE * dt;
        feet += offset.clamp(-step, step);
    }
    let offset = wrap_angle(yaw - feet);
    if offset.abs() > MAX_TURN {
        feet = yaw - offset.signum() * MAX_TURN;
    }
    let offset = wrap_angle(yaw - feet);
    (wrap_angle(feet), turning && offset.abs() > TURN_STOP)
}

/// While the feet step round, the legs step in place: the walk loop blended
/// in up to this much, a stride per half turn of the feet.
pub const TURN_STEP_WEIGHT: f32 = 0.7;
/// The step blends in and out at this rate (per second).
const TURN_STEP_RATE: f32 = 8.0;

/// Turning in place over time: where the feet point, whether they are
/// stepping round, and how far into the in-place step the legs are.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Turning {
    feet: Option<f32>,
    turning: bool,
    /// 0..1: how far the legs are stepping (see [`TURN_STEP_WEIGHT`]).
    pub step: f32,
}

impl Turning {
    /// One step of `dt` seconds with the aim at `yaw` and the body standing
    /// `still` or not. Returns the aim's offset from the feet (radians,
    /// positive left) and the stride the in-place step moves the legs
    /// through (strides).
    pub fn tick(&mut self, yaw: f32, still: bool, dt: f32) -> (f32, f32) {
        let before = self.feet.unwrap_or(yaw);
        let (feet, turning) = turn_feet(before, yaw, still, self.turning, dt);
        self.feet = Some(feet);
        self.turning = turning;
        let swung = if still {
            wrap_angle(feet - before).abs()
        } else {
            0.0
        };
        let target = if dt > 0.0 {
            (swung / dt / (TURN_RATE * 0.5)).min(1.0)
        } else {
            self.step
        };
        let rate = TURN_STEP_RATE * dt;
        self.step += (target - self.step).clamp(-rate, rate);
        if !still {
            self.step = 0.0;
        }
        (wrap_angle(yaw - feet), swung / std::f32::consts::PI)
    }

    /// Holds the feet on `feet` (the heading of a ladder being climbed),
    /// however the aim turns, as far as [`MAX_TURN`] allows. Returns the aim's
    /// offset from the feet (radians, positive left).
    pub fn hold(&mut self, feet: f32, yaw: f32) -> f32 {
        let offset = wrap_angle(yaw - feet).clamp(-MAX_TURN, MAX_TURN);
        self.feet = Some(wrap_angle(yaw - offset));
        self.turning = false;
        self.step = 0.0;
        offset
    }
}

/// The yaw (as `movement::basis_from_yaw` takes it) that faces the
/// horizontal direction `facing` (`x`, `z`).
pub fn yaw_of(facing: glam::Vec2) -> f32 {
    fmath::atan2(-facing.x, -facing.y)
}

/// Rungs a climb cycle (both hands and both feet once) covers: a stride
/// is two rungs of 0.3 m.
pub const LADDER_STRIDE: f32 = 0.6;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crawl_blends_the_two_measured_directions_around_the_travel() {
        use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};
        assert_eq!(prone_direction_samples(0.0), (0, 1, 0.0));
        // Right is the third of eight, backward the fifth from either side.
        let (a, b, t) = prone_direction_samples(FRAC_PI_2);
        assert_eq!((a, b), (2, 3));
        assert!(t.abs() < 1e-5);
        assert_eq!(prone_direction_samples(PI).0, 4);
        assert_eq!(prone_direction_samples(-PI).0, 4);
        // Halfway between forward and the forward-left diagonal.
        let (a, b, t) = prone_direction_samples(-FRAC_PI_4 / 2.0);
        assert_eq!((a, b), (7, 0));
        assert!((t - 0.5).abs() < 1e-5);
    }

    #[test]
    fn feet_turn_only_past_the_start_and_then_catch_up() {
        let dt = 1.0 / 120.0;
        // The aim turns 45°: the feet stay.
        let (feet, turning) = turn_feet(0.0, 45f32.to_radians(), true, false, dt);
        assert_eq!((feet, turning), (0.0, false));
        // Past 60° they step round until within 5° of the aim.
        let yaw = 70f32.to_radians();
        let (mut feet, mut turning) = turn_feet(0.0, yaw, true, false, dt);
        assert!(turning && feet > 0.0);
        let mut ticks = 1;
        while turning {
            (feet, turning) = turn_feet(feet, yaw, true, turning, dt);
            ticks += 1;
            assert!(ticks < 120);
        }
        assert!(wrap_angle(yaw - feet).abs() <= TURN_STOP + 1e-4);
        assert!(
            ticks as f32 * dt < 0.25,
            "a 70° turn steps round in {ticks} ticks"
        );
        // A spin drags the feet so the aim is never more than 90° off.
        let (feet, _) = turn_feet(0.0, 3.0, true, false, dt);
        assert!(wrap_angle(3.0 - feet).abs() <= MAX_TURN + 1e-4);
        // Moving, the feet are under the aim.
        assert_eq!(turn_feet(1.0, -2.0, false, true, dt), (-2.0, false));
        // Across ±π the turn goes the short way.
        let (feet, _) = turn_feet(3.1, -3.1, true, true, dt);
        assert!(feet > 3.1 || feet < -3.0, "{feet}");
    }

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
    fn every_direction_has_its_loop_and_a_sprint_drifts_between_its_diagonals() {
        use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};
        // Strafing, backpedalling, and the diagonals each pick their loop.
        assert!((direction(3.0, 0.0, false) - FRAC_PI_2).abs() < 1e-5);
        assert!((direction(0.0, -2.0, false).abs() - PI).abs() < 1e-5);
        assert_eq!(pose_direction_samples(0.0), (0, 1, 0.0));
        assert_eq!(pose_direction_samples(FRAC_PI_2), (3, 4, 0.0));
        let (a, b, t) = pose_direction_samples(-FRAC_PI_4 * 0.25);
        assert_eq!((a, b), (13, 0));
        assert!((t - 0.5).abs() < 1e-4);
        let (a, b, t) = pose_direction_samples(PI);
        assert!((a, b) == (7, 8) && t < 1e-4 || (a, b) == (6, 7) && t > 1.0 - 1e-4);
        // Behind the strafes and beside forward the poses are measured every
        // 22.5°.
        let (a, b, t) = pose_direction_samples(PI * 0.6);
        assert!((a, b) == (3, 4) && (t - 0.8).abs() < 1e-3, "{a} {b} {t}");
        let (a, b, t) = pose_direction_samples(FRAC_PI_4 * 0.75);
        assert!((a, b) == (1, 2) && (t - 0.5).abs() < 1e-3, "{a} {b} {t}");
        // A sprint drifting sideways blends toward its diagonal, never
        // past it, and only directions with a sprint loop are sprinted in.
        assert!((direction(1.0, 2.0, true) - fmath::atan2(1.0, 2.0)).abs() < 1e-6);
        assert_eq!(direction(3.0, 0.0, true), MAX_SPRINT_DIRECTION);
        assert_eq!(direction(-3.0, -1.0, true), -MAX_SPRINT_DIRECTION);
        assert_eq!(direction(0.0, 0.0, true), 0.0);
        for (i, name) in DIRECTIONS.iter().enumerate() {
            let sprint = matches!(*name, "Fwd" | "FR" | "FL");
            assert_eq!(loops_in(i) == STAND_LOOPS.len(), sprint, "{name}");
        }
        for (d, degrees) in POSE_DIRECTIONS.iter().enumerate().skip(1) {
            let within = (degrees.to_radians().sin().abs() <= FRAC_PI_4.sin() + 1e-5)
                && degrees.to_radians().cos() > 0.0;
            assert_eq!(SPRINT_POSE_DIRECTIONS.contains(&d), within, "{degrees}");
        }
        // Sideways steps are quicker; the run blends to the jog from 45° off.
        assert_eq!(cadence(0.0), 1.0);
        assert!((cadence(-FRAC_PI_2) - (1.0 + LATERAL_CADENCE)).abs() < 1e-5);
        // In an authored direction the drawn stride runs at its cadence;
        // halfway between two it runs faster by the blended foot path's loss
        // (cos 22.5° between forward and its diagonal), and symmetrically.
        for d in [
            0.0,
            FRAC_PI_4,
            FRAC_PI_2,
            3.0 * FRAC_PI_4,
            PI,
            -FRAC_PI_4,
            -FRAC_PI_2,
        ] {
            assert!((blend_cadence(d) - cadence(d)).abs() < 1e-4, "{d}");
        }
        let half = blend_cadence(FRAC_PI_4 * 0.5);
        let lengths = (1.0 + 1.0 / cadence(FRAC_PI_4)) * 0.5;
        assert!(
            half > 1.0 / lengths && half < 1.0 / (lengths * 0.92),
            "{half}"
        );
        assert!((blend_cadence(0.3) - blend_cadence(-0.3)).abs() < 1e-5);
        assert!((blend_cadence(2.5) - blend_cadence(-2.5)).abs() < 1e-5);
        assert_eq!(run_limit(0.0), RUN_BLEND_LIMIT);
        assert_eq!(run_limit(-PI), JOG_SPEED);
        assert!(run_limit(0.3) < RUN_BLEND_LIMIT && run_limit(0.3) > JOG_SPEED);
    }

    #[test]
    fn stride_cadence_follows_the_loops() {
        use crate::movement::Stance;
        let forward = |v: f32| Vec3::new(0.0, 0.0, -v);
        // At a loop's natural speed its clip plays at its own length.
        let walk = stride_rate(forward(WALK_SPEED), 0.0, Stance::Standing, false, true);
        assert!((walk - 1.0 / 1.3333).abs() < 1e-4, "{walk}");
        let sprint = stride_rate(forward(SPRINT_SPEED), 0.0, Stance::Standing, true, true);
        assert!((sprint - 1.0 / 0.6667).abs() < 1e-3, "{sprint}");
        // Between loops the cadence blends.
        let between = stride_rate(forward(2.0), 0.0, Stance::Standing, false, true);
        assert!(between > walk && between < 1.0 / 0.9333 * 1.0, "{between}");
        // Backward runs the stride forward too (the backpedal is its own
        // loop), and sideways it is quicker.
        let back = stride_rate(forward(-WALK_SPEED), 0.0, Stance::Standing, false, true);
        assert!((back - walk).abs() < 1e-4, "{back}");
        let side = stride_rate(
            Vec3::new(WALK_SPEED, 0.0, 0.0),
            0.0,
            Stance::Standing,
            false,
            true,
        );
        assert!(
            (side - walk * (1.0 + LATERAL_CADENCE)).abs() < 1e-4,
            "{side}"
        );
        // Still, sliding, or in the air it holds.
        assert_eq!(
            stride_rate(forward(0.2), 0.0, Stance::Standing, false, true),
            0.0
        );
        assert_eq!(
            stride_rate(forward(7.0), 0.0, Stance::Sliding, false, true),
            0.0
        );
        assert_eq!(
            stride_rate(forward(4.0), 0.0, Stance::Standing, false, false),
            0.0
        );
        let crouch = stride_rate(forward(CROUCH_SPEED), 0.0, Stance::Crouching, false, true);
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
        assert_eq!(stand_position(0.2, false, 0.0), 0.0);
        assert_eq!(stand_position(2.0, false, 0.0), 2.0);
        assert_eq!(stand_position(5.5, false, 0.0), RUN_BLEND_LIMIT);
        assert_eq!(stand_position(2.0, true, 0.0), JOG_SPEED);
        // Strafing and backpedalling stop at the jog.
        assert_eq!(stand_position(4.0, false, 1.6), JOG_SPEED);
        assert_eq!(stand_position(3.5, false, -3.1), JOG_SPEED);
    }
}
