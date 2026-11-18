//! `Zombie`: walks in a straight line at the nearest living player, swings
//! when in reach, and takes damage per hit zone. Two hits down a player
//! (`shooter_sim::zombies::hit_damage`).
//!
//! There is no pathfinding on purpose: each tick the zombie heads straight for
//! its target, and `move_and_slide` slides it along whatever is in the way.
//! Two exceptions keep the straight lines from pinning zombies to walls once
//! the map has more than one yard: a zombie spawned in a side yard first walks
//! straight to that yard's gate (`waypoint`), and a zombie that stops getting
//! anywhere walks straight to the open gate that best leads to its target.
//!
//! A zombie that spawns behind a window first tears down the window's
//! barrier (`scripts/zombies_barrier.gd`), one board at a time, then climbs
//! through. It groans now and then, snarls when it swings, and cries out when
//! it dies. Far from the camera its animation updates less often.

use godot::classes::{AudioStream, AudioStreamPlayer3D, CharacterBody3D, ICharacterBody3D, Node3D};
use godot::prelude::*;
use shooter_sim::hitbox::HitboxPose;
use shooter_sim::movement::Stance;
use shooter_sim::rng;
use shooter_sim::zombies::{self, Attack, AttackEvent, ZombieRules};

use crate::aim_assist::AimAssistTarget;
use crate::character::corpses::{self, CorpseYard};
use crate::character::rig::{SoldierFrame, SoldierRig, wrap_angle};
use crate::convert::{to_godot, to_sim};
use crate::hitbox_rig::HitboxRig;
use crate::target::{BODY_LAYER, WORLD_LAYER};

/// Collision layer 6 ("barriers"): window boards only zombies bump into.
pub const BARRIER_LAYER: u32 = 1 << 5;

/// Collision layer 2 ("players"), as a bit mask.
const PLAYER_LAYER: u32 = 1 << 1;
const GRAVITY: f32 = 20.0;
/// Radians per second the body turns toward its target (visual only; the
/// walk direction is always the straight line).
const TURN_RATE: f32 = 10.0;
/// A waypoint counts as reached within this many metres.
const WAYPOINT_REACH: f32 = 1.5;
/// Seconds of moving under a quarter of its speed before a zombie heads for a gate.
const STUCK_SECONDS: f32 = 1.0;
/// A zombie this close to its barrier tears at it.
const BARRIER_REACH: f32 = 1.7;
/// Seconds per board torn off.
const TEAR_SECONDS: f32 = 1.1;
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

    /// Set before the zombie enters the tree (the director scales it per round).
    #[export]
    #[init(val = 150.0)]
    max_health: f32,
    /// Ground speed in m/s.
    #[export]
    #[init(val = 1.4)]
    speed: f32,
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

    rules: ZombieRules,
    attack: Attack,
    health: f32,
    yaw: f32,
    fall_speed: f32,
    /// Ground velocity after the last move, for the animation.
    moved: Vector3,
    dead_for: f32,
    hits_landed: u32,
    visuals: bool,
    /// Walk here before chasing (a gate).
    waypoint: Option<Vector3>,
    /// The open gates between the map's yards.
    gates: Vec<Vector3>,
    stuck_for: f32,
    /// Any hit kills (the Insta-Kill power-up).
    insta_kill: bool,
    /// The window barrier between this zombie and the arena, until it is through.
    barrier: Option<Gd<Node3D>>,
    tear_for: f32,
    groan_in: f32,
    /// Rendered frames since the last animation update, and their time.
    anim_frames: u32,
    anim_time: f32,
    seed: u32,
    /// The dead body has gone to the corpse yard.
    body_left: bool,
}

