//! Room sound and occlusion, both judged from where the listener (the current
//! camera) is.
//!
//! **Reverb zones.** A map marks its tunnels and rooms with `ReverbZone` boxes.
//! While the camera is inside one, `AudioEnvironment` blends a reverb onto the
//! Weapons and Effects buses toward that zone's settings, and back off outside.
//! Judging by the listener rather than by each source (as `Area3D` reverb
//! buses do) also covers the player's own gunfire, which is not positional,
//! and a fight in one room is heard in that room's acoustics.
//!
//! **Occlusion.** Every playing `AudioStreamPlayer3D` is checked against the
//! world with a ray from the listener. A sound behind a wall gets quieter and
//! plays through a low-pass copy of its bus (`"<bus> (occluded)"`, made on
//! demand), so gunfire and footsteps in the next room sound muffled.

use std::collections::HashMap;

use godot::classes::{
    AudioEffectLowPassFilter, AudioEffectReverb, AudioServer, AudioStreamPlayer3D, INode, INode3D,
    PhysicsRayQueryParameters3D,
};
use godot::prelude::*;

use crate::target::WORLD_LAYER;

/// Buses the zone reverb is blended onto.
const REVERB_BUSES: [&str; 2] = ["Weapons", "Effects"];
/// Name of the reverb effect this module adds to each bus.
const REVERB_NAME: &str = "ZoneReverb";
/// How fast the reverb follows the listener between zones, per second.
const REVERB_BLEND_RATE: f32 = 4.0;

/// Seconds between occlusion checks of one playing source.
const OCCLUSION_INTERVAL: f32 = 0.1;
/// Most occlusion rays cast per frame, beyond sources that just started.
const OCCLUSION_RAYS_PER_FRAME: usize = 12;
/// A wall closer than this to the source still lets it through: an impact on
/// the near face of a wall is not behind that wall.
const OCCLUSION_SLACK: f32 = 0.4;
/// Volume taken off a fully occluded sound, in dB.
const OCCLUDED_DB: f32 = 9.0;
/// Low-pass cutoff of the occluded buses, in Hz.
const OCCLUDED_CUTOFF_HZ: f32 = 900.0;
/// How fast occlusion changes once a source is playing, per second, so a
/// sound does not jump when someone steps out from behind a pillar.
const OCCLUSION_RATE: f32 = 8.0;

/// A box of a map with its own acoustics, in the node's local space (centred
/// on its origin). Zones may overlap; the smallest one around the listener
/// wins, so a room inside a larger hall sounds like the room.
#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct ReverbZone {
    /// Full size of the box, in metres.
    #[export]
    #[init(val = Vector3::new(4.0, 3.0, 4.0))]
    size: Vector3,
    /// `AudioEffectReverb` settings: room size and damping 0 to 1, and how
    /// much reverb is mixed in (wet).
    #[export]
    #[init(val = 0.4)]
    room_size: f32,
    #[export]
    #[init(val = 0.5)]
    damping: f32,
    #[export]
    #[init(val = 0.25)]
    wet: f32,
    /// Delay before the first reflection, in ms (larger rooms, longer delay).
    #[export]
    #[init(val = 20.0)]
    predelay_ms: f32,
    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for ReverbZone {
    fn ready(&mut self) {
        self.base_mut().add_to_group(ReverbZone::GROUP);
    }
}

#[godot_api]
impl ReverbZone {
    const GROUP: &'static str = "reverb_zones";

    /// True if `point` (global) is inside the box.
    #[func]
    pub fn contains(&self, point: Vector3) -> bool {
        let local = self.base().get_global_transform().affine_inverse() * point;
        let half = self.size * 0.5;
        local.x.abs() <= half.x && local.y.abs() <= half.y && local.z.abs() <= half.z
    }

    fn settings(&self) -> Reverb {
        Reverb {
            room_size: self.room_size,
            damping: self.damping,
            wet: self.wet,
            predelay_ms: self.predelay_ms,
        }
    }

    fn volume(&self) -> f32 {
        self.size.x * self.size.y * self.size.z
    }
}

/// Reverb settings, blended between zones.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Reverb {
    room_size: f32,
    damping: f32,
    wet: f32,
    predelay_ms: f32,
}

impl Reverb {
    /// Outdoors: no reverb, but keep the last room's shape while it fades.
    fn outdoors(from: Reverb) -> Reverb {
        Reverb { wet: 0.0, ..from }
    }

    fn approach(self, target: Reverb, t: f32) -> Reverb {
        let lerp = |a: f32, b: f32| a + (b - a) * t;
        Reverb {
            room_size: lerp(self.room_size, target.room_size),
            damping: lerp(self.damping, target.damping),
            wet: lerp(self.wet, target.wet),
            predelay_ms: lerp(self.predelay_ms, target.predelay_ms),
        }
    }
}

