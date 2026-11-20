//! `Zombie`: a zombie as clients draw it, for the dedicated server's zombie
//! (`net::horde`, rules in `shooter_sim::zombies`). `NetClient` places it from
//! snapshots (`net::horde_view`) and plays its swings, flinches, and death;
//! it has no body to bump into, and the local player's hits only predict a
//! flinch (the server owns its health).
//!
//! It groans now and then, snarls when it swings, and cries out when it
//! dies, then falls as a ragdoll into the scene's corpse yard. Far from the
//! camera its animation updates less often. Its hit zones (`HitboxRig`) are
//! posed hunched, as the server rewinds them
//! (`shooter_sim::zombies::chase::hitbox_pose`).

use godot::classes::{AudioStream, AudioStreamPlayer3D, CharacterBody3D, ICharacterBody3D};
use godot::prelude::*;
use shooter_sim::rng;

use crate::aim_assist::AimAssistTarget;
use crate::character::corpses::{self, CorpseYard};
use crate::character::rig::{SoldierFrame, SoldierRig};
use crate::convert::to_sim;
use crate::hitbox_rig::HitboxRig;

/// A hit this hard (force level, about a tenth of the damage) staggers a
/// zombie for this long: it stops, rocks back, then comes on again.
const STAGGER_FORCE: f32 = 8.0;
const STAGGER_SECONDS: f32 = 0.5;

/// Collision layer 6 ("barriers"): window boards only the server's zombies
/// bump into.
pub const BARRIER_LAYER: u32 = 1 << 5;

/// Seconds between groans, at least and at most.
const GROAN_MIN: f32 = 3.0;
const GROAN_MAX: f32 = 9.0;
/// Beyond these distances from the camera, the animation updates every
/// second and every fourth frame.
const HALF_RATE_DISTANCE: f32 = 18.0;
const QUARTER_RATE_DISTANCE: f32 = 32.0;

#[derive(GodotClass)]
#[class(init, base = CharacterBody3D)]
pub struct Zombie {
    base: Base<CharacterBody3D>,

    /// Seconds before a dead zombie is removed (its voice finishes). Its body
    /// lies on in the scene's corpse yard.
    #[export]
    #[init(val = 4.0)]
    corpse_time: f32,
    /// Played now and then while it chases (an `AudioStreamRandomizer`).
    #[export]
    groans: Option<Gd<AudioStream>>,
    /// Played when it swings.
    #[export]
    attack_sounds: Option<Gd<AudioStream>>,
    /// Played when it dies.
    #[export]
    death_sounds: Option<Gd<AudioStream>>,

    #[init(node = "Soldier")]
    rig: OnReady<Gd<SoldierRig>>,
    #[init(node = "Hitboxes")]
    hitboxes: OnReady<Gd<HitboxRig>>,
    voice: Option<Gd<AudioStreamPlayer3D>>,

    #[init(val = true)]
    alive: bool,
    yaw: f32,
    /// Ground velocity from the snapshots, for the animation.
    moved: Vector3,
    dead_for: f32,
    visuals: bool,
    groan_in: f32,
    /// Rendered frames since the last animation update, and their time.
    anim_frames: u32,
    anim_time: f32,
    seed: u32,
    /// The dead body has gone to the corpse yard.
    body_left: bool,
    /// Seconds left of a stagger from a heavy hit.
    stagger_for: f32,
    /// When the local player's hit last flinched it (engine seconds), so the
    /// server's report of that hit does not flinch it again.
    flinched_at: Option<f64>,
}

#[godot_api]
impl ICharacterBody3D for Zombie {
    fn ready(&mut self) {
        self.visuals = crate::character::rig::visuals_enabled();
        self.yaw = self.base().get_global_rotation().y;
        self.base_mut().set_rotation(Vector3::ZERO);
        self.base_mut().set_collision_layer(0);
        self.base_mut().set_collision_mask(0);
        self.base_mut().add_to_group("zombies");
        self.voice = self.base().try_get_node_as::<AudioStreamPlayer3D>("Voice");
        self.seed = self.base().instance_id().to_i64() as u32;
        self.groan_in = GROAN_MIN + rng::unit(self.seed, 0) * (GROAN_MAX - GROAN_MIN);
    }

    /// Animation runs per rendered frame (less often far away).
    fn process(&mut self, delta: f64) {
        let _profile = crate::profile::scope("zombies (frame)");
        if !self.visuals {
            return;
        }
        self.anim_frames += 1;
        self.anim_time += delta as f32;
        let every = self.animation_interval();
        // Spread the zombies over the frames, so they do not all update at once.
        if !(self.anim_frames + self.seed).is_multiple_of(every) && self.alive {
            return;
        }
        if self.body_left {
            return;
        }
        let frame = self.frame(self.moved);
        let dt = std::mem::take(&mut self.anim_time);
        self.anim_frames = 0;
        self.rig.bind_mut().update(&frame, dt);
        if !self.alive {
            // The body falls and lies in the scene's yard, past this node's
            // own removal.
            self.body_left = true;
            let tree = self.base().get_tree();
            CorpseYard::of(&tree)
                .bind_mut()
                .adopt(self.rig.clone(), corpses::ZOMBIE_SECONDS);
        }
    }

    fn physics_process(&mut self, delta: f64) {
        let _profile = crate::profile::scope("zombies (physics)");
        let dt = delta as f32;
        self.stagger_for = (self.stagger_for - dt).max(0.0);
        if !self.alive {
            self.moved = Vector3::ZERO;
            self.dead_for += dt;
            if self.dead_for >= self.corpse_time {
                self.base_mut().queue_free();
            }
            return;
        }
        self.groan(dt);
    }
}

