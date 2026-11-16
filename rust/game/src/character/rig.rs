//! `SoldierRig`: the third-person character. It turns a frame of player state
//! (velocity, stance, view angles, aim, weapon, reload, alive) into an animated,
//! armed soldier: an animation tree for locomotion, a [`SoldierPoser`] for aim
//! and hands, a world copy of the weapon, team colours, and hit reactions.
//!
//! Remote players and target dummies drive it; nothing here affects gameplay.
//! Hit detection uses the shapes in `shooter_sim::hitbox`, which this pose is
//! fitted to.

use std::f32::consts::PI;

use godot::classes::animation_mixer::AnimationCallbackModeProcess;
use godot::classes::{
    AnimationLibrary, AnimationNode, AnimationNodeAnimation, AnimationNodeBlend2,
    AnimationNodeBlendSpace1D, AnimationNodeBlendTree, AnimationNodeOneShot,
    AnimationNodeTimeScale, AnimationNodeTransition, AnimationRootNode, AnimationTree,
    BoneAttachment3D, INode3D, MeshInstance3D, Node3D, Skeleton3D, SphereMesh, StandardMaterial3D,
};
use godot::prelude::*;
use shooter_sim::movement::Stance;
use shooter_sim::weapon::{Slot, WeaponId};

use super::palette::{self, Team};
use super::poser::{Grips, PoseInput, SoldierPoser, WeaponPlacement};
use crate::spring::{Spring, impulse_for_peak, smoothing};
use crate::viewmodel::{self, WeaponView};

/// Natural ground speeds (m/s) of the in-place loops, measured from the planted
/// foot. Playback scales to the real speed so feet do not slide.
const WALK_SPEED: f32 = 1.06;
const JOG_SPEED: f32 = 3.0;
const SPRINT_SPEED: f32 = 6.0;
const CROUCH_SPEED: f32 = 0.72;
/// Natural ground speed (m/s) of the zombie walk loop, measured from the planted foot.
const ZOMBIE_WALK_SPEED: f32 = 0.95;
/// Zombies slower than this play the shamble alone, sped up to their speed.
const ZOMBIE_SHAMBLE_LIMIT: f32 = 2.1;
/// Non-sprinting runs blend no further than this toward the sprint, then speed up.
const RUN_BLEND_LIMIT: f32 = 4.0;
/// The legs turn at most this far from the aim (radians) when strafing.
const MAX_LEG_TURN: f32 = 75.0 * PI / 180.0;

/// Weapon offsets from the eyes, in the aim frame (+X right, +Y up, -Z along the aim).
const ADS_OFFSET: Vector3 = Vector3::new(0.034, -0.045, -0.13);
const HIP_OFFSET: Vector3 = Vector3::new(0.085, -0.13, -0.15);
const SPRINT_OFFSET: Vector3 = Vector3::new(0.03, -0.3, -0.16);
/// Degrees: pitch, yaw, roll.
const HIP_ROTATION: Vector3 = Vector3::new(0.0, 3.0, -6.0);
const SPRINT_ROTATION: Vector3 = Vector3::new(-42.0, 38.0, -28.0);
/// Where the support hand reaches during a reload, in weapon space (a pouch on
/// the vest, below and behind the magazine well).
const RELOAD_REACH: Vector3 = Vector3::new(-0.12, -0.32, 0.12);

/// Hit flinch spring (radians of torso lean), as the old target dummy.
const LEAN_STIFFNESS: f32 = 140.0;
const LEAN_DAMPING: f32 = 9.0;

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
        }
    }
}

#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct SoldierRig {
    base: Base<Node3D>,

    /// 0: the player's side (tan, blue accents). 1: enemies (charcoal, red-orange).
    #[export]
    #[init(val = 1)]
    team: i32,
    /// Animate as a zombie: shambling walk, arms reaching, a scratch attack,
    /// and no weapon. Set before the rig enters the tree.
    #[export]
    zombie: bool,

    #[init(node = "Model")]
    model: OnReady<Gd<Node3D>>,
    tree: Option<Gd<AnimationTree>>,
    poser: Option<Gd<SoldierPoser>>,
    /// Body and gear, for the hit flash overlay.
    meshes: Vec<Gd<MeshInstance3D>>,
    flash_material: Option<Gd<StandardMaterial3D>>,
    weapon: Option<(WeaponId, Gd<WeaponView>)>,
    applied_team: Option<Team>,

    frame: SoldierFrame,
    lower_yaw: f32,
    walking_backward: bool,
    crouch: f32,
    slide: f32,
    air: f32,
    air_time: f32,
    sprint: f32,
    reload_blend: f32,
    reload_progress: f32,
    ik: f32,
    switch_drop: f32,
    /// Weapon kick: x pitch (radians), z push back (metres).
    kick: Spring,
    lean: Spring,
    flash: f32,
    /// Extra yaw while dying, so the body falls away from the killing shot.
    death_yaw: f32,
    death_yaw_target: f32,
    was_alive: bool,
    life_requested: Option<bool>,
}

