//! Footsteps and movement sounds that tell players where everyone is.
//!
//! The rules are shared by the local player and by everyone else
//! (`RemotePlayer`): a gait (crouched, quiet walk, walk, sprint) sets the
//! stride, how loud a step is, and how far away it can be heard. A quiet walk
//! is a standing walk well under walking speed, such as walking while aiming
//! down sights, so moving carefully is rewarded with quieter steps.
//!
//! Other players' steps do not use Godot's distance attenuation. Each step's
//! volume comes from [`falloff_db`] over its gait's audible range instead, so
//! every gait has a range the player can learn: a sprint is heard across a
//! lane, a crouch only in the same room. Steps through walls are muffled by
//! `AudioEnvironment` like every other positional sound.

use std::collections::HashMap;

use godot::classes::audio_stream_player_3d::AttenuationModel;
use godot::classes::{
    AudioStreamPlayer3D, AudioStreamRandomizer, Node3D, PhysicsRayQueryParameters3D,
};
use godot::prelude::*;
use shooter_sim::movement::Stance;
use shooter_sim::weapon::{Reload, WeaponId};

use crate::audio;
use crate::surfaces::{Surface, surface_at};
use crate::target::WORLD_LAYER;
use crate::weapon_foley;

/// How someone is moving on the ground, from quietest to loudest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gait {
    Crouch,
    Quiet,
    Walk,
    Sprint,
}

/// How a gait sounds: metres per step, level in dB, and how far away (in
/// metres) another player can hear it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GaitSound {
    pub stride: f32,
    pub level_db: f32,
    pub range: f32,
}

/// A standing walk slower than this share of walking speed is a quiet walk.
const QUIET_SPEED_RATIO: f32 = 0.72;
/// Below this speed (m/s) nobody takes a step.
const MIN_STEP_SPEED: f32 = 0.6;

/// The local player hears their own steps this much quieter than someone
/// else's at the same spot, so enemy steps stand out.
pub const OWN_STEP_DB: f32 = -4.0;
/// Teammates' steps are quieter and carry less far than enemies'.
pub const TEAMMATE_DB: f32 = -7.0;
pub const TEAMMATE_RANGE: f32 = 0.6;

/// Landing from a jump or a fall: heard further than any step.
pub const LAND_RANGE: f32 = 32.0;
/// A slide's scrape. The sample is a loud broadband scrape, so it plays
/// well below full level: your own at `SLIDE_DB`, others' 3 dB under that.
pub const SLIDE_RANGE: f32 = 28.0;
pub const SLIDE_DB: f32 = -9.0;
/// Climbing over a ledge.
pub const MANTLE_RANGE: f32 = 20.0;
/// A reload: close enough to push someone who is reloading.
pub const RELOAD_RANGE: f32 = 16.0;

impl Gait {
    /// The gait of someone on the ground, or None when they take no steps
    /// (sliding, or standing still).
    pub fn of(stance: Stance, sprinting: bool, speed: f32, walk_speed: f32) -> Option<Self> {
        if speed < MIN_STEP_SPEED {
            return None;
        }
        Some(match stance {
            Stance::Sliding => return None,
            Stance::Crouching => Self::Crouch,
            Stance::Standing if sprinting => Self::Sprint,
            Stance::Standing if speed < walk_speed * QUIET_SPEED_RATIO => Self::Quiet,
            Stance::Standing => Self::Walk,
        })
    }

    pub fn sound(self) -> GaitSound {
        let (stride, level_db, range) = match self {
            Self::Crouch => (1.25, -14.0, 8.0),
            Self::Quiet => (1.6, -10.0, 14.0),
            Self::Walk => (1.9, -5.0, 24.0),
            Self::Sprint => (2.3, 0.0, 36.0),
        };
        GaitSound {
            stride,
            level_db,
            range,
        }
    }
}

/// Distance in metres walked since the last step, and the stride it is
/// counting toward.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stride {
    pub travelled: f32,
    pub length: f32,
}

