//! `CorpseYard`: where dead bodies and dropped guns stay after a death, so a
//! player who respawns gets a fresh body while the old one lies where it
//! fell, as in Call of Duty.
//!
//! One yard per scene, made on first use. It steps each corpse's ragdoll
//! until it freezes, and after a corpse's time (or once more than
//! [`MAX_CORPSES`] lie around) sinks it into the ground and frees it. Dropped
//! guns are rigid bodies on the ragdoll layer, freed after
//! [`WEAPON_SECONDS`], at most [`MAX_WEAPONS`] at once.
//!
//! Only clients draw characters, so a dedicated server never makes one.

use std::cell::RefCell;
use std::collections::VecDeque;

use godot::classes::rigid_body_3d::CenterOfMassMode;
use godot::classes::{
    BoxShape3D, CollisionShape3D, INode3D, MeshInstance3D, Node3D, PhysicsMaterial, RigidBody3D,
    SceneTree,
};
use godot::prelude::*;

use super::ragdoll::RAGDOLL_LAYER;
use super::rig::SoldierRig;

/// Bodies lying at once; the oldest sinks to make room.
pub const MAX_CORPSES: usize = 10;
/// Seconds a soldier's body lies before it sinks away.
pub const SOLDIER_SECONDS: f32 = 30.0;
/// Seconds a zombie's body lies: a horde leaves many.
pub const ZOMBIE_SECONDS: f32 = 10.0;
/// A body sinks this far over this long, then is freed.
const SINK_DEPTH: f32 = 0.6;
const SINK_SECONDS: f32 = 1.5;
/// Dropped guns: how many and for how long.
pub const MAX_WEAPONS: usize = 8;
pub const WEAPON_SECONDS: f32 = 15.0;
const WEAPON_MASS: f32 = 3.5;

const GROUP: &str = "corpse_yard";

thread_local! {
    /// Guns let go of and waiting for the yard to take them (a corpse the
    /// yard is stepping drops its gun while the yard is busy).
    static DROPS: RefCell<Vec<(Gd<Node3D>, Vector3, Vector3)>> = const { RefCell::new(Vec::new()) };
}

/// Lets go of a held gun: the scene's yard picks it up on its next frame, and
/// it falls as a rigid body moving at `velocity` and turning at `spin`
/// (radians a second).
pub fn drop_weapon(tree: &Gd<SceneTree>, view: Gd<Node3D>, velocity: Vector3, spin: Vector3) {
    CorpseYard::of(tree);
    DROPS.with_borrow_mut(|drops| drops.push((view, velocity, spin)));
}

struct Corpse {
    rig: Gd<SoldierRig>,
    age: f32,
    lifetime: f32,
    sinking: Option<f32>,
}

struct Dropped {
    body: Gd<RigidBody3D>,
    age: f32,
}

#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct CorpseYard {
    base: Base<Node3D>,
    corpses: VecDeque<Corpse>,
    weapons: VecDeque<Dropped>,
}

#[godot_api]
impl INode3D for CorpseYard {
    fn process(&mut self, delta: f64) {
        let dt = delta as f32;
        self.corpses.retain(|c| c.rig.is_instance_valid());
        // Make room: the oldest bodies sink first.
        let lying = self.corpses.iter().filter(|c| c.sinking.is_none()).count();
        let mut excess = lying.saturating_sub(MAX_CORPSES);
        for corpse in self.corpses.iter_mut() {
            if excess == 0 {
                break;
            }
            if corpse.sinking.is_none() {
                corpse.sinking = Some(0.0);
                excess -= 1;
            }
        }
        for corpse in self.corpses.iter_mut() {
            corpse.age += dt;
            if corpse.sinking.is_none() && corpse.age >= corpse.lifetime {
                corpse.sinking = Some(0.0);
            }
            match &mut corpse.sinking {
                None => corpse.rig.bind_mut().step_corpse(dt),
                Some(t) => {
                    if *t == 0.0 {
                        corpse.rig.bind_mut().freeze_ragdoll();
                    }
                    *t += dt;
                    let step = SINK_DEPTH * dt / SINK_SECONDS;
                    let position = corpse.rig.get_global_position() + Vector3::DOWN * step;
                    corpse.rig.set_global_position(position);
                    if *t >= SINK_SECONDS {
                        corpse.rig.queue_free();
                    }
                }
            }
        }
        self.corpses
            .retain(|c| c.sinking.is_none_or(|t| t < SINK_SECONDS));

        for (view, velocity, spin) in DROPS.with_borrow_mut(std::mem::take) {
            if view.is_instance_valid() {
                self.take_weapon(view, velocity, spin);
            }
        }
        self.weapons.retain(|w| w.body.is_instance_valid());
        while self.weapons.len() > MAX_WEAPONS {
            if let Some(mut old) = self.weapons.pop_front() {
                old.body.queue_free();
            }
        }
        for dropped in self.weapons.iter_mut() {
            dropped.age += dt;
            if dropped.age >= WEAPON_SECONDS {
                dropped.body.queue_free();
            }
        }
        self.weapons.retain(|w| w.age < WEAPON_SECONDS);
    }
}

