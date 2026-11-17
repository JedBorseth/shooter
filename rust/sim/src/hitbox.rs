//! Hit zones for one player: a head sphere and capsules for the neck, chest,
//! stomach, arms, and legs, posed from what every machine knows about a player
//! (feet position, view angles, stance).
//!
//! The shapes do not come from the animated skeleton. The dedicated server
//! never runs animation, and lag compensation (Phase 3) has to rebuild the
//! hitboxes a player had some ticks ago from a snapshot. A pure function of the
//! snapshot fields makes both cheap and the same on every machine. The
//! third-person character (`rust/game/src/character`) is posed to match these
//! shapes, and the F4 debug overlay draws both together.
//!
//! The layouts were measured from the soldier model's `Idle`, `Crouch_Idle`, and
//! `Slide` poses, with the arms holding a rifle as the character's arm IK does.
//! Body space: +X is the character's right, +Y up, -Z forward, origin at the feet.

use crate::Vec3;
use crate::fmath;
use crate::movement::Stance;

/// Where a shot landed on a body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HitZone {
    Head,
    /// Neck, chest, and shoulders.
    Chest,
    Stomach,
    Arm,
    Leg,
}

impl HitZone {
    pub const ALL: [HitZone; 5] = [
        HitZone::Head,
        HitZone::Chest,
        HitZone::Stomach,
        HitZone::Arm,
        HitZone::Leg,
    ];

    /// Name used in collider metadata (`hit_zone`) and debug output.
    pub fn name(self) -> &'static str {
        match self {
            HitZone::Head => "head",
            HitZone::Chest => "chest",
            HitZone::Stomach => "stomach",
            HitZone::Arm => "arm",
            HitZone::Leg => "leg",
        }
    }

    pub fn from_name(name: &str) -> Option<HitZone> {
        HitZone::ALL.into_iter().find(|z| z.name() == name)
    }
}

/// A capsule: every point within `radius` of the segment from `a` to `b`. A
/// sphere when `a == b`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hitbox {
    pub zone: HitZone,
    pub a: Vec3,
    pub b: Vec3,
    pub radius: f32,
}

/// Everything the hitboxes depend on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitboxPose {
    /// Feet position, world space.
    pub feet: Vec3,
    /// View yaw and pitch in radians, as in [`crate::player`] (pitch up is positive).
    pub yaw: f32,
    pub pitch: f32,
    pub stance: Stance,
}

/// Number of shapes [`hitboxes`] returns.
pub const COUNT: usize = 13;

/// Body-space points for one stance.
struct Layout {
    head: Vec3,
    /// The head turns with the pitch around this point (the top of the neck).
    neck_top: Vec3,
    neck_base: Vec3,
    chest: (Vec3, Vec3),
    stomach: (Vec3, Vec3),
    /// The arms turn with the pitch around this point (between the shoulders).
    shoulders: Vec3,
    /// Hip, knee, and ankle, left leg then right leg.
    legs: [[Vec3; 3]; 2],
}

const fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

const STANDING: Layout = Layout {
    head: v(0.0, 1.665, -0.02),
    neck_top: v(0.0, 1.53, 0.0),
    neck_base: v(0.0, 1.44, 0.02),
    chest: (v(0.0, 1.2, 0.05), v(0.0, 1.36, 0.04)),
    stomach: (v(0.0, 0.95, 0.07), v(0.0, 1.08, 0.06)),
    shoulders: v(0.0, 1.4, 0.04),
    legs: [
        [
            v(-0.09, 0.88, 0.05),
            v(-0.11, 0.51, 0.0),
            v(-0.13, 0.1, 0.03),
        ],
        [v(0.09, 0.88, 0.05), v(0.11, 0.51, 0.0), v(0.13, 0.1, 0.03)],
    ],
};

const CROUCHING: Layout = Layout {
    head: v(0.0, 0.98, -0.18),
    neck_top: v(0.0, 0.87, -0.12),
    neck_base: v(0.0, 0.82, -0.04),
    chest: (v(0.0, 0.66, 0.13), v(0.0, 0.8, 0.04)),
    stomach: (v(0.0, 0.48, 0.25), v(0.0, 0.6, 0.19)),
    shoulders: v(0.0, 0.85, -0.02),
    legs: [
        [
            v(-0.12, 0.45, 0.22),
            v(-0.19, 0.5, -0.16),
            v(-0.2, 0.1, -0.2),
        ],
        [v(0.06, 0.49, 0.23), v(0.15, 0.28, -0.1), v(0.16, 0.17, 0.3)],
    ],
};

