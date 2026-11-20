//! A map's background sound: a bed for outdoors and one for indoors, and the
//! hum and buzz of props that make noise.
//!
//! Indoors is wherever the listener is inside a `ReverbZone`, the same boxes
//! that give rooms their reverb. Stepping inside fades the outdoor bed down and
//! closes a low-pass over it (heard through the walls), and fades the room
//! tone in. Air conditioners hum and lamps buzz from where they stand, found by
//! the prop they were instanced from, so a map gets them without placing any.
//!
//! On top of the beds, the world moves: wind gusts sweep past and loose metal
//! rattles somewhere nearby every so often (outdoors; muffled like the bed
//! indoors). A map can also set `distant_battle` for a fight far beyond its
//! edges: real far-away recordings of the game's guns in bursts, and now and
//! then a distant explosion, from 250-400 m out, one voice at a time. It is off
//! by default, so on multiplayer maps every gunshot you hear is a real one.
//!
//! Beds play on the Ambience bus, which ducks under gunfire (a compressor keyed
//! from the Weapons bus) and sits low enough never to cover a footstep. Nothing
//! starts until there is a camera, so the dedicated server never plays any.

use godot::classes::audio_stream_player_3d::AttenuationModel;
use godot::classes::{
    AudioEffectLowPassFilter, AudioServer, AudioStream, AudioStreamOggVorbis, AudioStreamPlayer,
    AudioStreamPlayer3D, AudioStreamRandomizer, INode3D, Node, Node3D, ResourceLoader,
};
use godot::global::randf_range;
use godot::prelude::*;

use crate::acoustics::zone_at;
use crate::audio;

const AMBIENCE_DIR: &str = "res://assets/audio/ambience";
const BUS: &str = "Ambience";
/// The outdoor bed's own bus, so it can be muffled indoors.
const OUTDOOR_BUS: &str = "Ambience (outdoor)";
/// The outdoor bed's low-pass, open outdoors and closed to this indoors.
const INDOOR_CUTOFF_HZ: f32 = 650.0;
const OPEN_CUTOFF_HZ: f32 = 20_000.0;
/// How much quieter the outdoor bed is from indoors, in dB.
const INDOOR_OUTDOOR_DB: f32 = 9.0;
/// How fast indoors and outdoors cross-fade, per second.
const BLEND_RATE: f32 = 1.8;
/// Silence, in dB, for a bed faded all the way out.
const SILENT_DB: f32 = -60.0;

/// The guns of the distant battle (their `far` recordings), and whether each
/// fires bursts.
const BATTLE_GUNS: [(&str, bool); 6] = [
    ("m4", true),
    ("ak47", true),
    ("lsw6", true),
    ("pdw9", true),
    ("br3", false),
    ("m40", false),
];

/// A horizontal offset of `distance` metres in a random direction, `rise`
/// metres up.
fn around(distance: f32, rise: f32) -> Vector3 {
    let angle = randf_range(0.0, std::f64::consts::TAU) as f32;
    Vector3::new(angle.sin() * distance, rise, angle.cos() * distance)
}

/// A one-shot's takes (`ambience/<name>_N.ogg`).
fn one_shot(name: &str) -> Option<Gd<AudioStreamRandomizer>> {
    thread_local! {
        static CACHE: std::cell::RefCell<std::collections::HashMap<String, Option<Gd<AudioStreamRandomizer>>>> =
            std::cell::RefCell::new(std::collections::HashMap::new());
    }
    CACHE.with(|cache| {
        cache
            .borrow_mut()
            .entry(name.to_owned())
            .or_insert_with(|| audio::variations(&format!("{AMBIENCE_DIR}/{name}"), 1.06))
            .clone()
    })
}

/// Props that make a sound: a part of their scene path, the loop, its level,
/// and how far it carries (metres).
const PROP_SOUNDS: &[(&str, &str, f32, f32)] = &[
    ("/props/aircon/", "hum", -8.0, 16.0),
    ("/props/street_lamp/", "buzz", -20.0, 6.0),
    ("/props/security_light/", "buzz", -22.0, 5.0),
    ("/props/utility_box/", "buzz", -24.0, 4.0),
];

