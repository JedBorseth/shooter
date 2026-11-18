//! `SoldierPoser`: a skeleton modifier that runs after the animation tree and
//! turns a locomotion pose into a soldier holding a weapon:
//!
//! 1. Turns the hips toward the movement direction and the spine back toward
//!    the aim, so strafing reuses the forward run cycle.
//! 2. Bends the spine, neck, and head up or down with the aim pitch, and leans
//!    the torso away from hits.
//! 3. Places the weapon relative to the eyes (third person), or reads where
//!    another node put it (first person).
//! 4. Solves both arms with two-bone IK so the hands hold the weapon's grip and
//!    handguard markers.
//!
//! Bone names are Godot's humanoid profile; the import retargets every
//! character onto it.
//!
//! The aim, weapon, and hand numbers come from `shooter_sim::hitbox::carry`,
//! which the arm hit zones are posed from too.

use godot::classes::{ISkeletonModifier3D, Node3D, Skeleton3D, SkeletonModifier3D};
use godot::prelude::*;
pub use shooter_sim::hitbox::carry::Hold;
use shooter_sim::hitbox::carry::{self, PITCH_SHARES};
use shooter_sim::hitbox::gait::TWIST_SHARES;

use super::ik::{self, Chain};
use crate::convert::{to_godot, to_godot_basis, to_sim};

/// Where the eyes are in the head bone's space (+Y up the head, +Z forward).
const EYE_IN_HEAD: Vector3 = to_godot(carry::EYE_IN_HEAD);
/// The bones the aim pitch bends, in [`PITCH_SHARES`] order.
const PITCH_BONES: [usize; 5] = [
    Bone::SPINE,
    Bone::CHEST,
    Bone::UPPER_CHEST,
    Bone::NECK,
    Bone::HEAD,
];
/// The bones that turn back toward the aim, in [`TWIST_SHARES`] order.
const TWIST_BONES: [usize; 3] = [Bone::SPINE, Bone::CHEST, Bone::UPPER_CHEST];

/// Bone indices resolved once per skeleton.
#[derive(Clone, Copy, Debug)]
struct Bone;

impl Bone {
    const HIPS: usize = 0;
    const SPINE: usize = 1;
    const CHEST: usize = 2;
    const UPPER_CHEST: usize = 3;
    const NECK: usize = 4;
    const HEAD: usize = 5;
    const LEFT_UPPER_ARM: usize = 6;
    const LEFT_LOWER_ARM: usize = 7;
    const LEFT_HAND: usize = 8;
    const RIGHT_UPPER_ARM: usize = 9;
    const RIGHT_LOWER_ARM: usize = 10;
    const RIGHT_HAND: usize = 11;
    const NAMES: [&'static str; 12] = [
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
    ];
}

/// How the hands hold a weapon, in the weapon's space (origin at the rear
/// sight, -Z toward the muzzle, +Y up, +X right).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grips {
    /// Centre of the firing hand's grip.
    pub right: Vector3,
    /// Where the support hand holds: the handguard, or the firing hand for pistols.
    pub left: Vector3,
    /// Pistol grip rake in degrees: how far the grip leans back from vertical.
    pub rake: f32,
    /// What the support hand holds, which also sets how each hand closes.
    pub hold: Hold,
}

/// How the support hand closes for a hold.
fn support_shape(hold: Hold) -> HandShape {
    match hold {
        Hold::Handguard => HANDGUARD,
        Hold::Pump => PUMP,
        Hold::Pistol => CUP,
    }
}

impl Default for Grips {
    fn default() -> Self {
        Self {
            right: Vector3::new(0.0, -0.15, 0.0),
            left: Vector3::new(0.0, -0.06, -0.28),
            rake: 22.0,
            hold: Hold::Handguard,
        }
    }
}

impl Grips {
    /// The hand bones' bases in weapon space: +Y toward the knuckles, +Z out of
    /// the palm (`carry::Grips::hand_bases`).
    fn hand_bases(&self) -> (Basis, Basis) {
        let (right, left) = carry::Grips {
            right: to_sim(self.right),
            left: to_sim(self.left),
            rake: self.rake,
            hold: self.hold,
        }
        .hand_bases();
        (to_godot_basis(right), to_godot_basis(left))
    }
}

