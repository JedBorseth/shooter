//! Wound marks: where a character is hit, its skin and clothes darken and
//! redden round the hit for a few seconds, then fade (`shaders/character.gdshader`).
//!
//! Each mark is kept on the bone nearest the hit, so it moves with the body,
//! and handed to the shader as a point in the mesh's own space (where the
//! shader sees skinned vertices) plus a strength: four instance uniforms,
//! `wound_0` to `wound_3`. The shader does a distance test per vertex, so a
//! mark costs nothing until a character has one, and very little after.
//! Corpses keep theirs.

use godot::classes::{MeshInstance3D, Skeleton3D};
use godot::prelude::*;

/// How many marks a character shows at once; a fifth replaces the oldest.
pub const SLOTS: usize = 4;
/// Seconds a mark holds at full strength, then takes to fade.
const HOLD_SECONDS: f32 = 5.0;
const FADE_SECONDS: f32 = 3.0;
/// The bones a mark can ride on: the hit zones' bones and the hands and feet.
const BONES: [&str; 18] = [
    "Hips",
    "Spine",
    "Chest",
    "UpperChest",
    "Neck",
    "Head",
    "LeftUpperArm",
    "LeftLowerArm",
    "LeftHand",
    "RightUpperArm",
    "RightLowerArm",
    "RightHand",
    "LeftUpperLeg",
    "LeftLowerLeg",
    "LeftFoot",
    "RightUpperLeg",
    "RightLowerLeg",
    "RightFoot",
];
const UNIFORMS: [&str; SLOTS] = ["wound_0", "wound_1", "wound_2", "wound_3"];

#[derive(Clone, Copy, Debug)]
struct Mark {
    bone: i32,
    /// The hit, in the bone's space.
    local: Vector3,
    age: f32,
    /// 0 to 1: how dark the mark is at full strength (heavier hits, darker).
    depth: f32,
}

#[derive(Default)]
pub struct Wounds {
    marks: [Option<Mark>; SLOTS],
    next: usize,
    /// Which uniforms hold a mark now, so emptied ones are cleared once.
    shown: [bool; SLOTS],
}

/// A mark's strength `age` seconds after the hit.
pub fn strength(age: f32) -> f32 {
    (1.0 - (age - HOLD_SECONDS) / FADE_SECONDS).clamp(0.0, 1.0)
}

/// How dark a hit of force level `force` (0 to 15; a rifle round is about 4)
/// marks.
pub fn depth_of(force: f32) -> f32 {
    (0.6 + 0.4 * (force / 8.0)).clamp(0.6, 1.0)
}

/// The index of the segment (from, to) nearest `point`.
fn nearest_segment(point: Vector3, segments: &[(Vector3, Vector3)]) -> Option<usize> {
    let distance = |(a, b): &(Vector3, Vector3)| {
        let ab = *b - *a;
        let t = if ab.length_squared() > 1e-8 {
            ((point - *a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (point - (*a + ab * t)).length()
    };
    segments
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| distance(a).total_cmp(&distance(b)))
        .map(|(i, _)| i)
}

impl Wounds {
    /// A mark where a shot hit `point` (world space).
    pub fn add(&mut self, skeleton: &Gd<Skeleton3D>, point: Vector3, depth: f32) {
        let in_skeleton = skeleton.get_global_transform().affine_inverse() * point;
        let mut bones = Vec::new();
        let mut segments = Vec::new();
        for name in BONES {
            let bone = skeleton.find_bone(name);
            if bone < 0 {
                continue;
            }
            let from = skeleton.get_bone_global_pose(bone).origin;
            // To the bone's first child, or a point past its end for a tip.
            let to = skeleton
                .get_bone_children(bone)
                .as_slice()
                .first()
                .map_or(from, |&child| skeleton.get_bone_global_pose(child).origin);
            bones.push(bone);
            segments.push((from, to));
        }
        let Some(nearest) = nearest_segment(in_skeleton, &segments) else {
            return;
        };
        let bone = bones[nearest];
        let local = skeleton.get_bone_global_pose(bone).affine_inverse() * in_skeleton;
        self.marks[self.next] = Some(Mark {
            bone,
            local,
            age: 0.0,
            depth,
        });
        self.next = (self.next + 1) % SLOTS;
    }

    /// Whether there is anything to draw or to clear.
    pub fn active(&self) -> bool {
        self.marks.iter().any(Option::is_some) || self.shown.iter().any(|&s| s)
    }

    /// Forgets every mark (a respawn).
    pub fn clear(&mut self) {
        self.marks = [None; SLOTS];
    }

    /// Ages the marks (unless `hold`, for the dead) and hands them to every
    /// mesh under `skeleton`.
    pub fn update(&mut self, skeleton: &Gd<Skeleton3D>, dt: f32, hold: bool) {
        if !self.active() {
            return;
        }
        let meshes: Vec<Gd<MeshInstance3D>> = skeleton
            .get_children()
            .iter_shared()
            .filter_map(|child| child.try_cast::<MeshInstance3D>().ok())
            .collect();
        let skeleton_to_world = skeleton.get_global_transform();
        for (slot, mark) in self.marks.iter_mut().enumerate() {
            let value = match mark {
                Some(m) => {
                    if !hold {
                        m.age += dt;
                    }
                    let s = strength(m.age) * m.depth;
                    if s <= 0.0 {
                        *mark = None;
                        None
                    } else {
                        let world =
                            skeleton_to_world * (skeleton.get_bone_global_pose(m.bone) * m.local);
                        Some((world, s))
                    }
                }
                None => None,
            };
            if value.is_none() && !self.shown[slot] {
                continue;
            }
            self.shown[slot] = value.is_some();
            for mesh in &meshes {
                let v = match value {
                    // In the mesh's own space, where the shader sees vertices.
                    Some((world, s)) => {
                        let p = mesh.get_global_transform().affine_inverse() * world;
                        Vector4::new(p.x, p.y, p.z, s)
                    }
                    None => Vector4::ZERO,
                };
                mesh.clone()
                    .set_instance_shader_parameter(UNIFORMS[slot], &v.to_variant());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mark_holds_then_fades() {
        assert_eq!(strength(0.0), 1.0);
        assert_eq!(strength(HOLD_SECONDS), 1.0);
        assert!((strength(HOLD_SECONDS + FADE_SECONDS / 2.0) - 0.5).abs() < 1e-5);
        assert_eq!(strength(HOLD_SECONDS + FADE_SECONDS), 0.0);
    }

    #[test]
    fn heavier_hits_mark_darker() {
        assert_eq!(depth_of(0.0), 0.6);
        assert!(depth_of(4.0) > depth_of(1.0));
        assert_eq!(depth_of(15.0), 1.0);
    }

    #[test]
    fn a_hit_rides_the_nearest_bone() {
        let segments = [
            (Vector3::new(0.0, 1.0, 0.0), Vector3::new(0.0, 1.5, 0.0)),
            (Vector3::new(0.2, 1.4, 0.0), Vector3::new(0.5, 1.4, 0.0)),
        ];
        assert_eq!(
            nearest_segment(Vector3::new(0.05, 1.2, 0.1), &segments),
            Some(0)
        );
        assert_eq!(
            nearest_segment(Vector3::new(0.4, 1.45, 0.0), &segments),
            Some(1)
        );
        assert_eq!(nearest_segment(Vector3::ZERO, &[]), None);
    }
}
