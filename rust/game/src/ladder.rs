//! Climbable ladders in a map: [`LadderVolume`] marks one, and [`find`]
//! reports where a player's feet are relative to it for the simulation's
//! ladder rules (`shooter_sim::movement::Ladder`).
//!
//! Map markup: put a `LadderVolume` at the foot of the ladder, on the face
//! of its rungs, with its -Z axis pointing the way a climber faces (into the
//! rungs) and `length` set to the height of the floor at its top. Keep the
//! ladder's own collision (the rungs' plate) no higher than that floor, so
//! a climber can step over it at the top. The volume builds its own shape.

use godot::classes::{
    Area3D, BoxShape3D, CollisionShape3D, IArea3D, PhysicsDirectSpaceState3D,
    PhysicsPointQueryParameters3D,
};
use godot::prelude::*;
use shooter_sim::movement::Ladder;

/// Collision layer 9 ("ladders"), as a bit mask: only ladder volumes.
pub const LADDER_LAYER: u32 = 1 << 8;

/// How far in front of the rungs' face a climber's feet (the capsule's
/// centre line) stand: the capsule's radius and a sliver.
pub const CLIMB_LINE: f32 = 0.37;

/// The volume around a ladder in which it can be grabbed: from this far in
/// front of the climbing line to this far behind it (over the top floor)...
const REACH_FRONT: f32 = 0.6;
const REACH_BEHIND: f32 = 1.0;
/// ...this far to either side of the rungs, and from this far below the foot
/// to this far over the top.
const REACH_SIDE: f32 = 0.2;
const REACH_BELOW: f32 = 0.3;
const REACH_ABOVE: f32 = 0.6;

#[derive(GodotClass)]
#[class(init, base = Area3D)]
pub struct LadderVolume {
    base: Base<Area3D>,
    /// Height of the floor at the ladder's top above its foot (metres).
    #[export]
    #[init(val = 3.0)]
    pub length: f32,
    /// Width between the stiles (metres).
    #[export]
    #[init(val = 0.5)]
    pub width: f32,
    /// Height of the lowest rung above the foot, and the rungs' spacing.
    #[export]
    #[init(val = 0.3)]
    pub first_rung: f32,
    #[export]
    #[init(val = 0.3)]
    pub rung_spacing: f32,
}

#[godot_api]
impl IArea3D for LadderVolume {
    fn ready(&mut self) {
        let (length, width) = (self.length, self.width);
        let mut area = self.base_mut();
        area.set_collision_layer(LADDER_LAYER);
        area.set_collision_mask(0);
        area.set_monitoring(false);
        let mut box_shape = BoxShape3D::new_gd();
        let tall = REACH_BELOW + length + REACH_ABOVE;
        let deep = REACH_FRONT + REACH_BEHIND;
        box_shape.set_size(Vector3::new(width + 2.0 * REACH_SIDE, tall, deep));
        let mut shape = CollisionShape3D::new_alloc();
        shape.set_name("Reach");
        shape.set_shape(&box_shape);
        // The climbing line is at +Z (in front of the rungs).
        shape.set_position(Vector3::new(
            0.0,
            tall / 2.0 - REACH_BELOW,
            CLIMB_LINE + REACH_FRONT - deep / 2.0,
        ));
        area.add_child(&shape);
    }
}

/// The ladder whose volume holds `feet`, if any, as the simulation sees it.
pub fn find(space: &mut Gd<PhysicsDirectSpaceState3D>, feet: Vector3) -> Option<Ladder> {
    find_volume(space, feet).map(|(_, ladder)| ladder)
}

/// [`find`], with the volume found.
pub fn find_volume(
    space: &mut Gd<PhysicsDirectSpaceState3D>,
    feet: Vector3,
) -> Option<(Gd<LadderVolume>, Ladder)> {
    let mut query = PhysicsPointQueryParameters3D::new_gd();
    query.set_position(feet + Vector3::UP * 0.05);
    query.set_collision_mask(LADDER_LAYER);
    query.set_collide_with_areas(true);
    query.set_collide_with_bodies(false);
    let hits = space.intersect_point_ex(&query).max_results(1).done();
    let hit = hits.get(0)?;
    let ladder = hit.get("collider")?.try_to::<Gd<LadderVolume>>().ok()?;
    let length = ladder.bind().length;
    let transform = ladder.get_global_transform().orthonormalized();
    let local = transform.affine_inverse() * feet;
    let into = -transform.basis.col_c();
    let facing = shooter_sim::Vec2::new(into.x, into.z).normalize_or_zero();
    (facing != shooter_sim::Vec2::ZERO).then_some((
        ladder,
        Ladder {
            facing,
            depth: CLIMB_LINE - local.z,
            lateral: local.x,
            height: local.y,
            length,
        },
    ))
}

/// The rungs of a ladder, for posing a climber's hands and feet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rungs {
    pub first: f32,
    pub spacing: f32,
}

impl LadderVolume {
    pub fn rungs(&self) -> Rungs {
        Rungs {
            first: self.first_rung,
            spacing: self.rung_spacing.max(0.1),
        }
    }
}

