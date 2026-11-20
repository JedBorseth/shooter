//! `HitboxRig`: one body's hit zones as physics shapes, posed every frame from
//! `shooter_sim::hitbox`. Each zone is a `StaticBody3D` on the hitboxes layer
//! with `hit_zone` metadata, so hitscan finds the zone from whatever it hits and
//! walks up to the owner's `apply_damage`.
//!
//! F4 (`debug_hitboxes`) draws every rig's shapes over the characters, coloured
//! by zone, to check the animated model against what the simulation hits. Only
//! debug builds can turn it on, and the shapes are depth tested, so it never
//! shows other players through walls.
//!
//! Posing is lazy: `set_pose` only records the pose, and the shapes move when
//! something is about to test against them ([`sync_all`], called by every
//! hitscan, melee, and blast query), or at least ten times a second for the
//! rays that only look (the lens flare, eye adaptation). Placing every
//! remote player's shapes each frame cost about 3 ms of a 6v6 frame.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use godot::classes::base_material_3d::{ShadingMode, Transparency};
use godot::classes::physics_server_3d::BodyState;
use godot::classes::{
    CapsuleMesh, CapsuleShape3D, CollisionShape3D, Engine, INode3D, Input, MeshInstance3D,
    PhysicsServer3D, SphereMesh, SphereShape3D, StandardMaterial3D, StaticBody3D,
};
use godot::prelude::*;
use shooter_sim::hitbox::{self, HitZone, Hitbox, HitboxPose};

use crate::convert::to_godot;

/// Collision layer 3 ("hitboxes"), as a bit mask.
pub const HITBOX_LAYER: u32 = 1 << 2;

static DEBUG_DRAW: AtomicBool = AtomicBool::new(false);

/// Longest a rig's shapes lag its pose when nothing tests against them.
const IDLE_SYNC_MSEC: u64 = 100;

thread_local! {
    /// Rigs whose pose changed since their shapes were last placed.
    static PENDING: RefCell<Vec<InstanceId>> = const { RefCell::new(Vec::new()) };
}

/// Places the shapes of every rig whose pose changed. Call it before any
/// query that can hit the hitboxes layer.
pub fn sync_all() {
    let pending = PENDING.with_borrow_mut(std::mem::take);
    for id in pending {
        if let Ok(mut rig) = Gd::<HitboxRig>::try_from_instance_id(id) {
            let mut rig = rig.bind_mut();
            rig.queued = false;
            rig.place_now();
        }
    }
}
static LAST_TOGGLE_FRAME: AtomicU64 = AtomicU64::new(u64::MAX);

/// Whether hitboxes are drawn.
pub fn debug_draw() -> bool {
    DEBUG_DRAW.load(Ordering::Relaxed)
}

pub fn set_debug_draw(on: bool) {
    DEBUG_DRAW.store(on, Ordering::Relaxed);
}

fn zone_colour(zone: HitZone) -> Color {
    match zone {
        HitZone::Head => Color::from_rgba(1.0, 0.15, 0.1, 0.45),
        HitZone::Chest => Color::from_rgba(1.0, 0.65, 0.1, 0.35),
        HitZone::Stomach => Color::from_rgba(1.0, 0.9, 0.2, 0.35),
        HitZone::Arm => Color::from_rgba(0.2, 0.8, 1.0, 0.35),
        HitZone::Leg => Color::from_rgba(0.3, 1.0, 0.4, 0.35),
    }
}

struct Part {
    body: Gd<StaticBody3D>,
    shape: Option<Gd<CapsuleShape3D>>,
    mesh: Gd<MeshInstance3D>,
    capsule_mesh: Option<Gd<CapsuleMesh>>,
}

#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct HitboxRig {
    base: Base<Node3D>,

    /// When false the shapes are only drawn, never hit (the server's rewound
    /// hitboxes in the F4 overlay).
    #[export]
    #[init(val = true)]
    collide: bool,
    /// One colour for every zone; fully transparent (the default) colours
    /// each zone on its own. Set before the rig enters the tree.
    #[export]
    tint: Color,

    parts: Vec<Part>,
    pose: Option<HitboxPose>,
    drawn: bool,
    /// The pose changed since the shapes were placed.
    dirty: bool,
    /// Listed in `PENDING`.
    queued: bool,
    /// When the shapes were last placed (engine milliseconds).
    placed_at: u64,
}

