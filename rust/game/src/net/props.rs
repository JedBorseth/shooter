//! Bridge to a map's interactive props manager (GDScript).
//!
//! Maps with damageable props (Nuketown) add a `Props` node that owns each
//! prop's health, stages, debris, explosions, and population scaling. That
//! manager exposes a small dictionary-based API; this module calls it through
//! Godot's dynamic bindings so the Rust networking code needs no typed class.
//!
//! The manager is found by the `world_props` group (one per map), or as a
//! `Props` child of the map scene. Every call is defensive: maps without
//! props, or a node that happens to be called `Props` but does not implement
//! the manager API (Slamcity's geometry `Props` combiner), are ignored.
//!
//! Authority stays on the server. Clients never send prop damage: the server
//! is the only caller of `damage_prop`, and clients only call `apply_state`
//! with states the server broadcast.

use godot::classes::Node;
use godot::prelude::*;
use shooter_sim::Vec3;
use shooter_sim::weapon::{self, WeaponId};

use crate::convert::{to_godot, to_sim};

/// Group a map's props manager joins, so networking can find it.
pub const GROUP: &str = "world_props";

/// The manager methods this bridge relies on. A node that implements all of
/// them is a props manager; anything else (such as Slamcity's `Props` CSG
/// combiner) is not.
const REQUIRED_METHODS: [&str; 7] = [
    "damage_prop",
    "drain_events",
    "snapshot_states",
    "apply_state",
    "reset_props",
    "drain_explosions",
    "set_population",
];

/// One prop's networked state, as the manager reports it or accepts it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropSnapshot {
    pub id: u16,
    pub stage: u8,
    pub impulse: Vec3,
}

/// One explosion the manager wants the server to apply to players.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropExplosion {
    pub position: Vec3,
    pub radius: f32,
    pub damage: f32,
    pub outer_damage: f32,
    /// The player credited with the explosion, if any.
    pub attacker: Option<u8>,
    /// The weapon that caused it, if it maps to a known weapon.
    pub weapon: Option<WeaponId>,
}

/// A handle to the map's props manager, if the map has one.
///
/// Every use checks that the node is still a live instance, so a freed map
/// cannot leave a dangling call behind.
#[derive(Default)]
pub struct PropsBridge {
    node: Option<Gd<Node>>,
}

impl PropsBridge {
    /// A bridge for a map without props.
    pub fn none() -> Self {
        Self::default()
    }

    /// Finds the props manager under `map`: a `world_props` group member that
    /// belongs to the map, or the map's `Props` child that implements the
    /// manager API.
    pub fn discover_under(map: &Gd<Node>) -> Self {
        let tree = map.get_tree();
        for node in tree.get_nodes_in_group(GROUP).iter_shared() {
            if (node == *map || map.is_ancestor_of(&node)) && is_manager(&node) {
                return Self { node: Some(node) };
            }
        }
        if let Some(child) = map.get_node_or_null("Props")
            && is_manager(&child)
        {
            return Self { node: Some(child) };
        }
        Self::none()
    }

    /// Finds the props manager anywhere in the scene tree. Used by clients,
    /// whose map is the current scene rather than a child of a server node.
    pub fn discover_in_tree(tree: &Gd<SceneTree>) -> Self {
        // Prefer the manager belonging to this client's scene; a process can
        // host both a server and a client (tests, local matches).
        let scene = tree.get_current_scene();
        for node in tree.get_nodes_in_group(GROUP).iter_shared() {
            let ours = scene
                .as_ref()
                .is_none_or(|scene| *scene == node || scene.is_ancestor_of(&node));
            if ours && is_manager(&node) {
                return Self { node: Some(node) };
            }
        }
        for node in tree.get_nodes_in_group(GROUP).iter_shared() {
            if is_manager(&node) {
                return Self { node: Some(node) };
            }
        }
        if let Some(map) = tree.get_current_scene()
            && let Some(child) = map.get_node_or_null("Props")
            && is_manager(&child)
        {
            return Self { node: Some(child) };
        }
        Self::none()
    }

    /// Whether a live manager is available.
    pub fn is_active(&self) -> bool {
        self.node().is_some()
    }

    fn node(&self) -> Option<Gd<Node>> {
        self.node
            .as_ref()
            .filter(|node| node.is_instance_valid())
            .cloned()
    }

    /// Damages one prop. Server only: clients never call this online.
    pub fn damage_prop(&self, id: u16, amount: f32, impulse: Vec3, attacker: u8, weapon: WeaponId) {
        let Some(mut node) = self.node() else {
            return;
        };
        if !(amount.is_finite() && amount > 0.0) || !impulse.is_finite() {
            return;
        }
        node.call(
            "damage_prop",
            &[
                i32::from(id).to_variant(),
                amount.to_variant(),
                to_godot_vec(impulse),
                i32::from(attacker).to_variant(),
                i32::from(weapon.0).to_variant(),
            ],
        );
    }

