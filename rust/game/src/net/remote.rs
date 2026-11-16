//! Another player as this client sees them: interpolated from snapshots,
//! drawn as a placeholder body with a gun, a muzzle flash, and gunfire sound.
//! (A rigged third-person character replaces the placeholder later.)

use std::collections::HashMap;

use godot::classes::{
    AudioStream, AudioStreamPlayer3D, INode3D, Label3D, Node3D, OmniLight3D, ResourceLoader,
};
use godot::prelude::*;
use shooter_sim::Vec3;
use shooter_sim::movement::{MoveParams, Stance};
use shooter_sim::net::interpolation::{Interpolate, lerp_angle};
use shooter_sim::net::protocol::PlayerView;
use shooter_sim::weapon::{Slot, WeaponId};

use crate::convert::to_godot;

/// One remote player's state at a snapshot, ready to blend.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RemoteSample {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub stance: Stance,
    pub sprinting: bool,
    pub ads: f32,
    pub weapon: WeaponId,
    pub reloading: bool,
    pub alive: bool,
    pub shots_fired: u32,
}

impl From<&PlayerView> for RemoteSample {
    fn from(v: &PlayerView) -> Self {
        Self {
            position: v.position(),
            yaw: v.yaw(),
            pitch: v.pitch(),
            stance: v.stance,
            sprinting: v.sprinting,
            ads: v.ads(),
            weapon: v.weapon,
            reloading: v.reloading,
            alive: v.alive,
            shots_fired: v.shots_fired,
        }
    }
}

impl Interpolate for RemoteSample {
    fn interpolate(&self, to: &Self, t: f32) -> Self {
        // Respawns and other teleports jump instead of sliding across the map.
        if self.alive != to.alive || self.position.distance(to.position) > 2.0 {
            return if t < 0.5 { *self } else { *to };
        }
        let discrete = if t < 0.5 { self } else { to };
        Self {
            position: self.position.lerp(to.position, t),
            yaw: lerp_angle(self.yaw, to.yaw, t),
            pitch: self.pitch + (to.pitch - self.pitch) * t,
            ads: self.ads + (to.ads - self.ads) * t,
            // Shots show once the snapshot that contains them is reached.
            shots_fired: self.shots_fired,
            ..*discrete
        }
    }
}

#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct RemotePlayer {
    base: Base<Node3D>,

    /// Scaled vertically for crouching.
    #[init(node = "Body")]
    body: OnReady<Gd<Node3D>>,
    /// At eye height; pitches with the view.
    #[init(node = "Head")]
    head: OnReady<Gd<Node3D>>,
    #[init(node = "Head/Gun")]
    gun: OnReady<Gd<Node3D>>,
    #[init(node = "Head/Gun/Flash")]
    flash: OnReady<Gd<OmniLight3D>>,
    #[init(node = "Head/Gun/Shot")]
    shot: OnReady<Gd<AudioStreamPlayer3D>>,
    #[init(node = "Name")]
    name_label: OnReady<Gd<Label3D>>,

    params: MoveParams,
    eye_height: f32,
    shots_fired: Option<u32>,
    flash_energy: f32,
    /// Falls over when dead: 0 standing, 1 down.
    death_blend: f32,
    fire_sounds: HashMap<WeaponId, Option<Gd<AudioStream>>>,
}

#[godot_api]
impl INode3D for RemotePlayer {
    fn ready(&mut self) {
        self.eye_height = self.params.eye_height(Stance::Standing);
        self.flash
            .set_param(godot::classes::light_3d::Param::ENERGY, 0.0);
    }
}

#[godot_api]
impl RemotePlayer {
    /// Last rendered feet position, for tests and debugging.
    #[func]
    pub fn feet(&self) -> Vector3 {
        self.base().get_global_position()
    }
}

impl RemotePlayer {
    pub fn set_display_name(&mut self, name: &str) {
        self.name_label.set_text(name);
    }

    pub fn apply(&mut self, s: &RemoteSample, delta: f32) {
        self.base_mut().set_global_position(to_godot(s.position));
        self.base_mut().set_rotation(Vector3::new(0.0, s.yaw, 0.0));
        let down = if s.alive { 0.0 } else { 1.0 };
        self.death_blend += (down - self.death_blend) * (1.0 - (-6.0 * delta).exp());

        let height = self.params.height(s.stance);
        let target_eye = self.params.eye_height(s.stance);
        self.eye_height += (target_eye - self.eye_height) * (1.0 - (-14.0 * delta).exp());
        self.body
            .set_scale(Vector3::new(1.0, height / self.params.stand_height, 1.0));
        self.body
            .set_rotation(Vector3::new(-self.death_blend * 1.4, 0.0, 0.0));
        let eye = self.eye_height * (1.0 - 0.8 * self.death_blend);
        self.head.set_position(Vector3::new(0.0, eye, 0.0));
        self.head.set_rotation(Vector3::new(s.pitch, 0.0, 0.0));

        // Gun pose: shouldered when aiming, lowered when sprinting or reloading.
        let lowered = if s.sprinting || s.reloading { 1.0 } else { 0.0 };
        let x = 0.22 - 0.2 * s.ads;
        self.gun
            .set_position(Vector3::new(x, -0.22 + 0.08 * s.ads, -0.25));
        self.gun
            .set_rotation(Vector3::new(-0.7 * lowered, 0.4 * lowered, 0.0));

        let fired = self.shots_fired.is_some_and(|last| s.shots_fired > last);
        self.shots_fired = Some(s.shots_fired);
        if fired {
            self.flash_energy = 3.0;
            if let Some(sound) = self.fire_sound(s.weapon) {
                self.shot.set_stream(&sound);
                self.shot.play();
            }
        }
        self.flash_energy = (self.flash_energy - delta * 40.0).max(0.0);
        let energy = self.flash_energy;
        self.flash
            .set_param(godot::classes::light_3d::Param::ENERGY, energy);
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
                [def.key.as_str(), fallback]
                    .into_iter()
                    .map(|key| format!("res://assets/audio/weapons/{key}_fire.ogg"))
                    .find(|path| loader.exists(path))
                    .and_then(|path| loader.load(&path))
                    .and_then(|r| r.try_cast::<AudioStream>().ok())
            })
            .clone()
    }
}
