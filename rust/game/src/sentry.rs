//! Training sentry: turns toward the player, fires short bursts when it has
//! line of sight, and can be shot down. It exists so damage feedback (vignette,
//! direction indicators, regeneration, death and respawn) can be tested before
//! multiplayer.

use godot::classes::{
    AudioStreamPlayer3D, INode3D, MeshInstance3D, Node3D, OmniLight3D, PackedScene,
    PhysicsRayQueryParameters3D, StandardMaterial3D, StaticBody3D,
};
use godot::prelude::*;
use shooter_sim::rng;

use crate::target::{HITBOX_LAYER, WORLD_LAYER};

/// Collision layer 2 ("players"), as a bit mask.
const PLAYER_LAYER: u32 = 1 << 1;

#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct Sentry {
    base: Base<Node3D>,

    #[export]
    #[init(val = true)]
    enabled: bool,
    #[export]
    #[init(val = 40.0)]
    range: f32,
    #[export]
    #[init(val = 12.0)]
    damage: f32,
    #[export]
    #[init(val = 150.0)]
    max_health: f32,
    /// Seconds between seeing the player and the first burst.
    #[export]
    #[init(val = 0.7)]
    reaction_time: f32,
    #[export]
    #[init(val = 1.6)]
    burst_interval: f32,
    /// Spread cone half-angle in degrees.
    #[export]
    #[init(val = 2.5)]
    spread: f32,
    /// Degrees per second the head turns.
    #[export]
    #[init(val = 120.0)]
    turn_rate: f32,
    #[export]
    #[init(val = 10.0)]
    repair_time: f32,
    #[export]
    tracer_scene: OnEditor<Gd<PackedScene>>,

    #[init(node = "Head")]
    head: OnReady<Gd<Node3D>>,
    #[init(node = "Head/Muzzle")]
    muzzle: OnReady<Gd<Node3D>>,
    #[init(node = "Head/Muzzle/Flash")]
    flash: OnReady<Gd<OmniLight3D>>,
    /// The tracking eye's model (its mesh somewhere under this node).
    #[init(node = "Head/Eye")]
    eye: OnReady<Gd<Node3D>>,
    #[init(node = "FireSound")]
    fire_sound: OnReady<Gd<AudioStreamPlayer3D>>,
    #[init(node = "Hitbox")]
    hitbox: OnReady<Gd<StaticBody3D>>,

    eye_material: Option<Gd<StandardMaterial3D>>,
    health: f32,
    seen_for: f32,
    cooldown: f32,
    burst_left: u8,
    burst_timer: f32,
    shots: u32,
    flash_time: f32,
    down_for: f32,
    yaw: f32,
    pitch: f32,
}

#[godot_api]
impl INode3D for Sentry {
    fn ready(&mut self) {
        self.health = self.max_health;
        self.flash.set_visible(false);
        // The eye is a model: its first mesh gets its own copy of the
        // material, whose glow `set_eye` turns up as the sentry acquires.
        let mut mesh = self
            .eye
            .find_children_ex("*")
            .type_("MeshInstance3D")
            .done()
            .iter_shared()
            .find_map(|node| node.try_cast::<MeshInstance3D>().ok());
        let material = mesh
            .as_ref()
            .and_then(|mesh| mesh.get_active_material(0))
            .map(|m| m.duplicate_resource())
            .and_then(|m| m.try_cast::<StandardMaterial3D>().ok());
        if let (Some(mesh), Some(material)) = (&mut mesh, &material) {
            mesh.set_material_override(material);
        }
        self.eye_material = material;
    }

    fn physics_process(&mut self, delta: f64) {
        let _profile = crate::profile::scope("sentry");
        let dt = delta as f32;
        self.flash_time -= dt;
        if self.flash_time <= 0.0 {
            self.flash.set_visible(false);
        }
        if self.down_for > 0.0 {
            self.down_for -= dt;
            // Slump forward while broken, then come back online.
            self.pitch += (-0.6 - self.pitch) * (1.0 - (-6.0 * dt).exp());
            self.apply_head();
            if self.down_for <= 0.0 {
                self.health = self.max_health;
                self.hitbox.set_collision_layer(HITBOX_LAYER);
            }
            self.set_eye(0.0);
            return;
        }
        if !self.enabled {
            self.set_eye(0.0);
            return;
        }

        let target = self.visible_player();
        let Some((player, aim_point)) = target else {
            self.seen_for = 0.0;
            self.burst_left = 0;
            self.set_eye(0.4);
            return;
        };
        self.seen_for += dt;
        self.set_eye(if self.seen_for >= self.reaction_time {
            4.0
        } else {
            1.5
        });

        // Turn the head toward the target at a limited rate.
        let from = self.head.get_global_position();
        let to = aim_point - from;
        let want_yaw = (-to.x).atan2(-to.z);
        let want_pitch = to.y.atan2(Vector2::new(to.x, to.z).length());
        let max_turn = self.turn_rate.to_radians() * dt;
        self.yaw += wrap_angle(want_yaw - self.yaw).clamp(-max_turn, max_turn);
        self.pitch += (want_pitch - self.pitch).clamp(-max_turn, max_turn);
        self.apply_head();
        let aligned = wrap_angle(want_yaw - self.yaw).abs() < 4f32.to_radians();

        self.cooldown -= dt;
        if self.burst_left == 0
            && self.cooldown <= 0.0
            && aligned
            && self.seen_for >= self.reaction_time
        {
            self.burst_left = 3;
            self.burst_timer = 0.0;
            self.cooldown = self.burst_interval;
        }
        if self.burst_left > 0 {
            self.burst_timer -= dt;
            if self.burst_timer <= 0.0 {
                self.burst_left -= 1;
                self.burst_timer = 0.11;
                self.fire(player);
            }
        }
    }
}

