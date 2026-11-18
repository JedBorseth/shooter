//! The complete per-player simulation: movement, combat, and health, advanced together.
//!
//! This is the unit that netcode will predict on the client and run
//! authoritatively on the server: `step(state, input) -> state'`.

use glam::{Vec2, Vec3};

use crate::melee;
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
    /// Sprint toggle; while scoped it holds breath instead.
    pub sprint: bool,
    pub crouch: bool,
    pub fire: bool,
    pub aim: bool,
    pub reload: bool,
    pub switch_to: Option<u8>,
    /// Take this weapon from the pickup the player is looking at.
    pub pickup: Option<WeaponId>,
    /// The melee button ([`crate::melee`]); a press swings the knife.
    pub melee: bool,
    /// With a melee press: the target to lunge at, as the horizontal offset
    /// (`x`, `z`) from the player's feet to theirs, if one was in range. The
    /// client that pressed melee picks it from what it draws.
    pub lunge: Option<Vec2>,
    /// Online only: the server tick (fractional) this client was drawing other
    /// players at when the input was read. The server rewinds their hitboxes to
    /// it to check this input's shots (`net::lag`). Sent while firing; the
    /// simulation itself ignores it. On the input that carries a
    /// [`FireAim`], it is the tick being drawn when the button went down.
    pub view_tick: Option<f64>,
    /// The view when the fire button went down since the last tick, if it
    /// did: the first shot fired on this input leaves from it
    /// ([`Self::shot_view`]). See `net::subtick`.
    pub fire_aim: Option<FireAim>,
}

/// Where the player was looking when they pressed fire between two ticks,
/// relative to this input's view angles: press angle minus tick angle, in
/// radians. The client captures it on the press event, so a shot leaves from
/// the frame the player clicked on rather than up to a tick later. The server
/// only accepts angles between the previous input's view and this one's
/// (`net::subtick::check`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FireAim {
    pub yaw: f32,
    pub pitch: f32,
}

impl PlayerInput {
    /// This input with movement, firing, melee, and pickups taken out, as
    /// during a round's freeze: the player can still look around, aim,
    /// reload, and switch weapons. The server and the predicting client apply
    /// the same filter.
    pub fn held_still(self) -> Self {
        Self {
            wish: Vec2::ZERO,
            jump: false,
            sprint: false,
            crouch: false,
            fire: false,
            melee: false,
            lunge: None,
            pickup: None,
            view_tick: None,
            fire_aim: None,
            ..self
        }
    }

    /// View angles (yaw, pitch) a shot fired on this input leaves from,
    /// before recoil and spread: the press angles when it carries a
    /// [`FireAim`], otherwise its own. The view itself still ends the tick at
    /// `yaw` and `pitch`.
    pub fn shot_view(&self) -> (f32, f32) {
        match self.fire_aim {
            Some(aim) => (self.yaw + aim.yaw, self.pitch + aim.pitch),
            None => (self.yaw, self.pitch),
        }
    }
}

/// Health rules. Damage itself comes from weapons (applied by whoever runs the
/// hit test) and from falling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HealthParams {
    pub max_health: f32,
    /// Seconds without damage before health regenerates.
    pub regen_delay: f32,
    /// Health per second while regenerating.
    pub regen_rate: f32,
    /// Landing faster than this (m/s) hurts: about a 4 m drop.
    pub fall_damage_speed: f32,
    /// Landing at this speed or faster kills: about a 10 m drop.
    pub fall_lethal_speed: f32,
}

impl Default for HealthParams {
    fn default() -> Self {
        Self {
            max_health: 100.0,
            regen_delay: 4.0,
            regen_rate: 40.0,
            fall_damage_speed: 12.5,
            fall_lethal_speed: 19.5,
        }
    }
}

impl HealthParams {
    /// Damage for landing at `speed` m/s.
    pub fn fall_damage(&self, speed: f32) -> f32 {
        if speed <= self.fall_damage_speed {
            return 0.0;
        }
        let t =
            (speed - self.fall_damage_speed) / (self.fall_lethal_speed - self.fall_damage_speed);
        (t * self.max_health).min(self.max_health)
    }
}