impl Stride {
    /// Adds `moved` metres at `gait`. True when a step lands.
    pub fn advance(&mut self, moved: f32, gait: Option<Gait>) -> bool {
        let Some(gait) = gait else {
            // Standing still: drift back so the next step is a full stride.
            self.travelled = (self.travelled - 0.02).max(0.0);
            return false;
        };
        self.length = gait.sound().stride;
        self.travelled += moved;
        if self.travelled >= self.length {
            self.travelled -= self.length;
            // A teleport or a frame hitch never fires a burst of steps.
            self.travelled = self.travelled.min(self.length * 0.5);
            return true;
        }
        false
    }

    /// 0 at a step, rising to 1 at the next.
    pub fn phase(&self) -> f32 {
        self.travelled / self.length.max(0.1)
    }
}

/// Level in dB of a sound `distance` metres away whose audible range is
/// `range`, or None beyond it. Close by it is nearly full level; it fades
/// smoothly to silence at the edge, so the range is a clean line.
pub fn falloff_db(distance: f32, range: f32) -> Option<f32> {
    if range <= 0.0 || distance >= range {
        return None;
    }
    let x = (distance / range).max(0.0);
    let gain = (1.0 - x * x).powi(2) / (1.0 + 2.0 * x);
    Some(20.0 * gain.max(1e-5).log10())
}

/// The movement sounds of one other player, at their feet.
///
/// The steps and the body sounds (landings, slides, mantles) play on the
/// Footsteps bus, and a reload on Effects, each through its own positional
/// player made here.
pub struct Foley {
    steps: Gd<AudioStreamPlayer3D>,
    body: Gd<AudioStreamPlayer3D>,
    gear: Gd<AudioStreamPlayer3D>,
    stride: Stride,
    was_on_floor: bool,
    air_time: f32,
    fall_speed: f32,
    pushed_off: bool,
    stance: Stance,
    mantling: bool,
    /// The reload's progress last frame, below 0 when not reloading.
    reload_progress: f32,
    sounds: HashMap<String, Option<Gd<AudioStreamRandomizer>>>,
    /// Steps played (tests).
    pub steps_played: u32,
}

/// Another player's reload in progress: the weapon, and how far through
/// its animation it is (0 to 1).
#[derive(Clone, Copy, Debug)]
pub struct RemoteReload {
    pub weapon: WeaponId,
    pub progress: f32,
}

/// What a player is doing this frame, as far as their sounds go.
pub struct Motion {
    pub feet: Vector3,
    pub velocity: Vector3,
    pub on_floor: bool,
    pub stance: Stance,
    pub sprinting: bool,
    pub alive: bool,
    pub mantling: bool,
    pub reloading: bool,
    pub walk_speed: f32,
}

impl Foley {
    /// Makes the players as children of `owner`.
    pub fn new(owner: &mut Gd<Node3D>) -> Self {
        let mut make = |name: &str, bus: &str, polyphony: i32| {
            let mut player = AudioStreamPlayer3D::new_alloc();
            player.set_name(name);
            player.set_bus(bus);
            player.set_max_polyphony(polyphony);
            // Volume comes from `falloff_db` per sound, not from distance.
            player.set_attenuation_model(AttenuationModel::DISABLED);
            player.set_attenuation_filter_cutoff_hz(20_500.0);
            owner.add_child(&player);
            player
        };
        Self {
            steps: make("Steps", "Footsteps", 3),
            body: make("BodyFoley", "Footsteps", 2),
            gear: make("Gear", weapon_foley::BUS, 2),
            stride: Stride::default(),
            was_on_floor: true,
            air_time: 0.0,
            fall_speed: 0.0,
            pushed_off: false,
            stance: Stance::Standing,
            mantling: false,
            reload_progress: -1.0,
            sounds: HashMap::new(),
            steps_played: 0,
        }
    }

