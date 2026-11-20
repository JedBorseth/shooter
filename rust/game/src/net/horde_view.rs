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

use super::horde;
use crate::convert::to_godot;
use crate::zombie::Zombie;
use crate::zombie_director::ZombieDirector;

/// Seconds after the local player's own hit during which the server's
/// report of a hit on that zombie does not flinch it again.
const PREDICTED_FLINCH_SECONDS: f32 = 0.4;

struct Puppet {
    id: u8,
    node: Gd<Zombie>,
    swings: u8,
    hits: u8,
}

pub struct HordeView {
    director: Gd<ZombieDirector>,
    scene: Gd<PackedScene>,
    windows: Vec<Gd<Node3D>>,
    history: HordeHistory,
    puppets: Vec<Puppet>,
    lag: f64,
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
        let windows = director
            .get_parent()
            .map(|map| horde::map_windows(&map))
            .unwrap_or_default();
        Some(Self {
            director,
            scene,
            windows,
            history: HordeHistory::default(),
            puppets: Vec::new(),
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
            let keep = puppet.node.is_instance_valid() && zombies.iter().any(|z| z.id == puppet.id);
            // A dead one lies on in the corpse yard and frees itself.
            if !keep && puppet.node.is_instance_valid() && puppet.node.bind().is_alive() {
                puppet.node.queue_free();
            }
            keep
        });
        for view in &zombies {
            match self.puppets.iter_mut().find(|p| p.id == view.id) {
                Some(puppet) => {
                    if view.swings != puppet.swings {
                        puppet.swings = view.swings;
                        puppet.node.bind_mut().net_swing();
                    }
                    let hit = view.hits != puppet.hits;
                    puppet.hits = view.hits;
                    let (x, z) = protocol::hit_direction(view.hit_heading);
                    let part = protocol::HitPart::from_u8(view.hit_part);
                    let direction = Vector3::new(x, 0.0, z);
                    let part_index = part.part.map_or(-1, i64::from);
                    let force = f32::from(part.force);
                    if !view.alive {
                        puppet.node.bind_mut().net_die(direction, part_index, force);
                    } else if hit
                        && !puppet
                            .node
                            .bind_mut()
                            .predicted_flinch(PREDICTED_FLINCH_SECONDS)
                    {
                        let at = puppet.node.get_global_position() + Vector3::UP * 1.2;
                        puppet
                            .node
                            .bind_mut()
                            .flinch(direction, false, at, part_index, force);
                    }
                }
                None if view.alive => self.spawn(view),
                None => {}
            }
        }
        self.history.record(tick, zombies);
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
            node: zombie,
            swings: view.swings,
            hits: view.hits,
        });
    }

    /// Places every zombie at `render_tick` (the other players' drawn tick)
    /// less the zombie lag.
    pub fn draw(&mut self, render_tick: f64) {
        let tick = render_tick - self.lag;
        for puppet in &mut self.puppets {
            if !puppet.node.is_instance_valid() || !puppet.node.bind().is_alive() {
                continue;
            }
            let Some(sample) = self.history.sample(puppet.id, tick) else {
                continue;
            };
            let walking = sample.walking();
            puppet.node.bind_mut().net_apply(
                to_godot(sample.position),
                sample.yaw,
                to_godot(sample.velocity),
                walking,
            );
        }
    }

    /// A Zombies event: the round, points, and boards go to the director
    /// and the windows. `seconds_until` turns a server tick into seconds
    /// from now.
    pub fn on_event(&mut self, event: &ZombiesEvent, seconds_until: impl Fn(Tick) -> f64) {
        match *event {
            ZombiesEvent::Round {
                round,
                phase,
                starts_at,
                remaining,
            } => {
                let phase = match phase {
                    ZombiesPhase::Break => 0,
                    ZombiesPhase::Fighting => 1,
                    ZombiesPhase::Over => 2,
                };
                let left = starts_at.map_or(0.0, seconds_until);
                self.director
                    .bind_mut()
                    .net_round(round, phase, left, remaining);
            }
            ZombiesEvent::Points { total, change } => {
                self.director.bind_mut().net_points(total, change);
            }
            ZombiesEvent::Boards { window, boards } => {
                if let Some(node) = self.windows.get_mut(usize::from(window)) {
                    node.call("show_boards", &[i64::from(boards).to_variant()]);
                }
            }
            ZombiesEvent::Hit { .. } => {}
        }
    }

    /// This player's kills and headshots, from the scoreboard.
    pub fn set_tallies(&mut self, kills: u32, headshots: u32) {
        self.director.bind_mut().net_tallies(kills, headshots);
    }
}
