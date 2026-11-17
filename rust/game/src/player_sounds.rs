//! The local player's body, heard from inside it: breathing after a sprint
//! and when hurt, the held breath on a scope, a heartbeat and a muffled world
//! at low health, the jolt of being hit, ringing ears after a heavy hit, the
//! fall when killed, the push-off of a jump, hands on a ledge, and the kit
//! rattling on every step.
//!
//! Breath, heart, and hurt play on the `Player` bus, which the health muffle
//! leaves clear: as health drops, a low-pass closes on the world's buses
//! (Weapons, Effects, Footsteps, Ambience) toward 1.2 kHz, so the heartbeat
//! and breath come forward. Dying sweeps it shut; respawning opens it.
//!
//! The local player adds one `PlayerSounds` (clients only) and feeds it a
//! [`BodyFrame`] every tick plus the events that matter.

use std::collections::HashMap;

use godot::classes::{
    AudioEffectLowPassFilter, AudioServer, AudioStreamPlayer, AudioStreamRandomizer, INode, Node,
};
use godot::prelude::*;

use crate::audio;
use crate::footsteps::Gait;

/// The bus breath, heartbeat, and the sound of being hit play on.
pub const BUS: &str = "Player";
/// Buses the health muffle closes on, and its effect's name.
const MUFFLED_BUSES: [&str; 4] = ["Weapons", "Effects", "Footsteps", "Ambience"];
const MUFFLE_NAME: &str = "HealthMuffle";
/// The muffle's cutoff: open at and above this share of health, closing to
/// `MUFFLE_LOW_HZ` at `MUFFLE_FLOOR`.
const MUFFLE_START: f32 = 0.5;
const MUFFLE_FLOOR: f32 = 0.08;
const MUFFLE_OPEN_HZ: f32 = 20_000.0;
const MUFFLE_LOW_HZ: f32 = 1_200.0;
const MUFFLE_DEAD_HZ: f32 = 350.0;
/// How fast the muffle follows (closing, then opening), per second.
const MUFFLE_CLOSE_RATE: f32 = 6.0;
const MUFFLE_OPEN_RATE: f32 = 1.25;
/// A heartbeat below this share of health.
const HEART_START: f32 = 0.4;
/// Hurt breathing below this share of health.
const HURT_BREATH: f32 = 0.35;
/// Seconds of sprinting before breathing is heard, and the most a sprint
/// leaves behind to recover from.
const SPRINT_BREATH_AFTER: f32 = 2.5;
const RECOVERY_MAX: f32 = 4.0;
/// Damage in one hit that rings the ears.
const TINNITUS_DAMAGE: f32 = 45.0;
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
    pub holding_breath: bool,
}

/// The muffle cutoff (Hz) for a share of health.
pub fn muffle_cutoff(health: f32) -> f32 {
    if health >= MUFFLE_START {
        return MUFFLE_OPEN_HZ;
    }
    let t = ((MUFFLE_START - health) / (MUFFLE_START - MUFFLE_FLOOR)).clamp(0.0, 1.0);
    // Geometric, so it closes evenly to the ear.
    MUFFLE_OPEN_HZ * (MUFFLE_LOW_HZ / MUFFLE_OPEN_HZ).powf(t)
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
    ears: Option<Gd<AudioStreamPlayer>>,
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
    /// The muffle's cutoff now (Hz).
    #[init(val = MUFFLE_OPEN_HZ)]
    cutoff: f32,
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
        self.ears = Some(self.player("Ears", BUS));
        self.body = Some(self.player("BodyMoves", "Footsteps"));
        self.gear = Some(self.player("StepGear", "Footsteps"));
        self.last.alive = true;
        self.last.health = 1.0;
        self.last.on_floor = true;
    }

    fn exit_tree(&mut self) {
        set_muffle(MUFFLE_OPEN_HZ);
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

    /// The muffle's cutoff now, in Hz (tests).
    #[func]
    fn muffle_hz(&self) -> f32 {
        self.cutoff
    }
}

