//! The complete per-player simulation: movement plus combat, advanced together.
//!
//! This is the unit that netcode will predict on the client and run
//! authoritatively on the server: `step(state, input) -> state'`.

use glam::{Vec2, Vec3};

use crate::movement::{self, MoveEnv, MoveInput, MoveParams, MoveState};
use crate::weapon::{self, CombatContext, CombatEvent, CombatInput, CombatState, WeaponId};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerInput {
    /// Desired direction in view space: `x` right, `y` forward.
    pub wish: Vec2,
    /// View angles in radians (without recoil).
    pub yaw: f32,
    pub pitch: f32,
    pub jump: bool,
    pub sprint: bool,
    pub crouch: bool,
    pub fire: bool,
    pub aim: bool,
    pub reload: bool,
    pub switch_to: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerState {
    pub movement: MoveState,
    pub combat: CombatState,
    reload_held: bool,
}

impl PlayerState {
    pub fn new(loadout: [WeaponId; 2]) -> Self {
        Self {
            movement: MoveState::default(),
            combat: CombatState::new(loadout),
            reload_held: false,
        }
    }

    /// Context the weapon needs from movement, also used for HUD spread.
    pub fn combat_context(&self, env: MoveEnv, params: &MoveParams) -> CombatContext {
        let speed = Vec2::new(self.movement.velocity.x, self.movement.velocity.z).length();
        CombatContext {
            sprinting: self.movement.sprinting,
            airborne: !env.on_floor,
            move_ratio: speed / params.walk_speed,
        }
    }
}

impl Default for PlayerState {
    fn default() -> Self {
        Self::new([WeaponId::M4, WeaponId::Pistol])
    }
}

/// Advances the player one tick. Returns the velocity the engine should move
/// with; combat events are appended to `events`.
pub fn step(
    state: &mut PlayerState,
    input: &PlayerInput,
    env: MoveEnv,
    params: &MoveParams,
    dt: f32,
    events: &mut Vec<CombatEvent>,
) -> Vec3 {
    let reload_pressed = input.reload && !state.reload_held;
    state.reload_held = input.reload;

    let def = state.combat.def();
    let ads = state.combat.ads;
    let move_input = MoveInput {
        wish: input.wish,
        yaw: input.yaw,
        jump: input.jump,
        sprint: input.sprint,
        crouch: input.crouch,
        block_sprint: input.fire || input.aim || reload_pressed,
        speed_multiplier: def.move_scale * (1.0 + (def.ads_move_scale - 1.0) * ads),
    };
    let velocity = movement::step(&mut state.movement, &move_input, env, params, dt);

    let combat_input = CombatInput {
        fire: input.fire,
        aim: input.aim,
        reload: reload_pressed,
        switch_to: input.switch_to,
    };
    let ctx = state.combat_context(env, params);
    weapon::step(&mut state.combat, &combat_input, &ctx, dt, events);
    velocity
}

/// World-space direction for view angles plus an aim offset (`x` pitch up, `y` yaw right).
pub fn aim_direction(yaw: f32, pitch: f32, offset: Vec2) -> Vec3 {
    let pitch = pitch + offset.x;
    let yaw = yaw - offset.y;
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    Vec3::new(-sy * cp, sp, -cy * cp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapon::Action;

    const DT: f32 = 1.0 / 60.0;

    fn ready_state() -> PlayerState {
        let mut s = PlayerState::default();
        s.combat.action = Action::Ready;
        s
    }

    #[test]
    fn firing_ends_sprint_and_waits_for_sprint_out() {
        let params = MoveParams::default();
        let mut s = ready_state();
        let mut events = Vec::new();
        let sprint = PlayerInput {
            wish: Vec2::Y,
            sprint: true,
            ..Default::default()
        };
        for _ in 0..30 {
            step(
                &mut s,
                &sprint,
                MoveEnv::default(),
                &params,
                DT,
                &mut events,
            );
        }
        assert!(s.movement.sprinting);

        let fire = PlayerInput {
            wish: Vec2::Y,
            fire: true,
            ..Default::default()
        };
        step(&mut s, &fire, MoveEnv::default(), &params, DT, &mut events);
        assert!(!s.movement.sprinting, "firing ends the sprint");
        assert!(events.is_empty(), "no shot during sprint-out");
        for _ in 0..20 {
            step(&mut s, &fire, MoveEnv::default(), &params, DT, &mut events);
        }
        assert!(
            events
                .iter()
                .any(|e| matches!(e, CombatEvent::Fired { .. }))
        );
    }

    #[test]
    fn aiming_slows_movement() {
        let params = MoveParams::default();
        let mut s = ready_state();
        let mut events = Vec::new();
        let input = PlayerInput {
            wish: Vec2::Y,
            aim: true,
            ..Default::default()
        };
        for _ in 0..120 {
            step(&mut s, &input, MoveEnv::default(), &params, DT, &mut events);
        }
        let speed = Vec2::new(s.movement.velocity.x, s.movement.velocity.z).length();
        assert!(
            (speed - params.walk_speed * 0.6).abs() < 0.2,
            "speed {speed}"
        );
    }

    #[test]
    fn aim_direction_matches_godot_axes() {
        let d = aim_direction(0.0, 0.0, Vec2::ZERO);
        assert!((d - Vec3::NEG_Z).length() < 1e-6);
        let up = aim_direction(0.0, 0.0, Vec2::new(0.1, 0.0));
        assert!(up.y > 0.0);
        let right = aim_direction(0.0, 0.0, Vec2::new(0.0, 0.1));
        assert!(right.x > 0.0);
    }
}
