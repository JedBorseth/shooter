//! The local player's body, heard from inside it: breathing after a sprint
//! and when hurt, the held breath on a scope, a heartbeat at low health, a
//! soft thud when hit, the fall when killed, the push-off of a jump, hands on
//! a ledge or a hand planted on cover, and the kit rattling on every step.
//!
//! Breath, heart, and hurt play on the `Player` bus. Being hurt never dulls
//! the world: no ringing ears and no low-pass on the other buses, so you can
//! still hear where the fight is.
//!
//! The local player adds one `PlayerSounds` (clients only) and feeds it a
//! [`BodyFrame`] every tick plus the events that matter.

use std::collections::HashMap;

use godot::classes::{AudioStreamPlayer, AudioStreamRandomizer, INode, Node};
use godot::prelude::*;

use crate::audio;
use crate::footsteps::Gait;

/// The bus breath, heartbeat, and the sound of being hit play on.
pub const BUS: &str = "Player";
/// A heartbeat below this share of health.
const HEART_START: f32 = 0.4;
/// Hurt breathing below this share of health.
const HURT_BREATH: f32 = 0.35;
/// Seconds of sprinting before breathing is heard, and the most a sprint
/// leaves behind to recover from.
const SPRINT_BREATH_AFTER: f32 = 2.5;
const RECOVERY_MAX: f32 = 4.0;
/// The thud of being hit by a round, and of a hard landing, in dB.
const HIT_TAKEN_DB: f32 = -10.0;
const FALL_TAKEN_DB: f32 = -14.0;
/// Step layers below the footstep, in dB from the step's own level.
const GEAR_SPRINT_DB: f32 = -10.0;
const GEAR_WALK_DB: f32 = -16.0;
const CLOTH_SPRINT_DB: f32 = -13.0;

/// One tick of the player's state, as far as their body's sounds go.
#[derive(Clone, Copy, Debug, Default)]
pub struct BodyFrame {
    pub alive: bool,
    /// Health, 0 to 1.
    pub health: f32,
    pub sprinting: bool,
    pub on_floor: bool,
    /// Upward speed, m/s.
    pub rising: f32,
    pub mantling: bool,
    /// The climb is a vault over cover rather than a mantle onto a ledge.
    pub vaulting: bool,
    pub holding_breath: bool,
}

/// The sound of starting a climb: hands on a ledge, or a hand planted on
/// cover to vault it. Other players' climbs (`footsteps::Foley`) use it too.
pub fn climb_sound(vaulting: bool) -> &'static str {
    if vaulting {
        "res://assets/audio/player/vault"
    } else {
        "res://assets/audio/player/mantle"
    }
}

/// Seconds between heartbeats at a share of health, or `None` above the
/// threshold: slow at first, racing near death.
pub fn heart_interval(health: f32) -> Option<f32> {
    (health < HEART_START && health > 0.0).then(|| {
        let t = (health / HEART_START).clamp(0.0, 1.0);
        0.55 + 0.4 * t
    })
}

#[derive(GodotClass)]
#[class(init, base = Node)]
pub struct PlayerSounds {
    breath: Option<Gd<AudioStreamPlayer>>,
    heart: Option<Gd<AudioStreamPlayer>>,
    hurt: Option<Gd<AudioStreamPlayer>>,
    body: Option<Gd<AudioStreamPlayer>>,
    gear: Option<Gd<AudioStreamPlayer>>,
    sounds: HashMap<String, Option<Gd<AudioStreamRandomizer>>>,
    last: BodyFrame,
    /// Seconds sprinting, and of recovery breathing left after a sprint.
    sprint_time: f32,
    recovery: f32,
    /// Seconds to the next breath and heartbeat, and whether the next breath
    /// is in or out.
    next_breath: f32,
    next_beat: f32,
    inhale: bool,
    /// Counts (tests).
    breaths: u32,
    beats: u32,
    base: Base<Node>,
}

#[godot_api]
impl INode for PlayerSounds {
    fn ready(&mut self) {
        self.breath = Some(self.player("Breath", BUS));
        self.heart = Some(self.player("Heart", BUS));
        self.hurt = Some(self.player("Hurt", BUS));
        self.body = Some(self.player("BodyMoves", "Footsteps"));
        self.gear = Some(self.player("StepGear", "Footsteps"));
        self.last.alive = true;
        self.last.health = 1.0;
        self.last.on_floor = true;
    }
}

#[godot_api]
impl PlayerSounds {
    /// Breaths and heartbeats played so far (tests).
    #[func]
    fn breaths_heard(&self) -> u32 {
        self.breaths
    }

    #[func]
    fn heartbeats_heard(&self) -> u32 {
        self.beats
    }
}

impl PlayerSounds {
    /// Advances by one tick of `dt` seconds.
    pub fn update(&mut self, frame: BodyFrame, dt: f32) {
        let last = std::mem::replace(&mut self.last, frame);
        if !frame.alive {
            self.sprint_time = 0.0;
            self.recovery = 0.0;
            return;
        }

        // A jump's push-off, and hands on a ledge (a mantle) or one hand
        // planted on cover (a vault).
        if last.on_floor && !frame.on_floor && frame.rising > 1.5 {
            self.play_on(Slot::Body, "res://assets/audio/player/jump", -6.0);
        }
        if frame.mantling && !last.mantling {
            self.play_on(Slot::Body, climb_sound(frame.vaulting), -4.0);
        }

        // The held breath.
        if frame.holding_breath && !last.holding_breath {
            self.play_on(Slot::Breath, "res://assets/audio/foley/breath_hold", -12.0);
            self.next_breath = 1.2;
        } else if !frame.holding_breath && last.holding_breath {
            self.play_on(
                Slot::Breath,
                "res://assets/audio/foley/breath_release",
                -11.0,
            );
            self.next_breath = 1.0;
        }

        self.breathe(frame, dt);
        self.heartbeat(frame, dt);
    }

