//! `SoldierRig`: the third-person character. It turns a frame of player state
//! (velocity, stance, view angles, aim, weapon, reload, alive) into an animated,
//! armed soldier: an animation tree for locomotion, a [`SoldierPoser`] for aim
//! and hands, a world copy of the weapon, team colours, and hit reactions.
//!
//! Remote players and target dummies drive it; nothing here affects gameplay.
//! Hit detection uses the shapes in `shooter_sim::hitbox`, which follow this
//! pose: they share its weapon carry (`shooter_sim::hitbox::carry`) and start
//! from base poses measured from its animations. `godot/tests/hitbox_fit_test.gd`
//! fails if the two drift apart.

use std::f32::consts::PI;

use godot::classes::animation_mixer::AnimationCallbackModeProcess;
use godot::classes::{
    AnimationLibrary, AnimationNode, AnimationNodeAnimation, AnimationNodeBlend2,
    AnimationNodeBlendSpace1D, AnimationNodeBlendTree, AnimationNodeOneShot,
    AnimationNodeTimeScale, AnimationNodeTimeSeek, AnimationRootNode, AnimationTree,
    BoneAttachment3D, INode3D, MeshInstance3D, Node3D, PhysicsRayQueryParameters3D, Skeleton3D,
    SphereMesh,
};
use godot::prelude::*;
use shooter_sim::hitbox::Activity;
use shooter_sim::hitbox::carry::{self, Carry};
use shooter_sim::hitbox::clock::Progress;
use shooter_sim::hitbox::gait;
use shooter_sim::movement::Stance;
use shooter_sim::weapon::WeaponId;

use super::palette::{self, Team};
use super::poser::{Grips, Hold, LadderLimbs, PoseInput, Section, SoldierPoser, WeaponPlacement};
use super::ragdoll::{self, Push, Ragdoll};
use super::wounds::{self, Wounds};
use crate::convert::{to_godot, to_godot_basis, to_sim};
use crate::spring::{Spring, impulse_for_peak, smoothing};
use crate::viewmodel::{self, WeaponView};

// The loops' natural ground speeds and the strafe's leg turn are in
// `shooter_sim::hitbox::gait`, shared with the hit zones. Playback scales to
// the real speed so feet do not slide.
/// Without a server's mantle progress (offline), the climb takes this long.
const OFFLINE_CLIMB_SECONDS: f32 = 0.46;
/// The climb blends out over this long once a mantle ends (seconds).
const CLIMB_FADE_OUT: f32 = 0.15;
/// `Slide_Start` plays from where the hips start dropping to where they are
/// down, sped up so the body is down as soon as the hit zones (seconds of
/// the clip, and its speed).
const SLIDE_START_FROM: f32 = 0.08;
const SLIDE_START_END: f32 = 0.33;
const SLIDE_START_SPEED: f32 = 2.0;
/// `Slide_Exit` plays its rise (from this second of the clip to its end)
/// over this long once a slide ends.
const SLIDE_EXIT_FROM: f32 = 0.15;
const SLIDE_EXIT_SECONDS: f32 = 0.25;
/// `Jump_Start`'s push-off plays from this second of the clip for this long.
const TAKEOFF_FROM: f32 = 0.06;
const TAKEOFF_SECONDS: f32 = 0.25;
/// A landing after this long in the air bends the knees, fully after this
/// much longer (as the simulation's landing counts it), for this long. The
/// hit zones do not crouch on landing, so the bend is held to a share of
/// `Jump_Land`'s deep squat: a knee flex, not a crouch.
const LANDING_MIN_AIR: f32 = 0.2;
const LANDING_FULL_AIR: f32 = 0.4;
const LANDING_SECONDS: f32 = 0.55;
const LANDING_FROM: f32 = 0.04;
const LANDING_WEIGHT: f32 = 0.2;
/// The torso leans into a start and back out of a stop: this many degrees
/// at this acceleration (m/s², a walk's start), on a soft spring.
const MOTION_LEAN_DEGREES: f32 = 6.0;
const MOTION_LEAN_ACCEL: f32 = 8.0;
const MOTION_LEAN_STIFFNESS: f32 = 60.0;
const MOTION_LEAN_DAMPING: f32 = 11.0;
/// Foot IK runs on bodies standing within this distance of the camera (m),
/// looks for ground this far above and below the feet, and eases at this
/// rate. Moving bodies' feet follow their stride.
const FOOT_IK_DISTANCE: f32 = 15.0;
const FOOT_RAY_UP: f32 = 0.45;
const FOOT_RAY_DOWN: f32 = 0.3;
const FOOT_IK_RATE: f32 = 14.0;
/// Where the support hand goes for a plant or defuse, in weapon space: low
/// in front. A throw follows the grenade's path (`grenade_hold`).
const PLANT_REACH: shooter_sim::Vec3 = shooter_sim::Vec3::new(-0.1, -0.55, -0.15);
/// A weapon switch lowers the old weapon over this long and raises the new
/// one over this long (seconds).
const SWITCH_LOWER_SECONDS: f32 = 0.22;
const SWITCH_RAISE_SECONDS: f32 = 0.3;
/// Each shot pushes the shoulder and spine back this far (degrees, at a
/// rifle's kick).
const SHOULDER_PUSH_DEGREES: f32 = 1.2;
/// The weapon file's first kick, in degrees, that the third-person kick is
/// scaled to: the M4's.
const REFERENCE_KICK: f32 = 0.3;

/// How hard a weapon kicks in third person, relative to a rifle: its first
/// kick from the weapon file, within a sensible range.
fn recoil_scale(def: &shooter_sim::weapon::WeaponDef) -> f32 {
    let first = def
        .recoil
        .pattern
        .first()
        .map_or(REFERENCE_KICK, |k| k.length());
    (first / REFERENCE_KICK).clamp(0.6, 2.2)
}

/// Natural ground speed (m/s) of the zombie walk loop, measured from the planted foot.
const ZOMBIE_WALK_SPEED: f32 = 0.95;
/// Zombies slower than this play the shamble alone, sped up to their speed.
const ZOMBIE_SHAMBLE_LIMIT: f32 = 2.1;

// Weapon offsets from the eyes, the reload reach, and the hand placement are
// in `shooter_sim::hitbox::carry`, shared with the arm hit zones.

/// Hit flinch spring (radians of torso lean), as the old target dummy.
const LEAN_STIFFNESS: f32 = 140.0;
const LEAN_DAMPING: f32 = 9.0;
/// Hit reactions, by where the round landed. Peaks are for a rifle round
/// (force level 3); harder hits react more, up to `MAX_REACTION` times.
/// A body hit tips the torso along the shot and twists it toward the side hit.
const TORSO_PEAK_DEGREES: f32 = 9.0;
const TWIST_PEAK_DEGREES: f32 = 8.0;
/// A head hit snaps the neck and head along the shot.
const HEAD_PEAK_DEGREES: f32 = 18.0;
const HEAD_STIFFNESS: f32 = 220.0;
const HEAD_DAMPING: f32 = 14.0;
/// An arm hit loosens that hand on the weapon: its IK drops to this weight
/// and recovers over `ARM_DROP_SECONDS`.
const ARM_DROP_WEIGHT: f32 = 0.75;
const ARM_DROP_SECONDS: f32 = 0.18;
/// A leg hit buckles the knees: the stance blend dips this far toward a
/// crouch and springs back.
const BUCKLE_PEAK: f32 = 0.3;
const BUCKLE_STIFFNESS: f32 = 90.0;
const BUCKLE_DAMPING: f32 = 11.0;
const MAX_REACTION: f32 = 2.0;

/// Arm, hand, and finger bones: the upper-body layer holds them in a gripping
/// pose under the IK, so the fingers curl around the weapon.
fn arm_bones() -> Vec<String> {
    let mut bones = Vec::new();
    for side in ["Left", "Right"] {
        for part in ["Shoulder", "UpperArm", "LowerArm", "Hand"] {
            bones.push(format!("{side}{part}"));
        }
        for part in ["ThumbMetacarpal", "ThumbProximal", "ThumbDistal"] {
            bones.push(format!("{side}{part}"));
        }
        for finger in ["Index", "Middle", "Ring", "Little"] {
            for joint in ["Proximal", "Intermediate", "Distal"] {
                bones.push(format!("{side}{finger}{joint}"));
            }
        }
    }
    bones
}

/// One rendered frame of state the rig animates from.
#[derive(Clone, Copy, Debug)]
pub struct SoldierFrame {
    /// World-space velocity (m/s).
    pub velocity: Vector3,
    pub on_floor: bool,
    pub stance: Stance,
    pub sprinting: bool,
    /// View yaw and pitch, as the simulation's (`yaw` 0 faces -Z, pitch up positive).
    pub yaw: f32,
    pub pitch: f32,
    pub ads: f32,
    pub weapon: Option<WeaponId>,
    /// Reload progress 0..1 while reloading.
    pub reload: Option<f32>,
    pub alive: bool,
    /// Climbing over a ledge: plays the climb once as it starts.
    pub mantling: bool,
    /// Swinging the knife: each new swing plays the slash once.
    pub meleeing: bool,
    /// The stride phase and mantle progress from the server
    /// (`hitbox::clock`). Without them (offline) the rig keeps its own.
    pub stride: Option<f32>,
    pub mantle_progress: Option<f32>,
    /// The crouch blend from the simulation (`MoveState::crouch`, 0 standing
    /// to 1 crouched), which the hit zones follow too. Without it (offline
    /// dummies) the rig eases between stances on its own.
    pub crouch: Option<f32>,
    /// How far the aim has turned from the planted feet and how far the legs
    /// step in place (`Progress::turn` and `step`, the server's). Without it
    /// (offline) the rig turns its own feet.
    pub turn: Option<(f32, f32)>,
    /// Something done with the hands instead of the weapon, and how far
    /// along it is (0..1). Nothing sets it yet: it is the hook for planting,
    /// defusing, and throwing once snapshots carry them.
    pub action: Option<(ActionPose, f32)>,
    /// What a throw throws.
    pub throw_kind: shooter_sim::equipment::Kind,
    /// On a ladder: the way it is climbed (`x`, `z`). The body faces the
    /// rungs with its hands and feet on them, the weapon put away.
    pub ladder: Option<shooter_sim::Vec2>,
}

/// Things a soldier does with their hands instead of the weapon. The hit
/// zones do not follow these poses (snapshots do not carry them yet).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionPose {
    /// Kneeling over a bomb site, the weapon lowered, both hands at the
    /// ground in front (Search and Destroy's plant and defuse).
    Plant,
    Defuse,
    /// The weapon lowered and the support arm swinging over to throw.
    Throw,
}

impl SoldierFrame {
    /// What the hit zones need of this frame beyond stance and view angles,
    /// with the animation `progress` the rig drew it at
    /// ([`SoldierRig::progress`]).
    pub fn activity(&self, progress: Progress) -> Activity {
        Activity {
            sprinting: self.sprinting,
            // On a ladder the zones hang standing, facing the rungs.
            airborne: !self.on_floor && self.ladder.is_none(),
            mantling: self.mantling,
            reloading: self.reload.is_some(),
            ads: self.ads,
            velocity: to_sim(self.velocity),
            weapon: self.weapon,
            progress,
            crouch: self.crouch,
            hunch: 0.0,
        }
    }
}

