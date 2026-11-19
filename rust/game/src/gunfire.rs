//! Other players' gunfire, positional, from the same layers as the local
//! player's own gun (`WeaponView`): every weapon sounds like itself, so you
//! can tell an M40 from an AK by ear.
//!
//! Distance changes the character of a shot, not just its level:
//!
//! - **Near** (under about 50 m): the crack and punch (`snap`, `body`) fall
//!   off with the square of distance, and the echo (`tail`) only with
//!   distance, outdoors the terrain echo and, from a shooter inside a reverb
//!   zone, the room (`tail_indoor`).
//! - **Mid** (about 25 to 200 m) and **far** (beyond about 100 m): real
//!   recordings of the same gun from down the range (`mid`, `far`), already
//!   dark with distance, cross-faded in as the near layers fade out, falling
//!   6 dB per doubling of distance.
//!
//! And a shot is heard when its sound arrives: at the speed of sound, so a
//! shot 170 m away lands half a second after its muzzle flash. A round that
//! passes the listener cracks before its report (`bullets`).

use std::collections::HashMap;

use godot::classes::audio_stream_player_3d::AttenuationModel;
use godot::classes::{AudioStreamPlayer3D, AudioStreamRandomizer, Node3D};
use godot::global::randf_range;
use godot::prelude::*;
use shooter_sim::weapon::WeaponId;

use crate::acoustics::{self, SPEED_OF_SOUND};
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
/// Where the mid and far layers start falling 6 dB per doubling (metres).
const MID_REFERENCE: f32 = 40.0;
const FAR_REFERENCE: f32 = 120.0;
/// Level of the mid and far layers against the near ones at the reference.
const MID_DB: f32 = -1.0;
const FAR_DB: f32 = 2.0;
/// The bus distant layers play on; it sends into Weapons.
const DISTANT_BUS: &str = "WeaponsDistant";
/// Shots waiting for their sound to arrive, at most.
const PENDING_MAX: usize = 8;

/// How much of each distance layer plays at `distance` metres: (near, mid,
/// far), each 0 to 1, cross-fading so the sum stays about even.
pub fn layer_weights(distance: f32) -> (f32, f32, f32) {
    // A raised-cosine ramp from 0 at `a` to 1 at `b`.
    let ramp = |a: f32, b: f32| {
        let t = ((distance - a) / (b - a)).clamp(0.0, 1.0);
        0.5 - 0.5 * (std::f32::consts::PI * t).cos()
    };
    let near = 1.0 - ramp(30.0, 70.0);
    let mid = ramp(20.0, 45.0) * (1.0 - ramp(140.0, 220.0));
    let far = ramp(100.0, 170.0);
    (near, mid, far)
}

/// The level (dB) of a mid or far layer at `distance`, with its own
/// reference distance: flat inside it, then 6 dB per doubling.
fn distance_db(distance: f32, reference: f32) -> f32 {
    -6.0 * (distance / reference).max(1.0).log2()
}

/// One weapon's layers and levels, from its view scene.
#[derive(Clone)]
struct GunSounds {
    snap: Gd<AudioStreamRandomizer>,
    /// None when the snap is the whole shot (`tools/build_gunshots.py`).
    body: Option<Gd<AudioStreamRandomizer>>,
    tail: Option<Gd<AudioStreamRandomizer>>,
    tail_indoor: Option<Gd<AudioStreamRandomizer>>,
    mid: Option<Gd<AudioStreamRandomizer>>,
    far: Option<Gd<AudioStreamRandomizer>>,
    /// Snap, body, and tail levels in dB, and the shot's overall level.
    levels: [f32; 3],
    volume: f32,
    pitch: f32,
}

/// A shot whose sound has not reached the listener yet.
struct Pending {
    left: f32,
    weapon: WeaponId,
    muzzle: Vector3,
    distance: f32,
    indoors: bool,
}

