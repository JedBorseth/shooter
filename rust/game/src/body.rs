//! One simulation tick applied to a `CharacterBody3D`.
//!
//! The local player, client-side replays, and the server's players all move
//! through [`tick`], so a given input produces the same result on the client
//! and the server. Keep the body setup in [`build_body`] identical to
//! `scenes/player.tscn`; any difference shows up as mispredictions.

use godot::classes::{
    CapsuleShape3D, CharacterBody3D, CollisionShape3D, PhysicsDirectSpaceState3D,
    PhysicsRayQueryParameters3D, ShapeCast3D, SphereShape3D,
};
use godot::prelude::*;
use shooter_sim::movement::{MoveEnv, MoveParams, Stance};
use shooter_sim::net::protocol::Predicted;
use shooter_sim::net::{TICK_DT, TICK_RATE};
use shooter_sim::player::{self, PlayerEvent, PlayerInput, PlayerParams};

use crate::convert::{to_godot, to_sim};
use crate::target::WORLD_LAYER;

/// Collision layer 2 ("players"), as a bit mask.
pub const PLAYER_LAYER: u32 = 1 << 1;

/// Collision layer 7: walls that stop players but not bullets or zombies
/// (the Zombies arena's boarded windows).
pub const PLAYER_BLOCK_LAYER: u32 = 1 << 6;

/// What a player body collides with: walls and floors, solid bodies such as
/// target dummies, and player-only blockers. `scenes/player.tscn` uses the
/// same mask (81), so the server's bodies stop where the client's do.
pub const PLAYER_COLLISION_MASK: u32 =
    crate::target::WORLD_LAYER | crate::target::BODY_LAYER | PLAYER_BLOCK_LAYER;

/// Radius of the sphere swept up for headroom before standing: as wide as
/// the capsule (0.35) less a sliver, so a wall the player leans on does not
/// count as a ceiling but an overhang's edge over the shoulders does.
pub const HEADROOM_RADIUS: f32 = 0.34;

/// The nodes a simulated body needs besides itself.
#[derive(Clone)]
pub struct BodyParts {
    pub body: Gd<CharacterBody3D>,
    pub collision: Gd<CollisionShape3D>,
    pub headroom: Gd<ShapeCast3D>,
}

/// True when the engine's physics rate matches the simulation tick. `tick`
/// steps by `TICK_DT`, so a mismatch makes everything run at the wrong speed
/// and the client and server disagree. Logs an error when it doesn't match.
pub fn check_tick_rate() -> bool {
    let engine = godot::classes::Engine::singleton().get_physics_ticks_per_second();
    let ok = engine == TICK_RATE as i32;
    if !ok {
        godot_error!(
            "physics runs at {engine} Hz but the simulation tick is {TICK_RATE} Hz; \
             set physics/common/physics_ticks_per_second to match shooter_sim::net::TICK_RATE"
        );
    }
    ok
}

/// The highest step a player walks up without jumping (classic IW: 18
/// inches), and how far the floor snap follows a step down.
pub const STEP_HEIGHT: f32 = 0.45;

/// Extra downward speed (m/s) that keeps a grounded body on the floor.
const GROUND_STICK: f32 = 0.5;

/// Grounded drops past what the steepest walkable slope explains by more
/// than this count as stepping down, which the camera eases.
const STEP_DOWN_MIN: f32 = 0.005;

/// What a tick did, for presentation.
pub struct TickOutcome {
    /// What the simulation saw before moving.
    pub env: MoveEnv,
    /// Discrete step up (positive) or down (negative) on the ground, removed
    /// from the camera and eased back in.
    pub step_height: f32,
}

