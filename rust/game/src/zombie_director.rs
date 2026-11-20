//! `ZombieDirector`: runs a Zombies match. It counts down to each round,
//! spawns the round's zombies at the map's spawn points, pays points for
//! hits and kills, and ends the match when the player dies. The rules are in
//! `shooter_sim::zombies`.
//!
//! It is also the match's economy: the player's points are a wallet (group
//! `zombies_wallet`) that wall weapons and doors spend from. Doors open zones:
//! spawn points with a `zone` metadata only spawn once their zone is open, and
//! zombies spawned there first walk to the spawn's `via` point (the gate).
//! Kills sometimes drop power-ups, which the player collects by walking over
//! them. Spawn points behind a window name its `barrier`; zombies spawned
//! there tear it down first, and players board it back up for points
//! ([`ZombieDirector::rebuild_reward`]).
//!
//! Online the dedicated server runs the horde (`net::horde`): the client
//! switches the map's director to [`ZombieDirector::set_networked`], and it
//! only mirrors what the server says (the round, this player's points) so
//! the HUD and the wallet keep working, while `NetClient` draws the zombies
//! as its children.

use godot::classes::{INode3D, Node3D, PackedScene};
use godot::prelude::*;
use shooter_sim::rng;
use shooter_sim::zombies::{self, Phase, PowerUp, RoundEvent, Rounds, ZombieRules};

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
    /// Dropped for a power-up; its script's `setup(kind)` and
    /// `set_time_left(seconds)` are called (`scripts/zombies_power_up.gd`).
    #[export]
    power_up_scene: Option<Gd<PackedScene>>,
    /// The player collects a power-up within this many metres.
    #[export]
    #[init(val = 1.6)]
    pickup_radius: f32,

    rules: ZombieRules,
    rounds: Option<Rounds>,
    points: i64,
    kills: i64,
    headshots: i64,
    events: Vec<RoundEvent>,
    /// Zones whose spawn points are in use ("" is the starting area).
    open_zones: Vec<String>,
    /// Gates between open zones, for zombies that walk into walls.
    gates: Vec<Vector3>,
    /// Kills and power-up drops this round.
    round_kills: u32,
    round_drops: u32,
    drops: Vec<Drop>,
    insta_kill_left: f32,
    double_points_left: f32,
    /// A Nuke is killing everything: its kills pay nothing on their own.
    nuking: bool,
    /// Points paid this round for rebuilding barriers.
    rebuild_points: i64,
    /// Online: the server runs the game, and this only mirrors it.
    networked: bool,
    /// Online: seconds (engine time) the break ends, and zombies left.
    net_break_ends: f64,
    net_remaining: u32,
    /// Online: engine seconds Insta-Kill and Double Points end.
    net_insta_kill_ends: f64,
    net_double_points_ends: f64,
    /// Online: engine seconds this player bleeds out, while down.
    net_bleeds_out: Option<f64>,
    /// Online: a revive under way that involves this player: the other
    /// player's name ("" for getting back up by ourselves), whether we are
    /// the one reviving, the engine seconds it is done, and how long it
    /// takes in all.
    net_revive: Option<(String, bool, f64, f64)>,
    /// Online: the teammate close enough to revive, if any.
    net_revive_prompt: String,
    /// Online: the power is on (drink machines and the upgrade machine).
    net_power: bool,
}

/// Points for each board put back on a barrier.
const REBUILD_POINTS: i64 = 10;
/// Most points rebuilding pays in one round.
const MAX_REBUILD_POINTS: i64 = 300;

#[derive(Clone)]
struct SpawnPoint {
    position: Vector3,
    via: Option<Vector3>,
    barrier: Option<Gd<Node3D>>,
}

struct Drop {
    kind: PowerUp,
    node: Gd<Node3D>,
    left: f32,
    /// Online: the server's id for it.
    id: Option<u8>,
}

#[godot_api]
impl INode3D for ZombieDirector {
    fn ready(&mut self) {
        self.points = self.starting_points;
        self.rounds = Some(Rounds::new(self.first_break));
        self.open_zones = vec![String::new()];
        self.base_mut().add_to_group("zombies_wallet");
    }

