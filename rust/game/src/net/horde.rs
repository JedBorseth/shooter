//! The dedicated server's Zombies horde: the rounds, every zombie's body and
//! mind, the map's boarded windows, and what clients are sent of them.
//!
//! Rules are in `shooter_sim::zombies` (rounds, gaits, the two-hit rule,
//! points) and `shooter_sim::zombies::chase` (one zombie's mind). This module
//! gives each zombie a capsule body that `move_and_slide`s through the map,
//! picks spawn points, tears boards off windows, and applies the damage the
//! server's shots, knives, and grenades decide. `NetServer` owns one while
//! it runs Zombies, and hands it the players each tick.
//!
//! The map marks its zombie spawns as `Node3D` children of `Spawns`, with
//! optional metadata: `zone` (spawn only once that zone is open), `via` (a
//! point to walk to first, such as the window's far side), and `barrier`
//! (the path to the window's barrier, `scripts/zombies_barrier.gd`, which
//! the zombie tears down first).
//!
//! Doors are `Doors`' children with a `zone` and a `price`
//! (`scripts/zombies_door.gd`): opening one (the server decides when a
//! player buys it) adds its zone's spawn points, and its position becomes a
//! gate stuck zombies head for. Kills drop power-ups (`zombies::PowerUp`)
//! that any standing player picks up for the whole team.
//!
//! Zombie bodies collide with the map, each other, players, and window
//! boards; player bodies do not collide with zombies, so a player's
//! prediction never runs into a zombie drawn where the server no longer has
//! it.

use godot::classes::{CapsuleShape3D, CharacterBody3D, CollisionShape3D, Node3D};
use godot::prelude::*;
use shooter_sim::net::Tick;
use shooter_sim::net::horde::HordeHistory;
use shooter_sim::net::lag::Target;
use shooter_sim::net::protocol::{
    self, MAX_ZOMBIES, ZOMBIE_SEND_TICKS, ZombieView, ZombiesEvent, ZombiesPhase,
};
use shooter_sim::rng;
use shooter_sim::zombies::chase::{self, ChaseEvent, Chaser, Prey, Window, World};
use shooter_sim::zombies::{self, Phase, PowerUp, RoundEvent, Rounds, ZombieRules};
use shooter_sim::{TICK_DT, Vec3};

use super::machines::Machines;
use crate::convert::{to_godot, to_sim};
use crate::target::WORLD_LAYER;
use crate::zombie::BARRIER_LAYER;

/// Collision layer 10 ("zombies"): the server's zombie bodies.
pub const ZOMBIE_LAYER: u32 = 1 << 9;
const PLAYER_LAYER: u32 = crate::body::PLAYER_LAYER;
/// Seconds before round 1, after the match starts.
const FIRST_BREAK: f32 = 4.0;
/// Spawn points closer than this to a player (metres) are skipped while any
/// other is free.
const SPAWN_MIN_DISTANCE: f32 = 8.0;
/// Seconds a dead zombie stays in snapshots, so every client sees it fall.
const DEAD_LINGER: f32 = 0.6;
const GRAVITY: f32 = 20.0;
const CAPSULE_RADIUS: f32 = 0.3;
const CAPSULE_HEIGHT: f32 = 1.75;
/// Boards on a full window (`zombies_barrier.gd`'s `BOARDS`).
pub const WINDOW_BOARDS: u8 = 6;
/// Where a zombie's swing comes from, above its feet, for the damage
/// direction.
const STRIKE_HEIGHT: f32 = 1.3;

/// A player as the horde sees them this tick.
#[derive(Clone, Copy, Debug)]
pub struct HordePlayer {
    pub id: u8,
    pub feet: Vec3,
    /// Standing: alive and not downed. Zombies go only for standing players.
    pub standing: bool,
}

