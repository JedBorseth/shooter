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

use super::grasp::{self, Contact, WeaponFields};
use super::ik::{self, Chain};
use crate::convert::{to_godot, to_godot_basis, to_sim};
use shooter_sim::weapon::{Attachment, FingerFlex, WeaponDef};

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
    const LEFT_TOES: usize = 18;
    const RIGHT_TOES: usize = 19;
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
        "LeftToes",
        "RightToes",
    ];
}

/// Bones the poser resolves.
const BONES: usize = 20;
/// Foot IK lowers the hips at most this far (m) so a foot can reach lower
/// ground: the hit zones stand on flat ground, so the body keeps close to them.
pub const MAX_PELVIS_DROP: f32 = 0.09;
/// Past what the straightened leg reaches, the foot points its toe down
/// toward lower ground, as a foot reaches for the next stair down: the ball
/// of the foot leans at most this far below level (radians).
pub const MAX_TOE_REACH: f32 = 0.85;

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
    /// The weapon's fitted finger poses (firing hand, support hand) from its
    /// file (`WeaponDef::hands`), where it has them, and the firing index off
    /// the trigger.
    pub fingers: [Option<FingerFlex>; 2],
    /// Each finger's fitted spread and the thumb's turn (degrees), per hand.
    pub spreads: [Option<[f32; 5]>; 2],
    /// How far each hand sits from its marker (weapon space, m), fitted with
    /// the fingers so the palm and knuckles rest on the weapon.
    pub seats: [Vector3; 2],
    pub index_off: Option<[f32; 3]>,
    /// The support hand holds a vertical foregrip, in a fist.
    pub foregrip: bool,
    /// Where the grip markers rest (right, left) before a clip or a bolt
    /// moves them: a hand's fitted pose fades as its marker leaves its rest.
    pub rests: [Option<Vector3>; 2],
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

/// How the firing hand closes for a hold: a pistol's trigger sits closer to
/// its grip, so the index curls further.
fn firing_shape(hold: Hold) -> HandShape {
    if hold == Hold::Pistol {
        FIRING_PISTOL
    } else {
        FIRING_HAND
    }
}

/// How the support hand closes for a hold: a fist round a vertical
/// foregrip, or the hold's own shape.
fn support_shape(hold: Hold, foregrip: bool) -> HandShape {
    match hold {
        _ if foregrip => KNIFE_FIST,
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
            fingers: [None, None],
            spreads: [None, None],
            seats: [Vector3::ZERO; 2],
            index_off: None,
            foregrip: false,
            rests: [None, None],
        }
    }
}

impl Grips {
    /// Sets the weapon's finger poses and foregrip from its definition.
    pub fn fit_to(&mut self, def: &WeaponDef) {
        let (right, index_off, left) = def.finger_poses();
        self.fingers = [right, left].map(|h| h.map(|h| h.fingers));
        self.spreads = [right, left].map(|h| h.and_then(|h| h.spread));
        self.seats =
            [right, left].map(|h| h.map_or(Vector3::ZERO, |h| Vector3::from_array(h.seat)));
        self.index_off = index_off;
        self.foregrip = def.attachment == Some(Attachment::Grip);
    }

    /// How each hand closes (firing, support): the hold's shapes, moved
    /// toward the weapon's fitted finger poses by `on` (0..1 per hand, how far
    /// the hand is on the grip they were fitted to).
    fn shapes(&self, on: [f32; 2]) -> [HandShape; 2] {
        let mut out = [
            firing_shape(self.hold),
            support_shape(self.hold, self.foregrip),
        ];
        for (h, shape) in out.iter_mut().enumerate() {
            let mut fitted = *shape;
            if let Some(flex) = self.fingers[h] {
                fitted.flex = flex;
            }
            if let Some(spread) = self.spreads[h] {
                fitted.spread.copy_from_slice(&spread[..4]);
                fitted.oppose = spread[4];
            }
            *shape = shape.lerp(&fitted, on[h]);
        }
        out
    }