    fn physics_process(&mut self, delta: f64) {
        if self.networked {
            // Tokens blink in their last seconds; the server removes them.
            for drop in &mut self.drops {
                drop.left -= delta as f32;
                drop.node.call("set_time_left", &[drop.left.to_variant()]);
            }
            return;
        }
        let dt = delta as f32;
        // Before the round copy below: a Nuke's kills count toward the round.
        if self.rounds.is_some_and(|r| r.phase != Phase::Over) {
            self.update_power_ups(dt);
        }
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
                    // Two more frags a round after the first, up to four.
                    if round > 1
                        && let Some(mut player) = self.player()
                        && player.has_method("restock_lethals")
                    {
                        player.call("restock_lethals", &[]);
                    }
                    self.round_kills = 0;
                    self.round_drops = 0;
                    self.rebuild_points = 0;
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

    /// A kill dropped a power-up (`kind` is its key, such as "insta_kill").
    #[signal]
    fn power_up_dropped(kind: GString);

    /// The player picked up a power-up; `label` is what to announce.
    #[signal]
    fn power_up_collected(kind: GString, label: GString);

    /// Points were spent on something: `points` this time, `total` left.
    #[signal]
    fn points_spent(points: i64, total: i64);

    /// Takes `amount` points if the player has them. Wall weapons and doors
    /// pay through this. Online it only says whether they can: the server
    /// takes the points and says what is left.
    #[func]
    fn spend(&mut self, amount: i64) -> bool {
        if amount < 0 || self.points < amount {
            return false;
        }
        if self.networked {
            return true;
        }
        self.points -= amount;
        let total = self.points;
        self.signals().points_spent().emit(amount, total);
        true
    }

    /// Opens a zone: its spawn points join the rotation, and `gate` (where
    /// the zone meets the rest of the map) guides zombies that get stuck.
    #[func]
    fn open_zone(&mut self, zone: GString, gate: Vector3) {
        let zone = zone.to_string();
        if !self.open_zones.contains(&zone) {
            self.open_zones.push(zone);
        }
        self.gates.push(gate);
        let gates = self.gates.clone();
        for mut zombie in self.zombies().iter_shared() {
            zombie.bind_mut().set_gates(gates.clone());
        }
    }

    /// Online: stops running the game here; the server's word comes in
    /// through `net_round` and `net_points`.
    #[func]
    pub fn set_networked(&mut self) {
        self.networked = true;
        self.points = 0;
        self.kills = 0;
        self.headshots = 0;
    }

    #[func]
    pub fn is_networked(&self) -> bool {
        self.networked
    }

    /// Whether a zone is open ("" is the starting area).
    #[func]
    fn is_zone_open(&self, zone: GString) -> bool {
        self.open_zones.contains(&zone.to_string())
    }

    /// Seconds of Insta-Kill left.
    #[func]
    fn insta_kill_left(&self) -> f32 {
        if self.networked {
            return (self.net_insta_kill_ends - now_seconds()).max(0.0) as f32;
        }
        self.insta_kill_left
    }

    /// Seconds of Double Points left.
    #[func]
    fn double_points_left(&self) -> f32 {
        if self.networked {
            return (self.net_double_points_ends - now_seconds()).max(0.0) as f32;
        }
        self.double_points_left
    }

    /// Pays for a board put back on a barrier, up to a cap each round.
    /// Returns the points paid.
    #[func]
    fn rebuild_reward(&mut self) -> i64 {
        if self.rebuild_points >= MAX_REBUILD_POINTS {
            return 0;
        }
        self.rebuild_points += REBUILD_POINTS;
        let before = self.points;
        self.award(REBUILD_POINTS);
        self.points - before
    }

    /// Gives points outright (tests and debugging).
    #[func]
    fn add_points(&mut self, amount: i64) {
        self.points += amount.max(0);
    }

    /// Power-ups lying on the ground (tests).
    #[func]
    fn drops_on_ground(&self) -> i64 {
        self.drops.len() as i64
    }

    /// Drops a power-up at `position` (tests and debugging); `kind` is a key
    /// such as "max_ammo".
    #[func]
    fn drop_power_up(&mut self, kind: GString, position: Vector3) -> bool {
        let Some(kind) = PowerUp::from_key(&kind.to_string()) else {
            return false;
        };
        self.spawn_drop(kind, position);
        true
    }

    /// The zombie every spawn instances (online, the drawn ones too).
    pub fn zombie_scene(&self) -> Gd<PackedScene> {
        (*self.zombie_scene).clone()
    }

    /// Online: the server's round. `phase` 0 is a break ending in
    /// `break_left` seconds, 1 the fight with `remaining` zombies left, 2 the
    /// end. Emits the round signals on the changes, as offline.
    pub fn net_round(&mut self, round: u32, phase: u8, break_left: f64, remaining: u32) {
        let phase = match phase {
            0 => Phase::Break,
            1 => Phase::Fighting,
            _ => Phase::Over,
        };
        let before = self.rounds;
        let mut rounds = Rounds::new(0.0);
        rounds.round = round;
        rounds.phase = phase;
        self.rounds = Some(rounds);
        self.net_remaining = remaining;
        self.net_break_ends = now_seconds() + break_left.max(0.0);
        let changed = before.is_none_or(|b| b.round != round || b.phase != phase);
        if !changed {
            return;
        }
        match phase {
            Phase::Fighting => {
                self.signals().round_started().emit(i64::from(round));
            }
            Phase::Break => {
                if before.is_some_and(|b| b.phase == Phase::Fighting) {
                    let cleared = i64::from(round.saturating_sub(1));
                    self.signals().round_cleared().emit(cleared);
                }
            }
            Phase::Over => {
                self.signals().game_over().emit(i64::from(round));
            }
        }
    }

    /// Online: this player's points, which just changed by `change`.
    pub fn net_points(&mut self, total: u32, change: i32) {
        self.points = i64::from(total);
        let total = self.points;
        if change > 0 {
            self.signals()
                .points_awarded()
                .emit(i64::from(change), total);
        } else if change < 0 {
            self.signals()
                .points_spent()
                .emit(i64::from(-change), total);
        }
    }

    /// Online: a kill dropped power-up `id` of `kind` at `at`.
    pub fn net_drop(&mut self, id: u8, kind: PowerUp, at: Vector3) {
        self.spawn_drop(kind, at);
        if let Some(drop) = self.drops.last_mut() {
            drop.id = Some(id);
        }
    }

    /// Online: power-up `id` is gone; `taken` if a player picked it up,
    /// which everyone hears announced.
    pub fn net_drop_gone(&mut self, id: u8, taken: bool) {
        let Some(index) = self.drops.iter().position(|d| d.id == Some(id)) else {
            return;
        };
        let mut drop = self.drops.remove(index);
        drop.node.queue_free();
        if taken {
            self.signals().power_up_collected().emit(
                &GString::from(drop.kind.key()),
                &GString::from(drop.kind.label()),
            );
        }
    }

    /// Online: seconds of Insta-Kill and Double Points left. Held as the
    /// engine times they end, so the HUD counts them down.
    pub fn net_timers(&mut self, insta_kill: f64, double_points: f64) {
        let now = now_seconds();
        self.net_insta_kill_ends = now + insta_kill.max(0.0);
        self.net_double_points_ends = now + double_points.max(0.0);
    }

    /// Online: this player went down and bleeds out in `seconds`, or (None)
    /// is no longer down.
    pub fn net_downed(&mut self, seconds: Option<f64>) {
        self.net_bleeds_out = seconds.map(|s| now_seconds() + s.max(0.0));
        if seconds.is_none() {
            self.net_revive = None;
        }
    }

    /// Online: a revive involving this player started (done in `seconds`),
    /// with `other` reviving us or being revived by us (`reviving`), or
    /// stopped (None).
    pub fn net_revive(&mut self, revive: Option<(String, bool, f64)>) {
        self.net_revive = revive.map(|(other, reviving, seconds)| {
            let seconds = seconds.max(0.0);
            (other, reviving, now_seconds() + seconds, seconds.max(0.01))
        });
    }

    /// Online: the power went on (or off for a new game).
    pub fn net_power(&mut self, on: bool) {
        self.net_power = on;
    }

    /// Whether the power is on. Offline there is no power switch to throw,
    /// so it always is.
    #[func]
    fn power_on(&self) -> bool {
        self.net_power || !self.networked
    }

    /// Online: the teammate close enough to revive ("" for none).
    pub fn net_revive_prompt(&mut self, name: &str) {
        name.clone_into(&mut self.net_revive_prompt);
    }

    /// Last stand for the HUD: `downed` and `bleed_left` (seconds) while
    /// this player is down; `revive_left` (seconds, -1 for none) and
    /// `revive_seconds` (its whole length), `revive_name` (the other
    /// player), and `reviving` (we are the one holding) during a revive;
    /// and `prompt` (the teammate close enough to revive, or "").
    #[func]
    fn last_stand_info(&self) -> VarDictionary {
        let now = now_seconds();
        let mut d = VarDictionary::new();
        d.set("downed", self.net_bleeds_out.is_some());
        d.set(
            "bleed_left",
            self.net_bleeds_out.map_or(0.0, |end| (end - now).max(0.0)),
        );
        let (name, reviving, left, total) = match &self.net_revive {
            Some((name, reviving, end, total)) => {
                (name.clone(), *reviving, (end - now).max(0.0), *total)
            }
            None => (
                String::new(),
                false,
                -1.0,
                f64::from(shooter_sim::zombies::laststand::REVIVE_SECONDS),
            ),
        };
        d.set("revive_left", left);
        d.set("revive_seconds", total);
        d.set("revive_name", &GString::from(&name));
        d.set("reviving", reviving);
        d.set("prompt", &GString::from(&self.net_revive_prompt));
        d
    }

    /// Online: this player's kills and headshots, from the scoreboard.
    pub fn net_tallies(&mut self, kills: u32, headshots: u32) {
        self.kills = i64::from(kills);
        self.headshots = i64::from(headshots);
    }

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
        if self.networked {
            if !self.rounds.is_some_and(|r| r.phase == Phase::Break) {
                return 0.0;
            }
            return (self.net_break_ends - now_seconds()).max(0.0) as f32;
        }
        self.rounds.map(|r| r.break_left()).unwrap_or(0.0)
    }

