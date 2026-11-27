//! Gamepad look: stick shaping and aim assist.
//!
//! Look input is applied on the client only. The server receives the
//! resulting yaw and pitch in `PlayerInput`, so nothing here has to be
//! deterministic across machines. It lives in `shooter-sim` because it is
//! engine-free math that is easiest to tune with unit tests.
//!
//! Aim assist follows Call of Duty's controller aim assist, and has only its
//! parts:
//!
//! - **Slowdown**: stick look turns slower while the crosshair is over or near
//!   an enemy's body.
//! - **Rotational**: while the player is moving or looking, the view follows
//!   part of the enemy's motion relative to the camera, so strafing past an
//!   enemy, or an enemy strafing, keeps the crosshair with them.
//!
//! Like Call of Duty's multiplayer, there is no target aim assist: aiming
//! down sights never snaps onto a player or bot.
//!
//! There is one tuning, [`AimAssistParams::default`]; the setting only turns
//! aim assist on or off. Every part fades out toward the end of the assist
//! range.

use glam::{Vec2, Vec3};

use crate::fmath;

/// Furthest the view pitches up or down, in radians: just short of straight up.
pub const PITCH_LIMIT: f32 = std::f32::consts::FRAC_PI_2 - 0.01;
/// Prone, the view pitches no further down or up than these (radians): the
/// ground is in the way below and the neck stops it above, as Call of Duty's
/// prone view limits do.
pub const PRONE_PITCH_DOWN: f32 = 0.52;
pub const PRONE_PITCH_UP: f32 = 0.87;

/// How far the view may pitch (down, up) with the body `prone` (0 to 1,
/// `MoveState::prone_amount`) of the way down from a crouch to prone.
pub fn pitch_range(prone: f32) -> (f32, f32) {
    let t = prone.clamp(0.0, 1.0);
    (
        PITCH_LIMIT + (PRONE_PITCH_DOWN - PITCH_LIMIT) * t,
        PITCH_LIMIT + (PRONE_PITCH_UP - PITCH_LIMIT) * t,
    )
}

/// The shape of the look stick's response, as Call of Duty's "Aim Response
/// Curve Type" setting offers it (Black Ops Cold War onward).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StickResponse {
    /// Turn rate rises with deflection raised to `StickParams::curve`: fine
    /// near the centre, fast at the edge.
    #[default]
    Standard,
    /// Turn rate in direct proportion to deflection.
    Linear,
    /// An S-curve (smoothstep): fine near the centre, quick through the middle
    /// of the throw, and easing into full speed at the edge.
    Dynamic,
}

impl StickResponse {
    /// The setting's index in the menu (0 Standard, 1 Linear, 2 Dynamic);
    /// anything else is Standard.
    pub fn from_index(index: i64) -> Self {
        match index {
            1 => Self::Linear,
            2 => Self::Dynamic,
            _ => Self::Standard,
        }
    }

    /// Output (0 to 1) for deflection through the live range `t` (0 to 1).
    pub fn apply(self, t: f32, exponent: f32) -> f32 {
        match self {
            Self::Standard => fmath::powf(t, exponent),
            Self::Linear => t,
            Self::Dynamic => t * t * (3.0 - 2.0 * t),
        }
    }
}

/// Stick response settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StickParams {
    /// Deflection below this reads as zero (radial, 0 to 1).
    pub inner_deadzone: f32,
    /// Deflection above `1 - outer_deadzone` reads as full.
    pub outer_deadzone: f32,
    /// Response curve exponent: 1 is linear, higher gives finer control near
    /// the center. Only the Standard response uses it.
    pub curve: f32,
    /// The response curve's shape.
    pub response: StickResponse,
    /// Horizontal turn rate at full deflection, in radians per second.
    pub yaw_speed: f32,
    /// Vertical turn rate at full deflection, in radians per second.
    pub pitch_speed: f32,
    /// Extra horizontal turn rate added after the stick is held at the edge.
    pub boost_yaw_speed: f32,
    /// Seconds at the edge before the boost starts.
    pub boost_delay: f32,
    /// Seconds for the boost to ramp to full.
    pub boost_ramp: f32,
}

