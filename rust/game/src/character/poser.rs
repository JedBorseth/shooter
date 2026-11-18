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
    const LEFT_UPPER_LEG: usize = 12;
    const LEFT_LOWER_LEG: usize = 13;
    const LEFT_FOOT: usize = 14;
    const RIGHT_UPPER_LEG: usize = 15;
    const RIGHT_LOWER_LEG: usize = 16;
    const RIGHT_FOOT: usize = 17;
    const NAMES: [&'static str; BONES] = [
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
}

/// Bones the poser resolves.
const BONES: usize = 18;
/// Foot IK lowers the hips at most this far (m) so a foot can reach lower
/// ground: the hit zones stand on flat ground, so the body keeps close to them.
pub const MAX_PELVIS_DROP: f32 = 0.09;

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
    /// The weapon's cross-section where the support hand holds it. The
    /// support palm rests on this surface instead of a fixed distance from
    /// the marker.
    pub section: Option<Section>,
}

/// A weapon's cross-section across the barrel, at the support marker's rest
/// position (weapon space).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Section {
    /// Where the support marker was when the section was cut.
    pub marker: Vector3,
    /// The section's centre (x, y) and half its width and height.
    pub centre: Vector2,
    pub half: Vector2,
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
            section: None,
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

    /// Where the support hand's grip centre goes and where that centre sits
    /// in the hand (`palm`), given the hand's basis in weapon space. On a
    /// handguard or pump with a known section, the centre is the section's
    /// and the palm rests on its surface. The section moves with the marker
    /// along the barrel (a pump's stroke) and lets go as the marker leaves the
    /// barrel's line (a reload clip taking the hand away).
    fn support_contact(&self, hand: Basis) -> (Vector3, Vector3) {
        let section = match (self.hold, self.section) {
            (Hold::Pistol, _) | (_, None) => return (self.left, LEFT_PALM),
            (_, Some(section)) => section,
        };
        let moved = Vector2::new(
            self.left.x - section.marker.x,
            self.left.y - section.marker.y,
        );
        let hold = 1.0 - ((moved.length() - 0.01) / 0.04).clamp(0.0, 1.0);
        if hold <= 0.0 {
            return (self.left, LEFT_PALM);
        }
        // The palm's normal (the hand's +Z) in weapon space, and how far the
        // section reaches along it (an ellipse's support).
        let n = hand * Vector3::BACK;
        let half = section.half;
        let reach = ((n.x * half.x).powi(2) + (n.y * half.y).powi(2)).sqrt();
        let centre = Vector3::new(
            section.centre.x + moved.x,
            section.centre.y + moved.y,
            self.left.z,
        );
        let palm = Vector3::new(LEFT_PALM.x, LEFT_PALM.y, reach + PALM_SKIN);
        (self.left.lerp(centre, hold), LEFT_PALM.lerp(palm, hold))
    }
}

/// How a hand closes, written per kind of grip, not per weapon model, so the
/// hands fit whatever mesh a weapon gets:
///
/// - `flex`: degrees per joint, toward the palm (+X on every finger bone after
///   the humanoid retarget), for the index, middle, ring, and little fingers
///   (proximal, intermediate, distal) and the thumb (metacarpal, proximal,
///   distal; the thumb's +X swings it across the palm);
/// - `spread`: degrees at each finger's knuckle, about the palm's normal,
///   positive toward the little finger's side, so fingers fan or close up;
/// - `oppose`: degrees at the thumb's base about its own Z, positive turning
///   the thumb in front of the palm, so it can meet the fingers round a grip.
#[derive(Clone, Copy, Debug, PartialEq)]
struct HandShape {
    flex: [[f32; 3]; 5],
    spread: [f32; 4],
    oppose: f32,
}
const FINGERS: [&str; 5] = ["Index", "Middle", "Ring", "Little", "Thumb"];

