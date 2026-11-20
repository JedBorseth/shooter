//! Ragdoll deaths: a dead character's body becomes twelve rigid capsules
//! joined at the hips, spine, neck, shoulders, elbows, hips, and knees, pushed
//! at the part the killing shot hit, and left lying where it lands.
//!
//! It is cosmetic. Dedicated servers never draw characters, so they never
//! build one; hits and kills are decided by the hit zones in
//! `shooter_sim::hitbox`, whose radii size the capsules here.
//!
//! - The capsules sit on their own collision layer ("ragdoll") that touches
//!   only the world: they never block players, bullets, or each other.
//! - The body starts from the exact pose the character was last drawn in
//!   (the poser captures it), moving at the character's velocity.
//! - For its first half second a shot body buckles (see [`Buckle`]): short
//!   lived muscles bend its knees, hips and spine toward a plan chosen from
//!   where and from which side the round struck, so it folds at the knees
//!   and goes down forward or sideways instead of toppling back stiff.
//! - Each frame the capsules' transforms are written into the skeleton's own
//!   bone poses. Once the body comes to rest (every capsule asleep), after
//!   [`MAX_SECONDS`], or when more than [`MAX_SIMULATING`] ragdolls are
//!   moving, it freezes: the capsules are freed and the skeleton keeps the
//!   last pose, so a corpse costs nothing but its drawing.

use std::cell::RefCell;
use std::collections::VecDeque;

use godot::classes::cone_twist_joint_3d::Param as ConeParam;
use godot::classes::hinge_joint_3d::{Flag as HingeFlag, Param as HingeParam};
use godot::classes::physics_server_3d::ConeTwistJointParam;
use godot::classes::{
    CapsuleShape3D, CollisionShape3D, ConeTwistJoint3D, HingeJoint3D, Joint3D, Node3D,
    PhysicsMaterial, PhysicsServer3D, RigidBody3D, Skeleton3D,
};
use godot::prelude::*;
use shooter_sim::hitbox::{self, BodyPart};

/// Collision layer 8 ("ragdoll"), as a bit mask: dead bodies and dropped guns.
pub const RAGDOLL_LAYER: u32 = 1 << 7;
/// Collision layer 1 ("world").
const WORLD_LAYER: u32 = 1;
/// At most this many ragdolls move at once; starting another freezes the
/// oldest. Zombies die in crowds, so they keep fewer.
pub const MAX_SIMULATING: usize = 6;
pub const MAX_SIMULATING_ZOMBIES: usize = 4;
/// A ragdoll still moving after this long freezes where it is (seconds).
pub const MAX_SECONDS: f32 = 5.0;
/// How fast a body that has stopped falling loses its speed along the
/// ground (1/s).
const GROUND_DRAG: f32 = 2.5;
/// Slower than this along the ground (m/s), a part is left to friction.
const DRAG_STOP: f32 = 0.2;
/// The capsules' damping while the body falls (linear, angular; 1/s).
const FALL_DAMP: (f32, f32) = (0.1, 1.5);
/// A settled body counts as at rest once every part has moved slower than
/// this (m/s, and rad/s about itself) for [`QUIET_SECONDS`]. Physics sleep
/// alone is not enough: a capsule rocking against a joint's limit, or
/// spinning slowly about its own length, keeps the whole body awake until
/// the [`MAX_SECONDS`] cap froze it, sometimes mid-fall with a limb up.
const QUIET_SPEED: f32 = 0.08;
const QUIET_SPIN: f32 = 0.6;
const QUIET_SECONDS: f32 = 0.25;
/// Whole-body mass (kg).
const MASS: f32 = 70.0;
/// The arms do not go limp at once: for the first moments their joints stay
/// near the pose the body died in, then they loosen to their full range.
/// Seconds held stiff, seconds to loosen, and the share of each joint's
/// range allowed while stiff. A body that buckles (see [`Buckle`]) frees its
/// legs and torso sooner.
const STIFF_SECONDS: f32 = 0.18;
const LOOSEN_SECONDS: f32 = 0.45;
const STIFF_RANGE: f32 = 0.12;
/// Without a buckle, the legs would stay stiff longer than the body.
const LEG_STIFF_SECONDS: f32 = 0.4;
const LEG_LOOSEN_SECONDS: f32 = 0.6;
/// A knife kill folds where it stood, as Call of Duty's knife deaths do: the
/// legs give at once and the knees buckle, so the body is further down a
/// tenth of a second in than a shot body that buckles. Its
/// body and leg stiffness (stiff, loosen seconds), the hips' drop (m/s), and
/// how fast the knees are thrown forward (m/s).
const CRUMPLE_BODY: (f32, f32) = (0.06, 0.3);
const CRUMPLE_LEGS: (f32, f32) = (0.0, 0.2);
const CRUMPLE_DROP: f32 = 1.6;
const CRUMPLE_KNEES: f32 = 1.4;

/// Push (N·s) at the part the killing shot hit: a base plus a share per
/// force level (`protocol::HitPart::force`, about a tenth of the damage), so
/// a shotgun at close range or a sniper rifle pushes harder than a rifle.
const PUSH_BASE: f32 = 24.0;
const PUSH_PER_LEVEL: f32 = 4.0;
/// A headshot pushes the head this much harder.
const HEAD_PUSH_SCALE: f32 = 0.85;

/// The hips' cone (degrees). While the body topples, the cones are centred
/// on the pose it died in; once loosened, each is rebuilt with explicit
/// frames centred on the limb's natural rest (see [`RestFrame`]), so a
/// hunched zombie no longer comes to rest with a leg or an arm raised.
const HIP_SWING: f32 = 60.0;
/// The shoulders' cone (degrees), about the arm hanging down and a little
/// out.
const SHOULDER_SWING: f32 = 80.0;
/// Seconds over which a rest-centred cone narrows from the swing the limb
/// has when it is built to its own limit, so limbs settle instead of
/// snapping into range.
const SETTLE_SECONDS: f32 = 0.6;
/// While it settles, a limb is pulled weakly toward its rest (N·m at
/// half a turn off, less nearer), as slack muscle would let it slump: a
/// body on its back does not stay balanced on its feet with its knees up.
const SLUMP_TORQUE: f32 = 20.0;
/// Seconds the slump lasts after a cone is rebuilt.
const SLUMP_SECONDS: f32 = 1.2;
/// Once a cone is rebuilt about the limb's rest, it narrows to this share
/// of the swing it had while the body toppled: a dead body seldom keeps a
/// hip flexed or a shoulder raised as far as a living one can.
const REST_SWING_SHARE: f32 = 0.8;
/// How far out a hanging arm's rest leans from straight down: this share
/// of the skeleton's rest arm direction (a T or A pose) on top of down.
const ARM_HANG_OUT: f32 = 0.35;

/// One end of a part's capsule: a bone's origin, moved along the bone's own
/// +Y (toward its child) by `along` metres.
#[derive(Clone, Copy)]
struct End {
    bone: &'static str,
    along: f32,
}

const fn at(bone: &'static str) -> End {
    End { bone, along: 0.0 }
}

#[derive(Clone, Copy)]
enum JointKind {
    /// Swing (cone half-angle) and twist limits in degrees, around the pose
    /// the body died in.
    Cone { swing: f32, twist: f32 },
    /// A hinge that bends only one way, up to `max` degrees from straight.
    /// `toward_front` bends the child toward the body's front (an elbow);
    /// otherwise toward its back (a knee).
    Hinge { max: f32, toward_front: bool },
}

#[derive(Clone, Copy)]
struct PartSpec {
    part: BodyPart,
    /// The bone the capsule drives. Bones between parts (the upper chest, the
    /// shoulders) and past them (hands, feet, fingers) keep their pose
    /// relative to their parent.
    bone: &'static str,
    from: End,
    to: End,
    radius: f32,
    /// Share of [`MASS`].
    mass: f32,
    parent: Option<BodyPart>,
    joint: JointKind,
}