impl Default for SoldierFrame {
    fn default() -> Self {
        Self {
            velocity: Vector3::ZERO,
            on_floor: true,
            stance: Stance::Standing,
            sprinting: false,
            yaw: 0.0,
            pitch: 0.0,
            ads: 0.0,
            weapon: None,
            reload: None,
            alive: true,
            mantling: false,
            meleeing: false,
            stride: None,
            mantle_progress: None,
            crouch: None,
            turn: None,
            action: None,
            throw_kind: Default::default(),
            ladder: None,
        }
    }
}

/// Where a death has got to.
#[derive(Default)]
enum Death {
    #[default]
    Alive,
    /// Waiting for the poser to capture the pose last drawn (a frame or
    /// two); `frames` counts the wait.
    Capturing {
        push: Option<Push>,
        velocity: Vector3,
        frames: u32,
    },
    Ragdoll(Ragdoll),
    /// At rest: the skeleton keeps the last pose.
    Still,
}

/// Share of the ground speed a body keeps as it falls.
const CARRIED_RUN: f32 = 0.6;
/// A death whose hit is older than this (seconds) falls without its push.
const HIT_MEMORY: f32 = 1.0;
/// Frames to wait for the poser's capture before taking the pose without it.
const CAPTURE_FRAMES: u32 = 3;

#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct SoldierRig {
    base: Base<Node3D>,
    /// Rounds fired, for varied cases.
    cases: u32,

    /// 0: the player's side (tan, blue accents). 1: enemies (charcoal, red-orange).
    #[export]
    #[init(val = 1)]
    team: i32,
    /// Animate as a zombie: shambling walk, arms reaching, a scratch attack,
    /// and no weapon. Set before the rig enters the tree.
    #[export]
    zombie: bool,
    /// Other bodies this character may wear instead of `Model` (zombies): one
    /// is picked per character, `Model` included.
    #[export]
    variants: Array<Gd<PackedScene>>,

    #[init(node = "Model")]
    model: OnReady<Gd<Node3D>>,
    tree: Option<Gd<AnimationTree>>,
    poser: Option<Gd<SoldierPoser>>,
    weapon: Option<(WeaponId, Gd<WeaponView>)>,
    applied_team: Option<Team>,

    frame: SoldierFrame,
    lower_yaw: f32,
    /// The travel's direction the loops blend (radians from the aim,
    /// positive right; `gait::direction`), eased round.
    direction: f32,
    crouch: f32,
    slide: f32,
    air: f32,
    air_time: f32,
    /// Transitions: seconds since a slide began and since it ended, since a
    /// jump left the ground, and since a landing (with its strength).
    slide_for: Option<f32>,
    slide_exit: Option<f32>,
    takeoff: Option<f32>,
    landing: Option<(f32, f32)>,
    was_sliding: bool,
    /// Leaning into starts and back from stops: the last velocity in the
    /// body's frame and the lean it pulls toward.
    last_local: Option<Vector3>,
    motion_lean: Spring,
    /// Foot IK: the ground's height under each foot (left, right) from the
    /// ground the body stands on, smoothed, and whether it is on.
    feet_lift: [f32; 2],
    feet_on: bool,
    sprint: f32,
    reload_blend: f32,
    reload_progress: f32,
    /// Seconds into the knife swing playing, if one is.
    melee: Option<f32>,
    ik: f32,
    switch_drop: f32,
    /// Weapon kick: x pitch (radians), z push back (metres).
    kick: Spring,
    lean: Spring,
    /// The old weapon is lowering before a switch.
    lowering: bool,
    /// Hit reactions: the head's tip along a shot (as `lean`), the torso's
    /// twist (radians, positive turns left), the knees' buckle (stance blend),
    /// and seconds left of each hand's loosened grip (right, left).
    head: Spring,
    twist: Spring<f32>,
    buckle: Spring<f32>,
    arm_drop: [f32; 2],
    /// Hit reactions played (tests).
    flinches: u32,
    /// The latest hit and how long ago (seconds): a death pushes the body
    /// with it.
    last_hit: Option<(Push, f32)>,
    /// Where it was hit lately, darkened on the body (`wounds.rs`).
    wounds: Wounds,
    death: Death,
    /// Where the hips were as the ragdoll started and where they are now.
    fall_from: Vector3,
    fall_to: Vector3,
    was_alive: bool,
    was_mantling: bool,
    /// The stride phase and mantle progress drawn (the server's, or the
    /// rig's own offline), and how far the climb has blended in.
    stride: f32,
    mantle_progress: f32,
    climb: f32,
    /// Turning in place: the aim's offset from the feet and the in-place
    /// step drawn, and the rig's own turning offline.
    turn: f32,
    step: f32,
    own_turning: gait::Turning,
    /// How far the climb on a ladder has blended in, and where its hands
    /// and feet go this frame.
    ladder: f32,
    ladder_limbs: Option<LadderLimbs>,
}

#[godot_api]
impl INode3D for SoldierRig {
    fn ready(&mut self) {
        self.was_alive = true;
        self.ik = 1.0;
        self.pick_body();
        let Some(skeleton) = self.skeleton() else {
            godot_error!("SoldierRig: no GeneralSkeleton under Model");
            return;
        };
        let mut poser = SoldierPoser::new_alloc();
        poser.set_name("Poser");
        skeleton.clone().upcast::<Node>().add_child(&poser);
        if !self.zombie {
            // The knife the support hand swings in a melee, and the grenade
            // it throws.
            let knife = crate::knife::build();
            self.base_mut().add_child(&knife);
            poser.bind_mut().knife = Some(knife);
            let grenade = crate::grenade_hold::build();
            self.base_mut().add_child(&grenade);
            poser.bind_mut().grenade = Some(grenade);
        }
        self.poser = Some(poser);

        // Every kit is one mesh, `Body`, and a decimated copy, `BodyFar`, drawn
        // in its place past FAR_DISTANCE (tools/build_character_assets.py).
        let far = skeleton.try_get_node_as::<MeshInstance3D>("BodyFar");
        if let Some(mut body) = skeleton.try_get_node_as::<MeshInstance3D>("Body")
            && far.is_some()
        {
            body.set_visibility_range_end(FAR_DISTANCE);
            body.set_visibility_range_end_margin(FAR_MARGIN);
        }
        if let Some(mut far) = far {
            far.set_visibility_range_begin(FAR_DISTANCE);
            far.set_visibility_range_begin_margin(FAR_MARGIN);
        }
        self.apply_team();
        // A per-character seed for the shader's wear, skin tone, and blood.
        let seed = self.base().instance_id().to_i64().unsigned_abs();
        let variant = (seed % 1009) as f32 * 0.0731;
        for child in skeleton.get_children().iter_shared() {
            if let Ok(mut mesh) = child.try_cast::<MeshInstance3D>() {
                mesh.set_instance_shader_parameter("variant", &variant.to_variant());
                if self.zombie {
                    // A horde is many bodies at mid range: drop detail sooner.
                    mesh.set_lod_bias(ZOMBIE_LOD_BIAS);
                }
            }
        }
        if self.zombie {
            add_zombie_eyes(&skeleton);
        }

        let tree = self.build_tree();
        self.tree = Some(tree);
        // Bullet holes and blood stay on the walls, not on people.
        crate::effects::keep_decals_off(&self.base().clone().upcast());
    }
}

#[godot_api]
impl SoldierRig {
    /// Swaps `Model` for one of `variants`, chosen by this character's seed.
    fn pick_body(&mut self) {
        if self.variants.is_empty() {
            return;
        }
        let seed = self.base().instance_id().to_i64().unsigned_abs();
        // Instance ids are spaced regularly, so mix the bits before picking:
        // otherwise a horde wears only some of the bodies.
        let pick = (splitmix(seed) % (self.variants.len() as u64 + 1)) as usize;
        if pick == 0 {
            return;
        }
        let Some(scene) = self.variants.get(pick - 1) else {
            return;
        };
        let Some(mut body) = scene.try_instantiate_as::<Node3D>() else {
            godot_error!("SoldierRig: a body variant is not a Node3D scene");
            return;
        };
        let mut old = self.model.clone();
        body.set_transform(old.get_transform());
        let index = old.get_index();
        self.base_mut().remove_child(&old);
        old.queue_free();
        body.set_name("Model");
        self.base_mut().add_child(&body);
        self.base_mut().move_child(&body, index);
        *self.model = body;
    }

    /// The world position of the weapon's muzzle, or the chest if unarmed.
    #[func]
    pub fn muzzle_position(&self) -> Vector3 {
        self.weapon
            .as_ref()
            .and_then(|(_, view)| view.try_get_node_as::<Node3D>("Muzzle"))
            .map(|m| m.get_global_position())
            .unwrap_or_else(|| self.base().get_global_position() + Vector3::new(0.0, 1.4, 0.0))
    }

    /// How far the body is down in a crouch (x) and in a slide (y), each 0
    /// to 1 as drawn, for the patch of contact shadow under the feet
    /// (`scripts/grounding.gd`), which lengthens as the body gets low.
    #[func]
    pub fn stance_blend(&self) -> Vector2 {
        Vector2::new(self.crouch.clamp(0.0, 1.0), self.slide.clamp(0.0, 1.0))
    }

    /// Horizontal direction (world) the body has fallen toward, or zero
    /// while alive or before it has moved.
    #[func]
    pub fn fall_direction(&self) -> Vector3 {
        if self.frame.alive {
            return Vector3::ZERO;
        }
        let moved = self.fall_to - self.fall_from;
        Vector3::new(moved.x, 0.0, moved.z)
            .try_normalized()
            .unwrap_or(Vector3::ZERO)
    }

    /// How far (m) the hips have moved along the ground since the ragdoll
    /// started (tests).
    #[func]
    pub fn fall_distance(&self) -> f32 {
        let moved = self.fall_to - self.fall_from;
        Vector3::new(moved.x, 0.0, moved.z).length()
    }

    /// Whether the body is a ragdoll still moving (tests).
    #[func]
    pub fn is_ragdoll(&self) -> bool {
        matches!(self.death, Death::Ragdoll(_))
    }

    /// Hit reactions played so far (tests).
    #[func]
    pub fn flinch_count(&self) -> i64 {
        i64::from(self.flinches)
    }

    /// Asks for the next drawn pose, aim and hit reactions included, for
    /// [`posed_bone_transform`](Self::posed_bone_transform) (tests: Godot
    /// drops the poser's changes once the frame is skinned).
    #[func]
    fn capture_pose(&mut self) {
        if let Some(poser) = &mut self.poser {
            let mut p = poser.bind_mut();
            p.captured = None;
            p.capture = true;
        }
    }

    /// The world transform of a bone in the pose last captured by
    /// [`capture_pose`](Self::capture_pose), or identity if none (tests).
    #[func]
    fn posed_bone_transform(&self, bone: GString) -> Transform3D {
        let (Some(skeleton), Some(poser)) = (self.skeleton(), &self.poser) else {
            return Transform3D::IDENTITY;
        };
        let p = poser.bind();
        let Some(locals) = &p.captured else {
            return Transform3D::IDENTITY;
        };
        let index = skeleton.find_bone(&bone);
        if index < 0 || locals.len() != skeleton.get_bone_count() as usize {
            return Transform3D::IDENTITY;
        }
        let parents: Vec<i32> = (0..skeleton.get_bone_count())
            .map(|b| skeleton.get_bone_parent(b))
            .collect();
        let order = ragdoll::parents_first(&parents);
        let globals = ragdoll::globals_of(locals, &parents, &order);
        skeleton.get_global_transform() * globals[index as usize]
    }

    /// Whether the body has come to rest after a death (tests).
    #[func]
    pub fn is_still(&self) -> bool {
        matches!(self.death, Death::Still)
    }