const SLIDING: Layout = Layout {
    head: v(-0.08, 0.58, 0.42),
    neck_top: v(-0.1, 0.48, 0.4),
    neck_base: v(-0.08, 0.4, 0.36),
    chest: (v(0.0, 0.22, 0.22), v(-0.05, 0.34, 0.34)),
    stomach: (v(0.0, 0.15, 0.0), v(0.0, 0.17, 0.12)),
    shoulders: v(-0.05, 0.38, 0.4),
    legs: [
        [
            v(-0.11, 0.12, -0.01),
            v(-0.4, 0.34, -0.14),
            v(-0.16, 0.11, -0.41),
        ],
        [
            v(0.07, 0.12, -0.02),
            v(0.06, 0.18, -0.42),
            v(0.05, 0.1, -0.84),
        ],
    ],
};

/// Arm joints relative to [`Layout::shoulders`] with level aim: shoulder,
/// elbow, and hand for the left arm, then the right. The right hand is on the
/// grip and the left on the handguard.
const ARMS: [[Vec3; 3]; 2] = [
    [
        v(-0.19, 0.0, 0.0),
        v(-0.21, -0.2, -0.2),
        v(-0.03, -0.1, -0.46),
    ],
    [
        v(0.19, 0.0, 0.0),
        v(0.25, -0.24, -0.08),
        v(0.09, -0.13, -0.28),
    ],
];

/// Radii: head, neck, chest, stomach, upper arm, forearm, thigh, shin.
const HEAD_RADIUS: f32 = 0.135;
const NECK_RADIUS: f32 = 0.065;
const CHEST_RADIUS: f32 = 0.2;
const STOMACH_RADIUS: f32 = 0.17;
const UPPER_ARM_RADIUS: f32 = 0.055;
const FOREARM_RADIUS: f32 = 0.05;
const THIGH_RADIUS: f32 = 0.085;
const SHIN_RADIUS: f32 = 0.065;

fn layout(stance: Stance) -> &'static Layout {
    match stance {
        Stance::Standing => &STANDING,
        Stance::Crouching => &CROUCHING,
        Stance::Sliding => &SLIDING,
    }
}

/// Rotates `p` about the X axis through `pivot` by `angle` (positive tilts -Z up).
fn pitch_about(p: Vec3, pivot: Vec3, angle: f32) -> Vec3 {
    let (s, c) = fmath::sin_cos(angle);
    let d = p - pivot;
    pivot + Vec3::new(d.x, d.y * c - d.z * s, d.y * s + d.z * c)
}

/// The shapes for a pose, in world space.
pub fn hitboxes(pose: &HitboxPose) -> [Hitbox; COUNT] {
    let l = layout(pose.stance);
    let (s, c) = fmath::sin_cos(pose.yaw);
    // Yaw turns -Z (forward) toward -X for positive yaw, as `movement::basis_from_yaw`.
    let world = |p: Vec3| pose.feet + Vec3::new(p.x * c + p.z * s, p.y, -p.x * s + p.z * c);
    let pitch = pose.pitch.clamp(-1.4, 1.4);
    let head = pitch_about(l.head, l.neck_top, pitch);
    // The arms follow the aim fully; the chest leans a little into it.
    let arm = |p: Vec3| pitch_about(l.shoulders + p, l.shoulders, pitch);
    let shape = |zone, a: Vec3, b: Vec3, radius| Hitbox {
        zone,
        a: world(a),
        b: world(b),
        radius,
    };
    let [left_leg, right_leg] = l.legs;
    let [left_arm, right_arm] = ARMS;
    [
        shape(HitZone::Head, head, head, HEAD_RADIUS),
        shape(HitZone::Chest, l.neck_base, l.neck_top, NECK_RADIUS),
        shape(HitZone::Chest, l.chest.0, l.chest.1, CHEST_RADIUS),
        shape(HitZone::Stomach, l.stomach.0, l.stomach.1, STOMACH_RADIUS),
        shape(
            HitZone::Arm,
            arm(left_arm[0]),
            arm(left_arm[1]),
            UPPER_ARM_RADIUS,
        ),
        shape(
            HitZone::Arm,
            arm(left_arm[1]),
            arm(left_arm[2]),
            FOREARM_RADIUS,
        ),
        shape(
            HitZone::Arm,
            arm(right_arm[0]),
            arm(right_arm[1]),
            UPPER_ARM_RADIUS,
        ),
        shape(
            HitZone::Arm,
            arm(right_arm[1]),
            arm(right_arm[2]),
            FOREARM_RADIUS,
        ),
        shape(HitZone::Leg, left_leg[0], left_leg[1], THIGH_RADIUS),
        shape(HitZone::Leg, left_leg[1], left_leg[2], SHIN_RADIUS),
        shape(HitZone::Leg, right_leg[0], right_leg[1], THIGH_RADIUS),
        shape(HitZone::Leg, right_leg[1], right_leg[2], SHIN_RADIUS),
        // The hips, between the stomach and the legs.
        shape(
            HitZone::Stomach,
            (left_leg[0] + l.stomach.0) * 0.5,
            (right_leg[0] + l.stomach.0) * 0.5,
            STOMACH_RADIUS * 0.85,
        ),
    ]
}