#[godot_api]
impl INode3D for HitboxRig {
    fn ready(&mut self) {
        let standing = hitbox::hitboxes(&HitboxPose::new(
            shooter_sim::Vec3::ZERO,
            0.0,
            0.0,
            shooter_sim::movement::Stance::Standing,
        ));
        for (i, hb) in standing.iter().enumerate() {
            let part = self.make_part(i, hb);
            self.parts.push(part);
        }
        self.set_enabled(false);
    }

    fn process(&mut self, _delta: f64) {
        let _profile = crate::profile::scope("hitbox rigs");
        // One toggle per frame, however many rigs see the key press.
        if godot::classes::Os::singleton().is_debug_build()
            && Input::singleton().is_action_just_pressed("debug_hitboxes")
        {
            let frame = Engine::singleton().get_process_frames();
            if LAST_TOGGLE_FRAME.swap(frame, Ordering::Relaxed) != frame {
                set_debug_draw(!debug_draw());
            }
        }
        let draw = debug_draw() && self.pose.is_some();
        if draw != self.drawn {
            self.drawn = draw;
            for part in &mut self.parts {
                part.mesh.set_visible(draw);
            }
        }
        if self.dirty
            && (self.drawn
                || godot::classes::Time::singleton().get_ticks_msec()
                    >= self.placed_at + IDLE_SYNC_MSEC)
        {
            self.place_now();
        }
    }
}

#[godot_api]
impl HitboxRig {
    /// Zone names and world-space segments, for tests: `[zone, a, b, radius]` each.
    #[func]
    fn shapes(&self) -> VarArray {
        let mut out = VarArray::new();
        if let Some(pose) = &self.pose {
            for hb in hitbox::hitboxes(pose) {
                out.push(
                    &varray![hb.zone.name(), to_godot(hb.a), to_godot(hb.b), hb.radius]
                        .to_variant(),
                );
            }
        }
        out
    }

    /// Where a third-person soldier's support hand holds the weapon `key`
    /// (weapon space; `carry::Grips::held`), before its palm is rested on the
    /// weapon's section: where `tests/bake_support_sections.gd` cuts the
    /// section. Zero for an unknown weapon or a one-handed hold.
    #[func]
    fn support_grip(key: GString) -> Vector3 {
        use shooter_sim::hitbox::carry::{Grips, Hold};
        shooter_sim::weapon::WeaponId::by_key(&key.to_string())
            .map(|id| Grips::held(id.def()))
            .filter(|g| g.hold != Hold::Pistol)
            .map_or(Vector3::ZERO, |g| to_godot(g.left))
    }

    #[func]
    fn set_debug_visible(on: bool) {
        set_debug_draw(on);
    }
}

impl HitboxRig {
    /// Draw-only, in one colour. Call before the rig enters the tree.
    pub fn make_ghost(&mut self, tint: Color) {
        self.collide = false;
        self.tint = tint;
    }

    /// Poses the shapes, or turns them off with `None` (dead, despawned).
    /// The shapes move on the next [`sync_all`] (or within a tenth of a
    /// second); a rig that turns on is placed at once.
    pub fn set_pose(&mut self, pose: Option<HitboxPose>) {
        let was_enabled = self.pose.is_some();
        self.pose = pose;
        if pose.is_none() {
            self.dirty = false;
            if was_enabled {
                self.set_enabled(false);
            }
            return;
        }
        self.dirty = true;
        if !was_enabled {
            self.place_now();
            self.set_enabled(true);
            return;
        }
        if !self.queued {
            self.queued = true;
            let id = self.base().instance_id();
            PENDING.with_borrow_mut(|pending| pending.push(id));
        }
    }