/// The parts, parents before children. The capsules are a little slimmer
/// than the hit zones, which are sized generously for hitting.
const PARTS: [PartSpec; 12] = [
    PartSpec {
        part: BodyPart::Hips,
        bone: "Hips",
        from: at("LeftUpperLeg"),
        to: at("RightUpperLeg"),
        radius: hitbox::STOMACH_RADIUS * 0.75,
        mass: 0.15,
        parent: None,
        joint: JointKind::Cone {
            swing: 0.0,
            twist: 0.0,
        },
    },
    PartSpec {
        part: BodyPart::Spine,
        bone: "Spine",
        from: at("Spine"),
        to: at("Chest"),
        radius: hitbox::STOMACH_RADIUS * 0.75,
        mass: 0.12,
        parent: Some(BodyPart::Hips),
        joint: JointKind::Cone {
            swing: 30.0,
            twist: 20.0,
        },
    },
    PartSpec {
        part: BodyPart::Chest,
        bone: "Chest",
        from: at("Chest"),
        to: at("Neck"),
        radius: hitbox::CHEST_RADIUS * 0.75,
        mass: 0.22,
        parent: Some(BodyPart::Spine),
        joint: JointKind::Cone {
            swing: 30.0,
            twist: 20.0,
        },
    },
    PartSpec {
        part: BodyPart::Head,
        bone: "Neck",
        from: End {
            bone: "Neck",
            along: 0.04,
        },
        to: End {
            bone: "Head",
            along: 0.1,
        },
        radius: hitbox::HEAD_RADIUS * 0.75,
        mass: 0.08,
        parent: Some(BodyPart::Chest),
        joint: JointKind::Cone {
            swing: 40.0,
            twist: 45.0,
        },
    },
    PartSpec {
        part: BodyPart::LeftUpperArm,
        bone: "LeftUpperArm",
        from: at("LeftUpperArm"),
        to: at("LeftLowerArm"),
        radius: hitbox::UPPER_ARM_RADIUS,
        mass: 0.03,
        parent: Some(BodyPart::Chest),
        joint: JointKind::Cone {
            swing: SHOULDER_SWING,
            twist: 45.0,
        },
    },
    PartSpec {
        part: BodyPart::LeftLowerArm,
        bone: "LeftLowerArm",
        from: at("LeftLowerArm"),
        to: End {
            bone: "LeftHand",
            along: 0.07,
        },
        radius: hitbox::FOREARM_RADIUS,
        mass: 0.025,
        parent: Some(BodyPart::LeftUpperArm),
        joint: JointKind::Hinge {
            max: 140.0,
            toward_front: true,
        },
    },
    PartSpec {
        part: BodyPart::RightUpperArm,
        bone: "RightUpperArm",
        from: at("RightUpperArm"),
        to: at("RightLowerArm"),
        radius: hitbox::UPPER_ARM_RADIUS,
        mass: 0.03,
        parent: Some(BodyPart::Chest),
        joint: JointKind::Cone {
            swing: SHOULDER_SWING,
            twist: 45.0,
        },
    },
    PartSpec {
        part: BodyPart::RightLowerArm,
        bone: "RightLowerArm",
        from: at("RightLowerArm"),
        to: End {
            bone: "RightHand",
            along: 0.07,
        },
        radius: hitbox::FOREARM_RADIUS,
        mass: 0.025,
        parent: Some(BodyPart::RightUpperArm),
        joint: JointKind::Hinge {
            max: 140.0,
            toward_front: true,
        },
    },
    PartSpec {
        part: BodyPart::LeftUpperLeg,
        bone: "LeftUpperLeg",
        from: at("LeftUpperLeg"),
        to: at("LeftLowerLeg"),
        radius: hitbox::THIGH_RADIUS * 0.9,
        mass: 0.1,
        parent: Some(BodyPart::Hips),
        joint: JointKind::Cone {
            swing: HIP_SWING,
            twist: 20.0,
        },
    },
    PartSpec {
        part: BodyPart::LeftLowerLeg,
        bone: "LeftLowerLeg",
        from: at("LeftLowerLeg"),
        to: at("LeftFoot"),
        radius: hitbox::SHIN_RADIUS,
        mass: 0.06,
        parent: Some(BodyPart::LeftUpperLeg),
        joint: JointKind::Hinge {
            max: 140.0,
            toward_front: false,
        },
    },
    PartSpec {
        part: BodyPart::RightUpperLeg,
        bone: "RightUpperLeg",
        from: at("RightUpperLeg"),
        to: at("RightLowerLeg"),
        radius: hitbox::THIGH_RADIUS * 0.9,
        mass: 0.1,
        parent: Some(BodyPart::Hips),
        joint: JointKind::Cone {
            swing: HIP_SWING,
            twist: 20.0,
        },
    },
    PartSpec {
        part: BodyPart::RightLowerLeg,
        bone: "RightLowerLeg",
        from: at("RightLowerLeg"),
        to: at("RightFoot"),
        radius: hitbox::SHIN_RADIUS,
        mass: 0.06,
        parent: Some(BodyPart::RightUpperLeg),
        joint: JointKind::Hinge {
            max: 140.0,
            toward_front: false,
        },
    },
];

/// One bone's local pose: position, rotation, scale.
pub type BonePose = (Vector3, Quaternion, Vector3);

/// Where and how hard the killing shot pushes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Push {
    /// Direction the shot travelled (world).
    pub direction: Vector3,
    /// The part it hit, if known (otherwise the chest).
    pub part: Option<BodyPart>,
    /// Where it hit (world), if known.
    pub point: Option<Vector3>,
    /// Force level 1 to 15 (`protocol::HitPart::force`).
    pub force: f32,
    /// A knife: the body crumples where it stands ([`Ragdoll::start`])
    /// instead of toppling stiff-legged with the push.
    pub knife: bool,
}

impl Push {
    /// The impulse (N·s) along the shot.
    pub fn impulse(&self) -> Vector3 {
        let Some(flat) = Vector3::new(self.direction.x, 0.0, self.direction.z).try_normalized()
        else {
            return Vector3::ZERO;
        };
        let head = if self.part == Some(BodyPart::Head) {
            HEAD_PUSH_SCALE
        } else {
            1.0
        };
        // Mostly along the ground: a round knocks a body over, never up.
        let along = (flat + Vector3::DOWN * 0.15).normalized();
        along * (PUSH_BASE + PUSH_PER_LEVEL * self.force.clamp(0.0, 15.0)) * head
    }
}

/// How a shot body buckles in its first moments, before it goes limp. Call
/// of Duty's deaths give at the knees and fold over: a soldier shot from the
/// front hunches over the wound and drops onto the near knee, one shot from
/// the side folds toward the push, a leg hit goes down on that leg, and a
/// headshot drops where it stood. The push still moves the body; short-lived
/// "muscles" (see [`Muscle`]) bend each joint toward this plan, so the body
/// no longer topples backward stiff as a plank.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Buckle {
    /// Each knee's bend from straight (degrees): left, right.
    pub knees: [f32; 2],
    /// Each hip's flex, the thigh forward of the hips (degrees): left, right.
    pub hips: [f32; 2],
    /// Forward bend of the spine and chest together (degrees).
    pub bend: f32,
    /// The head's nod forward (degrees).
    pub nod: f32,
    /// Side bend of the spine and chest together (degrees, + toward the
    /// body's right).
    pub lean: f32,
    /// Turn of the spine and chest together (degrees, + turning the chest's
    /// front toward the body's right): the struck shoulder goes back.
    pub twist: f32,
    /// How fast the hips drop as the legs give (m/s).
    pub drop: f32,
    /// How fast the upper body is carried toward the side it falls to
    /// (m/s, + toward the body's right): a body shot square on turns away
    /// from the struck shoulder and goes down on that side.
    pub sway: f32,
    /// Seconds the muscles hold the plan before fading over
    /// [`BUCKLE_FADE`].
    pub hold: f32,
}

