//! Gamepad look: stick shaping and aim assist.
//!
//! Look input is applied on the client only. The server receives the
//! resulting yaw and pitch in `PlayerInput`, so nothing here has to be
//! deterministic across machines. It lives in `shooter-sim` because it is
//! engine-free math that is easiest to tune with unit tests.
//!
//! Aim assist follows the console-shooter model, and never moves the view on
//! its own:
//!
//! - **Slowdown**: stick look turns slower while the crosshair is over or near a
//!   target, so small corrections are easier.
//! - **Rotational**: while the player is moving or looking, the view follows a
//!   fraction of the target's motion relative to the camera. Strafing past a
//!   target keeps the crosshair closer to it.
//!
//! There is no snap-to-target on aiming down sights.

use glam::{Vec2, Vec3};

use crate::fmath;

/// Stick response settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StickParams {
    /// Deflection below this reads as zero (radial, 0 to 1).
    pub inner_deadzone: f32,
    /// Deflection above `1 - outer_deadzone` reads as full.
    pub outer_deadzone: f32,
    /// Response curve exponent: 1 is linear, higher gives finer control near
    /// the center.
    pub curve: f32,
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
    raw / length * fmath::powf(t, params.curve)
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

/// Aim assist strength. Values blend from hip to fully aimed down sights.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AimAssistParams {
    /// Targets further than this, in metres, get no assist.
    pub range: f32,
    /// The assist area around a target, as a multiple of its radius.
    pub bubble_scale: f32,
    /// Smallest assist area, in radians, so distant targets still get some.
    pub min_bubble: f32,
    /// Stick speed multiplier with the crosshair on the target, at the hip.
    pub slowdown_hip: f32,
    /// Stick speed multiplier with the crosshair on the target, aimed.
    pub slowdown_ads: f32,
    /// Fraction of the target's relative motion the view follows, at the hip.
    pub rotation_hip: f32,
    /// Fraction of the target's relative motion the view follows, aimed.
    pub rotation_ads: f32,
}

impl Default for AimAssistParams {
    fn default() -> Self {
        Self {
            range: 60.0,
            bubble_scale: 3.0,
            min_bubble: 1.5_f32.to_radians(),
            slowdown_hip: 0.65,
            slowdown_ads: 0.45,
            rotation_hip: 0.35,
            rotation_ads: 0.6,
        }
    }
}

/// A target body as a vertical capsule: the segment from `bottom` to `top`,
/// inflated by `radius`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AssistTarget {
    /// Stable identity, so rotational assist only follows one target's motion.
    pub id: u64,
    pub bottom: Vec3,
    pub top: Vec3,
    pub radius: f32,
}

impl AssistTarget {
    pub fn center(&self) -> Vec3 {
        (self.bottom + self.top) * 0.5
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
    /// The player is looking or moving. Rotational assist only works then.
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
    /// The target the assist is on, if any.
    pub target: Option<u64>,
}

impl AssistOutput {
    pub const NONE: Self = Self {
        slowdown: 1.0,
        rotation: Vec2::ZERO,
        target: None,
    };
}

/// Aim assist state: the target being followed and its last bearing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AimAssist {
    tracked: Option<(u64, Vec2)>,
}

impl AimAssist {
    /// Forgets the followed target, for example when switching to the mouse.
    pub fn reset(&mut self) {
        self.tracked = None;
    }

