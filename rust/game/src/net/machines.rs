//! The Zombies map's machines on the dedicated server: the power switch and
//! the drink machines (`shooter_sim::zombies::drinks`).
//!
//! The map marks them as nodes the server finds by name:
//!
//! - `PowerSwitch`: the lever (`scripts/zombies_power_switch.gd`). A press
//!   turns the power on for the rest of the game, free, as in Black Ops.
//! - `MysteryCrates`: the spots the mystery crate can stand at
//!   (`MysteryCrate`, `rust/game/src/mystery_crate.rs`); it starts at the
//!   first.
//! - `Forge`: the upgrade machine (`rust/game/src/forge.rs`), powered.
//! - `DrinkMachines`: one child per machine
//!   (`scripts/zombies_drink_machine.gd`), each with a `drink` key
//!   (`Drink::key`).
//!
//! Each is interactable: the server finds what a player looks at with the
//! use button held (`NetServer::zombies_uses`) and asks this module which
//! machine it is. The nodes show the state the server gives them
//! (`set_on`, `set_available`), on the server and on every client.

use godot::classes::Node3D;
use godot::prelude::*;
use shooter_sim::net::Tick;
use shooter_sim::net::protocol::{CratePhase, ForgePhase, TrapPhase, ZombiesEvent};
use shooter_sim::rng;
use shooter_sim::weapon::{CombatState, WeaponId};
use shooter_sim::zombies::alt_ammo::AltAmmo;
use shooter_sim::zombies::drinks::{self, Drink};
use shooter_sim::zombies::traps::{self, TrapState};
use shooter_sim::zombies::{forge, mystery};

struct DrinkMachine {
    node: Gd<Node3D>,
    drink: Drink,
    /// False once it has left (Second Wind's, used up by a lone player).
    available: bool,
}

#[derive(Default)]
pub struct Machines {
    power_switch: Option<Gd<Node3D>>,
    power: bool,
    drink_machines: Vec<DrinkMachine>,
    /// Second Winds a lone player has bought this game.
    solo_second_winds: u8,
    pub mystery: Option<MysteryCrate>,
    pub forge: Option<Forge>,
    /// The map's traps (`Traps`' children).
    pub traps: Vec<Trap>,
    /// Games played, to vary the crate's draws.
    games: u32,
}

impl Machines {
    /// The machines `map` marks (see the module docs).
    pub fn new(map: &Gd<Node>) -> Self {
        let power_switch = map.try_get_node_as::<Node3D>("PowerSwitch");
        let drink_machines = map_drink_machines(map)
            .into_iter()
            .map(|(node, drink)| DrinkMachine {
                node,
                drink,
                available: true,
            })
            .collect();
        let mystery = Some(MysteryCrate::new(map)).filter(|c| c.spot_count() > 0);
        let mut machines = Self {
            power_switch,
            drink_machines,
            mystery,
            forge: Forge::new(map),
            traps: map_traps(map)
                .into_iter()
                .map(|node| Trap {
                    node,
                    state: TrapState::Idle,
                })
                .collect(),
            ..Self::default()
        };
        machines.reset();
        machines
    }

    /// A fresh game: the power off, every machine back, and the crate at
    /// its first spot.
    pub fn reset(&mut self) {
        self.games = self.games.wrapping_add(1);
        if let Some(mystery) = &mut self.mystery {
            mystery.reset(rng::hash(self.games, 0x4352_4154));
            mystery.show();
        }
        if let Some(forge) = &mut self.forge {
            forge.reset();
            forge.show();
        }
        for trap in &mut self.traps {
            trap.state = TrapState::Idle;
            trap.show(None);
        }
        self.set_power(false);
        self.solo_second_winds = 0;
        for index in 0..self.drink_machines.len() {
            self.set_available(index, true);
        }
    }

    /// Whether the power is on. A map without a power switch is always on.
    pub fn power(&self) -> bool {
        self.power || self.power_switch.is_none()
    }

    /// Turns the power on (or off for a new game), on the lever and every
    /// machine. False if nothing changed.
    pub fn set_power(&mut self, on: bool) -> bool {
        let changed = self.power != on;
        self.power = on;
        if let Some(lever) = &mut self.power_switch {
            lever.call("set_on", &[on.to_variant()]);
        }
        let powered = self.power();
        for machine in &mut self.drink_machines {
            machine.node.call("set_powered", &[powered.to_variant()]);
        }
        if let Some(forge) = &mut self.forge {
            forge.node.call("set_powered", &[powered.to_variant()]);
        }
        for trap in &mut self.traps {
            trap.node.call("set_powered", &[powered.to_variant()]);
        }
        changed
    }

