//! Weapon handling sounds, beat by beat: the magazine out and in, the bolt
//! release or charging handle, the pump, the bolt, each shell, the draw, the
//! holster, raising to the eye, and the inspect.
//!
//! `WeaponView` says *when* with its `foley_event(name)` signal; this module
//! decides *what it sounds like*. Every first-person view gets a
//! [`WeaponFoley`] child that listens to that signal, so the reload timeline
//! can move its emit points without touching any sound code.
//!
//! The recordings (`tools/build_weapon_audio.py foley`) live per weapon in
//! `assets/audio/weapons/<key>/foley/<event>_N.ogg`, and the layers every
//! weapon shares (cloth, kit rattle, breath) in `assets/audio/foley/`. Each
//! take is cut so its loudest hit sits [`lead`] seconds in: an emitter that
//! knows when the part makes contact fires the event that long before, and
//! the hit lands on the frame.
//!
//! Foley plays on its own `WeaponFoley` bus, with a fixed pool of voices.
//! That bus sends into Effects (its volume setting and room reverb), not
//! Weapons, so a reload never ducks the ambience the way gunfire does.

use std::collections::HashMap;

use godot::classes::{AudioStreamPlayer, AudioStreamRandomizer, INode, Node};
use godot::prelude::*;

use crate::audio;

/// The bus every handling sound plays on.
pub const BUS: &str = "WeaponFoley";
/// The same event twice within this many seconds plays once.
const REPEAT_GAP: f32 = 0.2;
/// Voices per weapon view: a reload's beats overlap at most two or three deep.
const VOICES: usize = 4;

/// Every event `WeaponView::foley_event` emits, as the contract with the
/// reload animation names them, plus `dry` (dry fire) and `pickup`.
pub const EVENTS: [&str; 20] = [
    "mag_out",
    "mag_in",
    "bolt_release",
    "charge",
    "shell_in",
    "pump_back",
    "pump_forward",
    "bolt_up",
    "bolt_back",
    "bolt_forward",
    "bolt_down",
    "draw",
    "holster",
    "ads_in",
    "ads_out",
    "inspect",
    "dry",
    "pickup",
    "mount",
    "unmount",
];

/// Seconds from the start of an event's sound to its main hit (the magazine
/// seating, the bolt slamming home), as `tools/build_weapon_audio.py` cuts
/// them (`FOLEY_ALIGN`). An emitter fires the event this long before the
/// contact it animates. Events without a hit (cloth, draw) start at once.
pub fn lead(event: &str) -> f32 {
    match event {
        "mag_out" => 0.06,
        "mag_in" => 0.14,
        "bolt_release" => 0.02,
        "charge" => 0.22,
        "shell_in" => 0.12,
        "pump_back" | "pump_forward" | "bolt_back" | "bolt_forward" => 0.08,
        "bolt_up" => 0.05,
        "bolt_down" => 0.04,
        _ => 0.0,
    }
}

