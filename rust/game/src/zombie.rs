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

use godot::classes::{CharacterBody3D, ICharacterBody3D, Node3D};
use godot::prelude::*;
use shooter_sim::hitbox::HitboxPose;
use shooter_sim::movement::Stance;
use shooter_sim::zombies::{self, Attack, AttackEvent, ZombieRules};

use crate::aim_assist::AimAssistTarget;
use crate::character::rig::{SoldierFrame, SoldierRig, wrap_angle};
use crate::convert::{to_godot, to_sim};
use crate::hitbox_rig::HitboxRig;
use crate::target::{BODY_LAYER, WORLD_LAYER};

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
    /// Seconds a body stays on the ground before it is removed.
    #[export]
    #[init(val = 4.0)]
    corpse_time: f32,

    #[init(node = "Soldier")]
    rig: OnReady<Gd<SoldierRig>>,
    #[init(node = "Hitboxes")]
    hitboxes: OnReady<Gd<HitboxRig>>,

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
            .set_collision_mask(WORLD_LAYER | BODY_LAYER | PLAYER_LAYER);
        self.base_mut().add_to_group("zombies");
    }

    /// Animation runs per rendered frame; movement and attacks per tick.
    fn process(&mut self, delta: f64) {
        if self.visuals {
            let frame = self.frame(self.moved);
            self.rig.bind_mut().update(&frame, delta as f32);
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
        let target = self.nearest_player(position);
        let distance = target
            .as_ref()
            .map(|t| zombies::flat_distance(to_sim(position), to_sim(t.get_global_position())));
        match self.attack.step(&self.rules, distance, dt) {
            Some(AttackEvent::Swung) => self.rig.bind_mut().attack(),
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
            let goal = self.waypoint.unwrap_or_else(|| target.get_global_position());
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

        let pose = HitboxPose {
            feet: to_sim(self.base().get_global_position()),
            yaw: self.yaw,
            pitch: 0.0,
            stance: Stance::Standing,
        };
        self.hitboxes.bind_mut().set_pose(Some(pose));
    }
}

#[godot_api]
impl Zombie {
    /// Emitted for every damaging hit, before the zombie reacts.
    #[signal]
    pub fn hurt(killed: bool, headshot: bool, position: Vector3);

    /// Emitted when one of this zombie's swings lands on a player.
    #[signal]
    pub fn struck();

    /// Applies damage and returns true if this hit killed the zombie.
    #[func]
    pub fn apply_damage(&mut self, amount: f32, headshot: bool) -> bool {
        if !self.is_alive() {
            return false;
        }
        let amount = if self.insta_kill { self.health } else { amount };
        self.health -= amount;
        let killed = self.health <= 0.0;
        if killed {
            self.health = 0.0;
            self.base_mut().set_collision_layer(0);
            self.base_mut().set_collision_mask(WORLD_LAYER);
            self.hitboxes.bind_mut().set_pose(None);
            if let Some(mut assist) = self.base().try_get_node_as::<AimAssistTarget>("AimAssist") {
                assist.bind_mut().set_active(false);
            }
        }
        let position = self.base().get_global_position();
        self.signals().hurt().emit(killed, headshot, position);
        killed
    }

    /// Flinch from a hit travelling along `direction`, or fall away from the killing shot.
    #[func]
    pub fn react_to_hit(&mut self, direction: Vector3, killed: bool) {
        self.rig.bind_mut().hit(direction, killed);
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
}

impl Zombie {
    /// Health and speed for a spawn; call before the zombie enters the tree.
    pub fn configure(&mut self, max_health: f32, speed: f32) {
        self.max_health = max_health;
        self.speed = speed;
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
