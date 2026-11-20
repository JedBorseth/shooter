//! Ladders, as Call of Duty plays them (classic IW and Modern Warfare 2019).
//!
//! - **Grab** by moving into the rungs from the ground or in the air, or by
//!   walking off the top toward the ladder's open side. The body stands up
//!   and is pulled onto the climbing line.
//! - **Climb** with the movement keys: moving toward the rungs climbs up,
//!   moving away climbs down (so a player who turned round, or stepped on
//!   from the top, still climbs the way they walk), at full speed within 60
//!   degrees of straight at or away from the rungs. No gravity, no strafing.
//!   The look is free; the body keeps facing the rungs, turning at most 90
//!   degrees from them at the hips (`hitbox::gait::Turning::hold`).
//! - **Leave** by climbing past the top (a short pull over onto the floor
//!   it leads to), by stepping back or aside at the bottom, or by jumping,
//!   which pushes off backward.
//! - **Slide down** with crouch (MW2019's fast ladder slide): the body drops
//!   faster and faster to a cap; climbing up or crouching again catches it.
//! - The weapon is lowered the whole time ([`super::MoveState::on_ladder`]
//!   blocks firing, aiming, the knife, and grenades in `player::step`).
//!
//! The engine finds the ladder (a marked volume) and reports where the feet
//! are relative to it ([`Ladder`]); everything else is decided here, so the
//! client predicts it exactly.

use glam::{Vec2, Vec3};

use super::{MoveEnv, MoveParams, MoveState, Stance, mantle, start_climb};

/// A ladder in reach, from the engine (`MoveEnv::ladder`): where the feet
/// are relative to its climbing line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ladder {
    /// Unit horizontal direction (`x`, `z`) a climber faces: into the rungs.
    pub facing: Vec2,
    /// How far the feet are past the climbing line along `facing`: negative
    /// in front of it (out from the ladder), positive behind it (over the
    /// floor at the top). The line is where a body touching the rungs stands.
    pub depth: f32,
    /// Feet offset across the ladder (to the right of a climber facing it)
    /// from its middle.
    pub lateral: f32,
    /// Feet height above the ladder's foot.
    pub height: f32,
    /// Height of the floor at its top above its foot.
    pub length: f32,
}

impl Ladder {
    fn facing3(&self) -> Vec3 {
        Vec3::new(self.facing.x, 0.0, self.facing.y)
    }

    /// Across the ladder, to the climber's right.
    fn right3(&self) -> Vec3 {
        Vec3::new(-self.facing.y, 0.0, self.facing.x)
    }
}

/// Ladder rules for one tick, run before the ground and air rules. Returns
/// the velocity when the player is on a ladder (or just left it by its top
/// or by jumping); `None` lets the usual rules move them.
#[allow(clippy::too_many_arguments)]
pub(super) fn step(
    state: &mut MoveState,
    env: MoveEnv,
    params: &MoveParams,
    wish: Vec2,
    forward: Vec3,
    right: Vec3,
    jump_pressed: bool,
    crouch_pressed: bool,
    dt: f32,
) -> Option<Vec3> {
    state.ladder_cooldown = (state.ladder_cooldown - dt).max(0.0);
    let Some(ladder) = env.ladder else {
        // Pushed or carried out of reach: let go and fall.
        if state.on_ladder() {
            leave(state, params);
        }
        return None;
    };
    // How much the input moves toward the rungs (+) or away from them (-).
    let wish_world = forward * wish.y + right * wish.x;
    let into = wish_world.dot(ladder.facing3());
    if !state.on_ladder() && !grab(state, &ladder, env, params, into) {
        return None;
    }
    state.ladder_facing = ladder.facing;
    let facing = ladder.facing3();

    if jump_pressed {
        // Push off backward, as a short hop away from the rungs.
        leave(state, params);
        state.jump_buffer = 0.0;
        state.velocity = -facing * params.ladder_jump_back + Vec3::Y * params.ladder_jump_up;
        return Some(state.velocity);
    }
    if crouch_pressed {
        state.ladder_slide = !state.ladder_slide;
    }
    if state.ladder_slide && into > 0.3 {
        state.ladder_slide = false;
    }

    // Over the top: the feet have cleared the floor it leads to, so step
    // forward onto it, with the turn of a short mantle that carries no rise.
    if into > 0.1 && !state.ladder_slide && ladder.height >= ladder.length + params.ladder_top_clear
    {
        leave(state, params);
        let carry = ladder.facing * params.ladder_exit_speed;
        start_climb(state, 0.02, params.ladder_exit_time, carry, false, false);
        return Some(mantle(state, params, dt));
    }

    // Mounting from the top: move out over the rungs before going down.
    let mounting = ladder.depth > 0.15;
    let mut rise = if mounting {
        0.0
    } else if state.ladder_slide {
        (state.velocity.y.min(0.0) - params.ladder_slide_accel * dt).max(-params.ladder_slide_speed)
    } else {
        // Full speed within 60 degrees of straight at (or away from) the rungs.
        let climb = (into / 0.5).clamp(-1.0, 1.0);
        if climb > 0.0 {
            climb * params.ladder_climb_speed
        } else {
            climb * params.ladder_down_speed
        }
    };
    if ladder.height >= ladder.length + params.ladder_top_clear {
        rise = rise.min(0.0);
    }

    // Standing on the floor at the bottom without climbing up lets go, and
    // the ground rules move the player this same tick.
    let at_bottom = ladder.height < ladder.length - params.ladder_top_reach;
    if env.on_floor && at_bottom && rise <= 0.0 && !mounting {
        leave(state, params);
        state.velocity.y = 0.0;
        return None;
    }

    // Held on the climbing line, in the middle of the rungs.
    let pull = (-ladder.depth * params.ladder_pull)
        .clamp(-params.ladder_mount_speed, params.ladder_mount_speed);
    let centre = (-ladder.lateral * params.ladder_pull).clamp(-1.0, 1.0);
    state.velocity = facing * pull + ladder.right3() * centre + Vec3::Y * rise;
    Some(state.velocity)
}