/// What the horde did this tick that the server must act on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HordeEvent {
    /// Round `round` began.
    RoundStarted(u32),
    /// A zombie's swing landed on player `player`, from `from`.
    Struck { player: u8, from: Vec3 },
    /// A window's boards changed (torn off or put back).
    Boards { window: u8, boards: u8 },
    /// A kill dropped a power-up.
    PowerUpDropped { id: u8, kind: PowerUp, at: Vec3 },
    /// Player `by` picked up power-up `id` (a Nuke has already killed
    /// everything, `nuked` zombies).
    PowerUpTaken {
        id: u8,
        kind: PowerUp,
        by: u8,
        nuked: u32,
    },
    /// Power-up `id` ran out on the ground.
    PowerUpExpired { id: u8 },
    /// Insta-Kill or Double Points started or ran out.
    TimersChanged,
}

/// What a hit did to a zombie.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZombieHurt {
    pub killed: bool,
    pub position: Vec3,
}

#[derive(Clone)]
struct SpawnPoint {
    position: Vec3,
    zone: String,
    via: Option<Vec3>,
    window: Option<usize>,
}

struct MapDoor {
    node: Gd<Node3D>,
    zone: String,
    price: u32,
    /// Where the zone meets the rest of the map.
    gate: Vec3,
    open: bool,
}

struct PowerUpDrop {
    id: u8,
    kind: PowerUp,
    at: Vec3,
    left: f32,
}

struct MapWindow {
    node: Gd<Node3D>,
    position: Vec3,
    boards: u8,
}

struct Zombie {
    id: u8,
    body: Gd<CharacterBody3D>,
    chaser: Chaser,
    health: f32,
    alive: bool,
    dead_for: f32,
    fall_speed: f32,
    swings: u8,
    hits: u8,
    hit_heading: u8,
    hit_part: u8,
}

impl Zombie {
    fn feet(&self) -> Vec3 {
        to_sim(self.body.get_global_position())
    }
}

pub struct Horde {
    rules: ZombieRules,
    rounds: Rounds,
    parent: Gd<Node3D>,
    spawns: Vec<SpawnPoint>,
    windows: Vec<MapWindow>,
    zombies: Vec<Zombie>,
    next_id: u8,
    /// Zones whose spawn points are in use ("" is the starting area).
    open_zones: Vec<String>,
    /// Gates between open zones, for zombies that walk into walls.
    gates: Vec<Vec3>,
    /// What clients were sent, for lag compensation.
    history: HordeHistory,
    last_send: Option<Tick>,
    events: Vec<RoundEvent>,
    /// Kills and power-up drops this round.
    round_kills: u32,
    round_drops: u32,
    doors: Vec<MapDoor>,
    drops: Vec<PowerUpDrop>,
    next_drop_id: u8,
    insta_kill_left: f32,
    double_points_left: f32,
    /// What happened since the server last asked.
    pending: Vec<HordeEvent>,
    /// The power switch and the drink machines.
    pub machines: Machines,
}

impl Horde {
    /// A horde for `map` (already in the tree), whose bodies go under
    /// `parent`. `None` if the map marks no zombie spawns.
    pub fn new(map: &Gd<Node>, parent: Gd<Node3D>) -> Option<Self> {
        let (spawns, windows) = map_spawns(map);
        if spawns.is_empty() {
            godot_error!("server: Zombies needs zombie spawn points under the map's `Spawns`");
            return None;
        }
        godot_print!(
            "server: Zombies with {} zombie spawns and {} windows",
            spawns.len(),
            windows.len()
        );
        let mut horde = Self {
            rules: ZombieRules::default(),
            rounds: Rounds::new(FIRST_BREAK),
            parent,
            spawns,
            windows,
            zombies: Vec::new(),
            next_id: 0,
            open_zones: vec![String::new()],
            gates: Vec::new(),
            history: HordeHistory::default(),
            last_send: None,
            events: Vec::new(),
            round_kills: 0,
            round_drops: 0,
            doors: map_doors(map),
            drops: Vec::new(),
            next_drop_id: 0,
            insta_kill_left: 0.0,
            double_points_left: 0.0,
            pending: Vec::new(),
            machines: Machines::new(map),
        };
        horde.reset();
        Some(horde)
    }