impl PlayerSounds {
    /// Advances by one tick of `dt` seconds.
    pub fn update(&mut self, frame: BodyFrame, dt: f32) {
        let last = std::mem::replace(&mut self.last, frame);
        if !frame.alive {
            self.approach_muffle(MUFFLE_DEAD_HZ, dt);
            self.sprint_time = 0.0;
            self.recovery = 0.0;
            return;
        }

        // A jump's push-off, and hands on a ledge.
        if last.on_floor && !frame.on_floor && frame.rising > 1.5 {
            self.play_on(Slot::Body, "res://assets/audio/player/jump", -6.0);
        }
        if frame.mantling && !last.mantling {
            self.play_on(Slot::Body, "res://assets/audio/player/mantle", -4.0);
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
        self.approach_muffle(muffle_cutoff(frame.health), dt);
    }

    /// The player was hit for `amount` (`fall` for a hard landing).
    pub fn on_damaged(&mut self, amount: f32, fall: bool) {
        let level = if fall { -8.0 } else { -2.0 };
        self.play_on(Slot::Hurt, "res://assets/audio/player/hit_taken", level);
        if !fall && amount >= TINNITUS_DAMAGE {
            self.play_on(Slot::Ears, "res://assets/audio/player/tinnitus", -16.0);
        }
        // The muffle snaps toward the new health at once (the update closes
        // it the rest of the way), so a big hit is felt.
        self.cutoff = self.cutoff.min(muffle_cutoff(self.last.health) * 1.5);
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

    fn approach_muffle(&mut self, target: f32, dt: f32) {
        let rate = if target < self.cutoff {
            MUFFLE_CLOSE_RATE
        } else {
            MUFFLE_OPEN_RATE
        };
        // In the log domain, so it moves evenly to the ear.
        let t = 1.0 - (-rate * dt).exp();
        let next = (self.cutoff.ln() + (target.ln() - self.cutoff.ln()) * t).exp();
        if (next - self.cutoff).abs() > 1.0 || (target - next).abs() < 1.0 {
            self.cutoff = next;
            set_muffle(next);
        }
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
            Slot::Ears => &mut self.ears,
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
    Ears,
    Body,
    Gear,
}

/// Sets the health muffle's cutoff on the world's buses (adding the
/// low-pass the first time), and bypasses it when fully open.
fn set_muffle(cutoff: f32) {
    let mut server = AudioServer::singleton();
    let open = cutoff >= MUFFLE_OPEN_HZ - 1.0;
    for bus in MUFFLED_BUSES {
        let index = server.get_bus_index(bus);
        if index < 0 {
            continue;
        }
        let existing = (0..server.get_bus_effect_count(index)).find(|&i| {
            server
                .get_bus_effect(index, i)
                .is_some_and(|e| e.get_name() == MUFFLE_NAME)
        });
        let slot = match existing {
            Some(slot) => slot,
            None if open => continue,
            None => {
                let mut effect = AudioEffectLowPassFilter::new_gd();
                effect.set_name(MUFFLE_NAME);
                server.add_bus_effect(index, &effect);
                server.get_bus_effect_count(index) - 1
            }
        };
        if let Some(mut effect) = server
            .get_bus_effect(index, slot)
            .and_then(|e| e.try_cast::<AudioEffectLowPassFilter>().ok())
        {
            effect.set_cutoff(cutoff);
        }
        server.set_bus_effect_enabled(index, slot, !open);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_muffle_closes_as_health_drops() {
        assert_eq!(muffle_cutoff(1.0), MUFFLE_OPEN_HZ);
        assert_eq!(muffle_cutoff(0.5), MUFFLE_OPEN_HZ);
        assert!(muffle_cutoff(0.3) < 8_000.0 && muffle_cutoff(0.3) > MUFFLE_LOW_HZ);
        assert!((muffle_cutoff(0.05) - MUFFLE_LOW_HZ).abs() < 1.0);
    }

    #[test]
    fn the_heart_races_near_death() {
        assert_eq!(heart_interval(0.8), None);
        let slow = heart_interval(0.35).unwrap_or(0.0);
        let fast = heart_interval(0.05).unwrap_or(1.0);
        assert!(fast < slow && fast > 0.5 && slow < 1.0);
    }
}