/// The firing hand on a pistol grip (about 30 mm deep): the lower three
/// fingers wrap it, fanning a little toward the little finger as the grip
/// widens; the index curls round to put its pad on the trigger, so it stays
/// inside the trigger guard instead of poking out the far side; the thumb
/// turns in front of the palm and closes over the far side.
const FIRING_HAND: HandShape = HandShape {
    flex: [
        [45.0, 65.0, 35.0],
        [70.0, 80.0, 45.0],
        [75.0, 80.0, 45.0],
        [80.0, 80.0, 45.0],
        [20.0, 30.0, 25.0],
    ],
    spread: [-4.0, 0.0, 3.0, 7.0],
    oppose: 32.0,
};
/// The support hand closed on the knife's grip: a fist, fingers together,
/// with the thumb turned in and wrapped over the fingers.
const KNIFE_FIST: HandShape = HandShape {
    flex: [
        [70.0, 80.0, 45.0],
        [75.0, 85.0, 45.0],
        [80.0, 85.0, 45.0],
        [85.0, 85.0, 45.0],
        [25.0, 40.0, 35.0],
    ],
    spread: [-2.0, 0.0, 2.0, 4.0],
    oppose: 40.0,
};
/// The support hand open and planted flat on a ledge: the fingers nearly
/// straight and spread, the thumb out to the side.
const PLANTED: HandShape = HandShape {
    flex: [
        [12.0, 10.0, 6.0],
        [10.0, 10.0, 6.0],
        [12.0, 10.0, 6.0],
        [15.0, 12.0, 8.0],
        [5.0, 10.0, 5.0],
    ],
    spread: [-7.0, -1.0, 4.0, 9.0],
    oppose: 8.0,
};
/// The support hand round a grenade: fingers curled over the body and the
/// spoon, the thumb across the top.
const GRENADE_GRIP: HandShape = HandShape {
    flex: [
        [40.0, 50.0, 30.0],
        [45.0, 55.0, 30.0],
        [50.0, 55.0, 30.0],
        [55.0, 55.0, 30.0],
        [20.0, 25.0, 20.0],
    ],
    spread: [-6.0, 0.0, 5.0, 10.0],
    oppose: 45.0,
};
/// The trigger finger laid straight along the frame, off the trigger, while
/// sprinting or reloading.
const INDEX_OFF_TRIGGER: [f32; 3] = [4.0, 6.0, 4.0];
/// The support hand under a rifle handguard (about 45 mm across): a loose C
/// with the fingertips up the far side, fanned along the handguard, and the
/// thumb laid forward along the near side.
const HANDGUARD: HandShape = HandShape {
    flex: [
        [55.0, 60.0, 35.0],
        [60.0, 65.0, 38.0],
        [62.0, 65.0, 38.0],
        [65.0, 65.0, 38.0],
        [10.0, 20.0, 15.0],
    ],
    spread: [-6.0, -1.0, 4.0, 9.0],
    oppose: 38.0,
};
/// The support hand round a shotgun's pump: a firmer wrap to drive it, the
/// fingers closed up together, the thumb laid forward along the near side.
const PUMP: HandShape = HandShape {
    flex: [
        [58.0, 62.0, 35.0],
        [62.0, 66.0, 36.0],
        [64.0, 66.0, 36.0],
        [66.0, 66.0, 36.0],
        [14.0, 25.0, 15.0],
    ],
    spread: [-2.0, 0.0, 2.0, 4.0],
    oppose: 45.0,
};
/// The support hand on a pistol: fingers wrapped over the firing hand's and
/// closed up beneath the trigger guard, the thumb turned forward along the
/// frame under the firing thumb.
const CUP: HandShape = HandShape {
    flex: [
        [62.0, 68.0, 40.0],
        [66.0, 70.0, 42.0],
        [68.0, 70.0, 42.0],
        [70.0, 70.0, 42.0],
        [20.0, 25.0, 10.0],
    ],
    spread: [2.0, 3.0, 5.0, 8.0],
    oppose: 45.0,
};

impl HandShape {
    /// This shape moved toward `other` by `t` (0..1), joint by joint.
    fn lerp(&self, other: &HandShape, t: f32) -> HandShape {
        let mix = |a: f32, b: f32| a + (b - a) * t;
        let mut out = *self;
        for f in 0..5 {
            for j in 0..3 {
                out.flex[f][j] = mix(self.flex[f][j], other.flex[f][j]);
            }
        }
        for f in 0..4 {
            out.spread[f] = mix(self.spread[f], other.spread[f]);
        }
        out.oppose = mix(self.oppose, other.oppose);
        out
    }