#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct Soundscape {
    /// Outdoor bed: a loop in `assets/audio/ambience` (`day`, `city`, `night`),
    /// or empty for none.
    #[export]
    #[init(val = GString::from("day"))]
    outdoor: GString,
    #[export]
    #[init(val = -16.0)]
    outdoor_db: f32,
    /// Indoor bed, while the listener is in a reverb zone.
    #[export]
    #[init(val = GString::from("interior"))]
    indoor: GString,
    #[export]
    #[init(val = -14.0)]
    indoor_db: f32,
    /// Give air conditioners, lamps, and utility boxes their sounds.
    #[export]
    #[init(val = true)]
    prop_sounds: bool,
    /// Wind gusts and loose debris now and then, outdoors.
    #[export]
    #[init(val = true)]
    one_shots: bool,
    /// A fight far beyond the map: distant bursts and explosions.
    #[export]
    distant_battle: bool,

    /// One-shot players: a gust (non-positional, on the outdoor bus), debris
    /// and the distant battle (positional), and when each plays next.
    gust_player: Option<Gd<AudioStreamPlayer>>,
    debris_player: Option<Gd<AudioStreamPlayer3D>>,
    battle_player: Option<Gd<AudioStreamPlayer3D>>,
    next_one_shot: f32,
    next_battle: f32,
    /// A burst in progress: rounds left, seconds between them, and the gun.
    burst: Option<(u32, f32, usize)>,
    battle_guns: Vec<Option<Gd<AudioStreamRandomizer>>>,
    /// One-shots and battle sounds played (tests).
    one_shots_played: u32,
    battle_played: u32,

    started: bool,
    /// 0 outdoors, 1 indoors, blended.
    indoors: f32,
    outdoor_player: Option<Gd<AudioStreamPlayer>>,
    indoor_player: Option<Gd<AudioStreamPlayer>>,
    emitters: Vec<Gd<AudioStreamPlayer3D>>,
    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for Soundscape {
    fn process(&mut self, delta: f64) {
        let _profile = crate::profile::scope("ambience");
        let Some(camera) = self.base().get_viewport().and_then(|v| v.get_camera_3d()) else {
            return;
        };
        if !self.started {
            self.start();
        }
        let listener = camera.get_global_position();
        let target = if zone_at(&self.base().get_tree(), listener).is_some() {
            1.0
        } else {
            0.0
        };
        let t = 1.0 - (-BLEND_RATE * delta as f32).exp();
        self.indoors += (target - self.indoors) * t;
        self.apply_blend();
        self.update_one_shots(listener, delta as f32);
        self.update_battle(listener, delta as f32);
    }

    fn exit_tree(&mut self) {
        // The next map starts with the outdoor bus open.
        set_outdoor_cutoff(OPEN_CUTOFF_HZ);
    }
}

#[godot_api]
impl Soundscape {
    /// 0 outdoors, 1 indoors (tests).
    #[func]
    fn indoors(&self) -> f32 {
        self.indoors
    }

    /// The beds' players (tests): outdoor, then indoor.
    #[func]
    fn beds(&self) -> Array<Gd<AudioStreamPlayer>> {
        self.outdoor_player
            .iter()
            .chain(self.indoor_player.iter())
            .cloned()
            .collect()
    }

    /// One-shots (gusts and debris) and distant battle sounds played (tests).
    #[func]
    fn one_shots_heard(&self) -> u32 {
        self.one_shots_played
    }

    #[func]
    fn battle_heard(&self) -> u32 {
        self.battle_played
    }

    /// The props' looping sounds (tests).
    #[func]
    fn emitters(&self) -> Array<Gd<AudioStreamPlayer3D>> {
        self.emitters.iter().cloned().collect()
    }
}

impl Soundscape {
    fn start(&mut self) {
        self.started = true;
        ensure_outdoor_bus();
        let outdoor = self.outdoor.to_string();
        let indoor = self.indoor.to_string();
        self.outdoor_player = self.bed(&outdoor, OUTDOOR_BUS, "OutdoorBed");
        self.indoor_player = self.bed(&indoor, BUS, "IndoorBed");
        if self.prop_sounds {
            self.attach_prop_sounds();
        }
        if self.one_shots {
            let mut gust = AudioStreamPlayer::new_alloc();
            gust.set_name("Gust");
            gust.set_bus(OUTDOOR_BUS);
            self.base_mut().add_child(&gust);
            self.gust_player = Some(gust);
            self.debris_player = Some(self.positional("Debris", 2));
            self.next_one_shot = randf_range(4.0, 10.0) as f32;
        }
        if self.distant_battle {
            self.battle_player = Some(self.positional("DistantBattle", 3));
            self.battle_guns = BATTLE_GUNS
                .iter()
                .map(|(key, _)| {
                    audio::variations(&format!("res://assets/audio/weapons/{key}/far"), 1.04)
                })
                .collect();
            self.next_battle = randf_range(2.0, 6.0) as f32;
        }
        self.apply_blend();
    }