    /// A fresh game: no zombies, every window boarded, only the starting
    /// area open, and a short break before round 1.
    pub fn reset(&mut self) {
        for zombie in self.zombies.drain(..) {
            let mut body = zombie.body;
            body.queue_free();
        }
        self.rounds = Rounds::new(FIRST_BREAK);
        self.open_zones = vec![String::new()];
        self.gates.clear();
        self.history.clear();
        self.round_kills = 0;
        self.round_drops = 0;
        self.drops.clear();
        self.insta_kill_left = 0.0;
        self.double_points_left = 0.0;
        self.pending.clear();
        self.machines.reset();
        for door in &mut self.doors {
            door.open = false;
            door.node
                .call("set_open", &[false.to_variant(), false.to_variant()]);
        }
        for window in &mut self.windows {
            window.boards = WINDOW_BOARDS;
            window
                .node
                .call("set_boards", &[i64::from(WINDOW_BOARDS).to_variant()]);
        }
    }

    /// Ends the game: everyone is down.
    pub fn end(&mut self) {
        self.rounds.end();
    }

    pub fn over(&self) -> bool {
        self.rounds.phase == Phase::Over
    }

    pub fn round(&self) -> u32 {
        self.rounds.round
    }

    pub fn fighting(&self) -> bool {
        self.rounds.phase == Phase::Fighting
    }

    /// Zombies alive now.
    pub fn alive(&self) -> usize {
        self.zombies.iter().filter(|z| z.alive).count()
    }

    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    pub fn window_boards(&self, index: usize) -> Option<u8> {
        self.windows.get(index).map(|w| w.boards)
    }

    /// The round as everyone is told it; `now` is the server tick.
    pub fn round_event(&self, now: Tick) -> ZombiesEvent {
        let phase = match self.rounds.phase {
            Phase::Break => ZombiesPhase::Break,
            Phase::Fighting => ZombiesPhase::Fighting,
            Phase::Over => ZombiesPhase::Over,
        };
        let starts_at = (self.rounds.phase == Phase::Break).then(|| {
            now + (self.rounds.break_left() * shooter_sim::TICK_RATE as f32).ceil() as Tick
        });
        ZombiesEvent::Round {
            round: self.rounds.round,
            phase,
            starts_at,
            remaining: self.rounds.remaining(),
        }
    }

    /// Every window's boards, for a player joining.
    pub fn window_events(&self) -> Vec<ZombiesEvent> {
        self.windows
            .iter()
            .enumerate()
            .map(|(i, w)| ZombiesEvent::Boards {
                window: i as u8,
                boards: w.boards,
            })
            .collect()
    }

    /// Ends the current break now (tests).
    pub fn skip_break(&mut self) {
        if self.rounds.phase == Phase::Break {
            let mut fresh = Rounds::new(0.0);
            fresh.round = self.rounds.round;
            self.rounds = fresh;
        }
    }