    /// The player was hit (`fall` for a hard landing).
    pub fn on_damaged(&mut self, fall: bool) {
        let level = if fall { FALL_TAKEN_DB } else { HIT_TAKEN_DB };
        self.play_on(Slot::Hurt, "res://assets/audio/player/hit_taken", level);
    }

    /// The player died.
    pub fn on_died(&mut self) {
        self.play_on(Slot::Hurt, "res://assets/audio/player/death", -3.0);
        if let Some(breath) = &mut self.breath {
            breath.stop();
        }
    }

    /// A footstep at `volume_db` in `gait`: the kit and cloth under it.
    pub fn on_step(&mut self, gait: Gait, volume_db: f32) {
        let gear = match gait {
            Gait::Sprint => Some(GEAR_SPRINT_DB),
            Gait::Walk => Some(GEAR_WALK_DB),
            Gait::Quiet | Gait::Crouch => None,
        };
        if let Some(offset) = gear {
            self.play_on(
                Slot::Gear,
                "res://assets/audio/player/step_gear",
                volume_db + offset,
            );
        }
        if gait == Gait::Sprint {
            self.play_on(
                Slot::Body,
                "res://assets/audio/player/step_cloth",
                volume_db + CLOTH_SPRINT_DB,
            );
        }
    }

    fn breathe(&mut self, frame: BodyFrame, dt: f32) {
        if frame.sprinting {
            self.sprint_time += dt;
            self.recovery = (self.sprint_time * 0.6).min(RECOVERY_MAX);
        } else {
            self.sprint_time = 0.0;
            self.recovery = (self.recovery - dt).max(0.0);
        }
        let hurt = frame.health < HURT_BREATH;
        let winded =
            self.sprint_time > SPRINT_BREATH_AFTER || (!frame.sprinting && self.recovery > 0.0);
        if frame.holding_breath || !(hurt || winded) {
            self.next_breath = self.next_breath.min(0.3);
            return;
        }
        self.next_breath -= dt;
        if self.next_breath > 0.0 {
            return;
        }
        let (base, period, level) = if hurt {
            let t = (frame.health / HURT_BREATH).clamp(0.0, 1.0);
            ("breath_hurt", 0.9 + 0.3 * t, -10.0 - 4.0 * t)
        } else if frame.sprinting {
            ("breath", 0.42, -15.0)
        } else {
            // Recovering: slower and quieter as it settles.
            let t = (self.recovery / RECOVERY_MAX).clamp(0.0, 1.0);
            ("breath", 0.75 - 0.25 * t, -20.0 + 5.0 * t)
        };
        let half = if self.inhale { "in" } else { "out" };
        let path = format!("res://assets/audio/player/{base}_{half}");
        self.play_on(Slot::Breath, &path, level);
        self.inhale = !self.inhale;
        self.next_breath = period * if self.inhale { 1.0 } else { 0.85 };
        self.breaths += 1;
    }

    fn heartbeat(&mut self, frame: BodyFrame, dt: f32) {
        let Some(interval) = heart_interval(frame.health) else {
            self.next_beat = 0.0;
            return;
        };
        self.next_beat -= dt;
        if self.next_beat > 0.0 {
            return;
        }
        let t = (frame.health / HEART_START).clamp(0.0, 1.0);
        self.play_on(
            Slot::Heart,
            "res://assets/audio/player/heartbeat",
            -6.0 - 8.0 * t,
        );
        self.next_beat = interval;
        self.beats += 1;
    }

    fn player(&mut self, name: &str, bus: &str) -> Gd<AudioStreamPlayer> {
        let mut player = AudioStreamPlayer::new_alloc();
        player.set_name(name);
        player.set_bus(bus);
        player.set_max_polyphony(2);
        self.base_mut().add_child(&player);
        player
    }

    fn play_on(&mut self, slot: Slot, base: &str, volume_db: f32) {
        let stream = self
            .sounds
            .entry(base.to_owned())
            .or_insert_with(|| audio::variations(base, 1.06))
            .clone();
        let Some(stream) = stream else { return };
        let player = match slot {
            Slot::Breath => &mut self.breath,
            Slot::Heart => &mut self.heart,
            Slot::Hurt => &mut self.hurt,
            Slot::Body => &mut self.body,
            Slot::Gear => &mut self.gear,
        };
        if let Some(player) = player {
            player.set_stream(&stream);
            player.set_volume_db(volume_db);
            player.play();
        }
    }
}

#[derive(Clone, Copy)]
enum Slot {
    Breath,
    Heart,
    Hurt,
    Body,
    Gear,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vault_has_its_own_sound() {
        assert!(climb_sound(true).ends_with("/vault"));
        assert!(climb_sound(false).ends_with("/mantle"));
    }

    #[test]
    fn the_heart_races_near_death() {
        assert_eq!(heart_interval(0.8), None);
        let slow = heart_interval(0.35).unwrap_or(0.0);
        let fast = heart_interval(0.05).unwrap_or(1.0);
        assert!(fast < slow && fast > 0.5 && slow < 1.0);
    }
}