/// The gunshot players of one other player, at their muzzle.
pub struct Gunfire {
    snap: Gd<AudioStreamPlayer3D>,
    body: Gd<AudioStreamPlayer3D>,
    tail: Gd<AudioStreamPlayer3D>,
    mid: Gd<AudioStreamPlayer3D>,
    far: Gd<AudioStreamPlayer3D>,
    guns: HashMap<WeaponId, Option<GunSounds>>,
    pending: Vec<Pending>,
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
            player.set_max_distance(0.0);
            owner.add_child(&player);
            player
        };
        let mut mid = make("ShotMid", AttenuationModel::DISABLED, 1.0, 2);
        let mut far = make("ShotFar", AttenuationModel::DISABLED, 1.0, 2);
        // Distant fire gives way to close fire (`acoustics::request_duck`).
        mid.set_bus(DISTANT_BUS);
        far.set_bus(DISTANT_BUS);
        Self {
            // Room for whole-shot snaps to overlap (`audio::WHOLE_SHOT_VOICES`).
            snap: make(
                "ShotSnap",
                AttenuationModel::INVERSE_SQUARE_DISTANCE,
                NEAR_UNIT,
                audio::WHOLE_SHOT_VOICES,
            ),
            body: make(
                "ShotBody",
                AttenuationModel::INVERSE_SQUARE_DISTANCE,
                NEAR_UNIT,
                4,
            ),
            // One echo at a time, as for the local gun, so full auto does not smear.
            tail: make("ShotTail", AttenuationModel::INVERSE_DISTANCE, FAR_UNIT, 1),
            // Their level comes from `distance_db`; the recordings are already
            // dark with distance.
            mid,
            far,
            guns: HashMap::new(),
            pending: Vec::new(),
            shots_played: 0,
        }
    }

    /// One shot of `weapon` from `muzzle`. It plays once its sound reaches
    /// `listener` (now, without one); `indoors` when the shooter is inside a
    /// reverb zone. False when the weapon has no gunshot layers.
    pub fn fire(
        &mut self,
        weapon: WeaponId,
        muzzle: Vector3,
        listener: Option<Vector3>,
        indoors: bool,
    ) -> bool {
        if self.sounds(weapon).is_none() {
            return false;
        }
        let distance = listener.map_or(0.0, |l| l.distance_to(muzzle));
        let delay = distance / SPEED_OF_SOUND;
        if delay < 0.01 || self.pending.len() >= PENDING_MAX {
            self.play(weapon, muzzle, distance, indoors);
        } else {
            self.pending.push(Pending {
                left: delay,
                weapon,
                muzzle,
                distance,
                indoors,
            });
        }
        true
    }

    /// Plays the shots whose sound has arrived.
    pub fn update(&mut self, delta: f32) {
        if self.pending.is_empty() {
            return;
        }
        let mut due = Vec::new();
        self.pending.retain_mut(|p| {
            p.left -= delta;
            if p.left <= 0.0 {
                due.push((p.weapon, p.muzzle, p.distance, p.indoors));
                false
            } else {
                true
            }
        });
        for (weapon, muzzle, distance, indoors) in due {
            self.play(weapon, muzzle, distance, indoors);
        }
    }

    fn sounds(&mut self, weapon: WeaponId) -> Option<GunSounds> {
        self.guns
            .entry(weapon)
            .or_insert_with(|| load(weapon))
            .clone()
    }

    fn play(&mut self, weapon: WeaponId, muzzle: Vector3, distance: f32, indoors: bool) {
        let Some(gun) = self.sounds(weapon) else {
            return;
        };
        let pitch = randf_range(0.97, 1.03) as f32 * gun.pitch;
        let (near, mid, far) = layer_weights(distance);
        acoustics::request_duck(acoustics::duck_for_distance(distance));
        let gain = |w: f32| 20.0 * w.max(1e-4).log10();
        let tail = if indoors {
            gun.tail_indoor.as_ref().or(gun.tail.as_ref())
        } else {
            gun.tail.as_ref()
        };
        let layers = [
            (&mut self.snap, Some(&gun.snap), gun.levels[0], near),
            (&mut self.body, gun.body.as_ref(), gun.levels[1], near),
            (&mut self.tail, tail, gun.levels[2], near),
            (
                &mut self.mid,
                gun.mid.as_ref(),
                gun.volume + MID_DB + distance_db(distance, MID_REFERENCE),
                mid,
            ),
            (
                &mut self.far,
                gun.far.as_ref(),
                gun.volume + FAR_DB + distance_db(distance, FAR_REFERENCE),
                far,
            ),
        ];
        for (player, stream, level, weight) in layers {
            let Some(stream) = stream else { continue };
            if weight < 0.02 {
                continue;
            }
            player.set_global_position(muzzle);
            // Changing the stream would cut off shots still ringing.
            if player.get_stream().map(|s| s.instance_id()) != Some(stream.instance_id()) {
                player.set_stream(stream);
            }
            let level = level + gain(weight);
            player.set_volume_db(level);
            // Never louder than the gun's own level up close.
            player.set_max_db(level.max(-80.0));
            player.set_pitch_scale(pitch);
            player.play();
        }
        self.shots_played += 1;
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
    let key = borrowed.unwrap_or(own);
    let dir = format!("res://assets/audio/weapons/{key}");
    let layer = |name: &str| audio::variations(&format!("{dir}/{name}"), 1.0);
    // `WeaponView`'s defaults, for scenes that leave them unset.
    let volume = float("fire_volume_db", -6.0) + REMOTE_DB;
    Some(GunSounds {
        snap: layer("snap")?,
        body: layer("body"),
        tail: layer("tail"),
        tail_indoor: layer("tail_indoor"),
        mid: layer("mid"),
        far: layer("far"),
        levels: [
            volume + float("snap_db", 2.0),
            volume + float("body_db", 0.0),
            volume + float("tail_db", -3.0),
        ],
        volume,
        pitch: float("fire_pitch", 1.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_cross_fade_with_distance() {
        assert_eq!(layer_weights(10.0), (1.0, 0.0, 0.0));
        let (near, mid, far) = layer_weights(50.0);
        assert!(near > 0.2 && near < 0.9 && mid > 0.99 && far == 0.0);
        let (near, mid, far) = layer_weights(130.0);
        assert!(near == 0.0 && mid > 0.9 && far > 0.2);
        let (near, mid, far) = layer_weights(300.0);
        assert!(near == 0.0 && mid == 0.0 && far == 1.0);
    }

    #[test]
    fn distant_layers_fall_six_db_per_doubling() {
        assert_eq!(distance_db(20.0, 40.0), 0.0);
        assert!((distance_db(80.0, 40.0) + 6.0).abs() < 1e-4);
        assert!((distance_db(160.0, 40.0) + 12.0).abs() < 1e-4);
    }
}