/// Advances `p` by one input: simulation step, then collision. `p.on_floor`
/// must be the floor state after the previous move.
pub fn tick(
    parts: &mut BodyParts,
    params: &PlayerParams,
    p: &mut Predicted,
    input: &PlayerInput,
    events: &mut Vec<PlayerEvent>,
) -> TickOutcome {
    parts.headroom.force_shapecast_update();
    let env = MoveEnv {
        on_floor: p.on_floor,
        can_stand: !parts.headroom.is_colliding(),
        ledge: find_ledge(parts, &params.movement, p, input.yaw),
    };
    let previous_crouch = p.state.movement.crouch;
    let requested = player::step(&mut p.state, input, env, params, TICK_DT, events);
    if previous_crouch != p.state.movement.crouch {
        apply_stance_shape(parts, &params.movement, p.state.movement.crouch);
    }

    let start = parts.body.get_global_transform();
    let grounded = env.on_floor && requested.y <= 0.0 && !p.state.movement.mantling();
    parts.body.set_velocity(to_godot(requested));
    parts.body.move_and_slide();
    let mut step_height = if grounded {
        step_up(&mut parts.body, start, to_godot(requested))
    } else {
        0.0
    };
    // Walls and floors clip the velocity; keep the clipped value as next tick's state.
    let velocity = parts.body.get_velocity();
    if grounded
        && step_height == 0.0
        && !parts.body.is_on_floor()
        && going_down(parts, to_godot(requested))
        && floor_below(parts, STEP_HEIGHT)
    {
        // Rolling over a stair's edge, the capsule's round bottom ends on a
        // contact too steep to count as floor and the engine's snap gives
        // up. Slide down it, as fast as the steepest walkable slope falls at
        // this speed, so the feet follow the stairs instead of falling.
        let flat = Vector2::new(velocity.x, velocity.z).length();
        let drop = flat * parts.body.get_floor_max_angle().tan() + GROUND_STICK;
        parts.body.set_velocity(Vector3::DOWN * drop);
        parts.body.move_and_slide();
        parts.body.set_velocity(velocity);
    }
    p.state.movement.velocity = to_sim(velocity);
    p.position = to_sim(parts.body.get_global_position());
    // Rolling over an edge, the contact is too steep to count as floor, but
    // with walkable ground a step below the feet the player is still on it.
    p.on_floor = step_height > 0.0
        || parts.body.is_on_floor()
        || (grounded && floor_below(parts, STEP_HEIGHT));
    // Going down stairs the body drops faster than any walkable slope could:
    // that excess is a step down, eased out of the camera.
    let end = parts.body.get_global_position();
    let moved = Vector2::new(end.x - start.origin.x, end.z - start.origin.z).length();
    let slope = moved * parts.body.get_floor_max_angle().tan();
    let excess = start.origin.y - end.y - slope;
    if step_height == 0.0 && grounded && p.on_floor && excess > STEP_DOWN_MIN {
        step_height = -excess;
    }
    TickOutcome { env, step_height }
}

/// True when the ground just past the capsule's front rim is lower than the
/// feet: walking down stairs, not up them (an early step up also leaves the
/// body over a lower tread, but with the next one ahead of it).
fn going_down(parts: &BodyParts, velocity: Vector3) -> bool {
    let body = &parts.body;
    let flat = Vector3::new(velocity.x, 0.0, velocity.z);
    if flat.length_squared() < 1e-6 {
        return false;
    }
    let Some(mut space) = body.get_world_3d().and_then(|w| w.get_direct_space_state()) else {
        return false;
    };
    let feet = body.get_global_position();
    let ahead = feet + flat.normalized() * 0.37;
    let exclude: Array<Rid> = array![body.get_rid()];
    let Some(query) = PhysicsRayQueryParameters3D::create_ex(
        ahead + Vector3::UP * STEP_HEIGHT,
        ahead - Vector3::UP * STEP_HEIGHT,
    )
    .collision_mask(WORLD_LAYER)
    .exclude(&exclude)
    .done() else {
        return false;
    };
    space
        .intersect_ray(&query)
        .get("position")
        .and_then(|p| p.try_to::<Vector3>().ok())
        .is_none_or(|p| p.y < feet.y - 0.02)
}

/// True when walkable ground lies within `depth` below the feet (straight
/// down from the body's centre line).
fn floor_below(parts: &BodyParts, depth: f32) -> bool {
    let body = &parts.body;
    let Some(mut space) = body.get_world_3d().and_then(|w| w.get_direct_space_state()) else {
        return false;
    };
    let feet = body.get_global_position();
    let exclude: Array<Rid> = array![body.get_rid()];
    let Some(query) = PhysicsRayQueryParameters3D::create_ex(
        feet + Vector3::UP * 0.05,
        feet - Vector3::UP * depth,
    )
    .collision_mask(WORLD_LAYER)
    .exclude(&exclude)
    .done() else {
        return false;
    };
    space
        .intersect_ray(&query)
        .get("normal")
        .and_then(|n| n.try_to::<Vector3>().ok())
        .is_some_and(|n| n.dot(Vector3::UP) >= body.get_floor_max_angle().cos())
}

/// The way into the wall that blocked this tick's move, if one did: the
/// horizontal reverse of its normal. Works at any angle, including a body
/// already sliding along a riser that the input still pushes into. The
/// capsule's rounded bottom meets a low riser's edge at a slant, so anything
/// too steep to stand on counts.
fn blocking_wall(body: &Gd<CharacterBody3D>, motion: Vector3) -> Option<Vector3> {
    let floor = body.get_floor_max_angle().cos();
    (0..body.get_slide_collision_count()).find_map(|i| {
        let collision = body.get_slide_collision(i)?;
        let normal = collision.get_normal();
        let flat = Vector3::new(normal.x, 0.0, normal.z);
        (normal.y < floor
            && normal.y > -0.3
            && flat.length_squared() > 0.01
            && motion.dot(flat) < -1e-5)
            .then(|| -flat.normalized())
    })
}