impl Buckle {
    /// A death with nothing known about the shot: the legs give and the
    /// body folds where it stands.
    pub const LIMP: Buckle = Buckle {
        knees: [90.0, 75.0],
        hips: [60.0, 50.0],
        bend: 15.0,
        nod: 10.0,
        lean: 0.0,
        twist: 0.0,
        drop: 0.5,
        sway: 0.0,
        hold: 0.2,
    };
}

/// Seconds over which the muscles let go after [`Buckle::hold`].
const BUCKLE_FADE: f32 = 0.3;
/// How fast a muscle closes on its target (1/s): the share of the error it
/// asks to turn per second.
const MUSCLE_GAIN: f32 = 9.0;
/// How fast a muscle's wanted spin replaces the joint's own (1/s).
const MUSCLE_RATE: f32 = 40.0;
/// While a body buckles, the spine, chest and neck may move this share of
/// their range about the pose it died in (stiff bodies hold
/// [`STIFF_RANGE`]): enough to fold forward, not enough to bow back far.
const BUCKLE_TORSO_RANGE: f32 = 1.0;
/// A buckling torso stays braced for this long (seconds), then opens to
/// [`BUCKLE_TORSO_RANGE`] over the next: the push tips the whole body
/// instead of bowing the spine back over legs that have given.
const BUCKLE_BRACE: (f32, f32) = (0.08, 0.12);
/// A force level from this up (a close shotgun, a sniper rifle) throws the
/// body: it folds less, and the push carries it.
const HEAVY_FORCE: f32 = 10.0;

impl Push {
    /// How the body hit by this push buckles, for a body facing `front`
    /// whose hips are at `hips` (world).
    pub fn buckle(&self, front: Vector3, hips: Vector3) -> Buckle {
        let flat = |v: Vector3| Vector3::new(v.x, 0.0, v.z).try_normalized();
        let front = flat(front).unwrap_or(Vector3::BACK);
        let right = front.cross(Vector3::UP);
        let travel = flat(self.direction).unwrap_or(-front);
        // + the shot travels the way the body faces (it came from behind);
        // + it travels toward the body's right (it came from its left).
        let ahead = travel.dot(front);
        let side = travel.dot(right);
        // The side the body goes down on (+1 right): with the push when it
        // is from the side; otherwise the side of the body the round struck,
        // which turns that shoulder away and drops it first.
        let struck = self.point.map_or(0.0, |p| (p - hips).dot(right));
        let fall = if side.abs() > 0.35 {
            side.signum()
        } else if struck.abs() > 0.03 {
            struck.signum()
        } else {
            // Square on: either way, but the same for the same shot.
            let p = self.point.unwrap_or(hips);
            if ((p.x * 97.0 + p.z * 61.0).floor() as i64).rem_euclid(2) == 0 {
                1.0
            } else {
                -1.0
            }
        };
        // Index 0 is left, 1 right; `near` is the side it falls to.
        let sides = |near: f32, far: f32| {
            if fall > 0.0 { [far, near] } else { [near, far] }
        };
        let front_share = (-ahead).max(0.0);
        let back_share = ahead.max(0.0);
        let side_share = side.abs();
        let mut plan = match self.part {
            Some(BodyPart::Head) => Buckle {
                // Out at once: the legs fold under it where it stands.
                knees: sides(110.0, 90.0),
                hips: sides(70.0, 55.0),
                bend: 35.0,
                nod: 0.0,
                lean: fall * (8.0 + 16.0 * side_share + 10.0 * front_share),
                twist: fall * 20.0 * front_share,
                drop: 0.7,
                sway: fall * 0.4 * front_share,
                hold: 0.2,
            },
            Some(
                leg @ (BodyPart::LeftUpperLeg
                | BodyPart::LeftLowerLeg
                | BodyPart::RightUpperLeg
                | BodyPart::RightLowerLeg),
            ) => {
                // The hit leg gives; the body goes down over it.
                let hit = if matches!(leg, BodyPart::LeftUpperLeg | BodyPart::LeftLowerLeg) {
                    -1.0
                } else {
                    1.0
                };
                let pick = |hit_leg: f32, other: f32| {
                    if hit > 0.0 {
                        [other, hit_leg]
                    } else {
                        [hit_leg, other]
                    }
                };
                Buckle {
                    knees: pick(105.0, 50.0),
                    hips: pick(60.0, 35.0),
                    bend: 15.0,
                    nod: 15.0,
                    lean: hit * 15.0,
                    twist: 0.0,
                    drop: 0.5,
                    sway: 0.0,
                    hold: 0.3,
                }
            }
            _ => Buckle {
                // Hunched over a wound in front, folded with the push from
                // the side, onto the knees and forward from behind.
                knees: sides(100.0, 65.0 - 20.0 * front_share),
                // As the knees go forward the hips flex as far, so the
                // torso stays over them instead of tipping back.
                hips: sides(70.0, 50.0),
                bend: 55.0 * front_share + 12.0 * back_share + 16.0 * side_share,
                nod: 15.0,
                lean: fall * (6.0 + 14.0 * side_share + 14.0 * front_share),
                twist: fall * 30.0 * front_share,
                drop: 0.5,
                sway: fall * 0.7 * front_share,
                hold: 0.25 + 0.15 * front_share,
            },
        };
        if self.force >= HEAVY_FORCE {
            // Thrown: less fold, the push does the rest.
            plan.bend *= 0.5;
            plan.drop *= 0.6;
        }
        plan
    }
}

/// Holds a ragdoll's capsules and joints, and pulls its muscles each
/// physics tick while the body buckles.
#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct RagdollHolder {
    base: Base<Node3D>,
    muscles: Vec<Muscle>,
    /// Seconds the muscles hold before letting go.
    hold: f32,
    age: f32,
}

#[godot_api]
impl INode3D for RagdollHolder {
    fn physics_process(&mut self, delta: f64) {
        let _profile = crate::profile::scope("ragdolls");
        if self.muscles.is_empty() {
            return;
        }
        let dt = delta as f32;
        self.age += dt;
        let strength = 1.0 - ((self.age - self.hold) / BUCKLE_FADE).clamp(0.0, 1.0);
        if strength <= 0.0 {
            self.muscles.clear();
            return;
        }
        for muscle in &mut self.muscles {
            muscle.pull(strength, dt);
        }
    }
}

/// A short-lived pull between two parts toward a planned bend: each step it
/// sets the child's spin relative to the parent's toward closing the gap,
/// shared by mass so the pair's momentum is kept and the body still falls
/// as the push and gravity take it.
struct Muscle {
    parent: Gd<RigidBody3D>,
    child: Gd<RigidBody3D>,
    /// The child's wanted rotation relative to the parent's.
    target: Quaternion,
    /// The child's share of the change (the parent takes the rest).
    child_share: f32,
}

impl Muscle {
    /// A muscle that turns `child` by `turn` (world, about the joint) from
    /// where it is now relative to `parent`.
    fn new(parent: &Gd<RigidBody3D>, child: &Gd<RigidBody3D>, turn: Basis) -> Muscle {
        let p = parent.get_global_basis().orthonormalized().get_quaternion();
        let c = child.get_global_basis().orthonormalized().get_quaternion();
        let wanted = turn.orthonormalized().get_quaternion() * c;
        let (pm, cm) = (parent.get_mass(), child.get_mass());
        Muscle {
            parent: parent.clone(),
            child: child.clone(),
            target: (p.inverse() * wanted).normalized(),
            child_share: pm / (pm + cm).max(1e-3),
        }
    }