    /// This shape's rotation for one finger bone, on top of its rest, with
    /// `flex` degrees of curl. `left` mirrors the axes that change sign
    /// between the hands.
    fn joint(&self, finger: usize, joint: usize, left: bool, flex: f32) -> Quaternion {
        let curl = Quaternion::from_axis_angle(Vector3::RIGHT, flex.to_radians());
        if joint != 0 {
            return curl;
        }
        // The left hand's +X points toward its little finger and the right
        // hand's toward its index, so a turn about +Z changes sign between
        // the hands. On the thumb's base, positive lays the thumb forward
        // along what the hand holds instead of standing it up beside it
        // (checked from the outside, above, and below in the hand captures).
        let side = if left { -1.0 } else { 1.0 };
        let base = side
            * if finger == 4 {
                self.oppose
            } else {
                self.spread[finger]
            };
        Quaternion::from_axis_angle(Vector3::BACK, base.to_radians()) * curl
    }
}

/// Where the grip sits in each hand's bone space (from the wrist).
const RIGHT_PALM: Vector3 = to_godot(carry::RIGHT_PALM);
const LEFT_PALM: Vector3 = to_godot(carry::LEFT_PALM);
/// From the hand bone to the gloved palm's surface, where the palm meets a
/// handguard.
const PALM_SKIN: f32 = 0.019;

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

/// What the support hand holds when it leaves the weapon (`PoseInput::knife`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Held {
    #[default]
    Knife,
    /// Nothing: the open hand planted on a ledge, `knife_at` being the hand
    /// bone's own frame.
    Plant,
}

/// Everything the poser needs for one frame. Set by the node that owns it.
#[derive(Clone, Copy, Debug, Default)]
pub struct PoseInput {
    /// Radians the hips turn (positive turns left, seen from above). The spine
    /// turns back by the same amount.
    pub lower_yaw: f32,
    /// Radians the aim has turned left of the planted feet, standing still:
    /// the hips turn back about the feet (not about themselves) and the spine
    /// turns the rest.
    pub turn: f32,
    /// Aim pitch in radians, up positive.
    pub pitch: f32,
    /// Torso lean as a horizontal vector in the character's space (+X right,
    /// -Z forward); its length is the angle in radians.
    pub lean: Vector3,
    /// The head's tip from a hit, as `lean`, shared by the neck and head.
    pub head_lean: Vector3,
    /// The torso's twist from a hit (radians about up, positive left),
    /// shared by the spine and chest.
    pub twist: f32,
    /// How far each hand's grip on the weapon loosens (right, left), 0..1,
    /// on top of `ik`: an arm hit loosens one.
    pub arm_loose: [f32; 2],
    pub placement: WeaponPlacement,
    pub grips: Option<Grips>,
    /// 0 lets the animation pose the arms, 1 puts the hands on the weapon.
    pub ik: f32,
    /// The support hand lets go of the weapon by this much (0..1), for reloads.
    pub support_off: f32,
    /// Where the support hand goes when it lets go, in weapon space.
    pub support_target: Vector3,
    /// The trigger finger comes off the trigger by this much (0..1): sprinting
    /// or reloading.
    pub trigger_off: f32,
    /// The support hand leaves the weapon by this much (0..1) for the item
    /// in `held` (the knife to swing, or the ledge to plant on), at `knife_at`.
    pub knife: f32,
    /// Where the item is, in the weapon's space (see `crate::knife`).
    pub knife_at: Transform3D,
    /// What the support hand takes when it leaves the weapon.
    pub held: Held,
    /// The support hand leaves the weapon by this much (0..1) to throw a
    /// grenade, which is at `grenade_at` (weapon space) while
    /// `grenade_in_hand`, with its pin in while `grenade_pinned`.
    pub grenade: f32,
    pub grenade_at: Transform3D,
    pub grenade_in_hand: bool,
    pub grenade_pinned: bool,
    /// Foot IK: how far (m, world up) the ground under each foot (left,
    /// right) is from the ground the animation stands on, or `None` for no
    /// foot IK (far away, in the air).
    pub feet: Option<[f32; 2]>,
}

