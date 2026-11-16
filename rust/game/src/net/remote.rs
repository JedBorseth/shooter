//! Another player as this client sees them: interpolated from snapshots and
//! drawn as the third-person soldier (`SoldierRig`), with gunfire sound at the
//! muzzle and hit zones posed from the same snapshot (drawn with F4).
//!
//! The local player's shots hit these hit zones for instant feedback (hit
//! markers, blood, a flinch), but deal no damage here: the server rewinds the
//! same poses (`shooter_sim::net::lag`) and decides every hit and kill.

use std::collections::HashMap;

use godot::classes::{AudioStream, AudioStreamPlayer3D, INode3D, Label3D, Node3D, ResourceLoader};
use godot::prelude::*;
pub use shooter_sim::net::lag::RemoteSample;
use shooter_sim::weapon::{Reload, Slot, WeaponId};

use crate::character::rig::{SoldierFrame, SoldierRig};
use crate::convert::to_godot;
use crate::hitbox_rig::HitboxRig;

#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct RemotePlayer {
    base: Base<Node3D>,

    #[init(node = "Soldier")]
    rig: OnReady<Gd<SoldierRig>>,
    #[init(node = "Hitboxes")]
    hitboxes: OnReady<Gd<HitboxRig>>,
    #[init(node = "Shot")]
    shot: OnReady<Gd<AudioStreamPlayer3D>>,
    #[init(node = "Name")]
    name_label: OnReady<Gd<Label3D>>,

    shots_fired: Option<u32>,
    /// Last drawn feet position and the velocity estimated from it.
    last_position: Option<Vector3>,
    velocity: Vector3,
    /// Seconds since the current reload started.
    reload_time: Option<f32>,
    fire_sounds: HashMap<WeaponId, Option<Gd<AudioStream>>>,
}

#[godot_api]
impl INode3D for RemotePlayer {}

#[godot_api]
impl RemotePlayer {
    /// Last rendered feet position, for tests and debugging.
    #[func]
    pub fn feet(&self) -> Vector3 {
        self.base().get_global_position()
    }

    /// The character, for tests.
    #[func]
    fn soldier(&self) -> Gd<SoldierRig> {
        self.rig.clone()
    }

    /// The hit zones, for tests.
    #[func]
    fn hitboxes(&self) -> Gd<HitboxRig> {
        self.hitboxes.clone()
    }

    /// Called by the local player's hitscan. The server owns health, so this
    /// never kills here; the kill arrives from the server.
    #[func]
    fn apply_damage(&mut self, _amount: f32, _headshot: bool) -> bool {
        false
    }

    /// A small predicted flinch away from the local player's hit.
    #[func]
    pub fn react_to_hit(&mut self, direction: Vector3, killed: bool) {
        self.rig.bind_mut().hit(direction, killed);
    }
}

impl RemotePlayer {
    pub fn set_display_name(&mut self, name: &str) {
        self.name_label.set_text(name);
    }

    pub fn apply(&mut self, s: &RemoteSample, delta: f32) {
        let position = to_godot(s.position);
        self.base_mut().set_global_position(position);

        // Velocity from the drawn motion (snapshots carry no velocity). A jump
        // in position (respawn) restarts the estimate.
        if let Some(last) = self.last_position
            && delta > 0.0
        {
            let step = position - last;
            if step.length() < 2.0 {
                let measured = step / delta;
                self.velocity += (measured - self.velocity) * (1.0 - (-12.0 * delta).exp());
            } else {
                self.velocity = Vector3::ZERO;
            }
        }
        self.last_position = Some(position);

        self.reload_time = match (s.reloading && s.alive, self.reload_time) {
            (true, Some(t)) => Some(t + delta),
            (true, None) => Some(0.0),
            (false, _) => None,
        };
        let reload = self
            .reload_time
            .map(|t| (t / reload_duration(s.weapon)).clamp(0.0, 1.0));

        let frame = SoldierFrame {
            velocity: self.velocity,
            on_floor: s.on_floor,
            stance: s.stance,
            sprinting: s.sprinting,
            yaw: s.yaw,
            pitch: s.pitch,
            ads: s.ads,
            weapon: Some(s.weapon),
            reload,
            alive: s.alive,
        };
        self.rig.bind_mut().update(&frame, delta);
        // The server rewinds exactly this pose when it checks our shots.
        self.hitboxes.bind_mut().set_pose(s.hitbox_pose());

        let fired = self.shots_fired.is_some_and(|last| s.shots_fired > last);
        self.shots_fired = Some(s.shots_fired);
        if fired && s.alive {
            self.rig.bind_mut().fire();
            let muzzle = self.rig.bind().muzzle_position();
            self.shot.set_global_position(muzzle);
            if let Some(sound) = self.fire_sound(s.weapon) {
                self.shot.set_stream(&sound);
                self.shot.play();
            }
        }
    }

    /// `assets/audio/weapons/<key>_fire.ogg`, or a stand-in for its class.
    fn fire_sound(&mut self, weapon: WeaponId) -> Option<Gd<AudioStream>> {
        self.fire_sounds
            .entry(weapon)
            .or_insert_with(|| {
                let def = weapon.def();
                let fallback = match def.slot {
                    Slot::Secondary => "pistol",
                    _ => "m4",
                };
                let mut loader = ResourceLoader::singleton();
                [def.key.as_str(), fallback, "ak47"]
                    .into_iter()
                    .map(|key| format!("res://assets/audio/weapons/{key}_fire.ogg"))
                    .find(|path| loader.exists(path))
                    .and_then(|path| loader.load(&path))
                    .and_then(|r| r.try_cast::<AudioStream>().ok())
            })
            .clone()
    }
}

/// About how long a weapon's reload animation runs, in seconds. Snapshots only
/// say whether a player is reloading, so the third-person reload paces itself.
fn reload_duration(weapon: WeaponId) -> f32 {
    match weapon.def().reload {
        Reload::Magazine { time, .. } => time,
        Reload::PerRound {
            start,
            per_round,
            end,
        } => start + per_round * 4.0 + end,
    }
    .max(0.3)
}