#[godot_api]
impl ICharacterBody3D for Zombie {
    fn ready(&mut self) {
        self.visuals = crate::character::rig::visuals_enabled();
        self.health = self.max_health;
        self.yaw = self.base().get_global_rotation().y;
        self.base_mut().set_rotation(Vector3::ZERO);
        self.base_mut().set_collision_layer(BODY_LAYER);
        self.base_mut()
            .set_collision_mask(WORLD_LAYER | BODY_LAYER | PLAYER_LAYER | BARRIER_LAYER);
        self.base_mut().add_to_group("zombies");
        self.voice = self.base().try_get_node_as::<AudioStreamPlayer3D>("Voice");
        self.seed = self.base().instance_id().to_i64() as u32;
        self.groan_in = GROAN_MIN + rng::unit(self.seed, 0) * (GROAN_MAX - GROAN_MIN);
    }

    /// Animation runs per rendered frame (less often far away); movement and
    /// attacks per tick.
    fn process(&mut self, delta: f64) {
        if !self.visuals {
            return;
        }
        self.anim_frames += 1;
        self.anim_time += delta as f32;
        let every = self.animation_interval();
        // Spread the zombies over the frames, so they do not all update at once.
        if !(self.anim_frames + self.seed).is_multiple_of(every) && self.is_alive() {
            return;
        }
        if self.body_left {
            return;
        }
        let frame = self.frame(self.moved);
        let dt = std::mem::take(&mut self.anim_time);
        self.anim_frames = 0;
        self.rig.bind_mut().update(&frame, dt);
        if !self.is_alive() {
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
        let dt = delta as f32;
        if !self.is_alive() {
            self.moved = Vector3::ZERO;
            self.dead_for += dt;
            if self.dead_for >= self.corpse_time {
                self.base_mut().queue_free();
            }
            return;
        }

        let position = self.base().get_global_position();
        self.groan(dt);
        if self.tear_barrier(position, dt) {
            self.moved = Vector3::ZERO;
            return;
        }
        let target = self.nearest_player(position);
        let distance = target
            .as_ref()
            .map(|t| zombies::flat_distance(to_sim(position), to_sim(t.get_global_position())));
        match self.attack.step(&self.rules, distance, dt) {
            Some(AttackEvent::Swung) => {
                self.rig.bind_mut().attack();
                let sound = self.attack_sounds.clone();
                self.say(sound);
            }
            Some(AttackEvent::Hit) => {
                if let Some(target) = &target {
                    self.strike(target.clone());
                }
            }
            Some(AttackEvent::Missed) | None => {}
        }

        if self
            .waypoint
            .is_some_and(|w| zombies::flat_distance(to_sim(position), to_sim(w)) < WAYPOINT_REACH)
        {
            self.waypoint = None;
        }
        let mut horizontal = Vector3::ZERO;
        if let Some(target) = &target {
            let goal = self
                .waypoint
                .unwrap_or_else(|| target.get_global_position());
            let direction = to_godot(zombies::chase_direction(to_sim(position), to_sim(goal)));
            if direction != Vector3::ZERO {
                let want = (-direction.x).atan2(-direction.z);
                let turn = TURN_RATE * dt;
                self.yaw += wrap_angle(want - self.yaw).clamp(-turn, turn);
            }
            // Stand still to swing, and stop just inside reach instead of
            // pushing into the player.
            let close = distance.is_some_and(|d| d <= self.rules.attack_reach * 0.8);
            if !self.attack.swinging() && !close {
                horizontal = direction * self.speed;
            }
        }

        if self.base().is_on_floor() {
            self.fall_speed = 0.0;
        } else {
            self.fall_speed += GRAVITY * dt;
        }
        let velocity = horizontal + Vector3::DOWN * self.fall_speed;
        let mut body = self.base_mut();
        body.set_velocity(velocity);
        body.move_and_slide();
        let moved = body.get_velocity();
        drop(body);
        self.moved = moved;
        self.check_stuck(horizontal, moved, dt, target.as_ref());

        let pose = HitboxPose::new(
            to_sim(self.base().get_global_position()),
            self.yaw,
            0.0,
            Stance::Standing,
        );
        self.hitboxes.bind_mut().set_pose(Some(pose));
    }
}

#[godot_api]
impl Zombie {
    /// Emitted for every damaging hit, before the zombie reacts.
    #[signal]
    pub fn hurt(killed: bool, headshot: bool, melee: bool, position: Vector3);