/// How a hand closes: flexion in degrees per joint (+X on every finger bone
/// after the humanoid retarget) for the index, middle, ring, and little
/// fingers (proximal, intermediate, distal) and the thumb (metacarpal,
/// proximal, distal). Written per kind of grip, not per weapon model, so the
/// hands fit whatever mesh a weapon gets.
type HandShape = [[f32; 3]; 5];
const FINGERS: [&str; 5] = ["Index", "Middle", "Ring", "Little", "Thumb"];

/// The firing hand on a pistol grip (about 30 mm deep): the lower three
/// fingers wrap it, the index rests on the trigger, the thumb closes over
/// the far side.
const FIRING_HAND: HandShape = [
    [25.0, 45.0, 20.0],
    [70.0, 80.0, 45.0],
    [75.0, 80.0, 45.0],
    [80.0, 80.0, 45.0],
    [20.0, 35.0, 30.0],
];
/// The support hand closed on the knife's grip: a fist with the thumb
/// wrapped over the fingers.
const KNIFE_FIST: HandShape = [
    [70.0, 80.0, 45.0],
    [75.0, 85.0, 45.0],
    [80.0, 85.0, 45.0],
    [85.0, 85.0, 45.0],
    [25.0, 40.0, 35.0],
];
/// The trigger finger laid straight along the frame, off the trigger, while
/// sprinting or reloading.
const INDEX_OFF_TRIGGER: [f32; 3] = [4.0, 6.0, 4.0];
/// The support hand under a rifle handguard (about 45 mm across): a loose C
/// with the fingertips up the far side and the thumb along the near side.
const HANDGUARD: HandShape = [
    [55.0, 60.0, 35.0],
    [60.0, 65.0, 38.0],
    [62.0, 65.0, 38.0],
    [65.0, 65.0, 38.0],
    [20.0, 20.0, 15.0],
];
/// The support hand round a shotgun's pump: a firmer wrap to drive it.
const PUMP: HandShape = [
    [55.0, 60.0, 35.0],
    [60.0, 65.0, 35.0],
    [62.0, 65.0, 35.0],
    [65.0, 65.0, 35.0],
    [25.0, 25.0, 15.0],
];
/// The support hand on a pistol: fingers wrapped over the firing hand's, the
/// thumb forward along the frame under the firing thumb.
const CUP: HandShape = [
    [62.0, 68.0, 40.0],
    [66.0, 70.0, 42.0],
    [68.0, 70.0, 42.0],
    [70.0, 70.0, 42.0],
    [15.0, 10.0, 5.0],
];

/// Where the grip sits in each hand's bone space (from the wrist).
const RIGHT_PALM: Vector3 = to_godot(carry::RIGHT_PALM);
const LEFT_PALM: Vector3 = to_godot(carry::LEFT_PALM);

/// Where the weapon goes this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum WeaponPlacement {
    /// Something else places the weapon node (the first-person viewmodel rig).
    #[default]
    External,
    /// Relative to the eyes, in the aim frame (+X right, +Y up, -Z along the aim):
    /// the rear sight's offset, and a rotation on top of the aim.
    FromEyes { offset: Vector3, rotation: Basis },
}

/// Everything the poser needs for one frame. Set by the node that owns it.
#[derive(Clone, Copy, Debug, Default)]
pub struct PoseInput {
    /// Radians the hips turn (positive turns left, seen from above). The spine
    /// turns back by the same amount.
    pub lower_yaw: f32,
    /// Aim pitch in radians, up positive.
    pub pitch: f32,
    /// Torso lean as a horizontal vector in the character's space (+X right,
    /// -Z forward); its length is the angle in radians.
    pub lean: Vector3,
    pub placement: WeaponPlacement,
    pub grips: Option<Grips>,
    /// 0 lets the animation pose the arms, 1 puts the hands on the weapon.
    pub ik: f32,
    /// While true the weapon follows the right hand instead of the aim (dead).
    pub weapon_in_hand: bool,
    /// The support hand lets go of the weapon by this much (0..1), for reloads.
    pub support_off: f32,
    /// Where the support hand goes when it lets go, in weapon space.
    pub support_target: Vector3,
    /// The trigger finger comes off the trigger by this much (0..1): sprinting
    /// or reloading.
    pub trigger_off: f32,
    /// The support hand leaves the weapon by this much (0..1) to swing the
    /// knife, held at `knife_at`.
    pub knife: f32,
    /// Where the knife is, in the weapon's space (see `crate::knife`).
    pub knife_at: Transform3D,
}