    /// Pulls toward the target with `strength` (0..1) over `dt` seconds.
    fn pull(&mut self, strength: f32, dt: f32) {
        if strength <= 0.0 || !self.parent.is_instance_valid() || !self.child.is_instance_valid() {
            return;
        }
        let p = self
            .parent
            .get_global_basis()
            .orthonormalized()
            .get_quaternion();
        let c = self
            .child
            .get_global_basis()
            .orthonormalized()
            .get_quaternion();
        let mut error = self.target * (p.inverse() * c).inverse();
        if error.w < 0.0 {
            error = -error;
        }
        let angle = 2.0 * error.w.clamp(-1.0, 1.0).acos();
        let axis = Vector3::new(error.x, error.y, error.z)
            .try_normalized()
            .unwrap_or(Vector3::ZERO);
        let wanted = Basis::from_quaternion(p) * axis * angle * MUSCLE_GAIN;
        let (pw, cw) = (
            self.parent.get_angular_velocity(),
            self.child.get_angular_velocity(),
        );
        let change = (wanted - (cw - pw)) * strength * (1.0 - (-MUSCLE_RATE * dt).exp());
        self.child
            .set_angular_velocity(cw + change * self.child_share);
        self.parent
            .set_angular_velocity(pw - change * (1.0 - self.child_share));
    }
}

/// A joint and its full limits.
struct Joint {
    node: Gd<Joint3D>,
    kind: JointKind,
    /// Hinges: their limits relative to the starting pose.
    limits: (f32, f32),
    /// A hip or knee: loosens later.
    leg: bool,
    /// The spine, chest, or neck: may fold further while the body buckles.
    torso: bool,
    /// Cones: where the limb rests, to rebuild the joint around once loose.
    rest: Option<RestFrame>,
    /// The rebuilt cone (a physics server joint), the swing span it started
    /// with (radians), and seconds since.
    rebuilt: Option<(Rid, f32, f32)>,
}

/// A cone joint's natural centre, fixed when the ragdoll starts: the
/// child's direction at rest in the parent capsule's own frame (the
/// skeleton's rest pose, with arms hanging instead of out), and the pivot in
/// the parent capsule's frame. The node joints Godot builds take both frames from
/// one transform, the death pose; these are made through the physics server
/// with a frame for each body.
#[derive(Clone)]
struct RestFrame {
    parent: Gd<RigidBody3D>,
    child: Gd<RigidBody3D>,
    neutral: Vector3,
    pivot_parent: Vector3,
}

impl Joint {
    /// Sets the joint's range to `share` of its full range (0..1).
    fn set_range(&mut self, share: f32) {
        if self.rebuilt.is_some() {
            return;
        }
        match self.kind {
            JointKind::Cone { swing, twist } => {
                if let Ok(mut cone) = self.node.clone().try_cast::<ConeTwistJoint3D>() {
                    cone.set_param(
                        ConeParam::SWING_SPAN,
                        (swing * share).to_radians().max(0.02),
                    );
                    cone.set_param(
                        ConeParam::TWIST_SPAN,
                        (twist * share).to_radians().max(0.02),
                    );
                }
            }
            JointKind::Hinge { .. } => {
                if let Ok(mut hinge) = self.node.clone().try_cast::<HingeJoint3D>() {
                    hinge.set_param(HingeParam::LIMIT_LOWER, self.limits.0 * share);
                    hinge.set_param(HingeParam::LIMIT_UPPER, self.limits.1 * share);
                }
            }
        }
    }
}

impl Joint {
    /// Rebuilds a loosened cone about the limb's rest (see [`RestFrame`]):
    /// its swing span starts wide enough for where the limb is now and
    /// narrows to the cone's own over [`SETTLE_SECONDS`].
    fn rebuild_at_rest(&mut self) {
        let (Some(rest), JointKind::Cone { swing, twist }) = (&self.rest, self.kind) else {
            return;
        };
        if self.rebuilt.is_some()
            || !rest.parent.is_instance_valid()
            || !rest.child.is_instance_valid()
        {
            return;
        }
        let parent = rest.parent.get_global_transform();
        let child = rest.child.get_global_transform();
        let Some(centre) = (parent.basis * rest.neutral).try_normalized() else {
            return;
        };
        let pivot = parent * rest.pivot_parent;
        let frame = basis_with_x(centre);
        // The capsule lies along its own Y.
        let axis = child.basis.col_b().normalized();
        let now = centre.angle_to(axis);
        // The child's frame is the centre's, turned the shortest way onto
        // the limb as it lies now: no twist to start with.
        let onto = Basis::from_quaternion(super::ik::rotation_between(centre, axis)) * frame;
        let local_a = parent.affine_inverse() * Transform3D::new(frame, pivot);
        let local_b = child.affine_inverse() * Transform3D::new(onto, pivot);
        let mut server = PhysicsServer3D::singleton();
        let rid = server.joint_create();
        server.joint_make_cone_twist(
            rid,
            rest.parent.get_rid(),
            local_a,
            rest.child.get_rid(),
            local_b,
        );
        server.joint_disable_collisions_between_bodies(rid, true);
        // A body asleep in a raised pose wakes to settle.
        for mut body in [rest.parent.clone(), rest.child.clone()] {
            body.set_sleeping(false);
        }
        let start = now.max(swing.to_radians() * REST_SWING_SHARE) + 0.05;
        server.cone_twist_joint_set_param(rid, ConeTwistJointParam::SWING_SPAN, start);
        server.cone_twist_joint_set_param(rid, ConeTwistJointParam::TWIST_SPAN, twist.to_radians());
        self.node.queue_free();
        self.rebuilt = Some((rid, start, 0.0));
    }

    /// Narrows a rebuilt cone toward its own swing, and lets the limb slump
    /// toward its rest.
    fn settle(&mut self, dt: f32) {
        let (Some((rid, start, age)), JointKind::Cone { swing, .. }) =
            (&mut self.rebuilt, self.kind)
        else {
            return;
        };
        if *age >= SLUMP_SECONDS {
            return;
        }
        *age += dt;
        if let Some(rest) = &mut self.rest
            && rest.parent.is_instance_valid()
            && rest.child.is_instance_valid()
        {
            let parent = rest.parent.get_global_transform();
            let centre = (parent.basis * rest.neutral).normalized_or_zero();
            let axis = rest.child.get_global_transform().basis.col_b().normalized();
            let off = axis.angle_to(centre);
            if let Some(turn) = axis.cross(centre).try_normalized() {
                let torque = turn * SLUMP_TORQUE * (off / std::f32::consts::PI);
                rest.child.apply_torque_impulse(torque * dt);
                rest.parent.apply_torque_impulse(-torque * dt);
            }
        }
        if *age > SETTLE_SECONDS {
            return;
        }
        let t = (*age / SETTLE_SECONDS).clamp(0.0, 1.0);
        let ease = t * t * (3.0 - 2.0 * t);
        let span = *start + (swing.to_radians() * REST_SWING_SHARE - *start) * ease;
        PhysicsServer3D::singleton().cone_twist_joint_set_param(
            *rid,
            ConeTwistJointParam::SWING_SPAN,
            span,
        );
    }

    /// Frees a rebuilt cone's server joint.
    fn free(&mut self) {
        if let Some((rid, ..)) = self.rebuilt.take() {
            PhysicsServer3D::singleton().free_rid(rid);
        }
    }
}

struct Part {
    bone: i32,
    body: Gd<RigidBody3D>,
    /// The bone's skeleton-space transform relative to the capsule body.
    offset: Transform3D,
}

/// A body falling under physics.
pub struct Ragdoll {
    skeleton: Gd<Skeleton3D>,
    /// Holds the capsules and joints (each capsule is top level).
    holder: Gd<RagdollHolder>,
    parts: Vec<Part>,
    joints: Vec<Joint>,
    /// Every bone's local pose; the driven bones' are rewritten each frame.
    locals: Vec<BonePose>,
    parents: Vec<i32>,
    /// Bone indices, parents before children.
    order: Vec<usize>,
    age: f32,
    /// Seconds every part has been quiet (see [`QUIET_SPEED`]).
    quiet: f32,
    /// Whether it came to rest before [`MAX_SECONDS`] froze it.
    pub rested: bool,
    /// Seconds held stiff and seconds to loosen: the body's, then the legs'.
    body_stiff: (f32, f32),
    leg_stiff: (f32, f32),
    /// A buckling body: seconds its muscles hold (see [`Buckle`]).
    buckle: Option<f32>,
    id: InstanceId,
}

