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

/// What a tick did, for presentation.
pub struct TickOutcome {
    /// What the simulation saw before moving.
    pub env: MoveEnv,
    /// Discrete step rise, removed from the camera and eased back in.
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
    parts.body.set_velocity(to_godot(requested));
    parts.body.move_and_slide();
    let step_height = if env.on_floor && requested.y <= 0.0 && !p.state.movement.mantling() {
        step_up(&mut parts.body, start, to_godot(requested))
    } else {
        0.0
    };
    // Walls and floors clip the velocity; keep the clipped value as next tick's state.
    p.state.movement.velocity = to_sim(parts.body.get_velocity());
    p.position = to_sim(parts.body.get_global_position());
    p.on_floor = step_height > 0.0 || parts.body.is_on_floor();
    TickOutcome { env, step_height }
}

/// Retry a blocked grounded move along a collision-tested up/forward/down path.
/// The forward distance is exactly one tick: no remount boost or wall climbing.
fn step_up(body: &mut Gd<CharacterBody3D>, start: Transform3D, velocity: Vector3) -> f32 {
    const HEIGHT: f32 = 0.3;
    let motion = Vector3::new(velocity.x, 0.0, velocity.z) * TICK_DT;
    if motion.length_squared() < 0.000001 {
        return 0.0;
    }
    let normal_move = body.get_global_position() - start.origin;
    let progress = Vector3::new(normal_move.x, 0.0, normal_move.z).dot(motion.normalized());
    if progress >= motion.length() * 0.95 {
        return 0.0;
    }
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
    // Probe just beyond the capsule's leading rim. A centre-only downward
    // sweep meets the vertical riser before it can see the tread beneath it.
    let Some(mut space) = body.get_world_3d().and_then(|w| w.get_direct_space_state()) else {
        return 0.0;
    };
    let probe = raised.origin + motion.normalized() * 0.37;
    let exclude: Array<Rid> = array![body.get_rid()];
    let Some(query) = PhysicsRayQueryParameters3D::create_ex(probe, probe - Vector3::UP * HEIGHT)
        .collision_mask(WORLD_LAYER)
        .exclude(&exclude)
        .done()
    else {
        return 0.0;
    };
    let hit = space.intersect_ray(&query);
    let (Ok(top), Ok(normal)) = (
        hit.get_or_nil("position").try_to::<Vector3>(),
        hit.get_or_nil("normal").try_to::<Vector3>(),
    ) else {
        return 0.0;
    };
    let rise = top.y + 0.001 - start.origin.y;
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
    body.set_floor_snap_length(0.3);

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