#[derive(GodotClass)]
#[class(init, base = SkeletonModifier3D)]
pub struct SoldierPoser {
    base: Base<SkeletonModifier3D>,
    pub input: PoseInput,
    /// The weapon node the hands hold.
    pub weapon: Option<Gd<Node3D>>,
    bones: Option<[i32; 12]>,
    /// Finger bones of both hands: bone, rest rotation, right hand, finger
    /// (index into `FINGERS`), and joint.
    fingers: Vec<(i32, Quaternion, bool, usize, usize)>,
    /// The weapon's transform relative to the right hand, captured when the
    /// weapon starts following the hand.
    in_hand: Option<Transform3D>,
    /// Where the IK last put the wrists (world space): right, left. Godot drops
    /// modified poses after skinning, so this is how tests see the result.
    solved: [Option<Vector3>; 2],
    /// The knife, put in the support hand while it swings one.
    pub knife: Option<Gd<Node3D>>,
}

#[godot_api]
impl ISkeletonModifier3D for SoldierPoser {
    fn process_modification_with_delta(&mut self, _delta: f64) {
        let Some(mut skel) = self.base().get_skeleton() else {
            return;
        };
        let Some(bones) = self.resolve(&skel) else {
            return;
        };
        let input = self.input;
        let bone = |i: usize| bones[i];
        // Hidden unless a swing below shows it.
        if let Some(node) = &mut self.knife
            && node.is_visible()
            && (input.knife <= 0.35 || input.weapon_in_hand || input.ik <= 0.0)
        {
            node.set_visible(false);
        }

        // 1. Hips toward the movement, spine back toward the aim.
        if input.lower_yaw.abs() > 1e-4 {
            rotate_global(&mut skel, bone(Bone::HIPS), Vector3::UP, input.lower_yaw);
            for (b, share) in TWIST_BONES.into_iter().zip(TWIST_SHARES) {
                rotate_global(&mut skel, bone(b), Vector3::UP, -input.lower_yaw * share);
            }
        }

        // 2. Aim pitch and hit lean. The skeleton faces +Z, so the character's
        // right is -X; pitching up turns +Z toward +Y, a negative turn about +X.
        for (b, share) in PITCH_BONES.into_iter().zip(PITCH_SHARES) {
            rotate_global(&mut skel, bone(b), Vector3::RIGHT, -input.pitch * share);
        }
        let lean = Vector3::new(-input.lean.x, 0.0, -input.lean.z);
        if let Some(dir) = lean.try_normalized() {
            let axis = Vector3::UP.cross(dir);
            let angle = lean.length();
            rotate_global(&mut skel, bone(Bone::SPINE), axis, angle * 0.5);
            rotate_global(&mut skel, bone(Bone::CHEST), axis, angle * 0.5);
        }

        // 3. The weapon, in skeleton space.
        let skel_xform = skel.get_global_transform();
        let Some(mut weapon) = self.weapon.clone() else {
            return;
        };
        let right_hand = skel.get_bone_global_pose(bone(Bone::RIGHT_HAND));
        let weapon_skel = if input.weapon_in_hand {
            let rel = *self.in_hand.get_or_insert_with(|| {
                right_hand.affine_inverse()
                    * (skel_xform.affine_inverse() * weapon.get_global_transform())
            });
            let t = right_hand * rel;
            weapon.set_global_transform(skel_xform * t);
            t
        } else {
            self.in_hand = None;
            match input.placement {
                WeaponPlacement::External => {
                    skel_xform.affine_inverse() * weapon.get_global_transform()
                }
                WeaponPlacement::FromEyes { offset, rotation } => {
                    let head = skel.get_bone_global_pose(bone(Bone::HEAD));
                    let eyes = head * EYE_IN_HEAD;
                    let aim = Basis::from_axis_angle(Vector3::RIGHT, -input.pitch)
                        * Basis::from_axis_angle(Vector3::UP, std::f32::consts::PI);
                    let t = Transform3D::new(aim * rotation, eyes + aim * offset);
                    weapon.set_global_transform(skel_xform * t);
                    t
                }
            }
        };

        // 4. Hands on the weapon.
        let Some(grips) = input.grips else {
            return;
        };
        let weight = input.ik.clamp(0.0, 1.0);
        if weight <= 0.0 || input.weapon_in_hand {
            self.solved = [None, None];
            return;
        }
        let knife = input.knife.clamp(0.0, 1.0);
        let support = support_shape(grips.hold);
        let trigger_off = input.trigger_off.clamp(0.0, 1.0);
        for &(index, rest, right, finger, joint) in &self.fingers {
            let angle = if right {
                if finger == 0 {
                    let on = FIRING_HAND[0][joint];
                    on + (INDEX_OFF_TRIGGER[joint] - on) * trigger_off
                } else {
                    FIRING_HAND[finger][joint]
                }
            } else {
                let open = support[finger][joint];
                open + (KNIFE_FIST[finger][joint] - open) * knife
            };
            let grip = rest * Quaternion::from_axis_angle(Vector3::RIGHT, angle.to_radians());
            let pose = skel.get_bone_pose_rotation(index);
            skel.set_bone_pose_rotation(index, pose.slerp(grip, weight));
        }
        let (right_basis, left_basis) = grips.hand_bases();
        // Elbow hints in the weapon's frame: the firing elbow down and out, the
        // support elbow down and in under the weapon.
        let right_pole = weapon_skel * to_godot(carry::RIGHT_POLE);
        let left_pole = weapon_skel * to_godot(carry::LEFT_POLE);
        self.place_hand(
            &mut skel,
            [
                bone(Bone::RIGHT_UPPER_ARM),
                bone(Bone::RIGHT_LOWER_ARM),
                bone(Bone::RIGHT_HAND),
            ],
            Transform3D::new(weapon_skel.basis * right_basis, weapon_skel * grips.right),
            RIGHT_PALM,
            right_pole,
            weight,
        );
        let mut left_grip = weapon_skel * grips.left;
        let mut left_hand_basis = weapon_skel.basis * left_basis;
        if input.support_off > 0.0 {
            let off = input.support_off.clamp(0.0, 1.0);
            left_grip = left_grip.lerp(weapon_skel * input.support_target, off);
            // Turn the palm toward the body while it fetches a magazine.
            let fetch = weapon_skel.basis * to_godot_basis(carry::fetch_basis());
            left_hand_basis = Basis::from_quaternion(
                left_hand_basis
                    .get_quaternion()
                    .slerp(fetch.get_quaternion(), off),
            );
        }
        // The knife: the wrist goes where the fist holds the knife at
        // `knife_at`, blended from wherever the hand was.
        let mut left_target =
            Transform3D::new(left_hand_basis, left_grip - left_hand_basis * LEFT_PALM);
        if knife > 0.0 {
            let hand = weapon_skel * crate::knife::hand_for(input.knife_at);
            let rotation = left_target
                .basis
                .get_quaternion()
                .slerp(hand.basis.orthonormalized().get_quaternion(), knife);
            left_target = Transform3D::new(
                Basis::from_quaternion(rotation),
                left_target.origin.lerp(hand.origin, knife),
            );
        }
        self.place_hand(
            &mut skel,
            [
                bone(Bone::LEFT_UPPER_ARM),
                bone(Bone::LEFT_LOWER_ARM),
                bone(Bone::LEFT_HAND),
            ],
            left_target,
            Vector3::ZERO,
            left_pole,
            weight,
        );
        if let Some(node) = &mut self.knife {
            let shown = knife > 0.35;
            if node.is_visible() != shown {
                node.set_visible(shown);
            }
            if shown {
                let hand = skel.get_bone_global_pose(bone(Bone::LEFT_HAND));
                node.set_global_transform(skel_xform * hand * crate::knife::in_left_hand());
            }
        }
        self.solved = [
            Some(skel_xform * skel.get_bone_global_pose(bone(Bone::RIGHT_HAND)).origin),
            Some(skel_xform * skel.get_bone_global_pose(bone(Bone::LEFT_HAND)).origin),
        ];
    }
}