#[godot_api]
impl Zombie {
    /// The server owns its health: a hit here only predicts the marker.
    #[func]
    pub fn apply_damage(&mut self, _amount: f32, _headshot: bool) -> bool {
        false
    }

    /// A knife strike: as [`apply_damage`](Self::apply_damage).
    #[func]
    pub fn apply_melee(&mut self, _amount: f32) -> bool {
        false
    }

    /// The local player's hit, travelling along `direction`, landed at
    /// `point` on hit shape `part` (-1 unknown) with force level `force`: a
    /// flinch now, before the server's word.
    #[func]
    pub fn react_to_hit(
        &mut self,
        direction: Vector3,
        killed: bool,
        point: Vector3,
        part: i64,
        force: f32,
    ) {
        if !killed {
            self.flinched_at = Some(now_seconds());
        }
        self.flinch(direction, killed, point, part, force);
    }

    /// Seconds left of a stagger, for tests.
    #[func]
    pub fn staggered_for(&self) -> f32 {
        self.stagger_for
    }

    #[func]
    pub fn is_alive(&self) -> bool {
        self.alive
    }

    /// Facing yaw in radians (0 faces -Z), for tests and captures.
    #[func]
    pub fn facing(&self) -> f32 {
        self.yaw
    }

    /// Places the drawn zombie where the server's was, facing `yaw` and
    /// moving at `velocity`, with its hit zones posed to match (also how
    /// captures pose one).
    #[func]
    pub fn net_apply(&mut self, feet: Vector3, yaw: f32, velocity: Vector3, walking: bool) {
        self.base_mut().set_global_position(feet);
        self.yaw = yaw;
        self.moved = velocity;
        if self.alive {
            let pose = shooter_sim::zombies::chase::hitbox_pose(to_sim(feet), yaw, walking);
            self.hitboxes.bind_mut().set_pose(Some(pose));
        }
    }

    /// The server's zombie swung (at a player or a window's boards).
    #[func]
    pub fn net_swing(&mut self) {
        self.rig.bind_mut().attack();
        let sound = self.attack_sounds.clone();
        self.say(sound);
    }

    /// The server's zombie died from a hit travelling along `direction` that
    /// landed on hit shape `part` with force level `force`.
    #[func]
    pub fn net_die(&mut self, direction: Vector3, part: i64, force: f32) {
        if !self.alive {
            return;
        }
        self.alive = false;
        let sound = self.death_sounds.clone();
        self.say(sound);
        self.hitboxes.bind_mut().set_pose(None);
        if let Some(mut assist) = self.base().try_get_node_as::<AimAssistTarget>("AimAssist") {
            assist.bind_mut().set_active(false);
        }
        let point = self.base().get_global_position() + Vector3::UP * 1.2;
        self.flinch(direction, true, point, part, force);
    }
}

impl Zombie {
    /// The flinch, or the fall, from a hit.
    pub fn flinch(
        &mut self,
        direction: Vector3,
        killed: bool,
        point: Vector3,
        part: i64,
        force: f32,
    ) {
        let push = crate::character::rig::push_of(direction, point, part, force);
        self.rig.bind_mut().hit_at(push, killed);
        // A heavy round (a shotgun blast, a sniper's) staggers it.
        if !killed && force >= STAGGER_FORCE && self.stagger_for <= 0.0 {
            self.stagger_for = STAGGER_SECONDS;
            self.rig.bind_mut().stagger();
        }
    }

    /// Whether the local player's own hit flinched it in the last `seconds`;
    /// answers yes once per hit.
    pub fn predicted_flinch(&mut self, seconds: f32) -> bool {
        let recent = self
            .flinched_at
            .is_some_and(|at| now_seconds() - at < f64::from(seconds));
        self.flinched_at = None;
        recent
    }

    fn groan(&mut self, dt: f32) {
        self.groan_in -= dt;
        if self.groan_in > 0.0 {
            return;
        }
        self.seed = self.seed.wrapping_add(1);
        self.groan_in = GROAN_MIN + rng::unit(self.seed, 1) * (GROAN_MAX - GROAN_MIN);
        let sound = self.groans.clone();
        self.say(sound);
    }

    fn say(&mut self, sound: Option<Gd<AudioStream>>) {
        if let (Some(voice), Some(sound)) = (&mut self.voice, sound) {
            voice.set_stream(&sound);
            voice.play();
        }
    }

    /// Animation updates every frame near the camera, every second frame
    /// further away, and every fourth frame far away.
    fn animation_interval(&self) -> u32 {
        let Some(camera) = self.base().get_viewport().and_then(|v| v.get_camera_3d()) else {
            return 1;
        };
        let distance = camera
            .get_global_position()
            .distance_to(self.base().get_global_position());
        if distance > QUARTER_RATE_DISTANCE {
            4
        } else if distance > HALF_RATE_DISTANCE {
            2
        } else {
            1
        }
    }

    fn frame(&self, velocity: Vector3) -> SoldierFrame {
        SoldierFrame {
            velocity,
            on_floor: true,
            yaw: self.yaw,
            alive: self.alive,
            ..SoldierFrame::default()
        }
    }
}

/// Engine time in seconds.
fn now_seconds() -> f64 {
    godot::classes::Time::singleton().get_ticks_usec() as f64 / 1e6
}
