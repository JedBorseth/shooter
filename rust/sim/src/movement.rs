//! First-person movement: walk, sprint, crouch, slide, jump, and mantle.
//!
//! The model is the Quake/Source "accelerate toward a wish velocity, then apply
//! friction" approach, tuned for faster, snappier, arcade-military pacing. The engine
//! owns collision: each tick it calls [`step`], moves the body with the
//! returned velocity, and writes the post-collision velocity back into
//! [`MoveState::velocity`].
//!
//! `step` is a pure function of `(state, input, env, params, dt)`. That is what
//! client-side prediction needs: the client can replay stored inputs from the
//! last server-confirmed state and land on the same result.

use glam::{Vec2, Vec3};

/// Player intent for one simulation tick. Built from devices on the client and
/// received over the network on the server.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveInput {
    /// Desired direction in view space: `x` is right, `y` is forward. Length ≤ 1.
    pub wish: Vec2,
    /// View yaw in radians. 0 looks down -Z; positive turns left (Godot convention).
    pub yaw: f32,
    pub jump: bool,
    /// Sprint button state. Sprint is a toggle: a press starts it, and it lasts
    /// until the player stops moving forward, crouches, or `block_sprint` is set.
    pub sprint: bool,
    /// Crouch button state. Crouch toggles on each press (C), as on classic PC shooters.
    pub crouch: bool,
    /// Set while firing, aiming, or starting a reload: ends and prevents sprinting.
    pub block_sprint: bool,
    /// Scales every ground speed (weapon mobility, aiming down sights).
    pub speed_multiplier: f32,
    /// A melee starts this tick and lunges at a target this far away
    /// (horizontal `x` and `z`, feet to feet). See [`crate::melee`].
    pub lunge: Option<Vec2>,
}

impl Default for MoveInput {
    fn default() -> Self {
        Self {
            wish: Vec2::ZERO,
            yaw: 0.0,
            jump: false,
            sprint: false,
            crouch: false,
            block_sprint: false,
            speed_multiplier: 1.0,
            lunge: None,
        }
    }
}

/// What the engine observed about the world this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveEnv {
    pub on_floor: bool,
    /// False when there is no headroom to stand up from a crouch.
    pub can_stand: bool,
    /// Height above the feet of a ledge in front of the player that they could
    /// climb onto (with room to stand on top), if there is one in reach.
    pub ledge: Option<f32>,
}

impl Default for MoveEnv {
    fn default() -> Self {
        Self {
            on_floor: true,
            can_stand: true,
            ledge: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stance {
    #[default]
    Standing,
    Crouching,
    Sliding,
}

/// Per-player state carried from tick to tick. Small and `Copy` so prediction
/// can snapshot it into a ring buffer every tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MoveState {
    pub velocity: Vec3,
    pub stance: Stance,
    pub sprinting: bool,
    /// Seconds of slide remaining while `stance == Sliding`.
    pub slide_time: f32,
    /// Seconds until another slide is allowed.
    pub slide_cooldown: f32,
    /// Seconds left in which a jump press still counts (jump buffering).
    pub jump_buffer: f32,
    /// Seconds left in which the player may still jump after leaving the floor
    /// (coyote time).
    pub coyote: f32,
    /// Previous tick's buttons, for edge detection without engine help.
    pub jump_held: bool,
    pub crouch_held: bool,
    pub sprint_held: bool,
    /// Seconds left climbing over a ledge (0 when not mantling).
    pub mantle_time: f32,
    /// Upward speed while mantling, set when the climb starts.
    pub mantle_rise: f32,
    /// Seconds left in a melee lunge (0 when not lunging).
    pub lunge_time: f32,
    /// Horizontal velocity of the lunge (`x`, `z`). It stays set for the tick
    /// after the lunge ends, which takes the speed off.
    pub lunge_velocity: Vec2,
    /// How far down the body is, 0 standing to 1 crouched (or sliding). The
    /// stance flips at once; this follows it over `crouch_down_time` or
    /// `stand_up_time`, and drives the capsule, the eye (camera and shot
    /// origin), the hit zones, and the speed cap together.
    pub crouch: f32,
    /// Seconds of unbroken sprint on the ground. A slide has to be earned
    /// with `slide_min_sprint` of it.
    pub sprint_time: f32,
    /// Horizontal direction (`x`, `z`) of the current mantle, fixed when it
    /// starts so turning mid-climb does not pull the player off the wall.
    pub mantle_dir: Vec2,
}

impl MoveState {
    pub fn mantling(&self) -> bool {
        self.mantle_time > 0.0
    }

    /// Capsule height now, from the crouch blend.
    pub fn height(&self, params: &MoveParams) -> f32 {
        params.height_at(self.crouch)
    }

    /// Eye height above the feet now: where the camera sits and shots leave from.
    pub fn eye_height(&self, params: &MoveParams) -> f32 {
        params.eye_height_at(self.crouch)
    }

    pub fn lunging(&self) -> bool {
        self.lunge_time > 0.0
    }
}

/// Tuning values. Distances are metres, times are seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveParams {
    pub walk_speed: f32,
    pub sprint_speed: f32,
    pub crouch_speed: f32,
    /// Fraction of `wish.y` that must point forward to keep sprinting.
    pub sprint_min_forward: f32,
    pub ground_accel: f32,
    pub air_accel: f32,
    /// Cap on the wish speed used for air acceleration (limits air strafing).
    pub air_wish_speed: f32,
    pub friction: f32,
    /// Below this speed friction acts as if moving at this speed, so the player stops crisply.
    pub stop_speed: f32,
    pub gravity: f32,
    pub jump_velocity: f32,
    /// A jump pressed up to this long before landing still happens on landing.
    pub jump_buffer_time: f32,
    /// A jump pressed up to this long after walking off a ledge still happens.
    pub coyote_time: f32,
    /// A slide starts at this multiple of the sprint speed (with the
    /// weapon's speed scale), whatever the speed before it.
    pub slide_speed_ratio: f32,
    /// Sprint speed fraction required to start a slide.
    pub slide_min_speed_ratio: f32,
    /// Seconds of unbroken sprint needed before a crouch press slides.
    pub slide_min_sprint: f32,
    pub slide_duration: f32,
    /// Seconds at the start of a slide before friction takes hold.
    pub slide_hold: f32,
    pub slide_friction: f32,
    /// Seconds on the ground after a slide before the next one.
    pub slide_cooldown: f32,
    pub stand_height: f32,
    pub crouch_height: f32,
    /// Seconds to go from standing to crouched, and back up.
    pub crouch_down_time: f32,
    pub stand_up_time: f32,
    /// Seconds to drop into a slide (faster than a crouch).
    pub slide_down_time: f32,
    /// Distance from the top of the capsule down to the eyes.
    pub eye_from_top: f32,
    /// Ledges from this high to `mantle_max` (above the feet) can be climbed.
    pub mantle_min: f32,
    pub mantle_max: f32,
    /// Seconds a mantle takes, plus this much per metre of height.
    pub mantle_base_time: f32,
    pub mantle_time_per_metre: f32,
    /// Forward speed while climbing, which carries the player onto the top.
    pub mantle_forward_speed: f32,
}

impl Default for MoveParams {
    fn default() -> Self {
        Self {
            walk_speed: 5.0,
            sprint_speed: 7.4,
            crouch_speed: 2.6,
            sprint_min_forward: 0.5,
            ground_accel: 15.0,
            air_accel: 2.0,
            air_wish_speed: 1.2,
            friction: 10.0,
            stop_speed: 4.0,
            gravity: 19.0,
            jump_velocity: 6.2,
            jump_buffer_time: 0.1,
            coyote_time: 0.1,
            slide_speed_ratio: 1.1,
            slide_min_speed_ratio: 0.85,
            slide_min_sprint: 0.3,
            slide_duration: 0.65,
            slide_hold: 0.15,
            slide_friction: 1.4,
            slide_cooldown: 0.5,
            stand_height: 1.8,
            crouch_height: 1.1,
            crouch_down_time: 0.25,
            stand_up_time: 0.3,
            slide_down_time: 0.15,
            eye_from_top: 0.12,
            mantle_min: 0.5,
            mantle_max: 1.35,
            mantle_base_time: 0.22,
            mantle_time_per_metre: 0.2,
            mantle_forward_speed: 2.6,
        }
    }
}

impl MoveParams {
    /// Capsule height for a stance.
    pub fn height(&self, stance: Stance) -> f32 {
        match stance {
            Stance::Standing => self.stand_height,
            Stance::Crouching | Stance::Sliding => self.crouch_height,
        }
    }