/// Retry a blocked grounded move along a collision-tested up/forward/down path.
/// The forward distance is one tick, nudged into the step when the move ran
/// along it at an angle: no remount boost or wall climbing.
fn step_up(body: &mut Gd<CharacterBody3D>, start: Transform3D, velocity: Vector3) -> f32 {
    const HEIGHT: f32 = STEP_HEIGHT;
    let motion = Vector3::new(velocity.x, 0.0, velocity.z) * TICK_DT;
    if motion.length_squared() < 0.000001 {
        return 0.0;
    }
    let Some(into) = blocking_wall(body, motion) else {
        return 0.0;
    };
    // At least a few centimetres into the step, so a body running along the
    // riser at a shallow angle still gets its leading edge over the tread.
    let motion = motion + into * (0.03 - motion.dot(into)).max(0.0);
    // Check the whole capsule, including ceilings, before committing any move.
    if body.test_move(start, Vector3::UP * HEIGHT) {
        return 0.0;
    }
    let mut raised = start;
    raised.origin += Vector3::UP * HEIGHT;
    if body.test_move(raised, motion) {
        return 0.0;
    }
    raised.origin += motion;
    // Probe just beyond the capsule's rim, toward the step. A centre-only
    // downward sweep meets the vertical riser before it can see the tread.
    let Some(mut space) = body.get_world_3d().and_then(|w| w.get_direct_space_state()) else {
        return 0.0;
    };
    let exclude: Array<Rid> = array![body.get_rid()];
    let mut down = |from: Vector3| {
        let query = PhysicsRayQueryParameters3D::create_ex(from, from - Vector3::UP * HEIGHT)
            .collision_mask(WORLD_LAYER)
            .exclude(&exclude)
            .done()?;
        let hit = space.intersect_ray(&query);
        Some((
            hit.get("position")?.try_to::<Vector3>().ok()?,
            hit.get("normal")?.try_to::<Vector3>().ok()?,
        ))
    };
    let Some((top, normal)) = down(raised.origin + into * 0.37) else {
        return 0.0;
    };
    let mut rise = top.y + 0.001 - start.origin.y;
    // On stairs a probe past the rim can see the tread after next; step only
    // as high as the tread under the body when that is lower, so the round
    // bottom does not land on an edge and roll back.
    if let Some((under, under_normal)) = down(raised.origin) {
        let centre = under.y + 0.001 - start.origin.y;
        if centre >= 0.05
            && centre < rise - 0.05
            && under_normal.dot(Vector3::UP) >= body.get_floor_max_angle().cos()
        {
            rise = centre;
        }
    }
    if !(0.001..=HEIGHT).contains(&rise)
        || normal.dot(Vector3::UP) < body.get_floor_max_angle().cos()
        || body.test_move(raised, Vector3::DOWN * (HEIGHT - rise))
    {
        return 0.0;
    }
    body.set_global_position(start.origin + motion + Vector3::UP * rise);
    body.set_velocity(Vector3::new(velocity.x, 0.0, velocity.z));
    rise
}

