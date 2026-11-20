//! The Zombies map's machines on the dedicated server: the power switch and
//! the drink machines (`shooter_sim::zombies::drinks`).
//!
//! The map marks them as nodes the server finds by name:
//!
//! - `PowerSwitch`: the lever (`scripts/zombies_power_switch.gd`). A press
//!   turns the power on for the rest of the game, free, as in Black Ops.
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
use shooter_sim::net::protocol::ZombiesEvent;
use shooter_sim::zombies::drinks::{self, Drink};

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
        let mut machines = Self {
            power_switch,
            drink_machines,
            ..Self::default()
        };
        machines.reset();
        machines
    }

    /// A fresh game: the power off and every machine back.
    pub fn reset(&mut self) {
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
