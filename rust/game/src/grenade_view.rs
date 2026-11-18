//! Drawing live grenades and their blasts, on clients only.
//!
//! Each live grenade is one instance of the frag model, placed between the
//! last two positions the simulation (offline, or the thrower's own throw)
//! or the server's snapshots gave it, and rolled by the distance it covers.
//! Grenades are keyed by thrower and shot number, so the thrower's own throw
//! hands over to the server's grenade without the model popping.
//!
//! A blast instances `scenes/equipment/frag_blast.tscn`, whose script plays
//! the bang and asks the pooled combat effects for the explosion
//! (`CombatEffects::explosion`); replacing that scene changes every
//! grenade's blast.

use godot::classes::{Node3D, PackedScene, ResourceLoader};
use godot::prelude::*;
use shooter_sim::equipment::{self, Kind};

/// Each kind's effect when it goes off.
fn blast_scene(kind: Kind) -> &'static str {
    match kind {
        Kind::Frag => "res://scenes/equipment/frag_blast.tscn",
        Kind::Flash => "res://scenes/equipment/flash_pop.tscn",
        Kind::Smoke => "res://scenes/equipment/smoke_cloud.tscn",
    }
}

/// Every drawn grenade is in this group (tests find them by it).
pub const GRENADE_GROUP: &str = "grenades";

/// Who threw a grenade (online player id, or `None` offline) and their shot
/// number for it.
pub type GrenadeKey = (Option<u8>, u32);

struct Shown {
    key: GrenadeKey,
    node: Gd<Node3D>,
    from: Vector3,
    to: Vector3,
    /// Seconds since `to` arrived, and the time between positions.
    age: f32,
    interval: f32,
    /// Placed in the latest update; unplaced ones are gone.
    seen: bool,
    /// Rolled so far, about the axis it rolls on.
    spin: Basis,
}

/// Every grenade model on screen.
#[derive(Default)]
pub struct GrenadeShow {
    shown: Vec<Shown>,
    models: Vec<(Kind, Gd<PackedScene>)>,
    blasts: Vec<(Kind, Gd<PackedScene>)>,
}

/// A scene from `cache`, loading it into it the first time.
fn cached(
    cache: &mut Vec<(Kind, Gd<PackedScene>)>,
    kind: Kind,
    path: &str,
) -> Option<Gd<PackedScene>> {
    if let Some((_, scene)) = cache.iter().find(|(k, _)| *k == kind) {
        return Some(scene.clone());
    }
    let scene = load_scene(path)?;
    cache.push((kind, scene.clone()));
    Some(scene)
}

fn load_scene(path: &str) -> Option<Gd<PackedScene>> {
    ResourceLoader::singleton()
        .load(path)
        .and_then(|r| r.try_cast::<PackedScene>().ok())
}

impl GrenadeShow {
    /// Starts an update: grenades not placed before [`Self::end`] are removed.
    pub fn begin(&mut self) {
        for shown in &mut self.shown {
            shown.seen = false;
        }
    }

    /// The grenade `key` is at `at` now; positions arrive `interval` seconds
    /// apart. `parent` holds new models.
    pub fn place(
        &mut self,
        parent: &mut Gd<Node>,
        key: GrenadeKey,
        kind: Kind,
        at: Vector3,
        interval: f32,
    ) {
        if let Some(shown) = self.shown.iter_mut().find(|s| s.key == key) {
            shown.from = shown.node.get_global_position();
            shown.to = at;
            shown.age = 0.0;
            shown.interval = interval.max(1e-3);
            shown.seen = true;
            return;
        }
        let Some(node) = cached(&mut self.models, kind, crate::grenade_hold::model(kind))
            .and_then(|scene| scene.instantiate())
            .and_then(|node| node.try_cast::<Node3D>().ok())
        else {
            return;
        };
        let mut node = node;
        node.set_name("Grenade");
        node.add_to_group(GRENADE_GROUP);
        // Thrown: the pin is out and the spoon flew off.
        crate::grenade_hold::set_pinned(&node, false, false);
        parent.add_child(&node);
        node.set_global_position(at);
        self.shown.push(Shown {
            key,
            node,
            from: at,
            to: at,
            age: 0.0,
            interval: interval.max(1e-3),
            seen: true,
            spin: Basis::IDENTITY,
        });
    }

    /// Ends an update, removing grenades that were not placed.
    pub fn end(&mut self) {
        self.shown.retain_mut(|shown| {
            if !shown.seen {
                shown.node.queue_free();
            }
            shown.seen
        });
    }

    /// Takes one grenade off the screen now (it went off).
    pub fn remove(&mut self, key: GrenadeKey) {
        if let Some(index) = self.shown.iter().position(|s| s.key == key) {
            let mut shown = self.shown.remove(index);
            shown.node.queue_free();
        }
    }

    /// Moves every model along its path for this frame and rolls it by the
    /// distance covered.
    pub fn draw(&mut self, dt: f32) {
        for shown in &mut self.shown {
            shown.age += dt;
            let t = (shown.age / shown.interval).clamp(0.0, 1.0);
            let before = shown.node.get_global_position();
            let at = shown.from.lerp(shown.to, t);
            let moved = at - before;
            let ground = Vector3::new(moved.x, 0.0, moved.z);
            let distance = moved.length();
            if distance > 1e-4 {
                // Roll about the horizontal axis across the motion; a lob
                // tumbles about the same axis.
                let axis = if ground.length() > 1e-4 {
                    Vector3::UP.cross(ground).normalized()
                } else {
                    Vector3::RIGHT
                };
                let angle = distance / equipment::RADIUS * 0.5;
                shown.spin = Basis::from_axis_angle(axis, angle) * shown.spin;
                shown.spin = shown.spin.orthonormalized();
            }
            shown
                .node
                .set_global_transform(Transform3D::new(shown.spin, at));
        }
    }

    /// Plays `kind`'s blast at `at`, under `parent`.
    pub fn blast(&mut self, parent: &mut Gd<Node>, kind: Kind, at: Vector3) {
        let Some(node) = cached(&mut self.blasts, kind, blast_scene(kind))
            .and_then(|scene| scene.instantiate())
            .and_then(|node| node.try_cast::<Node3D>().ok())
        else {
            return;
        };
        let mut node = node;
        parent.add_child(&node);
        node.set_global_position(at);
    }

    /// Where every drawn grenade is now.
    pub fn positions(&self) -> impl Iterator<Item = Vector3> + '_ {
        self.shown
            .iter()
            .map(|shown| shown.node.get_global_position())
    }

    pub fn is_empty(&self) -> bool {
        self.shown.is_empty()
    }

    /// Removes every model.
    pub fn clear(&mut self) {
        for mut shown in self.shown.drain(..) {
            shown.node.queue_free();
        }
    }
}