    /// Advances by one frame of `delta` seconds. `listener` is the camera,
    /// `friendly` whether this player is on the listener's team, and `reload`
    /// the reload in progress, if any.
    pub fn update(
        &mut self,
        motion: &Motion,
        delta: f32,
        listener: Option<Vector3>,
        friendly: bool,
        reload: Option<RemoteReload>,
    ) {
        let speed = Vector2::new(motion.velocity.x, motion.velocity.z).length();
        let (team_db, team_range) = if friendly {
            (TEAMMATE_DB, TEAMMATE_RANGE)
        } else {
            (0.0, 1.0)
        };
        let distance = listener.map_or(f32::INFINITY, |l| l.distance_to(motion.feet));
        let level = |base_db: f32, range: f32| {
            falloff_db(distance, range * team_range).map(|d| base_db + d + team_db)
        };

        if !motion.alive {
            self.was_on_floor = motion.on_floor;
            self.reload_progress = -1.0;
            return;
        }

        // Steps.
        if motion.on_floor {
            let gait = Gait::of(motion.stance, motion.sprinting, speed, motion.walk_speed);
            if self.stride.advance(speed * delta, gait)
                && let Some(gait) = gait
            {
                let sound = gait.sound();
                if let Some(db) = level(sound.level_db, sound.range) {
                    self.play_step(motion.feet, db);
                    // A sprinting soldier's kit rattles with every stride.
                    if gait == Gait::Sprint {
                        self.play_body(
                            "res://assets/audio/player/step_gear",
                            motion.feet,
                            db - 7.0,
                        );
                    }
                }
            }
        }

        // Leaving the ground and landing.
        if !motion.on_floor {
            self.air_time += delta;
            self.fall_speed = self.fall_speed.max(-motion.velocity.y);
            // The push off of a jump (not a walk off a ledge). Velocity is
            // estimated from snapshots, so give it a moment to rise.
            if !self.pushed_off && self.air_time < 0.2 && motion.velocity.y > 1.0 {
                self.pushed_off = true;
                if let Some(db) = level(-4.0, Gait::Walk.sound().range) {
                    self.play_body("res://assets/audio/player/jump", motion.feet, db);
                }
            }
        } else if !self.was_on_floor {
            if self.air_time > 0.25 {
                let impact = ((self.fall_speed - 2.0) / 8.0).clamp(0.3, 1.0);
                if let Some(db) = level(20.0 * impact.log10(), LAND_RANGE) {
                    self.play_body("res://assets/audio/player/land", motion.feet, db);
                    self.play_step(motion.feet, db - 3.0);
                }
            }
            self.air_time = 0.0;
            self.fall_speed = 0.0;
            self.pushed_off = false;
            self.stride.travelled = 0.0;
        }
        self.was_on_floor = motion.on_floor;

        if motion.stance == Stance::Sliding
            && self.stance != Stance::Sliding
            && let Some(db) = level(SLIDE_DB - 3.0, SLIDE_RANGE)
        {
            self.play_body("res://assets/audio/player/slide", motion.feet, db);
        }
        self.stance = motion.stance;

        if motion.mantling
            && !self.mantling
            && let Some(db) = level(-4.0, MANTLE_RANGE)
        {
            self.play_body("res://assets/audio/player/mantle", motion.feet, db);
        }
        self.mantling = motion.mantling;

        // The reload's beats, as the first-person one plays them.
        match reload.filter(|_| motion.reloading) {
            Some(reload) => {
                let magazine = matches!(reload.weapon.def().reload, Reload::Magazine { .. });
                for &(at, event) in weapon_foley::remote_reload_cues(magazine, false) {
                    if weapon_foley::crossed(self.reload_progress, reload.progress, at)
                        && let Some(db) = level(-6.0, RELOAD_RANGE)
                    {
                        self.play_gear(reload.weapon, event, motion.feet, db);
                    }
                }
                self.reload_progress = reload.progress;
            }
            None => self.reload_progress = -1.0,
        }
    }

    fn play_gear(&mut self, weapon: WeaponId, event: &str, feet: Vector3, volume_db: f32) {
        let key = format!("{}:{event}", weapon.def().model_key());
        let stream = self
            .sounds
            .entry(key)
            .or_insert_with(|| weapon_foley::remote_sound(weapon.def().model_key(), event))
            .clone();
        let Some(stream) = stream else { return };
        self.gear.set_global_position(feet + Vector3::UP * 1.2);
        self.gear.set_stream(&stream);
        self.gear.set_volume_db(volume_db);
        self.gear.play();
    }

    fn play_step(&mut self, feet: Vector3, volume_db: f32) {
        let surface = floor_surface(&self.steps.clone().upcast(), feet);
        let path = format!("res://assets/audio/footsteps/{}", surface.footstep_key());
        let Some(stream) = self.sound(&path, 1.08) else {
            return;
        };
        self.steps.set_global_position(feet + Vector3::UP * 0.05);
        self.steps.set_stream(&stream);
        self.steps.set_volume_db(volume_db);
        self.steps.play();
        self.steps_played += 1;
    }