    fn positional(&mut self, name: &str, polyphony: i32) -> Gd<AudioStreamPlayer3D> {
        let mut player = AudioStreamPlayer3D::new_alloc();
        player.set_name(name);
        player.set_bus(BUS);
        player.set_as_top_level(true);
        // Levels are set per sound; distance is in the recordings.
        player.set_attenuation_model(AttenuationModel::DISABLED);
        player.set_max_polyphony(polyphony);
        self.base_mut().add_child(&player);
        player
    }

    /// A gust or a rattle of debris every 7-18 s, outdoors.
    fn update_one_shots(&mut self, listener: Vector3, dt: f32) {
        if self.gust_player.is_none() {
            return;
        }
        self.next_one_shot -= dt;
        if self.next_one_shot > 0.0 {
            return;
        }
        self.next_one_shot = randf_range(7.0, 18.0) as f32;
        if self.indoors > 0.7 {
            return;
        }
        if randf_range(0.0, 1.0) < 0.7 {
            let level = self.outdoor_db + 3.0;
            if let (Some(stream), Some(player)) = (one_shot("gust"), self.gust_player.as_mut()) {
                player.set_stream(&stream);
                player.set_volume_db(level);
                player.play();
            }
        } else if let (Some(stream), Some(player)) =
            (one_shot("debris"), self.debris_player.as_mut())
        {
            let at = listener + around(randf_range(10.0, 25.0) as f32, 1.0);
            player.set_global_position(at);
            player.set_stream(&stream);
            player.set_volume_db(-20.0);
            player.play();
        }
        self.one_shots_played += 1;
    }

    /// The distant battle: a burst from one gun every 4-15 s, sometimes an
    /// explosion, always far beyond the map.
    fn update_battle(&mut self, listener: Vector3, dt: f32) {
        if self.battle_player.is_none() {
            return;
        }
        self.next_battle -= dt;
        if self.next_battle > 0.0 {
            return;
        }
        // Inside, the battle is barely there.
        let level = -14.0 - 10.0 * self.indoors;
        match self.burst {
            Some((left, gap, gun)) if left > 0 => {
                let stream = self.battle_guns.get(gun).cloned().flatten();
                if let (Some(stream), Some(player)) = (stream, self.battle_player.as_mut()) {
                    player.set_stream(&stream);
                    player.set_volume_db(level);
                    player.play();
                }
                self.battle_played += 1;
                self.burst = Some((left - 1, gap, gun));
                self.next_battle = gap * randf_range(0.85, 1.2) as f32;
            }
            _ => {
                self.burst = None;
                self.next_battle = randf_range(4.0, 15.0) as f32;
                let at = listener + around(randf_range(250.0, 400.0) as f32, 0.0);
                if let Some(player) = self.battle_player.as_mut() {
                    player.set_global_position(at);
                }
                if randf_range(0.0, 1.0) < 0.18 {
                    if let (Some(stream), Some(player)) =
                        (one_shot("explosion_far"), self.battle_player.as_mut())
                    {
                        player.set_stream(&stream);
                        player.set_volume_db(level + 2.0);
                        player.play();
                    }
                    self.battle_played += 1;
                } else {
                    let gun = (randf_range(0.0, BATTLE_GUNS.len() as f64) as usize)
                        .min(BATTLE_GUNS.len() - 1);
                    let (_, automatic) = BATTLE_GUNS[gun];
                    let (rounds, gap) = if automatic {
                        (randf_range(3.0, 9.0) as u32, randf_range(0.08, 0.11) as f32)
                    } else {
                        (randf_range(1.0, 4.0) as u32, randf_range(0.4, 0.9) as f32)
                    };
                    self.burst = Some((rounds, gap, gun));
                    self.next_battle = 0.0;
                }
            }
        }
    }

    fn bed(&mut self, name: &str, bus: &str, node: &str) -> Option<Gd<AudioStreamPlayer>> {
        let stream = looped(name)?;
        let length = stream.get_length();
        let mut player = AudioStreamPlayer::new_alloc();
        player.set_name(node);
        player.set_stream(&stream);
        player.set_bus(bus);
        player.set_volume_db(SILENT_DB);
        self.base_mut().add_child(&player);
        // Start somewhere in the loop, so each visit sounds a little different.
        player
            .play_ex()
            .from_position(randf_range(0.0, length) as f32)
            .done();
        Some(player)
    }

    fn apply_blend(&mut self) {
        let indoors = self.indoors.clamp(0.0, 1.0);
        if let Some(player) = &mut self.outdoor_player {
            player.set_volume_db(self.outdoor_db - INDOOR_OUTDOOR_DB * indoors);
        }
        if let Some(player) = &mut self.indoor_player {
            player.set_volume_db(fade_db(self.indoor_db, indoors));
        }
        // Exponential, so the muffling sounds even across the fade.
        let cutoff = OPEN_CUTOFF_HZ * (INDOOR_CUTOFF_HZ / OPEN_CUTOFF_HZ).powf(indoors);
        set_outdoor_cutoff(cutoff);
    }

