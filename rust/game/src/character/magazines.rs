//! `MagazineDrops`: magazines other players let fall from their guns. An
//! empty reload (and every pistol reload) drops the old magazine instead of
//! stowing it, as in Call of Duty: a loose copy of the gun's `WorldMagazine`
//! falls from where it sat, bounces once, and lies on its side for a few
//! seconds before it shrinks away.
//!
//! One node per scene, made on first use, with a pool of at most
//! [`MAX_DROPS`] copies (the oldest is reused). A falling magazine casts one
//! ray a frame; one lying still costs nothing but its draw. Nothing here is
//! on the server: only clients draw characters, and the copies never collide
//! with anything.

use std::cell::RefCell;

use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::{INode3D, MeshInstance3D, Node3D, PhysicsRayQueryParameters3D, SceneTree};
use godot::prelude::*;

use crate::target::WORLD_LAYER;

/// Magazines lying or falling at once; the oldest is reused.
pub const MAX_DROPS: usize = 6;
/// Seconds a dropped magazine stays, the last [`SHRINK_SECONDS`] shrinking.
pub const DROP_SECONDS: f32 = 5.0;
const SHRINK_SECONDS: f32 = 0.5;
/// Falling faster than this into the ground (m/s), it bounces once.
const BOUNCE_SPEED: f32 = 1.5;
const RESTITUTION: f32 = 0.2;
const FRICTION: f32 = 0.5;
/// Past this distance (m) a dropped magazine is not drawn.
const DRAW_DISTANCE: f32 = 40.0;

const GROUP: &str = "magazine_drops";

thread_local! {
    /// Drops waiting for the scene's node (made on first use).
    static PENDING: RefCell<Vec<Drop>> = const { RefCell::new(Vec::new()) };
}

/// One magazine let go of: its mesh and materials (from the gun's
/// `WorldMagazine`), where it was (global), and how it moves.
struct Drop {
    source: Gd<MeshInstance3D>,
    at: Transform3D,
    velocity: Vector3,
    spin: Vector3,
}

/// Lets `magazine` (a gun's `WorldMagazine`) fall from `at` (its global
/// transform, scale included) moving at `velocity` and turning at `spin`
/// (radians a second).
pub fn drop_magazine(
    tree: &Gd<SceneTree>,
    magazine: Gd<MeshInstance3D>,
    at: Transform3D,
    velocity: Vector3,
    spin: Vector3,
) {
    MagazineDrops::of(tree);
    PENDING.with_borrow_mut(|p| {
        p.push(Drop {
            source: magazine,
            at,
            velocity,
            spin,
        })
    });
}

struct Falling {
    node: Gd<MeshInstance3D>,
    /// The mesh's centre (its bounds' centre, in mesh space), so the
    /// magazine meets the ground with its middle rather than its pivot at
    /// the top, and how far its side is from that centre.
    centre: Vector3,
    half_thickness: f32,
    /// The scale the gun drew it at (an extended magazine is longer).
    scale: Vector3,
    basis: Basis,
    /// Where the centre is (global).
    position: Vector3,
    velocity: Vector3,
    spin: Vector3,
    age: f32,
    bounced: bool,
    resting: bool,
}

#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct MagazineDrops {
    base: Base<Node3D>,
    pool: Vec<Falling>,
    next: usize,
    /// Magazines dropped since the node was made.
    dropped: u64,
}

#[godot_api]
impl INode3D for MagazineDrops {
    fn process(&mut self, delta: f64) {
        for drop in PENDING.with_borrow_mut(std::mem::take) {
            if drop.source.is_instance_valid() {
                self.add(drop);
            }
        }
        self.step(delta as f32);
    }
}

#[godot_api]
impl MagazineDrops {
    /// How many magazines have been dropped since the scene began (tests).
    #[func]
    fn dropped(&self) -> i64 {
        self.dropped as i64
    }

    /// How many dropped magazines are showing (tests).
    #[func]
    fn showing(&self) -> i64 {
        self.pool.iter().filter(|f| f.node.is_visible()).count() as i64
    }