    /// The power switch's node (tests).
    pub fn power_switch_node(&self) -> Option<Gd<Node>> {
        self.power_switch.clone().map(|n| n.upcast())
    }

    /// Whether `node` is the power switch.
    pub fn is_power_switch(&self, node: &Gd<Node>) -> bool {
        self.power_switch
            .as_ref()
            .is_some_and(|lever| lever.instance_id() == node.instance_id())
    }

    /// The drink machine whose collider is `node`, if it is one.
    pub fn drink_machine_of(&self, node: &Gd<Node>) -> Option<usize> {
        self.drink_machines
            .iter()
            .position(|m| m.node.instance_id() == node.instance_id())
    }

    pub fn drink_machine_count(&self) -> usize {
        self.drink_machines.len()
    }

    /// Drink machine `index`'s node (tests).
    pub fn drink_machine_node(&self, index: usize) -> Option<Gd<Node>> {
        self.drink_machines
            .get(index)
            .map(|m| m.node.clone().upcast())
    }

    /// Drink machine `index`'s drink, and whether it is still there.
    pub fn drink_machine(&self, index: usize) -> Option<(Drink, bool)> {
        self.drink_machines
            .get(index)
            .map(|m| (m.drink, m.available))
    }

    /// The machine selling `drink`, if the map has one.
    pub fn machine_for(&self, drink: Drink) -> Option<usize> {
        self.drink_machines.iter().position(|m| m.drink == drink)
    }

    fn set_available(&mut self, index: usize, available: bool) {
        if let Some(machine) = self.drink_machines.get_mut(index) {
            machine.available = available;
            machine
                .node
                .call("set_available", &[available.to_variant()]);
        }
    }

    /// A lone player bought Second Wind from machine `index`. After the
    /// third, the machine leaves; returns true when it just did.
    pub fn sold_solo_second_wind(&mut self, index: usize) -> bool {
        self.solo_second_winds = self.solo_second_winds.saturating_add(1);
        if self.solo_second_winds >= drinks::SOLO_SECOND_WINDS {
            self.set_available(index, false);
            return true;
        }
        false
    }

    /// The power and every machine, for a player joining, a new game, or a
    /// change in whether the game is for one (`solo`).
    pub fn events(&self, solo: bool) -> Vec<ZombiesEvent> {
        let mut events = vec![ZombiesEvent::Power { on: self.power() }];
        events.extend((0..self.drink_machines.len()).filter_map(|i| self.machine_event(i, solo)));
        events
    }

    /// The trap whose switch is `node`, if it is one.
    pub fn trap_of(&self, node: &Gd<Node>) -> Option<usize> {
        self.traps
            .iter()
            .position(|t| t.node.instance_id() == node.instance_id())
    }

    /// Every trap as clients are told it.
    pub fn trap_events(&self, ticks_from_now: impl Fn(f32) -> Tick) -> Vec<ZombiesEvent> {
        self.traps
            .iter()
            .enumerate()
            .map(|(i, t)| t.event(i, &ticks_from_now))
            .collect()
    }

    /// Drink machine `index` as clients are told it.
    pub fn machine_event(&self, index: usize, solo: bool) -> Option<ZombiesEvent> {
        let machine = self.drink_machines.get(index)?;
        Some(ZombiesEvent::Machine {
            machine: index as u8,
            available: machine.available,
            price: machine.drink.price(solo),
            needs_power: !machine.drink.works_without_power(solo),
        })
    }
}

/// A trap on the map (`shooter_sim::zombies::traps`): its node (the switch
/// that buys it, at the fence's foot) and what it is doing.
pub struct Trap {
    pub node: Gd<Node3D>,
    pub state: TrapState,
}

impl Trap {
    /// Whether `point` (world) is in its live part.
    pub fn covers(&self, point: shooter_sim::Vec3) -> bool {
        let local =
            self.node.get_global_transform().affine_inverse() * crate::convert::to_godot(point);
        traps::inside(crate::convert::to_sim(local))
    }

    pub fn phase(&self) -> TrapPhase {
        match self.state {
            TrapState::Idle => TrapPhase::Idle,
            TrapState::Active { .. } => TrapPhase::Active,
            TrapState::Cooling { .. } => TrapPhase::Cooling,
        }
    }