impl Default for StickParams {
    fn default() -> Self {
        Self {
            inner_deadzone: 0.1,
            outer_deadzone: 0.04,
            curve: 2.0,
            response: StickResponse::Standard,
            yaw_speed: 200_f32.to_radians(),
            pitch_speed: 150_f32.to_radians(),
            boost_yaw_speed: 120_f32.to_radians(),
            boost_delay: 0.12,
            boost_ramp: 0.25,
        }
    }
}

/// Applies a radial deadzone and response curve. The result keeps the
/// stick's direction and has a length from 0 to 1.
pub fn shape_stick(raw: Vec2, params: &StickParams) -> Vec2 {
    let length = raw.length();
    let live = 1.0 - params.outer_deadzone - params.inner_deadzone;
    if length <= params.inner_deadzone || live <= 0.0 {
        return Vec2::ZERO;
    }
    let t = ((length - params.inner_deadzone) / live).clamp(0.0, 1.0);
    raw / length * params.response.apply(t, params.curve)
}

/// Turns stick deflection into turn rates, with a boost for long full turns.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StickLook {
    edge_time: f32,
}

impl StickLook {
    /// Returns turn rates in radians per second: `x` yaw right, `y` pitch up.
    /// `stick` comes from [`shape_stick`], with `y` up.
    pub fn rates(&mut self, stick: Vec2, params: &StickParams, dt: f32) -> Vec2 {
        // Only a mostly horizontal push at the edge counts as a full turn.
        let at_edge = stick.length() > 0.95 && stick.x.abs() > 0.8;
        self.edge_time = if at_edge { self.edge_time + dt } else { 0.0 };
        let ramp = if params.boost_ramp > 0.0 {
            ((self.edge_time - params.boost_delay) / params.boost_ramp).clamp(0.0, 1.0)
        } else {
            f32::from(self.edge_time >= params.boost_delay)
        };
        let yaw_speed = params.yaw_speed + params.boost_yaw_speed * ramp;
        Vec2::new(stick.x * yaw_speed, stick.y * params.pitch_speed)
    }
}

/// Aim assist tuning. Values named `_hip` and `_ads` blend from the hip to
/// fully aimed down sights.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AimAssistParams {
    /// Enemies further than this, in metres, get no assist.
    pub range: f32,
    /// Distance in metres where the assist starts fading, to nothing at `range`.
    pub fade_from: f32,
    /// The slowdown area around an enemy, as a multiple of their radius. 1 is
    /// the body alone.
    pub bubble_scale: f32,
    /// Smallest margin of the slowdown area past the body, in radians, so
    /// distant enemies still get some.
    pub min_bubble: f32,
    /// Stick speed multiplier with the crosshair on an enemy, at the hip.
    pub slowdown_hip: f32,
    /// Stick speed multiplier with the crosshair on an enemy, aimed.
    pub slowdown_ads: f32,
    /// Fraction of the enemy's relative motion the view follows, at the hip.
    pub rotation_hip: f32,
    /// Fraction of the enemy's relative motion the view follows, aimed.
    pub rotation_ads: f32,
    /// How much nearer the crosshair another enemy must be before the assist
    /// moves to them: 0.4 means 40% more weight.
    pub stickiness: f32,
}

impl Default for AimAssistParams {
    fn default() -> Self {
        Self {
            range: 80.0,
            fade_from: 55.0,
            bubble_scale: 4.0,
            min_bubble: 2_f32.to_radians(),
            slowdown_hip: 0.65,
            slowdown_ads: 0.5,
            rotation_hip: 0.45,
            rotation_ads: 0.45,
            stickiness: 0.4,
        }
    }
}

/// Where on the body rotational aims, as a fraction from the
/// bottom of the capsule's core to its top: the upper chest.
const UPPER_BODY: f32 = 0.75;

/// A target body as a vertical capsule: the segment from `bottom` to `top`,
/// inflated by `radius`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AssistTarget {
    /// Stable identity, so the assist stays on one enemy.
    pub id: u64,
    pub bottom: Vec3,
    pub top: Vec3,
    pub radius: f32,
}