    /// The world transform of a bone of the animated skeleton, for tests.
    #[func]
    pub fn bone_global_transform(&self, bone: GString) -> Transform3D {
        let Some(skeleton) = self.skeleton() else {
            return Transform3D::IDENTITY;
        };
        let index = skeleton.find_bone(&bone);
        if index < 0 {
            return Transform3D::IDENTITY;
        }
        skeleton.get_global_transform() * skeleton.get_bone_global_pose(index)
    }

    /// Where the hand IK last put a wrist (world space), or zero if it is off.
    #[func]
    pub fn wrist_position(&self, right: bool) -> Vector3 {
        self.poser
            .as_ref()
            .and_then(|p| p.bind().solved_wrist(right))
            .unwrap_or(Vector3::ZERO)
    }

    /// The held weapon node, if any.
    #[func]
    pub fn weapon_node(&self) -> Option<Gd<Node3D>> {
        self.weapon.as_ref().map(|(_, v)| v.clone().upcast())
    }

    /// Poses the character from a dictionary, for tests and screenshot scripts:
    /// `velocity` (Vector3), `on_floor`, `stance` ("standing", "crouching",
    /// "sliding"), `sprinting`, `yaw`, `pitch`, `ads`, `weapon` (file key),
    /// `reload` (progress, or -1), `alive`, `mantling`, `meleeing`, `crouch`
    /// (the simulation's crouch blend, or -1), `turn` (the aim's turn from the
    /// planted feet; missing turns the rig's own feet), `action` ("plant",
    /// "defuse", "throw") with `action_progress`. Missing keys keep their
    /// defaults.
    #[func]
    fn drive(&mut self, state: VarDictionary, delta: f32) {
        let get = |key: &str| state.get(key);
        let f = |key: &str, default: f32| {
            get(key)
                .and_then(|v| v.try_to::<f32>().ok())
                .unwrap_or(default)
        };
        let b = |key: &str, default: bool| {
            get(key)
                .and_then(|v| v.try_to::<bool>().ok())
                .unwrap_or(default)
        };
        let stance = match get("stance").map(|v| v.to_string()).as_deref() {
            Some("crouching") => Stance::Crouching,
            Some("sliding") => Stance::Sliding,
            _ => Stance::Standing,
        };
        let reload = f("reload", -1.0);
        let frame = SoldierFrame {
            velocity: get("velocity")
                .and_then(|v| v.try_to::<Vector3>().ok())
                .unwrap_or(Vector3::ZERO),
            on_floor: b("on_floor", true),
            stance,
            sprinting: b("sprinting", false),
            yaw: f("yaw", 0.0),
            pitch: f("pitch", 0.0),
            ads: f("ads", 0.0),
            weapon: get("weapon").and_then(|v| WeaponId::by_key(&v.to_string())),
            reload: (reload >= 0.0).then_some(reload),
            alive: b("alive", true),
            mantling: b("mantling", false),
            meleeing: b("meleeing", false),
            stride: (f("stride", -1.0) >= 0.0).then(|| f("stride", 0.0)),
            mantle_progress: (f("mantle_progress", -1.0) >= 0.0).then(|| f("mantle_progress", 0.0)),
            crouch: (f("crouch", -1.0) >= 0.0).then(|| f("crouch", 0.0)),
            turn: get("turn")
                .and_then(|v| v.try_to::<f32>().ok())
                .map(|t| (t, f("step", 0.0))),
            action: match get("action").map(|v| v.to_string()).as_deref() {
                Some("plant") => Some((ActionPose::Plant, f("action_progress", 0.5))),
                Some("defuse") => Some((ActionPose::Defuse, f("action_progress", 0.5))),
                Some("throw") => Some((ActionPose::Throw, f("action_progress", 0.5))),
                _ => None,
            },
            throw_kind: get("throw_kind")
                .and_then(|v| shooter_sim::equipment::Kind::by_key(&v.to_string()))
                .unwrap_or_default(),
            ladder: get("ladder_yaw")
                .and_then(|v| v.try_to::<f32>().ok())
                .map(|yaw| {
                    let (forward, _) = shooter_sim::movement::basis_from_yaw(yaw);
                    shooter_sim::Vec2::new(forward.x, forward.z)
                }),
        };
        self.update(&frame, delta);
    }

    /// One shot: muzzle flash and kick.
    #[func]
    fn shoot(&mut self) {
        self.fire();
    }

    /// A hit from a shot travelling along `direction`.
    #[func]
    fn take_hit(&mut self, direction: Vector3, killed: bool) {
        self.hit(direction, killed);
    }

    /// A hit on shape `part` of `shooter_sim::hitbox::hitboxes` (-1 unknown)
    /// at `point`, with force level `force` (0 to 15), for tests.
    #[func]
    fn take_hit_at(
        &mut self,
        direction: Vector3,
        killed: bool,
        point: Vector3,
        part: i64,
        force: f32,
    ) {
        self.hit_at(push_of(direction, point, part, force), killed);
    }

    #[func]
    pub fn set_team_index(&mut self, team: i32) {
        self.team = team;
        self.apply_team();
    }
}