/// The nearest shape a ray hits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub zone: HitZone,
    /// Distance along the ray.
    pub distance: f32,
    /// Index into the hitbox array.
    pub index: usize,
}

/// Casts a ray from `origin` along `direction` (need not be normalised) up to
/// `max_distance` and returns the first shape it enters. A ray that starts inside
/// a shape hits it at distance 0.
pub fn raycast(
    boxes: &[Hitbox],
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
) -> Option<RayHit> {
    let length = direction.length();
    if length <= f32::EPSILON {
        return None;
    }
    let dir = direction / length;
    boxes
        .iter()
        .enumerate()
        .filter_map(|(index, b)| {
            ray_capsule(origin, dir, b.a, b.b, b.radius)
                .filter(|&t| t <= max_distance)
                .map(|distance| RayHit {
                    zone: b.zone,
                    distance,
                    index,
                })
        })
        .min_by(|x, y| x.distance.total_cmp(&y.distance))
}

/// Distance along a unit ray to the first point inside a capsule, if any.
fn ray_capsule(origin: Vec3, dir: Vec3, a: Vec3, b: Vec3, radius: f32) -> Option<f32> {
    let axis = b - a;
    let axis_len_sq = axis.length_squared();
    if axis_len_sq <= f32::EPSILON {
        return ray_sphere(origin, dir, a, radius);
    }
    // Inside already?
    if distance_sq_to_segment(origin, a, b) <= radius * radius {
        return Some(0.0);
    }
    // Infinite cylinder around the axis, then clip to the segment; the caps are spheres.
    let ao = origin - a;
    let d_axis = dir.dot(axis) / axis_len_sq;
    let o_axis = ao.dot(axis) / axis_len_sq;
    let d_perp = dir - axis * d_axis;
    let o_perp = ao - axis * o_axis;
    let qa = d_perp.length_squared();
    let qb = 2.0 * d_perp.dot(o_perp);
    let qc = o_perp.length_squared() - radius * radius;
    let mut best: Option<f32> = None;
    if qa > f32::EPSILON {
        let disc = qb * qb - 4.0 * qa * qc;
        if disc >= 0.0 {
            let t = (-qb - disc.sqrt()) / (2.0 * qa);
            let along = o_axis + t * d_axis;
            if t >= 0.0 && (0.0..=1.0).contains(&along) {
                best = Some(t);
            }
        }
    }
    for cap in [a, b] {
        if let Some(t) = ray_sphere(origin, dir, cap, radius) {
            best = Some(best.map_or(t, |x| x.min(t)));
        }
    }
    best
}

fn ray_sphere(origin: Vec3, dir: Vec3, centre: Vec3, radius: f32) -> Option<f32> {
    let oc = origin - centre;
    let c = oc.length_squared() - radius * radius;
    if c <= 0.0 {
        return Some(0.0);
    }
    let b = oc.dot(dir);
    if b > 0.0 {
        return None;
    }
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    Some(-b - disc.sqrt())
}