/// Takes hold of the ladder if the player is moving onto it: into the rungs
/// from in front (on the ground or in the air), or toward its open side
/// from the floor at its top.
fn grab(
    state: &mut MoveState,
    ladder: &Ladder,
    env: MoveEnv,
    params: &MoveParams,
    into: f32,
) -> bool {
    if state.ladder_cooldown > 0.0
        || state.stance == Stance::Sliding
        || state.stance == Stance::Prone
        || state.mantling()
    {
        return false;
    }
    if state.stance == Stance::Crouching && !env.can_stand {
        return false;
    }
    let below_top = ladder.height < ladder.length - params.ladder_top_reach;
    let from_front =
        below_top && into > 0.5 && (-params.ladder_reach..=0.2).contains(&ladder.depth);
    let from_top = env.on_floor
        && ladder.height >= ladder.length - params.ladder_top_reach
        && ladder.depth > 0.05
        && into < -0.5;
    if !(from_front || from_top) {
        return false;
    }
    state.ladder_facing = ladder.facing;
    state.ladder_slide = false;
    state.stance = Stance::Standing;
    state.sprinting = false;
    state.slide_time = 0.0;
    state.slide_queued = false;
    state.jump_buffer = 0.0;
    state.coyote = 0.0;
    // Catching a ladder in the air is not a landing.
    state.air_time = 0.0;
    // Whatever speed the player came with stops at the rungs.
    state.velocity = Vec3::new(0.0, state.velocity.y.min(0.0), 0.0);
    true
}

/// Lets go of the ladder, and keeps the player off it for a moment so
/// the same input does not grab it again.
fn leave(state: &mut MoveState, params: &MoveParams) {
    state.ladder_facing = Vec2::ZERO;
    state.ladder_slide = false;
    state.ladder_cooldown = params.ladder_regrab;
}

impl MoveState {
    /// On a ladder: climbing, sliding, or hanging on it.
    pub fn on_ladder(&self) -> bool {
        self.ladder_facing != Vec2::ZERO
    }