/// The height of a ledge in front of the player they could climb onto: a
/// wall within reach at knee height, a top surface `mantle_min` to
/// `mantle_max` above the feet just past its face, and room to stand on it.
/// Only looked for while the player could use it (alive, not already
/// climbing or crouched). Client and server see the same geometry, so both
/// find the same ledge.
fn find_ledge(parts: &BodyParts, params: &MoveParams, p: &Predicted, yaw: f32) -> Option<f32> {
    let movement = &p.state.movement;
    if !p.state.alive() || movement.mantling() || movement.stance != Stance::Standing {
        return None;
    }
    let mut space = parts.body.get_world_3d()?.get_direct_space_state()?;
    let feet = to_godot(p.position);
    let (forward, _) = shooter_sim::movement::basis_from_yaw(yaw);
    let forward = to_godot(forward);
    let exclude: Array<Rid> = array![parts.body.get_rid()];
    let ray = |space: &mut Gd<PhysicsDirectSpaceState3D>, from: Vector3, to: Vector3| {
        let query = PhysicsRayQueryParameters3D::create_ex(from, to)
            .collision_mask(WORLD_LAYER)
            .exclude(&exclude)
            .done()?;
        let hit = space.intersect_ray(&query);
        hit.get_or_nil("position").try_to::<Vector3>().ok()
    };
    // A wall in front, at knee height.
    let knee = feet + Vector3::UP * (params.mantle_min * 0.8);
    let wall = ray(&mut space, knee, knee + forward * 0.75)?;
    // Its top, just past the face.
    let over = Vector3::new(wall.x, feet.y, wall.z) + forward * 0.3;
    let top = ray(
        &mut space,
        over + Vector3::UP * (params.mantle_max + 0.3),
        over + Vector3::UP * (params.mantle_min * 0.5),
    )?;
    let height = top.y - feet.y;
    if !(params.mantle_min..=params.mantle_max).contains(&height) {
        return None;
    }
    // Room to stand on top, and nothing between the head and the top.
    let head = feet + Vector3::UP * (height + 0.1);
    if ray(
        &mut space,
        top + Vector3::UP * 0.05,
        top + Vector3::UP * params.stand_height,
    )
    .is_some()
        || ray(
            &mut space,
            Vector3::new(feet.x, top.y + params.stand_height * 0.6, feet.z),
            top + Vector3::UP * params.stand_height * 0.6,
        )
        .is_some()
        || ray(&mut space, head, Vector3::new(top.x, head.y, top.z)).is_some()
    {
        return None;
    }
    Some(height)
}

/// Moves the body to a state without simulating (spawns, reconciliation).
pub fn place(parts: &mut BodyParts, params: &MoveParams, p: &Predicted) {
    apply_stance_shape(parts, params, p.state.movement.crouch);
    let position = to_godot(p.position);
    parts.body.set_global_position(position);
    sync_floor_contact(&mut parts.body, p.on_floor);
    parts.body.set_global_position(position);
    parts.body.set_velocity(to_godot(p.state.movement.velocity));
}

/// `move_and_slide` snaps to the floor only if the body touched it on its
/// previous move. After a teleport that memory belongs to the old position, so
/// reset it to match the state: otherwise a replay from mid-air snaps down
/// where the server kept falling.
fn sync_floor_contact(body: &mut Gd<CharacterBody3D>, on_floor: bool) {
    if body.is_on_floor() == on_floor {
        return;
    }
    if on_floor {
        body.apply_floor_snap();
    } else {
        // A zero-length move with snapping off recomputes contact in place.
        let snap = body.get_floor_snap_length();
        body.set_floor_snap_length(0.0);
        body.set_velocity(Vector3::ZERO);
        body.move_and_slide();
        body.set_floor_snap_length(snap);
    }
}

/// Resizes the capsule for a crouch blend (`MoveState::crouch`, 0 standing
/// to 1 crouched), keeping its bottom at the body origin (the feet).
pub fn apply_stance_shape(parts: &mut BodyParts, params: &MoveParams, crouch: f32) {
    let height = params.height_at(crouch);
    if let Some(shape) = parts.collision.get_shape()
        && let Ok(mut capsule) = shape.try_cast::<CapsuleShape3D>()
        && capsule.get_height() != height
    {
        capsule.set_height(height);
    }
    parts
        .collision
        .set_position(Vector3::new(0.0, height / 2.0, 0.0));
}

/// Builds a player body like `scenes/player.tscn`, with its own shapes (a
/// shared shape resource would resize every player when one crouches).
pub fn build_body(name: &str) -> BodyParts {
    let mut body = CharacterBody3D::new_alloc();
    body.set_name(name);
    body.set_collision_layer(PLAYER_LAYER);
    body.set_collision_mask(PLAYER_COLLISION_MASK);
    body.set_floor_constant_speed_enabled(true);
    body.set_floor_snap_length(STEP_HEIGHT);

    let mut capsule = CapsuleShape3D::new_gd();
    capsule.set_radius(0.35);
    capsule.set_height(1.8);
    let mut collision = CollisionShape3D::new_alloc();
    collision.set_name("Collision");
    collision.set_shape(&capsule);
    collision.set_position(Vector3::new(0.0, 0.9, 0.0));
    body.add_child(&collision);

    let mut sphere = SphereShape3D::new_gd();
    sphere.set_radius(HEADROOM_RADIUS);
    let mut headroom = ShapeCast3D::new_alloc();
    headroom.set_name("Headroom");
    headroom.set_enabled(false);
    headroom.set_shape(&sphere);
    headroom.set_position(Vector3::new(0.0, 0.8, 0.0));
    headroom.set_target_position(Vector3::new(0.0, 0.7, 0.0));
    body.add_child(&headroom);
    headroom.add_exception_rid(body.get_rid());

    BodyParts {
        body,
        collision,
        headroom,
    }
}