#[godot_api]
impl INode3D for SoldierRig {
    fn ready(&mut self) {
        self.was_alive = true;
        self.ik = 1.0;
        let Some(skeleton) = self.skeleton() else {
            godot_error!("SoldierRig: no GeneralSkeleton under Model");
            return;
        };
        let mut poser = SoldierPoser::new_alloc();
        poser.set_name("Poser");
        skeleton.clone().upcast::<Node>().add_child(&poser);
        self.poser = Some(poser);

        // A zombie's gear includes its hidden helmet, which the overlay would
        // draw as a ghost: flash only its body.
        let flashing: &[&str] = if self.zombie {
            &["Body"]
        } else {
            &["Body", "Gear"]
        };
        for name in flashing {
            if let Some(mesh) = skeleton.try_get_node_as::<MeshInstance3D>(*name) {
                self.meshes.push(mesh);
            }
        }
        if let Some(mut arms) = skeleton.try_get_node_as::<MeshInstance3D>("ArmsFP") {
            arms.set_visible(false);
        }
        self.flash_material = Some(palette::hit_flash_material());
        self.apply_team();
        if self.zombie {
            add_zombie_eyes(&skeleton);
        }

        let tree = self.build_tree();
        self.tree = Some(tree);
    }

    fn process(&mut self, delta: f64) {
        let dt = delta as f32;
        if self.flash > 0.0 {
            self.flash = (self.flash - dt * 6.0).max(0.0);
            let alpha = (self.flash * 0.6).min(0.9);
            if let Some(material) = &mut self.flash_material {
                material.set_albedo(Color::from_rgba(1.0, 0.25, 0.1, alpha));
            }
            if self.flash == 0.0 {
                for mesh in &mut self.meshes {
                    mesh.set_material_overlay(Option::<&Gd<godot::classes::Material>>::None);
                }
            }
        }
    }
}

#[godot_api]
impl SoldierRig {
    /// The world position of the weapon's muzzle, or the chest if unarmed.
    #[func]
    pub fn muzzle_position(&self) -> Vector3 {
        self.weapon
            .as_ref()
            .and_then(|(_, view)| view.try_get_node_as::<Node3D>("Muzzle"))
            .map(|m| m.get_global_position())
            .unwrap_or_else(|| self.base().get_global_position() + Vector3::new(0.0, 1.4, 0.0))
    }

