//! Other players' gunfire, positional, from the same layers as the local
//! player's own gun (`WeaponView`): every weapon sounds like itself, so you
//! can tell an M40 from an AK by ear.
//!
//! Distance changes the character of a shot, not just its level. The crack
//! and the punch (`snap`, `body`) fall off with the square of distance, and
//! the echo (`tail`) only with distance, so a fight across the map is mostly
//! its rolling echo while one around the corner is a sharp crack. Godot's
//! distance filter takes the top off far layers on the way (air absorption).

use std::collections::HashMap;

use godot::classes::audio_stream_player_3d::AttenuationModel;
use godot::classes::{AudioStreamPlayer3D, AudioStreamRandomizer, Node3D};
use godot::global::randf_range;
use godot::prelude::*;
use shooter_sim::weapon::WeaponId;

use crate::audio;
use crate::viewmodel::view_scene_property;

/// Distance (metres) the crack and punch stay at full level, then fall 12 dB
/// per doubling.
const NEAR_UNIT: f32 = 12.0;
/// Distance the echo stays at full level, then falls 6 dB per doubling.
const FAR_UNIT: f32 = 26.0;
/// Other players' shots sit a little under the local player's own gun, which
/// is heard from right behind it.
const REMOTE_DB: f32 = -2.0;

/// One weapon's layers and levels, from its view scene.
#[derive(Clone)]
struct GunSounds {
    snap: Gd<AudioStreamRandomizer>,
    body: Gd<AudioStreamRandomizer>,
    tail: Option<Gd<AudioStreamRandomizer>>,
    /// Snap, body, and tail levels in dB.
    levels: [f32; 3],
    pitch: f32,
}

/// The gunshot players of one other player, at their muzzle.
pub struct Gunfire {
    snap: Gd<AudioStreamPlayer3D>,
    body: Gd<AudioStreamPlayer3D>,
    tail: Gd<AudioStreamPlayer3D>,
    guns: HashMap<WeaponId, Option<GunSounds>>,
    /// Shots played (tests).
    pub shots_played: u32,
}

impl Gunfire {
    /// Makes the players as children of `owner`.
    pub fn new(owner: &mut Gd<Node3D>) -> Self {
        let mut make = |name: &str, model: AttenuationModel, unit: f32, polyphony: i32| {
            let mut player = AudioStreamPlayer3D::new_alloc();
            player.set_name(name);
            player.set_bus("Weapons");
            player.set_attenuation_model(model);
            player.set_unit_size(unit);
            player.set_max_polyphony(polyphony);
            owner.add_child(&player);
            player
        };
        Self {
            snap: make(
                "ShotSnap",
                AttenuationModel::INVERSE_SQUARE_DISTANCE,
                NEAR_UNIT,
                4,
            ),
            body: make(
                "ShotBody",
                AttenuationModel::INVERSE_SQUARE_DISTANCE,
                NEAR_UNIT,
                4,
            ),
            // One echo at a time, as for the local gun, so full auto does not smear.
            tail: make("ShotTail", AttenuationModel::INVERSE_DISTANCE, FAR_UNIT, 1),
            guns: HashMap::new(),
            shots_played: 0,
        }
    }

    /// Plays one shot of `weapon` from `muzzle`. False when the weapon has no
    /// gunshot layers.
    pub fn fire(&mut self, weapon: WeaponId, muzzle: Vector3) -> bool {
        let Some(gun) = self
            .guns
            .entry(weapon)
            .or_insert_with(|| load(weapon))
            .clone()
        else {
            return false;
        };
        let pitch = randf_range(0.97, 1.03) as f32 * gun.pitch;
        let layers = [
            (&mut self.snap, Some(&gun.snap), gun.levels[0]),
            (&mut self.body, Some(&gun.body), gun.levels[1]),
            (&mut self.tail, gun.tail.as_ref(), gun.levels[2]),
        ];
        for (player, stream, level) in layers {
            let Some(stream) = stream else { continue };
            player.set_global_position(muzzle);
            // Changing the stream would cut off shots still ringing.
            if player.get_stream().map(|s| s.instance_id()) != Some(stream.instance_id()) {
                player.set_stream(stream);
            }
            // Never louder than the gun's own level up close.
            player.set_volume_db(level);
            player.set_max_db(level);
            player.set_pitch_scale(pitch);
            player.play();
        }
        self.shots_played += 1;
        true
    }
}

/// A weapon's gunshot layers: its own (or borrowed, `sound_from`) folder and
/// the levels its view scene sets, as `WeaponView` builds them.
fn load(weapon: WeaponId) -> Option<GunSounds> {
    let read = |name: &str| view_scene_property(weapon, ".", name);
    let float = |name: &str, default: f32| {
        read(name)
            .and_then(|v| v.try_to::<f32>().ok())
            .unwrap_or(default)
    };
    let borrowed = read("sound_from")
        .and_then(|v| v.try_to::<GString>().ok())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());
    let own = weapon.def().model_key().to_owned();
    let mut keys = vec![borrowed.unwrap_or_else(|| own.clone())];
    // A gun with no recordings of its own falls back by class.
    keys.extend(["m4", "ak47"].map(str::to_owned));
    let dir = keys.iter().find_map(|key| {
        let dir = format!("res://assets/audio/weapons/{key}");
        audio::variations(&format!("{dir}/snap"), 1.0).map(|_| dir)
    })?;
    // `WeaponView`'s defaults, for scenes that leave them unset.
    let volume = float("fire_volume_db", -6.0) + REMOTE_DB;
    Some(GunSounds {
        snap: audio::variations(&format!("{dir}/snap"), 1.0)?,
        body: audio::variations(&format!("{dir}/body"), 1.0)?,
        tail: audio::variations(&format!("{dir}/tail"), 1.0),
        levels: [
            volume + float("snap_db", 2.0),
            volume + float("body_db", 0.0),
            volume + float("tail_db", -3.0),
        ],
        pitch: float("fire_pitch", 1.0),
    })
}