    /// A looping sound on every prop that makes one, at the prop's middle.
    fn attach_prop_sounds(&mut self) {
        let Some(root) = self.base().get_parent() else {
            return;
        };
        let mut found = Vec::new();
        collect_props(&root, &mut found);
        for (mut prop, name, level_db, range) in found {
            let Some(stream) = looped(name) else {
                continue;
            };
            let mut player = AudioStreamPlayer3D::new_alloc();
            player.set_name("PropSound");
            player.set_stream(&stream);
            player.set_bus(BUS);
            player.set_volume_db(level_db);
            player.set_attenuation_model(AttenuationModel::INVERSE_SQUARE_DISTANCE);
            player.set_unit_size(3.0);
            player.set_max_distance(range);
            player.set_position(Vector3::UP * 0.6);
            prop.add_child(&player);
            let length = stream.get_length();
            player
                .play_ex()
                .from_position(randf_range(0.0, length) as f32)
                .done();
            self.emitters.push(player);
        }
    }
}

/// Props under `node` that make a sound, with that sound's settings.
fn collect_props(node: &Gd<Node>, out: &mut Vec<(Gd<Node3D>, &'static str, f32, f32)>) {
    let path = node.get_scene_file_path().to_string();
    if !path.is_empty()
        && let Some((_, name, db, range)) = PROP_SOUNDS.iter().find(|(key, ..)| path.contains(key))
        && let Ok(prop) = node.clone().try_cast::<Node3D>()
    {
        out.push((prop, name, *db, *range));
        return;
    }
    for child in node.get_children().iter_shared() {
        collect_props(&child, out);
    }
}

/// `assets/audio/ambience/<name>.ogg`, set to loop.
fn looped(name: &str) -> Option<Gd<AudioStream>> {
    if name.is_empty() {
        return None;
    }
    let path = format!("{AMBIENCE_DIR}/{name}.ogg");
    let mut loader = ResourceLoader::singleton();
    if !loader.exists(&path) {
        godot_warn!("Soundscape: no ambience loop at {path}");
        return None;
    }
    let stream = loader.load(&path)?.try_cast::<AudioStream>().ok()?;
    if let Ok(mut ogg) = stream.clone().try_cast::<AudioStreamOggVorbis>() {
        ogg.set_loop(true);
    }
    Some(stream)
}

/// `level_db` at full `amount`, fading to silence at 0 on a curve that
/// sounds even.
fn fade_db(level_db: f32, amount: f32) -> f32 {
    if amount <= 0.001 {
        return SILENT_DB;
    }
    (level_db + 20.0 * amount.log10()).max(SILENT_DB)
}

/// The outdoor bed's bus with its low-pass, made once. It sends into the
/// Ambience bus, so the ambience volume and the gunfire ducking apply.
fn ensure_outdoor_bus() {
    let mut server = AudioServer::singleton();
    if server.get_bus_index(OUTDOOR_BUS) >= 0 {
        return;
    }
    let index = server.get_bus_count();
    server.add_bus();
    server.set_bus_name(index, OUTDOOR_BUS);
    server.set_bus_send(index, BUS);
    let mut filter = AudioEffectLowPassFilter::new_gd();
    filter.set_cutoff(OPEN_CUTOFF_HZ);
    server.add_bus_effect(index, &filter);
}

fn set_outdoor_cutoff(cutoff: f32) {
    let mut server = AudioServer::singleton();
    let index = server.get_bus_index(OUTDOOR_BUS);
    if index < 0 || server.get_bus_effect_count(index) == 0 {
        return;
    }
    if let Some(mut filter) = server
        .get_bus_effect(index, 0)
        .and_then(|e| e.try_cast::<AudioEffectLowPassFilter>().ok())
    {
        filter.set_cutoff(cutoff);
    }
    // Wide open, the filter only costs time.
    server.set_bus_effect_enabled(index, 0, cutoff < OPEN_CUTOFF_HZ * 0.95);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bed_fades_to_silence() {
        assert_eq!(fade_db(-14.0, 1.0), -14.0);
        assert_eq!(fade_db(-14.0, 0.0), SILENT_DB);
        assert!(fade_db(-14.0, 0.5) < -14.0 && fade_db(-14.0, 0.5) > -24.0);
    }

    #[test]
    fn every_prop_sound_has_a_range() {
        for (key, name, _, range) in PROP_SOUNDS {
            assert!(key.starts_with("/props/") && key.ends_with('/'));
            assert!(!name.is_empty() && *range > 0.0);
        }
    }
}