impl AssistTarget {
    pub fn center(&self) -> Vec3 {
        (self.bottom + self.top) * 0.5
    }

    /// The upper chest, which rotational follows.
    pub fn upper_body(&self) -> Vec3 {
        self.bottom.lerp(self.top, UPPER_BODY)
    }
}

/// One frame of view state for [`AimAssist::update`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AssistView {
    pub eye: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    /// 0 at the hip, 1 fully aimed down sights.
    pub ads: f32,
    /// The player is moving or looking. Rotational only works then.
    pub active: bool,
}

/// What aim assist does to this frame's look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AssistOutput {
    /// Multiply stick turn rates by this.
    pub slowdown: f32,
    /// Add to the view angles: `x` yaw (radians, Godot's sign: positive turns
    /// left), `y` pitch up.
    pub rotation: Vec2,
    /// The enemy the assist is on, if any.
    pub target: Option<u64>,
}

impl AssistOutput {
    pub const NONE: Self = Self {
        slowdown: 1.0,
        rotation: Vec2::ZERO,
        target: None,
    };
}

/// Aim assist state: the enemy being followed and its last bearing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AimAssist {
    tracked: Option<(u64, Vec2)>,
}

impl AimAssist {
    /// Forgets the followed enemy, for example when switching to
    /// the mouse.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Picks the enemy to assist on and returns the slowdown and rotation to
    /// apply this frame. `targets` should already be filtered for line of
    /// sight.
    pub fn update(
        &mut self,
        view: &AssistView,
        targets: &[AssistTarget],
        params: &AimAssistParams,
    ) -> AssistOutput {
        let forward = view_direction(view.yaw, view.pitch);
        let current = self.tracked.map(|(id, _)| id);
        let best = targets
            .iter()
            .filter_map(|target| {
                let weight = target_weight(view.eye, forward, target, params)?;
                let score = if Some(target.id) == current {
                    weight * (1.0 + params.stickiness)
                } else {
                    weight
                };
                Some((target, weight, score))
            })
            .max_by(|a, b| a.2.total_cmp(&b.2));

        let Some((target, weight, _)) = best else {
            self.tracked = None;
            return AssistOutput::NONE;
        };

        let slowdown = lerp(
            1.0,
            lerp(params.slowdown_hip, params.slowdown_ads, view.ads),
            weight,
        );

        // Rotational: the bearing of a fixed point on the body changes only
        // when the player or the enemy moves, never when the player turns, so
        // following its change cannot feed back into the player's own look.
        let mut rotation = Vec2::ZERO;
        let bearing_now = bearing(view.eye, target.upper_body());
        if let Some((id, previous)) = self.tracked
            && id == target.id
            && view.active
        {
            let delta = Vec2::new(
                wrap_angle(bearing_now.x - previous.x),
                bearing_now.y - previous.y,
            );
            rotation += delta * lerp(params.rotation_hip, params.rotation_ads, view.ads) * weight;
        }
        self.tracked = Some((target.id, bearing_now));

        AssistOutput {
            slowdown,
            rotation,
            target: Some(target.id),
        }
    }
}

/// Unit view direction for Godot yaw and pitch (yaw 0 looks down -Z).
pub fn view_direction(yaw: f32, pitch: f32) -> Vec3 {
    let (sy, cy) = fmath::sin_cos(yaw);
    let (sp, cp) = fmath::sin_cos(pitch);
    Vec3::new(-sy * cp, sp, -cy * cp)
}

/// Yaw and pitch that look from `from` to `to`, inverse of [`view_direction`].
pub fn bearing(from: Vec3, to: Vec3) -> Vec2 {
    let d = to - from;
    let yaw = fmath::atan2(-d.x, -d.z);
    let pitch = fmath::atan2(d.y, Vec2::new(d.x, d.z).length());
    Vec2::new(yaw, pitch)
}

