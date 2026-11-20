//! Bullets as this client sees and hears them: other players' shots traced
//! into rays, near misses that crack or whiz past the listener, and impacts
//! from a fixed pool of positional voices.
//!
//! **The ray.** The server decides every hit, and snapshots only say that a
//! player fired, not where. [`ShotTracer::trace`] rebuilds a remote shot from
//! what the client has (the shooter's eye, view direction, and weapon) with
//! one ray against the world and the hit zones, and returns a [`ShotRay`]:
//! where it started, which way it went, where it stopped, and what it hit.
//! Its spread is not known here, so the ray is the aim line; close enough for
//! sound and for drawing a tracer. Every traced shot is also emitted as the
//! `shot_traced` signal, the entry point for effects (tracers, impact
//! puffs) that want the same ray without casting their own.
//!
//! Tracing is bounded: at most [`RAYS_PER_FRAME`] rays a frame across all
//! shooters; shots past that are not traced (their gunfire still plays).
//! Nothing here exists on the dedicated server: the local player adds the
//! tracer, and the server has no local player.
//!
//! **Near misses.** A round passing within [`NEAR_MISS_RADIUS`] of the
//! listener cracks (a supersonic rifle round's shock wave) or whizzes (a
//! subsonic pistol, SMG, or buckshot pellet) at the closest point of its
//! path, when the bullet gets there, ahead of the muzzle report, which
//! travels at the speed of sound (`gunfire`). The lens closes in with it
//! (`lens.suppress`).
//!
//! **Impacts.** Every impact sound, the local player's own and everyone
//! else's, plays from a pool of [`IMPACT_VOICES`] positional players taken in
//! turn, so a shotgun blast into a wall is a handful of voices, not one
//! node per pellet. Metal, concrete, and tile sometimes ricochet.

use std::collections::HashMap;

use godot::classes::audio_stream_player_3d::AttenuationModel;
use godot::classes::{
    AudioStreamPlayer3D, AudioStreamRandomizer, INode3D, Node3D, PhysicsRayQueryParameters3D,
};
use godot::prelude::*;
use shooter_sim::rng;
use shooter_sim::weapon::WeaponId;

use crate::audio;
use crate::hitbox_rig::HITBOX_LAYER;
use crate::surfaces::{Surface, surface_at};
use crate::target::WORLD_LAYER;

/// Group the tracer joins, so remote players can find it.
pub const GROUP: &str = "shot_tracer";
/// Most remote shots traced in one frame.
pub const RAYS_PER_FRAME: u32 = 12;
/// How far a traced shot reaches, at most (metres).
const MAX_RANGE: f32 = 300.0;
/// The ray starts this far in front of the shooter's eye, clear of their own
/// hit zones.
const EYE_CLEARANCE: f32 = 0.6;
/// A round passing closer than this (metres) is heard going by.
pub const NEAR_MISS_RADIUS: f32 = 3.5;
/// Not for the first metres of a shot: that is the shooter's own space.
const NEAR_MISS_MIN_TRAVEL: f32 = 2.0;
/// Speeds (m/s) the sound of a passing round arrives at: rifle rounds about
/// two and a half times the speed of sound, pistol rounds and pellets below it.
const SUPERSONIC_SPEED: f32 = 850.0;
const SUBSONIC_SPEED: f32 = 330.0;
/// Remote impacts are heard within this distance of the listener.
const IMPACT_HEARING: f32 = 40.0;
/// Positional voices for impacts and ricochets, and for near misses.
pub const IMPACT_VOICES: usize = 10;
const FLYBY_VOICES: usize = 2;
/// Sounds waiting for their bullet to arrive, at most.
const PENDING_MAX: usize = 16;
/// Share of hits on hard surfaces that ricochet.
const RICOCHET_CHANCE: f32 = 0.15;

/// What a traced shot hit.
#[derive(Clone, Copy, Debug)]
pub struct ShotHit {
    pub position: Vector3,
    pub normal: Vector3,
    pub surface: Surface,
    /// A player's hit zone rather than the world.
    pub body: bool,
}

/// One remote shot, traced on this client.
#[derive(Clone, Copy, Debug)]
pub struct ShotRay {
    pub weapon: WeaponId,
    /// Where tracers start: the muzzle of the shooter's third-person weapon.
    pub muzzle: Vector3,
    /// Where the ray starts (in front of the shooter's eye) and its unit direction.
    pub origin: Vector3,
    pub direction: Vector3,
    /// Where the round stopped: the hit, or the end of its range.
    pub end: Vector3,
    pub hit: Option<ShotHit>,
}

