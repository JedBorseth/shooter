//! `ZombieDirector`: runs a Zombies match. It counts down to each round,
//! spawns the round's zombies at the map's spawn points, pays points for
//! hits and kills, and ends the match when the player dies. The rules are in
//! `shooter_sim::zombies`.

use godot::classes::{INode3D, Node3D, PackedScene};
use godot::prelude::*;
use shooter_sim::rng;
use shooter_sim::zombies::{self, Phase, RoundEvent, Rounds, ZombieRules};

use crate::zombie::Zombie;

#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct ZombieDirector {
    base: Base<Node3D>,

    #[export]
    zombie_scene: OnEditor<Gd<PackedScene>>,
    /// The player the match is about. Its death ends the match.
    #[export]
    player_path: NodePath,
    /// Parent of the spawn points (any `Node3D` children).
    #[export]
    spawns_path: NodePath,
    /// Seconds before round 1 starts.
    #[export]
    #[init(val = 4.0)]
    first_break: f32,
    /// Spawn points closer than this to the player (metres) are skipped while
    /// any other is free.
    #[export]
    #[init(val = 8.0)]
    spawn_min_distance: f32,
    /// Points the player starts with.
    #[export]
    #[init(val = 500)]
    starting_points: i64,

    rules: ZombieRules,
    rounds: Option<Rounds>,
    points: i64,
    kills: i64,
    headshots: i64,
    events: Vec<RoundEvent>,
}

#[godot_api]
impl INode3D for ZombieDirector {
    fn ready(&mut self) {
        self.points = self.starting_points;
        self.rounds = Some(Rounds::new(self.first_break));
    }

    fn physics_process(&mut self, delta: f64) {
        let dt = delta as f32;
        let Some(mut rounds) = self.rounds else {
            return;
        };
        if rounds.phase == Phase::Over {
            return;
        }
        let player_alive = self
            .player()
            .map(|mut p| p.call("is_alive", &[]).try_to::<bool>().unwrap_or(false))
            .unwrap_or(false);
        if !player_alive {
            rounds.end();
            self.rounds = Some(rounds);
            let round = i64::from(rounds.round);
            self.signals().game_over().emit(round);
            return;
        }

        let mut events = std::mem::take(&mut self.events);
        events.clear();
        rounds.step(&self.rules, dt, &mut events);
        self.rounds = Some(rounds);
        for event in &events {
            match *event {
                RoundEvent::RoundStarted { round } => {
                    self.signals().round_started().emit(i64::from(round));
                }
                RoundEvent::RoundCleared { round } => {
                    self.signals().round_cleared().emit(i64::from(round));
                }
                RoundEvent::Spawn {
                    index,
                    health,
                    gait,
                } => {
                    let round = rounds.round;
                    self.spawn(round, index, health, gait.speed(&self.rules));
                }
            }
        }
        self.events = events;
    }
}

#[godot_api]
impl ZombieDirector {
    /// A new round began.
    #[signal]
    fn round_started(round: i64);

    /// Every zombie of the round is dead.
    #[signal]
    fn round_cleared(round: i64);

    /// Points were earned: `points` this time, `total` now.
    #[signal]
    fn points_awarded(points: i64, total: i64);

    /// The player died in round `round`.
    #[signal]
    fn game_over(round: i64);

    #[func]
    fn round(&self) -> i64 {
        self.rounds.map(|r| i64::from(r.round)).unwrap_or(0)
    }

    /// True while zombies are coming (not in a break, not over).
    #[func]
    fn is_fighting(&self) -> bool {
        self.rounds.is_some_and(|r| r.phase == Phase::Fighting)
    }

    #[func]
    fn is_over(&self) -> bool {
        self.rounds.is_some_and(|r| r.phase == Phase::Over)
    }

    /// Seconds left before the next round, or 0 during one.
    #[func]
    fn break_left(&self) -> f32 {
        self.rounds.map(|r| r.break_left()).unwrap_or(0.0)
    }