/// What one tracked source sounded like before occlusion, and how occluded it
/// is now.
struct Source {
    player: Gd<AudioStreamPlayer3D>,
    bus: StringName,
    volume_db: f32,
    occlusion: f32,
    /// Seconds until the next ray; negative means check now.
    next_check: f32,
    /// The latest ray's verdict.
    blocked: bool,
    playing: bool,
}

/// Applies reverb zones and occlusion for the local listener. The player adds
/// one to itself; nothing else needs one.
#[derive(GodotClass)]
#[class(init, base = Node)]
pub struct AudioEnvironment {
    reverb: Option<Reverb>,
    sources: HashMap<InstanceId, Source>,
    /// Round-robin position for occlusion rays.
    cursor: usize,
    base: Base<Node>,
}

#[godot_api]
impl INode for AudioEnvironment {
    fn ready(&mut self) {
        let mut tree = self.base().get_tree();
        let callable = self.base().callable("track");
        tree.connect("node_added", &callable);
        let root = tree.get_root();
        for node in root
            .find_children_ex("*")
            .type_("AudioStreamPlayer3D")
            .owned(false)
            .done()
            .iter_shared()
        {
            self.track(node);
        }
    }

    fn process(&mut self, delta: f64) {
        let Some(listener) = self.listener() else {
            return;
        };
        let delta = delta as f32;
        self.update_reverb(listener, delta);
        self.update_occlusion(listener, delta);
    }

    fn exit_tree(&mut self) {
        // Leave the buses as the project defines them for the next scene.
        set_reverb(None);
        for source in self.sources.values_mut() {
            if source.player.is_instance_valid() {
                source.player.set_bus(&source.bus);
                source.player.set_volume_db(source.volume_db);
            }
        }
        self.sources.clear();
    }
}

#[godot_api]
impl AudioEnvironment {
    /// Starts watching a newly added node if it is a positional sound.
    #[func]
    fn track(&mut self, node: Gd<Node>) {
        let Ok(player) = node.try_cast::<AudioStreamPlayer3D>() else {
            return;
        };
        let id = player.instance_id();
        self.sources.entry(id).or_insert_with(|| Source {
            bus: player.get_bus(),
            volume_db: player.get_volume_db(),
            player,
            occlusion: 0.0,
            next_check: -1.0,
            blocked: false,
            playing: false,
        });
    }

    /// How occluded a tracked sound is now, 0 (clear) to 1 (tests).
    #[func]
    fn occlusion_of(&self, player: Gd<AudioStreamPlayer3D>) -> f32 {
        self.sources
            .get(&player.instance_id())
            .map_or(0.0, |s| s.occlusion)
    }

    /// The reverb mixed in now (wet, 0 outdoors), for tests and debugging.
    #[func]
    fn reverb_wet(&self) -> f32 {
        self.reverb.map_or(0.0, |r| r.wet)
    }
}

impl AudioEnvironment {
    fn listener(&self) -> Option<Vector3> {
        let camera = self.base().get_viewport()?.get_camera_3d()?;
        Some(camera.get_global_position())
    }

    fn update_reverb(&mut self, listener: Vector3, delta: f32) {
        let tree = self.base().get_tree();
        let zone = tree
            .get_nodes_in_group(ReverbZone::GROUP)
            .iter_shared()
            .filter_map(|node| node.try_cast::<ReverbZone>().ok())
            .filter(|zone| zone.bind().contains(listener))
            .min_by(|a, b| a.bind().volume().total_cmp(&b.bind().volume()));
        let target = zone.map(|z| z.bind().settings());
        let current = match (self.reverb, target) {
            (None, None) => return,
            // Entering the first room: start from its shape, faded out.
            (None, Some(t)) => Reverb::outdoors(t),
            (Some(c), _) => c,
        };
        let target = target.unwrap_or_else(|| Reverb::outdoors(current));
        let t = 1.0 - (-REVERB_BLEND_RATE * delta).exp();
        let next = current.approach(target, t);
        if target.wet <= 0.0 && next.wet < 0.005 {
            self.reverb = None;
            set_reverb(None);
        } else {
            self.reverb = Some(next);
            set_reverb(Some(next));
        }
    }