    /// Places the shapes at the current pose, if it changed.
    fn place_now(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        self.placed_at = godot::classes::Time::singleton().get_ticks_msec();
        let Some(pose) = self.pose else {
            return;
        };
        let _profile = crate::profile::scope("hitbox placement");
        for (part, hb) in self.parts.iter_mut().zip(hitbox::hitboxes(&pose)) {
            place(part, &hb);
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        let layer = if enabled && self.collide {
            HITBOX_LAYER
        } else {
            0
        };
        for part in &mut self.parts {
            part.body.set_collision_layer(layer);
        }
    }

    fn make_part(&mut self, index: usize, hb: &Hitbox) -> Part {
        let mut body = StaticBody3D::new_alloc();
        body.set_name(&format!("{}_{index}", hb.zone.name()));
        body.set_as_top_level(true);
        body.set_collision_layer(0);
        body.set_collision_mask(0);
        body.set_meta("hit_zone", &hb.zone.name().to_variant());
        // Which shape this is, for the hit reaction and the fall.
        body.set_meta("hit_part", &(index as i64).to_variant());
        body.set_meta("surface", &"flesh".to_variant());

        let mut collision = CollisionShape3D::new_alloc();
        let mut mesh = MeshInstance3D::new_alloc();
        let mut material = StandardMaterial3D::new_gd();
        material.set_shading_mode(ShadingMode::UNSHADED);
        material.set_transparency(Transparency::ALPHA);
        material.set_flag(
            godot::classes::base_material_3d::Flags::DISABLE_DEPTH_TEST,
            true,
        );
        let colour = if self.tint.a > 0.0 {
            self.tint
        } else {
            zone_colour(hb.zone)
        };
        material.set_albedo(colour);
        mesh.set_cast_shadows_setting(
            godot::classes::geometry_instance_3d::ShadowCastingSetting::OFF,
        );
        mesh.set_visible(false);

        let sphere = hb.a == hb.b;
        let (shape, capsule_mesh) = if sphere {
            let mut s = SphereShape3D::new_gd();
            s.set_radius(hb.radius);
            collision.set_shape(&s);
            let mut m = SphereMesh::new_gd();
            m.set_radius(hb.radius);
            m.set_height(hb.radius * 2.0);
            m.set_radial_segments(16);
            m.set_rings(8);
            m.set_material(&material);
            mesh.set_mesh(&m);
            (None, None)
        } else {
            let mut s = CapsuleShape3D::new_gd();
            s.set_radius(hb.radius);
            collision.set_shape(&s);
            let mut m = CapsuleMesh::new_gd();
            m.set_radius(hb.radius);
            m.set_radial_segments(12);
            m.set_rings(4);
            m.set_material(&material);
            mesh.set_mesh(&m);
            (Some(s), Some(m))
        };
        body.add_child(&collision);
        body.add_child(&mesh);
        self.base_mut().add_child(&body);
        Part {
            body,
            shape,
            mesh,
            capsule_mesh,
        }
    }
}

fn place(part: &mut Part, hb: &Hitbox) {
    let a = to_godot(hb.a);
    let b = to_godot(hb.b);
    let mid = (a + b) * 0.5;
    let axis = b - a;
    let length = axis.length();
    let basis = match axis.try_normalized() {
        Some(up) => crate::character::ik::basis_from_y_z(up, Vector3::BACK),
        None => Basis::IDENTITY,
    };
    if let Some(shape) = &mut part.shape {
        let height = length + 2.0 * hb.radius;
        if (shape.get_height() - height).abs() > 1e-4 {
            shape.set_height(height);
            if let Some(m) = &mut part.capsule_mesh {
                m.set_height(height);
            }
        }
    }
    let transform = Transform3D::new(basis, mid);
    part.body.set_global_transform(transform);
    // The node hands its transform to the physics server only when the tree
    // flushes transform changes (before the next physics step); a query this
    // tick must already see the shape where it is.
    PhysicsServer3D::singleton().body_set_state(
        part.body.get_rid(),
        BodyState::TRANSFORM,
        &transform.to_variant(),
    );
}