/// However a ragdoll ends (frozen, discarded, or dropped with its body when
/// a corpse sinks away, a respawn clears it, or the scene goes), its rebuilt
/// cone joints are physics server joints no node owns: free them here, or
/// each death leaks a dozen (`JoltJoint3D` RIDs at exit), and give up its
/// place in the cap.
impl Drop for Ragdoll {
    fn drop(&mut self) {
        release(self.id);
        for joint in &mut self.joints {
            joint.free();
        }
    }
}

thread_local! {
    /// Ragdolls moving now, oldest first, by owner.
    static SIMULATING: RefCell<VecDeque<InstanceId>> = const { RefCell::new(VecDeque::new()) };
}

/// The owners of ragdolls that should freeze now to keep at most `cap`
/// moving, oldest first, once `owner` starts one.
pub fn admit(owner: InstanceId, cap: usize) -> Vec<InstanceId> {
    SIMULATING.with_borrow_mut(|list| {
        list.retain(|id| *id != owner);
        list.push_back(owner);
        let mut over = Vec::new();
        while list.len() > cap.max(1) {
            if let Some(id) = list.pop_front() {
                over.push(id);
            }
        }
        over
    })
}

/// Ragdolls moving now (tests).
pub fn simulating() -> usize {
    SIMULATING.with_borrow(|list| list.len())
}

fn release(owner: InstanceId) {
    SIMULATING.with_borrow_mut(|list| list.retain(|id| *id != owner));
}

impl Ragdoll {
    /// Builds the ragdoll on `skeleton` from the pose it was last drawn in
    /// (`locals`, every bone's local pose), sets every part moving at
    /// `velocity`, and pushes it with `push`. `owner` names it in the cap.
    pub fn start(
        mut skeleton: Gd<Skeleton3D>,
        locals: Vec<BonePose>,
        velocity: Vector3,
        push: Option<Push>,
        owner: InstanceId,
    ) -> Option<Ragdoll> {
        let count = skeleton.get_bone_count() as usize;
        if locals.len() != count {
            return None;
        }
        // The drawn pose becomes the skeleton's own, so the body starts
        // exactly as it was last seen.
        for (i, (p, r, s)) in locals.iter().enumerate() {
            let i = i as i32;
            skeleton.set_bone_pose_position(i, *p);
            skeleton.set_bone_pose_rotation(i, *r);
            skeleton.set_bone_pose_scale(i, *s);
        }
        let parents: Vec<i32> = (0..count as i32)
            .map(|b| skeleton.get_bone_parent(b))
            .collect();
        let order = parents_first(&parents);
        let globals = globals_of(&locals, &parents, &order);
        let skel_xform = skeleton.get_global_transform();
        let names: Vec<String> = (0..count as i32)
            .map(|b| skeleton.get_bone_name(b).to_string())
            .collect();
        let bone = |name: &str| {
            names
                .iter()
                .position(|n| n == name)
                .map_or(-1, |i| i as i32)
        };
        let find = |end: End| -> Option<Vector3> {
            let b = bone(end.bone);
            (b >= 0).then(|| {
                let g = globals[b as usize];
                skel_xform * (g.origin + g.basis.col_b().normalized() * end.along)
            })
        };
        // The body's facing: the skeleton faces +Z.
        let front = (skel_xform.basis * Vector3::BACK).normalized();
        let rest_globals: Vec<Transform3D> = (0..count as i32)
            .map(|b| skeleton.get_bone_global_rest(b))
            .collect();
        let rest_at = |end: End| -> Option<Vector3> {
            let b = bone(end.bone);
            (b >= 0).then(|| {
                let g = rest_globals[b as usize];
                g.origin + g.basis.col_b().normalized() * end.along
            })
        };

        let mut holder = RagdollHolder::new_alloc();
        holder.set_name("Ragdoll");
        skeleton.add_child(&holder);
        let mut material = PhysicsMaterial::new_gd();
        material.set_friction(0.9);
        material.set_bounce(0.0);
        material.set_absorbent(true);

        let mut parts: Vec<Part> = Vec::new();
        let mut joints: Vec<Joint> = Vec::new();
        let mut built: Vec<(BodyPart, Gd<RigidBody3D>, Vector3, Vector3)> = Vec::new();
        for spec in PARTS {
            let b = bone(spec.bone);
            let (Some(a), Some(z)) = (find(spec.from), find(spec.to)) else {
                godot_error!("Ragdoll: no bone for {:?}", spec.part);
                return None;
            };
            if b < 0 {
                return None;
            }
            let axis = (z - a).try_normalized().unwrap_or(Vector3::UP);
            let length = a.distance_to(z);
            let mut shape = CapsuleShape3D::new_gd();
            shape.set_radius(spec.radius);
            shape.set_height(length + 2.0 * spec.radius);
            let mut collision = CollisionShape3D::new_alloc();
            collision.set_shape(&shape);
            let mut body = RigidBody3D::new_alloc();
            body.set_name(&format!("{:?}", spec.part));
            body.set_as_top_level(true);
            body.set_collision_layer(RAGDOLL_LAYER);
            body.set_collision_mask(WORLD_LAYER);
            body.set_mass(MASS * spec.mass);
            body.set_linear_damp(FALL_DAMP.0);
            body.set_angular_damp(FALL_DAMP.1);
            body.set_physics_material_override(&material);
            body.add_child(&collision);
            let transform = Transform3D::new(basis_with_y(axis), (a + z) * 0.5);
            holder.add_child(&body);
            body.set_global_transform(transform);
            body.set_linear_velocity(velocity);
            let bone_global = skel_xform * globals[b as usize];
            parts.push(Part {
                bone: b,
                body: body.clone(),
                offset: transform.affine_inverse() * bone_global,
            });
            if let Some(parent) = spec.parent
                && let Some((_, parent_body, _, _)) = built.iter().find(|(p, ..)| *p == parent)
            {
                let (mut joint, place, limits) =
                    make_joint(spec.joint, a, axis, built_axis(&built, parent), front);
                joint.set_as_top_level(true);
                holder.add_child(&joint);
                joint.set_global_transform(place);
                joint.set_node_a(&parent_body.get_path());
                joint.set_node_b(&body.get_path());
                let leg = matches!(
                    spec.part,
                    BodyPart::LeftUpperLeg
                        | BodyPart::RightUpperLeg
                        | BodyPart::LeftLowerLeg
                        | BodyPart::RightLowerLeg
                );
                let rest = matches!(spec.joint, JointKind::Cone { .. })
                    .then(|| {
                        let parent_spec = PARTS.iter().find(|p| p.part == parent)?;
                        let parent_bone = bone(parent_spec.bone);
                        let (from, to) = (rest_at(spec.from)?, rest_at(spec.to)?);
                        let (pfrom, pto) = (rest_at(parent_spec.from)?, rest_at(parent_spec.to)?);
                        let neutral = rest_neutral(spec.part, to - from, pto - pfrom)?;
                        // At rest, in the parent bone's frame; then in the
                        // parent capsule's frame as the body died.
                        let in_bone = rest_globals[parent_bone as usize]
                            .basis
                            .orthonormalized()
                            .inverse()
                            * neutral;
                        let world = (skel_xform * globals[parent_bone as usize])
                            .basis
                            .orthonormalized()
                            * in_bone;
                        let parent_place = parent_body.get_global_transform();
                        Some(RestFrame {
                            parent: parent_body.clone(),
                            child: body.clone(),
                            neutral: parent_place.basis.orthonormalized().inverse() * world,
                            pivot_parent: parent_place.affine_inverse() * a,
                        })
                    })
                    .flatten();
                let torso = matches!(
                    spec.part,
                    BodyPart::Spine | BodyPart::Chest | BodyPart::Head
                );
                let mut joint = Joint {
                    node: joint,
                    kind: spec.joint,
                    limits,
                    leg,
                    torso,
                    rest,
                    rebuilt: None,
                };
                joint.set_range(STIFF_RANGE);
                joints.push(joint);
            }
            built.push((spec.part, body, a, axis));
        }

        let crumple = push.is_some_and(|p| p.knife);
        if crumple {
            // The body drops as one (arms and head with it, so they do not
            // fly up as the hips fall away) and the knees go forward, so the
            // legs fold under it.
            for (part, body, ..) in &mut built {
                let v = body.get_linear_velocity();
                let kick = match part {
                    BodyPart::LeftLowerLeg | BodyPart::RightLowerLeg => front * CRUMPLE_KNEES,
                    BodyPart::LeftUpperLeg | BodyPart::RightUpperLeg => {
                        Vector3::DOWN * CRUMPLE_DROP * 0.5 + front * CRUMPLE_KNEES * 0.5
                    }
                    _ => Vector3::DOWN * CRUMPLE_DROP,
                };
                body.set_linear_velocity(v + kick);
            }
        }
        // Any other death buckles: the legs give and the body folds over as
        // it falls, toward the side the shot decides.
        let mut muscles = Vec::new();
        let buckle = (!crumple).then(|| {
            let hips = built
                .first()
                .map_or(Vector3::ZERO, |(_, b, ..)| b.get_global_position());
            let plan = push.map_or(Buckle::LIMP, |p| p.buckle(front, hips));
            muscles = buckle_muscles(&plan, &built, front);
            let right = Vector3::new(front.x, 0.0, front.z)
                .try_normalized()
                .unwrap_or(Vector3::BACK)
                .cross(Vector3::UP);
            for (part, body, ..) in &mut built {
                let v = body.get_linear_velocity();
                let kick = match part {
                    // The legs fold under the muscles alone: a shin thrown
                    // forward slides the foot out and sits the body down.
                    BodyPart::LeftLowerLeg | BodyPart::RightLowerLeg => Vector3::ZERO,
                    BodyPart::LeftUpperLeg | BodyPart::RightUpperLeg => {
                        Vector3::DOWN * plan.drop * 0.5
                    }
                    BodyPart::Hips | BodyPart::Spine => {
                        Vector3::DOWN * plan.drop + right * plan.sway * 0.5
                    }
                    _ => Vector3::DOWN * plan.drop + right * plan.sway,
                };
                body.set_linear_velocity(v + kick);
            }
            plan.hold
        });
        if let Some(hold) = buckle {
            // Pulled every physics tick, so the fold is the same at any
            // frame rate.
            let mut holder = holder.bind_mut();
            holder.muscles = muscles;
            holder.hold = hold;
        }
        if let Some(push) = push {
            let target = push.part.unwrap_or(BodyPart::Chest);
            if let Some((_, body, ..)) = built.iter_mut().find(|(p, ..)| *p == target) {
                let centre = body.get_global_position();
                // Off the capsule's axis toward where the round came from, so
                // it turns the part as well as moving it.
                let at = push
                    .point
                    .map_or(-push.direction.normalized_or_zero() * 0.05, |p| p - centre);
                body.apply_impulse_ex(push.impulse()).position(at).done();
            }
        }

        Some(Ragdoll {
            skeleton,
            holder,
            parts,
            joints,
            locals,
            parents,
            order,
            age: 0.0,
            quiet: 0.0,
            rested: false,
            body_stiff: if crumple {
                CRUMPLE_BODY
            } else {
                (STIFF_SECONDS, LOOSEN_SECONDS)
            },
            // A buckling body's legs are free from the start (the muscles
            // hold them); their cones are rebuilt about the rest once the
            // muscles have let go.
            leg_stiff: match buckle {
                _ if crumple => CRUMPLE_LEGS,
                Some(hold) => (hold, BUCKLE_FADE),
                None => (LEG_STIFF_SECONDS, LEG_LOOSEN_SECONDS),
            },
            buckle,
            id: owner,
        })
    }