    /// Runs one tick: the round, spawns, and every zombie.
    pub fn step(&mut self, players: &[HordePlayer]) {
        let dt = TICK_DT;
        let mut out = std::mem::take(&mut self.pending);
        let mut events = std::mem::take(&mut self.events);
        events.clear();
        self.rounds.step(&self.rules, dt, &mut events);
        for event in &events {
            match *event {
                RoundEvent::RoundStarted { round } => {
                    self.round_kills = 0;
                    self.round_drops = 0;
                    out.push(HordeEvent::RoundStarted(round));
                }
                RoundEvent::RoundCleared { .. } => {}
                RoundEvent::Spawn {
                    index,
                    health,
                    gait,
                } => {
                    let round = self.rounds.round;
                    self.spawn(round, index, health, gait.speed(&self.rules), players);
                }
            }
        }
        self.events = events;

        let prey: Vec<Prey> = players
            .iter()
            .filter(|p| p.standing)
            .map(|p| Prey {
                id: p.id,
                position: p.feet,
            })
            .collect();
        let mut windows: Vec<Window> = self
            .windows
            .iter()
            .map(|w| Window {
                position: w.position,
                boards: w.boards,
            })
            .collect();
        let over = self.over();
        for zombie in &mut self.zombies {
            if !zombie.alive {
                zombie.dead_for += dt;
                continue;
            }
            let at = zombie.feet();
            let world = World {
                prey: if over { &[] } else { &prey },
                gates: &self.gates,
                windows: &windows,
            };
            let plan = zombie.chaser.plan(&self.rules, at, &world, dt);
            match plan.event {
                Some(ChaseEvent::Swung) => zombie.swings = zombie.swings.wrapping_add(1),
                Some(ChaseEvent::Struck(player)) => out.push(HordeEvent::Struck {
                    player,
                    from: at + Vec3::Y * STRIKE_HEIGHT,
                }),
                Some(ChaseEvent::Tore(index)) => {
                    zombie.swings = zombie.swings.wrapping_add(1);
                    if let Some(window) = windows.get_mut(index)
                        && window.boards > 0
                    {
                        window.boards -= 1;
                        out.push(HordeEvent::Boards {
                            window: index as u8,
                            boards: window.boards,
                        });
                    }
                }
                None => {}
            }
            let goal = plan
                .target
                .and_then(|(id, _)| prey.iter().find(|p| p.id == id))
                .map(|p| p.position);
            let body = &mut zombie.body;
            if body.is_on_floor() {
                zombie.fall_speed = 0.0;
            } else {
                zombie.fall_speed += GRAVITY * dt;
            }
            body.set_velocity(to_godot(plan.velocity) + Vector3::DOWN * zombie.fall_speed);
            body.move_and_slide();
            let moved = to_sim(body.get_velocity());
            zombie
                .chaser
                .moved(zombie.feet(), plan.velocity, moved, goal, &self.gates, dt);
        }
        for (index, window) in windows.iter().enumerate() {
            if self.windows[index].boards != window.boards {
                self.set_boards(index, window.boards);
            }
        }
        self.zombies.retain_mut(|zombie| {
            let keep = zombie.alive || zombie.dead_for < DEAD_LINGER;
            if !keep {
                zombie.body.queue_free();
            }
            keep
        });
        self.pending = out;
        if !over {
            self.update_power_ups(players, dt);
        }
    }

    /// What happened since the last call, oldest first.
    pub fn take_events(&mut self) -> Vec<HordeEvent> {
        std::mem::take(&mut self.pending)
    }

    /// Ages the drops and the timed power-ups, and hands drops to the
    /// standing players who walk over them.
    fn update_power_ups(&mut self, players: &[HordePlayer], dt: f32) {
        let was = (self.insta_kill_left > 0.0, self.double_points_left > 0.0);
        self.insta_kill_left = (self.insta_kill_left - dt).max(0.0);
        self.double_points_left = (self.double_points_left - dt).max(0.0);
        if (self.insta_kill_left > 0.0, self.double_points_left > 0.0) != was {
            self.pending.push(HordeEvent::TimersChanged);
        }
        let mut taken = Vec::new();
        self.drops.retain_mut(|drop| {
            drop.left -= dt;
            let by = players.iter().find(|p| {
                p.standing
                    && zombies::flat_distance(p.feet, drop.at) < zombies::PICKUP_RADIUS
                    && (p.feet.y - drop.at.y).abs() < 2.5
            });
            if let Some(by) = by {
                taken.push((drop.id, drop.kind, by.id));
                return false;
            }
            if drop.left <= 0.0 {
                self.pending
                    .push(HordeEvent::PowerUpExpired { id: drop.id });
                return false;
            }
            true
        });
        for (id, kind, by) in taken {
            self.collect(id, kind, by);
        }
    }