/// How strongly an enemy pulls: 1 with the crosshair on their body, falling to
/// 0 at the edge of their bubble and toward the end of the assist range.
/// `None` when out of range, behind, or outside the bubble.
fn target_weight(
    eye: Vec3,
    forward: Vec3,
    target: &AssistTarget,
    params: &AimAssistParams,
) -> Option<f32> {
    let distance = eye.distance(target.center());
    if distance > params.range || distance < 0.1 {
        return None;
    }
    let (along, miss, _) = ray_segment_closest(eye, forward, target.bottom, target.top);
    if along <= 0.0 {
        return None;
    }
    // Angles from the crosshair to the body's surface and to the bubble edge.
    let off_body = fmath::atan2((miss - target.radius).max(0.0), along);
    let bubble =
        fmath::atan2(target.radius * (params.bubble_scale - 1.0), along).max(params.min_bubble);
    if off_body >= bubble {
        return None;
    }
    let t = 1.0 - off_body / bubble;
    let fade = if distance > params.fade_from && params.range > params.fade_from {
        1.0 - (distance - params.fade_from) / (params.range - params.fade_from)
    } else {
        1.0
    };
    Some(t * t * (3.0 - 2.0 * t) * fade)
}

/// Closest approach between a ray and a segment. Returns the distance along
/// the ray to that point, the gap between them, and the parameter along the
/// segment (0 at `a`, 1 at `b`).
fn ray_segment_closest(origin: Vec3, direction: Vec3, a: Vec3, b: Vec3) -> (f32, f32, f32) {
    let segment = b - a;
    let w = origin - a;
    let aa = segment.dot(segment);
    let ab = segment.dot(direction);
    let bw = direction.dot(w);
    let aw = segment.dot(w);
    let denominator = aa - ab * ab;
    let s = if aa <= f32::EPSILON || denominator.abs() <= f32::EPSILON {
        0.0
    } else {
        ((aw - ab * bw) / denominator).clamp(0.0, 1.0)
    };
    let point = a + segment * s;
    let along = (point - origin).dot(direction);
    let closest = origin + direction * along.max(0.0);
    (along, closest.distance(point), s)
}