#[derive(GodotClass)]
#[class(init, base = SkeletonModifier3D)]
pub struct SoldierPoser {
    base: Base<SkeletonModifier3D>,
    pub input: PoseInput,
    /// The weapon node the hands hold.
    pub weapon: Option<Gd<Node3D>>,
    bones: Option<[i32; BONES]>,
    /// Where the animation put each foot (left, right) last frame, world
    /// space, before foot IK: where the rig looks for the ground.
    pub feet_seen: [Vector3; 2],
    /// Finger bones of both hands: bone, rest rotation, right hand, finger
    /// (index into `FINGERS`), and joint.
    fingers: Vec<(i32, Quaternion, bool, usize, usize)>,
    /// Set to capture the next pose drawn: every bone's local pose after
    /// this modifier, into `captured` (the ragdoll starts from it).
    pub capture: bool,
    pub captured: Option<Vec<super::ragdoll::BonePose>>,
    /// Where the IK last put the wrists (world space): right, left. Godot drops
    /// modified poses after skinning, so this is how tests see the result.
    solved: [Option<Vector3>; 2],
    /// The knife, put in the support hand while it swings one.
    pub knife: Option<Gd<Node3D>>,
    /// A grenade, put in the support hand while it throws one.
    pub grenade: Option<Gd<Node3D>>,
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
        self.pose(&mut skel, bones);
        if std::mem::take(&mut self.capture) {
            let pose = (0..skel.get_bone_count())
                .map(|b| {
                    (
                        skel.get_bone_pose_position(b),
                        skel.get_bone_pose_rotation(b),
                        skel.get_bone_pose_scale(b),
                    )
                })
                .collect();
            self.captured = Some(pose);
        }
    }
}

