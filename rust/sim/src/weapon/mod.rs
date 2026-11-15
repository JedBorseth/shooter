//! Weapons: definitions (loaded from data files) and the per-tick combat state machine.
//!
//! Classic Call of Duty rules: firing or aiming ends a sprint, a short
//! "sprint-out" delay follows before the first shot, sprinting cancels a reload,
//! and the view kicks along a fixed recoil pattern that recovers toward the
//! original aim once the trigger is released. Bolt and pump weapons cycle after
//! each shot; shotguns load shell by shell and firing interrupts the reload.
//! Scoped weapons sway, and holding breath steadies them for a few seconds.
//!
//! Recoil follows the arcade split: the aim kick here is small and recovers
//! quickly, so sprays stay controllable. The weapon model kick and camera punch
//! in `shooter-game` carry most of the violence, and never move the aim.
//!
//! Like movement, this is deterministic: spread, pellets, and recoil jitter are
//! hashed from the player's shot counter, and scope sway is a function of
//! accumulated tick time, so client prediction and the server agree.

mod def;

use std::f32::consts::TAU;

use glam::Vec2;

pub use def::{
    Damage, FireMode, LoadError, RecoilDef, Reload, ScopeDef, Slot, Spread, WeaponDef, WeaponId,
    builtin, install, parse, parse_all, table,
};

use crate::rng;

/// Ammo for one carried weapon.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponState {
    pub id: WeaponId,
    pub magazine: u16,
    pub reserve: u16,
}

impl WeaponState {
    pub fn full(id: WeaponId) -> Self {
        let def = id.def();
        Self {
            id,
            magazine: def.magazine,
            reserve: def.reserve,
        }
    }
}

/// Stages of a round-by-round reload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadStage {
    Opening,
    Round,
    Closing,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Action {
    #[default]
    Ready,
    /// Magazine reload; seconds remaining and whether the magazine was empty.
    Reloading {
        remaining: f32,
        empty: bool,
    },
    /// Round-by-round reload; seconds left in the current stage.
    Loading {
        remaining: f32,
        stage: LoadStage,
    },
    /// Working the bolt or pump after a shot.
    Cycling {
        remaining: f32,
    },
    /// Putting the current weapon away before switching to slot `to`.
    Lowering {
        remaining: f32,
        to: u8,
    },
    Raising {
        remaining: f32,
    },
}

/// Player combat state: loadout, current action, aim, recoil, and breath.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CombatState {
    pub slots: [WeaponState; 2],
    pub active: u8,
    pub action: Action,
    /// 0 = hip, 1 = fully aimed down sights.
    pub ads: f32,
    /// Seconds until the weapon can fire again.
    pub cooldown: f32,
    /// Seconds before firing is allowed after a sprint ends.
    pub sprint_out: f32,
    /// Consecutive shots in the current spray, indexes the recoil pattern.
    pub burst: u16,
    pub since_shot: f32,
    /// Aim offset from recoil, in radians: `x` pitch up, `y` yaw right.
    pub recoil: Vec2,
    /// Total shots fired; seeds spread, pellets, and jitter deterministically.
    pub shots_fired: u32,
    pub fire_held: bool,
    /// Phase of the scope sway, in seconds.
    pub sway_time: f32,
    /// Breath left for steadying a scope, 0 to 1.
    pub breath: f32,
    /// Seconds of heavy sway left after running out of breath.
    pub winded: f32,
    pub holding_breath: bool,
    /// Reload was pressed while working the bolt or pump; it starts once the cycle ends.
    pub reload_queued: bool,
}

impl CombatState {
    pub fn new(loadout: [WeaponId; 2]) -> Self {
        Self {
            slots: loadout.map(WeaponState::full),
            active: 0,
            action: Action::Raising {
                remaining: loadout[0].def().raise_time,
            },
            ads: 0.0,
            cooldown: 0.0,
            sprint_out: 0.0,
            burst: 0,
            since_shot: f32::MAX,
            recoil: Vec2::ZERO,
            shots_fired: 0,
            fire_held: false,
            sway_time: 0.0,
            breath: 1.0,
            winded: 0.0,
            holding_breath: false,
            reload_queued: false,
        }
    }