impl SoldierPoser {
    /// Where the IK last put a wrist (world space), if it ran.
    pub fn solved_wrist(&self, right: bool) -> Option<Vector3> {
        self.solved[usize::from(!right)]
    }

    fn resolve(&mut self, skel: &Gd<Skeleton3D>) -> Option<[i32; 12]> {
        if self.bones.is_none() {
            let mut found = [0; 12];
            for (i, name) in Bone::NAMES.iter().enumerate() {
                let index = skel.find_bone(*name);
                if index < 0 {
                    godot_error!("SoldierPoser: skeleton has no bone `{name}`");
                    return None;
                }
                found[i] = index;
            }
            self.bones = Some(found);
            self.fingers.clear();
            for side in ["Left", "Right"] {
                for (f, finger) in FINGERS.iter().enumerate() {
                    let joints: [&str; 3] = if *finger == "Thumb" {
                        ["Metacarpal", "Proximal", "Distal"]
                    } else {
                        ["Proximal", "Intermediate", "Distal"]
                    };
                    for (j, joint) in joints.iter().enumerate() {
                        let index = skel.find_bone(&format!("{side}{finger}{joint}"));
                        if index >= 0 {
                            let rest = skel.get_bone_rest(index).basis.get_quaternion();
                            self.fingers.push((index, rest, side == "Right", f, j));
                        }
                    }
                }
            }
        }
        self.bones
    }

