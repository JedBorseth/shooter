//! One simulation tick applied to a `CharacterBody3D`.
//!
//! The local player, client-side replays, and the server's players all move
//! through [`tick`], so a given input produces the same result on the client
//! and the server. Keep the body setup in [`build_body`] identical to
//! `scenes/player.tscn`; any difference shows up as mispredictions.

use godot::classes::{
    CapsuleShape3D, CharacterBody3D, CollisionShape3D, ShapeCast3D, SphereShape3D,
};
use godot::prelude::*;
use shooter_sim::movement::{MoveEnv, MoveParams, Stance};
use shooter_sim::net::protocol::Predicted;
use shooter_sim::net::{TICK_DT, TICK_RATE};
use shooter_sim::player::{self, PlayerEvent, PlayerInput, PlayerParams};

use crate::convert::{to_godot, to_sim};

/// Collision layer 2 ("players"), as a bit mask.
pub const PLAYER_LAYER: u32 = 1 << 1;

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
    };
    let previous_stance = p.state.movement.stance;
    let requested = player::step(&mut p.state, input, env, params, TICK_DT, events);
    let movement = &params.movement;
    if movement.height(previous_stance) != movement.height(p.state.movement.stance) {
        apply_stance_shape(parts, movement, p.state.movement.stance);
    }

    parts.body.set_velocity(to_godot(requested));
    parts.body.move_and_slide();
    // Walls and floors clip the velocity; keep the clipped value as next tick's state.
    p.state.movement.velocity = to_sim(parts.body.get_velocity());
    p.position = to_sim(parts.body.get_global_position());
    p.on_floor = parts.body.is_on_floor();
    TickOutcome { env }
}

/// Moves the body to a state without simulating (spawns, reconciliation).
pub fn place(parts: &mut BodyParts, params: &MoveParams, p: &Predicted) {
    apply_stance_shape(parts, params, p.state.movement.stance);
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

/// Resizes the capsule for a stance, keeping its bottom at the body origin (the feet).
pub fn apply_stance_shape(parts: &mut BodyParts, params: &MoveParams, stance: Stance) {
    let height = params.height(stance);
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
    // Walls and floors, and solid bodies such as target dummies.
    body.set_collision_mask(crate::target::WORLD_LAYER | crate::target::BODY_LAYER);
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
    sphere.set_radius(0.3);
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