    /// Writes the capsules' pose into the skeleton. Returns false once the
    /// body has come to rest or moved for [`MAX_SECONDS`]: the caller then
    /// [freezes](Self::freeze) it.
    pub fn step(&mut self, dt: f32) -> bool {
        let (body_stiff, body_loosen) = self.body_stiff;
        let (leg_stiff, leg_loosen) = self.leg_stiff;
        let loosening = self.age < (leg_stiff + leg_loosen).max(body_stiff + body_loosen);
        self.age += dt;
        if loosening {
            let share = |stiff: f32, loosen: f32| {
                let t = ((self.age - stiff) / loosen).clamp(0.0, 1.0);
                STIFF_RANGE + (1.0 - STIFF_RANGE) * t * t * (3.0 - 2.0 * t)
            };
            let body = share(body_stiff, body_loosen);
            let buckling = self.buckle.is_some();
            let legs = if buckling {
                1.0
            } else {
                share(leg_stiff, leg_loosen)
            };
            for joint in &mut self.joints {
                joint.set_range(match () {
                    _ if joint.leg => legs,
                    _ if joint.torso && buckling => {
                        // Braced while the round's push spreads through the
                        // whole body, then free to fold.
                        let t = ((self.age - BUCKLE_BRACE.0) / BUCKLE_BRACE.1).clamp(0.0, 1.0);
                        body.max(STIFF_RANGE + (BUCKLE_TORSO_RANGE - STIFF_RANGE) * t)
                    }
                    _ => body,
                });
            }
        }

        // Loose: each cone moves its centre from the death pose to the
        // limb's rest, and settles there.
        let body_loose = self.age >= body_stiff + body_loosen;
        let legs_loose = self.age >= leg_stiff + leg_loosen;
        for joint in &mut self.joints {
            if (joint.leg && legs_loose) || (!joint.leg && body_loose) {
                joint.rebuild_at_rest();
            }
            joint.settle(dt);
        }
        // Ground drag: capsules roll and skate on contact, so a body that has
        // stopped falling sheds its speed along the ground as cloth and kit
        // would drag. Only while a part still slides: setting a velocity
        // wakes the body, so a drag written every frame never lets it sleep.
        if self.age > body_stiff {
            let keep = (-GROUND_DRAG * dt).exp();
            for part in &mut self.parts {
                if part.body.is_sleeping() {
                    continue;
                }
                let v = part.body.get_linear_velocity();
                let along = Vector2::new(v.x, v.z).length();
                if v.y.abs() < 1.0 && along > DRAG_STOP {
                    part.body
                        .set_linear_velocity(Vector3::new(v.x * keep, v.y, v.z * keep));
                }
            }
        }
        self.write_pose();
        // Not before every cone has been rebuilt about its rest and settled.
        let settled = leg_stiff + leg_loosen + SLUMP_SECONDS;
        let quiet = self.parts.iter().all(|p| {
            p.body.is_sleeping()
                || (p.body.get_linear_velocity().length() < QUIET_SPEED
                    && p.body.get_angular_velocity().length() < QUIET_SPIN)
        });
        self.quiet = if quiet { self.quiet + dt } else { 0.0 };
        let resting = self.age > settled && self.quiet >= QUIET_SECONDS;
        if resting {
            self.rested = true;
        }
        !(resting || self.age > MAX_SECONDS)
    }

    /// Where the body's centre of mass is now (world). Only the push,
    /// gravity, and the ground move it, so it shows which way the body fell
    /// however its muscles folded it.
    pub fn centre(&self) -> Vector3 {
        let (sum, mass) = self
            .parts
            .iter()
            .fold((Vector3::ZERO, 0.0), |(sum, mass), p| {
                let m = p.body.get_mass();
                (sum + p.body.get_global_position() * m, mass + m)
            });
        if mass > 0.0 {
            sum / mass
        } else {
            Vector3::ZERO
        }
    }