/// Where a climber's hands and feet go (world space): wrists right and
/// left, ankles left and right, for feet at `feet` on `ladder`.
///
/// The limbs climb in diagonal pairs, as people climb ladders: the right
/// hand with the left foot, then the left hand with the right foot, each
/// limb taking two rungs per move. A limb holds its rung until the body has
/// climbed most of the way to the next, then moves there in an arc away
/// from the ladder. Going down runs the same moves backward.
pub fn limb_targets(feet: Vector3, ladder: &Ladder, rungs: Rungs) -> ([Vector3; 2], [Vector3; 2]) {
    let facing = Vector3::new(ladder.facing.x, 0.0, ladder.facing.y);
    let right = Vector3::new(-ladder.facing.y, 0.0, ladder.facing.x);
    let base = feet.y - ladder.height;
    // The middle of the rungs' face, level with the feet.
    let face = feet + facing * (CLIMB_LINE - ladder.depth) - right * ladder.lateral;
    let step = rungs.spacing * 2.0;
    let top = rungs.first
        + ((ladder.length - 0.1 - rungs.first) / rungs.spacing)
            .floor()
            .max(0.0)
            * rungs.spacing;
    let limb = |reach: f32, phase: f32, side: f32, inset: f32, above: f32| {
        let ideal = ladder.height + reach - rungs.first - phase;
        let u = ideal / step;
        let k = u.floor();
        let f = u - k;
        let moving = crate::weapon_clips::smoothstep(((f - 0.65) / 0.35).clamp(0.0, 1.0));
        let rung = (rungs.first + phase + (k + moving) * step).clamp(rungs.first, top);
        let away = (std::f32::consts::PI * moving).sin() * 0.1;
        Vector3::new(face.x, base + rung + above, face.z) + right * side - facing * (inset + away)
    };
    let gap = rungs.spacing;
    // Hands hold rungs between shoulder and a little over the head; the
    // wrists sit just behind the rung they wrap.
    let hands = [
        limb(1.75, 0.0, 0.17, 0.07, 0.0),
        limb(1.75, gap, -0.17, 0.07, 0.0),
    ];
    // Feet stand on rungs from the feet up to two rungs higher; the ankle is
    // above and behind where the sole meets the rung.
    let ankles = [
        limb(0.6, 0.0, -0.11, 0.13, 0.08),
        limb(0.6, gap, 0.11, 0.13, 0.08),
    ];
    (hands, ankles)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ladder(height: f32) -> Ladder {
        Ladder {
            facing: shooter_sim::Vec2::new(1.0, 0.0),
            depth: 0.0,
            lateral: 0.0,
            height,
            length: 4.14,
        }
    }

    const RUNGS: Rungs = Rungs {
        first: 0.3,
        spacing: 0.3,
    };

    #[test]
    fn hands_and_feet_rest_on_rungs_in_diagonal_pairs() {
        let feet = Vector3::new(-CLIMB_LINE, 1.0, 0.0);
        let (hands, ankles) = limb_targets(feet, &ladder(1.0), RUNGS);
        let on_rung = |y: f32| ((y - 0.3) / 0.3 - ((y - 0.3) / 0.3).round()).abs() < 0.02;
        for h in hands {
            assert!(on_rung(h.y), "{h}");
            assert!((h.y - 1.0) > 1.0 && (h.y - 1.0) <= 1.76, "a hand at {h}");
            // Just in front of the rungs (at x = 0), off to its side.
            assert!(h.x < 0.0 && h.x > -0.2, "{h}");
        }
        for a in ankles {
            assert!(on_rung(a.y - 0.08), "{a}");
            assert!(a.y - 1.0 > 0.0 && a.y - 1.0 <= 0.7, "a foot at {a}");
        }
        assert!(
            hands[0].z > 0.0 && hands[1].z < 0.0,
            "right hand to the right (+Z)"
        );
        // The hands hold different rungs, as do the feet.
        assert!((hands[0].y - hands[1].y).abs() > 0.2);
        assert!((ankles[0].y - ankles[1].y).abs() > 0.2);
    }

    #[test]
    fn each_limb_moves_two_rungs_at_a_time_without_jumping() {
        let mut last: Option<([Vector3; 2], [Vector3; 2])> = None;
        let mut height = 0.0;
        while height < 3.0 {
            let feet = Vector3::new(-CLIMB_LINE, height, 0.0);
            let now = limb_targets(feet, &ladder(height), RUNGS);
            if let Some((hands, ankles)) = last {
                for (a, b) in hands.iter().chain(&ankles).zip(now.0.iter().chain(&now.1)) {
                    assert!(a.distance_to(*b) < 0.05, "a limb jumped from {a} to {b}");
                }
            }
            last = Some(now);
            height += 0.005;
        }
        // Near the top the hands stop at the top rung.
        let (hands, _) = limb_targets(Vector3::new(0.0, 3.5, 0.0), &ladder(3.5), RUNGS);
        assert!(hands.iter().all(|h| h.y <= 3.91), "{hands:?}");
    }
}