    /// Horizontal direction (world) the body is falling toward, or zero while alive.
    #[func]
    pub fn fall_direction(&self) -> Vector3 {
        if self.frame.alive {
            return Vector3::ZERO;
        }
        // Death01 falls backward: away from where the body faces.
        let yaw = self.frame.yaw + self.death_yaw;
        Vector3::new(yaw.sin(), 0.0, yaw.cos())
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
    /// `reload` (progress, or -1), `alive`. Missing keys keep their defaults.
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

    #[func]
    fn set_team_index(&mut self, team: i32) {
        self.team = team;
        self.apply_team();
    }
}

impl SoldierRig {
    /// Advances the character to `frame`.
    pub fn update(&mut self, frame: &SoldierFrame, dt: f32) {
        let was_alive = self.was_alive;
        self.frame = *frame;
        self.was_alive = frame.alive;
        if frame.alive && !was_alive {
            self.death_yaw = 0.0;
            self.death_yaw_target = 0.0;
            self.lean = Spring::default();
        }
        self.set_weapon(frame.weapon);

        // Body yaw. The model inside faces -Z like everything else.
        self.death_yaw += (self.death_yaw_target - self.death_yaw) * smoothing(14.0, dt);
        let yaw = frame.yaw + self.death_yaw;
        self.base_mut()
            .set_global_basis(Basis::from_axis_angle(Vector3::UP, yaw));

        // Locomotion, in the body's frame: +X right, -Z forward.
        let local = Basis::from_axis_angle(Vector3::UP, -frame.yaw) * frame.velocity;
        let flat = Vector3::new(local.x, 0.0, local.z);
        let speed = flat.length();
        let moving = speed > 0.4 && frame.alive;
        let forward = -flat.z;
        let right = flat.x;
        let mut leg_turn = 0.0;
        if moving && frame.stance != Stance::Sliding {
            let angle = right.atan2(forward);
            let cos = angle.cos();
            self.walking_backward = if self.walking_backward {
                cos < -0.1
            } else {
                cos < -0.35
            } && !frame.sprinting;
            let legs = if self.walking_backward {
                wrap_angle(angle - PI)
            } else {
                angle
            };
            leg_turn = legs.clamp(-MAX_LEG_TURN, MAX_LEG_TURN);
        } else if !moving {
            self.walking_backward = false;
        }
        // Positive turns the hips left; moving right turns them right.
        self.lower_yaw += (-leg_turn - self.lower_yaw) * smoothing(10.0, dt);

        let crouch_target = if frame.stance == Stance::Crouching {
            1.0
        } else {
            0.0
        };
        let slide_target = if frame.stance == Stance::Sliding {
            1.0
        } else {
            0.0
        };
        self.crouch += (crouch_target - self.crouch) * smoothing(12.0, dt);
        self.slide += (slide_target - self.slide) * smoothing(14.0, dt);
        self.air_time = if frame.on_floor {
            0.0
        } else {
            self.air_time + dt
        };
        let air_target = if self.air_time > 0.12 { 1.0 } else { 0.0 };
        self.air += (air_target - self.air) * smoothing(10.0, dt);

        let direction = if self.walking_backward { -1.0 } else { 1.0 };
        let stand_position = if !moving {
            0.0
        } else if frame.sprinting {
            speed.clamp(JOG_SPEED, SPRINT_SPEED)
        } else {
            speed.min(RUN_BLEND_LIMIT)
        };
        let stand_rate = if moving {
            (speed / stand_position.max(WALK_SPEED)).clamp(0.5, 1.8) * direction
        } else {
            1.0
        };
        let crouch_position = if moving { CROUCH_SPEED } else { 0.0 };
        let crouch_rate = if moving {
            (speed / CROUCH_SPEED).clamp(0.6, 2.2) * direction
        } else {
            1.0
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
        self.switch_drop = (self.switch_drop - dt * 3.0).max(0.0);
        self.kick.step(260.0, 22.0, dt);
        self.lean.step(LEAN_STIFFNESS, LEAN_DAMPING, dt);
        let ik_target = if frame.alive && self.weapon.is_some() {
            1.0
        } else {
            0.0
        };
        self.ik += (ik_target - self.ik) * smoothing(if frame.alive { 12.0 } else { 20.0 }, dt);

        let input = self.pose_input(frame);
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
                    speed.min(SPRINT_SPEED)
                };
                let rate = if moving {
                    (speed / position.max(ZOMBIE_WALK_SPEED)).clamp(0.5, 1.8)
                } else {
                    1.0
                };
                set(tree, "walk/blend_position", position.to_variant());
                set(tree, "walk_rate/scale", rate.to_variant());
            } else {
                set(tree, "stand/blend_position", stand_position.to_variant());
                set(tree, "stand_rate/scale", stand_rate.to_variant());
                set(tree, "crouch/blend_position", crouch_position.to_variant());
                set(tree, "crouch_rate/scale", crouch_rate.to_variant());
                set(tree, "stance/blend_amount", self.crouch.to_variant());
                set(tree, "slide/blend_amount", self.slide.to_variant());
                set(tree, "air/blend_amount", self.air.to_variant());
                let hold = if self.weapon.is_some() && frame.alive {
                    1.0f32
                } else {
                    0.0
                };
                set(tree, "hold/blend_amount", hold.to_variant());
            }
            // Only on a change: a request restarts the death clip.
            if self.life_requested != Some(frame.alive) {
                self.life_requested = Some(frame.alive);
                let life = if frame.alive { "alive" } else { "dead" };
                set(tree, "life/transition_request", life.to_variant());
            }
            tree.advance(f64::from(dt));
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

    /// Muzzle flash and a kick for one shot.
    pub fn fire(&mut self) {
        if let Some((_, view)) = &mut self.weapon {
            view.bind_mut().flash_world();
        }
        self.kick.velocity += Vector3::new(
            impulse_for_peak(4f32.to_radians(), 260.0, 22.0),
            0.0,
            impulse_for_peak(0.035, 260.0, 22.0),
        );
    }

