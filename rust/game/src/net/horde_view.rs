//! The Zombies horde as this client draws it: one `Zombie` (in puppet mode)
//! for every zombie the server sends, placed from the snapshots
//! ([`HordeHistory`], the same sampling the server's lag compensation uses),
//! with its swings, flinches, and death played as the counters in each
//! [`ZombieView`] change. Also mirrors the round, this player's points, and
//! the windows' boards onto the map's `ZombieDirector` and barriers, so the
//! Zombies HUD and the map work as they do offline.
//!
//! Zombies are drawn [`protocol::zombie_lag`] behind other players, since
//! they are sent half as often; the server rewinds them by the same amount.

use godot::prelude::*;
use shooter_sim::net::Tick;
use shooter_sim::net::horde::HordeHistory;
use shooter_sim::net::protocol::{self, ZombieView, ZombiesEvent, ZombiesPhase};
use shooter_sim::zombies::mystery;

use super::{horde, machines};
use crate::convert::to_godot;
use crate::hound::Hound;
use crate::zombie::Zombie;

/// The hound clients draw (`tools/build_hound.py`).
const HOUND_SCENE: &str = "res://scenes/zombies/hound.tscn";
/// The strike that brings a hound in (`scripts/zombies_hound_strike.gd`).
const STRIKE_SCRIPT: &str = "res://scripts/zombies_hound_strike.gd";
use crate::zombie_director::ZombieDirector;

/// Seconds after the local player's own hit during which the server's
/// report of a hit on that zombie does not flinch it again.
const PREDICTED_FLINCH_SECONDS: f32 = 0.4;

struct Puppet {
    id: u8,
    node: PuppetNode,
    swings: u8,
    hits: u8,
}

/// What a puppet draws: a zombie or a hound.
enum PuppetNode {
    Zombie(Gd<Zombie>),
    Hound(Gd<Hound>),
}

impl PuppetNode {
    fn valid(&self) -> bool {
        match self {
            PuppetNode::Zombie(z) => z.is_instance_valid(),
            PuppetNode::Hound(h) => h.is_instance_valid(),
        }
    }

    fn alive(&self) -> bool {
        match self {
            PuppetNode::Zombie(z) => z.bind().is_alive(),
            PuppetNode::Hound(h) => h.bind().is_alive(),
        }
    }

    fn free(&mut self) {
        match self {
            PuppetNode::Zombie(z) => z.queue_free(),
            PuppetNode::Hound(h) => h.queue_free(),
        }
    }

    fn position(&self) -> Vector3 {
        match self {
            PuppetNode::Zombie(z) => z.get_global_position(),
            PuppetNode::Hound(h) => h.get_global_position(),
        }
    }
}

pub struct HordeView {
    director: Gd<ZombieDirector>,
    scene: Gd<PackedScene>,
    hound_scene: Option<Gd<PackedScene>>,
    windows: Vec<Gd<Node3D>>,
    doors: Vec<Gd<Node3D>>,
    /// The power switch and the drink machines, in the server's order.
    power_switch: Option<Gd<Node3D>>,
    drink_machines: Vec<Gd<Node3D>>,
    crate_spots: Vec<Gd<Node3D>>,
    forge: Option<Gd<crate::forge::Forge>>,
    history: HordeHistory,
    puppets: Vec<Puppet>,
    lag: f64,
    /// Players down in last stand.
    downed: Vec<u8>,
}

impl HordeView {
    /// The view for the current map, if it is a Zombies map (it has a
    /// `ZombieDirector`). The director stops running a game of its own.
    pub fn attach(tree: &Gd<SceneTree>, snapshot_interval: u8) -> Option<Self> {
        let mut director = tree
            .get_first_node_in_group("zombies_wallet")?
            .try_cast::<ZombieDirector>()
            .ok()?;
        director.bind_mut().set_networked();
        // The drawn zombies replace the director's own.
        for zombie in director.bind().zombies().iter_shared() {
            let mut zombie = zombie;
            zombie.queue_free();
        }
        let scene = director.bind().zombie_scene();
        let map = director.get_parent();
        let windows = map.as_ref().map(horde::map_windows).unwrap_or_default();
        let doors = map.as_ref().map(horde::map_door_nodes).unwrap_or_default();
        let power_switch = map
            .as_ref()
            .and_then(|m| m.try_get_node_as::<Node3D>("PowerSwitch"));
        let drink_machines = map
            .as_ref()
            .map(machines::map_drink_machines)
            .unwrap_or_default()
            .into_iter()
            .map(|(node, _)| node)
            .collect();
        let crate_spots = map
            .as_ref()
            .map(machines::map_crate_spots)
            .unwrap_or_default();
        let forge = map
            .as_ref()
            .and_then(|m| m.try_get_node_as::<crate::forge::Forge>("Forge"));
        Some(Self {
            forge,
            director,
            scene,
            hound_scene: godot::tools::try_load::<PackedScene>(HOUND_SCENE).ok(),
            windows,
            doors,
            power_switch,
            drink_machines,
            crate_spots,
            history: HordeHistory::default(),
            puppets: Vec::new(),
            downed: Vec::new(),
            lag: protocol::zombie_lag(snapshot_interval),
        })
    }

