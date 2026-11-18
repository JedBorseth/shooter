//! Live grenades in a Godot world: the engine side of `shooter_sim::equipment`.
//!
//! [`GrenadeSim`] flies every live grenade through the physics space at the
//! tick rate and reports what went off. The dedicated server runs one as the
//! authority; offline the local player runs one; online the thrower's client
//! runs its own throw until the server's grenade arrives. Nothing here draws:
//! [`crate::grenade_view`] does that on clients only.

use godot::classes::{PhysicsDirectSpaceState3D, PhysicsRayQueryParameters3D};
use godot::prelude::*;
use shooter_sim::Vec3;
use shooter_sim::equipment::{self, Collide, Contact, Grenade, Kind};
use shooter_sim::net::protocol::MAX_GRENADES;

use crate::convert::{to_godot, to_sim};
use crate::target::{BODY_LAYER, WORLD_LAYER};

/// What a grenade bounces off: the map and solid bodies (dummies, props).
/// Players and zombies do not stop it.
pub const GRENADE_MASK: u32 = WORLD_LAYER | BODY_LAYER;

/// The hand a throw leaves from, relative to the eye: right, down, and
/// forward a little. A wall closer than that keeps it at the eye.
const HAND_OFFSET: Vector3 = Vector3::new(0.18, -0.12, -0.25);

/// The physics space as a grenade's world: a ray along each move, backed off
/// by the grenade's radius so it rests on surfaces instead of in them.
pub struct SpaceCollide<'a> {
    pub space: &'a mut Gd<PhysicsDirectSpaceState3D>,
}

impl Collide for SpaceCollide<'_> {
    fn sweep(&mut self, from: Vec3, to: Vec3) -> Option<Contact> {
        let length = from.distance(to);
        if length < 1e-6 {
            return None;
        }
        let query = PhysicsRayQueryParameters3D::create_ex(to_godot(from), to_godot(to))
            .collision_mask(GRENADE_MASK)
            .done()?;
        let hit = self.space.intersect_ray(&query);
        let point = to_sim(hit.get("position")?.try_to::<Vector3>().ok()?);
        let normal = to_sim(hit.get("normal")?.try_to::<Vector3>().ok()?).normalize_or_zero();
        if normal == Vec3::ZERO {
            return None;
        }
        let direction = (to - from) / length;
        // Stop where the body's surface, not its centre, meets the wall.
        let facing = (-direction.dot(normal)).max(0.2);
        let travel = from.distance(point) - equipment::RADIUS / facing;
        Some(Contact {
            fraction: (travel / length).clamp(0.0, 1.0),
            normal,
        })
    }
}

/// True if nothing solid stands between `from` and `to` (map only).
pub fn clear_line(space: &mut Gd<PhysicsDirectSpaceState3D>, from: Vector3, to: Vector3) -> bool {
    let Some(query) = PhysicsRayQueryParameters3D::create_ex(from, to)
        .collision_mask(WORLD_LAYER)
        .done()
    else {
        return true;
    };
    space.intersect_ray(&query).is_empty()
}

/// True if a blast at `center` can see a body standing at `feet`: any of
/// its shins, chest, or head in plain view.
pub fn blast_reaches(
    space: &mut Gd<PhysicsDirectSpaceState3D>,
    center: Vector3,
    feet: Vector3,
) -> bool {
    let from = center + Vector3::UP * equipment::BLAST_LIFT;
    equipment::BLAST_TARGET_HEIGHTS
        .iter()
        .any(|&height| clear_line(space, from, feet + Vector3::UP * height))
}

/// Where a throw from `eye` along `yaw` leaves the hand: beside the eye if
/// there is room, at the eye otherwise.
pub fn hand_point(space: &mut Gd<PhysicsDirectSpaceState3D>, eye: Vector3, yaw: f32) -> Vector3 {
    let basis = Basis::from_euler(EulerOrder::YXZ, Vector3::new(0.0, yaw, 0.0));
    let hand = eye + basis * HAND_OFFSET;
    let mut world = SpaceCollide { space };
    match world.sweep(to_sim(eye), to_sim(hand)) {
        Some(_) => eye,
        None => hand,
    }
}

/// One grenade in flight and whose it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiveGrenade {
    /// The thrower's player id online; offline, `None`.
    pub owner: Option<u8>,
    /// The thrower's shot number for the throw.
    pub shot: u32,
    pub grenade: Grenade,
}

/// What a tick of grenades did.
#[derive(Debug, Default)]
pub struct GrenadeTick {
    /// Grenades whose fuse ran out, removed from the field.
    pub blasts: Vec<LiveGrenade>,
    /// Bounces this tick: where, and how hard (m/s into the surface).
    pub bounces: Vec<(Vec3, f32)>,
}

/// Every live grenade in one world.
#[derive(Debug, Default)]
pub struct GrenadeSim {
    pub live: Vec<LiveGrenade>,
}

impl GrenadeSim {
    /// Adds a grenade. A full field (more than any match can throw at once)
    /// sets the oldest off now to make room.
    pub fn add(&mut self, grenade: LiveGrenade) {
        if self.live.len() >= MAX_GRENADES {
            self.live[0].grenade.fuse = 0.0;
        }
        self.live.push(grenade);
    }

    /// A throw: `fuse` 0 means it went off in the hand, where it is.
    #[allow(clippy::too_many_arguments)]
    pub fn throw(
        &mut self,
        space: &mut Gd<PhysicsDirectSpaceState3D>,
        owner: Option<u8>,
        shot: u32,
        kind: Kind,
        eye: Vector3,
        view: (f32, f32),
        carrier: Vec3,
        fuse: f32,
    ) {
        let grenade = if fuse <= 0.0 {
            Grenade::dropped(kind, to_sim(eye), Vec3::ZERO, 0.0)
        } else {
            let from = hand_point(space, eye, view.0);
            Grenade::thrown(kind, to_sim(from), view.0, view.1, carrier, fuse)
        };
        self.add(LiveGrenade {
            owner,
            shot,
            grenade,
        });
    }

    /// Flies every grenade one tick and takes out the ones that went off.
    pub fn step(&mut self, space: &mut Gd<PhysicsDirectSpaceState3D>, dt: f32) -> GrenadeTick {
        let mut out = GrenadeTick::default();
        let mut world = SpaceCollide { space };
        for live in &mut self.live {
            // Already spent (set off in the hand, or to make room).
            if live.grenade.fuse <= 0.0 {
                continue;
            }
            let flight = live.grenade.step(&mut world, dt);
            if let Some(speed) = flight.bounce {
                out.bounces.push((live.grenade.position, speed));
            }
        }
        self.live.retain(|live| {
            let spent = live.grenade.fuse <= 0.0;
            if spent {
                out.blasts.push(*live);
            }
            !spent
        });
        out
    }

    /// Takes out a grenade without it going off (its thrower's view of it
    /// once the server's arrives).
    pub fn remove(&mut self, owner: Option<u8>, shot: u32) -> Option<LiveGrenade> {
        let index = self
            .live
            .iter()
            .position(|g| g.owner == owner && g.shot == shot)?;
        Some(self.live.remove(index))
    }

    pub fn clear(&mut self) {
        self.live.clear();
    }
}