impl SoldierPoser {
    fn pose(&mut self, skel: &mut Gd<Skeleton3D>, bones: [i32; BONES]) {
        let input = self.input;
        let bone = |i: usize| bones[i];
        // Hidden unless a swing below shows it.
        if let Some(node) = &mut self.knife
            && node.is_visible()
            && (input.knife <= 0.35 || input.ik <= 0.0 || input.held != Held::Knife)
        {
            node.set_visible(false);
        }
        if let Some(node) = &mut self.grenade
            && node.is_visible()
            && (input.grenade <= 0.2 || !input.grenade_in_hand || input.ik <= 0.0)
        {
            node.set_visible(false);
        }

        // 1. Hips toward the movement (or back onto the planted feet), spine
        // back toward the aim.
        if input.turn.abs() > 1e-4 {
            // About the feet, which stay planted: the skeleton's origin.
            let hips = skel.get_bone_global_pose(bone(Bone::HIPS));
            let turn = Basis::from_axis_angle(Vector3::UP, -input.turn);
            skel.set_bone_global_pose(
                bone(Bone::HIPS),
                Transform3D::new(turn * hips.basis, turn * hips.origin),
            );
        }
        if input.lower_yaw.abs() > 1e-4 {
            rotate_global(skel, bone(Bone::HIPS), Vector3::UP, input.lower_yaw);
        }
        let twist = input.lower_yaw - input.turn;
        if twist.abs() > 1e-4 {
            for (b, share) in TWIST_BONES.into_iter().zip(TWIST_SHARES) {
                rotate_global(skel, bone(b), Vector3::UP, -twist * share);
            }
        }

        // 1b. Feet on the ground: the hips drop a little for a lower foot,
        // and each leg reaches its foot to the ground under it.
        let skel_xform = skel.get_global_transform();
        for (i, foot) in [Bone::LEFT_FOOT, Bone::RIGHT_FOOT].into_iter().enumerate() {
            self.feet_seen[i] = skel_xform * skel.get_bone_global_pose(bone(foot)).origin;
        }
        if let Some(lift) = input.feet {
            self.plant_feet(skel, bones, lift);
        }

        // 2. Aim pitch and hit lean. The skeleton faces +Z, so the character's
        // right is -X; pitching up turns +Z toward +Y, a negative turn about +X.
        for (b, share) in PITCH_BONES.into_iter().zip(PITCH_SHARES) {
            rotate_global(skel, bone(b), Vector3::RIGHT, -input.pitch * share);
        }
        let lean = Vector3::new(-input.lean.x, 0.0, -input.lean.z);
        if let Some(dir) = lean.try_normalized() {
            let axis = Vector3::UP.cross(dir);
            let angle = lean.length();
            rotate_global(skel, bone(Bone::SPINE), axis, angle * 0.5);
            rotate_global(skel, bone(Bone::CHEST), axis, angle * 0.5);
        }
        if input.twist.abs() > 1e-4 {
            rotate_global(skel, bone(Bone::SPINE), Vector3::UP, input.twist * 0.4);
            rotate_global(skel, bone(Bone::CHEST), Vector3::UP, input.twist * 0.6);
        }
        let head = Vector3::new(-input.head_lean.x, 0.0, -input.head_lean.z);
        if let Some(dir) = head.try_normalized() {
            let axis = Vector3::UP.cross(dir);
            let angle = head.length();
            rotate_global(skel, bone(Bone::NECK), axis, angle * 0.4);
            rotate_global(skel, bone(Bone::HEAD), axis, angle * 0.6);
        }

        // 3. The weapon, in skeleton space.
        let skel_xform = skel.get_global_transform();
        let Some(mut weapon) = self.weapon.clone() else {
            return;
        };
        let weapon_skel = match input.placement {
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
        };

        // 4. Hands on the weapon.
        let Some(grips) = input.grips else {
            return;
        };
        let weight = input.ik.clamp(0.0, 1.0);
        if weight <= 0.0 {
            self.solved = [None, None];
            return;
        }
        let knife = input.knife.clamp(0.0, 1.0);
        let support = support_shape(grips.hold);
        let trigger_off = input.trigger_off.clamp(0.0, 1.0);
        let held_shape = match input.held {
            Held::Knife => &KNIFE_FIST,
            Held::Plant => &PLANTED,
        };
        let support = support.lerp(held_shape, knife);
        let grenade = input.grenade.clamp(0.0, 1.0);
        let support = support.lerp(&GRENADE_GRIP, grenade);
        for &(index, rest, right, finger, joint) in &self.fingers {
            let shape = if right { &FIRING_HAND } else { &support };
            let mut flex = shape.flex[finger][joint];
            if right && finger == 0 {
                flex += (INDEX_OFF_TRIGGER[joint] - flex) * trigger_off;
            }
            let grip = rest * shape.joint(finger, joint, !right, flex);
            let pose = skel.get_bone_pose_rotation(index);
            skel.set_bone_pose_rotation(index, pose.slerp(grip, weight));
        }
        let (right_basis, left_basis) = grips.hand_bases();
        // Elbow hints in the weapon's frame: the firing elbow down and out, the
        // support elbow down and in under the weapon.
        let right_pole = weapon_skel * to_godot(carry::RIGHT_POLE);
        let left_pole = weapon_skel * to_godot(carry::LEFT_POLE);
        self.place_hand(
            skel,
            [
                bone(Bone::RIGHT_UPPER_ARM),
                bone(Bone::RIGHT_LOWER_ARM),
                bone(Bone::RIGHT_HAND),
            ],
            Transform3D::new(weapon_skel.basis * right_basis, weapon_skel * grips.right),
            RIGHT_PALM,
            right_pole,
            weight * (1.0 - input.arm_loose[0].clamp(0.0, 1.0)),
        );
        let (left_point, left_palm) = grips.support_contact(left_basis);
        let mut left_grip = weapon_skel * left_point;
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
            Transform3D::new(left_hand_basis, left_grip - left_hand_basis * left_palm);
        if knife > 0.0 {
            let hand = weapon_skel
                * match input.held {
                    Held::Knife => crate::knife::hand_for(input.knife_at),
                    Held::Plant => input.knife_at,
                };
            let rotation = left_target
                .basis
                .get_quaternion()
                .slerp(hand.basis.orthonormalized().get_quaternion(), knife);
            left_target = Transform3D::new(
                Basis::from_quaternion(rotation),
                left_target.origin.lerp(hand.origin, knife),
            );
        }
        if grenade > 0.0 {
            let hand = weapon_skel * crate::grenade_hold::hand_for(input.grenade_at);
            let rotation = left_target
                .basis
                .get_quaternion()
                .slerp(hand.basis.orthonormalized().get_quaternion(), grenade);
            left_target = Transform3D::new(
                Basis::from_quaternion(rotation),
                left_target.origin.lerp(hand.origin, grenade),
            );
        }
        self.place_hand(
            skel,
            [
                bone(Bone::LEFT_UPPER_ARM),
                bone(Bone::LEFT_LOWER_ARM),
                bone(Bone::LEFT_HAND),
            ],
            left_target,
            Vector3::ZERO,
            left_pole,
            weight * (1.0 - input.arm_loose[1].clamp(0.0, 1.0)),
        );
        if let Some(node) = &mut self.knife {
            let shown = knife > 0.35 && input.held == Held::Knife;
            if node.is_visible() != shown {
                node.set_visible(shown);
            }
            if shown {
                let hand = skel.get_bone_global_pose(bone(Bone::LEFT_HAND));
                node.set_global_transform(skel_xform * hand * crate::knife::in_left_hand());
            }
        }
        if let Some(node) = &mut self.grenade {
            let shown = grenade > 0.2 && input.grenade_in_hand;
            if node.is_visible() != shown {
                node.set_visible(shown);
            }
            if shown {
                let hand = skel.get_bone_global_pose(bone(Bone::LEFT_HAND));
                node.set_global_transform(skel_xform * hand * crate::grenade_hold::in_left_hand());
                crate::grenade_hold::set_pinned(node, input.grenade_pinned, true);
            }
        }
        self.solved = [
            Some(skel_xform * skel.get_bone_global_pose(bone(Bone::RIGHT_HAND)).origin),
            Some(skel_xform * skel.get_bone_global_pose(bone(Bone::LEFT_HAND)).origin),
        ];
    }
}