impl SoldierRig {
    /// Advances the character to `frame`.
    pub fn update(&mut self, frame: &SoldierFrame, dt: f32) {
        let was_alive = self.was_alive;
        let was_meleeing = self.frame.meleeing;
        if let Some((_, age)) = &mut self.last_hit {
            *age += dt;
        }
        if self.wounds.active()
            && let Some(skeleton) = self.skeleton()
        {
            // The dead keep their wounds.
            self.wounds.update(&skeleton, dt, !frame.alive);
        }
        if !frame.alive {
            // The body is drawn as it was, then falls as a ragdoll: the
            // poser keeps its last input, and the tree stops.
            self.frame.alive = false;
            self.frame.velocity = frame.velocity;
            self.was_alive = false;
            if was_alive {
                self.begin_death();
            }
            self.step_corpse(dt);
            return;
        }
        self.frame = *frame;
        self.was_alive = true;
        if !was_alive {
            self.revive();
        }
        self.switch_to(frame.weapon, dt);

        // Body yaw. The model inside faces -Z like everything else.
        self.base_mut()
            .set_global_basis(Basis::from_axis_angle(Vector3::UP, frame.yaw));

        // Locomotion, in the body's frame: +X right, -Z forward.
        let local = Basis::from_axis_angle(Vector3::UP, -frame.yaw) * frame.velocity;
        let flat = Vector3::new(local.x, 0.0, local.z);
        let speed = flat.length();
        let moving = speed > gait::MOVING_SPEED && frame.alive;
        let forward = -flat.z;
        let right = flat.x;
        // Every direction has its own loops (`gait::DIRECTIONS`); only a
        // sprint turns its hips toward a sideways drift.
        let mut leg_turn = 0.0;
        if moving && frame.stance != Stance::Sliding {
            let target = gait::direction(right, forward, frame.sprinting);
            self.direction = wrap_angle(
                self.direction + wrap_angle(target - self.direction) * smoothing(12.0, dt),
            );
            if frame.sprinting {
                leg_turn = gait::leg_turn(right, forward);
            }
        } else if self.step > 0.0 {
            // Stepping round on the spot is the forward walk.
            self.direction = 0.0;
        }
        // Positive turns the hips left; drifting right turns them right.
        self.lower_yaw += (-leg_turn - self.lower_yaw) * smoothing(10.0, dt);

        // Turning in place: the legs stay on the feet until they step round,
        // and step in place as they do (`gait::Turning`, the rule the
        // server's body clock and the hit zones follow). Offline the rig
        // keeps its own feet and steps its own stride.
        let mut stepped = 0.0;
        (self.turn, self.step) = frame.turn.unwrap_or_else(|| {
            if let Some(ladder) = frame.ladder {
                return (self.own_turning.hold(gait::yaw_of(ladder), frame.yaw), 0.0);
            }
            let still = frame.alive
                && frame.on_floor
                && !frame.mantling
                && frame.stance != Stance::Sliding
                && speed <= gait::MOVING_SPEED;
            let (turn, strides) = self.own_turning.tick(frame.yaw, still, dt);
            stepped = strides;
            (turn, self.own_turning.step)
        });

        let kneeling = matches!(
            frame.action,
            Some((ActionPose::Plant | ActionPose::Defuse, _))
        );
        let crouch_target = if frame.stance == Stance::Crouching || kneeling {
            1.0
        } else {
            0.0
        };
        let slide_target = if frame.stance == Stance::Sliding {
            1.0
        } else {
            0.0
        };
        self.crouch = match frame.crouch {
            // The body goes down and up on the simulation's own curve, the
            // one its capsule, eye, and hit zones follow.
            Some(_) if kneeling => self.crouch + (1.0 - self.crouch) * smoothing(10.0, dt),
            Some(blend) => shooter_sim::movement::crouch_ease(blend),
            None => self.crouch + (crouch_target - self.crouch) * smoothing(12.0, dt),
        };
        // A slide drops in with `Slide_Start` (sped up to be down as soon as
        // the hit zones are) and gets up with `Slide_Exit`.
        let sliding = frame.stance == Stance::Sliding;
        if sliding && !self.was_sliding {
            self.slide_for = Some(0.0);
            self.slide_exit = None;
        } else if !sliding && self.was_sliding {
            self.slide_for = None;
            self.slide_exit = Some(0.0);
        }
        self.was_sliding = sliding;
        if let Some(t) = &mut self.slide_for {
            *t += dt;
        }
        self.slide_exit = self
            .slide_exit
            .map(|t| t + dt)
            .filter(|t| *t < SLIDE_EXIT_SECONDS);
        self.slide = if sliding {
            (self.slide + dt / 0.04).min(1.0)
        } else if let Some(t) = self.slide_exit {
            // Up through the exit, then let the stance take over.
            1.0 - ((t - (SLIDE_EXIT_SECONDS - 0.1)) / 0.1).clamp(0.0, 1.0)
        } else {
            self.slide + (slide_target - self.slide) * smoothing(14.0, dt)
        };

        // A jump: the push-off of `Jump_Start` as the feet leave the ground,
        // the air loop, and a landing (`Jump_Land`) as strong as the fall
        // was long.
        let was_on_floor = self.air_time <= 0.0;
        // Hanging on a ladder is not being in the air.
        if frame.on_floor || frame.ladder.is_some() {
            if self.air_time > LANDING_MIN_AIR {
                let strength = ((self.air_time - LANDING_MIN_AIR) / LANDING_FULL_AIR).min(1.0);
                self.landing = Some((0.0, strength));
            }
            self.air_time = 0.0;
            self.takeoff = None;
        } else {
            if was_on_floor && frame.velocity.y > 1.5 && frame.alive {
                self.takeoff = Some(0.0);
            }
            self.air_time += dt;
        }
        self.takeoff = self
            .takeoff
            .map(|t| t + dt)
            .filter(|t| *t < TAKEOFF_SECONDS);
        self.landing = self
            .landing
            .map(|(t, s)| (t + dt, s))
            .filter(|(t, _)| *t < LANDING_SECONDS);
        // A jump goes into the air pose at once; anything else (a step off
        // an edge, stairs) only once it has been in the air a moment.
        let air_target = if self.takeoff.is_some() || self.air_time > 0.12 {
            1.0
        } else {
            0.0
        };
        let rate = if self.takeoff.is_some() { 25.0 } else { 10.0 };
        self.air += (air_target - self.air) * smoothing(rate, dt);

        // Leaning into a start and back out of a stop.
        let local_velocity = Basis::from_axis_angle(Vector3::UP, -frame.yaw) * frame.velocity;
        let local_velocity = Vector3::new(local_velocity.x, 0.0, local_velocity.z);
        let grounded = frame.on_floor && !sliding && !frame.mantling;
        let lean_target = match self.last_local {
            Some(last) if dt > 0.0 && grounded => {
                let accel = (local_velocity - last) / dt;
                let lean = accel * (MOTION_LEAN_DEGREES.to_radians() / MOTION_LEAN_ACCEL);
                lean.limit_length(Some(MOTION_LEAN_DEGREES.to_radians()))
            }
            _ => Vector3::ZERO,
        };
        self.last_local = Some(local_velocity);
        self.motion_lean
            .step_toward(lean_target, MOTION_LEAN_STIFFNESS, MOTION_LEAN_DAMPING, dt);

        let stand_position = if moving {
            gait::stand_position(speed, frame.sprinting, self.direction)
        } else {
            0.0
        };
        // Every loop runs on one stride phase: the server's, so the body is
        // drawn at the moment its hit zones are posed at, or offline the
        // rig's own, run at the loops' cadence.
        self.stride = frame.stride.unwrap_or_else(|| {
            let rate = if frame.alive {
                gait::stride_rate(
                    to_sim(frame.velocity),
                    frame.yaw,
                    frame.stance,
                    frame.sprinting,
                    frame.on_floor,
                )
            } else {
                0.0
            };
            (gait::advance_stride(self.stride, rate, dt) + stepped).rem_euclid(1.0)
        });
        self.mantle_progress = match frame.mantle_progress {
            Some(p) => p,
            None if frame.mantling && !self.was_mantling => 0.0,
            None if frame.mantling => (self.mantle_progress + dt / OFFLINE_CLIMB_SECONDS).min(1.0),
            None => self.mantle_progress,
        };
        // The climb blends in with the mantle's progress, as the hit zones
        // blend it, and out over a moment once the player is up.
        self.climb = if frame.mantling && frame.alive {
            gait::climb_share(self.mantle_progress)
        } else {
            (self.climb - dt / CLIMB_FADE_OUT).max(0.0)
        };

        // Weapon pose.
        let sprint_target = if frame.sprinting && frame.alive {
            1.0
        } else {
            0.0
        };
        self.sprint += (sprint_target - self.sprint) * smoothing(10.0, dt);
        let (reload_target, progress) = match frame.reload {
            Some(p) => (1.0, p),
            None => (0.0, self.reload_progress),
        };
        self.reload_progress = progress;
        self.reload_blend += (reload_target - self.reload_blend) * smoothing(12.0, dt);
        self.melee = match self.melee {
            Some(t) if t + dt < 0.6 && frame.alive => Some(t + dt),
            // A new swing starts as the flag rises.
            _ if frame.meleeing && !was_meleeing && frame.alive => Some(0.0),
            _ => None,
        };
        if !self.lowering {
            self.switch_drop = (self.switch_drop - dt / SWITCH_RAISE_SECONDS).max(0.0);
        }
        self.kick.step(260.0, 22.0, dt);
        self.lean.step(LEAN_STIFFNESS, LEAN_DAMPING, dt);
        self.head.step(HEAD_STIFFNESS, HEAD_DAMPING, dt);
        self.twist.step(LEAN_STIFFNESS, LEAN_DAMPING, dt);
        self.buckle.step(BUCKLE_STIFFNESS, BUCKLE_DAMPING, dt);
        for t in &mut self.arm_drop {
            *t = (*t - dt).max(0.0);
        }
        let ik_target = if frame.alive && self.weapon.is_some() && frame.ladder.is_none() {
            1.0
        } else {
            0.0
        };
        self.ik += (ik_target - self.ik) * smoothing(if frame.alive { 12.0 } else { 20.0 }, dt);
        self.update_ladder(frame, dt);

        self.update_feet(frame, moving || self.step > 0.0, dt);
        let input = self.pose_input(frame);
        self.pose_magazine(frame);
        if let Some(poser) = &mut self.poser {
            poser.bind_mut().input = input;
        }

        if let Some(tree) = &mut self.tree {
            let set = |tree: &mut Gd<AnimationTree>, path: &str, value: Variant| {
                tree.set(&format!("parameters/{path}"), &value);
            };
            if self.zombie {
                // Walkers shamble; faster zombies blend into a jog and a run.
                let position = if !moving {
                    0.0
                } else if speed < ZOMBIE_SHAMBLE_LIMIT {
                    ZOMBIE_WALK_SPEED
                } else {
                    speed.min(gait::SPRINT_SPEED)
                };
                let rate = if moving {
                    (speed / position.max(ZOMBIE_WALK_SPEED)).clamp(0.5, 1.8)
                } else {
                    1.0
                };
                set(tree, "walk/blend_position", position.to_variant());
                set(tree, "walk_rate/scale", rate.to_variant());
            } else {
                // Stepping round on the spot plays the walk at the stride, as
                // the hit zones pose it.
                let (stand_position, walk) = if stand_position <= 0.0 && self.step > 0.0 {
                    (gait::WALK_SPEED, self.step * gait::TURN_STEP_WEIGHT)
                } else {
                    (stand_position, gait::walk_share(stand_position))
                };
                // The travel's direction picks the loops; each direction
                // blends its own by speed.
                set(
                    tree,
                    "stand_loops/blend_position",
                    self.direction.to_variant(),
                );
                for (_, name, _) in DIRECTION_POINTS {
                    set(
                        tree,
                        &format!("stand_loops/{name}/blend_position"),
                        stand_position.to_variant(),
                    );
                }
                set(
                    tree,
                    "crouch_loop/blend_position",
                    self.direction.to_variant(),
                );
                set(tree, "stand_seek/seek_request", self.stride.to_variant());
                set(tree, "stand/blend_amount", walk.to_variant());
                set(tree, "crouch_seek/seek_request", self.stride.to_variant());
                let crouch_walk = if moving { 1.0f32 } else { 0.0 };
                set(tree, "crouch/blend_amount", crouch_walk.to_variant());
                set(
                    tree,
                    "climb_seek/seek_request",
                    self.mantle_progress.to_variant(),
                );
                set(tree, "climb/blend_amount", self.climb.to_variant());
                // A leg hit dips the body toward a crouch for a moment.
                let stance = (self.crouch + self.buckle.value).clamp(0.0, 1.0);
                set(tree, "stance/blend_amount", stance.to_variant());

                set(tree, "slide/blend_amount", self.slide.to_variant());
                let start = self.slide_for.map_or(1.0, |t| {
                    (t * SLIDE_START_SPEED / (SLIDE_START_END - SLIDE_START_FROM)).min(1.0)
                });
                set(tree, "slide_in/blend_amount", (1.0 - start).to_variant());
                let start_at = SLIDE_START_FROM + self.slide_for.unwrap_or(0.0) * SLIDE_START_SPEED;
                set(
                    tree,
                    "slide_start_seek/seek_request",
                    start_at.min(SLIDE_START_END).to_variant(),
                );
                let exit = self.slide_exit.map_or(0.0, |_| 1.0);
                set(tree, "slide_out/blend_amount", exit.to_variant());
                let exit_at = SLIDE_EXIT_FROM
                    + self.slide_exit.unwrap_or(0.0) / SLIDE_EXIT_SECONDS * (0.5 - SLIDE_EXIT_FROM);
                set(tree, "slide_exit_seek/seek_request", exit_at.to_variant());
                set(tree, "air/blend_amount", self.air.to_variant());
                let push = self
                    .takeoff
                    .map_or(0.0, |t| 1.0 - (t / TAKEOFF_SECONDS).clamp(0.0, 1.0));
                set(tree, "jump_in/blend_amount", push.to_variant());
                set(
                    tree,
                    "jump_start_seek/seek_request",
                    (TAKEOFF_FROM + self.takeoff.unwrap_or(0.0)).to_variant(),
                );
                let (land_at, land) = self.landing.map_or((0.0, 0.0), |(t, s)| {
                    let envelope = (t / 0.04).min(1.0)
                        * (1.0 - ((t - 0.3) / (LANDING_SECONDS - 0.3)).clamp(0.0, 1.0));
                    (t, s * envelope * LANDING_WEIGHT)
                });
                set(tree, "land/blend_amount", land.to_variant());
                set(
                    tree,
                    "land_seek/seek_request",
                    (LANDING_FROM + land_at).to_variant(),
                );
                let hold = if self.weapon.is_some() && frame.alive {
                    1.0f32
                } else {
                    0.0
                };
                set(tree, "hold/blend_amount", hold.to_variant());
            }
            self.was_mantling = frame.mantling;
            tree.advance(f64::from(dt));
        }
    }

    /// Foot IK for a body near the camera standing on the ground: one ray
    /// down under each foot finds the ground, and the lifts ease toward it.
    fn update_feet(&mut self, frame: &SoldierFrame, moving: bool, dt: f32) {
        let origin = self.base().get_global_position();
        let near = self
            .base()
            .get_viewport()
            .and_then(|v| v.get_camera_3d())
            .is_some_and(|c| c.get_global_position().distance_to(origin) < FOOT_IK_DISTANCE);
        let grounded = frame.alive
            && frame.on_floor
            && frame.ladder.is_none()
            && !frame.mantling
            && frame.stance != Stance::Sliding
            && self.air < 0.05;
        let mut target = [0.0f32; 2];
        if near && grounded && !moving {
            let seen = self.poser.as_ref().map(|p| p.bind().feet_seen);
            let space = self
                .base()
                .get_world_3d()
                .and_then(|w| w.get_direct_space_state());
            if let (Some(seen), Some(mut space)) = (seen, space) {
                for (i, foot) in seen.into_iter().enumerate() {
                    if foot == Vector3::ZERO {
                        continue;
                    }
                    let top = Vector3::new(foot.x, origin.y + FOOT_RAY_UP, foot.z);
                    let bottom = Vector3::new(foot.x, origin.y - FOOT_RAY_DOWN, foot.z);
                    let Some(query) = PhysicsRayQueryParameters3D::create_ex(top, bottom)
                        .collision_mask(1)
                        .done()
                    else {
                        continue;
                    };
                    let hit = space.intersect_ray(&query);
                    if let Some(at) = hit.get("position").and_then(|p| p.try_to::<Vector3>().ok()) {
                        target[i] = (at.y - origin.y).clamp(-FOOT_RAY_DOWN, FOOT_RAY_UP);
                    }
                }
            }
        }
        let k = smoothing(FOOT_IK_RATE, dt);
        for (lift, target) in self.feet_lift.iter_mut().zip(target) {
            *lift += (target - *lift) * k;
        }
        self.feet_on = self.feet_lift.iter().any(|l| l.abs() > 0.002);
    }