    /// A hit travelling along `direction` (world): the torso flinches away and
    /// the body flashes. On a kill the body turns to fall away from the shot.
    pub fn hit(&mut self, direction: Vector3, killed: bool) {
        self.flash = if killed { 2.0 } else { 1.0 };
        if let Some(material) = &self.flash_material {
            for mesh in &mut self.meshes {
                mesh.set_material_overlay(material);
            }
        }
        let Some(away) = Vector3::new(direction.x, 0.0, direction.z).try_normalized() else {
            return;
        };
        if killed {
            // Death01 falls backward, so face the shooter: forward = -away.
            let face = away.x.atan2(away.z);
            self.death_yaw_target = wrap_angle(face - self.frame.yaw);
        } else {
            let local = Basis::from_axis_angle(Vector3::UP, -self.frame.yaw) * away;
            let peak = 7f32.to_radians();
            self.lean.velocity += local * impulse_for_peak(peak, LEAN_STIFFNESS, LEAN_DAMPING);
        }
    }

    fn pose_input(&self, frame: &SoldierFrame) -> PoseInput {
        let ads = smoothstep(frame.ads) * (1.0 - self.sprint);
        let sprint = smoothstep(self.sprint);
        let dip = (PI * self.reload_progress).sin().max(0.0) * self.reload_blend;
        let kick = self.kick.value;
        let mut offset = HIP_OFFSET.lerp(ADS_OFFSET, ads);
        offset += (SPRINT_OFFSET - HIP_OFFSET) * sprint;
        offset += Vector3::new(
            -0.04 * dip,
            -0.04 * dip - 0.25 * self.switch_drop,
            0.03 * dip,
        );
        offset.z += kick.z;
        let deg = |v: f32| v.to_radians();
        let hip = 1.0 - ads;
        let pitch =
            deg(HIP_ROTATION.x) * hip + deg(SPRINT_ROTATION.x) * sprint + kick.x + deg(12.0) * dip
                - deg(40.0) * self.switch_drop;
        let yaw = deg(HIP_ROTATION.y) * hip + deg(SPRINT_ROTATION.y) * sprint;
        let roll = deg(HIP_ROTATION.z) * hip + deg(SPRINT_ROTATION.z) * sprint + deg(30.0) * dip;
        let rotation = Basis::from_euler(EulerOrder::YXZ, Vector3::new(pitch, yaw, roll));

        // The support hand fetches a magazine in the middle of the reload.
        let p = self.reload_progress;
        let support_off = self.reload_blend
            * (smoothstep((p - 0.1) / 0.15) * (1.0 - smoothstep((p - 0.6) / 0.2)));
        let grips = self.weapon.as_ref().map(|(id, view)| {
            let mut g = view.bind().grips();
            g.pistol = id.def().slot == Slot::Secondary;
            g
        });
        PoseInput {
            lower_yaw: self.lower_yaw,
            pitch: frame.pitch.clamp(-1.3, 1.3) * (1.0 - 0.6 * self.slide),
            lean: self.lean.value,
            placement: WeaponPlacement::FromEyes { offset, rotation },
            grips,
            ik: self.ik,
            weapon_in_hand: !frame.alive,
            support_off,
            support_target: RELOAD_REACH,
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
        for child in skeleton.get_children().iter_shared() {
            if let Ok(mesh) = child.try_cast::<MeshInstance3D>() {
                recolour(mesh, team);
            }
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
        if let Some(poser) = &mut self.poser {
            poser.bind_mut().weapon = Some(view.clone().upcast());
        }
        self.weapon = Some((id, view));
        if had_weapon {
            self.switch_drop = 1.0;
        }
    }

    fn build_tree(&mut self) -> Gd<AnimationTree> {
        let mut tree = AnimationTree::new_alloc();
        tree.set_name("AnimationTree");
        for (name, path) in [
            ("ual1", "res://assets/characters/animations/ual1.glb"),
            ("ual2", "res://assets/characters/animations/ual2.glb"),
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
        let space = |points: &[(&str, f32)]| {
            let mut s = AnimationNodeBlendSpace1D::new_gd();
            s.set_min_space(0.0);
            s.set_max_space(SPRINT_SPEED);
            for (name, position) in points {
                let point = name.rsplit('/').next().unwrap_or(name);
                s.add_blend_point_ex(&clip(name).upcast::<AnimationRootNode>(), *position)
                    .name(point)
                    .done();
            }
            s
        };
        add(
            "stand",
            space(&[
                ("ual1/Idle", 0.0),
                ("ual1/Walk", WALK_SPEED),
                ("ual1/Jog_Fwd", JOG_SPEED),
                ("ual1/Sprint", SPRINT_SPEED),
            ])
            .upcast(),
            0.0,
        );
        add(
            "crouch",
            space(&[("ual1/Crouch_Idle", 0.0), ("ual1/Crouch_Fwd", CROUCH_SPEED)]).upcast(),
            0.0,
        );
        add("stand_rate", AnimationNodeTimeScale::new_gd().upcast(), 1.0);
        add(
            "crouch_rate",
            AnimationNodeTimeScale::new_gd().upcast(),
            1.0,
        );
        add("stance", AnimationNodeBlend2::new_gd().upcast(), 2.0);
        add("slide_clip", clip("ual2/Slide").upcast(), 2.0);
        add("slide", AnimationNodeBlend2::new_gd().upcast(), 3.0);
        add("jump_clip", clip("ual1/Jump").upcast(), 3.0);
        add("air", AnimationNodeBlend2::new_gd().upcast(), 4.0);
        add("hold_clip", clip("ual1/Pistol_Aim_Neutral").upcast(), 4.0);
        let mut hold = AnimationNodeBlend2::new_gd();
        hold.set_filter_enabled(true);
        for bone in arm_bones() {
            hold.set_filter_path(&NodePath::from(&format!("%GeneralSkeleton:{bone}")), true);
        }
        add("hold", hold.upcast(), 5.0);
        let mut death = clip("ual1/Death01");
        death.set_loop_mode(godot::classes::animation::LoopMode::NONE);
        add("death_clip", death.upcast(), 5.0);
        let mut life = AnimationNodeTransition::new_gd();
        life.set_input_count(2);
        life.set_input_name(0, "alive");
        life.set_input_name(1, "dead");
        life.set_input_reset(1, true);
        life.set_xfade_time(0.12);
        add("life", life.upcast(), 6.0);

        let links = [
            ("stand_rate", 0, "stand"),
            ("crouch_rate", 0, "crouch"),
            ("stance", 0, "stand_rate"),
            ("stance", 1, "crouch_rate"),
            ("slide", 0, "stance"),
            ("slide", 1, "slide_clip"),
            ("air", 0, "slide"),
            ("air", 1, "jump_clip"),
            ("hold", 0, "air"),
            ("hold", 1, "hold_clip"),
            ("life", 0, "hold"),
            ("life", 1, "death_clip"),
            ("output", 0, "life"),
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
/// with speed, a one-shot scratch attack, and the death fall.
fn zombie_graph() -> Gd<AnimationNodeBlendTree> {
    let mut graph = AnimationNodeBlendTree::new_gd();
    let clip = |name: &str| {
        let mut a = AnimationNodeAnimation::new_gd();
        a.set_animation(name);
        a
    };
    let mut walk = AnimationNodeBlendSpace1D::new_gd();
    walk.set_min_space(0.0);
    walk.set_max_space(SPRINT_SPEED);
    for (name, position) in [
        ("ual2/Zombie_Idle", 0.0),
        ("ual2/Zombie_Walk_Fwd", ZOMBIE_WALK_SPEED),
        ("ual1/Jog_Fwd", JOG_SPEED),
        ("ual1/Sprint", SPRINT_SPEED),
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
    let mut death = clip("ual1/Death01");
    death.set_loop_mode(godot::classes::animation::LoopMode::NONE);
    graph.add_node("death_clip", &death.upcast::<AnimationNode>());
    let mut life = AnimationNodeTransition::new_gd();
    life.set_input_count(2);
    life.set_input_name(0, "alive");
    life.set_input_name(1, "dead");
    life.set_input_reset(1, true);
    life.set_xfade_time(0.12);
    graph.add_node("life", &life.upcast::<AnimationNode>());
    for (to, port, from) in [
        ("walk_rate", 0, "walk"),
        ("attack", 0, "walk_rate"),
        ("attack", 1, "attack_clip"),
        ("life", 0, "attack"),
        ("life", 1, "death_clip"),
        ("output", 0, "life"),
    ] {
        graph.connect_node(to, port, from);
    }
    graph
}

/// Swaps each surface's imported material for the team's material of the same role.
fn recolour(mut mesh: Gd<MeshInstance3D>, team: Team) {
    let Some(source) = mesh.get_mesh() else {
        return;
    };
    for surface in 0..source.get_surface_count() {
        let Some(material) = source.surface_get_material(surface) else {
            continue;
        };
        let role = material.get_name().to_string();
        if let Some(team_material) = palette::material(team, &role) {
            mesh.set_surface_override_material(surface, &team_material);
        }
    }
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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
    }
    grips
}