    fn collect(&mut self, id: u8, kind: PowerUp, by: u8) {
        let mut nuked = 0;
        match kind {
            PowerUp::MaxAmmo => {}
            PowerUp::InstaKill => {
                self.insta_kill_left = zombies::POWER_UP_SECONDS;
                self.pending.push(HordeEvent::TimersChanged);
            }
            PowerUp::DoublePoints => {
                self.double_points_left = zombies::POWER_UP_SECONDS;
                self.pending.push(HordeEvent::TimersChanged);
            }
            PowerUp::Nuke => {
                // Its kills pay nothing on their own and drop nothing.
                let ids: Vec<u8> = self
                    .zombies
                    .iter()
                    .filter(|z| z.alive)
                    .map(|z| z.id)
                    .collect();
                for zombie in ids {
                    if self.kill(zombie, Vec3::Y).is_some() {
                        nuked += 1;
                    }
                }
            }
        }
        self.pending.push(HordeEvent::PowerUpTaken {
            id,
            kind,
            by,
            nuked,
        });
    }

    /// Kills zombie `id` outright, as a Nuke does: no drop.
    fn kill(&mut self, id: u8, direction: Vec3) -> Option<()> {
        let zombie = self.zombies.iter_mut().find(|z| z.id == id && z.alive)?;
        let damage = zombie.health + 1.0;
        let drops = std::mem::replace(&mut self.round_drops, zombies::MAX_DROPS_PER_ROUND);
        self.hurt(id, damage, direction, None, false);
        self.round_drops = drops;
        Some(())
    }

    /// Seconds of Insta-Kill and Double Points left.
    pub fn timers(&self) -> (f32, f32) {
        (self.insta_kill_left, self.double_points_left)
    }

    /// Whether points are doubled now.
    pub fn double_points(&self) -> bool {
        self.double_points_left > 0.0
    }

    /// The timers as everyone is told them; `now` is the server tick.
    pub fn timers_event(&self, now: Tick) -> ZombiesEvent {
        let until = |left: f32| {
            (left > 0.0).then(|| now + (left * shooter_sim::TICK_RATE as f32).ceil() as Tick)
        };
        ZombiesEvent::PowerUpTimers {
            insta_kill_until: until(self.insta_kill_left),
            double_points_until: until(self.double_points_left),
        }
    }

    /// Every power-up lying on the ground, for a player joining.
    pub fn drop_events(&self) -> Vec<ZombiesEvent> {
        self.drops
            .iter()
            .map(|d| ZombiesEvent::PowerUpDropped {
                id: d.id,
                kind: d.kind,
                at: d.at,
            })
            .collect()
    }

    /// Drops a power-up at `at` (tests).
    pub fn drop_power_up(&mut self, kind: PowerUp, at: Vec3) {
        let id = self.next_drop_id;
        self.next_drop_id = self.next_drop_id.wrapping_add(1);
        self.drops.push(PowerUpDrop {
            id,
            kind,
            at,
            left: zombies::POWER_UP_LIFETIME,
        });
        self.pending
            .push(HordeEvent::PowerUpDropped { id, kind, at });
    }

    /// Power-ups lying on the ground.
    pub fn drops_on_ground(&self) -> usize {
        self.drops.len()
    }

    /// The door whose collider is `node`, if it is one.
    pub fn door_of(&self, node: &Gd<Node>) -> Option<usize> {
        self.doors
            .iter()
            .position(|d| d.node.instance_id() == node.instance_id())
    }

    /// The window whose barrier is `node`, if it is one.
    pub fn window_of(&self, node: &Gd<Node>) -> Option<usize> {
        self.windows
            .iter()
            .position(|w| w.node.instance_id() == node.instance_id())
    }

    /// Door `index`'s node (tests).
    pub fn door_node(&self, index: usize) -> Option<Gd<Node>> {
        self.doors.get(index).map(|d| d.node.clone().upcast())
    }