    /// Stops the physics and keeps the pose it ended in.
    pub fn freeze(mut self) {
        self.write_pose();
        self.discard();
    }

    /// Frees the capsules without touching the pose (the joints and the
    /// place in the cap go as it drops).
    pub fn discard(mut self) {
        if self.holder.is_instance_valid() {
            self.holder.queue_free();
        }
    }

    fn write_pose(&mut self) {
        if !self.skeleton.is_instance_valid() {
            return;
        }
        let inverse = self.skeleton.get_global_transform().affine_inverse();
        let mut wanted: Vec<Option<Transform3D>> = vec![None; self.locals.len()];
        for part in &self.parts {
            let t = inverse * part.body.get_global_transform() * part.offset;
            wanted[part.bone as usize] = Some(t);
        }
        let mut globals = vec![Transform3D::IDENTITY; self.locals.len()];
        for &b in &self.order {
            let parent = self.parents[b];
            let parent_global = (parent >= 0).then(|| globals[parent as usize]);
            let (p, r, s) = self.locals[b];
            globals[b] = match wanted[b] {
                Some(t) => {
                    // The driven bone's local pose under its (new) parent.
                    let local = parent_global.map_or(t, |g| g.affine_inverse() * t);
                    let rotation = local.basis.orthonormalized().get_quaternion();
                    self.locals[b] = (local.origin, rotation, s);
                    self.skeleton.set_bone_pose_position(b as i32, local.origin);
                    self.skeleton.set_bone_pose_rotation(b as i32, rotation);
                    t
                }
                None => {
                    let local = Transform3D::new(Basis::from_quaternion(r).scaled(s), p);
                    parent_global.map_or(local, |g| g * local)
                }
            };
        }
    }
}

/// The muscles that bend a body into `plan`, from the parts as built
/// (part, body, joint point, axis) for a body facing `front`.
fn buckle_muscles(
    plan: &Buckle,
    built: &[(BodyPart, Gd<RigidBody3D>, Vector3, Vector3)],
    front: Vector3,
) -> Vec<Muscle> {
    let find = |part: BodyPart| built.iter().find(|(p, ..)| *p == part);
    // Turning about `pitch` by a positive angle tips what points up toward
    // the front (and what hangs down toward the back); about `front`, it
    // leans what points up toward the body's right.
    let front = Vector3::new(front.x, 0.0, front.z)
        .try_normalized()
        .unwrap_or(Vector3::BACK);
    let pitch = Vector3::UP.cross(front);
    let turn = |axis: Vector3, degrees: f32| Basis::from_axis_angle(axis, degrees.to_radians());
    let mut muscles = Vec::new();
    let mut add = |parent: BodyPart, child: BodyPart, by: Basis| {
        if let (Some((_, p, ..)), Some((_, c, ..))) = (find(parent), find(child)) {
            muscles.push(Muscle::new(p, c, by));
        }
    };
    // Turning about up by a positive angle turns the front toward the
    // body's left, so a twist to the right is a negative turn.
    let torso = turn(Vector3::UP, -plan.twist * 0.5)
        * turn(front, plan.lean * 0.5)
        * turn(pitch, plan.bend * 0.5);
    add(BodyPart::Hips, BodyPart::Spine, torso);
    add(BodyPart::Spine, BodyPart::Chest, torso);
    add(BodyPart::Chest, BodyPart::Head, turn(pitch, plan.nod));
    let legs = [
        (BodyPart::LeftUpperLeg, BodyPart::LeftLowerLeg),
        (BodyPart::RightUpperLeg, BodyPart::RightLowerLeg),
    ];
    for (i, (thigh, shin)) in legs.into_iter().enumerate() {
        add(BodyPart::Hips, thigh, turn(pitch, -plan.hips[i]));
        if let (Some((.., thigh_axis)), Some((.., shin_axis))) = (find(thigh), find(shin)) {
            // About the knee's own hinge, as far again as it still needs to
            // reach the planned bend: a crouched knee stays bent.
            let (hinge, bent) = hinge_axis(*thigh_axis, *shin_axis, -front);
            let more = (plan.knees[i] - bent.to_degrees()).max(0.0);
            add(
                thigh,
                shin,
                Basis::from_axis_angle(hinge, more.to_radians()),
            );
        }
    }
    muscles
}

/// The axis of an already built part's capsule.
fn built_axis(built: &[(BodyPart, Gd<RigidBody3D>, Vector3, Vector3)], part: BodyPart) -> Vector3 {
    built
        .iter()
        .find(|(p, ..)| *p == part)
        .map_or(Vector3::UP, |(.., axis)| *axis)
}

/// The joint between a part and its parent, at `at` (world), for a child
/// capsule along `axis` hanging from a parent along `parent_axis`.
fn make_joint(
    kind: JointKind,
    at: Vector3,
    axis: Vector3,
    parent_axis: Vector3,
    front: Vector3,
) -> (Gd<Joint3D>, Transform3D, (f32, f32)) {
    match kind {
        JointKind::Cone { swing, twist } => {
            let mut joint = ConeTwistJoint3D::new_alloc();
            joint.set_param(ConeParam::SWING_SPAN, swing.to_radians());
            joint.set_param(ConeParam::TWIST_SPAN, twist.to_radians());
            joint.set_exclude_nodes_from_collision(true);
            // The twist axis is the joint's X: along the child.
            (
                joint.upcast(),
                Transform3D::new(basis_with_x(axis), at),
                (0.0, 0.0),
            )
        }
        JointKind::Hinge { max, toward_front } => {
            let mut joint = HingeJoint3D::new_alloc();
            let toward = if toward_front { front } else { -front };
            let (hinge, bend) = hinge_axis(parent_axis, axis, toward);
            // The hinge axis is the joint's Z. Turning the child about it by
            // a positive angle bends the joint further.
            let x = hinge
                .cross(Vector3::UP)
                .try_normalized()
                .unwrap_or(Vector3::RIGHT);
            let y = hinge.cross(x);
            let (lower, upper) = hinge_limits(bend, max.to_radians());
            joint.set_flag(HingeFlag::USE_LIMIT, true);
            joint.set_param(HingeParam::LIMIT_LOWER, lower);
            joint.set_param(HingeParam::LIMIT_UPPER, upper);
            joint.set_exclude_nodes_from_collision(true);
            (
                joint.upcast(),
                Transform3D::new(Basis::from_cols(x, y, hinge), at),
                (lower, upper),
            )
        }
    }
}

/// Where a limb rests relative to its parent, in skeleton rest space, from
/// the rest directions of the part (`child`, along its capsule) and its
/// parent: the rest pose's own for the spine, neck, and legs; for the
/// arms, hanging down the chest and a little out (the rest pose holds them
/// out to the sides).
fn rest_neutral(part: BodyPart, child: Vector3, parent: Vector3) -> Option<Vector3> {
    let child = child.try_normalized()?;
    match part {
        BodyPart::LeftUpperArm | BodyPart::RightUpperArm => {
            (-parent.try_normalized()? + child * ARM_HANG_OUT).try_normalized()
        }
        _ => Some(child),
    }
}

/// A hinge's axis for a joint between a parent along `parent` and a child
/// along `child`, which bends toward `toward`, and how far it is bent now
/// (radians, 0 straight). Turning `child` about the axis by a positive angle
/// bends it further.
pub fn hinge_axis(parent: Vector3, child: Vector3, toward: Vector3) -> (Vector3, f32) {
    let bend = parent.angle_to(child);
    let axis = if bend > 0.2 {
        parent.cross(child)
    } else {
        parent.cross(toward)
    };
    (axis.try_normalized().unwrap_or(Vector3::RIGHT), bend)
}