    fn update_occlusion(&mut self, listener: Vector3, delta: f32) {
        self.sources.retain(|_, s| s.player.is_instance_valid());
        let Some(mut space) = self
            .base()
            .get_viewport()
            .and_then(|v| v.find_world_3d())
            .and_then(|w| w.get_direct_space_state())
        else {
            return;
        };

        let ids: Vec<InstanceId> = self.sources.keys().copied().collect();
        let mut budget = OCCLUSION_RAYS_PER_FRAME;
        let count = ids.len().max(1);
        for step in 0..ids.len() {
            let id = ids[(self.cursor + step) % count];
            let Some(source) = self.sources.get_mut(&id) else {
                continue;
            };
            let playing = source.player.is_playing();
            if !playing {
                if source.playing {
                    // Restore it, so the next play starts as authored.
                    source.player.set_bus(&source.bus);
                    source.player.set_volume_db(source.volume_db);
                    source.occlusion = 0.0;
                    source.next_check = -1.0;
                }
                source.playing = false;
                continue;
            }
            let just_started = !source.playing;
            source.playing = true;
            source.next_check -= delta;
            if source.next_check < 0.0 && (just_started || budget > 0) {
                if !just_started {
                    budget -= 1;
                }
                source.next_check = OCCLUSION_INTERVAL;
                source.blocked = blocked(&mut space, listener, source.player.get_global_position());
            }
            let target = if source.blocked { 1.0 } else { 0.0 };
            source.occlusion = if just_started {
                target
            } else {
                let t = 1.0 - (-OCCLUSION_RATE * delta).exp();
                source.occlusion + (target - source.occlusion) * t
            };
            apply_occlusion(source);
        }
        self.cursor = (self.cursor + 1) % count;
    }
}

/// True if the world blocks the line from the listener to a sound, short of
/// the sound itself.
fn blocked(
    space: &mut Gd<godot::classes::PhysicsDirectSpaceState3D>,
    from: Vector3,
    to: Vector3,
) -> bool {
    let distance = from.distance_to(to);
    if distance <= OCCLUSION_SLACK {
        return false;
    }
    let Some(query) = PhysicsRayQueryParameters3D::create_ex(from, to)
        .collision_mask(WORLD_LAYER)
        .done()
    else {
        return false;
    };
    let hit = space.intersect_ray(&query);
    let Ok(point) = hit.get_or_nil("position").try_to::<Vector3>() else {
        return false;
    };
    from.distance_to(point) < distance - OCCLUSION_SLACK
}

fn apply_occlusion(source: &mut Source) {
    let occlusion = source.occlusion.clamp(0.0, 1.0);
    source
        .player
        .set_volume_db(source.volume_db - OCCLUDED_DB * occlusion);
    let bus = if occlusion > 0.5 {
        occluded_bus(&source.bus)
    } else {
        source.bus.clone()
    };
    if source.player.get_bus() != bus {
        source.player.set_bus(&bus);
    }
}

/// The low-pass copy of `bus`, made the first time it is needed. It sends into
/// `bus`, so that bus's volume setting and effects still apply.
fn occluded_bus(bus: &StringName) -> StringName {
    let name = StringName::from(format!("{bus} (occluded)").as_str());
    let mut server = AudioServer::singleton();
    if server.get_bus_index(&name) >= 0 {
        return name;
    }
    let index = server.get_bus_count();
    server.add_bus();
    server.set_bus_name(index, &name.to_string());
    server.set_bus_send(index, bus);
    let mut filter = AudioEffectLowPassFilter::new_gd();
    filter.set_cutoff(OCCLUDED_CUTOFF_HZ);
    server.add_bus_effect(index, &filter);
    name
}

/// Puts `reverb` on the reverb buses, or turns their zone reverb off.
fn set_reverb(reverb: Option<Reverb>) {
    let mut server = AudioServer::singleton();
    for bus in REVERB_BUSES {
        let index = server.get_bus_index(bus);
        if index < 0 {
            continue;
        }
        let existing = (0..server.get_bus_effect_count(index)).find(|&i| {
            server
                .get_bus_effect(index, i)
                .is_some_and(|e| e.get_name() == REVERB_NAME)
        });
        let Some(reverb) = reverb else {
            if let Some(i) = existing {
                server.set_bus_effect_enabled(index, i, false);
            }
            continue;
        };
        let (slot, mut effect) = match existing.and_then(|i| {
            let effect = server
                .get_bus_effect(index, i)?
                .try_cast::<AudioEffectReverb>()
                .ok()?;
            Some((i, effect))
        }) {
            Some(found) => found,
            None => {
                let mut effect = AudioEffectReverb::new_gd();
                effect.set_name(REVERB_NAME);
                // Keep the dry signal at full level; the zone only adds room.
                effect.set_dry(1.0);
                effect.set_spread(0.8);
                effect.set_hpf(0.15);
                server.add_bus_effect(index, &effect);
                (server.get_bus_effect_count(index) - 1, effect)
            }
        };
        effect.set_room_size(reverb.room_size);
        effect.set_damping(reverb.damping);
        effect.set_wet(reverb.wet);
        effect.set_predelay_msec(reverb.predelay_ms);
        server.set_bus_effect_enabled(index, slot, true);
    }
}