    /// Shows its phase on the server's own node; `left` seconds of it.
    pub fn show(&mut self, left: Option<f32>) {
        let phase = self.phase().name();
        self.node.call(
            "show_phase",
            &[phase.to_variant(), left.unwrap_or(0.0).to_variant()],
        );
    }

    pub fn event(&self, index: usize, ticks_from_now: impl Fn(f32) -> Tick) -> ZombiesEvent {
        ZombiesEvent::Trap {
            trap: index as u8,
            phase: self.phase(),
            until: self.state.left().map(ticks_from_now),
        }
    }
}

/// The map's traps in the order the server numbers them
/// (`ZombiesEvent::Trap`): `Traps`' children.
pub fn map_traps(map: &Gd<Node>) -> Vec<Gd<Node3D>> {
    let Some(parent) = map.get_node_or_null("Traps") else {
        return Vec::new();
    };
    parent
        .get_children()
        .iter_shared()
        .filter_map(|child| child.try_cast::<Node3D>().ok())
        .collect()
}

/// The map's drink machines in the order the server numbers them
/// (`ZombiesEvent::Machine`): `DrinkMachines`' children with a known
/// `drink`.
pub fn map_drink_machines(map: &Gd<Node>) -> Vec<(Gd<Node3D>, Drink)> {
    let Some(parent) = map.get_node_or_null("DrinkMachines") else {
        return Vec::new();
    };
    parent
        .get_children()
        .iter_shared()
        .filter_map(|child| child.try_cast::<Node3D>().ok())
        .filter_map(|node| {
            let key = node.get("drink").to_string();
            match Drink::from_key(&key) {
                Some(drink) => Some((node, drink)),
                None => {
                    godot_error!(
                        "Zombies: drink machine `{}` sells unknown `{key}`",
                        node.get_name()
                    );
                    None
                }
            }
        })
        .collect()
}

/// What a mystery crate spot is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
enum CrateState {
    /// No crate here.
    Hidden,
    Idle,
    /// Weapons cycling for `by`; it lands on `weapon`, or the duck.
    Cycling {
        by: u8,
        weapon: Option<WeaponId>,
        left: f32,
    },
    Offering {
        by: u8,
        weapon: WeaponId,
        left: f32,
    },
    Duck {
        by: u8,
        left: f32,
    },
    /// Gone; it shows up at spot `next`.
    Away {
        next: usize,
        left: f32,
    },
}

/// What the crate did this tick that the server acts on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CrateChange {
    /// Its state changed: tell everyone.
    Changed,
    /// It landed on the duck: player `by` gets their points back.
    Refund { by: u8, points: u32 },
}

/// The mystery crate (`shooter_sim::zombies::mystery`) and the map's spots
/// for it: `MysteryCrates`' children (`MysteryCrate`), the first where a
/// game starts. During a Fire Sale every other spot has a crate of its own
/// too, until the sale ends and each has finished its spin.
pub struct MysteryCrate {
    nodes: Vec<Gd<Node3D>>,
    spots: Vec<CrateState>,
    /// The spot the crate itself is at (or leaving).
    at: usize,
    /// Spins at this spot, and moves this game.
    uses: u32,
    moves: u32,
    /// A Fire Sale is on.
    sale: bool,
    seed: u32,
}

impl MysteryCrate {
    pub fn new(map: &Gd<Node>) -> Self {
        let nodes = map_crate_spots(map);
        let mut crate_ = Self {
            spots: Vec::new(),
            nodes,
            at: 0,
            uses: 0,
            moves: 0,
            sale: false,
            seed: 0,
        };
        crate_.reset(0);
        crate_
    }

    /// A fresh game: the crate back at its first spot, closed; `seed`
    /// varies the draws from game to game.
    pub fn reset(&mut self, seed: u32) {
        self.at = 0;
        self.spots = (0..self.nodes.len())
            .map(|i| {
                if i == 0 {
                    CrateState::Idle
                } else {
                    CrateState::Hidden
                }
            })
            .collect();
        self.uses = 0;
        self.moves = 0;
        self.sale = false;
        self.seed = seed;
    }

    pub fn spot_count(&self) -> usize {
        self.nodes.len()
    }

    /// Times the crate has moved this game.
    pub fn moves(&self) -> u32 {
        self.moves
    }