    /// Picks the target nearest the crosshair and returns the slowdown and
    /// rotation to apply. `targets` should already be filtered for line of sight.
    pub fn update(
        &mut self,
        view: &AssistView,
        targets: &[AssistTarget],
        params: &AimAssistParams,
    ) -> AssistOutput {
        let forward = view_direction(view.yaw, view.pitch);
        let best = targets
            .iter()
            .filter_map(|target| {
                let weight = target_weight(view.eye, forward, target, params)?;
                Some((target, weight))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1));

        let Some((target, weight)) = best else {
            self.tracked = None;
            return AssistOutput::NONE;
        };

        let slowdown_full = lerp(params.slowdown_hip, params.slowdown_ads, view.ads);
        let slowdown = lerp(1.0, slowdown_full, weight);

        // The bearing of the target's center changes only when the player or
        // the target moves, never when the player turns, so following its
        // change cannot feed back into the player's own look input.
        let bearing = bearing(view.eye, target.center());
        let mut rotation = Vec2::ZERO;
        if let Some((id, previous)) = self.tracked
            && id == target.id
            && view.active
        {
            let delta = Vec2::new(wrap_angle(bearing.x - previous.x), bearing.y - previous.y);
            let strength = lerp(params.rotation_hip, params.rotation_ads, view.ads) * weight;
            rotation = delta * strength;
        }
        self.tracked = Some((target.id, bearing));

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

/// How strongly a target pulls: 1 with the crosshair on its body, falling to 0
/// at the edge of its assist area. `None` when out of range or behind.
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
    let (along, miss) = ray_segment_distance(eye, forward, target.bottom, target.top);
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
    Some(t * t * (3.0 - 2.0 * t))
}

/// Closest approach between a ray and a segment. Returns the distance along
/// the ray to that point and the gap between them.
fn ray_segment_distance(origin: Vec3, direction: Vec3, a: Vec3, b: Vec3) -> (f32, f32) {
    let segment = b - a;
    let w = origin - a;
    let aa = segment.dot(segment);
    let ab = segment.dot(direction);
    let bw = direction.dot(w);
    let aw = segment.dot(w);
    let denominator = aa - ab * ab;
    // Parameter along the segment (0..1); the ray parameter follows from it.
    let s = if aa <= f32::EPSILON || denominator.abs() <= f32::EPSILON {
        0.0
    } else {
        ((aw - ab * bw) / denominator).clamp(0.0, 1.0)
    };
    let point = a + segment * s;
    let along = (point - origin).dot(direction);
    let closest = origin + direction * along.max(0.0);
    (along, closest.distance(point))
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

    const DT: f32 = 1.0 / 60.0;

    fn dummy(id: u64, x: f32, z: f32) -> AssistTarget {
        AssistTarget {
            id,
            bottom: Vec3::new(x, 0.3, z),
            top: Vec3::new(x, 1.5, z),
            radius: 0.3,
        }
    }

    fn view_at(eye: Vec3, look_at: Vec3, active: bool) -> AssistView {
        let b = bearing(eye, look_at);
        AssistView {
            eye,
            yaw: b.x,
            pitch: b.y,
            ads: 0.0,
            active,
        }
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
        let params = AimAssistParams::default();
        let target = dummy(1, 0.0, -10.0);
        let eye = Vec3::new(0.0, 1.0, 0.0);
        let mut assist = AimAssist::default();

        let on = assist.update(&view_at(eye, target.center(), false), &[target], &params);
        assert_eq!(on.target, Some(1));
        assert!((on.slowdown - params.slowdown_hip).abs() < 1e-5);

        let near = assist.update(
            &view_at(eye, Vec3::new(0.6, 1.0, -10.0), false),
            &[target],
            &params,
        );
        assert!(near.slowdown > on.slowdown && near.slowdown < 1.0);

        let off = assist.update(
            &view_at(eye, Vec3::new(3.0, 1.0, -10.0), false),
            &[target],
            &params,
        );
        assert_eq!(off, AssistOutput::NONE);
    }

    #[test]
    fn aiming_down_sights_slows_more() {
        let params = AimAssistParams::default();
        let target = dummy(1, 0.0, -10.0);
        let mut view = view_at(Vec3::new(0.0, 1.0, 0.0), target.center(), false);
        view.ads = 1.0;
        let out = AimAssist::default().update(&view, &[target], &params);
        assert!((out.slowdown - params.slowdown_ads).abs() < 1e-5);
    }

    #[test]
    fn out_of_range_and_behind_get_nothing() {
        let params = AimAssistParams::default();
        let eye = Vec3::new(0.0, 1.0, 0.0);
        let far = dummy(1, 0.0, -100.0);
        let behind = dummy(2, 0.0, 10.0);
        let view = view_at(eye, far.center(), true);
        let out = AimAssist::default().update(&view, &[far, behind], &params);
        assert_eq!(out, AssistOutput::NONE);
    }

    #[test]
    fn picks_the_target_nearest_the_crosshair() {
        let params = AimAssistParams::default();
        let eye = Vec3::new(0.0, 1.0, 0.0);
        let a = dummy(1, 0.5, -10.0);
        let b = dummy(2, -0.1, -10.0);
        let view = view_at(eye, Vec3::new(0.0, 1.0, -10.0), false);
        let out = AimAssist::default().update(&view, &[a, b], &params);
        assert_eq!(out.target, Some(2));
    }

    /// Strafing past a target: the view follows part of its relative motion,
    /// but only while the player is giving input.
    #[test]
    fn rotation_follows_relative_motion_while_active() {
        let params = AimAssistParams::default();
        let target = dummy(1, 0.0, -10.0);
        for active in [true, false] {
            let mut assist = AimAssist::default();
            let mut eye = Vec3::new(0.0, 1.0, 0.0);
            let mut view = view_at(eye, target.center(), active);
            // A short strafe: long enough to matter, short enough that the
            // crosshair stays in the assist area without stick corrections.
            for _ in 0..10 {
                eye.x += 4.0 * DT;
                view.eye = eye;
                let out = assist.update(&view, &[target], &params);
                view.yaw += out.rotation.x;
                view.pitch += out.rotation.y;
            }
            let error = (bearing(eye, target.center()).x - view.yaw).abs();
            let unassisted = bearing(eye, target.center()).x.abs();
            if active {
                assert!(view.yaw > 0.0, "turns left toward a target on the left");
                assert!(error < unassisted * 0.8, "{error} vs {unassisted}");
            } else {
                assert_eq!(
                    view.yaw,
                    bearing(Vec3::new(0.0, 1.0, 0.0), target.center()).x
                );
            }
        }
    }

    /// Turning the view must not create rotation on its own.
    #[test]
    fn turning_alone_adds_no_rotation() {
        let params = AimAssistParams::default();
        let target = dummy(1, 0.0, -10.0);
        let eye = Vec3::new(0.0, 1.0, 0.0);
        let mut assist = AimAssist::default();
        let mut view = view_at(eye, target.center(), true);
        for _ in 0..10 {
            view.yaw += 0.002;
            let out = assist.update(&view, &[target], &params);
            assert_eq!(out.rotation, Vec2::ZERO);
        }
    }

    #[test]
    fn switching_targets_does_not_jump() {
        let params = AimAssistParams::default();
        let eye = Vec3::new(0.0, 1.0, 0.0);
        let a = dummy(1, 0.0, -10.0);
        let b = dummy(2, 0.0, -20.0);
        let mut assist = AimAssist::default();
        let view = view_at(eye, a.center(), true);
        assist.update(&view, &[a], &params);
        let out = assist.update(&view, &[b], &params);
        assert_eq!(out.target, Some(2));
        assert_eq!(out.rotation, Vec2::ZERO);
    }

    #[test]
    fn wrap_angle_stays_in_range() {
        use std::f32::consts::PI;
        assert!((wrap_angle(2.0 * PI - 0.1) + 0.1).abs() < 1e-5);
        assert!((wrap_angle(-2.0 * PI + 0.1) - 0.1).abs() < 1e-5);
    }
}