    /// The props that changed since the last call, for broadcasting.
    pub fn drain_events(&self) -> Vec<PropSnapshot> {
        let Some(mut node) = self.node() else {
            return Vec::new();
        };
        snapshots_from(node.call("drain_events", &[]))
    }

    /// Every prop's current state, for a late joiner or a reset comparison.
    pub fn snapshot_states(&self) -> Vec<PropSnapshot> {
        let Some(mut node) = self.node() else {
            return Vec::new();
        };
        snapshots_from(node.call("snapshot_states", &[]))
    }

    /// Applies a server-broadcast state. Client only.
    pub fn apply_state(&self, state: &PropSnapshot) {
        let Some(mut node) = self.node() else {
            return;
        };
        node.call(
            "apply_state",
            &[
                i32::from(state.id).to_variant(),
                i32::from(state.stage).to_variant(),
                to_godot_vec(state.impulse),
            ],
        );
    }

    /// Returns every prop to its starting state, at match start.
    pub fn reset(&self) {
        let Some(mut node) = self.node() else {
            return;
        };
        node.call("reset_props", &[]);
    }

    /// Explosions the manager produced this tick, for the server to apply.
    pub fn drain_explosions(&self) -> Vec<PropExplosion> {
        let Some(mut node) = self.node() else {
            return Vec::new();
        };
        explosions_from(node.call("drain_explosions", &[]))
    }

    /// Tells the manager how many players (bots included) are in the game.
    pub fn set_population(&self, count: i32) {
        let Some(mut node) = self.node() else {
            return;
        };
        node.call("set_population", &[count.max(0).to_variant()]);
    }
}

/// Whether `node` implements the props manager API.
fn is_manager(node: &Gd<Node>) -> bool {
    REQUIRED_METHODS
        .iter()
        .all(|method| node.has_method(*method))
}

/// The dictionaries in a manager's array result. The manager returns typed
/// `Array[Dictionary]`, which does not convert to an untyped array, so both
/// forms are accepted.
fn dictionaries(value: &Variant) -> Vec<VarDictionary> {
    if let Ok(array) = value.try_to::<Array<VarDictionary>>() {
        return array.iter_shared().collect();
    }
    value
        .try_to::<Array<Variant>>()
        .map(|array| {
            array
                .iter_shared()
                .filter_map(|item| item.try_to::<VarDictionary>().ok())
                .collect()
        })
        .unwrap_or_default()
}

fn snapshots_from(value: Variant) -> Vec<PropSnapshot> {
    dictionaries(&value)
        .into_iter()
        .filter_map(|dict| {
            Some(PropSnapshot {
                id: dict_u16(&dict, "id")?,
                stage: dict_u8(&dict, "stage")?,
                impulse: dict_vec3(&dict, "impulse")?,
            })
        })
        .collect()
}

fn explosions_from(value: Variant) -> Vec<PropExplosion> {
    dictionaries(&value)
        .into_iter()
        .filter_map(|dict| {
            let radius = dict_f32(&dict, "radius")?;
            let damage = dict_f32(&dict, "damage")?;
            if !(radius.is_finite() && radius > 0.0) || !(damage.is_finite() && damage > 0.0) {
                return None;
            }
            let attacker = dict
                .get("attacker")
                .and_then(|v| v.try_to::<i64>().ok())
                .and_then(|v| u8::try_from(v).ok());
            // The manager may use -1 (or any non-weapon int) for an
            // unattributed blast; only a real weapon index gets a kill feed
            // entry.
            let weapon = dict
                .get("weapon")
                .and_then(|v| v.try_to::<i64>().ok())
                .and_then(|v| u8::try_from(v).ok())
                .map(WeaponId)
                .filter(|id| usize::from(id.0) < weapon::table().len());
            Some(PropExplosion {
                position: dict_vec3(&dict, "position")?,
                radius,
                damage,
                outer_damage: dict
                    .get("outer_damage")
                    .and_then(|v| v.try_to::<f64>().ok())
                    .filter(|v| v.is_finite() && *v >= 0.0 && *v <= f64::from(damage))
                    .unwrap_or(0.0) as f32,
                attacker,
                weapon,
            })
        })
        .collect()
}

fn dict_u16(dict: &VarDictionary, key: &str) -> Option<u16> {
    u16::try_from(dict.get(key)?.try_to::<i64>().ok()?).ok()
}

fn dict_u8(dict: &VarDictionary, key: &str) -> Option<u8> {
    u8::try_from(dict.get(key)?.try_to::<i64>().ok()?).ok()
}

fn dict_f32(dict: &VarDictionary, key: &str) -> Option<f32> {
    let value = dict.get(key)?;
    value
        .try_to::<f32>()
        .ok()
        .or_else(|| value.try_to::<i64>().ok().map(|v| v as f32))
}

fn dict_vec3(dict: &VarDictionary, key: &str) -> Option<Vec3> {
    let v = dict.get(key)?.try_to::<Vector3>().ok()?;
    let v = to_sim(v);
    v.is_finite().then_some(v)
}

fn to_godot_vec(v: Vec3) -> Variant {
    to_godot(v).to_variant()
}