    /// Zombies drawn now (tests).
    pub fn drawn(&self) -> usize {
        self.puppets.len()
    }

    /// A snapshot carried the horde: zombies new to it appear, gone ones
    /// leave, and swings, hits, and deaths play.
    pub fn on_snapshot(&mut self, tick: Tick, zombies: Vec<ZombieView>) {
        self.puppets.retain_mut(|puppet| {
            let keep = puppet.node.valid() && zombies.iter().any(|z| z.id == puppet.id);
            // A dead one lies on in the corpse yard (or crumbles) and frees
            // itself.
            if !keep && puppet.node.valid() && puppet.node.alive() {
                puppet.node.free();
            }
            keep
        });
        for view in &zombies {
            match self.puppets.iter_mut().find(|p| p.id == view.id) {
                Some(puppet) => {
                    let swung = view.swings != puppet.swings;
                    puppet.swings = view.swings;
                    let hit = view.hits != puppet.hits;
                    puppet.hits = view.hits;
                    let (x, z) = protocol::hit_direction(view.hit_heading);
                    let part = protocol::HitPart::from_u8(view.hit_part);
                    let direction = Vector3::new(x, 0.0, z);
                    let part_index = part.part.map_or(-1, i64::from);
                    let force = f32::from(part.force);
                    let at = puppet.node.position() + Vector3::UP * 1.2;
                    match &mut puppet.node {
                        PuppetNode::Zombie(node) => {
                            let mut node = node.bind_mut();
                            if swung {
                                node.net_swing();
                            }
                            if !view.alive {
                                node.net_die(direction, part_index, force);
                            } else if hit && !node.predicted_flinch(PREDICTED_FLINCH_SECONDS) {
                                node.flinch(direction, false, at, part_index, force);
                            }
                        }
                        PuppetNode::Hound(node) => {
                            let mut node = node.bind_mut();
                            if swung {
                                node.net_swing();
                            }
                            if !view.alive {
                                node.net_die();
                            } else if hit && !node.predicted_flinch(PREDICTED_FLINCH_SECONDS) {
                                node.flinch_along(direction);
                            }
                        }
                    }
                }
                None if view.alive && view.hound => self.spawn_hound(view),
                None if view.alive => self.spawn(view),
                None => {}
            }
        }
        self.history.record(tick, zombies);
    }

    fn spawn_hound(&mut self, view: &ZombieView) {
        let Some(node) = self.hound_scene.as_ref().and_then(|s| s.instantiate()) else {
            return;
        };
        let Ok(mut hound) = node.try_cast::<Hound>() else {
            godot_error!("net client: {HOUND_SCENE} is not a Hound");
            return;
        };
        hound.set_name(&format!("NetHound{}", view.id));
        self.director.add_child(&hound);
        hound
            .bind_mut()
            .net_apply(to_godot(view.position()), view.yaw(), Vector3::ZERO);
        self.puppets.push(Puppet {
            id: view.id,
            node: PuppetNode::Hound(hound),
            swings: view.swings,
            hits: view.hits,
        });
    }

    /// A strike lands at `at`: a hound is on its way.
    fn strike(&mut self, at: Vector3) {
        let Ok(script) = godot::tools::try_load::<godot::classes::Script>(STRIKE_SCRIPT) else {
            return;
        };
        let mut node = Node3D::new_alloc();
        node.set_script(&script);
        self.director.add_child(&node);
        node.set_global_position(at);
    }

    fn spawn(&mut self, view: &ZombieView) {
        let Some(node) = self.scene.instantiate() else {
            return;
        };
        let Ok(mut zombie) = node.try_cast::<Zombie>() else {
            godot_error!("net client: the director's zombie scene is not a Zombie");
            return;
        };
        zombie.set_name(&format!("NetZombie{}", view.id));
        zombie.bind_mut().set_puppet();
        zombie.set_rotation(Vector3::new(0.0, view.yaw(), 0.0));
        self.director.add_child(&zombie);
        zombie.set_global_position(to_godot(view.position()));
        self.puppets.push(Puppet {
            id: view.id,
            node: PuppetNode::Zombie(zombie),
            swings: view.swings,
            hits: view.hits,
        });
    }

    /// Places every zombie at `render_tick` (the other players' drawn tick)
    /// less the zombie lag.
    pub fn draw(&mut self, render_tick: f64) {
        let tick = render_tick - self.lag;
        for puppet in &mut self.puppets {
            if !puppet.node.valid() || !puppet.node.alive() {
                continue;
            }
            let Some(sample) = self.history.sample(puppet.id, tick) else {
                continue;
            };
            let walking = sample.walking();
            let (feet, velocity) = (to_godot(sample.position), to_godot(sample.velocity));
            match &mut puppet.node {
                PuppetNode::Zombie(node) => node
                    .bind_mut()
                    .net_apply(feet, sample.yaw, velocity, walking),
                PuppetNode::Hound(node) => node.bind_mut().net_apply(feet, sample.yaw, velocity),
            }
        }
    }