    /// On a ladder: blends the climb in (out again once off it), finds the
    /// rungs, and puts the hands and feet on them at the body's height. The
    /// weapon is put away while the hands are on the rungs.
    fn update_ladder(&mut self, frame: &SoldierFrame, dt: f32) {
        let on = frame.alive && frame.ladder.is_some();
        let target = if on { 1.0 } else { 0.0 };
        self.ladder += (target - self.ladder) * smoothing(if on { 14.0 } else { 10.0 }, dt);
        if self.ladder < 0.01 {
            self.ladder = 0.0;
        }
        if let Some((_, view)) = &mut self.weapon {
            let shown = self.ladder < 0.5;
            if view.is_visible() != shown {
                view.set_visible(shown);
            }
        }
        if self.ladder <= 0.0 {
            self.ladder_limbs = None;
            return;
        }
        let feet = self.base().get_global_position();
        let found = self
            .base()
            .get_world_3d()
            .and_then(|w| w.get_direct_space_state())
            .and_then(|mut space| crate::ladder::find_volume(&mut space, feet));
        match found {
            Some((volume, ladder)) => {
                let (hands, ankles) =
                    crate::ladder::limb_targets(feet, &ladder, volume.bind().rungs());
                self.ladder_limbs = Some(LadderLimbs {
                    weight: self.ladder,
                    hands,
                    ankles,
                    facing: Vector3::new(ladder.facing.x, 0.0, ladder.facing.y),
                });
            }
            // Out of the ladder's reach (stepping off): the limbs keep their
            // last holds as the climb blends out.
            None => {
                if let Some(limbs) = &mut self.ladder_limbs {
                    limbs.weight = self.ladder;
                }
            }
        }
    }

    /// The animation progress the body is drawn at: what its hit zones pose.
    pub fn progress(&self) -> Progress {
        Progress {
            stride: self.stride,
            reload: self.frame.reload.unwrap_or(0.0),
            mantle: if self.frame.mantling {
                self.mantle_progress
            } else {
                0.0
            },
            turn: self.turn,
            step: self.step,
        }
    }

    /// A zombie's scratch attack (zombie rigs only).
    pub fn attack(&mut self) {
        if let Some(tree) = &mut self.tree
            && self.zombie
        {
            tree.set(
                "parameters/attack/request",
                &(godot::classes::animation_node_one_shot::OneShotRequest::FIRE.ord()).to_variant(),
            );
        }
    }

    /// A zombie rocks back from a heavy hit (`Hit_Knockback`; zombie rigs
    /// only).
    pub fn stagger(&mut self) {
        if let Some(tree) = &mut self.tree
            && self.zombie
        {
            tree.set(
                "parameters/stagger/request",
                &(godot::classes::animation_node_one_shot::OneShotRequest::FIRE.ord()).to_variant(),
            );
        }
    }

    /// Muzzle flash and a kick for one shot.
    pub fn fire(&mut self) {
        let mut scale = 1.0;
        if let Some((id, view)) = &mut self.weapon {
            view.bind_mut().flash_world();
            scale = recoil_scale(id.def());
        }
        // Its fired case, into the local player's world pool.
        self.cases = self.cases.wrapping_add(1);
        let tree = self.base().get_tree();
        if let (Some((_, view)), Some(mut effects)) = (
            self.weapon.clone(),
            crate::effects::CombatEffects::find(&tree),
        ) {
            let mut view = view;
            view.bind_mut().eject_world_case(&mut effects, self.cases);
        }
        // The gun kicks as hard as the weapon kicks, and the shoulder and
        // spine take some of it.
        self.kick.velocity += Vector3::new(
            impulse_for_peak(4f32.to_radians() * scale, 260.0, 22.0),
            0.0,
            impulse_for_peak(0.035 * scale, 260.0, 22.0),
        );
        self.lean.velocity += Vector3::BACK
            * impulse_for_peak(
                (SHOULDER_PUSH_DEGREES * scale).to_radians(),
                LEAN_STIFFNESS,
                LEAN_DAMPING,
            );
    }

    /// A hit travelling along `direction` (world), nothing more known: the
    /// torso flinches away. On a kill the body falls with the shot's push.
    pub fn hit(&mut self, direction: Vector3, killed: bool) {
        self.hit_at(
            Push {
                direction,
                part: None,
                point: None,
                force: DEFAULT_FORCE,
                knife: false,
            },
            killed,
        );
    }

    /// A hit that pushes as `push` says: the part hit reacts (the head
    /// snaps, the torso tips and twists, an arm loosens its grip, the knees
    /// buckle). A kill keeps it for the fall that follows once the owner
    /// draws the body dead.
    pub fn hit_at(&mut self, push: Push, killed: bool) {
        self.note_hit(push);
        if killed || !self.frame.alive {
            return;
        }
        let Some(away) = Vector3::new(push.direction.x, 0.0, push.direction.z).try_normalized()
        else {
            return;
        };
        self.flinches += 1;
        // In the body's frame: +X right, -Z forward.
        let local = Basis::from_axis_angle(Vector3::UP, -self.frame.yaw) * away;
        let scale = (push.force.max(1.0) / DEFAULT_FORCE)
            .sqrt()
            .min(MAX_REACTION);
        let tip =
            |peak: f32, k: f32, d: f32| local * impulse_for_peak(peak.to_radians() * scale, k, d);
        use shooter_sim::hitbox::BodyPart as P;
        match push.part.unwrap_or(P::Chest) {
            P::Head => {
                self.head.velocity += tip(HEAD_PEAK_DEGREES, HEAD_STIFFNESS, HEAD_DAMPING);
                self.lean.velocity += tip(3.0, LEAN_STIFFNESS, LEAN_DAMPING);
            }
            P::LeftUpperArm | P::LeftLowerArm | P::RightUpperArm | P::RightLowerArm => {
                let right = matches!(push.part, Some(P::RightUpperArm | P::RightLowerArm));
                self.arm_drop[usize::from(!right)] = ARM_DROP_SECONDS;
                self.lean.velocity += tip(3.0, LEAN_STIFFNESS, LEAN_DAMPING);
                // The shoulder hit goes back with the round.
                let side = if right { -1.0 } else { 1.0 };
                self.twist.velocity += side
                    * impulse_for_peak(
                        (TWIST_PEAK_DEGREES * scale).to_radians(),
                        LEAN_STIFFNESS,
                        LEAN_DAMPING,
                    );
            }
            P::LeftUpperLeg | P::LeftLowerLeg | P::RightUpperLeg | P::RightLowerLeg => {
                self.buckle.velocity += impulse_for_peak(
                    BUCKLE_PEAK * scale.min(1.5),
                    BUCKLE_STIFFNESS,
                    BUCKLE_DAMPING,
                );
                // The body folds forward over the buckling knee.
                self.lean.velocity += Vector3::FORWARD
                    * impulse_for_peak(4f32.to_radians(), LEAN_STIFFNESS, LEAN_DAMPING)
                    + tip(2.0, LEAN_STIFFNESS, LEAN_DAMPING);
            }
            P::Hips | P::Spine | P::Chest => {
                self.lean.velocity += tip(TORSO_PEAK_DEGREES, LEAN_STIFFNESS, LEAN_DAMPING);
                // A round from the side turns the torso away from it.
                self.twist.velocity += -local.x
                    * impulse_for_peak(
                        (TWIST_PEAK_DEGREES * scale).to_radians(),
                        LEAN_STIFFNESS,
                        LEAN_DAMPING,
                    );
            }
        }
    }

    /// Remembers a hit for the fall a death may follow it with, without
    /// reacting to it (it was already shown, predicted).
    pub fn note_hit(&mut self, push: Push) {
        self.last_hit = Some((push, 0.0));
        if let (Some(point), Some(skeleton)) = (push.point, self.skeleton()) {
            self.wounds
                .add(&skeleton, point, wounds::depth_of(push.force));
        }
    }

    /// The weapon's magazine through a reload, on the same timeline as the
    /// support hand (`carry::Carry::magazine_pose`).
    fn pose_magazine(&mut self, frame: &SoldierFrame) {
        let progress = self.reload_progress;
        let Some((id, view)) = &mut self.weapon else {
            return;
        };
        let pose = frame.reload.filter(|_| frame.alive).map(|_| {
            let def = id.def();
            Carry {
                reload: 1.0,
                reload_progress: progress,
                class: carry::CarryClass::of(def),
                timing: carry::ReloadTiming::of(def),
                ..Carry::default()
            }
            .magazine_pose(&carry::Grips::held(def))
        });
        view.bind_mut().pose_world_magazine(pose);
    }

    fn pose_input(&self, frame: &SoldierFrame) -> PoseInput {
        // A knife swing drops the gun most of the way to the sprint carry
        // and leans the body into the strike.
        let swing = self.melee.map_or(0.0, |t| {
            crate::weapon_clips::keyed(&[(0.0, 0.0), (0.08, 1.0), (0.3, 1.0), (0.5, 0.0)], t)
        });
        let lunge = self.melee.map_or(0.0, |t| {
            crate::weapon_clips::keyed(&[(0.0, 0.0), (0.12, 1.0), (0.34, 0.0)], t)
        });
        // The same carry the arm hit zones are posed from, with this rig's
        // smoothed blends, kick, weapon switch, and knife swing on top.
        let def = self.weapon.as_ref().map(|(id, _)| id.def());
        // Hands busy with something else: the weapon drops out of the way
        // as for a knife swing.
        let (action_drop, action_reach) = match frame.action {
            Some((ActionPose::Plant | ActionPose::Defuse, _)) => (1.0, Some(PLANT_REACH)),
            Some((ActionPose::Throw, t)) => (
                crate::weapon_clips::keyed(&[(0.0, 0.0), (0.15, 1.0), (0.8, 1.0), (1.0, 0.0)], t),
                None,
            ),
            None => (0.0, None),
        };
        let swing = swing.max(action_drop);
        let carry = Carry {
            ads: frame.ads,
            sprint: self.sprint,
            reload: self.reload_blend,
            reload_progress: self.reload_progress,
            kick: to_sim(self.kick.value),
            switch_drop: self.switch_drop,
            swing,
            class: def.map_or_else(Default::default, carry::CarryClass::of),
            timing: def.map_or_else(Default::default, carry::ReloadTiming::of),
        };
        // The support hand's reload path, from the grips the server poses the
        // arm zones with.
        let held = def.map_or_else(carry::Grips::default, carry::Grips::held);
        let (mut support_target, mut support_off) = carry.support_path(&held);
        if let Some(reach) = action_reach {
            support_target = reach;
            support_off = action_drop;
        }
        let (offset, rotation) = carry.weapon_offset();
        let (offset, rotation) = (to_godot(offset), to_godot_basis(rotation));

        // The slash is keyed from the eyes, where the weapon rests at the hip,
        // so it stays at head height while the gun drops out of its way.
        let weapon_from_eyes = Transform3D::new(rotation, offset).affine_inverse()
            * Transform3D::new(Basis::IDENTITY, to_godot(carry::HIP_OFFSET));
        let knife = self.melee.map_or((0.0, Transform3D::IDENTITY), |t| {
            let (blend, at) = crate::knife::third_person(t);
            (blend, weapon_from_eyes * at)
        });
        // A throw: the support hand carries the grenade over and out.
        let throw = match frame.action {
            Some((ActionPose::Throw, t)) => {
                let pose = crate::grenade_hold::third_person(t, frame.throw_kind);
                Some((pose, weapon_from_eyes * pose.at))
            }
            _ => None,
        };
        let grips = self.weapon.as_ref().map(|(id, view)| {
            let mut g = view.bind().grips();
            g.hold = Hold::of(id.def());
            g.fit_to(id.def());
            // The server poses the arm hit zones from the weapon file's grips
            // (`carry::hand_targets`), so the third-person hands stay on the
            // same numbers rather than the measured handguard.
            g.section = None;
            if g.hold == Hold::Handguard && !g.foregrip {
                // Mid-handguard rather than at the muzzle end of it, so the
                // support elbow bends and drops under the weapon. The fitted
                // support pose was fitted where the first-person hand holds,
                // so here the hand keeps the hold's shape. (On a foregrip the
                // hand stays on the foregrip, where its pose was fitted.)
                g.left.z += carry::SUPPORT_BACK;
                g.fingers[1] = None;
                g.spreads[1] = None;
                g.seats[1] = Vector3::ZERO;
            }
            g
        });
        let slide = 1.0 - (1.0 - carry::SLIDE_PITCH_SCALE) * self.slide;
        PoseInput {
            lower_yaw: self.lower_yaw,
            turn: if frame.alive { self.turn } else { 0.0 },
            pitch: frame.pitch.clamp(-carry::MAX_PITCH, carry::MAX_PITCH) * slide,
            lean: self.lean.value + self.motion_lean.value + Vector3::new(0.0, 0.0, -0.3) * lunge,
            head_lean: self.head.value,
            twist: self.twist.value,
            arm_loose: self
                .arm_drop
                .map(|t| (1.0 - ARM_DROP_WEIGHT) * (t / ARM_DROP_SECONDS).clamp(0.0, 1.0)),
            placement: WeaponPlacement::FromEyes { offset, rotation },
            grips,
            ik: self.ik,
            // The support hand fetches a magazine in the middle of the reload.
            support_off,
            support_target: to_godot(support_target),
            // Trigger discipline: off the trigger while sprinting or reloading.
            trigger_off: carry.trigger_off(),
            knife: knife.0,
            knife_at: knife.1,
            held: super::poser::Held::Knife,
            grenade: throw.map_or(0.0, |(pose, _)| pose.blend),
            grenade_at: throw.map_or(Transform3D::IDENTITY, |(_, at)| at),
            grenade_in_hand: throw.is_some_and(|(pose, _)| pose.in_hand),
            grenade_pinned: throw.is_some_and(|(pose, _)| pose.pinned),
            grenade_kind: throw.map_or(Default::default(), |(pose, _)| pose.kind),
            feet: (self.feet_on && frame.alive).then_some(self.feet_lift),
            ladder: self.ladder_limbs.filter(|_| frame.alive),
        }
    }