impl ShotRay {
    /// The closest the round came to `point`: (distance along the ray from
    /// its origin, miss distance).
    pub fn closest_approach(&self, point: Vector3) -> (f32, f32) {
        let length = self.origin.distance_to(self.end);
        let along = (point - self.origin).dot(self.direction).clamp(0.0, length);
        let nearest = self.origin + self.direction * along;
        (along, nearest.distance_to(point))
    }
}

/// Whether a weapon's rounds break the sound barrier (they crack rather than
/// whiz). Pistols, the 9 mm SMGs, and buckshot do not.
pub fn supersonic(weapon: WeaponId) -> bool {
    let def = weapon.def();
    !(def.pellets > 1 || matches!(def.model_key(), "m1911" | "bp9" | "pdw9" | "cq10") || def.beam)
}

/// How strongly a near miss at `distance` metres suppresses (0 to 1):
/// fully at arm's length, fading to nothing at the edge of the radius.
pub fn suppression(distance: f32, supersonic: bool) -> f32 {
    let t = (1.0 - distance / NEAR_MISS_RADIUS).clamp(0.0, 1.0);
    let s = t.sqrt();
    if supersonic { s } else { s * 0.6 }
}

/// Level (dB) of a near miss at `distance` metres.
fn near_miss_db(distance: f32, supersonic: bool) -> f32 {
    let base = if supersonic { -1.0 } else { -6.0 };
    base - 9.0 * (distance / NEAR_MISS_RADIUS).clamp(0.0, 1.0)
}

/// A sound waiting for its bullet to arrive.
struct Pending {
    left: f32,
    kind: Voice,
    stream: Gd<AudioStreamRandomizer>,
    position: Vector3,
    volume_db: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Voice {
    Impact,
    Flyby,
}

/// Traces remote shots and plays bullet sounds. The local player adds one to
/// itself (client only).
#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct ShotTracer {
    impacts: Vec<Gd<AudioStreamPlayer3D>>,
    next_impact: usize,
    flybys: Vec<Gd<AudioStreamPlayer3D>>,
    next_flyby: usize,
    pending: Vec<Pending>,
    sounds: HashMap<String, Option<Gd<AudioStreamRandomizer>>>,
    /// Rays cast this frame, and which frame that is.
    rays: u32,
    frame: u64,
    /// Shots traced, for deterministic choices (ricochets).
    traced: u32,
    /// Near misses and impacts heard (tests).
    near_misses: u32,
    impacts_played: u32,
    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for ShotTracer {
    fn ready(&mut self) {
        self.base_mut().add_to_group(GROUP);
        // Positional players that stay where they are put.
        self.base_mut().set_as_top_level(true);
        for i in 0..IMPACT_VOICES {
            let player = self.voice(&format!("Impact{i}"), 5.0);
            self.impacts.push(player);
        }
        for i in 0..FLYBY_VOICES {
            let mut player = self.voice(&format!("Flyby{i}"), 1.0);
            // Its level comes from the miss distance, not from attenuation.
            player.set_attenuation_model(AttenuationModel::DISABLED);
            self.flybys.push(player);
        }
    }

    fn process(&mut self, delta: f64) {
        let delta = delta as f32;
        if self.pending.is_empty() {
            return;
        }
        let mut due = Vec::new();
        self.pending.retain_mut(|p| {
            p.left -= delta;
            if p.left <= 0.0 {
                due.push((p.kind, p.stream.clone(), p.position, p.volume_db));
                false
            } else {
                true
            }
        });
        for (kind, stream, position, volume) in due {
            self.start(kind, &stream, position, volume);
        }
    }
}

#[godot_api]
impl ShotTracer {
    /// A traced remote shot, for effects: the muzzle, where the round
    /// stopped, the surface normal there, what it hit (`Surface` key, empty
    /// for nothing), whether it hit at all, and the weapon key.
    #[signal]
    fn shot_traced(
        muzzle: Vector3,
        end: Vector3,
        normal: Vector3,
        surface: StringName,
        hit: bool,
        weapon: StringName,
    );

    /// Plays an impact on `surface` (a `Surface` key) at `position`.
    #[func]
    pub fn impact(&mut self, position: Vector3, surface: GString) {
        let surface = Surface::from_tag(&surface.to_string()).unwrap_or_default();
        self.play_impact(position, surface, 0.0, 0.0);
    }

    /// Traces a shot of `weapon` (key) from `eye` along `direction` as if
    /// another player fired it (tests). False when over the frame's budget.
    #[func]
    fn trace_test_shot(&mut self, weapon: GString, eye: Vector3, direction: Vector3) -> bool {
        let Some(weapon) = WeaponId::by_key(&weapon.to_string()) else {
            return false;
        };
        self.trace(weapon, eye, direction.normalized(), eye)
            .is_some()
    }