    /// How many of those lie still (tests).
    #[func]
    fn resting(&self) -> i64 {
        self.pool
            .iter()
            .filter(|f| f.node.is_visible() && f.resting)
            .count() as i64
    }

    /// The lowest point of every showing magazine's bounds (global), for
    /// tests that check they lie on the ground.
    #[func]
    fn lowest_points(&self) -> PackedVector3Array {
        self.pool
            .iter()
            .filter(|f| f.node.is_visible())
            .map(|f| {
                let aabb = f.node.get_aabb();
                let to_world = f.node.get_global_transform();
                (0..8).map(|i| to_world * aabb.get_corner(i)).fold(
                    Vector3::splat(f32::MAX),
                    |low, p| {
                        if p.y < low.y { p } else { low }
                    },
                )
            })
            .collect()
    }
}

impl MagazineDrops {
    /// The scene's node, made on first use.
    pub fn of(tree: &Gd<SceneTree>) -> Gd<MagazineDrops> {
        if let Some(node) = tree.get_first_node_in_group(GROUP)
            && let Ok(drops) = node.try_cast::<MagazineDrops>()
        {
            return drops;
        }
        let mut drops = MagazineDrops::new_alloc();
        drops.set_name("MagazineDrops");
        drops.add_to_group(GROUP);
        let mut parent = tree
            .get_current_scene()
            .unwrap_or_else(|| tree.get_root().upcast());
        parent.add_child(&drops);
        drops
    }

    fn add(&mut self, drop: Drop) {
        let Some(mesh) = drop.source.get_mesh() else {
            return;
        };
        // A free slot, a new one, or the oldest.
        let slot = match self.pool.iter().position(|f| !f.node.is_visible()) {
            Some(i) => i,
            None if self.pool.len() < MAX_DROPS => {
                let mut node = MeshInstance3D::new_alloc();
                node.set_cast_shadows_setting(ShadowCastingSetting::OFF);
                node.set_visibility_range_end(DRAW_DISTANCE);
                self.base_mut().add_child(&node);
                self.pool.push(Falling {
                    node,
                    centre: Vector3::ZERO,
                    half_thickness: 0.0,
                    scale: Vector3::ONE,
                    basis: Basis::IDENTITY,
                    position: Vector3::ZERO,
                    velocity: Vector3::ZERO,
                    spin: Vector3::ZERO,
                    age: 0.0,
                    bounced: false,
                    resting: false,
                });
                self.pool.len() - 1
            }
            None => {
                let i = self.next % self.pool.len();
                self.next = i + 1;
                i
            }
        };
        self.dropped += 1;
        let f = &mut self.pool[slot];
        f.node.set_mesh(&mesh);
        f.node
            .set_material_override(drop.source.get_material_override().as_ref());
        for surface in 0..mesh.get_surface_count() {
            f.node.set_surface_override_material(
                surface,
                drop.source.get_surface_override_material(surface).as_ref(),
            );
        }
        let aabb = mesh.get_aabb();
        let scale = drop.at.basis.get_scale();
        f.scale = scale;
        f.centre = aabb.center();
        // A magazine's thin side is its X (across the gun).
        f.half_thickness = 0.5 * aabb.size.x * scale.x;
        f.basis = drop.at.basis.orthonormalized();
        f.position = drop.at * f.centre;
        f.velocity = drop.velocity;
        f.spin = drop.spin;
        f.age = 0.0;
        f.bounced = false;
        f.resting = false;
        f.node.set_visible(true);
        place(f);
    }