/// Everything the simulation needs besides state and input.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerParams {
    pub movement: MoveParams,
    pub health: HealthParams,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerState {
    pub movement: MoveState,
    pub combat: CombatState,
    pub health: f32,
    /// Seconds since the player last took damage.
    pub since_damage: f32,
    /// Downward speed during the last airborne tick, for landing and fall damage.
    pub fall_speed: f32,
    pub airborne: bool,
    pub(crate) reload_held: bool,
    /// The melee button was down last tick (a press starts a strike).
    pub melee_held: bool,
}

impl PlayerState {
    pub fn new(loadout: [WeaponId; 2]) -> Self {
        Self {
            movement: MoveState::default(),
            combat: CombatState::new(loadout),
            health: HealthParams::default().max_health,
            since_damage: f32::MAX,
            fall_speed: 0.0,
            airborne: false,
            reload_held: false,
            melee_held: false,
        }
    }

    /// A freshly spawned player with the default loadout and full health.
    pub fn spawned(params: &PlayerParams) -> Self {
        Self {
            health: params.health.max_health,
            ..Self::default()
        }
    }

    pub fn alive(&self) -> bool {
        self.health > 0.0
    }

    /// Context the weapon needs from movement, also used for HUD spread.
    pub fn combat_context(&self, env: MoveEnv, params: &MoveParams) -> CombatContext {
        let speed = Vec2::new(self.movement.velocity.x, self.movement.velocity.z).length();
        CombatContext {
            // The weapon is lowered while climbing, as while sprinting.
            sprinting: self.movement.sprinting || self.movement.mantling(),
            airborne: !env.on_floor,
            crouched: self.movement.stance == crate::movement::Stance::Crouching,
            move_ratio: speed / params.walk_speed,
        }
    }
}

impl Default for PlayerState {
    fn default() -> Self {
        Self::new(default_loadout())
    }
}