    /// Two-bone IK for one arm: the wrist goes where the hand's palm meets
    /// `grip`, the hand takes `grip`'s basis.
    #[allow(clippy::too_many_arguments)]
    fn place_hand(
        &self,
        skel: &mut Gd<Skeleton3D>,
        [upper, lower, hand]: [i32; 3],
        grip: Transform3D,
        palm: Vector3,
        pole: Vector3,
        weight: f32,
    ) {
        let wrist_target = grip.origin - grip.basis * palm;
        let chain = Chain {
            root: skel.get_bone_global_pose(upper).origin,
            mid: skel.get_bone_global_pose(lower).origin,
            end: skel.get_bone_global_pose(hand).origin,
        };
        let solution = ik::solve_two_bone(chain, wrist_target, pole);
        let ease = |q: Quaternion| Quaternion::IDENTITY.slerp(q, weight);
        apply_global_rotation(skel, upper, ease(solution.upper));
        apply_global_rotation(skel, lower, ease(solution.lower));
        let current = skel.get_bone_global_pose(hand);
        let basis = Basis::from_quaternion(
            current
                .basis
                .orthonormalized()
                .get_quaternion()
                .slerp(grip.basis.get_quaternion(), weight),
        );
        skel.set_bone_global_pose(hand, Transform3D::new(basis, current.origin));
    }
}

/// Turns a bone (and so its children) about an axis through its own origin, in
/// skeleton space.
fn rotate_global(skel: &mut Gd<Skeleton3D>, bone: i32, axis: Vector3, angle: f32) {
    if angle.abs() < 1e-6 {
        return;
    }
    apply_global_rotation(
        skel,
        bone,
        Quaternion::from_axis_angle(axis.normalized(), angle),
    );
}

fn apply_global_rotation(skel: &mut Gd<Skeleton3D>, bone: i32, rotation: Quaternion) {
    if rotation == Quaternion::IDENTITY {
        return;
    }
    let pose = skel.get_bone_global_pose(bone);
    skel.set_bone_global_pose(
        bone,
        Transform3D::new(Basis::from_quaternion(rotation) * pose.basis, pose.origin),
    );
}

#[cfg(test)]
mod hold_tests {
    use super::*;

    #[test]
    fn support_hands_close_less_than_a_fist() {
        // A handguard is wider than a pistol grip: the support fingers stay
        // more open than the firing hand's wrapped fingers.
        for finger in 1..4 {
            let support: f32 = HANDGUARD[finger].iter().sum();
            let firing: f32 = FIRING_HAND[finger].iter().sum();
            assert!(support < firing, "finger {finger}");
        }
        let on: f32 = FIRING_HAND[0].iter().sum();
        let off: f32 = INDEX_OFF_TRIGGER.iter().sum();
        assert!(off < on * 0.3, "off the trigger the index lies straight");
    }
}