    fn step(&mut self, dt: f32) {
        let mut space = None;
        for f in &mut self.pool {
            if !f.node.is_visible() {
                continue;
            }
            f.age += dt;
            if f.age >= DROP_SECONDS {
                f.node.set_visible(false);
                continue;
            }
            if f.resting {
                let left = DROP_SECONDS - f.age;
                if left < SHRINK_SECONDS {
                    f.node
                        .set_scale(f.scale * (left / SHRINK_SECONDS).max(0.01));
                }
                continue;
            }
            f.velocity += Vector3::DOWN * 9.8 * dt;
            // Aim the ray from the centre to past its lower side.
            let next = f.position + f.velocity * dt;
            let reach = next + Vector3::DOWN * f.half_thickness;
            if space.is_none() {
                space = f
                    .node
                    .get_world_3d()
                    .and_then(|w| w.get_direct_space_state());
            }
            let hit = space.clone().and_then(|mut space| {
                let query = PhysicsRayQueryParameters3D::create_ex(f.position, reach)
                    .collision_mask(WORLD_LAYER)
                    .done()?;
                let hit = space.intersect_ray(&query);
                let at = hit.get_or_nil("position").try_to::<Vector3>().ok()?;
                let normal = hit
                    .get_or_nil("normal")
                    .try_to::<Vector3>()
                    .unwrap_or(Vector3::UP);
                Some((at, normal))
            });
            match hit {
                Some((at, normal)) => {
                    let into = -f.velocity.dot(normal);
                    if into > BOUNCE_SPEED && !f.bounced && normal.y > 0.5 {
                        f.bounced = true;
                        let along = f.velocity + normal * into;
                        f.velocity = along * FRICTION + normal * into * RESTITUTION;
                        f.spin *= 0.4;
                        f.position = at + normal * f.half_thickness;
                    } else {
                        // Lies on its side, its length along the ground.
                        f.resting = true;
                        f.velocity = Vector3::ZERO;
                        f.basis = lying_on_side(f.basis, normal);
                        f.position = at + normal * f.half_thickness;
                    }
                }
                None => {
                    f.position = next;
                    let turn = f.spin * dt;
                    if turn.length_squared() > 0.0 {
                        f.basis =
                            Basis::from_axis_angle(turn.normalized(), turn.length()) * f.basis;
                    }
                }
            }
            place(f);
        }
    }
}

/// Draws a falling magazine where its centre is.
fn place(f: &mut Falling) {
    let basis = f.basis * Basis::from_scale(f.scale);
    let origin = f.position - basis * f.centre;
    f.node.set_global_transform(Transform3D::new(basis, origin));
}

/// A magazine come to rest on ground with `normal`: on its broad side (its
/// X along the normal), its length (Y) laid along the ground as near its
/// fall's direction as possible.
pub fn lying_on_side(basis: Basis, normal: Vector3) -> Basis {
    let length = basis.col_b();
    let mut flat = length - normal * length.dot(normal);
    if flat.length_squared() < 1e-4 {
        flat = basis.col_c() - normal * basis.col_c().dot(normal);
    }
    if flat.length_squared() < 1e-4 {
        flat = if normal.x.abs() < 0.9 {
            Vector3::RIGHT
        } else {
            Vector3::FORWARD
        };
        flat -= normal * flat.dot(normal);
    }
    let flat = flat.normalized();
    // Keep whichever broad side was lower facing down.
    let side = if basis.col_a().dot(normal) >= 0.0 {
        normal
    } else {
        -normal
    };
    Basis::from_cols(side, flat, side.cross(flat))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_magazine_lies_on_its_side_along_the_ground() {
        // Falling end first, tipped a little.
        let basis = Basis::from_axis_angle(Vector3::FORWARD, 0.3);
        let lying = lying_on_side(basis, Vector3::UP);
        assert!(lying.col_a().dot(Vector3::UP).abs() > 0.999, "{lying:?}");
        assert!(lying.col_b().y.abs() < 1e-5, "length along the ground");
        assert!((lying.determinant() - 1.0).abs() < 1e-4, "a rotation");
        // Straight down its length: still lies flat.
        let lying = lying_on_side(Basis::IDENTITY, Vector3::UP);
        assert!(lying.col_b().y.abs() < 1e-5);
        assert!((lying.determinant() - 1.0).abs() < 1e-4);
    }
}