fn distance_sq_to_segment(p: Vec3, a: Vec3, b: Vec3) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    (a + ab * t - p).length_squared()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pose(stance: Stance) -> HitboxPose {
        HitboxPose {
            feet: Vec3::new(3.0, 0.5, -2.0),
            yaw: 0.0,
            pitch: 0.0,
            stance,
        }
    }

    /// Shoots straight down -Z (at a player facing +Z toward the shooter... or
    /// away; capsules are symmetric enough) at a height and returns the zone.
    fn zone_at(pose: &HitboxPose, x: f32, y: f32) -> Option<HitZone> {
        let origin = pose.feet + Vec3::new(x, y, 10.0);
        raycast(&hitboxes(pose), origin, Vec3::NEG_Z, 100.0).map(|h| h.zone)
    }

    #[test]
    fn standing_zones_by_height() {
        let p = pose(Stance::Standing);
        assert_eq!(zone_at(&p, 0.0, 1.7), Some(HitZone::Head));
        assert_eq!(zone_at(&p, 0.0, 1.3), Some(HitZone::Chest));
        assert_eq!(zone_at(&p, 0.0, 1.0), Some(HitZone::Stomach));
        assert_eq!(zone_at(&p, 0.12, 0.3), Some(HitZone::Leg));
        assert_eq!(zone_at(&p, 0.0, 1.95), None, "over the head");
        assert_eq!(zone_at(&p, 0.0, 0.3), None, "between the legs");
        assert_eq!(zone_at(&p, 0.6, 1.2), None, "beside the body");
    }

    #[test]
    fn crouching_lowers_the_head() {
        let p = pose(Stance::Crouching);
        assert_eq!(zone_at(&p, 0.0, 1.7), None);
        // From behind, the hunched shoulders cover the lower half of the head.
        assert_eq!(zone_at(&p, 0.0, 1.05), Some(HitZone::Head));
        assert_eq!(zone_at(&p, 0.0, 0.78), Some(HitZone::Chest));
    }

    #[test]
    fn sliding_is_low() {
        let p = pose(Stance::Sliding);
        assert_eq!(zone_at(&p, 0.0, 1.0), None);
        assert!(zone_at(&p, -0.08, 0.58).is_some());
    }

    #[test]
    fn arms_hold_the_weapon_in_front() {
        let p = pose(Stance::Standing);
        // From the side, at hand height and in front of the chest: an arm.
        let origin = p.feet + Vec3::new(-5.0, 1.29, -0.4);
        let hit = raycast(&hitboxes(&p), origin, Vec3::X, 100.0).unwrap();
        assert_eq!(hit.zone, HitZone::Arm);
    }

    #[test]
    fn yaw_turns_the_body() {
        // Facing +X (yaw -90°): the arms reach toward +X, so a ray from the
        // front at hand height meets an arm before the chest.
        let mut p = pose(Stance::Standing);
        p.yaw = -std::f32::consts::FRAC_PI_2;
        let boxes = hitboxes(&p);
        let hand = boxes[5].b - p.feet;
        assert!(hand.x > 0.3, "left hand reaches forward along +X: {hand:?}");
        assert!(hand.z.abs() < 0.1, "{hand:?}");
    }

    #[test]
    fn pitch_moves_the_head_and_arms() {
        let mut up = pose(Stance::Standing);
        up.pitch = 0.8;
        let level = hitboxes(&pose(Stance::Standing));
        let raised = hitboxes(&up);
        assert!(
            raised[5].b.y > level[5].b.y + 0.2,
            "hands rise when aiming up"
        );
        assert!(raised[0].a.z > level[0].a.z, "the head tips back");
        assert_eq!(raised[2], level[2], "the chest stays put");
    }

    #[test]
    fn nearest_shape_wins() {
        let p = pose(Stance::Standing);
        // Straight down onto the head: the head is hit before the chest under it.
        let origin = p.feet + Vec3::new(0.0, 3.0, -0.02);
        let hit = raycast(&hitboxes(&p), origin, Vec3::NEG_Y, 10.0).unwrap();
        assert_eq!(hit.zone, HitZone::Head);
        assert!((hit.distance - (3.0 - 1.665 - HEAD_RADIUS)).abs() < 1e-3);
    }

    #[test]
    fn max_distance_and_inside() {
        let p = pose(Stance::Standing);
        let boxes = hitboxes(&p);
        let origin = p.feet + Vec3::new(0.0, 1.3, 10.0);
        assert!(raycast(&boxes, origin, Vec3::NEG_Z, 5.0).is_none());
        let inside = p.feet + Vec3::new(0.0, 1.3, 0.04);
        assert_eq!(raycast(&boxes, inside, Vec3::Y, 1.0).unwrap().distance, 0.0);
        assert!(raycast(&boxes, origin, Vec3::ZERO, 50.0).is_none());
    }

    #[test]
    fn ray_capsule_matches_cylinder_and_caps() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(0.0, 1.0, 0.0);
        // Side of the cylinder.
        let t = ray_capsule(Vec3::new(-5.0, 0.5, 0.0), Vec3::X, a, b, 0.2).unwrap();
        assert!((t - 4.8).abs() < 1e-5);
        // Top cap.
        let t = ray_capsule(Vec3::new(0.0, 5.0, 0.0), Vec3::NEG_Y, a, b, 0.2).unwrap();
        assert!((t - 3.8).abs() < 1e-5);
        // Miss.
        assert!(ray_capsule(Vec3::new(-5.0, 1.5, 0.0), Vec3::X, a, b, 0.2).is_none());
        // Pointing away.
        assert!(ray_capsule(Vec3::new(-5.0, 0.5, 0.0), Vec3::NEG_X, a, b, 0.2).is_none());
    }

    #[test]
    fn zone_names_round_trip() {
        for zone in HitZone::ALL {
            assert_eq!(HitZone::from_name(zone.name()), Some(zone));
        }
    }

    #[test]
    fn deterministic() {
        let mut p = pose(Stance::Crouching);
        p.yaw = 1.234;
        p.pitch = -0.3;
        assert_eq!(hitboxes(&p), hitboxes(&p));
    }
}