/// One sound an event plays: a file set, its level, and a delay.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Layer {
    /// `weapon/<event>` or `shared/<name>`.
    source: Source,
    level_db: f32,
    delay: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Source {
    /// This weapon's recording of the event, or of the fallback event.
    Weapon(&'static str, Option<&'static str>),
    Shared(&'static str),
}

/// Level of the handling sounds against the gun (whose own layers sit around
/// -6 dB): present, as they are right at the ear, but well under a shot.
const BASE_DB: f32 = -11.0;

/// The layers of each event.
fn layers(event: &str) -> Vec<Layer> {
    const fn w(name: &'static str, level_db: f32) -> Layer {
        Layer {
            source: Source::Weapon(name, None),
            level_db,
            delay: 0.0,
        }
    }
    const fn s(name: &'static str, level_db: f32, delay: f32) -> Layer {
        Layer {
            source: Source::Shared(name),
            level_db,
            delay,
        }
    }
    match event {
        // The magazine out, then the hand to the pouch for the next one.
        "mag_out" => vec![w("mag_out", 0.0), s("pouch", -9.0, 0.32)],
        "mag_in" => vec![w("mag_in", 1.0)],
        // Guns without a bolt release are charged instead, and the other way round.
        "bolt_release" => vec![Layer {
            source: Source::Weapon("bolt_release", Some("charge")),
            level_db: 1.5,
            delay: 0.0,
        }],
        "charge" => vec![Layer {
            source: Source::Weapon("charge", Some("bolt_release")),
            level_db: 1.0,
            delay: 0.0,
        }],
        "shell_in" => vec![w("shell_in", -1.0)],
        "pump_back" => vec![w("pump_back", 2.0)],
        "pump_forward" => vec![w("pump_forward", 3.0)],
        "bolt_up" => vec![w("bolt_up", 0.0)],
        "bolt_back" => vec![w("bolt_back", 1.0)],
        "bolt_forward" => vec![w("bolt_forward", 2.0)],
        "bolt_down" => vec![w("bolt_down", 0.0)],
        "draw" => vec![s("draw", -4.0, 0.0)],
        "holster" => vec![s("holster", -6.0, 0.0)],
        "ads_in" => vec![s("ads_in", -10.0, 0.0)],
        "ads_out" => vec![s("ads_out", -13.0, 0.0)],
        "inspect" => vec![s("inspect", -6.0, 0.0)],
        "dry" => vec![w("dry", 0.0)],
        "pickup" => vec![s("pouch", -4.0, 0.0), s("pouch", -8.0, 0.12)],
        // The handguard set down on cover, and the gear shifting with the
        // body; lifted off, a softer brush. Shared handling sounds until the
        // audio pass records contact foley for it.
        "mount" => vec![s("ads_in", -3.0, 0.0), s("pouch", -12.0, 0.04)],
        "unmount" => vec![s("ads_out", -9.0, 0.0)],
        _ => vec![],
    }
}

/// The path of a layer's file set, before `_N`.
fn path(weapon: &str, event: &str, shared: bool) -> String {
    if shared {
        format!("res://assets/audio/foley/{event}")
    } else {
        format!("res://assets/audio/weapons/{weapon}/foley/{event}")
    }
}

/// Plays one first-person weapon's handling sounds. `WeaponView` adds one
/// as a child of itself and connects its `foley_event` signal here.
#[derive(GodotClass)]
#[class(init, base = Node)]
pub struct WeaponFoley {
    /// Weapon key, such as `m4`: whose recordings to play.
    #[export]
    pub(crate) weapon: GString,
    /// The weapon whose recordings stand in for any this one lacks (the
    /// view's `sound_from`: a new weapon borrows a close one's handling until
    /// it has its own).
    #[export]
    pub(crate) lender: GString,
    voices: Vec<Gd<AudioStreamPlayer>>,
    next_voice: usize,
    sounds: HashMap<String, Option<Gd<AudioStreamRandomizer>>>,
    /// Layers waiting for their delay: (seconds left, stream, level).
    pending: Vec<(f32, Gd<AudioStreamRandomizer>, f32)>,
    /// Events heard, in order (tests).
    heard: Vec<StringName>,
    /// Seconds since this node started, and when each event last played:
    /// a pickup and the draw it starts both ask for the draw sound.
    clock: f32,
    last_played: HashMap<String, f32>,
    base: Base<Node>,
}

#[godot_api]
impl INode for WeaponFoley {
    fn ready(&mut self) {
        for i in 0..VOICES {
            let mut player = AudioStreamPlayer::new_alloc();
            player.set_name(&format!("Foley{i}"));
            player.set_bus(BUS);
            self.base_mut().add_child(&player);
            self.voices.push(player);
        }
    }

    fn process(&mut self, delta: f64) {
        let _profile = crate::profile::scope("weapon foley");
        let delta = delta as f32;
        self.clock += delta;
        if self.pending.is_empty() {
            return;
        }
        let mut due = Vec::new();
        self.pending.retain_mut(|(left, stream, level)| {
            *left -= delta;
            if *left <= 0.0 {
                due.push((stream.clone(), *level));
                false
            } else {
                true
            }
        });
        for (stream, level) in due {
            self.start(&stream, level);
        }
    }
}

#[godot_api]
impl WeaponFoley {
    /// Plays the layers of `event` (one of [`EVENTS`]).
    #[func]
    pub fn play(&mut self, event: StringName) {
        let name = event.to_string();
        let now = self.clock;
        if self
            .last_played
            .insert(name.clone(), now)
            .is_some_and(|then| now - then < REPEAT_GAP)
        {
            return;
        }
        self.heard.push(event);
        if self.heard.len() > 64 {
            self.heard.remove(0);
        }
        for layer in layers(&name) {
            let stream = match layer.source {
                Source::Weapon(own, fallback) => self
                    .weapon_sound(own)
                    .or_else(|| fallback.and_then(|f| self.weapon_sound(f))),
                Source::Shared(shared) => self.sound(&path("", shared, true)),
            };
            let Some(stream) = stream else { continue };
            let level = BASE_DB + layer.level_db;
            if layer.delay > 0.0 {
                // Bounded: a reload queues one delayed layer at a time.
                if self.pending.len() < VOICES {
                    self.pending.push((layer.delay, stream, level));
                }
            } else {
                self.start(&stream, level);
            }
        }
    }

    /// Every event name the view emits (tests).
    #[func]
    fn event_names() -> PackedStringArray {
        EVENTS.iter().map(|e| GString::from(*e)).collect()
    }

    /// The events played so far, oldest first (tests).
    #[func]
    fn events_heard(&self) -> Array<StringName> {
        self.heard.iter().cloned().collect()
    }
}

impl WeaponFoley {
    /// The weapon's own recordings of `event`, or, for a weapon that has
    /// none yet, those of the weapon it borrows its sound from.
    fn weapon_sound(&mut self, event: &str) -> Option<Gd<AudioStreamRandomizer>> {
        let weapon = self.weapon.to_string();
        let lender = self.lender.to_string();
        self.sound(&path(&weapon, event, false)).or_else(|| {
            (!lender.is_empty())
                .then(|| self.sound(&path(&lender, event, false)))
                .flatten()
        })
    }

    fn sound(&mut self, base: &str) -> Option<Gd<AudioStreamRandomizer>> {
        self.sounds
            .entry(base.to_owned())
            .or_insert_with(|| audio::variations(base, 1.04))
            .clone()
    }

    /// Starts a sound on the next voice, round-robin: the oldest one is cut
    /// when all four are busy.
    fn start(&mut self, stream: &Gd<AudioStreamRandomizer>, level_db: f32) {
        if self.voices.is_empty() {
            return;
        }
        let index = self.next_voice % self.voices.len();
        self.next_voice = index + 1;
        let voice = &mut self.voices[index];
        voice.set_stream(stream);
        voice.set_volume_db(level_db);
        voice.play();
    }
}

/// When the main hit of `event` should land during an action of `duration`
/// seconds whose contact happens at fraction `hit`: the fraction to emit at.
pub fn emit_fraction(event: &str, hit: f32, duration: f32) -> f32 {
    (hit - lead(event) / duration.max(0.05)).max(0.0)
}

/// Whether progress moving from `before` to `now` crossed `at`. A new action
/// starts from a `before` below zero, so a cue at 0 fires on its first frame.
pub fn crossed(before: f32, now: f32, at: f32) -> bool {
    before < at && now >= at
}

/// The handling sounds of another player's reload, positional: magazine out
/// and in (and the bolt or charging handle after an empty one) at the same
/// fractions as the first-person reload.
pub fn remote_reload_cues(magazine: bool, empty: bool) -> &'static [(f32, &'static str)] {
    match (magazine, empty) {
        (true, false) => &[(0.2, "mag_out"), (0.69, "mag_in")],
        (true, true) => &[(0.2, "mag_out"), (0.69, "mag_in"), (0.82, "bolt_release")],
        (false, _) => &[(0.0, "pump_back"), (0.5, "shell_in")],
    }
}

/// A weapon's recording of `event` for another player's reload, or `None`.
pub fn remote_sound(weapon: &str, event: &str) -> Option<Gd<AudioStreamRandomizer>> {
    let own = audio::variations(&path(weapon, event, false), 1.04);
    own.or_else(|| match event {
        "bolt_release" => audio::variations(&path(weapon, "charge", false), 1.04),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_event_has_layers() {
        for event in EVENTS {
            assert!(!layers(event).is_empty(), "{event} plays nothing");
        }
    }

    #[test]
    fn a_hit_lands_on_its_contact() {
        // A 2.1 s reload seats the magazine at 0.69: the sound starts its
        // lead earlier, so its hit is on that frame.
        let at = emit_fraction("mag_in", 0.69, 2.1);
        assert!((0.69 - at) * 2.1 - lead("mag_in") < 1e-4);
        // Never before the action starts.
        assert_eq!(emit_fraction("charge", 0.05, 0.6), 0.0);
    }

    #[test]
    fn crossing_fires_once() {
        assert!(crossed(-1.0, 0.0, 0.0));
        assert!(crossed(0.1, 0.2, 0.15));
        assert!(!crossed(0.2, 0.3, 0.15));
        assert!(!crossed(0.15, 0.2, 0.15));
    }
}