fn wrap_angle(angle: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (angle + PI).rem_euclid(TAU) - PI
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    const DT: f32 = 1.0 / 60.0;

    fn dummy(id: u64, x: f32, z: f32) -> AssistTarget {
        AssistTarget {
            id,
            bottom: Vec3::new(x, 0.3, z),
            top: Vec3::new(x, 1.45, z),
            radius: 0.3,
        }
    }

    fn view_at(eye: Vec3, look_at: Vec3) -> AssistView {
        let b = bearing(eye, look_at);
        AssistView {
            eye,
            yaw: b.x,
            pitch: b.y,
            ads: 0.0,
            active: false,
        }
    }

    fn default_params() -> AimAssistParams {
        AimAssistParams::default()
    }

    #[test]
    fn deadzone_and_curve() {
        let p = StickParams::default();
        assert_eq!(shape_stick(Vec2::new(0.08, 0.0), &p), Vec2::ZERO);
        let full = shape_stick(Vec2::new(0.0, 0.99), &p);
        assert!((full.length() - 1.0).abs() < 1e-5, "{full}");
        // Halfway through the live range gives a quarter with the squared curve.
        let mid = shape_stick(Vec2::new(0.1 + 0.43, 0.0), &p);
        assert!((mid.x - 0.25).abs() < 1e-3, "{mid}");
        // Direction is preserved.
        let diag = shape_stick(Vec2::new(0.5, 0.5), &p);
        assert!((diag.x - diag.y).abs() < 1e-6);
    }

    #[test]
    fn response_curve_types() {
        let mut p = StickParams::default();
        let at = |p: &StickParams, t: f32| {
            let live = 1.0 - p.inner_deadzone - p.outer_deadzone;
            shape_stick(Vec2::new(p.inner_deadzone + t * live, 0.0), p).x
        };
        p.response = StickResponse::Linear;
        assert!((at(&p, 0.3) - 0.3).abs() < 1e-4);
        p.response = StickResponse::Dynamic;
        // Finer than linear early, faster than linear late, full at the edge.
        assert!(at(&p, 0.2) < 0.2 && at(&p, 0.2) > 0.2_f32.powi(2));
        assert!(at(&p, 0.7) > 0.7);
        assert!((at(&p, 1.0) - 1.0).abs() < 1e-4);
        assert!((at(&p, 0.5) - 0.5).abs() < 1e-4);
        assert_eq!(StickResponse::from_index(2), StickResponse::Dynamic);
        assert_eq!(StickResponse::from_index(9), StickResponse::Standard);
    }

    #[test]
    fn boost_ramps_in_on_long_full_turns() {
        let p = StickParams::default();
        let mut look = StickLook::default();
        let first = look.rates(Vec2::X, &p, DT);
        assert!((first.x - p.yaw_speed).abs() < 1e-5);
        for _ in 0..60 {
            look.rates(Vec2::X, &p, DT);
        }
        let boosted = look.rates(Vec2::X, &p, DT);
        assert!((boosted.x - (p.yaw_speed + p.boost_yaw_speed)).abs() < 1e-4);
        let released = look.rates(Vec2::new(0.5, 0.0), &p, DT);
        assert!((released.x - 0.5 * p.yaw_speed).abs() < 1e-5);
    }

    #[test]
    fn bearing_inverts_view_direction() {
        for (yaw, pitch) in [(0.0, 0.0), (1.0, 0.3), (-2.5, -0.7)] {
            let b = bearing(Vec3::ZERO, view_direction(yaw, pitch) * 10.0);
            assert!((b.x - yaw).abs() < 1e-5 && (b.y - pitch).abs() < 1e-5);
        }
    }

    #[test]
    fn slowdown_is_strongest_on_target_and_fades_out() {
        let params = default_params();
        let target = dummy(1, 0.0, -10.0);
        let eye = Vec3::new(0.0, 1.6, 0.0);
        let mut assist = AimAssist::default();

        let on = assist.update(&view_at(eye, target.center()), &[target], &params);
        assert_eq!(on.target, Some(1));
        assert!((on.slowdown - params.slowdown_hip).abs() < 1e-5);

        let near = assist.update(
            &view_at(eye, Vec3::new(0.8, 1.0, -10.0)),
            &[target],
            &params,
        );
        assert!(near.slowdown > on.slowdown && near.slowdown < 1.0);

        let off = assist.update(
            &view_at(eye, Vec3::new(4.0, 1.0, -10.0)),
            &[target],
            &params,
        );
        assert_eq!(off, AssistOutput::NONE);
    }

    #[test]
    fn slowdown_area_reaches_past_the_body() {
        let params = default_params();
        let eye = Vec3::new(0.0, 1.6, 0.0);
        for distance in [5.0, 15.0, 40.0] {
            let target = dummy(1, 0.0, -distance);
            let edge = (target.radius / distance).atan() + 1.5_f32.to_radians();
            let mut view = view_at(eye, target.center());
            view.yaw -= edge;
            let out = AimAssist::default().update(&view, &[target], &params);
            assert_eq!(out.target, Some(1), "at {distance} m");
        }
    }

    #[test]
    fn aiming_down_sights_slows_more() {
        let params = default_params();
        let target = dummy(1, 0.0, -10.0);
        let mut view = view_at(Vec3::new(0.0, 1.6, 0.0), target.center());
        view.ads = 1.0;
        let out = AimAssist::default().update(&view, &[target], &params);
        assert!((out.slowdown - params.slowdown_ads).abs() < 1e-5);
    }

    #[test]
    fn out_of_range_and_behind_get_nothing() {
        let params = default_params();
        let eye = Vec3::new(0.0, 1.6, 0.0);
        let far = dummy(1, 0.0, -100.0);
        let behind = dummy(2, 0.0, 10.0);
        let mut view = view_at(eye, far.center());
        view.active = true;
        let out = AimAssist::default().update(&view, &[far, behind], &params);
        assert_eq!(out, AssistOutput::NONE);
    }

    #[test]
    fn fades_toward_the_end_of_range() {
        let params = default_params();
        let eye = Vec3::new(0.0, 1.6, 0.0);
        let slowdown = |distance: f32| {
            let target = dummy(1, 0.0, -distance);
            let view = view_at(eye, target.center());
            AimAssist::default()
                .update(&view, &[target], &params)
                .slowdown
        };
        assert!((slowdown(30.0) - params.slowdown_hip).abs() < 1e-4);
        assert!(slowdown(70.0) > slowdown(30.0) && slowdown(70.0) < 1.0);
    }

    #[test]
    fn picks_the_target_nearest_the_crosshair() {
        let params = default_params();
        let eye = Vec3::new(0.0, 1.6, 0.0);
        let a = dummy(1, 1.2, -10.0);
        let b = dummy(2, -0.1, -10.0);
        let view = view_at(eye, Vec3::new(0.0, 1.0, -10.0));
        let out = AimAssist::default().update(&view, &[a, b], &params);
        assert_eq!(out.target, Some(2));
    }

    /// Two enemies side by side: once on one, the crosshair moving to halfway
    /// between them does not move the assist across.
    #[test]
    fn stays_on_the_current_target() {
        let params = default_params();
        let eye = Vec3::new(0.0, 1.6, 0.0);
        let a = dummy(1, -1.0, -10.0);
        let b = dummy(2, 1.0, -10.0);
        let mut assist = AimAssist::default();
        let look = |x: f32| view_at(eye, Vec3::new(x, 1.0, -10.0));
        assert_eq!(assist.update(&look(-0.6), &[a, b], &params).target, Some(1));
        assert_eq!(assist.update(&look(0.0), &[a, b], &params).target, Some(1));
        assert_eq!(assist.update(&look(0.9), &[a, b], &params).target, Some(2));
    }

    /// Strafing past an enemy: the view follows the rotational share of their
    /// relative motion, but only while the player is giving input.
    #[test]
    fn rotation_follows_relative_motion_while_active() {
        let params = default_params();
        let target = dummy(1, 0.0, -10.0);
        for active in [true, false] {
            let mut assist = AimAssist::default();
            let start = Vec3::new(0.0, 1.6, 0.0);
            let mut eye = start;
            let mut view = view_at(eye, target.upper_body());
            view.active = active;
            // The first frame finds the target; following starts after it.
            assist.update(&view, &[target], &params);
            // Short enough that the motion left behind stays on the body.
            for _ in 0..6 {
                eye.x += 4.0 * DT;
                view.eye = eye;
                let out = assist.update(&view, &[target], &params);
                view.yaw += out.rotation.x;
                view.pitch += out.rotation.y;
            }
            let start_yaw = bearing(start, target.upper_body()).x;
            let moved = bearing(eye, target.upper_body()).x - start_yaw;
            let followed = view.yaw - start_yaw;
            if active {
                assert!(
                    (followed / moved - params.rotation_hip).abs() < 0.02,
                    "{followed} of {moved}"
                );
            } else {
                assert_eq!(followed, 0.0);
            }
        }
    }

    /// Turning the view must not create rotation on its own.
    #[test]
    fn turning_alone_adds_no_rotation() {
        let params = default_params();
        let target = dummy(1, 0.0, -10.0);
        let eye = Vec3::new(0.0, 1.6, 0.0);
        let mut assist = AimAssist::default();
        let mut view = view_at(eye, target.upper_body());
        view.active = true;
        for _ in 0..10 {
            view.yaw += 0.002;
            let out = assist.update(&view, &[target], &params);
            assert_eq!(out.rotation, Vec2::ZERO);
        }
    }

    #[test]
    fn switching_targets_does_not_jump() {
        let params = default_params();
        let eye = Vec3::new(0.0, 1.6, 0.0);
        let a = dummy(1, 0.0, -10.0);
        let b = dummy(2, 0.0, -20.0);
        let mut assist = AimAssist::default();
        let mut view = view_at(eye, a.upper_body());
        view.active = true;
        assist.update(&view, &[a], &params);
        let out = assist.update(&view, &[b], &params);
        assert_eq!(out.target, Some(2));
        assert_eq!(out.rotation, Vec2::ZERO);
    }

    /// A simulated thumb tracks an enemy who strafes back and forth: it sees
    /// the enemy a reaction time late, under-leads a little, and has a slight
    /// tremor. Returns the share of frames the crosshair was on the body.
    fn tracking_share(assisted: bool, ads: f32, player_strafes: bool) -> f32 {
        let params = assisted.then(AimAssistParams::default);
        let stick = StickParams::default();
        let zoom = if ads > 0.0 { 0.75 } else { 1.0 };
        let mut eye = Vec3::new(0.0, 1.6, 0.0);
        let mut x = 0.0_f32;
        let mut target_velocity = 4.5;
        let mut player_velocity = 3.0;
        let mut view = view_at(eye, dummy(1, 0.0, -15.0).upper_body());
        view.ads = ads;
        let mut assist = AimAssist::default();
        let reaction = 9; // frames, 150 ms
        let mut seen: VecDeque<Vec2> = VecDeque::new();
        let mut thumb = Vec2::ZERO;
        let mut rng = 0x2545_f491_u32;
        let mut noise = || {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            (rng as f32 / u32::MAX as f32 - 0.5) * 0.2
        };
        let frames = 600;
        let mut on = 0;
        for frame in 0..frames {
            if frame % 50 == 0 && frame > 0 {
                target_velocity = -target_velocity;
            }
            if frame % 70 == 0 && frame > 0 {
                player_velocity = -player_velocity;
            }
            x += target_velocity * DT;
            if player_strafes {
                eye.x += player_velocity * DT;
            }
            let target = dummy(1, x, -15.0);
            view.eye = eye;

            seen.push_back(bearing(eye, target.upper_body()));
            if seen.len() > reaction + 1 {
                seen.pop_front();
            }
            let late = seen[0];
            let motion = if seen.len() > 1 {
                Vec2::new(wrap_angle(seen[1].x - seen[0].x), seen[1].y - seen[0].y) / DT
            } else {
                Vec2::ZERO
            };
            let lead = late + motion * (seen.len() - 1) as f32 * DT * 0.6;
            let error = Vec2::new(wrap_angle(lead.x - view.yaw), lead.y - view.pitch);
            let want =
                (error * 6.0 + motion * 0.6) / Vec2::new(stick.yaw_speed, stick.pitch_speed) / zoom;
            // Stick right turns yaw down. The thumb sets a raw deflection
            // through the response curve and cannot move instantly.
            let raw = Vec2::new(-want.x, want.y).clamp(Vec2::splat(-1.0), Vec2::splat(1.0));
            let raw = raw.map(|v| {
                let live = 1.0 - stick.inner_deadzone - stick.outer_deadzone;
                v.signum() * (stick.inner_deadzone + v.abs().powf(1.0 / stick.curve) * live)
            }) + Vec2::new(noise(), noise());
            thumb += (raw - thumb) * (1.0 - (-DT / 0.08).exp());
            let shaped = shape_stick(thumb, &stick);
            view.active = player_strafes || shaped != Vec2::ZERO;

            let mut slowdown = 1.0;
            if let Some(params) = &params {
                let out = assist.update(&view, &[target], params);
                slowdown = out.slowdown;
                view.yaw += out.rotation.x;
                view.pitch += out.rotation.y;
            }
            view.yaw -= shaped.x * stick.yaw_speed * zoom * slowdown * DT;
            view.pitch += shaped.y * stick.pitch_speed * zoom * slowdown * DT;

            let forward = view_direction(view.yaw, view.pitch);
            let (_, miss, _) = ray_segment_closest(eye, forward, target.bottom, target.top);
            if miss <= target.radius {
                on += 1;
            }
        }
        on as f32 / frames as f32
    }

    #[test]
    fn assist_makes_tracking_a_strafing_enemy_easier() {
        for (ads, strafes) in [(0.0, false), (1.0, false), (1.0, true)] {
            let off = tracking_share(false, ads, strafes);
            let on = tracking_share(true, ads, strafes);
            println!("ads {ads} strafing {strafes}: off {off:.2} on {on:.2}");
            assert!(on > off * 1.2, "on {on} vs off {off}");
        }
    }
}