    /// Lets go of a ladder (death, a respawn): the next tick falls, with
    /// none of the climb's speed.
    pub fn leave_ladder(&mut self, params: &MoveParams) {
        if self.on_ladder() {
            leave(self, params);
            self.velocity = Vec3::ZERO;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movement::{MoveInput, step};

    const DT: f32 = 1.0 / 120.0;
    /// The ladder's top floor, above its foot.
    const LENGTH: f32 = 4.14;
    /// Facing +X: `basis_from_yaw` looks down +X at -90°.
    const TO_LADDER: f32 = -std::f32::consts::FRAC_PI_2;
    const FROM_LADDER: f32 = std::f32::consts::FRAC_PI_2;

    /// A stand-in for the engine around one ladder climbed toward +X: the
    /// climbing line at `x = 0`, the rungs' plate just past it (solid below
    /// the top), the ground at 0 in front and the top floor from `x = 0.1`.
    struct World {
        state: MoveState,
        position: Vec3,
        on_floor: bool,
    }

    impl World {
        fn at(position: Vec3) -> Self {
            Self {
                state: MoveState::default(),
                position,
                on_floor: true,
            }
        }

        fn ground(x: f32, y: f32) -> f32 {
            if x >= 0.1 && y >= LENGTH - 0.05 {
                LENGTH
            } else {
                0.0
            }
        }

        fn env(&self) -> MoveEnv {
            let p = self.position;
            let near = (-0.6..=1.0).contains(&p.x)
                && p.z.abs() < 0.45
                && (-0.3..=LENGTH + 0.6).contains(&p.y);
            MoveEnv {
                on_floor: self.on_floor,
                ladder: near.then_some(Ladder {
                    facing: Vec2::X,
                    depth: p.x,
                    lateral: p.z,
                    height: p.y,
                    length: LENGTH,
                }),
                ..MoveEnv::default()
            }
        }

        fn tick(&mut self, input: &MoveInput) {
            let env = self.env();
            let v = step(&mut self.state, input, env, &MoveParams::default(), DT);
            let mut next = self.position + v * DT;
            // The rungs' plate stops the body below the top.
            if next.x > 0.0 && self.position.x <= 0.0 && next.y < LENGTH {
                next.x = 0.0;
            }
            let ground = Self::ground(next.x, next.y);
            self.on_floor = next.y <= ground + 1e-4;
            if self.on_floor {
                next.y = ground;
                self.state.velocity.y = self.state.velocity.y.max(0.0);
            }
            self.position = next;
        }

        fn run(&mut self, input: MoveInput, seconds: f32) {
            for _ in 0..(seconds / DT).round() as usize {
                self.tick(&input);
            }
        }
    }

    fn moving(yaw: f32, wish: Vec2) -> MoveInput {
        MoveInput {
            wish,
            yaw,
            ..MoveInput::default()
        }
    }

    #[test]
    fn walking_into_the_rungs_climbs_at_climb_speed_and_steps_off_onto_the_top() {
        let mut w = World::at(Vec3::new(-0.4, 0.0, 0.1));
        let up = moving(TO_LADDER, Vec2::Y);
        w.run(up, 0.3);
        assert!(w.state.on_ladder(), "grabbed");
        assert!(
            w.position.x.abs() < 0.05 && w.position.z.abs() < 0.05,
            "on the line: {}",
            w.position
        );
        let params = MoveParams::default();
        assert!((w.state.velocity.y - params.ladder_climb_speed).abs() < 1e-3);
        // Up the ladder, over the top, and onto the floor there.
        let mut seconds = 0.3;
        while !(w.on_floor && w.position.y > 1.0 && !w.state.on_ladder() && !w.state.mantling()) {
            w.tick(&up);
            seconds += DT;
            assert!(seconds < 4.0, "stuck at {}", w.position);
        }
        assert!((w.position.y - LENGTH).abs() < 1e-3);
        assert!(w.position.x > 0.4, "well onto the top: {}", w.position);
        // About two seconds for a 4 m ladder, as in the reference.
        assert!((2.0..3.0).contains(&seconds), "{seconds} s");
        // Walking on, no landing penalty.
        assert_eq!(w.state.land_slow, 0.0);
    }

    #[test]
    fn moving_away_from_the_rungs_climbs_down_and_steps_off_at_the_bottom() {
        let mut w = World::at(Vec3::new(-0.3, 0.0, 0.0));
        w.run(moving(TO_LADDER, Vec2::Y), 1.0);
        let height = w.position.y;
        assert!(height > 1.5);
        // Facing the ladder and pulling back climbs down.
        w.run(moving(TO_LADDER, -Vec2::Y), 0.2);
        assert!(w.state.on_ladder());
        let params = MoveParams::default();
        assert!((w.state.velocity.y + params.ladder_down_speed).abs() < 1e-3);
        w.run(moving(TO_LADDER, -Vec2::Y), 1.5);
        assert!(
            w.on_floor && !w.state.on_ladder(),
            "stepped off at the bottom"
        );
        assert!(w.position.x < -0.05, "and backed away: {}", w.position);
        // Letting go holds the ladder still.
        let mut w = World::at(Vec3::new(-0.3, 0.0, 0.0));
        w.run(moving(TO_LADDER, Vec2::Y), 0.8);
        let y = w.position.y;
        w.run(
            MoveInput {
                yaw: TO_LADDER,
                ..MoveInput::default()
            },
            1.0,
        );
        assert!(
            w.state.on_ladder() && (w.position.y - y).abs() < 1e-3,
            "no gravity"
        );
    }

    #[test]
    fn walking_off_the_top_toward_its_open_side_climbs_down() {
        let mut w = World::at(Vec3::new(0.7, LENGTH, 0.0));
        // Walking forward, facing away from the rungs, toward the ladder.
        let off = moving(FROM_LADDER, Vec2::Y);
        w.run(off, 0.2);
        assert!(w.state.on_ladder(), "grabbed from the top");
        let mut seconds = 0.2;
        while !(w.on_floor && w.position.y < 0.01) {
            w.tick(&off);
            seconds += DT;
            assert!(seconds < 4.0, "stuck at {}", w.position);
            assert!(w.position.y > -0.01);
        }
        assert!(seconds < 3.0, "{seconds} s down");
        // On the floor at the bottom, it lets go.
        w.tick(&MoveInput::default());
        assert!(!w.state.on_ladder());
    }

    #[test]
    fn crouch_slides_down_fast_and_lands_standing() {
        let mut w = World::at(Vec3::new(-0.3, 0.0, 0.0));
        w.run(moving(TO_LADDER, Vec2::Y), 2.0);
        assert!(w.position.y > 3.0);
        let press = MoveInput {
            yaw: TO_LADDER,
            crouch: true,
            ..MoveInput::default()
        };
        w.tick(&press);
        assert!(w.state.ladder_slide);
        let mut fastest: f32 = 0.0;
        let mut seconds = 0.0;
        while !w.on_floor {
            w.tick(&press);
            fastest = fastest.max(-w.state.velocity.y);
            seconds += DT;
            assert!(seconds < 2.0);
        }
        let params = MoveParams::default();
        assert!(
            (fastest - params.ladder_slide_speed).abs() < 0.05,
            "{fastest}"
        );
        // About 4 m in under a second, against two by climbing down.
        assert!(seconds < 1.0, "{seconds} s");
        w.tick(&MoveInput {
            yaw: TO_LADDER,
            ..MoveInput::default()
        });
        assert!(!w.state.on_ladder());
        assert_eq!(w.state.stance, Stance::Standing);
    }

    #[test]
    fn climbing_up_catches_a_slide() {
        let mut w = World::at(Vec3::new(-0.3, 0.0, 0.0));
        w.run(moving(TO_LADDER, Vec2::Y), 2.0);
        w.tick(&MoveInput {
            yaw: TO_LADDER,
            crouch: true,
            ..MoveInput::default()
        });
        w.run(
            MoveInput {
                yaw: TO_LADDER,
                crouch: true,
                ..MoveInput::default()
            },
            0.2,
        );
        w.run(moving(TO_LADDER, Vec2::Y), 0.1);
        assert!(!w.state.ladder_slide && w.state.velocity.y > 0.0);
    }

    #[test]
    fn jumping_pushes_off_backward_and_does_not_regrab_at_once() {
        let mut w = World::at(Vec3::new(-0.3, 0.0, 0.0));
        w.run(moving(TO_LADDER, Vec2::Y), 1.0);
        let jump = MoveInput {
            yaw: TO_LADDER,
            wish: Vec2::Y,
            jump: true,
            ..MoveInput::default()
        };
        w.tick(&jump);
        assert!(!w.state.on_ladder());
        assert!(w.state.velocity.x < -2.0 && w.state.velocity.y > 0.0);
        // Still pushing toward it: no regrab within the delay.
        for _ in 0..((MoveParams::default().ladder_regrab / DT) as usize - 2) {
            w.tick(&moving(TO_LADDER, Vec2::Y));
            assert!(!w.state.on_ladder());
        }
    }

    #[test]
    fn walking_past_or_sliding_into_it_does_not_grab() {
        let mut w = World::at(Vec3::new(-0.3, 0.0, -0.4));
        // Along the ladder's face, not into it.
        w.run(moving(0.0, -Vec2::Y), 0.5);
        assert!(!w.state.on_ladder());
        let mut w = World::at(Vec3::new(-0.3, 0.0, 0.0));
        w.state.stance = Stance::Sliding;
        w.state.slide_time = 0.5;
        w.tick(&moving(TO_LADDER, Vec2::Y));
        assert!(!w.state.on_ladder());
    }

    #[test]
    fn a_ladder_is_caught_in_the_air() {
        let mut w = World::at(Vec3::new(-0.3, 1.5, 0.0));
        w.on_floor = false;
        w.state.velocity = Vec3::new(0.0, -4.0, 0.0);
        w.state.air_time = 0.5;
        w.tick(&moving(TO_LADDER, Vec2::Y));
        assert!(w.state.on_ladder());
        assert_eq!(w.state.air_time, 0.0, "not a landing later");
        assert!(w.state.velocity.y > 0.0, "climbing at once");
    }

    #[test]
    fn crouched_players_stand_up_to_climb() {
        let mut w = World::at(Vec3::new(-0.3, 0.0, 0.0));
        w.state.stance = Stance::Crouching;
        w.state.crouch = 1.0;
        w.run(moving(TO_LADDER, Vec2::Y), 0.5);
        assert!(w.state.on_ladder());
        assert_eq!(w.state.stance, Stance::Standing);
        assert!(w.state.crouch < 1.0);
    }
}