    /// How far each hand is on the grip its fitted pose was fitted to (right,
    /// left; 0..1): the firing hand's marker leaving its rest (onto a bolt)
    /// lets go, and the support marker leaving the barrel's line (a reload
    /// clip) lets go, while a pump's stroke along it does not.
    fn on_grips(&self) -> [f32; 2] {
        let right = self.rests[0].map_or(1.0, |rest| {
            1.0 - ((self.right.distance_to(rest) - 0.01) / 0.04).clamp(0.0, 1.0)
        });
        let left = self.rests[1].map_or(1.0, |rest| self.held_at(rest));
        [right, left]
    }

    /// The hand bones' bases in weapon space: +Y toward the knuckles, +Z out of
    /// the palm (`carry::Grips::hand_bases`).
    fn hand_bases(&self) -> (Basis, Basis) {
        let (right, left) = carry::Grips {
            right: to_sim(self.right),
            left: to_sim(self.left),
            rake: self.rake,
            hold: self.hold,
            section: None,
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
    /// How firmly the support hand is on what it holds (0..1): the marker
    /// leaving the barrel's line (a reload clip taking the hand away) lets go,
    /// while sliding along it (a pump's stroke) does not.
    fn held_at(&self, rest: Vector3) -> f32 {
        let moved = Vector2::new(self.left.x - rest.x, self.left.y - rest.y);
        1.0 - ((moved.length() - 0.01) / 0.04).clamp(0.0, 1.0)
    }

    fn support_contact(&self, hand: Basis) -> (Vector3, Vector3) {
        let section = match (self.hold, self.section) {
            (Hold::Pistol, _) | (_, None) => return (self.left, LEFT_PALM),
            (_, Some(section)) => section,
        };
        let moved = Vector2::new(
            self.left.x - section.marker.x,
            self.left.y - section.marker.y,
        );
        let hold = self.held_at(section.marker);
        if hold <= 0.0 {
            return (self.left, LEFT_PALM);
        }
        // The same contact the arm hit zones use (`carry::section_contact`).
        let cut = carry::Section {
            centre: shooter_sim::Vec2::new(section.centre.x + moved.x, section.centre.y + moved.y),
            half: shooter_sim::Vec2::new(section.half.x, section.half.y),
        };
        let hand = shooter_sim::Mat3::from_cols(
            to_sim(hand.col_a()),
            to_sim(hand.col_b()),
            to_sim(hand.col_c()),
        );
        let (centre, palm) = carry::section_contact(cut, self.left.z, hand);
        let (centre, palm) = (to_godot(centre), to_godot(palm));
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
/// The firing hand on a pistol held in both hands: as on a rifle's pistol
/// grip, but the trigger finger curled further. A pistol's trigger sits
/// closer to the grip, so the rifle shape's index ran through the guard and
/// out the far side, where it stood beside the slide in the sight picture.
const FIRING_PISTOL: HandShape = HandShape {
    flex: [
        [60.0, 90.0, 60.0],
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
/// The support hand round a shotgun's pump: the fingers closed up together
/// and hooked round its far side (bent little at the knuckles, hard at the
/// middle and end joints), so the fingertips stay below the receiver's top
/// and off the bead when aimed; the thumb laid forward along the near side.
const PUMP: HandShape = HandShape {
    flex: [
        [40.0, 85.0, 55.0],
        [44.0, 88.0, 55.0],
        [46.0, 88.0, 55.0],
        [50.0, 88.0, 55.0],
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
        Self::turned(joint, left, flex, self.base(finger))
    }

    /// The finger's spread at its knuckle, or the thumb's turn across the
    /// palm (degrees).
    fn base(&self, finger: usize) -> f32 {
        if finger == 4 {
            self.oppose
        } else {
            self.spread[finger]
        }
    }

    /// One finger bone's rotation on top of its rest: `flex` degrees of curl,
    /// and on the knuckle `base` degrees of spread (or the thumb's turn).
    fn turned(joint: usize, left: bool, flex: f32, base: f32) -> Quaternion {
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
        Quaternion::from_axis_angle(Vector3::BACK, (side * base).to_radians()) * curl
    }
}

/// Where the grip sits in each hand's bone space (from the wrist).
const RIGHT_PALM: Vector3 = to_godot(carry::RIGHT_PALM);
const LEFT_PALM: Vector3 = to_godot(carry::LEFT_PALM);
/// From the hand bone to the gloved palm's surface, where the palm meets a
/// handguard.
#[cfg(test)]
const PALM_SKIN: f32 = carry::PALM_SKIN;

/// One finger's three joint bones and the chain they make.
type FingerBones = ([i32; 3], grasp::Chain);

/// A weapon's finger poses fitted to its model (`SoldierPoser::fit_hands`),
/// and how deep the fingers and palms still go into it (m, right and left;
/// negative: clear of it).
#[derive(Clone, Copy, Debug)]
pub struct FittedHands {
    pub right: FingerFlex,
    pub index_off: [f32; 3],
    pub left: FingerFlex,
    /// Each hand's fingers' spread and thumb's turn (degrees).
    pub spreads: [[f32; 5]; 2],
    /// How far each hand sits from its marker (weapon space, m).
    pub seats: [Vector3; 2],
    pub depths: HandDepths,
}

/// A candidate hand pose measured on the weapon ([`SoldierPoser::try_hand`]):
/// each finger's depth into it (m) and its points (weapon space) with the
/// finger's radius at each.
#[derive(Clone, Debug, Default)]
pub struct HandProbe {
    pub depths: [f32; 5],
    pub points: [Vec<(Vector3, f32)>; 5],
}

/// How deep a pose's fingers and palms go into a weapon (m; negative: clear
/// of it): per hand (right, left) and finger (`FINGERS` order), the firing
/// index off the trigger, and each palm.
#[derive(Clone, Copy, Debug)]
pub struct HandDepths {
    pub fingers: [[f32; 5]; 2],
    pub index_off: f32,
    pub palms: [f32; 2],
    /// How far each drawn hand is from where it was aimed (m): the arm
    /// falling short of its grip.
    pub misses: [f32; 2],
}

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
    /// The upper chest's blade behind a long gun, support shoulder forward
    /// (radians, `carry::Carry::blade`); the neck turns back as much.
    pub blade: f32,
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
    pub grenade_kind: shooter_sim::equipment::Kind,
    /// Foot IK: how far (m, world up) the ground under each foot (left,
    /// right) is from the ground the animation stands on, or `None` for no
    /// foot IK (far away, in the air).
    pub feet: Option<[f32; 2]>,
    /// On a ladder: hands and feet on its rungs.
    pub ladder: Option<LadderLimbs>,
    /// Vaulting: where the support wrist plants (world space) and how far
    /// it has left the weapon for it (`hitbox::action::vault_plant`).
    pub plant: Option<(Vector3, f32)>,
}

/// A vault's planted hand: its fingers point forward and down, and its
/// elbow bends toward this point out to the left and back (skeleton space,
/// which faces +Z with the character's left at +X), as the arm hit zones'
/// `VAULT_ELBOW` is in body space.
const VAULT_FINGERS: Vector3 = Vector3::new(0.0, -0.6, 1.0);
const VAULT_ELBOW: Vector3 = Vector3::new(0.6, 1.1, -0.3);

/// A climber's hands and feet on a ladder's rungs (world space), from
/// `crate::ladder::limb_targets`, blended in by `weight`.
#[derive(Clone, Copy, Debug, Default)]
pub struct LadderLimbs {
    pub weight: f32,
    /// Wrists: right, left.
    pub hands: [Vector3; 2],
    /// Ankles: left, right.
    pub ankles: [Vector3; 2],
    /// The way the climber faces (world, horizontal).
    pub facing: Vector3,
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
    /// Finger bones of both hands (right, left), per finger in `FINGERS`
    /// order: the three joints' bones and the chain they make, if the
    /// skeleton has them.
    fingers: [[Option<FingerBones>; 5]; 2],
    /// The hands are drawn at this scale of the skeleton: the first-person
    /// arms are drawn longer than life, but their hands are life size, so
    /// they fit the weapon they hold. Grip offsets in hand space are already
    /// in the weapon's units (the weapon's basis in skeleton space carries
    /// the skeleton's scale), so this makes the drawn hand agree with them.
    #[init(val = 1.0)]
    pub hand_scale: f32,
    /// Each hand bone's transform in the weapon's space as last drawn (right,
    /// left), for checking the weapon's finger poses (`hand_depths`), and
    /// where the hands were aimed with no seat, for fitting them
    /// (`fit_hands`). Both clear while no weapon is held.
    held: [Option<Transform3D>; 2],
    held_target: [Option<Transform3D>; 2],
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

        // 1. Standing still, the hips back onto the planted feet, and the
        // spine back toward the aim.
        if input.turn.abs() > 1e-4 {
            // About the feet, which stay planted: the skeleton's origin.
            let hips = skel.get_bone_global_pose(bone(Bone::HIPS));
            let turn = Basis::from_axis_angle(Vector3::UP, -input.turn);
            skel.set_bone_global_pose(
                bone(Bone::HIPS),
                Transform3D::new(turn * hips.basis, turn * hips.origin),
            );
        }
        let twist = -input.turn;
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
        if let Some(ladder) = input.ladder.filter(|l| l.weight > 0.0) {
            self.climb_legs(skel, bones, &ladder);
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

        // 2b. Bladed behind a long gun, as the hit zones pose it: the upper
        // chest turns about the pitched spine's up (`carry::blade_axis`; the
        // skeleton faces +Z, so its pitch axis is the body's turned round),
        // and the neck turns back so the head still faces the aim.
        if input.blade > 1e-4 {
            let share: f32 = PITCH_SHARES[..3].iter().sum();
            let axis = Basis::from_axis_angle(Vector3::RIGHT, -input.pitch * share) * Vector3::UP;
            rotate_global(skel, bone(Bone::UPPER_CHEST), axis, -input.blade);
            rotate_global(skel, bone(Bone::NECK), axis, input.blade);
        }

        if let Some(ladder) = input.ladder.filter(|l| l.weight > 0.0) {
            self.climb_arms(skel, bones, &ladder);
        }

        // 3. The weapon, in skeleton space.
        self.held = [None, None];
        self.held_target = [None, None];
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
        let trigger_off = input.trigger_off.clamp(0.0, 1.0);
        let held_shape = match input.held {
            Held::Knife => &KNIFE_FIST,
            Held::Plant => &PLANTED,
        };
        let grenade = input.grenade.clamp(0.0, 1.0);
        // The weapon's own finger poses where its file has them (fitted to
        // its model), else the hold's shapes; and each hand's seat on the
        // weapon, both as far as the hand is on the grip they were fitted to.
        let on = grips.on_grips();
        let on = [on[0], on[1] * (1.0 - input.support_off.clamp(0.0, 1.0))];
        let seats = [grips.seats[0] * on[0], grips.seats[1] * on[1]];
        let [mut firing, mut support] = grips.shapes(on);
        let index_off = grips.index_off.unwrap_or(INDEX_OFF_TRIGGER);
        for joint in 0..3 {
            let flex = &mut firing.flex[0][joint];
            *flex += (index_off[joint] - *flex) * trigger_off;
        }
        support = support.lerp(held_shape, knife).lerp(&GRENADE_GRIP, grenade);
        let left_hand = [false, true];
        for (h, shape) in [&firing, &support].into_iter().enumerate() {
            for (f, finger) in self.fingers[h].iter().enumerate() {
                let Some((joints, chain)) = finger else {
                    continue;
                };
                for j in 0..3 {
                    let grip = chain.rests[j] * shape.joint(f, j, left_hand[h], shape.flex[f][j]);
                    let pose = skel.get_bone_pose_rotation(joints[j]);
                    skel.set_bone_pose_rotation(joints[j], pose.slerp(grip, weight));
                }
            }
        }
        let (right_basis, left_basis) = grips.hand_bases();
        // Elbow hints in the weapon's frame: the firing elbow down and out, the
        // support elbow down and in under the weapon.
        let right_pole = weapon_skel * to_godot(carry::RIGHT_POLE);
        let mut left_pole = weapon_skel * to_godot(carry::LEFT_POLE);
        self.place_hand(
            skel,
            [
                bone(Bone::RIGHT_UPPER_ARM),
                bone(Bone::RIGHT_LOWER_ARM),
                bone(Bone::RIGHT_HAND),
            ],
            Transform3D::new(
                weapon_skel.basis * right_basis,
                weapon_skel * (grips.right + seats[0]),
            ),
            RIGHT_PALM,
            right_pole,
            weight * (1.0 - input.arm_loose[0].clamp(0.0, 1.0)),
        );
        let (left_point, left_palm) = grips.support_contact(left_basis);
        let mut left_grip = weapon_skel * (left_point + seats[1]);
        let mut left_hand_basis = weapon_skel.basis * left_basis;
        // Where each hand would go with no seat (weapon space), for fitting
        // poses from: the same however the stored seat moved it, or however
        // far the arm reached.
        // `place_hand` puts the wrist at `grip - basis * palm` (weapon space)
        // and draws the hand at `hand_scale` of the skeleton, which is
        // `hand_scale / weapon_scale` of the weapon.
        let weapon_scale = weapon_skel.basis.col_a().length();
        let hand_at = |basis: Basis, grip: Vector3, palm: Vector3| {
            let turn = Basis::from_quaternion(basis.get_quaternion());
            Transform3D::new(
                turn.scaled(Vector3::splat(self.hand_scale / weapon_scale)),
                grip - turn * palm,
            )
        };
        self.held_target = [
            Some(hand_at(right_basis, grips.right, RIGHT_PALM)),
            Some(hand_at(left_basis, left_point, left_palm)),
        ];
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
        if let Some((at, plant)) = input.plant.filter(|(_, b)| *b > 0.0) {
            // Palm down on the top, fingers forward (the skeleton faces +Z),
            // the elbow out to the side and back, as the arm zones pose it.
            let hand = left_target.basis.orthonormalized();
            let turn = ik::rotation_between(hand.col_b(), VAULT_FINGERS.normalized());
            let rotation = left_target.basis.get_quaternion().slerp(
                (Basis::from_quaternion(turn) * hand).get_quaternion(),
                plant,
            );
            left_target = Transform3D::new(
                Basis::from_quaternion(rotation),
                left_target
                    .origin
                    .lerp(skel_xform.affine_inverse() * at, plant),
            );
            left_pole = left_pole.lerp(VAULT_ELBOW, plant);
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
        // Where the hands ended up on the weapon, for checking the poses.
        let weapon_from_skel = weapon_skel.affine_inverse();
        self.held = [Bone::RIGHT_HAND, Bone::LEFT_HAND]
            .map(|b| Some(weapon_from_skel * skel.get_bone_global_pose(bone(b))));
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
                crate::grenade_hold::show(node, input.grenade_kind, input.grenade_pinned);
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
            (
                Bone::LEFT_UPPER_LEG,
                Bone::LEFT_LOWER_LEG,
                Bone::LEFT_FOOT,
                Bone::LEFT_TOES,
            ),
            (
                Bone::RIGHT_UPPER_LEG,
                Bone::RIGHT_LOWER_LEG,
                Bone::RIGHT_FOOT,
                Bone::RIGHT_TOES,
            ),
        ];
        let up = (to_skel * Vector3::UP).normalized();
        for ((upper, lower, foot, toes), lift) in legs.into_iter().zip(lift) {
            let reach = lift + drop;
            if reach.abs() < 1e-3 {
                continue;
            }
            let (upper, lower, foot, toes) = (bone(upper), bone(lower), bone(foot), bone(toes));
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
            let ball = skel.get_bone_global_pose(toes).origin - chain.end;
            let solution = ik::solve_two_bone(chain, target, pole);
            apply_global_rotation(skel, upper, solution.upper);
            apply_global_rotation(skel, lower, solution.lower);
            // The foot keeps the angle the animation gave it, unless the
            // straight leg still falls short of lower ground: then the toe
            // points down to it, so the ball of the foot reaches the stair.
            let short = (solution.end - target).dot(up);
            let placed = skel.get_bone_global_pose(foot);
            let basis = toe_down(flat, ball, up, short);
            skel.set_bone_global_pose(foot, Transform3D::new(basis, placed.origin));
        }
    }

    /// Fits finger poses for the weapon in hand to `fields` (its surface),
    /// from where the hands were last posed on it: the firing hand's fingers
    /// on the grip, its index off the trigger, and the support hand's,
    /// starting from the hold's shapes. `None` until the hands have held a
    /// weapon.
    pub fn fit_hands(&self, fields: &WeaponFields, grips: &Grips) -> Option<FittedHands> {
        let [Some(right_at), Some(left_at)] = self.held_target else {
            return None;
        };
        if self.fingers.iter().flatten().any(Option::is_none) {
            return None;
        }
        let shapes = [
            firing_shape(grips.hold),
            support_shape(grips.hold, grips.foregrip),
        ];
        let fit = |h: usize, at: Transform3D, field: &grasp::Field, shape: &HandShape| {
            let left = h == 1;
            let knuckles: Vec<_> = self.fingers[h]
                .iter()
                .enumerate()
                .filter_map(|(f, finger)| {
                    let (_, chain) = finger.as_ref()?;
                    let base = shape.joint(f, 0, left, shape.flex[f][0]);
                    Some(grasp::knuckle(chain, base, left))
                })
                .collect();
            let seat = grasp::seat(field, at, &knuckles);
            let at = Transform3D::new(at.basis, at.origin + seat);
            let contact = Contact {
                field,
                hand_to_field: at,
                left,
            };
            let mut flex = shape.flex;
            let mut spread = [0.0; 5];
            let mut depth = [f32::MIN; 5];
            for (f, finger) in self.fingers[h].iter().enumerate() {
                spread[f] = shape.base(f);
                let Some((_, chain)) = finger else { continue };
                let joint = |j: usize, a: f32, s: f32| HandShape::turned(j, left, a, s);
                // The trigger finger threads the guard: it may turn further.
                let wide = !left && f == 0;
                (flex[f], spread[f]) = grasp::fit_finger(
                    chain,
                    &contact,
                    (shape.flex[f], shape.base(f)),
                    (true, wide, true),
                    joint,
                );
                depth[f] = grasp::finger_depth(chain, &contact, flex[f], spread[f], joint);
            }
            (flex, spread, seat, depth, grasp::palm_depth(&contact))
        };
        let (right, right_spread, right_seat, right_depth, right_palm) =
            fit(0, right_at, fields.right.as_ref()?, &shapes[0]);
        let (left, left_spread, left_seat, left_depth, left_palm) =
            fit(1, left_at, fields.left.as_ref()?, &shapes[1]);
        let right_at = Transform3D::new(right_at.basis, right_at.origin + right_seat);
        // The index laid along the frame: only kept out of it.
        let (_, index) = self.fingers[0][0].as_ref()?;
        let contact = Contact {
            field: fields.right.as_ref()?,
            hand_to_field: right_at,
            left: false,
        };
        // Off the trigger the index keeps the spread it has on it (the game
        // blends only its curl).
        let joint = |j: usize, a: f32, s: f32| HandShape::turned(j, false, a, s);
        let (index_off, _) = grasp::fit_finger(
            index,
            &contact,
            (INDEX_OFF_TRIGGER, right_spread[0]),
            (false, true, false),
            joint,
        );
        let index_depth = grasp::finger_depth(index, &contact, index_off, right_spread[0], joint);
        Some(FittedHands {
            right,
            index_off,
            left,
            spreads: [right_spread, left_spread],
            seats: [right_seat, left_seat],
            depths: HandDepths {
                fingers: [right_depth, left_depth],
                index_off: index_depth,
                palms: [right_palm, left_palm],
                misses: [0.0; 2],
            },
        })
    }

    /// How deep the hands' drawn finger poses (the weapon's fitted poses, or
    /// the hold's shapes where it has none) go into `fields`, where the hands
    /// were last aimed: per hand and finger, the index off the trigger, and
    /// each palm (m; negative: clear); and how far each drawn hand fell
    /// short of where it was aimed. `None` until the hands have held a
    /// weapon.
    pub fn hand_depths(&self, fields: &WeaponFields, grips: &Grips) -> Option<HandDepths> {
        let [Some(right_aim), Some(left_aim)] = self.held_target else {
            return None;
        };
        let [Some(right_drawn), Some(left_drawn)] = self.held else {
            return None;
        };
        if self.fingers.iter().flatten().any(Option::is_none) {
            return None;
        }
        let on = grips.on_grips();
        // Where the hands are aimed, seat and all: the same every frame,
        // where the drawn hands sway with the arms by a hair.
        let seated = |h: usize, aim: Transform3D| {
            Transform3D::new(aim.basis, aim.origin + grips.seats[h] * on[h])
        };
        let (right_at, left_at) = (seated(0, right_aim), seated(1, left_aim));
        let misses = [
            right_drawn.origin.distance_to(right_at.origin),
            left_drawn.origin.distance_to(left_at.origin),
        ];
        let shapes = grips.shapes(on);
        let mut fingers = [[f32::MIN; 5]; 2];
        let mut palms = [f32::MIN; 2];
        let mut index_off = f32::MIN;
        for (h, (at, field)) in [(right_at, &fields.right), (left_at, &fields.left)]
            .into_iter()
            .enumerate()
        {
            let field = field.as_ref()?;
            let left = h == 1;
            let contact = Contact {
                field,
                hand_to_field: at,
                left,
            };
            let shape = &shapes[h];
            let joint = |j: usize, a: f32, s: f32| HandShape::turned(j, left, a, s);
            for (f, finger) in self.fingers[h].iter().enumerate() {
                let Some((_, chain)) = finger else { continue };
                fingers[h][f] =
                    grasp::finger_depth(chain, &contact, shape.flex[f], shape.base(f), joint);
                if h == 0 && f == 0 {
                    let off = grips.index_off.unwrap_or(INDEX_OFF_TRIGGER);
                    index_off = grasp::finger_depth(chain, &contact, off, shape.base(0), joint);
                }
            }
            palms[h] = grasp::palm_depth(&contact);
        }
        Some(HandDepths {
            fingers,
            index_off,
            palms,
            misses,
        })
    }

    /// A candidate pose for one hand (0 firing, 1 support) where it was last
    /// aimed on the weapon in hand, seated `seat` off its marker: each
    /// finger's depth into `fields` (m; negative: clear) and the points
    /// along it (weapon space) with its radius there. For offline tools that
    /// tune a pose by hand (`FpArms::try_hand`); `None` until the hands have
    /// held a weapon.
    pub fn try_hand(
        &self,
        fields: &WeaponFields,
        hand: usize,
        flex: FingerFlex,
        spread: [f32; 5],
        seat: Vector3,
    ) -> Option<HandProbe> {
        let aim = self.held_target.get(hand).copied().flatten()?;
        let field = if hand == 0 {
            fields.right.as_ref()?
        } else {
            fields.left.as_ref()?
        };
        let left = hand == 1;
        let contact = Contact {
            field,
            hand_to_field: Transform3D::new(aim.basis, aim.origin + seat),
            left,
        };
        let joint = |j: usize, a: f32, s: f32| HandShape::turned(j, left, a, s);
        let mut probe = HandProbe::default();
        for (f, finger) in self.fingers[hand].iter().enumerate() {
            let (_, chain) = finger.as_ref()?;
            probe.depths[f] = grasp::finger_depth(chain, &contact, flex[f], spread[f], joint);
            probe.points[f] = grasp::finger_points(chain, &contact, flex[f], spread[f], joint);
        }
        Some(probe)
    }

    /// Puts the feet on a ladder's rungs: each leg reaches its ankle to the
    /// rung, the knee bending toward the ladder.
    fn climb_legs(&self, skel: &mut Gd<Skeleton3D>, bones: [i32; BONES], ladder: &LadderLimbs) {
        let bone = |i: usize| bones[i];
        let to_skel = skel.get_global_transform().affine_inverse();
        let toward = (to_skel.basis * ladder.facing).normalized();
        let legs = [
            (
                Bone::LEFT_UPPER_LEG,
                Bone::LEFT_LOWER_LEG,
                Bone::LEFT_FOOT,
                Bone::LEFT_TOES,
            ),
            (
                Bone::RIGHT_UPPER_LEG,
                Bone::RIGHT_LOWER_LEG,
                Bone::RIGHT_FOOT,
                Bone::RIGHT_TOES,
            ),
        ];
        let weight = ladder.weight.clamp(0.0, 1.0);
        for ((upper, lower, foot, _), ankle) in legs.into_iter().zip(ladder.ankles) {
            let (upper, lower, foot) = (bone(upper), bone(lower), bone(foot));
            let chain = Chain {
                root: skel.get_bone_global_pose(upper).origin,
                mid: skel.get_bone_global_pose(lower).origin,
                end: skel.get_bone_global_pose(foot).origin,
            };
            let target = chain.end.lerp(to_skel * ankle, weight);
            let pole = chain.mid + toward * 0.5 + Vector3::UP * 0.2;
            let flat = skel.get_bone_global_pose(foot).basis;
            let solution = ik::solve_two_bone(chain, target, pole);
            apply_global_rotation(skel, upper, solution.upper);
            apply_global_rotation(skel, lower, solution.lower);
            let placed = skel.get_bone_global_pose(foot);
            skel.set_bone_global_pose(foot, Transform3D::new(flat, placed.origin));
        }
    }

    /// Puts the hands on a ladder's rungs, fingers over them, elbows down
    /// and out.
    fn climb_arms(&self, skel: &mut Gd<Skeleton3D>, bones: [i32; BONES], ladder: &LadderLimbs) {
        let bone = |i: usize| bones[i];
        let to_skel = skel.get_global_transform().affine_inverse();
        let toward = (to_skel.basis * ladder.facing).normalized();
        let side = Vector3::UP.cross(toward).normalized();
        let arms = [
            (
                [
                    bone(Bone::RIGHT_UPPER_ARM),
                    bone(Bone::RIGHT_LOWER_ARM),
                    bone(Bone::RIGHT_HAND),
                ],
                ladder.hands[0],
                -1.0,
            ),
            (
                [
                    bone(Bone::LEFT_UPPER_ARM),
                    bone(Bone::LEFT_LOWER_ARM),
                    bone(Bone::LEFT_HAND),
                ],
                ladder.hands[1],
                1.0,
            ),
        ];
        // The fingers point at the rungs and a little up, to hook over them.
        let reach = (toward + Vector3::UP * 0.4).normalized();
        for (chain, wrist, out) in arms {
            let mid = skel.get_bone_global_pose(chain[1]).origin;
            let pole = mid + Vector3::DOWN * 0.5 + side * (0.3 * out) - toward * 0.1;
            let hand = skel.get_bone_global_pose(chain[2]).basis.orthonormalized();
            let turn = ik::rotation_between(hand.col_b(), reach);
            let basis = Basis::from_quaternion(turn) * hand;
            self.place_hand(
                skel,
                chain,
                Transform3D::new(basis, to_skel * wrist),
                Vector3::ZERO,
                pole,
                ladder.weight.clamp(0.0, 1.0),
            );
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
            for (h, side) in ["Right", "Left"].into_iter().enumerate() {
                for (f, finger) in FINGERS.iter().enumerate() {
                    let thumb = *finger == "Thumb";
                    let joints: [&str; 3] = if thumb {
                        ["Metacarpal", "Proximal", "Distal"]
                    } else {
                        ["Proximal", "Intermediate", "Distal"]
                    };
                    let found =
                        joints.map(|joint| skel.find_bone(&format!("{side}{finger}{joint}")));
                    self.fingers[h][f] = found.iter().all(|b| *b >= 0).then(|| {
                        let rest = found.map(|b| skel.get_bone_rest(b));
                        // The fingertip, past the last joint: measured on the
                        // gloves, nine tenths of the middle segment's length
                        // on a finger, two thirds of it on the thumb.
                        let tip = rest[2].origin.length() * if thumb { 0.64 } else { 0.92 };
                        let chain = grasp::Chain {
                            offsets: rest.map(|r| r.origin),
                            rests: rest.map(|r| r.basis.get_quaternion()),
                            tip,
                            thumb,
                        };
                        (found, chain)
                    });
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
        )
        .scaled(Vector3::splat(self.hand_scale));
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

/// A foot's basis `flat` turned so its ball (`ball`, from the ankle) is
/// `short` lower along `up`, as far as [`MAX_TOE_REACH`] allows: the toe
/// pointing down to ground the straight leg cannot reach.
fn toe_down(flat: Basis, ball: Vector3, up: Vector3, short: f32) -> Basis {
    let length = ball.length();
    let axis = ball.cross(up).try_normalized();
    let (Some(axis), true) = (axis, short > 1e-3 && length > 1e-3) else {
        return flat;
    };
    // The ball's angle below level now, and the one that lowers it `short`.
    let below = (-ball.dot(up) / length).clamp(-1.0, 1.0).asin();
    let want = ((-ball.dot(up) + short) / length).clamp(-1.0, 1.0).asin();
    let turn = (want.min(MAX_TOE_REACH) - below).max(0.0);
    // Turning about `ball × up` takes the ball toward `up`; the other way
    // takes it down.
    Basis::from_axis_angle(axis, -turn) * flat
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