impl SoldierPoser {
    /// Lowers the hips for a foot on lower ground and bends each leg so its
    /// foot lands `lift` metres above (or below) where the animation put it.
    fn plant_feet(&self, skel: &mut Gd<Skeleton3D>, bones: [i32; BONES], lift: [f32; 2]) {
        let bone = |i: usize| bones[i];
        let skel_xform = skel.get_global_transform();
        let to_skel = skel_xform.basis.inverse();
        let drop = (-lift[0].min(lift[1])).clamp(0.0, MAX_PELVIS_DROP);
        if drop > 1e-4 {
            let hips = skel.get_bone_global_pose(bone(Bone::HIPS));
            skel.set_bone_global_pose(
                bone(Bone::HIPS),
                Transform3D::new(hips.basis, hips.origin - to_skel * (Vector3::UP * drop)),
            );
        }
        let legs = [
            (Bone::LEFT_UPPER_LEG, Bone::LEFT_LOWER_LEG, Bone::LEFT_FOOT),
            (
                Bone::RIGHT_UPPER_LEG,
                Bone::RIGHT_LOWER_LEG,
                Bone::RIGHT_FOOT,
            ),
        ];
        for ((upper, lower, foot), lift) in legs.into_iter().zip(lift) {
            let reach = lift + drop;
            if reach.abs() < 1e-3 {
                continue;
            }
            let (upper, lower, foot) = (bone(upper), bone(lower), bone(foot));
            let chain = Chain {
                root: skel.get_bone_global_pose(upper).origin,
                mid: skel.get_bone_global_pose(lower).origin,
                end: skel.get_bone_global_pose(foot).origin,
            };
            let target = chain.end + to_skel * (Vector3::UP * reach);
            // The knee keeps bending the way it bends now (forward).
            let middle = (chain.root + chain.end) * 0.5;
            let bend = (chain.mid - middle)
                .try_normalized()
                .unwrap_or(Vector3::BACK);
            let pole = chain.mid + bend * 0.4;
            let flat = skel.get_bone_global_pose(foot).basis;
            let solution = ik::solve_two_bone(chain, target, pole);
            apply_global_rotation(skel, upper, solution.upper);
            apply_global_rotation(skel, lower, solution.lower);
            // The foot keeps the angle the animation gave it.
            let placed = skel.get_bone_global_pose(foot);
            skel.set_bone_global_pose(foot, Transform3D::new(flat, placed.origin));
        }
    }