    /// Near misses heard so far (tests).
    #[func]
    fn near_misses_heard(&self) -> u32 {
        self.near_misses
    }

    /// Impacts played so far (tests).
    #[func]
    fn impacts_heard(&self) -> u32 {
        self.impacts_played
    }
}

impl ShotTracer {
    /// The scene's tracer, if the local player made one.
    pub fn find(tree: &Gd<SceneTree>) -> Option<Gd<ShotTracer>> {
        tree.get_first_node_in_group(GROUP)
            .and_then(|n| n.try_cast::<ShotTracer>().ok())
    }

    /// Traces one remote shot of `weapon` from the shooter's `eye` along
    /// `direction` (unit), plays what the listener would hear of it, emits
    /// `shot_traced`, and returns the ray. `None` when this frame's ray
    /// budget is spent.
    pub fn trace(
        &mut self,
        weapon: WeaponId,
        eye: Vector3,
        direction: Vector3,
        muzzle: Vector3,
    ) -> Option<ShotRay> {
        let frame = godot::classes::Engine::singleton().get_process_frames();
        if frame != self.frame {
            self.frame = frame;
            self.rays = 0;
        }
        if self.rays >= RAYS_PER_FRAME {
            return None;
        }
        self.rays += 1;
        self.traced = self.traced.wrapping_add(1);
        let range = weapon.def().range.min(MAX_RANGE);
        let origin = eye + direction * EYE_CLEARANCE;
        let mut ray = ShotRay {
            weapon,
            muzzle,
            origin,
            direction,
            end: origin + direction * range,
            hit: None,
        };
        let mut space = self
            .base()
            .get_world_3d()
            .and_then(|w| w.get_direct_space_state())?;
        let query = PhysicsRayQueryParameters3D::create_ex(origin, ray.end)
            .collision_mask(WORLD_LAYER | HITBOX_LAYER)
            .done()?;
        let hit = space.intersect_ray(&query);
        if let Ok(position) = hit.get_or_nil("position").try_to::<Vector3>() {
            let normal = hit
                .get_or_nil("normal")
                .try_to::<Vector3>()
                .unwrap_or(Vector3::UP);
            let collider = hit.get_or_nil("collider").try_to::<Gd<Node>>().ok();
            let body = collider.as_ref().is_some_and(|c| {
                c.clone()
                    .try_cast::<godot::classes::CollisionObject3D>()
                    .is_ok_and(|o| o.get_collision_layer() & HITBOX_LAYER != 0)
            });
            let face = hit.get_or_nil("face_index").try_to::<i32>().unwrap_or(-1);
            let surface = if body {
                Surface::Flesh
            } else {
                surface_at(collider.as_ref(), face)
            };
            ray.end = position;
            ray.hit = Some(ShotHit {
                position,
                normal,
                surface,
                body,
            });
        }
        self.hear(&ray);
        let (normal, surface, hit) = ray.hit.map_or((Vector3::ZERO, "", false), |h| {
            (h.normal, h.surface.key(), true)
        });
        let args = [
            ray.muzzle.to_variant(),
            ray.end.to_variant(),
            normal.to_variant(),
            StringName::from(surface).to_variant(),
            hit.to_variant(),
            StringName::from(weapon.def().model_key()).to_variant(),
        ];
        self.base_mut().emit_signal("shot_traced", &args);
        Some(ray)
    }

    /// The near miss and the impact of a traced shot, as the listener hears them.
    fn hear(&mut self, ray: &ShotRay) {
        let Some(listener) = self
            .base()
            .get_viewport()
            .and_then(|v| v.get_camera_3d())
            .map(|c| c.get_global_position())
        else {
            return;
        };
        let fast = supersonic(ray.weapon);
        let speed = if fast {
            SUPERSONIC_SPEED
        } else {
            SUBSONIC_SPEED
        };
        let (along, miss) = ray.closest_approach(listener);
        let length = ray.origin.distance_to(ray.end);
        // Passing by, not stopping short of the listener or hitting them.
        if miss < NEAR_MISS_RADIUS && along > NEAR_MISS_MIN_TRAVEL && along < length - 0.3 {
            let base = if fast {
                "res://assets/audio/bullets/crack"
            } else {
                "res://assets/audio/bullets/whiz"
            };
            if let Some(stream) = self.sound(base) {
                let at = ray.origin + ray.direction * along;
                self.queue(
                    Voice::Flyby,
                    stream,
                    at,
                    near_miss_db(miss, fast),
                    along / speed,
                );
                self.near_misses += 1;
                let strength = suppression(miss, fast);
                let mut tree = self.base().get_tree();
                tree.call_group("lens", "suppress", &[strength.to_variant()]);
            }
        }
        // A round into the world, or into someone (flesh).
        if let Some(hit) = ray.hit
            && hit.position.distance_to(listener) < IMPACT_HEARING
        {
            self.play_impact(hit.position, hit.surface, length / speed, 0.0);
            let hard = !hit.body
                && matches!(
                    hit.surface,
                    Surface::Metal | Surface::Concrete | Surface::Tile
                );
            if hard && rng::unit(self.traced, 77) < RICOCHET_CHANCE {
                self.play_ricochet(hit.position, length / speed + 0.02);
            }
        }
    }

