//! Shooting-range target dummy: takes damage, flinches away from hits, topples
//! away from the killing shot, and respawns.

use godot::classes::{IStaticBody3D, Label3D, MeshInstance3D, StandardMaterial3D, StaticBody3D};
use godot::prelude::*;

use crate::aim_assist::AimAssistTarget;
use crate::spring::{Spring, impulse_for_peak};

/// Hit flinch spring (tilt in radians): loose enough to wobble once.
const LEAN_STIFFNESS: f32 = 140.0;
const LEAN_DAMPING: f32 = 9.0;

/// Collision layer 3 ("hitboxes"), as a bit mask.
pub const HITBOX_LAYER: u32 = 1 << 2;
/// Collision layer 1 ("world").
pub const WORLD_LAYER: u32 = 1;

#[derive(GodotClass)]
#[class(init, base = StaticBody3D)]
pub struct TargetDummy {
    base: Base<StaticBody3D>,

    #[export]
    #[init(val = 100.0)]
    max_health: f32,
    #[export]
    #[init(val = 3.0)]
    respawn_delay: f32,

    #[init(node = "HealthLabel")]
    label: OnReady<Gd<Label3D>>,
    /// Pivot at the feet; rotating it topples the dummy.
    #[init(node = "Visual")]
    visual: OnReady<Gd<Node3D>>,
    #[init(node = "Visual/BodyMesh")]
    body_mesh: OnReady<Gd<MeshInstance3D>>,
    #[init(node = "Visual/HeadMesh")]
    head_mesh: OnReady<Gd<MeshInstance3D>>,
    #[init(node = "Head")]
    head: OnReady<Gd<StaticBody3D>>,

    flash_material: Option<Gd<StandardMaterial3D>>,
    health: f32,
    flash: f32,
    respawn_in: f32,
    fall: f32,
    /// Horizontal direction, in the dummy's space, that the dummy topples toward.
    topple: Vector3,
    /// Hit flinch: a horizontal vector whose length is the tilt angle.
    lean: Spring,
    /// Kill pop, 1 right after the kill and decaying to 0.
    pop: f32,
}

#[godot_api]
impl IStaticBody3D for TargetDummy {
    fn ready(&mut self) {
        self.health = self.max_health;
        self.topple = Vector3::FORWARD;
        // Each dummy flashes on its own, so it needs its own copy of the material.
        let material = self
            .body_mesh
            .get_active_material(0)
            .map(|m| m.duplicate_resource())
            .and_then(|m| m.try_cast::<StandardMaterial3D>().ok());
        if let Some(material) = &material {
            let mut material = material.clone();
            material.set_feature(godot::classes::base_material_3d::Feature::EMISSION, true);
            material.set_emission(Color::from_rgb(1.0, 0.25, 0.1));
            material.set_emission_energy_multiplier(0.0);
            self.body_mesh.set_material_override(&material);
            self.head_mesh.set_material_override(&material);
        }
        self.flash_material = material;
        self.update_label();
    }

    fn process(&mut self, delta: f64) {
        let delta = delta as f32;
        if self.flash > 0.0 {
            self.flash = (self.flash - delta * 6.0).max(0.0);
            if let Some(material) = &mut self.flash_material {
                material.set_emission_energy_multiplier(self.flash * 3.0);
            }
        }

        let dead = self.health <= 0.0;
        let target_fall = if dead { 1.0 } else { 0.0 };
        self.fall += (target_fall - self.fall) * (1.0 - (-10.0 * delta).exp());
        self.lean.step(LEAN_STIFFNESS, LEAN_DAMPING, delta);
        self.pop = (self.pop - delta * 5.0).max(0.0);

        // Tilt the top toward `tilt`; its length is the angle.
        let tilt = self.topple * (self.fall * std::f32::consts::FRAC_PI_2) + self.lean.value;
        let angle = tilt.length();
        let basis = if angle > 1e-4 {
            Basis::from_axis_angle(Vector3::UP.cross(tilt / angle), angle)
        } else {
            Basis::IDENTITY
        };
        let scale = 1.0 + 0.12 * self.pop * self.pop;
        self.visual.set_basis(basis.scaled(Vector3::splat(scale)));

        if dead {
            self.respawn_in -= delta;
            if self.respawn_in <= 0.0 {
                self.respawn();
            }
        }
    }
}

#[godot_api]
impl TargetDummy {
    /// Applies damage and returns true if this hit killed the dummy.
    #[func]
    pub fn apply_damage(&mut self, amount: f32, _headshot: bool) -> bool {
        if self.health <= 0.0 {
            return false;
        }
        self.health -= amount;
        self.flash = 1.0;
        let killed = self.health <= 0.0;
        if killed {
            self.health = 0.0;
            self.respawn_in = self.respawn_delay;
            self.set_hittable(false);
        }
        self.update_label();
        killed
    }

    /// Visual reaction to a hit from a shot travelling along `direction` (world
    /// space): a flinch, or on a kill, a pop and a fall away from the shooter.
    #[func]
    pub fn react_to_hit(&mut self, direction: Vector3, killed: bool) {
        let local = self.base().get_global_basis().inverse() * direction;
        let Some(away) = Vector3::new(local.x, 0.0, local.z).try_normalized() else {
            return;
        };
        if killed {
            // Still getting up from the last kill: turn the fall toward the new
            // direction instead of snapping to it.
            self.topple = (self.topple * self.fall + away * (1.0 - self.fall))
                .try_normalized()
                .unwrap_or(away);
            self.pop = 1.0;
            self.flash = 2.0;
        } else {
            let peak = 7f32.to_radians();
            self.lean.velocity += away * impulse_for_peak(peak, LEAN_STIFFNESS, LEAN_DAMPING);
        }
    }

    #[func]
    pub fn health(&self) -> f32 {
        self.health
    }
}

impl TargetDummy {
    fn respawn(&mut self) {
        self.health = self.max_health;
        self.set_hittable(true);
        self.update_label();
    }

    fn set_hittable(&mut self, hittable: bool) {
        self.base_mut().set_collision_layer(if hittable {
            WORLD_LAYER | HITBOX_LAYER
        } else {
            0
        });
        self.head
            .set_collision_layer(if hittable { HITBOX_LAYER } else { 0 });
        self.label.set_visible(hittable);
        if let Some(mut assist) = self
            .base()
            .get_node_or_null("AimAssist")
            .and_then(|n| n.try_cast::<AimAssistTarget>().ok())
        {
            assist.bind_mut().set_active(hittable);
        }
    }

    fn update_label(&mut self) {
        let text = format!("{:.0}", self.health.max(0.0));
        self.label.set_text(&text);
    }
}