/// The hinge's limits, relative to the pose it starts in (bent by `bend`),
/// for a joint that bends from straight up to `max`.
pub fn hinge_limits(bend: f32, max: f32) -> (f32, f32) {
    // Godot measures a hinge's angle as the first body's turn relative to
    // the second, the opposite sign to the child's turn about the axis.
    let bend = bend.clamp(0.0, max);
    (-(max - bend), bend)
}

/// A basis with `y` as its Y axis.
fn basis_with_y(y: Vector3) -> Basis {
    let y = y.normalized();
    let helper = if y.dot(Vector3::FORWARD).abs() < 0.9 {
        Vector3::FORWARD
    } else {
        Vector3::RIGHT
    };
    let x = y.cross(helper).normalized();
    let z = x.cross(y);
    Basis::from_cols(x, y, z)
}

/// A basis with `x` as its X axis.
fn basis_with_x(x: Vector3) -> Basis {
    let x = x.normalized();
    let helper = if x.dot(Vector3::UP).abs() < 0.9 {
        Vector3::UP
    } else {
        Vector3::FORWARD
    };
    let z = x.cross(helper).normalized();
    let y = z.cross(x);
    Basis::from_cols(x, y, z)
}

/// Bone indices with every parent before its children.
pub(crate) fn parents_first(parents: &[i32]) -> Vec<usize> {
    let depth = |mut b: i32| {
        let mut d = 0;
        while b >= 0 && d < parents.len() {
            b = parents[b as usize];
            d += 1;
        }
        d
    };
    let mut order: Vec<usize> = (0..parents.len()).collect();
    order.sort_by_key(|&b| depth(b as i32));
    order
}

/// Every bone's skeleton-space transform from local poses.
pub(crate) fn globals_of(
    locals: &[BonePose],
    parents: &[i32],
    order: &[usize],
) -> Vec<Transform3D> {
    let mut globals = vec![Transform3D::IDENTITY; locals.len()];
    for &b in order {
        let (p, r, s) = locals[b];
        let local = Transform3D::new(Basis::from_quaternion(r).scaled(s), p);
        let parent = parents[b];
        globals[b] = if parent >= 0 {
            globals[parent as usize] * local
        } else {
            local
        };
    }
    globals
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_sum_to_the_whole_body() {
        let total: f32 = PARTS.iter().map(|p| p.mass).sum();
        assert!((total - 1.0).abs() < 1e-4, "{total}");
        // Every parent is built before its child.
        for (i, spec) in PARTS.iter().enumerate() {
            if let Some(parent) = spec.parent {
                assert!(
                    PARTS[..i].iter().any(|p| p.part == parent),
                    "{:?}",
                    spec.part
                );
            }
        }
    }

    #[test]
    fn parents_come_first() {
        let parents = [-1, 0, 1, 0, 3, 2];
        let order = parents_first(&parents);
        for (i, &b) in order.iter().enumerate() {
            let p = parents[b];
            if p >= 0 {
                assert!(order[..i].contains(&(p as usize)));
            }
        }
    }

    #[test]
    fn a_straight_knee_bends_backward_only() {
        // Thigh and shin straight down, the body facing -Z: a knee bends the
        // shin toward +Z. A positive turn about the axis must bend it back.
        let down = Vector3::DOWN;
        let (axis, bend) = hinge_axis(down, down, Vector3::BACK);
        assert!(bend.abs() < 1e-5);
        let bent = Basis::from_axis_angle(axis, 0.5) * down;
        assert!(bent.z > 0.4, "{bent:?}");
        let (lower, upper) = hinge_limits(bend, 140f32.to_radians());
        assert!(upper.abs() < 1e-5 && lower < -2.0);
    }

    #[test]
    fn a_bent_elbow_keeps_its_bend_axis() {
        let upper = Vector3::DOWN;
        let fore = Vector3::new(0.0, -0.3, -1.0).normalized();
        let (axis, bend) = hinge_axis(upper, fore, Vector3::FORWARD);
        let more = Basis::from_axis_angle(axis, 0.2) * fore;
        assert!(
            upper.angle_to(more) > bend,
            "a positive turn bends it further"
        );
        let (lower, upper_limit) = hinge_limits(bend, 140f32.to_radians());
        assert!((upper_limit - bend).abs() < 1e-5);
        assert!((lower + (140f32.to_radians() - bend)).abs() < 1e-5);
    }

    fn shot(direction: Vector3, part: BodyPart, point: Option<Vector3>, force: f32) -> Push {
        Push {
            direction,
            part: Some(part),
            point,
            force,
            knife: false,
        }
    }

    #[test]
    fn a_body_goes_down_toward_the_push_from_the_side() {
        // Facing +Z, its right is -X. A round travelling -X pushes it right.
        let front = Vector3::BACK;
        let hips = Vector3::new(0.0, 1.0, 0.0);
        let plan = shot(Vector3::LEFT, BodyPart::Chest, None, 3.0).buckle(front, hips);
        assert!(plan.lean > 10.0, "{plan:?}");
        assert!(plan.knees[1] > plan.knees[0], "the right knee goes first");
        let plan = shot(Vector3::RIGHT, BodyPart::Chest, None, 3.0).buckle(front, hips);
        assert!(
            plan.lean < -10.0 && plan.knees[0] > plan.knees[1],
            "{plan:?}"
        );
    }

    #[test]
    fn a_shot_from_the_front_hunches_over_it_and_drops_on_the_struck_side() {
        let front = Vector3::BACK;
        let hips = Vector3::new(0.0, 1.0, 0.0);
        let right = Vector3::new(-0.12, 1.3, 0.1);
        let plan = shot(Vector3::FORWARD, BodyPart::Chest, Some(right), 3.0).buckle(front, hips);
        assert!(plan.bend > 20.0, "folds forward over the wound: {plan:?}");
        assert!(plan.lean > 0.0 && plan.knees[1] > plan.knees[0], "{plan:?}");
        let behind = shot(Vector3::BACK, BodyPart::Chest, Some(right), 3.0).buckle(front, hips);
        assert!(behind.bend < plan.bend && behind.bend > 0.0, "{behind:?}");
        // Square on, the same shot always falls the same way.
        let square = shot(Vector3::FORWARD, BodyPart::Chest, None, 3.0);
        assert_eq!(square.buckle(front, hips), square.buckle(front, hips));
    }

    #[test]
    fn a_leg_hit_gives_that_leg_and_a_headshot_drops_fastest() {
        let front = Vector3::BACK;
        let hips = Vector3::new(0.0, 1.0, 0.0);
        let leg = shot(Vector3::FORWARD, BodyPart::LeftLowerLeg, None, 3.0).buckle(front, hips);
        assert!(leg.knees[0] > 100.0 && leg.knees[1] < 60.0, "{leg:?}");
        assert!(leg.lean < 0.0, "down over the left leg: {leg:?}");
        let head = shot(Vector3::FORWARD, BodyPart::Head, None, 3.0).buckle(front, hips);
        let chest = shot(Vector3::FORWARD, BodyPart::Chest, None, 3.0).buckle(front, hips);
        assert!(head.drop > chest.drop && head.hold < chest.hold, "{head:?}");
        let blast = shot(Vector3::FORWARD, BodyPart::Chest, None, 15.0).buckle(front, hips);
        assert!(blast.bend < chest.bend, "a blast throws more than it folds");
    }

    #[test]
    fn pushes_scale_with_force_and_stay_low() {
        let push = |force, part| Push {
            direction: Vector3::new(0.0, 0.3, -1.0),
            part,
            point: None,
            force,
            knife: false,
        };
        let rifle = push(3.0, None).impulse();
        let shotgun = push(15.0, None).impulse();
        assert!(shotgun.length() > rifle.length() * 2.0);
        assert!(rifle.y < 0.0, "never lifts the body");
        assert!(rifle.z < -10.0);
        let head = push(3.0, Some(BodyPart::Head)).impulse();
        assert!(head.length() < rifle.length());
    }
}