    fn play_body(&mut self, base: &str, feet: Vector3, volume_db: f32) {
        let Some(stream) = self.sound(base, 1.05) else {
            return;
        };
        self.body.set_global_position(feet + Vector3::UP * 0.4);
        self.body.set_stream(&stream);
        self.body.set_volume_db(volume_db);
        self.body.play();
    }

    fn sound(&mut self, base: &str, pitch: f32) -> Option<Gd<AudioStreamRandomizer>> {
        self.sounds
            .entry(base.to_owned())
            .or_insert_with(|| audio::variations(base, pitch))
            .clone()
    }
}

/// What the ground under `feet` is made of.
pub fn floor_surface(node: &Gd<Node3D>, feet: Vector3) -> Surface {
    let Some(mut space) = node.get_world_3d().and_then(|w| w.get_direct_space_state()) else {
        return Surface::Concrete;
    };
    let from = feet + Vector3::UP * 0.3;
    let Some(query) = PhysicsRayQueryParameters3D::create_ex(from, feet + Vector3::DOWN * 0.6)
        .collision_mask(WORLD_LAYER)
        .done()
    else {
        return Surface::Concrete;
    };
    let hit = space.intersect_ray(&query);
    if hit.is_empty() {
        return Surface::Concrete;
    }
    let collider = hit.get_or_nil("collider").try_to::<Gd<Node>>().ok();
    let face = hit.get_or_nil("face_index").try_to::<i32>().unwrap_or(-1);
    surface_at(collider.as_ref(), face)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WALK: f32 = 5.0;

    #[test]
    fn gaits_follow_stance_and_speed() {
        assert_eq!(Gait::of(Stance::Standing, false, 0.2, WALK), None);
        assert_eq!(Gait::of(Stance::Sliding, false, 7.0, WALK), None);
        assert_eq!(
            Gait::of(Stance::Crouching, false, 2.6, WALK),
            Some(Gait::Crouch)
        );
        assert_eq!(
            Gait::of(Stance::Standing, true, 7.4, WALK),
            Some(Gait::Sprint)
        );
        assert_eq!(
            Gait::of(Stance::Standing, false, 5.0, WALK),
            Some(Gait::Walk)
        );
        // Aiming down sights slows a walk into a quiet one.
        assert_eq!(
            Gait::of(Stance::Standing, false, 3.0, WALK),
            Some(Gait::Quiet)
        );
    }

    #[test]
    fn louder_gaits_carry_further_and_stride_longer() {
        let gaits = [Gait::Crouch, Gait::Quiet, Gait::Walk, Gait::Sprint];
        for pair in gaits.windows(2) {
            let (a, b) = (pair[0].sound(), pair[1].sound());
            assert!(a.level_db < b.level_db);
            assert!(a.range < b.range);
            assert!(a.stride < b.stride);
        }
    }

    #[test]
    fn falloff_is_full_close_by_and_silent_at_the_range() {
        assert!(falloff_db(0.0, 20.0).unwrap().abs() < 1e-4);
        assert!(falloff_db(20.0, 20.0).is_none());
        assert!(falloff_db(25.0, 20.0).is_none());
        let mut last = 0.1;
        for d in 0..20 {
            let db = falloff_db(d as f32, 20.0).unwrap();
            assert!(db < last, "falls off with distance");
            last = db;
        }
        // Still clearly audible at half range.
        assert!(falloff_db(10.0, 20.0).unwrap() > -14.0);
    }

    #[test]
    fn a_step_lands_once_per_stride() {
        let mut stride = Stride::default();
        let mut steps = 0;
        // Ten metres of walking at 60 frames per second.
        for _ in 0..120 {
            if stride.advance(5.0 / 60.0, Some(Gait::Walk)) {
                steps += 1;
            }
        }
        let expected = (10.0 / Gait::Walk.sound().stride) as i32;
        assert!((steps - expected).abs() <= 1, "{steps} steps");
    }

    #[test]
    fn a_teleport_fires_one_step_not_a_burst() {
        let mut stride = Stride::default();
        assert!(stride.advance(40.0, Some(Gait::Sprint)));
        assert!(!stride.advance(0.1, Some(Gait::Sprint)));
    }
}
