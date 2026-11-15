//! First-person movement: walk, sprint, crouch, slide, and jump.
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
        }
    }
}

/// What the engine observed about the world this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveEnv {
    pub on_floor: bool,
    /// False when there is no headroom to stand up from a crouch.
    pub can_stand: bool,
}

impl Default for MoveEnv {
    fn default() -> Self {
        Self {
            on_floor: true,
            can_stand: true,
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
    /// Previous tick's buttons, for edge detection without engine help.
    pub jump_held: bool,
    pub crouch_held: bool,
    pub sprint_held: bool,
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
    /// Horizontal speed a slide starts at (or the current speed, if faster).
    pub slide_speed: f32,
    /// Sprint speed fraction required to start a slide.
    pub slide_min_speed_ratio: f32,
    pub slide_duration: f32,
    pub slide_friction: f32,
    pub slide_cooldown: f32,
    pub stand_height: f32,
    pub crouch_height: f32,
    /// Distance from the top of the capsule down to the eyes.
    pub eye_from_top: f32,
}

impl Default for MoveParams {
    fn default() -> Self {
        Self {
            walk_speed: 5.0,
            sprint_speed: 7.4,
            crouch_speed: 2.6,
            sprint_min_forward: 0.5,
            ground_accel: 12.0,
            air_accel: 2.0,
            air_wish_speed: 1.2,
            friction: 7.0,
            stop_speed: 2.5,
            gravity: 19.0,
            jump_velocity: 6.2,
            slide_speed: 9.5,
            slide_min_speed_ratio: 0.85,
            slide_duration: 0.75,
            slide_friction: 1.4,
            slide_cooldown: 0.4,
            stand_height: 1.8,
            crouch_height: 1.1,
            eye_from_top: 0.12,
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

    /// Eye height above the feet for a stance.
    pub fn eye_height(&self, stance: Stance) -> f32 {
        self.height(stance) - self.eye_from_top
    }
}

/// Unit vectors for a yaw: (forward, right), on the horizontal plane.
pub fn basis_from_yaw(yaw: f32) -> (Vec3, Vec3) {
    let (s, c) = yaw.sin_cos();
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
    let jump_pressed = input.jump && !state.jump_held;
    let crouch_pressed = input.crouch && !state.crouch_held;
    let sprint_pressed = input.sprint && !state.sprint_held;
    state.jump_held = input.jump;
    state.crouch_held = input.crouch;
    state.sprint_held = input.sprint;
    state.slide_cooldown = (state.slide_cooldown - dt).max(0.0);

    let wish = input.wish.clamp_length_max(1.0);
    let (forward, right) = basis_from_yaw(input.yaw);
    let wish_dir = (forward * wish.y + right * wish.x).normalize_or_zero();
    let horizontal_speed = horizontal(state.velocity).length();

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

    let mut jumped = false;
    if jump_pressed && env.on_floor && state.stance != Stance::Crouching {
        // Jumping out of a slide keeps its momentum.
        if state.stance == Stance::Sliding {
            state.stance = Stance::Standing;
        }
        state.velocity.y = params.jump_velocity;
        jumped = true;
    }

    if env.on_floor && !jumped {
        state.velocity.y = 0.0;
        let friction = if state.stance == Stance::Sliding {
            params.slide_friction
        } else {
            params.friction
        };
        apply_friction(&mut state.velocity, friction, params.stop_speed, dt);
        if state.stance != Stance::Sliding {
            let wish_speed = wish.length() * max_speed(state, params) * input.speed_multiplier;
            accelerate(
                &mut state.velocity,
                wish_dir,
                wish_speed,
                params.ground_accel,
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
            let fast_enough =
                horizontal_speed >= params.sprint_speed * params.slide_min_speed_ratio;
            if crouch_pressed
                && env.on_floor
                && state.sprinting
                && fast_enough
                && state.slide_cooldown <= 0.0
            {
                state.stance = Stance::Sliding;
                state.slide_time = params.slide_duration;
                state.sprinting = false;
                let dir = horizontal(state.velocity)
                    .try_normalize()
                    .unwrap_or(forward);
                let speed = horizontal_speed.max(params.slide_speed);
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
        Stance::Sliding => {}
    }
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

fn max_speed(state: &MoveState, params: &MoveParams) -> f32 {
    match state.stance {
        Stance::Crouching | Stance::Sliding => params.crouch_speed,
        Stance::Standing if state.sprinting => params.sprint_speed,
        Stance::Standing => params.walk_speed,
    }
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
        assert!(speed(&s) >= p.slide_speed - 0.5);

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

    #[test]
    fn air_control_is_limited() {
        let p = MoveParams::default();
        let mut s = MoveState::default();
        let air = MoveEnv {
            on_floor: false,
            can_stand: true,
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
}
