//! A map's background sound: a bed for outdoors and one for indoors, and the
//! hum and buzz of props that make noise.
//!
//! Indoors is wherever the listener is inside a `ReverbZone`, the same boxes
//! that give rooms their reverb. Stepping inside fades the outdoor bed down and
//! closes a low-pass over it (heard through the walls), and fades the room
//! tone in. Air conditioners hum and lamps buzz from where they stand, found by
//! the prop they were instanced from, so a map gets them without placing any.
//!
//! Beds play on the Ambience bus, which ducks under gunfire (a compressor keyed
//! from the Weapons bus) and sits low enough never to cover a footstep. Nothing
//! starts until there is a camera, so the dedicated server never plays any.

use godot::classes::audio_stream_player_3d::AttenuationModel;
use godot::classes::{
    AudioEffectLowPassFilter, AudioServer, AudioStream, AudioStreamOggVorbis, AudioStreamPlayer,
    AudioStreamPlayer3D, INode3D, Node, Node3D, ResourceLoader,
};
use godot::global::randf_range;
use godot::prelude::*;

use crate::acoustics::zone_at;

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
        self.apply_blend();
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
