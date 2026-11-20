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
use shooter_sim::net::protocol::{CratePhase, ForgePhase, ZombiesEvent};
use shooter_sim::rng;
use shooter_sim::weapon::{CombatState, WeaponId};
use shooter_sim::zombies::drinks::{self, Drink};
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

/// What the mystery crate is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
enum CrateState {
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
/// game starts.
pub struct MysteryCrate {
    spots: Vec<Gd<Node3D>>,
    at: usize,
    state: CrateState,
    /// Spins at this spot, and moves this game.
    uses: u32,
    moves: u32,
    seed: u32,
}

impl MysteryCrate {
    pub fn new(map: &Gd<Node>) -> Self {
        Self {
            spots: map_crate_spots(map),
            at: 0,
            state: CrateState::Idle,
            uses: 0,
            moves: 0,
            seed: 0,
        }
    }

    /// A fresh game: the crate back at its first spot, closed; `seed`
    /// varies the draws from game to game.
    pub fn reset(&mut self, seed: u32) {
        self.at = 0;
        self.state = CrateState::Idle;
        self.uses = 0;
        self.moves = 0;
        self.seed = seed;
    }

    pub fn spot_count(&self) -> usize {
        self.spots.len()
    }

    /// The spot whose collider is `node`, if it is one.
    pub fn spot_of(&self, node: &Gd<Node>) -> Option<usize> {
        self.spots
            .iter()
            .position(|s| s.instance_id() == node.instance_id())
    }

    /// Spot `index`'s node (tests).
    pub fn spot_node(&self, index: usize) -> Option<Gd<Node>> {
        self.spots.get(index).map(|n| n.clone().upcast())
    }

    /// The spot the crate is at (or leaving).
    pub fn at(&self) -> usize {
        self.at
    }

    /// The crate's phase as scripts and tests name it.
    pub fn phase_name(&self) -> &'static str {
        match self.state {
            CrateState::Idle => "idle",
            CrateState::Cycling { .. } => "cycling",
            CrateState::Offering { .. } => "offering",
            CrateState::Duck { .. } => "duck",
            CrateState::Away { .. } => "away",
        }
    }

    /// Player `by`, carrying `combat` with `points`, presses use at spot
    /// `spot`: if the crate is there and closed, it opens and they pay the
    /// price (returned).
    pub fn press(&mut self, spot: usize, by: u8, combat: &CombatState, points: u32) -> Option<u32> {
        if spot != self.at || self.state != CrateState::Idle || points < mystery::PRICE {
            return None;
        }
        let uses = self.uses + 1;
        let roll = mystery::move_roll(self.seed, uses, self.moves);
        let duck = self.spots.len() > 1 && mystery::should_move(uses, self.moves, roll);
        let weapon = if duck {
            None
        } else {
            let seed = self
                .seed
                .wrapping_add(uses.wrapping_mul(7919))
                .wrapping_add(self.moves);
            Some(mystery::draw(combat, seed)?)
        };
        self.uses = uses;
        self.state = CrateState::Cycling {
            by,
            weapon,
            left: mystery::CYCLE_SECONDS,
        };
        Some(mystery::PRICE)
    }

    /// The weapon spot `spot` holds out to player `by`, if it does.
    pub fn offered(&self, spot: usize, by: u8) -> Option<WeaponId> {
        match self.state {
            CrateState::Offering {
                by: buyer, weapon, ..
            } if spot == self.at && buyer == by => Some(weapon),
            _ => None,
        }
    }

    /// The buyer took the weapon: the crate closes.
    pub fn taken(&mut self) {
        if matches!(self.state, CrateState::Offering { .. }) {
            self.state = CrateState::Idle;
        }
    }

    /// Runs the crate one tick.
    pub fn step(&mut self, dt: f32, out: &mut Vec<CrateChange>) {
        let next = match &mut self.state {
            CrateState::Idle => None,
            CrateState::Cycling { left, .. }
            | CrateState::Offering { left, .. }
            | CrateState::Duck { left, .. }
            | CrateState::Away { left, .. } => {
                *left -= dt;
                (*left <= 0.0).then_some(self.state)
            }
        };
        let Some(done) = next else {
            return;
        };
        self.state = match done {
            CrateState::Cycling {
                by,
                weapon: Some(weapon),
                ..
            } => CrateState::Offering {
                by,
                weapon,
                left: mystery::TAKE_SECONDS,
            },
            CrateState::Cycling {
                by, weapon: None, ..
            } => {
                out.push(CrateChange::Refund {
                    by,
                    points: mystery::PRICE,
                });
                CrateState::Duck {
                    by,
                    left: mystery::DUCK_SECONDS,
                }
            }
            CrateState::Duck { .. } => {
                // Any other spot, drawn from the game's seed.
                let others = self.spots.len().saturating_sub(1).max(1);
                let pick =
                    rng::hash(self.seed, self.moves.wrapping_add(0x4d4f_5645)) as usize % others;
                let next = if pick >= self.at { pick + 1 } else { pick } % self.spots.len().max(1);
                CrateState::Away {
                    next,
                    left: mystery::AWAY_SECONDS,
                }
            }
            CrateState::Away { next, .. } => {
                self.at = next;
                self.uses = 0;
                self.moves += 1;
                CrateState::Idle
            }
            CrateState::Offering { .. } | CrateState::Idle => CrateState::Idle,
        };
        out.push(CrateChange::Changed);
    }

    /// Shows the crate's state on the map's spots, so the server's
    /// colliders follow it.
    pub fn show(&self) {
        let (phase, by, weapon, left) = self.view();
        crate::mystery_crate::show_on_spots(
            &self.spots,
            self.at as u8,
            phase,
            by,
            weapon,
            left.unwrap_or(0.0),
            None,
        );
    }

    fn view(&self) -> (CratePhase, u8, Option<WeaponId>, Option<f32>) {
        match self.state {
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

    /// The crate as clients are told it; `ticks_from_now` turns seconds into
    /// a server tick.
    pub fn event(&self, ticks_from_now: impl Fn(f32) -> Tick) -> Option<ZombiesEvent> {
        if self.spots.is_empty() {
            return None;
        }
        let (phase, by, weapon, left) = self.view();
        Some(ZombiesEvent::Crate {
            spot: self.at as u8,
            phase,
            by,
            weapon,
            until: left.map(ticks_from_now),
        })
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
}

impl Forge {
    pub fn new(map: &Gd<Node>) -> Option<Self> {
        Some(Self {
            node: map.try_get_node_as::<Node3D>("Forge")?,
            state: ForgeState::Idle,
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

    /// Starts upgrading into `weapon` for player `by`.
    pub fn start(&mut self, by: u8, weapon: WeaponId) {
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

    pub fn taken(&mut self) {
        if matches!(self.state, ForgeState::Offering { .. }) {
            self.state = ForgeState::Idle;
        }
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