    /// Window `index`'s barrier (tests).
    pub fn window_node(&self, index: usize) -> Option<Gd<Node>> {
        self.windows.get(index).map(|w| w.node.clone().upcast())
    }

    /// Door `index`'s price, if it is still closed.
    pub fn door_price(&self, index: usize) -> Option<u32> {
        self.doors.get(index).filter(|d| !d.open).map(|d| d.price)
    }

    pub fn door_count(&self) -> usize {
        self.doors.len()
    }

    pub fn door_open(&self, index: usize) -> bool {
        self.doors.get(index).is_some_and(|d| d.open)
    }

    /// Opens door `index`: its zone's spawns join the rotation and its gate
    /// guides stuck zombies.
    pub fn open_door(&mut self, index: usize) -> bool {
        let Some(door) = self.doors.get_mut(index).filter(|d| !d.open) else {
            return false;
        };
        door.open = true;
        door.node
            .call("set_open", &[true.to_variant(), false.to_variant()]);
        if !self.open_zones.contains(&door.zone) {
            self.open_zones.push(door.zone.clone());
        }
        self.gates.push(door.gate);
        true
    }

    /// Every door's state, for a player joining or a new game.
    pub fn door_events(&self) -> Vec<ZombiesEvent> {
        self.doors
            .iter()
            .enumerate()
            .map(|(i, d)| ZombiesEvent::Door {
                door: i as u8,
                open: d.open,
            })
            .collect()
    }

    /// Puts boards back on (or takes them off) window `index`.
    pub fn set_boards(&mut self, index: usize, boards: u8) {
        let Some(window) = self.windows.get_mut(index) else {
            return;
        };
        window.boards = boards.min(WINDOW_BOARDS);
        window
            .node
            .call("set_boards", &[i64::from(window.boards).to_variant()]);
    }

    /// The horde to send, if a send is due at `tick`. Records what was sent
    /// for lag compensation.
    pub fn views_for(&mut self, tick: Tick) -> Option<Vec<ZombieView>> {
        if self
            .last_send
            .is_some_and(|last| tick.saturating_sub(last) < ZOMBIE_SEND_TICKS)
        {
            return None;
        }
        self.last_send = Some(tick);
        let views: Vec<ZombieView> = self
            .zombies
            .iter()
            .take(MAX_ZOMBIES)
            .map(|z| ZombieView {
                swings: z.swings,
                hits: z.hits,
                hit_heading: z.hit_heading,
                hit_part: z.hit_part,
                ..ZombieView::new(z.id, z.feet(), z.chaser.yaw, z.alive)
            })
            .collect();
        self.history.record(tick, views.clone());
        Some(views)
    }

    /// Every living zombie's hit zones as clients drew them at `tick` (a
    /// shot's view tick less the zombie lag).
    pub fn targets(&self, tick: f64) -> Vec<Target> {
        self.history
            .sample_all(tick)
            .into_iter()
            .filter(|s| self.zombies.iter().any(|z| z.id == s.id && z.alive))
            .filter_map(|s| {
                Some(Target {
                    id: s.id,
                    pose: s.hitbox_pose()?,
                })
            })
            .collect()
    }

    /// Living zombies' feet as clients drew them at `tick`, for knives.
    pub fn melee_candidates(&self, tick: f64) -> Vec<(u8, Vec3)> {
        self.history
            .sample_all(tick)
            .into_iter()
            .filter(|s| s.alive && self.zombies.iter().any(|z| z.id == s.id && z.alive))
            .map(|s| (s.id, s.position))
            .collect()
    }

    /// Living zombies within `radius` of `center`, with their feet now.
    pub fn near(&self, center: Vec3, radius: f32) -> Vec<(u8, Vec3)> {
        self.zombies
            .iter()
            .filter(|z| z.alive)
            .map(|z| (z.id, z.feet()))
            .filter(|(_, feet)| (*feet + Vec3::Y).distance(center) <= radius)
            .collect()
    }