    /// Where the IK last put a wrist (world space), if it ran.
    pub fn solved_wrist(&self, right: bool) -> Option<Vector3> {
        self.solved[usize::from(!right)]
    }

    fn resolve(&mut self, skel: &Gd<Skeleton3D>) -> Option<[i32; BONES]> {
        if self.bones.is_none() {
            let mut found = [0; BONES];
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
            let support: f32 = HANDGUARD.flex[finger].iter().sum();
            let firing: f32 = FIRING_HAND.flex[finger].iter().sum();
            assert!(support < firing, "finger {finger}");
        }
        let on: f32 = FIRING_HAND.flex[0].iter().sum();
        let off: f32 = INDEX_OFF_TRIGGER.iter().sum();
        assert!(off < on * 0.3, "off the trigger the index lies straight");
    }

    #[test]
    fn the_support_palm_rests_on_the_handguard() {
        let marker = Vector3::new(0.0, -0.08, -0.3);
        let mut grips = Grips {
            left: marker,
            section: Some(Section {
                marker,
                centre: Vector2::new(0.0, -0.05),
                half: Vector2::new(0.02, 0.025),
            }),
            ..Grips::default()
        };
        // Palm straight up under the handguard: the centre is the section's,
        // and the palm sits its half height plus the glove below it.
        let up = ik::basis_from_y_z(Vector3::FORWARD, Vector3::UP);
        let (point, palm) = grips.support_contact(up);
        assert!(
            (point - Vector3::new(0.0, -0.05, -0.3)).length() < 1e-5,
            "{point:?}"
        );
        assert!((palm.z - (0.025 + PALM_SKIN)).abs() < 1e-4, "{palm:?}");
        // A pump stroke slides the marker along the barrel: still held.
        grips.left.z -= 0.06;
        let (point, _) = grips.support_contact(up);
        assert!(
            (point - Vector3::new(0.0, -0.05, -0.36)).length() < 1e-5,
            "{point:?}"
        );
        // A reload clip takes the marker away: the hand follows the marker.
        grips.left = marker + Vector3::new(-0.05, -0.15, 0.1);
        let (point, palm) = grips.support_contact(up);
        assert_eq!((point, palm), (grips.left, LEFT_PALM));
        // Pistols cup the firing hand whatever the weapon's section.
        grips.left = marker;
        grips.hold = Hold::Pistol;
        assert_eq!(grips.support_contact(up), (marker, LEFT_PALM));
    }

    #[test]
    fn fingers_fan_and_thumbs_oppose_mirrored_between_hands() {
        // The little finger fans out further than the index on a handguard.
        const { assert!(HANDGUARD.spread[3] > HANDGUARD.spread[0]) };
        for shape in [FIRING_HAND, HANDGUARD, PUMP, CUP, KNIFE_FIST] {
            assert!(shape.oppose > 0.0, "every grip turns the thumb in");
            for (finger, joint) in [(0, 0), (3, 0), (4, 0)] {
                let flex = shape.flex[finger][joint];
                let right = shape.joint(finger, joint, false, flex);
                let left = shape.joint(finger, joint, true, flex);
                // Mirroring across the hand's X axis: the curl is the same,
                // the turn about the palm normal flips.
                let mirror = Quaternion::new(right.x, -right.y, -right.z, right.w);
                assert!(
                    left.dot(mirror).abs() > 0.9999,
                    "{finger}: {left:?} vs {right:?}"
                );
            }
        }
        // Only the knuckle spreads; the other joints just curl.
        let curl = HANDGUARD.joint(1, 1, false, 40.0);
        assert_eq!(
            curl,
            Quaternion::from_axis_angle(Vector3::RIGHT, 40f32.to_radians())
        );
    }

    #[test]
    fn the_knife_fist_blends_from_the_support_shape() {
        assert_eq!(HANDGUARD.lerp(&KNIFE_FIST, 0.0), HANDGUARD);
        assert_eq!(HANDGUARD.lerp(&KNIFE_FIST, 1.0), KNIFE_FIST);
        let half = HANDGUARD.lerp(&KNIFE_FIST, 0.5);
        assert!((half.oppose - (HANDGUARD.oppose + KNIFE_FIST.oppose) * 0.5).abs() < 1e-5);
    }
}