#[godot_api]
impl CorpseYard {
    /// Bodies lying (tests).
    #[func]
    pub fn corpse_count(&self) -> i64 {
        self.corpses.len() as i64
    }

    /// Guns lying (tests).
    #[func]
    pub fn weapon_count(&self) -> i64 {
        self.weapons.len() as i64
    }

    /// The bodies, oldest first (tests).
    #[func]
    pub fn corpses(&self) -> Array<Gd<SoldierRig>> {
        self.corpses.iter().map(|c| c.rig.clone()).collect()
    }

    /// Ragdolls moving now, across the scene (tests).
    #[func]
    pub fn simulating() -> i64 {
        super::ragdoll::simulating() as i64
    }
}

impl CorpseYard {
    /// The scene's yard, made on first use.
    pub fn of(tree: &Gd<SceneTree>) -> Gd<CorpseYard> {
        if let Some(node) = tree.get_first_node_in_group(GROUP)
            && let Ok(yard) = node.try_cast::<CorpseYard>()
        {
            return yard;
        }
        let mut yard = CorpseYard::new_alloc();
        yard.set_name("CorpseYard");
        yard.add_to_group(GROUP);
        let mut parent = tree
            .get_current_scene()
            .unwrap_or_else(|| tree.get_root().upcast());
        parent.add_child(&yard);
        yard
    }

    /// Takes a dying body: it stays where it fell for `lifetime` seconds.
    pub fn adopt(&mut self, mut rig: Gd<SoldierRig>, lifetime: f32) {
        let yard = self.to_gd();
        if rig.get_parent().is_some() {
            rig.reparent_ex(&yard).keep_global_transform(true).done();
        } else {
            self.base_mut().add_child(&rig);
        }
        self.corpses.push_back(Corpse {
            rig,
            age: 0.0,
            lifetime,
            sinking: None,
        });
    }

    /// Takes a body back out of the yard and frees it (a death this client
    /// predicted that the server did not confirm).
    pub fn release(&mut self, mut rig: Gd<SoldierRig>) {
        let id = rig.instance_id();
        self.corpses.retain(|c| c.rig.instance_id() != id);
        rig.bind_mut().freeze_ragdoll();
        rig.queue_free();
    }

    fn take_weapon(&mut self, mut view: Gd<Node3D>, velocity: Vector3, spin: Vector3) {
        let place = view.get_global_transform();
        let bounds = shown_bounds(&view);
        let mut body = RigidBody3D::new_alloc();
        body.set_name("DroppedWeapon");
        body.set_collision_layer(RAGDOLL_LAYER);
        body.set_collision_mask(1);
        body.set_mass(WEAPON_MASS);
        body.set_angular_damp(1.0);
        body.set_center_of_mass_mode(CenterOfMassMode::CUSTOM);
        body.set_center_of_mass(bounds.center());
        let mut material = PhysicsMaterial::new_gd();
        material.set_friction(0.8);
        material.set_bounce(0.1);
        body.set_physics_material_override(&material);
        let mut shape = BoxShape3D::new_gd();
        shape.set_size(bounds.size.coord_max(Vector3::splat(0.03)));
        let mut collision = CollisionShape3D::new_alloc();
        collision.set_shape(&shape);
        collision.set_position(bounds.center());
        body.add_child(&collision);
        self.base_mut().add_child(&body);
        body.set_global_transform(place);
        if view.get_parent().is_some() {
            view.reparent_ex(&body).keep_global_transform(true).done();
        } else {
            body.add_child(&view);
        }
        body.set_linear_velocity(velocity);
        body.set_angular_velocity(spin);
        self.weapons.push_back(Dropped { body, age: 0.0 });
    }
}

/// The bounds of a node's shown meshes, in its own space.
fn shown_bounds(root: &Gd<Node3D>) -> Aabb {
    let inverse = root.get_global_transform().affine_inverse();
    let mut bounds: Option<Aabb> = None;
    for node in root
        .find_children_ex("*")
        .type_("MeshInstance3D")
        .owned(false)
        .done()
        .iter_shared()
    {
        let Ok(mesh) = node.try_cast::<MeshInstance3D>() else {
            continue;
        };
        if !mesh.is_visible_in_tree() && root.is_visible_in_tree() {
            continue;
        }
        let local = inverse * mesh.get_global_transform();
        let aabb = local * mesh.get_aabb();
        bounds = Some(bounds.map_or(aabb, |b| b.merge(aabb)));
    }
    bounds.unwrap_or(Aabb::new(
        Vector3::new(-0.03, -0.1, -0.5),
        Vector3::new(0.06, 0.2, 0.7),
    ))
}
