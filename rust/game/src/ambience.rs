//! A map's background sound: a bed for outdoors and one for indoors, and the
//! hum and buzz of props that make noise.
//!
//! Indoors is wherever the listener is inside an enclosed `ReverbZone`, the
//! same boxes that give rooms their reverb (outdoor reflection zones such as
//! Warehouse's Field zones keep the listener outdoors). Stepping inside fades
//! the outdoor bed down and closes a low-pass over it (heard through the
//! walls), and fades the room tone in. Air conditioners hum and lamps buzz
//! from where they stand, found by the prop they were instanced from, so a map
//! gets them without placing any.
//!
//! Maps with contrasting outdoor halves (such as Warehouse's overgrown Field
//! and concrete Docks) can set `preset` (for example `"warehouse"`): each
//! outdoor area gets its own bed and area-specific one-shots that cross-fade
//! smoothly by listener position, a roof breach lets faint sky wind bleed into
//! the hall beneath it, and an optional sparse off-map industrial layer plays
//! distant plant noises 30–80 m beyond the perimeter walls on one voice with
//! per-category cooldowns.
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

use crate::acoustics::{SKIP_OCCLUSION_META, is_indoors_at};
use crate::ambience_areas::{
    INDUSTRIAL_INDOOR_ATTEN_DB, IndustrialScheduler, MapSoundscapePreset,
    outdoor_cutoff_with_breach, preset_by_name,
};
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
/// How fast indoors, outdoors, area beds, and roof breach cross-fade, per second.
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
    /// Optional area-aware map preset (e.g. `"warehouse"`). When set, drives
    /// per-area outdoor beds, area-specific outdoor one-shots, roof-breach
    /// wind bleed, and sparse off-map industrial one-shots.
    #[export]
    #[init(val = GString::new())]
    preset: GString,
    /// Outdoor bed: a loop in `assets/audio/ambience` (`day`, `city`, `night`),
    /// or empty for none. Ignored when `preset` defines per-area outdoor beds.
    #[export]
    #[init(val = GString::from("day"))]
    outdoor: GString,
    #[export]
    #[init(val = -16.0)]
    outdoor_db: f32,
    /// Indoor bed, while the listener is in an indoor reverb zone.
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
    /// Wind gusts and loose debris (or area one-shots) now and then, outdoors.
    #[export]
    #[init(val = true)]
    one_shots: bool,
    /// A fight far beyond the map: distant bursts and explosions.
    #[export]
    distant_battle: bool,

    /// One-shot players: a gust (non-positional, on the outdoor bus), debris /
    /// area one-shots and the distant battle (positional), and when each plays next.
    gust_player: Option<Gd<AudioStreamPlayer>>,
    debris_player: Option<Gd<AudioStreamPlayer3D>>,
    battle_player: Option<Gd<AudioStreamPlayer3D>>,
    next_one_shot: f32,
    next_battle: f32,
    /// Last positional area one-shot index played, so it never repeats back to back.
    last_area_shot: Option<(usize, usize)>,
    /// A burst in progress: rounds left, seconds between them, and the gun.
    burst: Option<(u32, f32, usize)>,
    battle_guns: Vec<Option<Gd<AudioStreamRandomizer>>>,
    /// One-shots, battle sounds, and off-map industrial sounds played (tests).
    one_shots_played: u32,
    battle_played: u32,
    industrial_played: u32,

    /// Area-aware preset state.
    active_preset: Option<&'static MapSoundscapePreset>,
    area_players: Vec<Option<Gd<AudioStreamPlayer>>>,
    area_weights: Vec<f32>,
    active_area_idx: usize,
    breach_player: Option<Gd<AudioStreamPlayer>>,
    breach: f32,
    industrial_player: Option<Gd<AudioStreamPlayer3D>>,
    industrial_scheduler: Option<IndustrialScheduler>,

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
        let listener = camera.get_global_position();
        if !self.started {
            self.start(listener);
        }
        let dt = delta as f32;
        let target = if is_indoors_at(&self.base().get_tree(), listener) {
            1.0
        } else {
            0.0
        };
        let t = 1.0 - (-BLEND_RATE * dt).exp();
        self.indoors += (target - self.indoors) * t;
        if let Some(preset) = self.active_preset {
            self.active_area_idx = preset.select_area(listener.x, listener.z);
            for (idx, weight) in self.area_weights.iter_mut().enumerate() {
                let goal = if idx == self.active_area_idx {
                    1.0
                } else {
                    0.0
                };
                *weight += (goal - *weight) * t;
            }
            let breach_target = preset
                .roof_breach
                .map_or(0.0, |b| b.weight(listener.x, listener.z, self.indoors));
            self.breach += (breach_target - self.breach) * t;
        }
        self.apply_blend();
        self.update_one_shots(listener, dt);
        self.update_industrial(listener, dt);
        self.update_battle(listener, dt);
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

    /// Active outdoor area id for preset maps (e.g. `"field"`, `"docks"`),
    /// or empty for single-bed maps (tests).
    #[func]
    fn active_area(&self) -> GString {
        self.active_preset
            .and_then(|p| p.areas.get(self.active_area_idx))
            .map_or_else(GString::new, |a| GString::from(a.id))
    }

    /// 0 away from roof breach or outdoors, 1 directly beneath it indoors (tests).
    #[func]
    fn breach_amount(&self) -> f32 {
        self.breach
    }

    /// The beds' players (tests): outdoor(s), then indoor, then optional breach.
    #[func]
    fn beds(&self) -> Array<Gd<AudioStreamPlayer>> {
        self.outdoor_player
            .iter()
            .chain(self.area_players.iter().flatten())
            .chain(self.indoor_player.iter())
            .chain(self.breach_player.iter())
            .cloned()
            .collect()
    }

    /// One-shots (gusts, debris, or area one-shots) played (tests).
    #[func]
    fn one_shots_heard(&self) -> u32 {
        self.one_shots_played
    }

    /// Off-map industrial one-shots played (tests).
    #[func]
    fn industrial_heard(&self) -> u32 {
        self.industrial_played
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
    fn start(&mut self, listener: Vector3) {
        self.started = true;
        ensure_outdoor_bus();
        let preset_name = self.preset.to_string();
        self.active_preset = preset_by_name(&preset_name);
        if let Some(preset) = self.active_preset {
            self.active_area_idx = preset.select_area(listener.x, listener.z);
            self.area_players = preset
                .areas
                .iter()
                .map(|area| self.bed(area.bed, OUTDOOR_BUS, &format!("OutdoorBed_{}", area.id)))
                .collect();
            self.area_weights = (0..preset.areas.len())
                .map(|i| if i == self.active_area_idx { 1.0 } else { 0.0 })
                .collect();
            // Use the preset's indoor bed unless the scene explicitly overrode it.
            let indoor = self.indoor.to_string();
            let indoor_name = if indoor == "interior" || indoor.is_empty() {
                self.indoor_db = preset.indoor_db;
                preset.indoor_bed.to_owned()
            } else {
                indoor
            };
            self.indoor_player = self.bed(&indoor_name, BUS, "IndoorBed");
            if let Some(breach) = preset.roof_breach {
                self.breach_player = self.bed(breach.bed, BUS, "BreachWind");
            }
            if preset.industrial_off_map {
                // Off-map: the boundary collider must not muffle it (set before
                // add_child, when AudioEnvironment decides whether to track it).
                let player = self.positional_on("IndustrialOffMap", OUTDOOR_BUS, 1, true);
                self.industrial_player = Some(player);
                self.industrial_scheduler =
                    Some(IndustrialScheduler::new(randf_range(0.0, 1.0) as f32));
            }
        } else {
            let outdoor = self.outdoor.to_string();
            let indoor = self.indoor.to_string();
            self.outdoor_player = self.bed(&outdoor, OUTDOOR_BUS, "OutdoorBed");
            self.indoor_player = self.bed(&indoor, BUS, "IndoorBed");
        }
        if self.prop_sounds {
            self.attach_prop_sounds();
        }
        if self.one_shots {
            let mut gust = AudioStreamPlayer::new_alloc();
            gust.set_name("Gust");
            gust.set_bus(OUTDOOR_BUS);
            self.base_mut().add_child(&gust);
            self.gust_player = Some(gust);
            // An area preset places its one-shots beyond the play space's
            // boundary collider, so occlusion would muffle every one of them.
            let off_map = self.active_preset.is_some();
            let bus = if off_map { OUTDOOR_BUS } else { BUS };
            self.debris_player = Some(self.positional_on("Debris", bus, 2, off_map));
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
        self.positional_on(name, BUS, polyphony, false)
    }

    fn positional_on(
        &mut self,
        name: &str,
        bus: &str,
        polyphony: i32,
        skip_occlusion: bool,
    ) -> Gd<AudioStreamPlayer3D> {
        let mut player = AudioStreamPlayer3D::new_alloc();
        if skip_occlusion {
            player.set_meta(SKIP_OCCLUSION_META, &true.to_variant());
        }
        player.set_name(name);
        player.set_bus(bus);
        player.set_as_top_level(true);
        // Levels are set per sound; distance is in the recordings.
        player.set_attenuation_model(AttenuationModel::DISABLED);
        player.set_max_polyphony(polyphony);
        self.base_mut().add_child(&player);
        player
    }

    /// A gust or area-appropriate outdoor one-shot every 7-18 s, outdoors.
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
        if let Some(preset) = self.active_preset {
            self.play_area_one_shot(preset, listener);
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

    fn play_area_one_shot(&mut self, preset: &'static MapSoundscapePreset, listener: Vector3) {
        let area_idx = self.active_area_idx;
        let Some(area) = preset.areas.get(area_idx) else {
            return;
        };
        if let Some(gust_stem) = area.gust_stem
            && randf_range(0.0, 1.0) < f64::from(area.gust_probability)
        {
            let level = area.bed_db + area.gust_offset_db;
            if let (Some(stream), Some(player)) = (one_shot(gust_stem), self.gust_player.as_mut()) {
                player.set_stream(&stream);
                player.set_volume_db(level);
                player.play();
                self.one_shots_played += 1;
            }
            return;
        }
        let eligible: Vec<usize> = area
            .positional_one_shots
            .iter()
            .enumerate()
            .filter(|&(i, shot)| {
                if self.last_area_shot == Some((area_idx, i)) && area.positional_one_shots.len() > 1
                {
                    return false;
                }
                if let (Some(cat), Some(sched)) =
                    (shot.shared_category, self.industrial_scheduler.as_ref())
                {
                    if !sched.can_play(cat) {
                        return false;
                    }
                    if self
                        .industrial_player
                        .as_ref()
                        .is_some_and(|p| p.is_playing())
                    {
                        return false;
                    }
                }
                true
            })
            .map(|(i, _)| i)
            .collect();
        if eligible.is_empty() {
            return;
        }
        let pick_idx =
            eligible[(randf_range(0.0, eligible.len() as f64) as usize).min(eligible.len() - 1)];
        let shot = area.positional_one_shots[pick_idx];
        if let (Some(stream), Some(player)) = (one_shot(shot.stem), self.debris_player.as_mut()) {
            let dist =
                randf_range(f64::from(shot.min_distance), f64::from(shot.max_distance)) as f32;
            let at = listener + around(dist, shot.rise_y);
            player.set_global_position(at);
            player.set_stream(&stream);
            player.set_volume_db(shot.volume_db);
            player.play();
            self.last_area_shot = Some((area_idx, pick_idx));
            if let (Some(cat), Some(sched)) =
                (shot.shared_category, self.industrial_scheduler.as_mut())
            {
                sched.record_played(cat);
            }
            self.one_shots_played += 1;
        }
    }

    /// Sparse off-map working-industrial one-shots (30–80 m beyond the walls,
    /// single voice, strict cooldowns so they stay atmosphere rather than noise).
    fn update_industrial(&mut self, listener: Vector3, dt: f32) {
        let (Some(sched), Some(player)) = (
            self.industrial_scheduler.as_mut(),
            self.industrial_player.as_mut(),
        ) else {
            return;
        };
        let busy = player.is_playing();
        let on_dock_side = self
            .active_preset
            .and_then(|p| p.areas.get(self.active_area_idx))
            .is_some_and(|a| a.id == "docks");
        let cue = sched.tick(
            dt,
            busy,
            on_dock_side,
            randf_range(0.0, 1.0) as f32,
            randf_range(0.0, 1.0) as f32,
            randf_range(0.0, 1.0) as f32,
            randf_range(0.0, 1.0) as f32,
        );
        let Some(cue) = cue else {
            return;
        };
        let Some(stream) = one_shot(cue.category.stem()) else {
            return;
        };
        let at = listener + Vector3::new(cue.offset_x, cue.offset_y, cue.offset_z);
        player.set_global_position(at);
        player.set_stream(&stream);
        player.set_volume_db(cue.category.volume_db() - INDUSTRIAL_INDOOR_ATTEN_DB * self.indoors);
        player.play();
        self.industrial_played += 1;
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
        if let Some(preset) = self.active_preset {
            for (idx, area) in preset.areas.iter().enumerate() {
                if let Some(Some(player)) = self.area_players.get_mut(idx) {
                    let weight = self.area_weights.get(idx).copied().unwrap_or(0.0);
                    let base_db = area.bed_db - INDOOR_OUTDOOR_DB * indoors;
                    player.set_volume_db(fade_db(base_db, weight));
                }
            }
            if let (Some(breach), Some(player)) = (preset.roof_breach, &mut self.breach_player) {
                player.set_volume_db(fade_db(breach.bed_db, self.breach));
            }
        } else if let Some(player) = &mut self.outdoor_player {
            player.set_volume_db(self.outdoor_db - INDOOR_OUTDOOR_DB * indoors);
        }
        if let Some(player) = &mut self.indoor_player {
            player.set_volume_db(fade_db(self.indoor_db, indoors));
        }
        let lift_hz = self
            .active_preset
            .and_then(|p| p.roof_breach)
            .map_or(INDOOR_CUTOFF_HZ, |b| b.cutoff_lift_hz);
        let cutoff = outdoor_cutoff_with_breach(
            OPEN_CUTOFF_HZ,
            INDOOR_CUTOFF_HZ,
            indoors,
            self.breach,
            lift_hz,
        );
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