    /// The spot whose collider is `node`, if it is one.
    pub fn spot_of(&self, node: &Gd<Node>) -> Option<usize> {
        self.nodes
            .iter()
            .position(|s| s.instance_id() == node.instance_id())
    }

    /// Spot `index`'s node (tests).
    pub fn spot_node(&self, index: usize) -> Option<Gd<Node>> {
        self.nodes.get(index).map(|n| n.clone().upcast())
    }

    /// The spot the crate is at (or leaving).
    pub fn at(&self) -> usize {
        self.at
    }

    /// Spot `spot`'s phase as scripts and tests name it.
    pub fn phase_name(&self, spot: usize) -> &'static str {
        match self.spots.get(spot) {
            None | Some(CrateState::Hidden) => "hidden",
            Some(CrateState::Idle) => "idle",
            Some(CrateState::Cycling { .. }) => "cycling",
            Some(CrateState::Offering { .. }) => "offering",
            Some(CrateState::Duck { .. }) => "duck",
            Some(CrateState::Away { .. }) => "away",
        }
    }

    /// What a spin costs now: 10 during a Fire Sale.
    pub fn price(&self) -> u32 {
        if self.sale {
            mystery::SALE_PRICE
        } else {
            mystery::PRICE
        }
    }

    /// Starts or ends a Fire Sale: every spot without the crate gets one of
    /// its own while it lasts (Black Ops' `toggle_fire_sale_on`). A crate on
    /// its way between spots lands at once. True if anything changed.
    pub fn set_sale(&mut self, on: bool) -> bool {
        if self.sale == on {
            return false;
        }
        self.sale = on;
        if on {
            if let CrateState::Away { next, .. } = self.spots[self.at] {
                self.land(next);
            }
            for spot in &mut self.spots {
                if *spot == CrateState::Hidden {
                    *spot = CrateState::Idle;
                }
            }
        } else {
            // Spare crates close once their spin is done.
            for (index, spot) in self.spots.iter_mut().enumerate() {
                if index != self.at && *spot == CrateState::Idle {
                    *spot = CrateState::Hidden;
                }
            }
        }
        true
    }

    /// The crate arrives at spot `next`.
    fn land(&mut self, next: usize) {
        self.spots[self.at] = CrateState::Hidden;
        self.at = next;
        self.spots[next] = CrateState::Idle;
        self.uses = 0;
        self.moves += 1;
    }

    /// Player `by`, carrying `combat` with `points`, presses use at spot
    /// `spot`: if a crate is there and closed, it opens and they pay the
    /// price (returned).
    pub fn press(&mut self, spot: usize, by: u8, combat: &CombatState, points: u32) -> Option<u32> {
        let price = self.price();
        let state = *self.spots.get(spot)?;
        if state != CrateState::Idle || points < price {
            return None;
        }
        // A Fire Sale spin never lands on the duck and does not count
        // towards a move (`_box_opened_by_fire_sale`).
        let sale = self.sale;
        let uses = if sale { self.uses } else { self.uses + 1 };
        let roll = mystery::move_roll(self.seed, uses, self.moves);
        let duck = !sale
            && spot == self.at
            && self.nodes.len() > 1
            && mystery::should_move(uses, self.moves, roll);
        let weapon = if duck {
            None
        } else {
            let seed = self
                .seed
                .wrapping_add(uses.wrapping_mul(7919))
                .wrapping_add(self.moves)
                .wrapping_add(rng::hash(spot as u32, u32::from(by)).wrapping_mul(u32::from(sale)));
            Some(mystery::draw(combat, seed)?)
        };
        self.uses = uses;
        self.spots[spot] = CrateState::Cycling {
            by,
            weapon,
            left: mystery::CYCLE_SECONDS,
        };
        Some(price)
    }

    /// The weapon spot `spot` holds out to player `by`, if it does.
    pub fn offered(&self, spot: usize, by: u8) -> Option<WeaponId> {
        match *self.spots.get(spot)? {
            CrateState::Offering {
                by: buyer, weapon, ..
            } if buyer == by => Some(weapon),
            _ => None,
        }
    }

    /// The spot holding a weapon out to player `by`, if one is.
    pub fn offering_to(&self, by: u8) -> Option<usize> {
        (0..self.spots.len()).find(|spot| self.offered(*spot, by).is_some())
    }

    /// Player `by` took `weapon` from spot `spot`: it closes.
    pub fn taken(&mut self, spot: usize, by: u8, weapon: WeaponId) {
        if self.offered(spot, by) == Some(weapon) {
            self.close(spot);
        }
    }

    /// Spot `spot`'s spin is over: closed, or gone if it was a spare crate
    /// and the sale has ended.
    fn close(&mut self, spot: usize) {
        self.spots[spot] = if spot == self.at || self.sale {
            CrateState::Idle
        } else {
            CrateState::Hidden
        };
    }

    /// Runs every spot one tick.
    pub fn step(&mut self, dt: f32, out: &mut Vec<CrateChange>) {
        for index in 0..self.spots.len() {
            self.step_spot(index, dt, out);
        }
    }

    fn step_spot(&mut self, index: usize, dt: f32, out: &mut Vec<CrateChange>) {
        let state = &mut self.spots[index];
        let done = match state {
            CrateState::Hidden | CrateState::Idle => return,
            CrateState::Cycling { left, .. }
            | CrateState::Offering { left, .. }
            | CrateState::Duck { left, .. }
            | CrateState::Away { left, .. } => {
                *left -= dt;
                if *left > 0.0 {
                    return;
                }
                *state
            }
        };
        match done {
            CrateState::Cycling {
                by,
                weapon: Some(weapon),
                ..
            } => {
                self.spots[index] = CrateState::Offering {
                    by,
                    weapon,
                    left: mystery::TAKE_SECONDS,
                };
            }
            CrateState::Cycling {
                by, weapon: None, ..
            } => {
                out.push(CrateChange::Refund {
                    by,
                    points: mystery::PRICE,
                });
                self.spots[index] = CrateState::Duck {
                    by,
                    left: mystery::DUCK_SECONDS,
                };
            }
            CrateState::Duck { .. } => {
                // Any other spot, drawn from the game's seed.
                let count = self.nodes.len().max(1);
                let others = count.saturating_sub(1).max(1);
                let pick =
                    rng::hash(self.seed, self.moves.wrapping_add(0x4d4f_5645)) as usize % others;
                let next = if pick >= self.at { pick + 1 } else { pick } % count;
                self.spots[index] = CrateState::Away {
                    next,
                    left: mystery::AWAY_SECONDS,
                };
            }
            CrateState::Away { next, .. } => self.land(next),
            CrateState::Offering { .. } => self.close(index),
            CrateState::Hidden | CrateState::Idle => {}
        }
        out.push(CrateChange::Changed);
    }

    /// Shows every spot's state on the map, so the server's colliders
    /// follow it.
    pub fn show(&self) {
        for (index, node) in self.nodes.iter().enumerate() {
            let (phase, by, weapon, left) = self.view(index);
            crate::mystery_crate::show_on_spot(
                node,
                phase,
                by,
                weapon,
                left.unwrap_or(0.0),
                None,
                self.price(),
            );
        }
    }

    fn view(&self, spot: usize) -> (CratePhase, u8, Option<WeaponId>, Option<f32>) {
        match self.spots[spot] {
            CrateState::Hidden => (CratePhase::Hidden, 0, None, None),
            CrateState::Idle => (CratePhase::Idle, 0, None, None),
            CrateState::Cycling { by, weapon, left } => {
                (CratePhase::Cycling, by, weapon, Some(left))
            }
            CrateState::Offering { by, weapon, left } => {
                (CratePhase::Offering, by, Some(weapon), Some(left))
            }
            CrateState::Duck { by, left } => (CratePhase::Duck, by, None, Some(left)),
            CrateState::Away { left, .. } => (CratePhase::Away, 0, None, Some(left)),
        }
    }

    /// Every spot as clients are told it; `ticks_from_now` turns seconds
    /// into a server tick.
    pub fn events(&self, ticks_from_now: impl Fn(f32) -> Tick) -> Vec<ZombiesEvent> {
        (0..self.nodes.len())
            .map(|spot| {
                let (phase, by, weapon, left) = self.view(spot);
                ZombiesEvent::Crate {
                    spot: spot as u8,
                    phase,
                    by,
                    weapon,
                    until: left.map(&ticks_from_now),
                    sale: self.sale,
                }
            })
            .collect()
    }
}

