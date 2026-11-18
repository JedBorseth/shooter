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
//! - Each frame the capsules' transforms are written into the skeleton's own
//!   bone poses. Once the body comes to rest (every capsule asleep), after
//!   [`MAX_SECONDS`], or when more than [`MAX_SIMULATING`] ragdolls are
//!   moving, it freezes: the capsules are freed and the skeleton keeps the
//!   last pose, so a corpse costs nothing but its drawing.

use std::cell::RefCell;
use std::collections::VecDeque;

use godot::classes::cone_twist_joint_3d::Param as ConeParam;
use godot::classes::hinge_joint_3d::{Flag as HingeFlag, Param as HingeParam};
use godot::classes::{
    CapsuleShape3D, CollisionShape3D, ConeTwistJoint3D, HingeJoint3D, Joint3D, Node3D,
    PhysicsMaterial, RigidBody3D, Skeleton3D,
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
/// Whole-body mass (kg).
const MASS: f32 = 70.0;
/// A body hit hard does not fold at once: for the first moments its joints
/// stay near the pose it died in, so it topples with the push, then they
/// loosen to their full range and it goes limp. Seconds held stiff, seconds
/// to loosen, and the share of each joint's range allowed while stiff.
const STIFF_SECONDS: f32 = 0.18;
const LOOSEN_SECONDS: f32 = 0.45;
const STIFF_RANGE: f32 = 0.12;
/// The legs stay stiff longer, so a body topples over instead of folding
/// onto its knees where it stood.
const LEG_STIFF_SECONDS: f32 = 0.4;
const LEG_LOOSEN_SECONDS: f32 = 0.6;

/// Push (N·s) at the part the killing shot hit: a base plus a share per
/// force level (`protocol::HitPart::force`, about a tenth of the damage), so
/// a shotgun at close range or a sniper rifle pushes harder than a rifle.
const PUSH_BASE: f32 = 24.0;
const PUSH_PER_LEVEL: f32 = 4.0;
/// A headshot pushes the head this much harder.
const HEAD_PUSH_SCALE: f32 = 0.85;

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
            swing: 80.0,
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
            swing: 80.0,
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
            swing: 60.0,
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
            swing: 60.0,
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
    /// Force level 0 to 15 (`protocol::HitPart::force`).
    pub force: f32,
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

/// A joint and its full limits.
struct Joint {
    node: Gd<Joint3D>,
    kind: JointKind,
    /// Hinges: their limits relative to the starting pose.
    limits: (f32, f32),
    /// A hip or knee: loosens later.
    leg: bool,
}

impl Joint {
    /// Sets the joint's range to `share` of its full range (0..1).
    fn set_range(&mut self, share: f32) {
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
    holder: Gd<Node3D>,
    parts: Vec<Part>,
    joints: Vec<Joint>,
    /// Every bone's local pose; the driven bones' are rewritten each frame.
    locals: Vec<BonePose>,
    parents: Vec<i32>,
    /// Bone indices, parents before children.
    order: Vec<usize>,
    age: f32,
    id: InstanceId,
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

        let mut holder = Node3D::new_alloc();
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
            body.set_linear_damp(0.1);
            body.set_angular_damp(1.5);
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
                let mut joint = Joint {
                    node: joint,
                    kind: spec.joint,
                    limits,
                    leg,
                };
                joint.set_range(STIFF_RANGE);
                joints.push(joint);
            }
            built.push((spec.part, body, a, axis));
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
            id: owner,
        })
    }

    /// Writes the capsules' pose into the skeleton. Returns false once the
    /// body has come to rest or moved for [`MAX_SECONDS`]: the caller then
    /// [freezes](Self::freeze) it.
    pub fn step(&mut self, dt: f32) -> bool {
        let loosening = self.age < LEG_STIFF_SECONDS + LEG_LOOSEN_SECONDS;
        self.age += dt;
        if loosening {
            let share = |stiff: f32, loosen: f32| {
                let t = ((self.age - stiff) / loosen).clamp(0.0, 1.0);
                STIFF_RANGE + (1.0 - STIFF_RANGE) * t * t * (3.0 - 2.0 * t)
            };
            let body = share(STIFF_SECONDS, LOOSEN_SECONDS);
            let legs = share(LEG_STIFF_SECONDS, LEG_LOOSEN_SECONDS);
            for joint in &mut self.joints {
                joint.set_range(if joint.leg { legs } else { body });
            }
        }
        // Ground drag: capsules roll and skate on contact, so a body that has
        // stopped falling sheds its speed along the ground as cloth and kit
        // would drag.
        if self.age > STIFF_SECONDS {
            let keep = (-GROUND_DRAG * dt).exp();
            for part in &mut self.parts {
                let v = part.body.get_linear_velocity();
                if v.y.abs() < 1.0 {
                    part.body
                        .set_linear_velocity(Vector3::new(v.x * keep, v.y, v.z * keep));
                }
            }
        }
        self.write_pose();
        let resting = self.age > 0.5 && self.parts.iter().all(|p| p.body.is_sleeping());
        !(resting || self.age > MAX_SECONDS)
    }

    /// Where the hips are now (world), for tests and the death camera.
    pub fn hips(&self) -> Vector3 {
        self.parts
            .first()
            .map_or(Vector3::ZERO, |p| p.body.get_global_position())
    }

    /// Stops the physics and keeps the pose it ended in.
    pub fn freeze(mut self) {
        self.write_pose();
        self.discard();
    }

    /// Frees the capsules without touching the pose.
    pub fn discard(mut self) {
        release(self.id);
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

    #[test]
    fn pushes_scale_with_force_and_stay_low() {
        let push = |force, part| Push {
            direction: Vector3::new(0.0, 0.3, -1.0),
            part,
            point: None,
            force,
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