    /// Starts a death: the poser captures the pose drawn this frame, and the
    /// next step builds the ragdoll from it.
    fn begin_death(&mut self) {
        let push = self
            .last_hit
            .filter(|(_, age)| *age <= HIT_MEMORY)
            .map(|(push, _)| push);
        if let Some(poser) = &mut self.poser {
            poser.bind_mut().capture = true;
        }
        self.death = Death::Capturing {
            push,
            velocity: self.frame.velocity,
            frames: 0,
        };
    }

    /// Advances a dead body: builds its ragdoll once the pose is captured,
    /// writes the ragdoll's pose, and freezes it at rest. The corpse yard
    /// calls this for bodies it holds.
    pub fn step_corpse(&mut self, dt: f32) {
        match std::mem::take(&mut self.death) {
            Death::Alive => {}
            Death::Capturing {
                push,
                velocity,
                frames,
            } => {
                let captured = self
                    .poser
                    .as_mut()
                    .and_then(|p| p.bind_mut().captured.take());
                let pose = match captured {
                    Some(pose) => Some(pose),
                    None if frames + 1 >= CAPTURE_FRAMES => self.skeleton().map(|s| base_pose(&s)),
                    None => {
                        self.death = Death::Capturing {
                            push,
                            velocity,
                            frames: frames + 1,
                        };
                        return;
                    }
                };
                self.death = self.start_ragdoll(pose, velocity, push);
            }
            Death::Ragdoll(mut ragdoll) => {
                let moving = ragdoll.step(dt);
                self.fall_to = ragdoll.hips();
                if moving {
                    self.death = Death::Ragdoll(ragdoll);
                } else {
                    ragdoll.freeze();
                    self.death = Death::Still;
                }
            }
            Death::Still => self.death = Death::Still,
        }
    }

    /// Stops a moving ragdoll where it is (too many moving, or the body is
    /// sinking away).
    pub fn freeze_ragdoll(&mut self) {
        if let Death::Ragdoll(ragdoll) = std::mem::take(&mut self.death) {
            self.fall_to = ragdoll.hips();
            ragdoll.freeze();
            self.death = Death::Still;
        } else if matches!(self.death, Death::Alive) {
            self.death = Death::Alive;
        }
    }

    fn start_ragdoll(
        &mut self,
        pose: Option<Vec<ragdoll::BonePose>>,
        velocity: Vector3,
        push: Option<Push>,
    ) -> Death {
        // The poser and the animation stop: the skeleton's own pose is the
        // body from here on.
        if let Some(poser) = &mut self.poser {
            poser.set_active(false);
            poser.bind_mut().weapon = None;
        }
        self.set_contact_shadow(false);
        // Legs that stop running shed some of the run: a body tumbles on
        // rather than skating across the ground.
        let velocity = Vector3::new(
            velocity.x * CARRIED_RUN,
            velocity.y,
            velocity.z * CARRIED_RUN,
        );
        self.drop_weapon(velocity);
        let (Some(pose), Some(skeleton)) = (pose, self.skeleton()) else {
            return Death::Still;
        };
        let owner = self.base().instance_id();
        let Some(ragdoll) = Ragdoll::start(skeleton, pose, velocity, push, owner) else {
            return Death::Still;
        };
        self.fall_from = ragdoll.hips();
        self.fall_to = self.fall_from;
        let cap = if self.zombie {
            ragdoll::MAX_SIMULATING_ZOMBIES
        } else {
            ragdoll::MAX_SIMULATING
        };
        for id in ragdoll::admit(owner, cap) {
            if id == owner {
                continue;
            }
            if let Ok(mut other) = Gd::<SoldierRig>::try_from_instance_id(id) {
                other.bind_mut().freeze_ragdoll();
            }
        }
        Death::Ragdoll(ragdoll)
    }

    /// Lets go of the held gun: it falls on its own, into the scene's yard.
    fn drop_weapon(&mut self, velocity: Vector3) {
        let Some((_, mut view)) = self.weapon.take() else {
            return;
        };
        // Dropped mid-reload, the magazine goes back in.
        view.bind_mut().pose_world_magazine(None);
        if !self.base().is_inside_tree() {
            return;
        }
        let tree = self.base().get_tree();
        let seed = splitmix(self.base().instance_id().to_i64().unsigned_abs());
        let unit = |shift: u32| ((seed >> shift) & 0xff) as f32 / 255.0 * 2.0 - 1.0;
        let spin = Vector3::new(unit(0), unit(8), unit(16)) * 3.0;
        let toss = Vector3::new(unit(24), 0.3, unit(32)) * 0.6;
        super::corpses::drop_weapon(&tree, view.upcast(), velocity + toss, spin);
    }

    /// The soft patch under the feet (`scripts/grounding.gd`) follows the
    /// rig, not a ragdoll: it goes off while the body lies.
    fn set_contact_shadow(&mut self, on: bool) {
        if let Some(mut patch) = self.base().get_node_or_null("ContactShadow") {
            patch.set_process_mode(if on {
                godot::classes::node::ProcessMode::INHERIT
            } else {
                godot::classes::node::ProcessMode::DISABLED
            });
            if let Ok(mut patch) = patch.try_cast::<Node3D>() {
                patch.set_visible(on);
            }
        }
    }

    /// Back to life (a respawn in place): the ragdoll goes, and the
    /// animation and poser take the body again.
    fn revive(&mut self) {
        if let Death::Ragdoll(ragdoll) = std::mem::take(&mut self.death) {
            ragdoll.discard();
        }
        self.death = Death::Alive;
        self.set_contact_shadow(true);
        self.last_hit = None;
        self.wounds.clear();
        self.lean = Spring::default();
        self.head = Spring::default();
        self.twist = Spring::default();
        self.buckle = Spring::default();
        self.arm_drop = [0.0; 2];
        if let Some(poser) = &mut self.poser {
            poser.set_active(true);
            let mut p = poser.bind_mut();
            p.capture = false;
            p.captured = None;
        }
    }

    fn skeleton(&self) -> Option<Gd<Skeleton3D>> {
        self.model
            .find_child_ex("GeneralSkeleton")
            .owned(false)
            .done()
            .and_then(|n| n.try_cast::<Skeleton3D>().ok())
    }

    fn apply_team(&mut self) {
        let team = Team::from_index(self.team);
        if self.applied_team == Some(team) {
            return;
        }
        self.applied_team = Some(team);
        let Some(skeleton) = self.skeleton() else {
            return;
        };
        // The kit this character wears (face coverings), from a per-character seed.
        let seed = self.base().instance_id().to_i64().unsigned_abs();
        let hidden_parts = palette::hidden_parts(seed, team);
        // And a face: a head shape (blend shapes on the soldier's meshes) and
        // the skin tone that goes with it.
        let face = palette::face(seed, team);
        for child in skeleton.get_children().iter_shared() {
            if let Ok(mut mesh) = child.try_cast::<MeshInstance3D>() {
                mesh.set_instance_shader_parameter("hidden_parts", &hidden_parts.to_variant());
                if let Some(face) = face {
                    let [r, g, b] = face.tone;
                    mesh.set_instance_shader_parameter(
                        "skin_tone",
                        &Vector3::new(r, g, b).to_variant(),
                    );
                    for i in 0..mesh.get_blend_shape_count() {
                        mesh.set_blend_shape_value(i, face.blend(i as usize));
                    }
                }
                recolour(mesh, team);
            }
        }
    }

    /// A weapon switch in two parts: the old weapon lowers out of the way,
    /// then the new one comes up. A first weapon, or one after a respawn,
    /// appears at once.
    fn switch_to(&mut self, weapon: Option<WeaponId>, dt: f32) {
        let held = self.weapon.as_ref().map(|(id, _)| *id);
        if held == weapon {
            self.lowering = false;
            return;
        }
        if held.is_none() || weapon.is_none() {
            self.lowering = false;
            self.set_weapon(weapon);
            return;
        }
        self.lowering = true;
        self.switch_drop = (self.switch_drop + dt / SWITCH_LOWER_SECONDS).min(1.0);
        if self.switch_drop >= 1.0 {
            self.lowering = false;
            self.set_weapon(weapon);
        }
    }

    fn set_weapon(&mut self, weapon: Option<WeaponId>) {
        if self.weapon.as_ref().map(|(id, _)| *id) == weapon {
            return;
        }
        let had_weapon = self.weapon.is_some();
        if let Some((_, old)) = self.weapon.take() {
            old.upcast::<Node>().queue_free();
        }
        let Some(id) = weapon else {
            if let Some(poser) = &mut self.poser {
                poser.bind_mut().weapon = None;
            }
            return;
        };
        let Some(mut view) = viewmodel::instantiate_world_weapon(id) else {
            godot_error!("SoldierRig: no weapon scene for `{}`", id.def().key);
            return;
        };
        view.set_name("Weapon");
        self.base_mut().add_child(&view);
        crate::effects::keep_decals_off(&view.clone().upcast());
        if let Some(poser) = &mut self.poser {
            poser.bind_mut().weapon = Some(view.clone().upcast());
        }
        self.weapon = Some((id, view));
        if had_weapon {
            // The new weapon comes up from where the old one went.
            self.switch_drop = 1.0;
        }
    }

