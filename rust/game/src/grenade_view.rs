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
//!
//! Smoke clouds are keyed like grenades. Online, snapshots list every cloud
//! still hanging (`protocol::SmokeView`): a cloud the client has not drawn
//! (it joined after the pop) starts part-way through its life, and a drawn
//! cloud the server no longer lists (a new round) goes.

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

/// A smoke cloud on screen.
struct Cloud {
    key: GrenadeKey,
    node: Gd<Node3D>,
    /// A snapshot has listed it, so leaving a later one out means it is gone.
    listed: bool,
}

/// A smoke cloud the server says is hanging.
pub struct HangingCloud {
    pub key: GrenadeKey,
    /// Where the grenade popped.
    pub at: Vector3,
    /// Seconds since it popped.
    pub age: f32,
}

/// Every grenade model and smoke cloud on screen.
#[derive(Default)]
pub struct GrenadeShow {
    shown: Vec<Shown>,
    clouds: Vec<Cloud>,
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

    /// Plays the blast of grenade `key` (`kind`) at `at`, under `parent`. A
    /// smoke cloud a snapshot already started is not started twice.
    pub fn blast(&mut self, parent: &mut Gd<Node>, key: GrenadeKey, kind: Kind, at: Vector3) {
        if kind == Kind::Smoke {
            self.clouds.retain(|cloud| cloud.node.is_instance_valid());
            if self.clouds.iter().any(|cloud| cloud.key == key) {
                return;
            }
        }
        let Some(node) = self.start_blast(parent, kind, at, 0.0) else {
            return;
        };
        if kind == Kind::Smoke {
            self.clouds.push(Cloud {
                key,
                node,
                listed: false,
            });
        }
    }

    /// Online: the smoke clouds the server says are hanging. Starts any not
    /// drawn yet part-way through, and clears drawn ones it stopped listing.
    pub fn sync_clouds(&mut self, parent: &mut Gd<Node>, hanging: &[HangingCloud]) {
        self.clouds.retain(|cloud| cloud.node.is_instance_valid());
        for smoke in hanging {
            if let Some(cloud) = self.clouds.iter_mut().find(|c| c.key == smoke.key) {
                cloud.listed = true;
                continue;
            }
            if smoke.age >= equipment::SMOKE_SECONDS {
                continue;
            }
            if let Some(node) = self.start_blast(parent, Kind::Smoke, smoke.at, smoke.age) {
                self.clouds.push(Cloud {
                    key: smoke.key,
                    node,
                    listed: true,
                });
            }
        }
        self.clouds.retain_mut(|cloud| {
            let keep = !cloud.listed || hanging.iter().any(|s| s.key == cloud.key);
            if !keep {
                if cloud.node.has_method("dissipate") {
                    cloud.node.call("dissipate", &[]);
                } else {
                    cloud.node.queue_free();
                }
            }
            keep
        });
    }

    /// Instances `kind`'s blast at `at`, `age` seconds into it (only smoke
    /// clouds take an age).
    fn start_blast(
        &mut self,
        parent: &mut Gd<Node>,
        kind: Kind,
        at: Vector3,
        age: f32,
    ) -> Option<Gd<Node3D>> {
        let mut node = cached(&mut self.blasts, kind, blast_scene(kind))?
            .instantiate()?
            .try_cast::<Node3D>()
            .ok()?;
        if age > 0.0 {
            node.set("start_age", &age.to_variant());
        }
        // Placed before it enters the tree: blast scenes start their effect
        // where they are in `_ready`.
        let local = match parent.clone().try_cast::<Node3D>() {
            Ok(parent) if parent.is_inside_tree() => {
                parent.get_global_transform().affine_inverse() * at
            }
            _ => at,
        };
        node.set_position(local);
        parent.add_child(&node);
        Some(node)
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

    /// Removes every model, and forgets the clouds (they stay in the world
    /// and fade by themselves).
    pub fn clear(&mut self) {
        for mut shown in self.shown.drain(..) {
            shown.node.queue_free();
        }
        self.clouds.clear();
    }
}