    /// Emitted when one of this zombie's swings lands on a player.
    #[signal]
    pub fn struck();

    /// Applies damage and returns true if this hit killed the zombie.
    #[func]
    pub fn apply_damage(&mut self, amount: f32, headshot: bool) -> bool {
        self.take_damage(amount, headshot, false)
    }

    /// A knife strike: damage like [`apply_damage`](Self::apply_damage), and
    /// a kill pays the knife's points.
    #[func]
    pub fn apply_melee(&mut self, amount: f32) -> bool {
        self.take_damage(amount, false, true)
    }

    fn take_damage(&mut self, amount: f32, headshot: bool, melee: bool) -> bool {
        if !self.is_alive() {
            return false;
        }
        let amount = if self.insta_kill { self.health } else { amount };
        self.health -= amount;
        let killed = self.health <= 0.0;
        if killed {
            self.health = 0.0;
            let sound = self.death_sounds.clone();
            self.say(sound);
            self.base_mut().set_collision_layer(0);
            self.base_mut().set_collision_mask(WORLD_LAYER);
            self.hitboxes.bind_mut().set_pose(None);
            if let Some(mut assist) = self.base().try_get_node_as::<AimAssistTarget>("AimAssist") {
                assist.bind_mut().set_active(false);
            }
        }
        let position = self.base().get_global_position();
        self.signals()
            .hurt()
            .emit(killed, headshot, melee, position);
        killed
    }

    /// Flinch from a hit travelling along `direction` that landed at `point`
    /// on hit shape `part` (-1 unknown) with force level `force`, or fall
    /// with the killing shot's push.
    #[func]
    pub fn react_to_hit(
        &mut self,
        direction: Vector3,
        killed: bool,
        point: Vector3,
        part: i64,
        force: f32,
    ) {
        let push = crate::character::rig::push_of(direction, point, part, force);
        self.rig.bind_mut().hit_at(push, killed);
    }

    #[func]
    pub fn health(&self) -> f32 {
        self.health
    }

    #[func]
    pub fn is_alive(&self) -> bool {
        self.health > 0.0
    }

    /// Swings that have landed on a player, for tests.
    #[func]
    pub fn hits_landed(&self) -> i64 {
        i64::from(self.hits_landed)
    }

    /// Facing yaw in radians (0 faces -Z), for tests.
    #[func]
    pub fn facing(&self) -> f32 {
        self.yaw
    }

    /// Walks straight here before chasing (a side yard's gate).
    #[func]
    pub fn set_waypoint(&mut self, waypoint: Vector3) {
        self.waypoint = Some(waypoint);
    }

    /// While on, any hit kills (Insta-Kill).
    #[func]
    pub fn set_insta_kill(&mut self, on: bool) {
        self.insta_kill = on;
    }

    /// The window barrier this zombie must tear down to get in.
    #[func]
    pub fn set_barrier(&mut self, barrier: Gd<Node3D>) {
        self.barrier = Some(barrier);
    }

    /// Whether it is still outside, at or on its way to a barrier (tests).
    #[func]
    pub fn behind_barrier(&self) -> bool {
        self.barrier.is_some()
    }
}

impl Zombie {
    /// Health and speed for a spawn; call before the zombie enters the tree.
    pub fn configure(&mut self, max_health: f32, speed: f32) {
        self.max_health = max_health;
        self.speed = speed;
    }