    /// Zombies still to kill this round, spawned or not.
    #[func]
    fn remaining(&self) -> i64 {
        if self.networked {
            return i64::from(self.net_remaining);
        }
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
    pub fn zombies(&self) -> Array<Gd<Zombie>> {
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
    fn on_zombie_hurt(&mut self, killed: bool, headshot: bool, melee: bool, position: Vector3) {
        if killed {
            self.kills += 1;
            if headshot {
                self.headshots += 1;
            }
            if let Some(rounds) = &mut self.rounds {
                rounds.on_killed();
            }
            let round = self.rounds.map_or(1, |r| r.round);
            let kill = self.round_kills;
            self.round_kills += 1;
            if !self.nuking
                && let Some(kind) = zombies::power_up_drop(round, kill, self.round_drops)
            {
                self.round_drops += 1;
                self.spawn_drop(kind, position);
            }
        }
        if self.nuking {
            return;
        }
        let points = i64::from(zombies::points::for_hit(killed, headshot, melee));
        self.award(points);
    }
}

impl ZombieDirector {
    /// Pays points, doubled during Double Points.
    fn award(&mut self, points: i64) {
        let points = if self.double_points_left > 0.0 {
            points * 2
        } else {
            points
        };
        self.points += points;
        let total = self.points;
        self.signals().points_awarded().emit(points, total);
    }

    fn spawn_drop(&mut self, kind: PowerUp, position: Vector3) {
        let Some(node) = self
            .power_up_scene
            .as_ref()
            .and_then(|s| s.instantiate())
            .and_then(|n| n.try_cast::<Node3D>().ok())
        else {
            return;
        };
        let mut node = node;
        self.base_mut().add_child(&node);
        node.set_global_position(position + Vector3::UP * 0.9);
        node.call("setup", &[GString::from(kind.key()).to_variant()]);
        self.drops.push(Drop {
            kind,
            node,
            left: zombies::POWER_UP_LIFETIME,
            id: None,
        });
        self.signals()
            .power_up_dropped()
            .emit(&GString::from(kind.key()));
    }

    /// Ages the drops and the timed power-ups, and collects drops the player
    /// walks over.
    fn update_power_ups(&mut self, dt: f32) {
        let insta_before = self.insta_kill_left > 0.0;
        self.insta_kill_left = (self.insta_kill_left - dt).max(0.0);
        self.double_points_left = (self.double_points_left - dt).max(0.0);
        let player = self.player().map(|p| p.get_global_position());
        let mut collected = Vec::new();
        let radius = self.pickup_radius;
        self.drops.retain_mut(|drop| {
            drop.left -= dt;
            let near = player.is_some_and(|p| {
                let at = drop.node.get_global_position();
                Vector2::new(at.x - p.x, at.z - p.z).length() < radius && (at.y - p.y).abs() < 2.5
            });
            if near {
                collected.push(drop.kind);
            } else {
                drop.node.call("set_time_left", &[drop.left.to_variant()]);
            }
            let keep = !near && drop.left > 0.0;
            if !keep {
                drop.node.queue_free();
            }
            keep
        });
        for kind in collected {
            self.collect(kind);
        }
        let insta_now = self.insta_kill_left > 0.0;
        if insta_now != insta_before {
            for mut zombie in self.zombies().iter_shared() {
                zombie.bind_mut().set_insta_kill(insta_now);
            }
        }
    }

    fn collect(&mut self, kind: PowerUp) {
        match kind {
            PowerUp::MaxAmmo => {
                if let Some(mut player) = self.player() {
                    player.call("refill_all_ammo", &[]);
                }
            }
            PowerUp::InstaKill => self.insta_kill_left = zombies::POWER_UP_SECONDS,
            PowerUp::DoublePoints => self.double_points_left = zombies::POWER_UP_SECONDS,
            PowerUp::Nuke => {
                let zombies = self.zombies();
                self.nuking = true;
                {
                    // Each kill's signal calls back into this director
                    // (`on_zombie_hurt`); the guard allows that.
                    let _guard = self.base_mut();
                    for mut zombie in zombies.iter_shared() {
                        let health = zombie.bind().health();
                        if health > 0.0 {
                            zombie.bind_mut().apply_damage(health + 1.0, false);
                        }
                    }
                }
                self.nuking = false;
                self.award(i64::from(zombies::NUKE_POINTS));
            }
        }
        self.signals()
            .power_up_collected()
            .emit(&GString::from(kind.key()), &GString::from(kind.label()));
    }

    fn player(&self) -> Option<Gd<Node3D>> {
        self.base().try_get_node_as::<Node3D>(&self.player_path)
    }

    /// Spawn points in open zones, each with its gate or window (`via`) and
    /// barrier if it has them.
    fn spawn_points(&self) -> Vec<SpawnPoint> {
        let Some(parent) = self.base().try_get_node_as::<Node>(&self.spawns_path) else {
            return Vec::new();
        };
        parent
            .get_children()
            .iter_shared()
            .filter_map(|c| c.try_cast::<Node3D>().ok())
            .filter(|n| {
                let zone = n
                    .get_meta_ex("zone")
                    .default(&"".to_variant())
                    .done()
                    .to_string();
                self.open_zones.contains(&zone)
            })
            .map(|n| {
                let via = n
                    .has_meta("via")
                    .then(|| n.get_meta("via").try_to::<Vector3>().ok())
                    .flatten();
                let barrier = n
                    .has_meta("barrier")
                    .then(|| n.get_meta("barrier").try_to::<NodePath>().ok())
                    .flatten()
                    .and_then(|path| n.try_get_node_as::<Node3D>(&path));
                SpawnPoint {
                    position: n.get_global_position(),
                    via,
                    barrier,
                }
            })
            .collect()
    }

    /// A spawn point away from the player, picked by a hash of the round and
    /// zombie, so the same match spawns the same way.
    fn pick_spawn(&self, round: u32, index: u32) -> Option<SpawnPoint> {
        let points = self.spawn_points();
        if points.is_empty() {
            return None;
        }
        let player = self.player().map(|p| p.get_global_position());
        let far: Vec<SpawnPoint> = points
            .iter()
            .filter(|s| {
                player.is_none_or(|pl| s.position.distance_to(pl) >= self.spawn_min_distance)
            })
            .cloned()
            .collect();
        let mut pool = if far.is_empty() { points } else { far };
        let pick = rng::hash(round, index) as usize % pool.len();
        Some(pool.swap_remove(pick))
    }

    fn spawn(&mut self, round: u32, index: u32, health: f32, speed: f32) {
        let Some(SpawnPoint {
            position,
            via,
            barrier,
        }) = self.pick_spawn(round, index)
        else {
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
        {
            let mut z = zombie.bind_mut();
            z.configure(health, speed);
            z.set_gates(self.gates.clone());
            z.set_insta_kill(self.insta_kill_left > 0.0);
            if let Some(via) = via {
                z.set_waypoint(via);
            }
            if let Some(barrier) = barrier {
                z.set_barrier(barrier);
            }
        }
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

/// Engine time in seconds.
fn now_seconds() -> f64 {
    godot::classes::Time::singleton().get_ticks_usec() as f64 / 1e6
}