    pub fn weapon(&self) -> &WeaponState {
        &self.slots[usize::from(self.active)]
    }

    pub fn def(&self) -> &'static WeaponDef {
        self.weapon().id.def()
    }

    /// Current spread cone half-angle in degrees, for HUD crosshairs.
    pub fn spread_degrees(&self, ctx: &CombatContext) -> f32 {
        let spread = &self.def().spread;
        let mut hip =
            spread.hip + (spread.hip_moving - spread.hip) * ctx.move_ratio.clamp(0.0, 1.0);
        if ctx.airborne {
            hip = hip.max(spread.air);
        }
        hip + (spread.ads - hip) * self.ads
    }

    pub fn is_reloading(&self) -> bool {
        matches!(
            self.action,
            Action::Reloading { .. } | Action::Loading { .. }
        )
    }

    /// Demo cheat: fills every carried weapon's magazine and reserve to that
    /// weapon's maximum.
    ///
    /// A reload in progress (including a shell-by-shell one) ends, since it is
    /// no longer needed. A weapon switch or a bolt or pump cycle in progress
    /// continues. If the active weapon's chamber was empty (its last round was
    /// fired, so no cycle followed), a bolt or pump weapon works its action
    /// before it can fire, so the refill never skips a cycle.
    ///
    /// This runs outside [`step`]. When netcode lands it should become a
    /// server-side debug command, not client input.
    pub fn refill_ammo(&mut self, events: &mut Vec<CombatEvent>) {
        let chamber_empty = self.weapon().magazine == 0;
        for slot in &mut self.slots {
            *slot = WeaponState::full(slot.id);
        }
        self.reload_queued = false;
        if self.is_reloading() || matches!(self.action, Action::Ready) {
            let cycle_time = self.def().cycle_time;
            self.action = if chamber_empty && cycle_time > 0.0 {
                events.push(CombatEvent::Cycled {
                    weapon: self.weapon().id,
                });
                Action::Cycling {
                    remaining: cycle_time,
                }
            } else {
                Action::Ready
            };
        }
        events.push(CombatEvent::PickedUp {
            weapon: self.weapon().id,
        });
    }

    /// How far into a scope the player is: 0 unless the weapon has one.
    pub fn scope_amount(&self) -> f32 {
        if self.def().scope.is_some() {
            self.ads
        } else {
            0.0
        }
    }

    /// Scope sway in radians (`x` pitch up, `y` yaw right). The camera and the
    /// shot both use it, so the reticle always shows where the round goes.
    pub fn scope_sway(&self) -> Vec2 {
        let Some(scope) = self.def().scope else {
            return Vec2::ZERO;
        };
        let breath = if self.holding_breath {
            0.1
        } else if self.winded > 0.0 {
            1.8
        } else {
            1.0
        };
        let t = self.sway_time;
        let pattern = Vec2::new(
            (t * 1.7).sin() * 0.6 + (t * 0.9 + 0.4).sin() * 0.4,
            (t * 1.1 + 1.0).sin() * 0.7 + (t * 0.6).cos() * 0.3,
        );
        pattern * scope.sway.to_radians() * breath * self.ads
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CombatInput {
    pub fire: bool,
    pub aim: bool,
    /// Reload was pressed this tick.
    pub reload: bool,
    /// Requested slot (0 or 1), if any.
    pub switch_to: Option<u8>,
    /// Held to steady a scope.
    pub hold_breath: bool,
    /// Take this weapon from a pickup or weapon rack this tick.
    pub pickup: Option<WeaponId>,
}

/// Movement facts the weapon reacts to.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CombatContext {
    pub sprinting: bool,
    pub airborne: bool,
    /// Horizontal speed relative to walk speed.
    pub move_ratio: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CombatEvent {
    /// A shot left the barrel. `aim_offset` (radians, `x` pitch up, `y` yaw right)
    /// is recoil, spread, and sway relative to the view direction. Each of the
    /// weapon's pellets adds [`pellet_offset`]`(weapon, shot, i)` to it.
    Fired {
        weapon: WeaponId,
        aim_offset: Vec2,
        shot: u32,
    },
    DryFire {
        weapon: WeaponId,
    },
    ReloadStarted {
        weapon: WeaponId,
        empty: bool,
    },
    /// One round went in during a round-by-round reload.
    RoundLoaded {
        weapon: WeaponId,
    },
    ReloadFinished {
        weapon: WeaponId,
    },
    /// The bolt or pump is being worked.
    Cycled {
        weapon: WeaponId,
    },
    /// The previous weapon is put away and `weapon` starts coming up.
    Switched {
        weapon: WeaponId,
    },
    /// A weapon was taken from a pickup, or its ammo was refilled.
    PickedUp {
        weapon: WeaponId,
    },
}

/// Offset of one pellet from the shot direction, in radians.
pub fn pellet_offset(weapon: WeaponId, shot: u32, pellet: u8) -> Vec2 {
    let def = weapon.def();
    if def.pellets <= 1 {
        return Vec2::ZERO;
    }
    let cone = def.pellet_spread.to_radians();
    let stream = 16 + u32::from(pellet) * 2;
    let angle = rng::unit(shot, stream) * TAU;
    let radius = rng::unit(shot, stream + 1).sqrt() * cone;
    Vec2::new(angle.sin(), angle.cos()) * radius
}

/// Advances combat by one tick.
pub fn step(
    state: &mut CombatState,
    input: &CombatInput,
    ctx: &CombatContext,
    dt: f32,
    events: &mut Vec<CombatEvent>,
) {
    let was_firing = state.fire_held;
    let fire_pressed = input.fire && !was_firing;
    state.fire_held = input.fire;
    // During a spray a slightly negative cooldown carries into the next shot so the
    // fire rate stays exact even when the interval isn't a whole number of ticks.
    state.cooldown -= dt;
    if !was_firing {
        state.cooldown = state.cooldown.max(0.0);
    }
    state.since_shot += dt;

    if let Some(weapon) = input.pickup {
        pick_up(state, weapon, events);
    }

    if ctx.sprinting {
        state.sprint_out = state.def().sprint_out_time;
        state.reload_queued = false;
        if state.is_reloading() {
            // Classic rule: sprinting interrupts a reload; rounds already loaded stay.
            state.action = Action::Ready;
        }
    } else {
        state.sprint_out = (state.sprint_out - dt).max(0.0);
    }

    request_switch(state, input);
    advance_action(state, dt, events);

    let def = state.def();
    // A reload can't cut a bolt or pump cycle short: it waits for the cycle to end.
    if input.reload && !ctx.sprinting {
        if matches!(state.action, Action::Cycling { .. }) {
            state.reload_queued = true;
        } else {
            try_start_reload(state, events);
        }
    }
    if state.reload_queued && matches!(state.action, Action::Ready) {
        state.reload_queued = false;
        try_start_reload(state, events);
    }

    let trigger = match def.fire_mode {
        FireMode::Auto => input.fire,
        FireMode::Semi => fire_pressed,
    };
    // Firing with a round loaded interrupts a shell-by-shell reload.
    if trigger && matches!(state.action, Action::Loading { .. }) && state.weapon().magazine > 0 {
        state.action = Action::Ready;
    }

    let aim_allowed = matches!(state.action, Action::Ready | Action::Cycling { .. });
    let can_aim = input.aim && !ctx.sprinting && aim_allowed;
    let ads_step = dt / def.ads_time;
    state.ads = if can_aim {
        (state.ads + ads_step).min(1.0)
    } else {
        (state.ads - ads_step).max(0.0)
    };
    update_breath(state, input.hold_breath, dt);

    let ready = matches!(state.action, Action::Ready) && !ctx.sprinting && state.sprint_out <= 0.0;
    if trigger && ready && state.cooldown <= 0.0 {
        if state.weapon().magazine == 0 {
            if fire_pressed {
                events.push(CombatEvent::DryFire {
                    weapon: state.weapon().id,
                });
            }
            try_start_reload(state, events);
        } else {
            fire(state, ctx, events);
        }
    } else if state.weapon().magazine == 0
        && matches!(state.action, Action::Ready)
        && state.cooldown <= 0.0
        && !ctx.sprinting
    {
        // Auto-reload once the last round's cooldown has passed.
        try_start_reload(state, events);
    }

    recover(state, dt);
}

fn pick_up(state: &mut CombatState, weapon: WeaponId, events: &mut Vec<CombatEvent>) {
    let def = weapon.def();
    if let Some(slot) = state.slots.iter_mut().find(|s| s.id == weapon) {
        slot.reserve = slot.reserve.max(def.reserve);
        events.push(CombatEvent::PickedUp { weapon });
        return;
    }
    let slot = def.slot.index();
    state.slots[usize::from(slot)] = WeaponState::full(weapon);
    state.active = slot;
    state.burst = 0;
    state.cooldown = 0.0;
    state.ads = 0.0;
    state.reload_queued = false;
    state.action = Action::Raising {
        remaining: def.raise_time,
    };
    events.push(CombatEvent::PickedUp { weapon });
    events.push(CombatEvent::Switched { weapon });
}

fn request_switch(state: &mut CombatState, input: &CombatInput) {
    let Some(to) = input.switch_to else { return };
    if usize::from(to) >= state.slots.len() {
        return;
    }
    let already_going = match state.action {
        Action::Lowering { to: target, .. } => target == to,
        _ => to == state.active,
    };
    if already_going {
        return;
    }
    state.action = Action::Lowering {
        remaining: state.def().lower_time,
        to,
    };
}

fn advance_action(state: &mut CombatState, dt: f32, events: &mut Vec<CombatEvent>) {
    match state.action {
        Action::Ready => {}
        Action::Reloading { remaining, empty } => {
            let remaining = remaining - dt;
            if remaining > 0.0 {
                state.action = Action::Reloading { remaining, empty };
                return;
            }
            let slot = &mut state.slots[usize::from(state.active)];
            let take = slot
                .id
                .def()
                .magazine
                .saturating_sub(slot.magazine)
                .min(slot.reserve);
            slot.magazine += take;
            slot.reserve -= take;
            state.action = Action::Ready;
            events.push(CombatEvent::ReloadFinished { weapon: slot.id });
        }
        Action::Loading { remaining, stage } => {
            let remaining = remaining - dt;
            if remaining > 0.0 {
                state.action = Action::Loading { remaining, stage };
                return;
            }
            let slot = &mut state.slots[usize::from(state.active)];
            let def = slot.id.def();
            let Reload::PerRound { per_round, end, .. } = def.reload else {
                state.action = Action::Ready;
                return;
            };
            // Carry the overshoot into the next stage so timing stays exact.
            state.action = match stage {
                LoadStage::Opening => Action::Loading {
                    remaining: remaining + per_round,
                    stage: LoadStage::Round,
                },
                LoadStage::Round => {
                    if slot.reserve > 0 && slot.magazine < def.magazine {
                        slot.magazine += 1;
                        slot.reserve -= 1;
                        events.push(CombatEvent::RoundLoaded { weapon: slot.id });
                    }
                    if slot.magazine >= def.magazine || slot.reserve == 0 {
                        Action::Loading {
                            remaining: remaining + end,
                            stage: LoadStage::Closing,
                        }
                    } else {
                        Action::Loading {
                            remaining: remaining + per_round,
                            stage: LoadStage::Round,
                        }
                    }
                }
                LoadStage::Closing => {
                    events.push(CombatEvent::ReloadFinished { weapon: slot.id });
                    Action::Ready
                }
            };
        }
        Action::Cycling { remaining } => {
            let remaining = remaining - dt;
            state.action = if remaining > 0.0 {
                Action::Cycling { remaining }
            } else {
                Action::Ready
            };
        }
        Action::Lowering { remaining, to } => {
            let remaining = remaining - dt;
            if remaining > 0.0 {
                state.action = Action::Lowering { remaining, to };
                return;
            }
            state.active = to;
            state.burst = 0;
            state.cooldown = 0.0;
            state.action = Action::Raising {
                remaining: state.def().raise_time,
            };
            events.push(CombatEvent::Switched {
                weapon: state.weapon().id,
            });
        }
        Action::Raising { remaining } => {
            let remaining = remaining - dt;
            state.action = if remaining > 0.0 {
                Action::Raising { remaining }
            } else {
                Action::Ready
            };
        }
    }
}

fn try_start_reload(state: &mut CombatState, events: &mut Vec<CombatEvent>) {
    let weapon = *state.weapon();
    let def = weapon.id.def();
    if !matches!(state.action, Action::Ready)
        || weapon.magazine >= def.magazine
        || weapon.reserve == 0
    {
        return;
    }
    let empty = weapon.magazine == 0;
    state.action = match def.reload {
        Reload::Magazine { time, empty_time } => Action::Reloading {
            remaining: if empty { empty_time } else { time },
            empty,
        },
        Reload::PerRound { start, .. } => Action::Loading {
            remaining: start,
            stage: LoadStage::Opening,
        },
    };
    events.push(CombatEvent::ReloadStarted {
        weapon: weapon.id,
        empty,
    });
}

fn update_breath(state: &mut CombatState, hold: bool, dt: f32) {
    state.winded = (state.winded - dt).max(0.0);
    let scope = state.def().scope.filter(|_| state.ads > 0.5);
    state.holding_breath = false;
    if let Some(scope) = scope {
        state.sway_time += dt;
        if hold && state.winded <= 0.0 && state.breath > 0.0 {
            state.holding_breath = true;
            state.breath -= dt / scope.hold_breath;
            if state.breath <= 0.0 {
                state.breath = 0.0;
                state.winded = scope.breath_recovery;
            }
            return;
        }
    }
    if state.winded <= 0.0 && !hold {
        // Breath comes back in about two seconds once the button is let go.
        state.breath = (state.breath + dt * 0.5).min(1.0);
    }
}

fn fire(state: &mut CombatState, ctx: &CombatContext, events: &mut Vec<CombatEvent>) {
    let def = state.def();
    let seed = state.shots_fired;

    // Spread: uniform point in a cone, from two hashed values.
    let cone = state.spread_degrees(ctx).to_radians();
    let angle = rng::unit(seed, 1) * TAU;
    let radius = rng::unit(seed, 2).sqrt() * cone;
    let spread = Vec2::new(angle.sin(), angle.cos()) * radius;
    events.push(CombatEvent::Fired {
        weapon: state.weapon().id,
        aim_offset: state.recoil + spread + state.scope_sway(),
        shot: seed,
    });

    let recoil = &def.recoil;
    let base = recoil.pattern[usize::from(state.burst).min(recoil.pattern.len() - 1)];
    let jitter = Vec2::new(
        rng::signed(seed, 3) * recoil.jitter.x,
        rng::signed(seed, 4) * recoil.jitter.y,
    );
    let scale = 1.0 + (recoil.ads_scale - 1.0) * state.ads;
    state.recoil += Vec2::new(
        (base.x + jitter.x).to_radians(),
        (base.y + jitter.y).to_radians(),
    ) * scale;

    let slot = &mut state.slots[usize::from(state.active)];
    slot.magazine -= 1;
    // Accumulate so the average fire rate is exact even when the interval is not a whole number of ticks.
    state.cooldown += def.fire_interval();
    state.burst = state.burst.saturating_add(1);
    state.since_shot = 0.0;
    state.shots_fired = state.shots_fired.wrapping_add(1);

    if def.cycle_time > 0.0 && slot.magazine > 0 {
        state.action = Action::Cycling {
            remaining: def.cycle_time,
        };
        events.push(CombatEvent::Cycled { weapon: slot.id });
    }
}

fn recover(state: &mut CombatState, dt: f32) {
    let recoil = &state.def().recoil;
    if state.since_shot < state.def().fire_interval() + recoil.recovery_delay {
        return;
    }
    state.burst = 0;
    state.recoil *= (-recoil.recovery_rate * dt).exp();
    if state.recoil.length_squared() < 1e-8 {
        state.recoil = Vec2::ZERO;
    }
}

#[cfg(test)]
mod tests;