    fn build_tree(&mut self) -> Gd<AnimationTree> {
        let mut tree = AnimationTree::new_alloc();
        tree.set_name("AnimationTree");
        for (name, path) in [
            ("ual1", "res://assets/characters/animations/ual1.glb"),
            ("ual2", "res://assets/characters/animations/ual2.glb"),
            ("gait", "res://assets/characters/animations/gait.glb"),
        ] {
            match try_load::<AnimationLibrary>(path) {
                Ok(lib) => {
                    tree.add_animation_library(name, &lib);
                }
                Err(e) => {
                    godot_error!("SoldierRig: cannot load {path}: {e}");
                }
            }
        }
        tree.set_root_node("../Model");
        if self.zombie {
            let graph = zombie_graph();
            return self.finish_tree(tree, graph);
        }

        let mut graph = AnimationNodeBlendTree::new_gd();
        let mut add = |name: &str, node: Gd<AnimationNode>, x: f32| {
            graph.add_node(name, &node);
            graph.set_node_position(name, Vector2::new(x * 220.0, 0.0));
        };
        let clip = |name: &str| {
            let mut a = AnimationNodeAnimation::new_gd();
            a.set_animation(name);
            a
        };
        // A loop on the shared stride phase: its timeline stretched to one
        // second and started where its left foot is furthest forward, so a
        // seek to the phase lands every loop on the same moment of a stride.
        let stride_clip = |l: &gait::Loop, index: usize| {
            let (name, offset) = directed_clip(l, index);
            let mut a = clip(&name);
            a.set_use_custom_timeline(true);
            a.set_timeline_length(1.0);
            a.set_stretch_time_scale(true);
            a.set_start_offset(f64::from(offset * l.seconds));
            a.set_loop_mode(godot::classes::animation::LoopMode::LINEAR);
            a
        };
        // The standing loops: by the travel's direction (the eight authored
        // directions, backward at both ends), and in each by speed. Only
        // forward has the sprint.
        let mut loops = AnimationNodeBlendSpace1D::new_gd();
        loops.set_min_space(-PI);
        loops.set_max_space(PI);
        let mut crouch_loops = AnimationNodeBlendSpace1D::new_gd();
        crouch_loops.set_min_space(-PI);
        crouch_loops.set_max_space(PI);
        for (index, name, at) in DIRECTION_POINTS {
            let mut speeds = AnimationNodeBlendSpace1D::new_gd();
            speeds.set_min_space(0.0);
            speeds.set_max_space(gait::SPRINT_SPEED);
            let count = if index == 0 {
                gait::STAND_LOOPS.len()
            } else {
                gait::DIRECTION_LOOPS
            };
            for l in &gait::STAND_LOOPS[..count] {
                let point = l.clip.rsplit('/').next().unwrap_or(l.clip);
                speeds
                    .add_blend_point_ex(
                        &stride_clip(l, index).upcast::<AnimationRootNode>(),
                        l.position,
                    )
                    .name(point)
                    .done();
            }
            loops
                .add_blend_point_ex(&speeds.upcast::<AnimationRootNode>(), at)
                .name(name)
                .done();
            crouch_loops
                .add_blend_point_ex(
                    &stride_clip(&gait::CROUCH_LOOP, index).upcast::<AnimationRootNode>(),
                    at,
                )
                .name(name)
                .done();
        }
        add("stand_idle", clip("ual1/Idle").upcast(), 0.0);
        add("stand_loops", loops.upcast(), 0.0);
        add("stand_seek", AnimationNodeTimeSeek::new_gd().upcast(), 1.0);
        add("stand", AnimationNodeBlend2::new_gd().upcast(), 1.5);
        add("crouch_idle", clip("ual1/Crouch_Idle").upcast(), 0.0);
        add("crouch_loop", crouch_loops.upcast(), 0.0);
        add("crouch_seek", AnimationNodeTimeSeek::new_gd().upcast(), 1.0);
        add("crouch", AnimationNodeBlend2::new_gd().upcast(), 1.5);
        add("stance", AnimationNodeBlend2::new_gd().upcast(), 2.0);

        add("slide_clip", clip("ual2/Slide").upcast(), 2.0);
        // Getting down into a slide and up out of it, each seeked by the rig.
        let once = |name: &str| {
            let mut a = clip(name);
            a.set_loop_mode(godot::classes::animation::LoopMode::NONE);
            a
        };
        add("slide_start_clip", once("ual2/Slide_Start").upcast(), 2.0);
        add(
            "slide_start_seek",
            AnimationNodeTimeSeek::new_gd().upcast(),
            2.0,
        );
        add("slide_in", AnimationNodeBlend2::new_gd().upcast(), 2.5);
        add("slide_exit_clip", once("ual2/Slide_Exit").upcast(), 2.0);
        add(
            "slide_exit_seek",
            AnimationNodeTimeSeek::new_gd().upcast(),
            2.0,
        );
        add("slide_out", AnimationNodeBlend2::new_gd().upcast(), 2.5);
        add("slide", AnimationNodeBlend2::new_gd().upcast(), 3.0);
        add("jump_clip", clip("ual1/Jump").upcast(), 3.0);
        // The push-off as a jump leaves the ground, and the landing.
        add("jump_start_clip", once("ual1/Jump_Start").upcast(), 3.0);
        add(
            "jump_start_seek",
            AnimationNodeTimeSeek::new_gd().upcast(),
            3.0,
        );
        add("jump_in", AnimationNodeBlend2::new_gd().upcast(), 3.5);
        add("land_clip", once("ual1/Jump_Land").upcast(), 4.0);
        add("land_seek", AnimationNodeTimeSeek::new_gd().upcast(), 4.0);
        add("land", AnimationNodeBlend2::new_gd().upcast(), 4.5);
        add("air", AnimationNodeBlend2::new_gd().upcast(), 4.0);
        // A mantle: the whole climb clip over the mantle, seeked to its
        // progress (timeline stretched to one second).
        let mut climb = clip("ual2/ClimbUp_1m_RM");
        climb.set_use_custom_timeline(true);
        climb.set_timeline_length(1.0);
        climb.set_stretch_time_scale(true);
        climb.set_loop_mode(godot::classes::animation::LoopMode::NONE);
        add("climb_clip", climb.upcast(), 4.0);
        add("climb_seek", AnimationNodeTimeSeek::new_gd().upcast(), 4.0);
        add("climb", AnimationNodeBlend2::new_gd().upcast(), 4.5);
        add("hold_clip", clip("ual1/Pistol_Aim_Neutral").upcast(), 4.0);
        let mut hold = AnimationNodeBlend2::new_gd();
        hold.set_filter_enabled(true);
        for bone in arm_bones() {
            hold.set_filter_path(&NodePath::from(&format!("%GeneralSkeleton:{bone}")), true);
        }
        add("hold", hold.upcast(), 5.0);

        let links = [
            ("stand_seek", 0, "stand_loops"),
            ("stand", 0, "stand_idle"),
            ("stand", 1, "stand_seek"),
            ("crouch_seek", 0, "crouch_loop"),
            ("crouch", 0, "crouch_idle"),
            ("crouch", 1, "crouch_seek"),
            ("stance", 0, "stand"),
            ("stance", 1, "crouch"),
            ("slide", 0, "stance"),
            ("slide_start_seek", 0, "slide_start_clip"),
            ("slide_in", 0, "slide_clip"),
            ("slide_in", 1, "slide_start_seek"),
            ("slide_exit_seek", 0, "slide_exit_clip"),
            ("slide_out", 0, "slide_in"),
            ("slide_out", 1, "slide_exit_seek"),
            ("slide", 1, "slide_out"),
            ("air", 0, "slide"),
            ("jump_start_seek", 0, "jump_start_clip"),
            ("jump_in", 0, "jump_clip"),
            ("jump_in", 1, "jump_start_seek"),
            ("air", 1, "jump_in"),
            ("land_seek", 0, "land_clip"),
            ("land", 0, "air"),
            ("land", 1, "land_seek"),
            ("climb_seek", 0, "climb_clip"),
            ("climb", 0, "land"),
            ("climb", 1, "climb_seek"),
            ("hold", 0, "climb"),
            ("hold", 1, "hold_clip"),
            ("output", 0, "hold"),
        ];
        for (to, port, from) in links {
            graph.connect_node(to, port, from);
        }
        self.finish_tree(tree, graph)
    }

    fn finish_tree(
        &mut self,
        mut tree: Gd<AnimationTree>,
        graph: Gd<AnimationNodeBlendTree>,
    ) -> Gd<AnimationTree> {
        tree.set_tree_root(&graph.upcast::<AnimationRootNode>());
        // Advanced by `update`, so the pose follows the time the owner gives it
        // (and tests and screenshots are repeatable).
        tree.set_callback_mode_process(AnimationCallbackModeProcess::MANUAL);
        self.base_mut().add_child(&tree);
        tree.set_active(true);
        tree
    }
}

/// The direction blends' points: the index into `gait::DIRECTIONS`, the
/// point's name, and where it sits (radians from the aim, positive right).
/// Backward sits at both ends.
const DIRECTION_POINTS: [(usize, &str, f32); 9] = [
    (4, "B_left", -PI),
    (5, "BL", -0.75 * PI),
    (6, "L", -0.5 * PI),
    (7, "FL", -0.25 * PI),
    (0, "Fwd", 0.0),
    (1, "FR", 0.25 * PI),
    (2, "R", 0.5 * PI),
    (3, "BR", 0.75 * PI),
    (4, "B", PI),
];

/// The clip of loop `l` as authored in direction `index`
/// (`gait::DIRECTIONS`) and its offset: forward is the source library's own;
/// the others are `gait/<Loop>_<Dir>` (`tools/build_gait_animations.py`),
/// with stride phase 0 at their start.
fn directed_clip(l: &gait::Loop, index: usize) -> (String, f32) {
    if index == 0 {
        return (l.clip.to_string(), l.offset);
    }
    let base = match l.clip {
        "ual1/Walk" => "Walk",
        "ual1/Jog_Fwd" => "Jog",
        _ => "Crouch",
    };
    (format!("gait/{base}_{}", gait::DIRECTIONS[index]), 0.0)
}

/// Force level for a hit with nothing more known (a rifle round).
const DEFAULT_FORCE: f32 = 3.0;

/// A push from a hit on shape `part` of `shooter_sim::hitbox::hitboxes` (-1
/// or out of range: unknown) at `point` (zero: unknown), with force level
/// `force`.
pub fn push_of(direction: Vector3, point: Vector3, part: i64, force: f32) -> Push {
    let part = usize::try_from(part)
        .ok()
        .and_then(|p| shooter_sim::hitbox::SHAPE_PARTS.get(p).copied());
    // A knife's level (`HitPart::KNIFE_FORCE`, 0) pushes like a light round,
    // enough to tip the body away from the blade as it folds.
    let knife = force <= f32::from(shooter_sim::net::protocol::KNIFE_FORCE);
    Push {
        direction,
        part,
        point: (point != Vector3::ZERO).then_some(point),
        force: if knife { KNIFE_PUSH_FORCE } else { force },
        knife,
    }
}

/// The push level a knife hit takes (as a rifle round's).
const KNIFE_PUSH_FORCE: f32 = 4.0;

/// Every bone's local pose as the skeleton holds it.
fn base_pose(skeleton: &Gd<Skeleton3D>) -> Vec<ragdoll::BonePose> {
    (0..skeleton.get_bone_count())
        .map(|b| {
            (
                skeleton.get_bone_pose_position(b),
                skeleton.get_bone_pose_rotation(b),
                skeleton.get_bone_pose_scale(b),
            )
        })
        .collect()
}

/// A 64-bit mix (SplitMix64's finaliser): spreads regularly spaced seeds.
fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// Distance (m) past which a character draws its far mesh, and the
/// hysteresis around it. At 15 m a soldier is about 150 pixels tall at 1080p,
/// where the baked normal map carries the detail the far mesh drops.
const FAR_DISTANCE: f32 = 15.0;
const FAR_MARGIN: f32 = 1.0;