    /// Eye height above the feet for a stance, fully in it.
    pub fn eye_height(&self, stance: Stance) -> f32 {
        self.height(stance) - self.eye_from_top
    }

    /// Capsule height partway down: `crouch` 0 standing to 1 crouched.
    pub fn height_at(&self, crouch: f32) -> f32 {
        let t = crouch_ease(crouch);
        self.stand_height + (self.crouch_height - self.stand_height) * t
    }

    /// Eye height partway down: `crouch` 0 standing to 1 crouched.
    pub fn eye_height_at(&self, crouch: f32) -> f32 {
        self.height_at(crouch) - self.eye_from_top
    }
}

/// The shape of a stance change over its crouch blend: starts and ends
/// gently (smoothstep). The capsule, the eye, the hit zones' and the drawn
/// body's crouch all follow this one curve.
pub fn crouch_ease(crouch: f32) -> f32 {
    let t = crouch.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Unit vectors for a yaw: (forward, right), on the horizontal plane.
pub fn basis_from_yaw(yaw: f32) -> (Vec3, Vec3) {
    let (s, c) = crate::fmath::sin_cos(yaw);
    (Vec3::new(-s, 0.0, -c), Vec3::new(c, 0.0, -s))
}

/// Advances movement by `dt` seconds and returns the velocity the engine should move with.
pub fn step(
    state: &mut MoveState,
    input: &MoveInput,
    env: MoveEnv,
    params: &MoveParams,
    dt: f32,
) -> Vec3 {
    let velocity = step_rules(state, input, env, params, dt);
    step_crouch(state, env, params, dt);
    if !state.sprinting {
        state.sprint_time = 0.0;
    } else if env.on_floor {
        state.sprint_time += dt;
    }
    velocity
}

/// Moves the crouch blend toward the stance. Standing up waits while there
/// is no headroom (the stance stays standing, so it finishes once clear).
fn step_crouch(state: &mut MoveState, env: MoveEnv, params: &MoveParams, dt: f32) {
    let target = if state.stance == Stance::Standing {
        0.0
    } else {
        1.0
    };
    if target > state.crouch {
        let time = if state.stance == Stance::Sliding {
            params.slide_down_time
        } else {
            params.crouch_down_time
        };
        state.crouch = (state.crouch + dt / time.max(1e-3)).min(1.0);
    } else if target < state.crouch && (env.can_stand || state.mantling()) {
        state.crouch = (state.crouch - dt / params.stand_up_time.max(1e-3)).max(0.0);
    }
}

fn step_rules(
    state: &mut MoveState,
    input: &MoveInput,
    env: MoveEnv,
    params: &MoveParams,
    dt: f32,
) -> Vec3 {
    let jump_pressed = input.jump && !state.jump_held;
    let crouch_pressed = input.crouch && !state.crouch_held;
    let sprint_pressed = input.sprint && !state.sprint_held;
    state.jump_held = input.jump;
    state.crouch_held = input.crouch;
    state.sprint_held = input.sprint;
    // The cooldown runs only on the ground, so a jump cannot wait it out.
    if env.on_floor {
        state.slide_cooldown = (state.slide_cooldown - dt).max(0.0);
    }
    state.jump_buffer = if jump_pressed {
        params.jump_buffer_time
    } else {
        (state.jump_buffer - dt).max(0.0)
    };
    state.coyote = if env.on_floor {
        params.coyote_time
    } else {
        (state.coyote - dt).max(0.0)
    };

    let wish = input.wish.clamp_length_max(1.0);
    let (forward, right) = basis_from_yaw(input.yaw);
    let wish_dir = (forward * wish.y + right * wish.x).normalize_or_zero();
    let horizontal_speed = horizontal(state.velocity).length();

    if state.mantling() {
        return mantle(state, params, dt);
    }

    if let Some(offset) = input.lunge {
        start_lunge(state, offset, params);
    }
    if state.lunging() {
        return lunge(state, env, params, dt);
    }
    if state.lunge_velocity != Vec2::ZERO {
        // The lunge ended last tick: drop to a walk at the target instead of
        // sliding on through it.
        state.lunge_velocity = Vec2::ZERO;
        let carried = horizontal(state.velocity).clamp_length_max(crate::melee::LUNGE_CARRY_SPEED);
        state.velocity = Vec3::new(carried.x, state.velocity.y, carried.z);
    }

    tick_slide(state, params, dt);
    update_stance(
        state,
        input,
        env,
        params,
        wish,
        crouch_pressed,
        sprint_pressed,
        horizontal_speed,
        forward,
    );

    // Jump cancels a crouch when the standing capsule has headroom. Keep the
    // jump buffer intact so the same press can also jump (or mantle).
    if state.jump_buffer > 0.0 && state.stance == Stance::Crouching && env.can_stand {
        state.stance = Stance::Standing;
    }

    // Jump during a slide is a slide-cancel: stand up with the slide's speed
    // and sprint on, without leaping. The buffer survives the mantle check, so
    // a slide into a ledge still climbs it.
    let slide_cancelled =
        state.jump_buffer > 0.0 && state.stance == Stance::Sliding && env.can_stand;
    if slide_cancelled {
        cancel_slide(state, input, params, wish);
    }

    // Jumping at a ledge in reach while moving toward it climbs over it.
    if state.jump_buffer > 0.0
        && wish.y > 0.3
        && state.stance != Stance::Crouching
        && let Some(height) = env.ledge
        && (params.mantle_min..=params.mantle_max).contains(&height)
    {
        state.jump_buffer = 0.0;
        state.coyote = 0.0;
        state.sprinting = false;
        state.stance = Stance::Standing;
        let time = params.mantle_base_time + params.mantle_time_per_metre * height;
        state.mantle_time = time;
        // A little over the ledge, so the feet clear its edge.
        state.mantle_rise = (height + 0.15) / time;
        state.mantle_dir = Vec2::new(forward.x, forward.z);
        return mantle(state, params, dt);
    }

    if slide_cancelled {
        // The press went to the cancel; another press jumps.
        state.jump_buffer = 0.0;
    }

    let mut jumped = false;
    let grounded = env.on_floor || state.coyote > 0.0;
    // Only a standing player jumps: a crouch stands up first (above), and a
    // slide under a low ceiling holds the press until there is headroom.
    if state.jump_buffer > 0.0 && grounded && state.stance == Stance::Standing {
        // Consume both windows so one press gives one jump.
        state.jump_buffer = 0.0;
        state.coyote = 0.0;
        state.velocity.y = params.jump_velocity;
        jumped = true;
    }

    if env.on_floor && !jumped {
        state.velocity.y = 0.0;
        let friction = if state.stance != Stance::Sliding {
            params.friction
        } else if state.slide_time > params.slide_duration - params.slide_hold {
            // The slide holds its entry speed for a moment before it bleeds off.
            0.0
        } else {
            params.slide_friction
        };
        apply_friction(&mut state.velocity, friction, params.stop_speed, dt);
        if state.stance != Stance::Sliding {
            let wish_speed = wish.length() * max_speed(state, params) * input.speed_multiplier;
            // Scale acceleration by at least `stop_speed`, so slow stances (crouch,
            // aiming) still reach their speed against the stronger low-speed friction.
            accelerate(
                &mut state.velocity,
                wish_dir,
                wish_speed,
                params.ground_accel * (params.stop_speed / wish_speed.max(1e-3)).max(1.0),
                dt,
            );
        }
    } else {
        let wish_speed = (wish.length() * max_speed(state, params) * input.speed_multiplier)
            .min(params.air_wish_speed);
        accelerate(
            &mut state.velocity,
            wish_dir,
            wish_speed,
            params.air_accel,
            dt,
        );
        state.velocity.y -= params.gravity * dt;
    }

    state.velocity
}

#[allow(clippy::too_many_arguments)]
fn update_stance(
    state: &mut MoveState,
    input: &MoveInput,
    env: MoveEnv,
    params: &MoveParams,
    wish: Vec2,
    crouch_pressed: bool,
    sprint_pressed: bool,
    horizontal_speed: f32,
    forward: Vec3,
) {
    let moving_forward = wish.y >= params.sprint_min_forward;
    // Toggle sprint, as in classic Call of Duty: a press starts it; it ends when
    // forward input stops, the player crouches, or a weapon action blocks it.
    let start_sprint = sprint_pressed && moving_forward && !input.block_sprint;
    let keep_sprint = state.sprinting && moving_forward && !input.block_sprint;

    match state.stance {
        Stance::Standing => {
            // Weapon weight scales the slide like the sprint it comes from.
            let sprint_speed = params.sprint_speed * input.speed_multiplier;
            let fast_enough = horizontal_speed >= sprint_speed * params.slide_min_speed_ratio;
            if crouch_pressed
                && env.on_floor
                && state.sprinting
                && state.sprint_time >= params.slide_min_sprint
                && fast_enough
                && state.slide_cooldown <= 0.0
            {
                state.stance = Stance::Sliding;
                state.slide_time = params.slide_duration;
                state.sprinting = false;
                let dir = horizontal(state.velocity)
                    .try_normalize()
                    .unwrap_or(forward);
                // Always the same entry speed: speed carried in from an
                // earlier slide never stacks on top of it.
                let speed = sprint_speed * params.slide_speed_ratio;
                state.velocity = Vec3::new(dir.x * speed, state.velocity.y, dir.z * speed);
            } else if crouch_pressed {
                state.stance = Stance::Crouching;
                state.sprinting = false;
            } else {
                state.sprinting = start_sprint || keep_sprint;
            }
        }
        Stance::Crouching => {
            // Crouch is a toggle too; sprinting out of a crouch stands the player up.
            if (crouch_pressed || start_sprint) && env.can_stand {
                state.stance = Stance::Standing;
                state.sprinting = start_sprint;
            }
        }
        Stance::Sliding => {
            // Crouching again also cancels the slide (as does jump, in `step`).
            if crouch_pressed && env.can_stand {
                cancel_slide(state, input, params, wish);
            }
        }
    }
}

/// Slide-cancel: stand up out of the slide, sprinting on if still pushing
/// forward, and start the slide cooldown. The slide's speed carries over up
/// to a sprint, never above it, so chaining cancels cannot outrun a sprint.
fn cancel_slide(state: &mut MoveState, input: &MoveInput, params: &MoveParams, wish: Vec2) {
    state.stance = Stance::Standing;
    state.slide_time = 0.0;
    state.slide_cooldown = params.slide_cooldown;
    state.sprinting = wish.y >= params.sprint_min_forward && !input.block_sprint;
    state.sprint_time = 0.0;
    let cap = params.sprint_speed * input.speed_multiplier;
    let flat = horizontal(state.velocity).clamp_length_max(cap);
    state.velocity = Vec3::new(flat.x, state.velocity.y, flat.z);
}

/// One tick of a mantle: rise at a steady speed while pushing forward, with
/// no gravity. The engine's collision carries the body up the wall and onto
/// the top.
fn mantle(state: &mut MoveState, params: &MoveParams, dt: f32) -> Vec3 {
    state.mantle_time = (state.mantle_time - dt).max(0.0);
    let rise = if state.mantle_time > 0.0 {
        state.mantle_rise
    } else {
        0.0
    };
    let forward = Vec3::new(state.mantle_dir.x, 0.0, state.mantle_dir.y);
    state.velocity = forward * params.mantle_forward_speed + Vec3::Y * rise;
    state.velocity
}

/// Throws the player at a melee target `offset` away (see [`crate::melee`]).
/// A lunge ends a sprint and a slide (in a crouch).
fn start_lunge(state: &mut MoveState, offset: Vec2, params: &MoveParams) {
    let travel = crate::melee::lunge_travel(offset);
    if travel <= 0.01 {
        return;
    }
    let speed = crate::melee::LUNGE_SPEED;
    state.lunge_velocity = offset.normalize() * speed;
    state.lunge_time = travel / speed;
    state.sprinting = false;
    state.jump_buffer = 0.0;
    if state.stance == Stance::Sliding {
        state.stance = Stance::Crouching;
        state.slide_time = 0.0;
        state.slide_cooldown = params.slide_cooldown;
    }
}

/// One tick of a lunge: straight at the target at the lunge speed, with
/// gravity as usual. The last tick covers only what is left of the
/// distance, so every lunge travels exactly as far as it set out to.
fn lunge(state: &mut MoveState, env: MoveEnv, params: &MoveParams, dt: f32) -> Vec3 {
    let step = state.lunge_time.min(dt);
    state.lunge_time = (state.lunge_time - dt).max(0.0);
    let flat = state.lunge_velocity * (step / dt);
    let vertical = if env.on_floor {
        0.0
    } else {
        state.velocity.y - params.gravity * dt
    };
    state.velocity = Vec3::new(flat.x, vertical, flat.y);
    state.velocity
}

/// Advances the slide timer and ends the slide, in a crouch, when it runs out or slows down.
/// Kept separate from `update_stance` so the timer ticks exactly once per step.
fn tick_slide(state: &mut MoveState, params: &MoveParams, dt: f32) {
    if state.stance != Stance::Sliding {
        return;
    }
    state.slide_time -= dt;
    let slowed = horizontal(state.velocity).length() <= params.crouch_speed;
    if state.slide_time <= 0.0 || slowed {
        state.slide_cooldown = params.slide_cooldown;
        state.stance = Stance::Crouching;
    }
}

/// The speed cap. Going down, it eases from the standing gait's to the crouch
/// speed with the body. Standing up frees it at once, and acceleration
/// carries the player back up to speed (a slide-cancel keeps its sprint).
fn max_speed(state: &MoveState, params: &MoveParams) -> f32 {
    let standing = if state.sprinting {
        params.sprint_speed
    } else {
        params.walk_speed
    };
    if state.stance == Stance::Standing {
        return standing;
    }
    let t = crouch_ease(state.crouch);
    standing + (params.crouch_speed - standing) * t
}

fn horizontal(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

/// Quake-style acceleration: add speed along `wish_dir` up to `wish_speed`.
fn accelerate(velocity: &mut Vec3, wish_dir: Vec3, wish_speed: f32, accel: f32, dt: f32) {
    let current = velocity.dot(wish_dir);
    let add = wish_speed - current;
    if add <= 0.0 {
        return;
    }
    let gain = (accel * wish_speed * dt).min(add);
    *velocity += wish_dir * gain;
}

/// Ground friction on the horizontal component only.
fn apply_friction(velocity: &mut Vec3, friction: f32, stop_speed: f32, dt: f32) {
    let h = horizontal(*velocity);
    let speed = h.length();
    if speed < 1e-3 {
        velocity.x = 0.0;
        velocity.z = 0.0;
        return;
    }
    let drop = speed.max(stop_speed) * friction * dt;
    let scale = (speed - drop).max(0.0) / speed;
    velocity.x *= scale;
    velocity.z *= scale;
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn run(state: &mut MoveState, input: MoveInput, env: MoveEnv, ticks: usize) {
        let params = MoveParams::default();
        for _ in 0..ticks {
            step(state, &input, env, &params, DT);
        }
    }

    fn speed(state: &MoveState) -> f32 {
        horizontal(state.velocity).length()
    }

    fn forward() -> MoveInput {
        MoveInput {
            wish: Vec2::Y,
            ..Default::default()
        }
    }

    #[test]
    fn a_lunge_covers_its_distance_at_speed_then_stops_at_the_target() {
        let params = MoveParams::default();
        let mut s = MoveState {
            sprinting: true,
            ..Default::default()
        };
        // A target 3 m ahead and a little to the right (yaw 0 looks down -Z).
        let offset = Vec2::new(0.4, -3.0);
        let mut position = Vec3::ZERO;
        let mut input = MoveInput {
            lunge: Some(offset),
            ..Default::default()
        };
        let mut ticks = 0;
        loop {
            let v = step(&mut s, &input, MoveEnv::default(), &params, DT);
            input.lunge = None;
            position += v * DT;
            ticks += 1;
            if !s.lunging() {
                break;
            }
            assert!(!s.sprinting, "a lunge ends the sprint");
        }
        let travelled = Vec2::new(position.x, position.z);
        let expected = crate::melee::lunge_travel(offset);
        assert!((travelled.length() - expected).abs() < 1e-3, "{travelled}");
        assert!(
            travelled.normalize().dot(offset.normalize()) > 0.999,
            "straight at it"
        );
        assert!(ticks <= 12, "quick: {ticks} ticks");
        // The next tick takes the speed off.
        step(&mut s, &input, MoveEnv::default(), &params, DT);
        assert!(
            speed(&s) <= crate::melee::LUNGE_CARRY_SPEED + 1e-4,
            "{}",
            speed(&s)
        );
    }

    #[test]
    fn a_lunge_at_a_target_in_reach_does_not_move() {
        let mut s = MoveState::default();
        let input = MoveInput {
            lunge: Some(Vec2::new(0.0, -0.6)),
            ..Default::default()
        };
        run(&mut s, input, MoveEnv::default(), 1);
        assert!(!s.lunging());
        assert_eq!(speed(&s), 0.0);
    }

    #[test]
    fn walking_reaches_and_holds_walk_speed() {
        let mut s = MoveState::default();
        run(&mut s, forward(), MoveEnv::default(), 120);
        let p = MoveParams::default();
        assert!(
            (speed(&s) - p.walk_speed).abs() < 0.3,
            "speed {}",
            speed(&s)
        );
        // Yaw 0 moves toward -Z.
        assert!(s.velocity.z < 0.0);
    }

    #[test]
    fn friction_stops_the_player() {
        let mut s = MoveState::default();
        run(&mut s, forward(), MoveEnv::default(), 120);
        run(&mut s, MoveInput::default(), MoveEnv::default(), 30);
        assert_eq!(speed(&s), 0.0);
    }

    #[test]
    fn sprint_is_faster_and_needs_forward_input() {
        let p = MoveParams::default();
        let mut s = MoveState::default();
        run(
            &mut s,
            MoveInput {
                sprint: true,
                ..forward()
            },
            MoveEnv::default(),
            180,
        );
        assert!(s.sprinting);
        assert!(speed(&s) > p.walk_speed + 1.0);

        let mut back = MoveState::default();
        run(
            &mut back,
            MoveInput {
                wish: Vec2::NEG_Y,
                sprint: true,
                ..Default::default()
            },
            MoveEnv::default(),
            60,
        );
        assert!(!back.sprinting);
    }

    #[test]
    fn crouch_from_sprint_slides_then_decays() {
        let p = MoveParams::default();
        let mut s = MoveState::default();
        let sprint = MoveInput {
            sprint: true,
            ..forward()
        };
        run(&mut s, sprint, MoveEnv::default(), 180);
        run(
            &mut s,
            MoveInput {
                crouch: true,
                ..sprint
            },
            MoveEnv::default(),
            1,
        );
        assert_eq!(s.stance, Stance::Sliding);
        let entry = p.sprint_speed * p.slide_speed_ratio;
        assert!((speed(&s) - entry).abs() < 0.05, "{}", speed(&s));

        run(
            &mut s,
            MoveInput {
                crouch: true,
                ..forward()
            },
            MoveEnv::default(),
            120,
        );
        assert_eq!(s.stance, Stance::Crouching);
        assert!(speed(&s) <= p.crouch_speed + 0.2);
    }

    #[test]
    fn crouch_while_walking_does_not_slide() {
        let mut s = MoveState::default();
        run(&mut s, forward(), MoveEnv::default(), 120);
        run(
            &mut s,
            MoveInput {
                crouch: true,
                ..forward()
            },
            MoveEnv::default(),
            1,
        );
        assert_eq!(s.stance, Stance::Crouching);
    }

    #[test]
    fn crouch_toggles_and_respects_headroom() {
        let crouch = MoveInput {
            crouch: true,
            ..Default::default()
        };
        let mut s = MoveState::default();
        run(&mut s, crouch, MoveEnv::default(), 1);
        run(&mut s, MoveInput::default(), MoveEnv::default(), 10);
        assert_eq!(s.stance, Stance::Crouching, "crouch stays on after release");

        let low_ceiling = MoveEnv {
            on_floor: true,
            can_stand: false,
            ledge: None,
        };
        run(&mut s, crouch, low_ceiling, 1);
        run(&mut s, MoveInput::default(), low_ceiling, 1);
        assert_eq!(
            s.stance,
            Stance::Crouching,
            "cannot stand under a low ceiling"
        );

        run(&mut s, crouch, MoveEnv::default(), 1);
        assert_eq!(s.stance, Stance::Standing);
    }

    #[test]
    fn sprint_toggle_persists_until_forward_stops() {
        let mut s = MoveState::default();
        run(
            &mut s,
            MoveInput {
                sprint: true,
                ..forward()
            },
            MoveEnv::default(),
            1,
        );
        run(&mut s, forward(), MoveEnv::default(), 60);
        assert!(s.sprinting, "sprint stays on after releasing the key");

        run(&mut s, MoveInput::default(), MoveEnv::default(), 1);
        assert!(!s.sprinting, "letting go of forward ends the sprint");
        run(&mut s, forward(), MoveEnv::default(), 10);
        assert!(!s.sprinting, "sprint does not resume without a new press");
    }

    #[test]
    fn jumping_from_crouch_stands_and_jumps_when_clear() {
        let mut state = MoveState {
            stance: Stance::Crouching,
            ..MoveState::default()
        };
        run(
            &mut state,
            MoveInput {
                jump: true,
                ..MoveInput::default()
            },
            MoveEnv::default(),
            1,
        );
        assert_eq!(state.stance, Stance::Standing);
        assert!(state.velocity.y > 0.0);
    }

    #[test]
    fn crouch_jump_respects_a_low_ceiling_and_buffers_until_clear() {
        let mut state = MoveState {
            stance: Stance::Crouching,
            ..MoveState::default()
        };
        let input = MoveInput {
            jump: true,
            ..MoveInput::default()
        };
        run(
            &mut state,
            input,
            MoveEnv {
                can_stand: false,
                ..MoveEnv::default()
            },
            1,
        );
        assert_eq!(state.stance, Stance::Crouching);
        assert_eq!(state.velocity.y, 0.0);
        run(&mut state, input, MoveEnv::default(), 1);
        assert_eq!(state.stance, Stance::Standing);
        assert!(state.velocity.y > 0.0);
    }

    #[test]
    fn weapon_actions_block_sprint() {
        let mut s = MoveState::default();
        run(
            &mut s,
            MoveInput {
                sprint: true,
                ..forward()
            },
            MoveEnv::default(),
            10,
        );
        assert!(s.sprinting);
        run(
            &mut s,
            MoveInput {
                block_sprint: true,
                ..forward()
            },
            MoveEnv::default(),
            1,
        );
        assert!(!s.sprinting);

        run(
            &mut s,
            MoveInput {
                sprint: true,
                block_sprint: true,
                ..forward()
            },
            MoveEnv::default(),
            1,
        );
        assert!(!s.sprinting, "cannot start sprinting while blocked");
    }

    #[test]
    fn speed_multiplier_scales_ground_speed() {
        let p = MoveParams::default();
        let mut s = MoveState::default();
        run(
            &mut s,
            MoveInput {
                speed_multiplier: 0.5,
                ..forward()
            },
            MoveEnv::default(),
            120,
        );
        assert!(
            (speed(&s) - p.walk_speed * 0.5).abs() < 0.2,
            "speed {}",
            speed(&s)
        );
    }

    #[test]
    fn jump_requires_a_fresh_press() {
        let p = MoveParams::default();
        let mut s = MoveState::default();
        let params = p;
        let jump = MoveInput {
            jump: true,
            ..Default::default()
        };
        step(&mut s, &jump, MoveEnv::default(), &params, DT);
        assert!(s.velocity.y > 0.0);

        // Landed while still holding jump: no automatic bunny hop.
        s.velocity.y = 0.0;
        step(&mut s, &jump, MoveEnv::default(), &params, DT);
        assert_eq!(s.velocity.y, 0.0);
    }

    const AIR: MoveEnv = MoveEnv {
        on_floor: false,
        can_stand: true,
        ledge: None,
    };

    fn jump() -> MoveInput {
        MoveInput {
            jump: true,
            ..Default::default()
        }
    }

    #[test]
    fn jump_pressed_just_before_landing_is_buffered() {
        let p = MoveParams::default();
        let mut s = MoveState {
            velocity: Vec3::new(0.0, -3.0, 0.0),
            ..Default::default()
        };
        // Pressed 50 ms before touching down, then released.
        step(&mut s, &jump(), AIR, &p, crate::TICK_DT);
        for _ in 0..5 {
            step(&mut s, &MoveInput::default(), AIR, &p, crate::TICK_DT);
        }
        step(
            &mut s,
            &MoveInput::default(),
            MoveEnv::default(),
            &p,
            crate::TICK_DT,
        );
        assert!(s.velocity.y > p.jump_velocity * 0.9, "jumps on landing");
    }

    #[test]
    fn jump_pressed_too_early_is_dropped() {
        let p = MoveParams::default();
        let mut s = MoveState::default();
        step(&mut s, &jump(), AIR, &p, crate::TICK_DT);
        for _ in 0..30 {
            step(&mut s, &MoveInput::default(), AIR, &p, crate::TICK_DT);
        }
        step(
            &mut s,
            &MoveInput::default(),
            MoveEnv::default(),
            &p,
            crate::TICK_DT,
        );
        assert_eq!(s.velocity.y, 0.0);
    }

    #[test]
    fn coyote_time_allows_one_late_jump() {
        let p = MoveParams::default();
        let mut s = MoveState::default();
        step(
            &mut s,
            &MoveInput::default(),
            MoveEnv::default(),
            &p,
            crate::TICK_DT,
        );
        // Walked off a ledge 50 ms ago.
        for _ in 0..6 {
            step(&mut s, &MoveInput::default(), AIR, &p, crate::TICK_DT);
        }
        step(&mut s, &jump(), AIR, &p, crate::TICK_DT);
        assert!(s.velocity.y > p.jump_velocity * 0.9, "late jump works");

        // A second press in the air does nothing.
        step(&mut s, &MoveInput::default(), AIR, &p, crate::TICK_DT);
        let before = s.velocity.y;
        step(&mut s, &jump(), AIR, &p, crate::TICK_DT);
        assert!(s.velocity.y < before, "no double jump");
    }

    #[test]
    fn coyote_time_runs_out() {
        let p = MoveParams::default();
        let mut s = MoveState::default();
        step(
            &mut s,
            &MoveInput::default(),
            MoveEnv::default(),
            &p,
            crate::TICK_DT,
        );
        for _ in 0..30 {
            step(&mut s, &MoveInput::default(), AIR, &p, crate::TICK_DT);
        }
        let before = s.velocity.y;
        step(&mut s, &jump(), AIR, &p, crate::TICK_DT);
        assert!(s.velocity.y < before);
    }

    #[test]
    fn stops_crisply_at_the_real_tick_rate() {
        let p = MoveParams::default();
        let mut s = MoveState::default();
        let ticks = |seconds: f32| (seconds * crate::TICK_RATE as f32) as usize;
        for _ in 0..ticks(1.0) {
            step(&mut s, &forward(), MoveEnv::default(), &p, crate::TICK_DT);
        }
        assert!((speed(&s) - p.walk_speed).abs() < 0.05, "full speed");
        for _ in 0..ticks(0.2) {
            step(
                &mut s,
                &MoveInput::default(),
                MoveEnv::default(),
                &p,
                crate::TICK_DT,
            );
        }
        assert_eq!(speed(&s), 0.0, "stopped from a walk within 200 ms");

        // Reaches full walking speed within 150 ms from rest.
        for _ in 0..ticks(0.15) {
            step(&mut s, &forward(), MoveEnv::default(), &p, crate::TICK_DT);
        }
        assert!(
            (speed(&s) - p.walk_speed).abs() < 0.05,
            "speed {}",
            speed(&s)
        );
    }

    #[test]
    fn air_control_is_limited() {
        let p = MoveParams::default();
        let mut s = MoveState::default();
        let air = MoveEnv {
            on_floor: false,
            can_stand: true,
            ledge: None,
        };
        run(
            &mut s,
            MoveInput {
                wish: Vec2::X,
                ..Default::default()
            },
            air,
            30,
        );
        assert!(speed(&s) <= p.air_wish_speed + 1e-3);
        assert!(s.velocity.y < 0.0, "gravity applies in the air");
    }

    #[test]
    fn step_is_deterministic() {
        let params = MoveParams::default();
        let inputs = [
            forward(),
            MoveInput {
                sprint: true,
                ..forward()
            },
            MoveInput {
                crouch: true,
                sprint: true,
                ..forward()
            },
        ];
        let simulate = || {
            let mut s = MoveState::default();
            for i in 0..300 {
                step(
                    &mut s,
                    &inputs[(i / 60) % inputs.len()],
                    MoveEnv::default(),
                    &params,
                    DT,
                );
            }
            s
        };
        assert_eq!(simulate(), simulate());
    }

    #[test]
    fn jumping_at_a_ledge_climbs_it() {
        let p = MoveParams::default();
        let mut s = MoveState::default();
        let env = MoveEnv {
            ledge: Some(1.1),
            ..Default::default()
        };
        let input = MoveInput {
            wish: Vec2::Y,
            jump: true,
            ..Default::default()
        };
        let v = step(&mut s, &input, env, &p, DT);
        assert!(s.mantling());
        assert!(v.y > 2.0 && v.z < -1.0, "{v}");
        // It rises about the ledge's height over the climb.
        let mut risen = v.y * DT;
        let held = MoveInput {
            wish: Vec2::Y,
            ..Default::default()
        };
        while s.mantling() {
            risen += step(&mut s, &held, MoveEnv::default(), &p, DT).y * DT;
        }
        assert!((risen - 1.25).abs() < 0.15, "{risen}");
    }

    #[test]
    fn ledges_out_of_reach_or_standing_still_just_jump() {
        let p = MoveParams::default();
        for (ledge, wish) in [
            (Some(2.0), Vec2::Y),
            (Some(1.0), Vec2::ZERO),
            (None, Vec2::Y),
        ] {
            let mut s = MoveState::default();
            let env = MoveEnv {
                ledge,
                ..Default::default()
            };
            let input = MoveInput {
                wish,
                jump: true,
                ..Default::default()
            };
            step(&mut s, &input, env, &p, DT);
            assert!(!s.mantling(), "{ledge:?} {wish}");
            assert!(s.velocity.y > 5.0);
        }
    }

    #[test]
    fn crouching_again_cancels_a_slide() {
        let mut s = MoveState::default();
        let sprint = MoveInput {
            wish: Vec2::Y,
            sprint: true,
            ..Default::default()
        };
        run(&mut s, sprint, MoveEnv::default(), 2);
        run(&mut s, forward(), MoveEnv::default(), 90);
        let crouch = MoveInput {
            wish: Vec2::Y,
            crouch: true,
            ..Default::default()
        };
        run(&mut s, crouch, MoveEnv::default(), 1);
        assert_eq!(s.stance, Stance::Sliding);
        run(&mut s, forward(), MoveEnv::default(), 6);
        let fast = speed(&s);
        run(&mut s, crouch, MoveEnv::default(), 1);
        assert_eq!(s.stance, Stance::Standing);
        assert!(s.sprinting, "a slide-cancel sprints on");
        assert!(speed(&s) > fast * 0.9, "the slide's speed carries over");
        let p = MoveParams::default();
        assert!(
            speed(&s) <= p.sprint_speed + 1e-4,
            "but only up to a sprint"
        );
        assert!(s.slide_cooldown > 0.0);
    }

    /// Sprints, then crouches into a slide and slides for a few ticks.
    fn start_slide(s: &mut MoveState) {
        let sprint = MoveInput {
            wish: Vec2::Y,
            sprint: true,
            ..Default::default()
        };
        run(s, sprint, MoveEnv::default(), 2);
        run(s, forward(), MoveEnv::default(), 90);
        let crouch = MoveInput {
            wish: Vec2::Y,
            crouch: true,
            ..Default::default()
        };
        run(s, crouch, MoveEnv::default(), 1);
        run(s, forward(), MoveEnv::default(), 6);
        assert_eq!(s.stance, Stance::Sliding);
    }

    #[test]
    fn jumping_cancels_a_slide_without_leaping() {
        let mut s = MoveState::default();
        start_slide(&mut s);
        let fast = speed(&s);
        let jump = MoveInput {
            wish: Vec2::Y,
            jump: true,
            ..Default::default()
        };
        run(&mut s, jump, MoveEnv::default(), 1);
        assert_eq!(s.stance, Stance::Standing);
        assert!(s.sprinting, "a slide-cancel sprints on");
        assert!(speed(&s) > fast * 0.9, "the slide's speed carries over");
        assert!(s.slide_cooldown > 0.0);
        assert_eq!(s.velocity.y, 0.0, "the cancel is not a jump");
        assert_eq!(s.jump_buffer, 0.0, "the press is spent on the cancel");

        // Holding the button does not jump; a fresh press does.
        run(&mut s, jump, MoveEnv::default(), 10);
        assert_eq!(s.velocity.y, 0.0);
        assert!(s.sprinting);
        run(&mut s, forward(), MoveEnv::default(), 1);
        run(&mut s, jump, MoveEnv::default(), 1);
        assert!(s.velocity.y > 5.0);
    }

    #[test]
    fn jumping_in_a_slide_under_a_low_ceiling_waits_for_headroom() {
        let mut s = MoveState::default();
        start_slide(&mut s);
        let low_ceiling = MoveEnv {
            on_floor: true,
            can_stand: false,
            ledge: None,
        };
        let jump = MoveInput {
            wish: Vec2::Y,
            jump: true,
            ..Default::default()
        };
        run(&mut s, jump, low_ceiling, 1);
        assert_eq!(s.stance, Stance::Sliding, "no standing up into the ceiling");
        assert_eq!(s.velocity.y, 0.0);
        // Clear of the ceiling within the jump buffer, the press cancels the slide.
        run(&mut s, jump, MoveEnv::default(), 1);
        assert_eq!(s.stance, Stance::Standing);
        assert_eq!(s.velocity.y, 0.0);
    }

    #[test]
    fn jumping_in_a_slide_at_a_ledge_mantles() {
        let mut s = MoveState::default();
        start_slide(&mut s);
        let ledge = MoveEnv {
            ledge: Some(1.0),
            ..Default::default()
        };
        let jump = MoveInput {
            wish: Vec2::Y,
            jump: true,
            ..Default::default()
        };
        run(&mut s, jump, ledge, 1);
        assert!(s.mantling());
    }

    const TICK: f32 = crate::TICK_DT;

    fn ticks(seconds: f32) -> usize {
        (seconds * crate::TICK_RATE as f32).round() as usize
    }

    fn run_at(state: &mut MoveState, input: MoveInput, env: MoveEnv, seconds: f32) {
        let params = MoveParams::default();
        for _ in 0..ticks(seconds) {
            step(state, &input, env, &params, TICK);
        }
    }

    #[test]
    fn crouching_takes_time_and_the_eye_and_capsule_follow_one_blend() {
        let p = MoveParams::default();
        let mut s = MoveState::default();
        let crouch = MoveInput {
            crouch: true,
            ..Default::default()
        };
        step(&mut s, &crouch, MoveEnv::default(), &p, TICK);
        assert_eq!(s.stance, Stance::Crouching, "the stance flips at once");
        assert!(s.crouch < 0.1, "but the body is still up: {}", s.crouch);
        run_at(&mut s, MoveInput::default(), MoveEnv::default(), 0.1);
        let eye = s.eye_height(&p);
        assert!(
            eye < p.eye_height(Stance::Standing) - 0.05
                && eye > p.eye_height(Stance::Crouching) + 0.1,
            "partway down after 0.1 s: {eye}"
        );
        assert!((s.height(&p) - eye - p.eye_from_top).abs() < 1e-5);
        run_at(&mut s, MoveInput::default(), MoveEnv::default(), 0.16);
        assert_eq!(s.crouch, 1.0, "fully down in {} s", p.crouch_down_time);
        assert!((s.eye_height(&p) - p.eye_height(Stance::Crouching)).abs() < 1e-5);

        // Standing up takes a little longer.
        step(&mut s, &crouch, MoveEnv::default(), &p, TICK);
        assert_eq!(s.stance, Stance::Standing);
        run_at(&mut s, MoveInput::default(), MoveEnv::default(), 0.25);
        assert!(s.crouch > 0.0, "not up yet after 0.25 s");
        run_at(&mut s, MoveInput::default(), MoveEnv::default(), 0.06);
        assert_eq!(s.crouch, 0.0);
    }

    #[test]
    fn the_speed_cap_follows_the_crouch_blend() {
        let p = MoveParams::default();
        let mut s = MoveState::default();
        run_at(&mut s, forward(), MoveEnv::default(), 1.0);
        assert!((speed(&s) - p.walk_speed).abs() < 0.05);
        let crouch = MoveInput {
            crouch: true,
            ..forward()
        };
        step(&mut s, &crouch, MoveEnv::default(), &p, TICK);
        assert!(speed(&s) > p.walk_speed - 0.2, "no instant drop to a crawl");
        run_at(&mut s, forward(), MoveEnv::default(), 0.5);
        assert!((speed(&s) - p.crouch_speed).abs() < 0.05, "{}", speed(&s));
    }

    #[test]
    fn standing_up_waits_under_a_low_ceiling() {
        let p = MoveParams::default();
        let mut s = MoveState {
            stance: Stance::Crouching,
            crouch: 1.0,
            ..Default::default()
        };
        let low = MoveEnv {
            can_stand: false,
            ..Default::default()
        };
        // Already standing (it left the crouch under open sky), then walked
        // under a beam halfway up.
        s.stance = Stance::Standing;
        run_at(&mut s, MoveInput::default(), MoveEnv::default(), 0.1);
        let partway = s.crouch;
        assert!(partway > 0.0 && partway < 1.0);
        run_at(&mut s, MoveInput::default(), low, 0.5);
        assert_eq!(s.crouch, partway, "holds under the beam");
        run_at(&mut s, MoveInput::default(), MoveEnv::default(), 0.5);
        assert_eq!(s.crouch, 0.0, "finishes once clear");
        assert_eq!(s.eye_height(&p), p.eye_height(Stance::Standing));
    }

    #[test]
    fn a_slide_has_to_be_earned_with_a_short_sprint() {
        // Already at sprint speed (say, off a slide-cancel), but the sprint
        // only just started.
        let mut s = MoveState {
            velocity: Vec3::new(0.0, 0.0, -7.4),
            ..Default::default()
        };
        let sprint = MoveInput {
            sprint: true,
            ..forward()
        };
        run_at(&mut s, sprint, MoveEnv::default(), 0.1);
        let crouch = MoveInput {
            crouch: true,
            ..forward()
        };
        run_at(&mut s, crouch, MoveEnv::default(), TICK);
        assert_eq!(s.stance, Stance::Crouching, "too soon: a plain crouch");
    }

    #[test]
    fn weapon_weight_scales_the_slide() {
        let p = MoveParams::default();
        for scale in [0.88, 1.0, 1.05] {
            let mut s = MoveState::default();
            let sprint = MoveInput {
                sprint: true,
                speed_multiplier: scale,
                ..forward()
            };
            run_at(&mut s, sprint, MoveEnv::default(), 1.5);
            step(
                &mut s,
                &MoveInput {
                    crouch: true,
                    ..sprint
                },
                MoveEnv::default(),
                &p,
                TICK,
            );
            assert_eq!(
                s.stance,
                Stance::Sliding,
                "a heavy gun still slides: {scale}"
            );
            let entry = p.sprint_speed * scale * p.slide_speed_ratio;
            assert!((speed(&s) - entry).abs() < 0.05, "{scale}: {}", speed(&s));
        }
    }

    #[test]
    fn a_slide_covers_about_four_metres() {
        let p = MoveParams::default();
        let mut s = MoveState::default();
        let sprint = MoveInput {
            sprint: true,
            ..forward()
        };
        run_at(&mut s, sprint, MoveEnv::default(), 1.5);
        let crouch = MoveInput {
            crouch: true,
            ..forward()
        };
        let mut travelled = 0.0;
        let mut time = 0.0;
        let mut input = crouch;
        while time < 2.0 {
            step(&mut s, &input, MoveEnv::default(), &p, TICK);
            input = forward();
            if s.stance != Stance::Sliding {
                break;
            }
            travelled += speed(&s) * TICK;
            time += TICK;
        }
        assert!((0.55..=0.7).contains(&time), "lasts {time} s");
        assert!((3.4..=4.4).contains(&travelled), "covers {travelled} m");
    }

    /// Plays one slide-cancel strategy for ten seconds and returns the mean
    /// ground speed: sprint, slide as soon as allowed, cancel `cancel_after`
    /// into the slide (by crouch, or by jump and then a real jump), repeat.
    fn chain_slides(cancel_after: usize, by_jump: bool, hop: bool) -> f32 {
        let p = MoveParams::default();
        let mut s = MoveState::default();
        let mut position = Vec3::ZERO;
        let mut height = 0.0f32;
        let mut previous = MoveInput::default();
        let mut sliding_for = 0;
        let total = ticks(10.0);
        // Up to speed first.
        run_at(
            &mut s,
            MoveInput {
                sprint: true,
                ..forward()
            },
            MoveEnv::default(),
            1.0,
        );
        for i in 0..total {
            let on_floor = height <= 0.0;
            let env = MoveEnv {
                on_floor,
                ..Default::default()
            };
            let mut input = forward();
            // Mash every button on alternate ticks, so each is a fresh press.
            let fresh = |held: bool| !held;
            if s.stance == Stance::Sliding {
                sliding_for += 1;
                if sliding_for >= cancel_after {
                    if by_jump {
                        input.jump = fresh(previous.jump);
                    } else {
                        input.crouch = fresh(previous.crouch);
                    }
                }
            } else {
                sliding_for = 0;
                input.sprint = fresh(previous.sprint) && !s.sprinting;
                input.crouch = s.sprinting && fresh(previous.crouch);
                input.jump = hop && on_floor && fresh(previous.jump);
            }
            let _ = i;
            let v = step(&mut s, &input, env, &p, TICK);
            previous = input;
            position += Vec3::new(v.x, 0.0, v.z) * TICK;
            height = (height + v.y * TICK).max(0.0);
            if height <= 0.0 && s.velocity.y < 0.0 {
                s.velocity.y = 0.0;
            }
        }
        position.length() / (total as f32 * TICK)
    }

    #[test]
    fn chained_slide_cancels_never_outrun_a_sprint() {
        let p = MoveParams::default();
        let mut worst = (0.0, 0, false, false);
        for cancel_after in 1..ticks(0.7) {
            for by_jump in [false, true] {
                for hop in [false, true] {
                    let mean = chain_slides(cancel_after, by_jump, hop);
                    if mean > worst.0 {
                        worst = (mean, cancel_after, by_jump, hop);
                    }
                }
            }
        }
        assert!(
            worst.0 <= p.sprint_speed * 1.03,
            "the best chain averages {:.2} m/s ({:?}) against a {} m/s sprint",
            worst.0,
            worst,
            p.sprint_speed
        );
    }

    #[test]
    fn turning_mid_mantle_keeps_climbing_the_same_way() {
        let p = MoveParams::default();
        let mut s = MoveState::default();
        let env = MoveEnv {
            ledge: Some(1.0),
            ..Default::default()
        };
        let input = MoveInput {
            wish: Vec2::Y,
            jump: true,
            ..Default::default()
        };
        let first = step(&mut s, &input, env, &p, TICK);
        assert!(s.mantling());
        let turned = MoveInput {
            wish: Vec2::Y,
            yaw: 1.5,
            ..Default::default()
        };
        let v = step(&mut s, &turned, MoveEnv::default(), &p, TICK);
        assert!(s.mantling());
        assert_eq!(Vec2::new(v.x, v.z), Vec2::new(first.x, first.z));
    }
}