    /// Zombies still to kill this round, spawned or not.
    #[func]
    fn remaining(&self) -> i64 {
        self.rounds.map(|r| i64::from(r.remaining())).unwrap_or(0)
    }

    #[func]
    fn points(&self) -> i64 {
        self.points
    }

    #[func]
    fn kills(&self) -> i64 {
        self.kills
    }

    #[func]
    fn headshots(&self) -> i64 {
        self.headshots
    }

    /// Zombies of the round in play, alive or dead (tests).
    #[func]
    fn zombies(&self) -> Array<Gd<Zombie>> {
        let mut out = Array::new();
        for child in self.base().get_children().iter_shared() {
            if let Ok(zombie) = child.try_cast::<Zombie>() {
                out.push(&zombie);
            }
        }
        out
    }

    /// Ends the current break now (tests and impatient players).
    #[func]
    fn skip_break(&mut self) {
        if let Some(rounds) = &mut self.rounds
            && rounds.phase == Phase::Break
        {
            let mut fresh = Rounds::new(0.0);
            fresh.round = rounds.round;
            *rounds = fresh;
        }
    }

    /// A zombie this director spawned was hit.
    #[func]
    fn on_zombie_hurt(&mut self, killed: bool, headshot: bool) {
        let points = i64::from(zombies::points::for_hit(killed, headshot));
        self.points += points;
        if killed {
            self.kills += 1;
            if headshot {
                self.headshots += 1;
            }
            if let Some(rounds) = &mut self.rounds {
                rounds.on_killed();
            }
        }
        let total = self.points;
        self.signals().points_awarded().emit(points, total);
    }
}

impl ZombieDirector {
    fn player(&self) -> Option<Gd<Node3D>> {
        self.base().try_get_node_as::<Node3D>(&self.player_path)
    }

    fn spawn_points(&self) -> Vec<Vector3> {
        let Some(parent) = self.base().try_get_node_as::<Node>(&self.spawns_path) else {
            return Vec::new();
        };
        parent
            .get_children()
            .iter_shared()
            .filter_map(|c| c.try_cast::<Node3D>().ok())
            .map(|n| n.get_global_position())
            .collect()
    }

    /// A spawn point away from the player, picked by a hash of the round and
    /// zombie, so the same match spawns the same way.
    fn pick_spawn(&self, round: u32, index: u32) -> Option<Vector3> {
        let points = self.spawn_points();
        if points.is_empty() {
            return None;
        }
        let player = self.player().map(|p| p.get_global_position());
        let far: Vec<Vector3> = points
            .iter()
            .copied()
            .filter(|p| player.is_none_or(|pl| p.distance_to(pl) >= self.spawn_min_distance))
            .collect();
        let pool = if far.is_empty() { points } else { far };
        let pick = rng::hash(round, index) as usize % pool.len();
        Some(pool[pick])
    }

    fn spawn(&mut self, round: u32, index: u32, health: f32, speed: f32) {
        let Some(position) = self.pick_spawn(round, index) else {
            godot_error!("ZombieDirector: no spawn points under {}", self.spawns_path);
            return;
        };
        let Some(node) = self.zombie_scene.instantiate() else {
            return;
        };
        let Ok(mut zombie) = node.try_cast::<Zombie>() else {
            godot_error!("ZombieDirector: zombie_scene is not a Zombie");
            return;
        };
        zombie.set_name(&format!("Zombie_{round}_{index}"));
        zombie.bind_mut().configure(health, speed);
        // Face the player from the start.
        if let Some(player) = self.player() {
            let to = player.get_global_position() - position;
            zombie.set_rotation(Vector3::new(0.0, (-to.x).atan2(-to.z), 0.0));
        }
        zombie
            .signals()
            .hurt()
            .connect_other(&self.to_gd(), Self::on_zombie_hurt);
        self.base_mut().add_child(&zombie);
        zombie.set_global_position(position);
    }
}
