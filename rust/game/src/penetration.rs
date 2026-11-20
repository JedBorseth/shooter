//! Traces a shot through thin cover: the engine side of
//! `shooter_sim::penetration`. Each wall the ray meets is measured by casting
//! back from beyond it, and the round goes on through if the rules allow.

use godot::classes::{Node, PhysicsDirectSpaceState3D, PhysicsRayQueryParameters3D};
use godot::prelude::*;
use shooter_sim::penetration::{Material, Penetration};

use crate::player::{find_damageable, surface_of};

/// One surface a round met.
pub struct TraceHit {
    pub position: Vector3,
    pub normal: Vector3,
    pub collider: Option<Gd<Node>>,
    /// The hit triangle of a mesh collider, for its material (-1 if none).
    pub face_index: i32,
    /// Damage multiplier at this hit (1 before any wall).
    pub damage_scale: f32,
    /// Where the round leaves the far side of cover, not where it went in.
    pub exit: bool,
}

/// Where a round went: the surfaces it met in order, and where it stopped.
pub struct Trace {
    pub hits: Vec<TraceHit>,
    pub end: Vector3,
}

/// Follows a round from `from` along `direction` for `range` metres. It stops
/// at the first damageable collider (a hitbox, a dummy) or at a wall it cannot
/// go through. `budget` is the weapon's penetration.
pub fn trace(
    space: &mut Gd<PhysicsDirectSpaceState3D>,
    from: Vector3,
    direction: Vector3,
    range: f32,
    mask: u32,
    exclude: &Array<Rid>,
    budget: f32,
) -> Trace {
    if mask & crate::hitbox_rig::HITBOX_LAYER != 0 {
        crate::hitbox_rig::sync_all();
    }
    let far = from + direction * range;
    let mut round = Penetration::new(budget);
    let mut start = from;
    let mut hits = Vec::new();
    loop {
        let Some((hit, rid)) = cast(space, start, far, mask, exclude, true) else {
            return Trace { hits, end: far };
        };
        let collider = hit.collider.clone();
        let position = hit.position;
        hits.push(TraceHit {
            damage_scale: round.damage_scale,
            ..hit
        });
        if collider.as_ref().and_then(find_damageable).is_some() {
            return Trace {
                hits,
                end: position,
            };
        }
        let material = Material::from_surface(&surface_of(collider.as_ref()));
        let reach = round.reach(material);
        let exit = (reach > 0.005)
            .then(|| find_exit(space, position, direction, reach, rid, mask, exclude))
            .flatten();
        let Some(exit) = exit.filter(|e| round.cross(material, position.distance_to(e.position)))
        else {
            return Trace {
                hits,
                end: position,
            };
        };
        start = exit.position + direction * 0.005;
        // The exit hole.
        hits.push(TraceHit {
            damage_scale: round.damage_scale,
            exit: true,
            ..exit
        });
    }
}

/// The far side of the wall (collider `wall`) entered at `entry`, within
/// `reach` metres: cast back toward the entry from beyond, hitting only front
/// faces. Another object found in between is stepped past; starting inside it,
/// the next cast does not see it.
#[allow(clippy::too_many_arguments)]
fn find_exit(
    space: &mut Gd<PhysicsDirectSpaceState3D>,
    entry: Vector3,
    direction: Vector3,
    reach: f32,
    wall: Rid,
    mask: u32,
    exclude: &Array<Rid>,
) -> Option<TraceHit> {
    let mut back_from = entry + direction * reach;
    for _ in 0..3 {
        let (hit, rid) = cast(
            space,
            back_from,
            entry + direction * 0.001,
            mask,
            exclude,
            false,
        )?;
        if rid == wall && hit.normal.dot(direction) > 0.0 {
            return Some(hit);
        }
        back_from = hit.position - direction * 0.002;
        if back_from.distance_to(entry) < 0.002 {
            return None;
        }
    }
    None
}

fn cast(
    space: &mut Gd<PhysicsDirectSpaceState3D>,
    from: Vector3,
    to: Vector3,
    mask: u32,
    exclude: &Array<Rid>,
    back_faces: bool,
) -> Option<(TraceHit, Rid)> {
    let query = PhysicsRayQueryParameters3D::create_ex(from, to)
        .collision_mask(mask)
        .exclude(exclude)
        .done()?;
    let mut query = query;
    query.set_hit_back_faces(back_faces);
    let hit = space.intersect_ray(&query);
    if hit.is_empty() {
        return None;
    }
    let rid = hit
        .get_or_nil("rid")
        .try_to::<Rid>()
        .unwrap_or(Rid::Invalid);
    let hit = TraceHit {
        position: hit.get_or_nil("position").try_to::<Vector3>().ok()?,
        normal: hit
            .get_or_nil("normal")
            .try_to::<Vector3>()
            .unwrap_or(Vector3::UP),
        collider: hit.get_or_nil("collider").try_to::<Gd<Node>>().ok(),
        face_index: hit.get_or_nil("face_index").try_to::<i32>().unwrap_or(-1),
        damage_scale: 1.0,
        exit: false,
    };
    Some((hit, rid))
}