/// LOD bias for zombies (1 is the default): a horde is two dozen bodies at
/// mid range, where the lower levels of detail look the same.
const ZOMBIE_LOD_BIAS: f32 = 0.2;

/// Eye offsets from the head bone, in skeleton space (the model faces +Z there).
const EYE_SPREAD: f32 = 0.034;
const EYE_UP: f32 = 0.075;
const EYE_FORWARD: f32 = 0.095;

/// Two glowing eyes on the head bone.
fn add_zombie_eyes(skeleton: &Gd<Skeleton3D>) {
    let head = skeleton.find_bone("Head");
    if head < 0 {
        godot_error!("SoldierRig: no Head bone for the zombie's eyes");
        return;
    }
    let rest = skeleton.get_bone_global_rest(head);
    let mut attachment = BoneAttachment3D::new_alloc();
    attachment.set_name("Eyes");
    attachment.set_bone_name("Head");
    skeleton.clone().upcast::<Node>().add_child(&attachment);
    let mut sphere = SphereMesh::new_gd();
    sphere.set_radius(0.016);
    sphere.set_height(0.026);
    sphere.set_radial_segments(8);
    sphere.set_rings(4);
    sphere.set_material(&palette::zombie_eye_material());
    for side in [-1.0, 1.0] {
        let eye = Vector3::new(side * EYE_SPREAD, EYE_UP, EYE_FORWARD);
        let mut mesh = MeshInstance3D::new_alloc();
        mesh.set_mesh(&sphere);
        mesh.set_cast_shadows_setting(
            godot::classes::geometry_instance_3d::ShadowCastingSetting::OFF,
        );
        mesh.set_position(rest.basis.inverse() * eye);
        attachment.add_child(&mesh);
    }
}

/// The zombie's animation graph: a shamble that blends into a jog and a run
/// with speed, a one-shot scratch attack, and a stagger from heavy hits.
/// Deaths are ragdolls.
fn zombie_graph() -> Gd<AnimationNodeBlendTree> {
    let mut graph = AnimationNodeBlendTree::new_gd();
    let clip = |name: &str| {
        let mut a = AnimationNodeAnimation::new_gd();
        a.set_animation(name);
        a
    };
    let mut walk = AnimationNodeBlendSpace1D::new_gd();
    walk.set_min_space(0.0);
    walk.set_max_space(gait::SPRINT_SPEED);
    for (name, position) in [
        ("ual2/Zombie_Idle", 0.0),
        ("ual2/Zombie_Walk_Fwd", ZOMBIE_WALK_SPEED),
        ("ual1/Jog_Fwd", gait::JOG_SPEED),
        ("ual1/Sprint", gait::SPRINT_SPEED),
    ] {
        let point = name.rsplit('/').next().unwrap_or(name);
        walk.add_blend_point_ex(&clip(name).upcast::<AnimationRootNode>(), position)
            .name(point)
            .done();
    }
    graph.add_node("walk", &walk.upcast::<AnimationNode>());
    graph.add_node(
        "walk_rate",
        &AnimationNodeTimeScale::new_gd().upcast::<AnimationNode>(),
    );
    let mut scratch = clip("ual2/Zombie_Scratch");
    scratch.set_loop_mode(godot::classes::animation::LoopMode::NONE);
    graph.add_node("attack_clip", &scratch.upcast::<AnimationNode>());
    let mut attack = AnimationNodeOneShot::new_gd();
    attack.set_fadein_time(0.08);
    attack.set_fadeout_time(0.25);
    graph.add_node("attack", &attack.upcast::<AnimationNode>());
    // A heavy hit rocks it back.
    let mut knockback = clip("ual2/Hit_Knockback");
    knockback.set_loop_mode(godot::classes::animation::LoopMode::NONE);
    graph.add_node("stagger_clip", &knockback.upcast::<AnimationNode>());
    let mut stagger = AnimationNodeOneShot::new_gd();
    stagger.set_fadein_time(0.05);
    stagger.set_fadeout_time(0.2);
    graph.add_node("stagger", &stagger.upcast::<AnimationNode>());
    for (to, port, from) in [
        ("walk_rate", 0, "walk"),
        ("attack", 0, "walk_rate"),
        ("attack", 1, "attack_clip"),
        ("stagger", 0, "attack"),
        ("stagger", 1, "stagger_clip"),
        ("output", 0, "stagger"),
    ] {
        graph.connect_node(to, port, from);
    }
    graph
}

/// Swaps each surface's imported material for the team's material of its kit.
fn recolour(mut mesh: Gd<MeshInstance3D>, team: Team) {
    let Some(source) = mesh.get_mesh() else {
        return;
    };
    for surface in 0..source.get_surface_count() {
        let Some(material) = source.surface_get_material(surface) else {
            continue;
        };
        if let Some(character) =
            palette::material_for(&material.get_name().to_string(), team, false)
        {
            mesh.set_surface_override_material(surface, &character);
        }
    }
}

/// False in dedicated server exports, which never draw characters: owners can
/// skip [`SoldierRig::update`] there. Hit zones do not depend on it.
pub fn visuals_enabled() -> bool {
    !godot::classes::Os::singleton().has_feature("dedicated_server")
}

/// Wraps an angle to (-PI, PI].
pub fn wrap_angle(a: f32) -> f32 {
    let mut a = a % (2.0 * PI);
    if a > PI {
        a -= 2.0 * PI;
    } else if a <= -PI {
        a += 2.0 * PI;
    }
    a
}

/// Grip markers on a weapon view, with defaults for views without them.
pub fn grips_of(view: &Gd<Node3D>, rake: f32) -> Grips {
    let mut grips = Grips {
        rake,
        ..Grips::default()
    };
    if let Some(m) = view.try_get_node_as::<Node3D>("GripRight") {
        grips.right = m.get_position();
    }
    if let Some(m) = view.try_get_node_as::<Node3D>("GripLeft") {
        grips.left = m.get_position();
        grips.section = support_section(view, grips.left);
    }
    grips
}

/// The weapon's cross-section around the support marker, as a centre and
/// half extents (weapon space): every triangle of the weapon's shown parts is
/// cut by the plane across the barrel at the marker, and the cut points within
/// 45 mm of the marker across and 35 mm up or down count. Measured once per
/// weapon view and kept as metadata.
fn support_section(view: &Gd<Node3D>, marker: Vector3) -> Option<Section> {
    const META: &str = "support_section";
    if view.has_meta(META) {
        let v = view.get_meta(META).try_to::<PackedFloat32Array>().ok()?;
        let v = v.as_slice();
        return (v.len() == 7 && v[5] > 0.0).then(|| Section {
            marker: Vector3::new(v[0], v[1], v[2]),
            centre: Vector2::new(v[3], v[4]),
            half: Vector2::new(v[5], v[6]),
        });
    }
    let mut points = Vec::new();
    for tri in super::grasp::shown_triangles(view) {
        section_points(&tri.corners, marker.z, &mut points);
    }
    let section = section_bounds(&points, marker).map(|(centre, half)| Section {
        marker,
        centre,
        half,
    });
    let packed = section.map_or([0.0; 7], |s| {
        let (m, c, h) = (s.marker, s.centre, s.half);
        [m.x, m.y, m.z, c.x, c.y, h.x, h.y]
    });
    view.clone()
        .set_meta(META, &PackedFloat32Array::from(&packed[..]).to_variant());
    section
}

/// Whether `node` and every parent up to (not including) `root` are set
/// visible: the parts the weapon shows, whether or not the weapon itself is.
pub(super) fn shown_under(node: &Gd<Node3D>, root: &Gd<Node3D>) -> bool {
    let mut at = node.clone();
    loop {
        if at == *root {
            return true;
        }
        if !at.is_visible() {
            return false;
        }
        match at.get_parent().and_then(|p| p.try_cast::<Node3D>().ok()) {
            Some(parent) => at = parent,
            None => return true,
        }
    }
}

/// Where a triangle's edges cross the plane `z = cut`.
fn section_points(tri: &[Vector3; 3], cut: f32, out: &mut Vec<Vector3>) {
    for i in 0..3 {
        let (a, b) = (tri[i], tri[(i + 1) % 3]);
        let (da, db) = (a.z - cut, b.z - cut);
        if da == 0.0 {
            out.push(a);
        } else if da * db < 0.0 {
            out.push(a.lerp(b, da / (da - db)));
        }
    }
}

/// The bounds of the section points near the marker: within 45 mm across and
/// 35 mm up or down, so sights and rails well above a handguard do not count
/// (the hand holds it from below). The centre and half extents, if any.
fn section_bounds(points: &[Vector3], marker: Vector3) -> Option<(Vector2, Vector2)> {
    let near = points
        .iter()
        .filter(|q| (q.x - marker.x).abs() < 0.045 && (q.y - marker.y).abs() < 0.035);
    let (mut lo, mut hi) = (Vector2::splat(f32::MAX), Vector2::splat(f32::MIN));
    for q in near {
        lo = lo.coord_min(Vector2::new(q.x, q.y));
        hi = hi.coord_max(Vector2::new(q.x, q.y));
    }
    // Something under 5 mm across is a sling loop or a stray edge, not a grip.
    if hi.x - lo.x < 0.005 || hi.y - lo.y < 0.005 {
        return None;
    }
    Some(((lo + hi) * 0.5, (hi - lo) * 0.5))
}

#[cfg(test)]
mod section_tests {
    use super::*;

    #[test]
    fn a_long_tube_is_cut_where_the_hand_holds_it() {
        // A square tube 40 mm across, 50 mm tall, centred 20 mm below the
        // axis, running 300 mm along the barrel with no vertex near the cut.
        let (x, y0, y1) = (0.02, -0.045, 0.005);
        let quad = |a: Vector2, b: Vector2| {
            let p = |v: Vector2, z: f32| Vector3::new(v.x, v.y, z);
            [
                p(a, 0.0),
                p(b, 0.0),
                p(b, -0.3),
                p(a, 0.0),
                p(b, -0.3),
                p(a, -0.3),
            ]
        };
        let corners = [
            Vector2::new(-x, y0),
            Vector2::new(x, y0),
            Vector2::new(x, y1),
            Vector2::new(-x, y1),
        ];
        let mut points = Vec::new();
        for i in 0..4 {
            for tri in quad(corners[i], corners[(i + 1) % 4]).as_chunks::<3>().0 {
                section_points(tri, -0.15, &mut points);
            }
        }
        let marker = Vector3::new(0.0, -0.025, -0.15);
        let (centre, half) = section_bounds(&points, marker).expect("a section");
        assert!(
            (centre - Vector2::new(0.0, -0.02)).length() < 1e-4,
            "{centre:?}"
        );
        assert!(
            (half - Vector2::new(0.02, 0.025)).length() < 1e-4,
            "{half:?}"
        );
    }

    #[test]
    fn nothing_at_the_marker_gives_no_section() {
        assert_eq!(section_bounds(&[], Vector3::ZERO), None);
    }
}

#[cfg(test)]
mod body_pick_tests {
    use super::splitmix;

    #[test]
    fn regularly_spaced_seeds_reach_every_body() {
        // Godot's instance ids step by fixed strides; every body must show up.
        for stride in [1u64, 2, 7, 8, 64, 4096] {
            let mut seen = [false; 5];
            for i in 0..40u64 {
                seen[(splitmix(0x1_0000 + i * stride) % 5) as usize] = true;
            }
            assert!(seen.iter().all(|s| *s), "stride {stride}: {seen:?}");
        }
    }
}