#[godot_api]
impl Sentry {
    /// Takes a hit from the player. Returns true if this shot destroyed it.
    #[func]
    fn apply_damage(&mut self, amount: f32, _headshot: bool) -> bool {
        if self.down_for > 0.0 {
            return false;
        }
        self.health -= amount;
        if self.health <= 0.0 {
            self.down_for = self.repair_time;
            self.burst_left = 0;
            self.hitbox.set_collision_layer(0);
            return true;
        }
        false
    }

    #[func]
    fn health(&self) -> f32 {
        self.health.max(0.0)
    }
}

impl Sentry {
    fn apply_head(&mut self) {
        self.head
            .set_rotation(Vector3::new(self.pitch, self.yaw, 0.0));
    }

    fn set_eye(&mut self, energy: f32) {
        if let Some(material) = &mut self.eye_material {
            material.set_emission_energy_multiplier(energy);
        }
    }

    /// The closest living player in range and in sight, with the point to aim at.
    fn visible_player(&mut self) -> Option<(Gd<Node3D>, Vector3)> {
        let tree = self.base().get_tree();
        let from = self.muzzle.get_global_position();
        let mut best: Option<(Gd<Node3D>, Vector3, f32)> = None;
        for node in tree.get_nodes_in_group("players").iter_shared() {
            let Ok(mut player) = node.try_cast::<Node3D>() else {
                continue;
            };
            if !player
                .call("is_alive", &[])
                .try_to::<bool>()
                .unwrap_or(false)
            {
                continue;
            }
            let chest = player.get_global_position() + Vector3::UP * 1.2;
            let distance = from.distance_to(chest);
            if distance > self.range || best.as_ref().is_some_and(|b| b.2 < distance) {
                continue;
            }
            let hit = self.ray(from, chest, WORLD_LAYER | PLAYER_LAYER);
            let sees = hit
                .as_ref()
                .and_then(|(_, _, collider)| collider.clone())
                .is_some_and(|c| c.instance_id() == player.instance_id());
            if sees {
                best = Some((player, chest, distance));
            }
        }
        best.map(|(player, point, _)| (player, point))
    }

    fn fire(&mut self, player: Gd<Node3D>) {
        let mut player = player;
        self.shots = self.shots.wrapping_add(1);
        let seed = self.base().instance_id().to_i64() as u32;
        let from = self.muzzle.get_global_position();
        let forward = -self.muzzle.get_global_basis().col_c();
        let cone = self.spread.to_radians();
        let angle = rng::unit(seed, self.shots * 2) * std::f32::consts::TAU;
        let radius = rng::unit(seed, self.shots * 2 + 1).sqrt() * cone;
        let basis = self.muzzle.get_global_basis();
        let direction = (forward
            + basis.col_a() * (angle.cos() * radius)
            + basis.col_b() * (angle.sin() * radius))
            .normalized();
        let to = from + direction * self.range;

        let hit = self.ray(from, to, WORLD_LAYER | PLAYER_LAYER | HITBOX_LAYER);
        let end = hit.as_ref().map(|(p, _, _)| *p).unwrap_or(to);
        if let Some((_, _, Some(collider))) = &hit
            && collider.instance_id() == player.instance_id()
        {
            player.call(
                "take_damage",
                &[self.damage.to_variant(), from.to_variant()],
            );
        }

        self.flash.set_visible(true);
        self.flash_time = 0.04;
        self.fire_sound.play();
        // From the local player's effects pool when there is one.
        if let Some(mut effects) = crate::effects::CombatEffects::find(&self.base().get_tree()) {
            effects.bind_mut().tracer(from, end);
        } else if let Some(mut tracer) = self
            .tracer_scene
            .instantiate()
            .and_then(|n| n.try_cast::<Node3D>().ok())
            && let Some(mut parent) = self.base().get_parent()
        {
            parent.add_child(&tracer);
            tracer.call("setup", &[from.to_variant(), end.to_variant()]);
        }
    }

    fn ray(
        &self,
        from: Vector3,
        to: Vector3,
        mask: u32,
    ) -> Option<(Vector3, Vector3, Option<Gd<Node>>)> {
        if mask & HITBOX_LAYER != 0 {
            crate::hitbox_rig::sync_all();
        }
        let mut space = self.base().get_world_3d()?.get_direct_space_state()?;
        let query = PhysicsRayQueryParameters3D::create_ex(from, to)
            .collision_mask(mask)
            .done()?;
        let hit = space.intersect_ray(&query);
        if hit.is_empty() {
            return None;
        }
        let position = hit.get_or_nil("position").try_to::<Vector3>().ok()?;
        let normal = hit
            .get_or_nil("normal")
            .try_to::<Vector3>()
            .unwrap_or(Vector3::UP);
        let collider = hit.get_or_nil("collider").try_to::<Gd<Node>>().ok();
        Some((position, normal, collider))
    }
}

fn wrap_angle(a: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    (a + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI
}