    /// Applies `damage` to zombie `id`: a hit travelling along `direction`
    /// that landed on hit shape `part`. `None` if it is not alive.
    pub fn hurt(
        &mut self,
        id: u8,
        damage: f32,
        direction: Vec3,
        part: Option<u8>,
        melee: bool,
    ) -> Option<ZombieHurt> {
        let insta_kill = self.insta_kill_left > 0.0;
        let zombie = self.zombies.iter_mut().find(|z| z.id == id && z.alive)?;
        // Insta-Kill: any hit kills.
        let damage = if insta_kill { zombie.health } else { damage };
        zombie.health -= damage;
        let killed = zombie.health <= 0.0;
        let hit = protocol::HitPart::new(part, damage, melee);
        zombie.hits = zombie.hits.wrapping_add(1);
        zombie.hit_heading = protocol::heading_to_u8(direction.x, direction.z);
        zombie.hit_part = hit.to_u8();
        let position = zombie.feet();
        if killed {
            zombie.health = 0.0;
            zombie.alive = false;
            zombie.body.set_collision_layer(0);
            zombie.body.set_collision_mask(0);
            self.rounds.on_killed();
            let kill = self.round_kills;
            self.round_kills += 1;
            if let Some(kind) = zombies::power_up_drop(self.rounds.round, kill, self.round_drops) {
                self.round_drops += 1;
                self.drop_power_up(kind, position);
            }
        } else if !melee && hit.force >= chase::STAGGER_FORCE {
            zombie.chaser.stagger();
        }
        Some(ZombieHurt { killed, position })
    }

    /// A zombie's health, if it is alive (tests).
    pub fn health(&self, id: u8) -> Option<f32> {
        self.zombies
            .iter()
            .find(|z| z.id == id && z.alive)
            .map(|z| z.health)
    }

    /// Every living zombie's id and feet (tests).
    pub fn living(&self) -> Vec<(u8, Vec3)> {
        self.zombies
            .iter()
            .filter(|z| z.alive)
            .map(|z| (z.id, z.feet()))
            .collect()
    }

    fn spawn(&mut self, round: u32, index: u32, health: f32, speed: f32, players: &[HordePlayer]) {
        let open: Vec<&SpawnPoint> = self
            .spawns
            .iter()
            .filter(|s| self.open_zones.contains(&s.zone))
            .collect();
        if open.is_empty() {
            return;
        }
        let far: Vec<&SpawnPoint> = open
            .iter()
            .copied()
            .filter(|s| {
                players
                    .iter()
                    .filter(|p| p.standing)
                    .all(|p| p.feet.distance(s.position) >= SPAWN_MIN_DISTANCE)
            })
            .collect();
        let pool = if far.is_empty() { open } else { far };
        let point = pool[rng::hash(round, index) as usize % pool.len()].clone();
        let Some(id) = self.free_id() else {
            return;
        };
        let face = players
            .iter()
            .filter(|p| p.standing)
            .map(|p| p.feet)
            .min_by(|a, b| {
                a.distance(point.position)
                    .total_cmp(&b.distance(point.position))
            })
            .map_or(0.0, |target| chase::yaw_of(target - point.position));
        let seed = rng::hash(round.wrapping_mul(31), index);
        let mut chaser = Chaser::new(speed * chase::speed_factor(seed), face);
        chaser.waypoint = point.via;
        chaser.window = point.window;
        let mut body = build_body(id);
        self.parent.add_child(&body);
        body.set_global_position(to_godot(point.position));
        self.zombies.push(Zombie {
            id,
            body,
            chaser,
            health,
            alive: true,
            dead_for: 0.0,
            fall_speed: 0.0,
            swings: 0,
            hits: 0,
            hit_heading: 0,
            hit_part: 0,
        });
    }

    /// An id no zombie in play has.
    fn free_id(&mut self) -> Option<u8> {
        for _ in 0..=u8::MAX {
            let id = self.next_id;
            self.next_id = self.next_id.wrapping_add(1);
            if !self.zombies.iter().any(|z| z.id == id) {
                return Some(id);
            }
        }
        None
    }
}