/// The map's mystery crate spots, in the order the server numbers them.
pub fn map_crate_spots(map: &Gd<Node>) -> Vec<Gd<Node3D>> {
    let Some(parent) = map.get_node_or_null("MysteryCrates") else {
        return Vec::new();
    };
    parent
        .get_children()
        .iter_shared()
        .filter_map(|child| child.try_cast::<Node3D>().ok())
        .collect()
}

/// What the forge is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ForgeState {
    Idle,
    Working { by: u8, weapon: WeaponId, left: f32 },
    Offering { by: u8, weapon: WeaponId, left: f32 },
}

/// The forge (`shooter_sim::zombies::forge`): the map's `Forge` node
/// (`Forge`, `rust/game/src/forge.rs`).
pub struct Forge {
    node: Gd<Node3D>,
    state: ForgeState,
    /// A re-pack: the alternate ammo the weapon comes out with.
    repack: Option<AltAmmo>,
}

impl Forge {
    pub fn new(map: &Gd<Node>) -> Option<Self> {
        Some(Self {
            node: map.try_get_node_as::<Node3D>("Forge")?,
            state: ForgeState::Idle,
            repack: None,
        })
    }

    pub fn reset(&mut self) {
        self.state = ForgeState::Idle;
    }

    pub fn is(&self, node: &Gd<Node>) -> bool {
        self.node.instance_id() == node.instance_id()
    }