    /// A Zombies event: the round, points, and boards go to the director
    /// and the windows. `seconds_until` turns a server tick into seconds
    /// from now.
    /// `me` is the local player.
    pub fn on_event(
        &mut self,
        event: &ZombiesEvent,
        me: Option<u8>,
        seconds_until: impl Fn(Tick) -> f64,
    ) {
        match *event {
            ZombiesEvent::Round {
                round,
                phase,
                starts_at,
                remaining,
                hounds,
            } => {
                let phase = match phase {
                    ZombiesPhase::Break => 0,
                    ZombiesPhase::Fighting => 1,
                    ZombiesPhase::Over => 2,
                };
                let left = starts_at.map_or(0.0, seconds_until);
                self.director
                    .bind_mut()
                    .net_round(round, phase, left, remaining, hounds);
            }
            ZombiesEvent::Points { total, change } => {
                self.director.bind_mut().net_points(total, change);
            }
            ZombiesEvent::Boards { window, boards } => {
                if let Some(node) = self.windows.get_mut(usize::from(window)) {
                    node.call("show_boards", &[i64::from(boards).to_variant()]);
                }
            }
            ZombiesEvent::Door { door, open } => {
                if let Some(node) = self.doors.get_mut(usize::from(door)) {
                    node.call("set_open", &[open.to_variant(), true.to_variant()]);
                }
            }
            ZombiesEvent::PowerUpDropped { id, kind, at } => {
                self.director.bind_mut().net_drop(id, kind, to_godot(at));
            }
            ZombiesEvent::PowerUpGone { id, by } => {
                self.director.bind_mut().net_drop_gone(id, by.is_some());
            }
            ZombiesEvent::PowerUpTimers {
                insta_kill_until,
                double_points_until,
                fire_sale_until,
            } => {
                let insta = insta_kill_until.map_or(0.0, &seconds_until);
                let double = double_points_until.map_or(0.0, &seconds_until);
                let sale = fire_sale_until.map_or(0.0, &seconds_until);
                self.director.bind_mut().net_timers(insta, double, sale);
            }
            ZombiesEvent::Downed {
                player,
                bleeds_out_at,
            } => {
                let downed = bleeds_out_at.is_some();
                self.downed.retain(|id| *id != player);
                if downed {
                    self.downed.push(player);
                }
            }
            ZombiesEvent::Power { on } => {
                self.director.bind_mut().net_power(on);
                if let Some(lever) = &mut self.power_switch {
                    lever.call("set_on", &[on.to_variant()]);
                }
                for machine in &mut self.drink_machines {
                    machine.call("set_powered", &[on.to_variant()]);
                }
                if let Some(forge) = &mut self.forge {
                    forge.bind_mut().set_powered(on);
                }
            }
            ZombiesEvent::Machine {
                machine,
                available,
                price,
                needs_power,
            } => {
                if let Some(node) = self.drink_machines.get_mut(usize::from(machine)) {
                    node.call("set_available", &[available.to_variant()]);
                    node.call(
                        "set_terms",
                        &[i64::from(price).to_variant(), needs_power.to_variant()],
                    );
                }
            }
            ZombiesEvent::Drank { player, drink } => {
                // The machine that sold it answers (its light and sound).
                for machine in &mut self.drink_machines {
                    if machine.get("drink").to_string() == drink.key() {
                        machine.call("sold", &[i64::from(player).to_variant()]);
                    }
                }
            }
            ZombiesEvent::Crate {
                spot,
                phase,
                by,
                weapon,
                until,
                sale,
            } => {
                let seconds = until.map_or(0.0, &seconds_until) as f32;
                let price = if sale {
                    mystery::SALE_PRICE
                } else {
                    mystery::PRICE
                };
                if let Some(node) = self.crate_spots.get(usize::from(spot)) {
                    crate::mystery_crate::show_on_spot(node, phase, by, weapon, seconds, me, price);
                }
            }
            ZombiesEvent::Forge {
                phase,
                by,
                weapon,
                until,
            } => {
                let seconds = until.map_or(0.0, &seconds_until) as f32;
                if let Some(forge) = &mut self.forge {
                    forge
                        .bind_mut()
                        .show(phase, me == Some(by), by, weapon, seconds);
                }
            }
            ZombiesEvent::HoundStrike { at } => self.strike(to_godot(at)),
            // Last stand on the HUD is `NetClient`'s, which knows names.
            ZombiesEvent::Hit { .. } | ZombiesEvent::Revive { .. } => {}
        }
    }

    /// Players down in last stand.
    pub fn downed(&self) -> &[u8] {
        &self.downed
    }

    pub fn director(&mut self) -> &mut Gd<ZombieDirector> {
        &mut self.director
    }

    /// This player's kills and headshots, from the scoreboard.
    pub fn set_tallies(&mut self, kills: u32, headshots: u32) {
        self.director.bind_mut().net_tallies(kills, headshots);
    }
}