impl Drop for Horde {
    fn drop(&mut self) {
        for zombie in &mut self.zombies {
            if zombie.body.is_instance_valid() {
                zombie.body.queue_free();
            }
        }
    }
}

fn build_body(id: u8) -> Gd<CharacterBody3D> {
    let mut body = CharacterBody3D::new_alloc();
    body.set_name(&format!("Zombie{id}"));
    body.set_collision_layer(ZOMBIE_LAYER);
    body.set_collision_mask(WORLD_LAYER | ZOMBIE_LAYER | PLAYER_LAYER | BARRIER_LAYER);
    body.set_floor_snap_length(0.3);
    let mut capsule = CapsuleShape3D::new_gd();
    capsule.set_radius(CAPSULE_RADIUS);
    capsule.set_height(CAPSULE_HEIGHT);
    let mut shape = CollisionShape3D::new_alloc();
    shape.set_shape(&capsule);
    shape.set_position(Vector3::UP * (CAPSULE_HEIGHT * 0.5));
    body.add_child(&shape);
    body
}

/// The map's doors in the order the server numbers them
/// (`ZombiesEvent::Door`): `Doors`' children with a `zone`.
pub fn map_door_nodes(map: &Gd<Node>) -> Vec<Gd<Node3D>> {
    map_doors(map).into_iter().map(|d| d.node).collect()
}

fn map_doors(map: &Gd<Node>) -> Vec<MapDoor> {
    let Some(parent) = map.get_node_or_null("Doors") else {
        return Vec::new();
    };
    parent
        .get_children()
        .iter_shared()
        .filter_map(|child| child.try_cast::<Node3D>().ok())
        .filter_map(|node| {
            let zone = node.get("zone");
            if zone.is_nil() {
                return None;
            }
            let price = node.get("price").try_to::<i64>().unwrap_or(0).max(0) as u32;
            Some(MapDoor {
                zone: zone.to_string(),
                price,
                gate: to_sim(node.get_global_position()),
                node,
                open: false,
            })
        })
        .collect()
}

/// The map's boarded windows, in the order the server numbers them
/// (`ZombiesEvent::Boards`).
pub fn map_windows(map: &Gd<Node>) -> Vec<Gd<Node3D>> {
    map_spawns(map).1.into_iter().map(|w| w.node).collect()
}

/// The map's zombie spawn points and the windows they come through.
fn map_spawns(map: &Gd<Node>) -> (Vec<SpawnPoint>, Vec<MapWindow>) {
    let mut spawns = Vec::new();
    let mut windows: Vec<MapWindow> = Vec::new();
    let Some(parent) = map.get_node_or_null("Spawns") else {
        return (spawns, windows);
    };
    for child in parent.get_children().iter_shared() {
        let Ok(node) = child.try_cast::<Node3D>() else {
            continue;
        };
        let zone = node
            .get_meta_ex("zone")
            .default(&"".to_variant())
            .done()
            .to_string();
        let via = node
            .has_meta("via")
            .then(|| node.get_meta("via").try_to::<Vector3>().ok())
            .flatten()
            .map(to_sim);
        let barrier = node
            .has_meta("barrier")
            .then(|| node.get_meta("barrier").try_to::<NodePath>().ok())
            .flatten()
            .and_then(|path| node.try_get_node_as::<Node3D>(&path));
        let window = barrier.map(|barrier| {
            match windows
                .iter()
                .position(|w| w.node.instance_id() == barrier.instance_id())
            {
                Some(index) => index,
                None => {
                    windows.push(MapWindow {
                        position: to_sim(barrier.get_global_position()),
                        node: barrier,
                        boards: WINDOW_BOARDS,
                    });
                    windows.len() - 1
                }
            }
        });
        spawns.push(SpawnPoint {
            position: to_sim(node.get_global_position()),
            zone,
            via,
            window,
        });
    }
    (spawns, windows)
}