    /// Tears at the barrier while next to it and boards remain. Returns true
    /// while the zombie is busy with it (and should not move).
    fn tear_barrier(&mut self, position: Vector3, dt: f32) -> bool {
        let Some(mut barrier) = self.barrier.clone() else {
            return false;
        };
        if !barrier.is_instance_valid() {
            self.barrier = None;
            return false;
        }
        let boards = barrier.call("boards", &[]).try_to::<i64>().unwrap_or(0);
        let at = barrier.get_global_position();
        let near = zombies::flat_distance(to_sim(position), to_sim(at)) < BARRIER_REACH;
        if boards == 0 {
            // Through the window once past it: chase from here on.
            if self.waypoint.is_none() {
                self.barrier = None;
            }
            return false;
        }
        if !near {
            return false;
        }
        let to = at - position;
        self.yaw = (-to.x).atan2(-to.z);
        self.tear_for += dt;
        if self.tear_for >= TEAR_SECONDS {
            self.tear_for = 0.0;
            self.rig.bind_mut().attack();
            barrier.call("tear", &[]);
        }
        true
    }

    fn groan(&mut self, dt: f32) {
        self.groan_in -= dt;
        if self.groan_in > 0.0 {
            return;
        }
        self.counter_groan();
        let sound = self.groans.clone();
        self.say(sound);
    }

    fn counter_groan(&mut self) {
        self.seed = self.seed.wrapping_add(1);
        self.groan_in = GROAN_MIN + rng::unit(self.seed, 1) * (GROAN_MAX - GROAN_MIN);
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

    /// The gates a stuck zombie may head for.
    pub fn set_gates(&mut self, gates: Vec<Vector3>) {
        self.gates = gates;
    }

    /// A zombie walking into a wall heads for the open gate on the shortest
    /// way to its target, then chases again.
    fn check_stuck(
        &mut self,
        wanted: Vector3,
        moved: Vector3,
        dt: f32,
        target: Option<&Gd<Node3D>>,
    ) {
        let want = Vector2::new(wanted.x, wanted.z).length();
        let got = Vector2::new(moved.x, moved.z).length();
        if want < 0.1 || got > want * 0.25 || self.waypoint.is_some() {
            self.stuck_for = 0.0;
            return;
        }
        self.stuck_for += dt;
        if self.stuck_for < STUCK_SECONDS {
            return;
        }
        self.stuck_for = 0.0;
        let Some(target) = target.map(|t| t.get_global_position()) else {
            return;
        };
        let here = self.base().get_global_position();
        self.waypoint = self
            .gates
            .iter()
            .copied()
            .filter(|g| g.distance_to(here) > WAYPOINT_REACH * 2.0)
            .min_by(|a, b| {
                let cost = |g: &Vector3| g.distance_to(here) + g.distance_to(target);
                cost(a).total_cmp(&cost(b))
            });
    }

    fn frame(&self, velocity: Vector3) -> SoldierFrame {
        SoldierFrame {
            velocity,
            on_floor: true,
            yaw: self.yaw,
            alive: self.is_alive(),
            ..SoldierFrame::default()
        }
    }

    /// The closest living player, by straight-line distance.
    fn nearest_player(&self, from: Vector3) -> Option<Gd<Node3D>> {
        let tree = self.base().get_tree();
        let mut best: Option<(Gd<Node3D>, f32)> = None;
        for node in tree.get_nodes_in_group("players").iter_shared() {
            let Ok(mut player) = node.try_cast::<Node3D>() else {
                continue;
            };
            let alive = player
                .call("is_alive", &[])
                .try_to::<bool>()
                .unwrap_or(false);
            if !alive {
                continue;
            }
            let distance = from.distance_squared_to(player.get_global_position());
            if best.as_ref().is_none_or(|(_, d)| distance < *d) {
                best = Some((player, distance));
            }
        }
        best.map(|(player, _)| player)
    }

    fn strike(&mut self, mut target: Gd<Node3D>) {
        let health = target.call("health", &[]).try_to::<f32>().unwrap_or(0.0);
        let max = target
            .call("max_health", &[])
            .try_to::<f32>()
            .unwrap_or(100.0);
        let damage = zombies::hit_damage(health, max);
        if damage <= 0.0 {
            return;
        }
        let from = self.base().get_global_position() + Vector3::UP * 1.3;
        target.call("take_damage", &[damage.to_variant(), from.to_variant()]);
        self.hits_landed += 1;
        self.signals().struck().emit();
    }
}