/// The loadout a player spawns with.
pub fn default_loadout() -> [WeaponId; 2] {
    [WeaponId::expect("m4"), WeaponId::expect("m1911")]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlayerEvent {
    Combat(CombatEvent),
    /// Touched down after falling at `speed` m/s.
    Landed {
        speed: f32,
    },
    /// Lost `amount` health; `fall` when it came from a landing.
    Damaged {
        amount: f32,
        fall: bool,
    },
    Died,
}

/// Applies damage and reports it. Returns true if this killed the player.
pub fn apply_damage(
    state: &mut PlayerState,
    amount: f32,
    fall: bool,
    events: &mut Vec<PlayerEvent>,
) -> bool {
    if !state.alive() || amount <= 0.0 {
        return false;
    }
    let amount = amount.min(state.health);
    state.health -= amount;
    state.since_damage = 0.0;
    events.push(PlayerEvent::Damaged { amount, fall });
    if state.health <= 0.0 {
        state.health = 0.0;
        events.push(PlayerEvent::Died);
        return true;
    }
    false
}

/// Advances the player one tick and returns the velocity the engine should move
/// with. A dead player gets no control and just falls.
pub fn step(
    state: &mut PlayerState,
    input: &PlayerInput,
    env: MoveEnv,
    params: &PlayerParams,
    dt: f32,
    events: &mut Vec<PlayerEvent>,
) -> Vec3 {
    if env.on_floor && state.airborne {
        events.push(PlayerEvent::Landed {
            speed: state.fall_speed,
        });
        let damage = params.health.fall_damage(state.fall_speed);
        apply_damage(state, damage, true, events);
    }
    state.airborne = !env.on_floor;

    // Decided after landing damage, so a fatal landing already gets no control.
    let input = if state.alive() {
        *input
    } else {
        // A jump pressed just before dying must not fire later from the buffer.
        state.movement.jump_buffer = 0.0;
        state.movement.coyote = 0.0;
        PlayerInput {
            yaw: input.yaw,
            pitch: input.pitch,
            ..Default::default()
        }
    };

    state.since_damage += dt;
    if state.alive() && state.since_damage >= params.health.regen_delay {
        state.health = (state.health + params.health.regen_rate * dt).min(params.health.max_health);
    }

    let reload_pressed = input.reload && !state.reload_held;
    state.reload_held = input.reload;
    let melee_pressed = input.melee && !state.melee_held;
    state.melee_held = input.melee;
    // Not while climbing, when the weapon is down anyway.
    let melee = melee_pressed && state.combat.can_melee() && !state.movement.mantling();
    let lunge = input.lunge.filter(|offset| melee && melee::lunges(*offset));

    let def = state.combat.def();
    let ads = state.combat.ads;
    let scoped = state.combat.scope_amount() > 0.5;
    let move_input = MoveInput {
        wish: input.wish,
        yaw: input.yaw,
        jump: input.jump,
        // While scoped the sprint key holds breath instead. Movement still sees the
        // key (blocked), so letting go of the scope with Shift held doesn't count
        // as a fresh sprint press.
        sprint: input.sprint,
        crouch: input.crouch,
        block_sprint: input.fire
            || input.aim
            || reload_pressed
            || scoped
            || melee
            || state.combat.meleeing(),
        speed_multiplier: def.move_scale * (1.0 + (def.ads_move_scale - 1.0) * ads),
        lunge,
    };
    let velocity = movement::step(&mut state.movement, &move_input, env, &params.movement, dt);
    if !env.on_floor {
        state.fall_speed = (-velocity.y).max(0.0);
    }

    let (shot_yaw, shot_pitch) = input.shot_view();
    let combat_input = CombatInput {
        fire: input.fire,
        shot_yaw,
        shot_pitch,
        aim: input.aim,
        reload: reload_pressed,
        switch_to: input.switch_to,
        hold_breath: input.sprint && scoped,
        pickup: input.pickup,
        melee: melee.then(|| (melee::strike_delay(lunge), lunge.is_some())),
    };
    let ctx = state.combat_context(env, &params.movement);
    let mut combat = Vec::new();
    weapon::step(&mut state.combat, &combat_input, &ctx, dt, &mut combat);
    events.extend(combat.into_iter().map(PlayerEvent::Combat));
    velocity
}

/// World-space direction for view angles plus an aim offset (`x` pitch up, `y` yaw right).
pub fn aim_direction(yaw: f32, pitch: f32, offset: Vec2) -> Vec3 {
    crate::look::view_direction(yaw - offset.y, pitch + offset.x)
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

    fn run(
        s: &mut PlayerState,
        input: PlayerInput,
        env: MoveEnv,
        ticks: usize,
    ) -> Vec<PlayerEvent> {
        let params = PlayerParams::default();
        let mut events = Vec::new();
        for _ in 0..ticks {
            step(s, &input, env, &params, DT, &mut events);
        }
        events
    }

    fn fired(events: &[PlayerEvent]) -> bool {
        events
            .iter()
            .any(|e| matches!(e, PlayerEvent::Combat(CombatEvent::Fired { .. })))
    }

    const AIRBORNE: MoveEnv = MoveEnv {
        on_floor: false,
        can_stand: true,
        ledge: None,
    };

    #[test]
    fn firing_ends_sprint_and_waits_for_sprint_out() {
        let mut s = ready_state();
        let sprint = PlayerInput {
            wish: Vec2::Y,
            sprint: true,
            ..Default::default()
        };
        run(&mut s, sprint, MoveEnv::default(), 30);
        assert!(s.movement.sprinting);

        let fire = PlayerInput {
            wish: Vec2::Y,
            fire: true,
            ..Default::default()
        };
        let events = run(&mut s, fire, MoveEnv::default(), 1);
        assert!(!s.movement.sprinting, "firing ends the sprint");
        assert!(events.is_empty(), "no shot during sprint-out");
        let events = run(&mut s, fire, MoveEnv::default(), 20);
        assert!(fired(&events));
    }

    fn melee_events(events: &[PlayerEvent]) -> (usize, usize) {
        let started = events
            .iter()
            .filter(|e| matches!(e, PlayerEvent::Combat(CombatEvent::MeleeStarted { .. })))
            .count();
        let struck = events
            .iter()
            .filter(|e| matches!(e, PlayerEvent::Combat(CombatEvent::MeleeStruck { .. })))
            .count();
        (started, struck)
    }

    #[test]
    fn a_melee_press_strikes_once_and_holds_the_weapon_down() {
        let mut s = ready_state();
        let knife = PlayerInput {
            melee: true,
            fire: true,
            ..Default::default()
        };
        let events = run(&mut s, knife, MoveEnv::default(), 1);
        assert_eq!(melee_events(&events), (1, 0));
        assert!(s.combat.meleeing());
        // The strike lands after the delay; holding the button does not swing again.
        let ticks = (melee::STRIKE_DELAY / DT).round() as usize + 1;
        let events = run(&mut s, knife, MoveEnv::default(), ticks);
        assert_eq!(melee_events(&events), (0, 1));
        assert!(!fired(&events), "no shots during a melee");
        let recovery = (melee::RECOVERY / DT).round() as usize + 4;
        let events = run(&mut s, knife, MoveEnv::default(), recovery);
        assert_eq!(melee_events(&events), (0, 0));
        assert!(!s.combat.meleeing(), "back up after the recovery");
        assert!(
            fired(&events),
            "the held trigger fires once the weapon is up"
        );
    }

    #[test]
    fn a_lunge_carries_the_player_to_the_target_and_strikes_on_arrival() {
        let mut s = ready_state();
        let params = PlayerParams::default();
        let offset = Vec2::new(0.0, -3.0);
        let mut input = PlayerInput {
            melee: true,
            lunge: Some(offset),
            ..Default::default()
        };
        let mut position = Vec3::ZERO;
        let mut events = Vec::new();
        let mut struck_at = None;
        for tick in 0..30 {
            events.clear();
            let v = step(&mut s, &input, MoveEnv::default(), &params, DT, &mut events);
            position += v * DT;
            input.lunge = None;
            if melee_events(&events).1 == 1 {
                struck_at = Some((tick, position));
            }
        }
        let (tick, at) = struck_at.expect("the strike landed");
        let travel = melee::lunge_travel(offset);
        assert!((-at.z - travel).abs() < 0.05, "struck at {at}");
        assert!(tick >= 1 && tick <= (melee::lunge_time(offset) / DT).ceil() as usize + 1);
        assert!(
            -position.z < travel + 0.15,
            "stops at the target: {position}"
        );
    }

    #[test]
    fn a_lunge_is_ignored_without_a_melee_that_can_start() {
        let mut s = ready_state();
        s.combat.action = Action::Raising { remaining: 0.5 };
        let input = PlayerInput {
            melee: true,
            lunge: Some(Vec2::new(0.0, -3.0)),
            ..Default::default()
        };
        let events = run(&mut s, input, MoveEnv::default(), 1);
        assert_eq!(melee_events(&events), (0, 0), "no knife while drawing");
        assert!(!s.movement.lunging(), "and no dash");
    }

    #[test]
    fn a_melee_cuts_a_reload_short() {
        let mut s = ready_state();
        s.combat.slots[0].magazine = 10;
        let reload = PlayerInput {
            reload: true,
            ..Default::default()
        };
        run(&mut s, reload, MoveEnv::default(), 1);
        assert!(s.combat.is_reloading());
        let knife = PlayerInput {
            melee: true,
            ..Default::default()
        };
        run(&mut s, knife, MoveEnv::default(), 1);
        assert!(s.combat.meleeing());
        run(&mut s, PlayerInput::default(), MoveEnv::default(), 40);
        assert_eq!(s.combat.action, Action::Ready);
        assert_eq!(
            s.combat.slots[0].magazine, 10,
            "the fresh magazine never seated"
        );
    }

    #[test]
    fn aiming_slows_movement() {
        let mut s = ready_state();
        let input = PlayerInput {
            wish: Vec2::Y,
            aim: true,
            ..Default::default()
        };
        run(&mut s, input, MoveEnv::default(), 120);
        let speed = Vec2::new(s.movement.velocity.x, s.movement.velocity.z).length();
        let walk = MoveParams::default().walk_speed;
        assert!((speed - walk * 0.6).abs() < 0.2, "speed {speed}");
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

    /// Falls for `ticks` ticks (feeding back the velocity, as the engine would), then lands.
    fn fall_and_land(s: &mut PlayerState, ticks: usize) -> Vec<PlayerEvent> {
        run(s, PlayerInput::default(), AIRBORNE, ticks);
        s.movement.velocity.y = 0.0;
        run(s, PlayerInput::default(), MoveEnv::default(), 1)
    }

    #[test]
    fn short_falls_land_without_damage() {
        let mut s = ready_state();
        // 0.5 s of falling is about 9.5 m/s, a 2.4 m drop.
        let events = fall_and_land(&mut s, 30);
        assert!(matches!(events[0], PlayerEvent::Landed { speed } if speed > 9.0));
        assert_eq!(s.health, 100.0);
    }

    #[test]
    fn long_falls_hurt_and_very_long_falls_kill() {
        let mut s = ready_state();
        // 0.85 s is about 16 m/s, a 7 m drop.
        let events = fall_and_land(&mut s, 51);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, PlayerEvent::Damaged { fall: true, .. }))
        );
        assert!(s.health < 60.0 && s.alive(), "health {}", s.health);

        let mut s = ready_state();
        let events = fall_and_land(&mut s, 70);
        assert!(events.contains(&PlayerEvent::Died));
        assert!(!s.alive());
    }

    #[test]
    fn a_jump_buffered_before_a_fatal_landing_does_not_fire() {
        let mut s = ready_state();
        run(&mut s, PlayerInput::default(), AIRBORNE, 66);
        let jump = PlayerInput {
            jump: true,
            ..Default::default()
        };
        run(&mut s, jump, AIRBORNE, 3);
        s.movement.velocity.y = 0.0;
        let events = run(&mut s, jump, MoveEnv::default(), 1);
        assert!(events.contains(&PlayerEvent::Died));
        assert!(s.movement.velocity.y <= 0.0, "the body stays down");
    }

    #[test]
    fn a_jump_buffered_before_dying_in_the_air_does_not_fire() {
        let mut s = ready_state();
        let jump = PlayerInput {
            jump: true,
            ..Default::default()
        };
        run(&mut s, jump, AIRBORNE, 1);
        let mut events = Vec::new();
        apply_damage(&mut s, 200.0, false, &mut events);
        run(&mut s, PlayerInput::default(), AIRBORNE, 1);
        s.movement.velocity.y = 0.0;
        run(&mut s, PlayerInput::default(), MoveEnv::default(), 1);
        assert!(s.movement.velocity.y <= 0.0, "the body stays down");
    }

    #[test]
    fn health_regenerates_after_a_delay() {
        let mut s = ready_state();
        let mut events = Vec::new();
        apply_damage(&mut s, 60.0, false, &mut events);
        run(&mut s, PlayerInput::default(), MoveEnv::default(), 180);
        assert_eq!(s.health, 40.0, "no regeneration during the delay");
        run(&mut s, PlayerInput::default(), MoveEnv::default(), 240);
        assert_eq!(s.health, 100.0);
    }

    #[test]
    fn dead_players_cannot_act() {
        let mut s = ready_state();
        let mut events = Vec::new();
        assert!(apply_damage(&mut s, 150.0, false, &mut events));
        assert_eq!(s.health, 0.0);
        let input = PlayerInput {
            wish: Vec2::Y,
            fire: true,
            ..Default::default()
        };
        let events = run(&mut s, input, MoveEnv::default(), 60);
        assert!(!fired(&events));
        assert_eq!(s.movement.velocity, Vec3::ZERO);
        assert_eq!(s.health, 0.0, "the dead do not regenerate");
    }

    #[test]
    fn sprint_key_holds_breath_while_scoped() {
        let mut s = PlayerState::new([WeaponId::expect("m40"), WeaponId::expect("m1911")]);
        s.combat.action = Action::Ready;
        let aim = PlayerInput {
            aim: true,
            ..Default::default()
        };
        run(&mut s, aim, MoveEnv::default(), 30);
        let hold = PlayerInput {
            aim: true,
            sprint: true,
            ..Default::default()
        };
        run(&mut s, hold, MoveEnv::default(), 5);
        assert!(s.combat.holding_breath);
        assert!(!s.movement.sprinting);

        // Let go of the scope while still holding Shift and moving: no sprint.
        let release = PlayerInput {
            wish: Vec2::Y,
            sprint: true,
            ..Default::default()
        };
        run(&mut s, release, MoveEnv::default(), 40);
        assert!(
            !s.movement.sprinting,
            "releasing the scope must not start a sprint"
        );
    }

    #[test]
    fn spawned_players_get_the_configured_health() {
        let mut params = PlayerParams::default();
        params.health.max_health = 150.0;
        assert_eq!(PlayerState::spawned(&params).health, 150.0);
    }

    #[test]
    fn a_held_still_input_keeps_only_looking_aiming_and_weapon_handling() {
        let input = PlayerInput {
            wish: Vec2::new(0.5, 1.0),
            yaw: 1.25,
            pitch: -0.3,
            jump: true,
            sprint: true,
            crouch: true,
            fire: true,
            aim: true,
            reload: true,
            switch_to: Some(1),
            pickup: Some(WeaponId::expect("ak47")),
            melee: true,
            lunge: Some(Vec2::new(0.5, -2.0)),
            view_tick: Some(12.5),
            fire_aim: Some(FireAim {
                yaw: 0.01,
                pitch: 0.0,
            }),
        };
        assert_eq!(
            input.held_still(),
            PlayerInput {
                yaw: 1.25,
                pitch: -0.3,
                aim: true,
                reload: true,
                switch_to: Some(1),
                ..Default::default()
            }
        );
    }

    #[test]
    fn a_press_shot_leaves_from_the_press_aim_and_held_fire_from_the_tick() {
        let mut s = ready_state();
        let shots = |events: &[PlayerEvent]| -> Vec<(f32, f32)> {
            events
                .iter()
                .filter_map(|e| match *e {
                    PlayerEvent::Combat(CombatEvent::Fired { yaw, pitch, .. }) => {
                        Some((yaw, pitch))
                    }
                    _ => None,
                })
                .collect()
        };
        let press = PlayerInput {
            yaw: 1.0,
            pitch: 0.2,
            fire: true,
            fire_aim: Some(FireAim {
                yaw: 0.05,
                pitch: -0.01,
            }),
            ..Default::default()
        };
        let events = run(&mut s, press, MoveEnv::default(), 1);
        assert_eq!(shots(&events), vec![(1.05, 0.19)]);
        // The movement view still ends the tick at the tick's angles.
        let after_press = s;
        let mut plain = ready_state();
        run(
            &mut plain,
            PlayerInput {
                fire_aim: None,
                ..press
            },
            MoveEnv::default(),
            1,
        );
        assert_eq!(after_press, plain);

        // The M4 is automatic: holding on fires from each tick's own angles.
        let held = PlayerInput {
            yaw: 1.1,
            pitch: 0.25,
            fire: true,
            ..Default::default()
        };
        let events = run(&mut s, held, MoveEnv::default(), 60);
        let held_shots = shots(&events);
        assert!(held_shots.len() > 3, "{held_shots:?}");
        assert!(held_shots.iter().all(|&shot| shot == (1.1, 0.25)));
    }
}