    pub fn node(&self) -> Gd<Node> {
        self.node.clone().upcast()
    }

    pub fn free(&self) -> bool {
        self.state == ForgeState::Idle
    }

    pub fn phase_name(&self) -> &'static str {
        match self.state {
            ForgeState::Idle => "idle",
            ForgeState::Working { .. } => "working",
            ForgeState::Offering { .. } => "offering",
        }
    }

    /// Starts upgrading into `weapon` for player `by`, or re-packing it
    /// with alternate ammo `repack`.
    pub fn start(&mut self, by: u8, weapon: WeaponId, repack: Option<AltAmmo>) {
        self.repack = repack;
        self.state = ForgeState::Working {
            by,
            weapon,
            left: forge::WORK_SECONDS,
        };
    }

    /// The upgraded weapon held out to player `by`, if it is.
    pub fn offered(&self, by: u8) -> Option<WeaponId> {
        match self.state {
            ForgeState::Offering {
                by: owner, weapon, ..
            } if owner == by => Some(weapon),
            _ => None,
        }
    }

    /// The weapon was taken: who took what, and the alternate ammo it
    /// comes with if it was a re-pack.
    pub fn taken(&mut self) -> Option<(u8, WeaponId, Option<AltAmmo>)> {
        let ForgeState::Offering { by, weapon, .. } = self.state else {
            return None;
        };
        self.state = ForgeState::Idle;
        Some((by, weapon, self.repack.take()))
    }

    /// Runs the forge one tick; true when its state changed (left untaken,
    /// the weapon is lost, as in Black Ops).
    pub fn step(&mut self, dt: f32) -> bool {
        match &mut self.state {
            ForgeState::Idle => false,
            ForgeState::Working { by, weapon, left } => {
                *left -= dt;
                if *left > 0.0 {
                    return false;
                }
                self.state = ForgeState::Offering {
                    by: *by,
                    weapon: *weapon,
                    left: forge::TAKE_SECONDS,
                };
                true
            }
            ForgeState::Offering { left, .. } => {
                *left -= dt;
                if *left > 0.0 {
                    return false;
                }
                self.state = ForgeState::Idle;
                true
            }
        }
    }

    fn view(&self) -> (ForgePhase, u8, Option<WeaponId>, Option<f32>) {
        match self.state {
            ForgeState::Idle => (ForgePhase::Idle, 0, None, None),
            ForgeState::Working { by, weapon, left } => {
                (ForgePhase::Working, by, Some(weapon), Some(left))
            }
            ForgeState::Offering { by, weapon, left } => {
                (ForgePhase::Offering, by, Some(weapon), Some(left))
            }
        }
    }

    /// Shows the forge's state on the server's own node.
    pub fn show(&self) {
        let (phase, by, weapon, left) = self.view();
        if let Ok(mut node) = self.node.clone().try_cast::<crate::forge::Forge>() {
            node.bind_mut()
                .show(phase, false, by, weapon, left.unwrap_or(0.0));
        }
    }

    /// The forge as clients are told it.
    pub fn event(&self, ticks_from_now: impl Fn(f32) -> Tick) -> ZombiesEvent {
        let (phase, by, weapon, left) = self.view();
        ZombiesEvent::Forge {
            phase,
            by,
            weapon,
            until: left.map(ticks_from_now),
        }
    }
}