    fn play_impact(&mut self, position: Vector3, surface: Surface, delay: f32, gain_db: f32) {
        let base = format!("res://assets/audio/impacts/{}", surface.impact_sound_key());
        if let Some(stream) = self.sound(&base) {
            self.queue(Voice::Impact, stream, position, gain_db - 2.0, delay);
            self.impacts_played += 1;
        }
    }

    fn play_ricochet(&mut self, position: Vector3, delay: f32) {
        if let Some(stream) = self.sound("res://assets/audio/bullets/ricochet") {
            self.queue(Voice::Impact, stream, position, -6.0, delay);
        }
    }

    fn queue(
        &mut self,
        kind: Voice,
        stream: Gd<AudioStreamRandomizer>,
        position: Vector3,
        volume_db: f32,
        delay: f32,
    ) {
        if delay <= 0.005 {
            self.start(kind, &stream, position, volume_db);
        } else if self.pending.len() < PENDING_MAX {
            self.pending.push(Pending {
                left: delay,
                kind,
                stream,
                position,
                volume_db,
            });
        }
    }

    /// Starts a sound on the next voice of its pool, cutting the oldest.
    fn start(
        &mut self,
        kind: Voice,
        stream: &Gd<AudioStreamRandomizer>,
        position: Vector3,
        volume_db: f32,
    ) {
        let (pool, next) = match kind {
            Voice::Impact => (&mut self.impacts, &mut self.next_impact),
            Voice::Flyby => (&mut self.flybys, &mut self.next_flyby),
        };
        if pool.is_empty() {
            return;
        }
        let index = *next % pool.len();
        *next = index + 1;
        let voice = &mut pool[index];
        voice.set_global_position(position);
        voice.set_stream(stream);
        voice.set_volume_db(volume_db);
        voice.play();
    }

    fn voice(&mut self, name: &str, unit_size: f32) -> Gd<AudioStreamPlayer3D> {
        let mut player = AudioStreamPlayer3D::new_alloc();
        player.set_name(name);
        player.set_bus("Effects");
        player.set_unit_size(unit_size);
        player.set_max_distance(IMPACT_HEARING * 1.5);
        player.set_max_polyphony(1);
        self.base_mut().add_child(&player);
        player
    }

    fn sound(&mut self, base: &str) -> Option<Gd<AudioStreamRandomizer>> {
        self.sounds
            .entry(base.to_owned())
            .or_insert_with(|| audio::variations(base, 1.08))
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ray(origin: Vector3, end: Vector3) -> ShotRay {
        ShotRay {
            weapon: WeaponId::expect("m4"),
            muzzle: origin,
            origin,
            direction: (end - origin).normalized(),
            end,
            hit: None,
        }
    }

    #[test]
    fn closest_approach_is_along_the_path() {
        let r = ray(Vector3::ZERO, Vector3::new(0.0, 0.0, -50.0));
        let (along, miss) = r.closest_approach(Vector3::new(1.0, 0.0, -20.0));
        assert!((along - 20.0).abs() < 1e-4);
        assert!((miss - 1.0).abs() < 1e-4);
        // Behind the shooter: clamped to the start.
        let (along, _) = r.closest_approach(Vector3::new(0.0, 0.0, 5.0));
        assert_eq!(along, 0.0);
        // Past where the round stopped: clamped to the end.
        let (along, miss) = r.closest_approach(Vector3::new(0.0, 0.0, -60.0));
        assert!((along - 50.0).abs() < 1e-4 && (miss - 10.0).abs() < 1e-4);
    }

    #[test]
    fn closer_misses_suppress_harder() {
        assert!(suppression(0.3, true) > suppression(2.0, true));
        assert_eq!(suppression(NEAR_MISS_RADIUS, true), 0.0);
        assert!(suppression(1.0, false) < suppression(1.0, true));
        assert!(near_miss_db(0.5, true) > near_miss_db(3.0, true));
    }
}
