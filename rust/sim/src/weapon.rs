//! Weapon definitions and the per-tick combat state machine.
//!
//! Classic Call of Duty rules: firing or aiming ends a sprint, a short
//! "sprint-out" delay follows before the first shot, sprinting cancels a reload,
//! and the view kicks along a fixed recoil pattern that recovers toward the
//! original aim once the trigger is released.
//!
//! Like movement, this is deterministic: spread and recoil jitter are hashed
//! from the player's shot counter, so client prediction and the server agree.

use std::f32::consts::TAU;

use glam::Vec2;

use crate::rng;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WeaponId {
    #[default]
    M4,
    Pistol,
}

impl WeaponId {
    pub const ALL: [WeaponId; 2] = [WeaponId::M4, WeaponId::Pistol];

    pub fn def(self) -> &'static WeaponDef {
        match self {
            WeaponId::M4 => &M4,
            WeaponId::Pistol => &PISTOL,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireMode {
    Auto,
    Semi,
}

/// Recoil kicks are in degrees: `x` is pitch up, `y` is yaw right.
#[derive(Debug)]
pub struct RecoilDef {
    /// Kick per consecutive shot; the last entry repeats.
    pub pattern: &'static [Vec2],
    /// Random variation added to each kick (± degrees).
    pub jitter: Vec2,
    /// Kick multiplier while fully aimed down sights.
    pub ads_scale: f32,
    /// Seconds without firing before the view starts to settle.
    pub recovery_delay: f32,
    /// How quickly the view returns to the original aim, in 1/seconds.
    pub recovery_rate: f32,
}

#[derive(Debug)]
pub struct WeaponDef {
    pub id: WeaponId,
    pub name: &'static str,
    pub fire_mode: FireMode,
    pub rounds_per_minute: f32,
    pub magazine: u16,
    pub reserve: u16,
    pub damage_near: f32,
    pub damage_far: f32,
    /// Full damage up to this distance (metres)...
    pub falloff_start: f32,
    /// ...then linear falloff to `damage_far` at this distance.
    pub falloff_end: f32,
    pub headshot_multiplier: f32,
    pub range: f32,
    pub reload_time: f32,
    pub reload_empty_time: f32,
    pub ads_time: f32,
    pub sprint_out_time: f32,
    pub raise_time: f32,
    pub lower_time: f32,
    /// Hip-fire cone half-angles in degrees.
    pub hip_spread: f32,
    pub hip_spread_moving: f32,
    pub air_spread: f32,
    pub ads_spread: f32,
    /// World FOV multiplier when fully aimed (lower zooms in).
    pub ads_fov_scale: f32,
    pub ads_move_scale: f32,
    pub move_scale: f32,
    pub recoil: RecoilDef,
}

impl WeaponDef {
    pub fn fire_interval(&self) -> f32 {
        60.0 / self.rounds_per_minute
    }

    /// Body-shot damage at a distance.
    pub fn damage_at(&self, distance: f32) -> f32 {
        if distance <= self.falloff_start {
            return self.damage_near;
        }
        if distance >= self.falloff_end {
            return self.damage_far;
        }
        let t = (distance - self.falloff_start) / (self.falloff_end - self.falloff_start);
        self.damage_near + (self.damage_far - self.damage_near) * t
    }
}

const fn kick(pitch: f32, yaw: f32) -> Vec2 {
    Vec2::new(pitch, yaw)
}

/// M4A1-style carbine: 4-shot kill up close (30 dmg), 5 at range.
pub static M4: WeaponDef = WeaponDef {
    id: WeaponId::M4,
    name: "M4A1",
    fire_mode: FireMode::Auto,
    rounds_per_minute: 750.0,
    magazine: 30,
    reserve: 120,
    damage_near: 30.0,
    damage_far: 21.0,
    falloff_start: 25.0,
    falloff_end: 50.0,
    headshot_multiplier: 1.4,
    range: 400.0,
    reload_time: 2.1,
    reload_empty_time: 2.6,
    ads_time: 0.25,
    sprint_out_time: 0.22,
    raise_time: 0.45,
    lower_time: 0.3,
    hip_spread: 2.2,
    hip_spread_moving: 4.0,
    air_spread: 6.0,
    ads_spread: 0.0,
    ads_fov_scale: 0.8,
    ads_move_scale: 0.6,
    move_scale: 1.0,
    recoil: RecoilDef {
        pattern: &[
            kick(0.55, 0.05),
            kick(0.5, 0.1),
            kick(0.45, 0.15),
            kick(0.42, -0.05),
            kick(0.4, -0.2),
            kick(0.38, -0.15),
            kick(0.36, 0.1),
            kick(0.35, 0.22),
            kick(0.34, 0.12),
            kick(0.33, -0.1),
            kick(0.32, -0.2),
            kick(0.32, 0.0),
        ],
        jitter: Vec2::new(0.06, 0.1),
        ads_scale: 0.75,
        recovery_delay: 0.1,
        recovery_rate: 7.0,
    },
};

/// M1911-style sidearm: semi-automatic, fast handling.
pub static PISTOL: WeaponDef = WeaponDef {
    id: WeaponId::Pistol,
    name: "M1911",
    fire_mode: FireMode::Semi,
    rounds_per_minute: 400.0,
    magazine: 8,
    reserve: 32,
    damage_near: 40.0,
    damage_far: 25.0,
    falloff_start: 10.0,
    falloff_end: 30.0,
    headshot_multiplier: 1.4,
    range: 200.0,
    reload_time: 1.5,
    reload_empty_time: 1.9,
    ads_time: 0.18,
    sprint_out_time: 0.15,
    raise_time: 0.3,
    lower_time: 0.25,
    hip_spread: 2.0,
    hip_spread_moving: 3.0,
    air_spread: 5.0,
    ads_spread: 0.1,
    ads_fov_scale: 0.9,
    ads_move_scale: 0.75,
    move_scale: 1.05,
    recoil: RecoilDef {
        pattern: &[kick(1.4, 0.1)],
        jitter: Vec2::new(0.25, 0.35),
        ads_scale: 0.8,
        recovery_delay: 0.06,
        recovery_rate: 9.0,
    },
};

/// Ammo and timers for one carried weapon.
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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Action {
    #[default]
    Ready,
    /// Reloading; seconds remaining and whether the magazine was empty.
    Reloading {
        remaining: f32,
        empty: bool,
    },
    /// Putting the current weapon away before switching to `to`.
    Lowering {
        remaining: f32,
        to: u8,
    },
    Raising {
        remaining: f32,
    },
}

/// Player combat state: loadout, current action, aim, and recoil.
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
    /// Total shots fired; seeds spread and jitter deterministically.
    pub shots_fired: u32,
    pub fire_held: bool,
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
        let def = self.def();
        let mut hip = def.hip_spread
            + (def.hip_spread_moving - def.hip_spread) * ctx.move_ratio.clamp(0.0, 1.0);
        if ctx.airborne {
            hip = hip.max(def.air_spread);
        }
        hip + (def.ads_spread - hip) * self.ads
    }

    pub fn is_reloading(&self) -> bool {
        matches!(self.action, Action::Reloading { .. })
    }

    /// Demo cheat: fills every carried weapon's magazine and reserve to the
    /// maximum, and ends any reload in progress since it is no longer needed.
    pub fn refill_ammo(&mut self) {
        for slot in &mut self.slots {
            *slot = WeaponState::full(slot.id);
        }
        if self.is_reloading() {
            self.action = Action::Ready;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CombatInput {
    pub fire: bool,
    pub aim: bool,
    pub reload: bool,
    /// Requested slot (0 or 1), if any.
    pub switch_to: Option<u8>,
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
    /// A round left the barrel. `aim_offset` (radians, `x` pitch up, `y` yaw right)
    /// is recoil plus spread, relative to the view direction.
    Fired {
        weapon: WeaponId,
        aim_offset: Vec2,
    },
    DryFire {
        weapon: WeaponId,
    },
    ReloadStarted {
        weapon: WeaponId,
        empty: bool,
    },
    ReloadFinished {
        weapon: WeaponId,
    },
    /// The previous weapon is put away and `weapon` starts coming up.
    Switched {
        weapon: WeaponId,
    },
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

    if ctx.sprinting {
        state.sprint_out = state.def().sprint_out_time;
        if state.is_reloading() {
            // Classic rule: sprinting interrupts a reload; the magazine is unchanged.
            state.action = Action::Ready;
        }
    } else {
        state.sprint_out = (state.sprint_out - dt).max(0.0);
    }

    request_switch(state, input);
    advance_action(state, dt, events);

    let def = state.def();
    if input.reload && !ctx.sprinting {
        try_start_reload(state, events);
    }

    let can_aim = input.aim && !ctx.sprinting && matches!(state.action, Action::Ready);
    let ads_step = dt / def.ads_time;
    state.ads = if can_aim {
        (state.ads + ads_step).min(1.0)
    } else {
        (state.ads - ads_step).max(0.0)
    };

    let trigger = match def.fire_mode {
        FireMode::Auto => input.fire,
        FireMode::Semi => fire_pressed,
    };
    let ready = matches!(state.action, Action::Ready) && !ctx.sprinting && state.sprint_out <= 0.0;
    if trigger && ready && state.cooldown <= 0.0 {
        if state.weapon().magazine == 0 {
            if fire_pressed {
                events.push(CombatEvent::DryFire { weapon: def.id });
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
            let take = (slot.id.def().magazine - slot.magazine).min(slot.reserve);
            slot.magazine += take;
            slot.reserve -= take;
            state.action = Action::Ready;
            events.push(CombatEvent::ReloadFinished { weapon: slot.id });
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
    let remaining = if empty {
        def.reload_empty_time
    } else {
        def.reload_time
    };
    state.action = Action::Reloading { remaining, empty };
    events.push(CombatEvent::ReloadStarted {
        weapon: weapon.id,
        empty,
    });
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
        weapon: def.id,
        aim_offset: state.recoil + spread,
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
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn ready() -> CombatState {
        let mut s = CombatState::new([WeaponId::M4, WeaponId::Pistol]);
        s.action = Action::Ready;
        s
    }

    fn run(
        state: &mut CombatState,
        input: CombatInput,
        ctx: CombatContext,
        ticks: usize,
    ) -> Vec<CombatEvent> {
        let mut events = Vec::new();
        for _ in 0..ticks {
            step(state, &input, &ctx, DT, &mut events);
        }
        events
    }

    fn shots(events: &[CombatEvent]) -> usize {
        events
            .iter()
            .filter(|e| matches!(e, CombatEvent::Fired { .. }))
            .count()
    }

    #[test]
    fn full_auto_matches_fire_rate() {
        let mut s = ready();
        let events = run(
            &mut s,
            CombatInput {
                fire: true,
                ..Default::default()
            },
            CombatContext::default(),
            120,
        );
        // 750 RPM for 2 s is 25 rounds.
        assert_eq!(shots(&events), 25);
        assert_eq!(s.weapon().magazine, 5);
    }

    #[test]
    fn refill_ammo_fills_every_slot_and_ends_reload() {
        let mut s = ready();
        s.slots[0].magazine = 3;
        s.slots[0].reserve = 0;
        s.slots[1].magazine = 0;
        s.slots[1].reserve = 4;
        s.action = Action::Reloading {
            remaining: 1.0,
            empty: false,
        };
        s.refill_ammo();
        assert_eq!(s.slots[0], WeaponState::full(WeaponId::M4));
        assert_eq!(s.slots[1], WeaponState::full(WeaponId::Pistol));
        assert_eq!(s.action, Action::Ready);

        // A weapon switch in progress is left alone.
        s.action = Action::Lowering {
            remaining: 0.2,
            to: 1,
        };
        s.refill_ammo();
        assert!(matches!(s.action, Action::Lowering { to: 1, .. }));
    }

    #[test]
    fn semi_auto_needs_separate_presses() {
        let mut s = ready();
        s.active = 1;
        let held = run(
            &mut s,
            CombatInput {
                fire: true,
                ..Default::default()
            },
            CombatContext::default(),
            60,
        );
        assert_eq!(shots(&held), 1);
        let mut events = Vec::new();
        for i in 0..60 {
            let input = CombatInput {
                fire: i % 12 == 6,
                ..Default::default()
            };
            step(&mut s, &input, &CombatContext::default(), DT, &mut events);
        }
        assert_eq!(shots(&events), 5);
    }

    #[test]
    fn empty_magazine_auto_reloads_and_refills() {
        let mut s = ready();
        let events = run(
            &mut s,
            CombatInput {
                fire: true,
                ..Default::default()
            },
            CombatContext::default(),
            200,
        );
        assert!(events.contains(&CombatEvent::ReloadStarted {
            weapon: WeaponId::M4,
            empty: true
        }));
        let events = run(
            &mut s,
            CombatInput::default(),
            CombatContext::default(),
            200,
        );
        assert!(events.contains(&CombatEvent::ReloadFinished {
            weapon: WeaponId::M4
        }));
        assert_eq!(s.weapon().magazine, 30);
        assert_eq!(s.weapon().reserve, 90);
    }

    #[test]
    fn sprinting_blocks_fire_then_sprint_out_delays_it() {
        let mut s = ready();
        let sprint = CombatContext {
            sprinting: true,
            ..Default::default()
        };
        let events = run(
            &mut s,
            CombatInput {
                fire: true,
                ..Default::default()
            },
            sprint,
            30,
        );
        assert_eq!(shots(&events), 0);

        let fire = CombatInput {
            fire: true,
            ..Default::default()
        };
        // 0.22 s sprint-out is 13 ticks at 60 Hz.
        let events = run(&mut s, fire, CombatContext::default(), 12);
        assert_eq!(shots(&events), 0);
        let events = run(&mut s, fire, CombatContext::default(), 2);
        assert_eq!(shots(&events), 1);
    }

    #[test]
    fn sprinting_cancels_reload() {
        let mut s = ready();
        s.slots[0].magazine = 10;
        run(
            &mut s,
            CombatInput {
                reload: true,
                ..Default::default()
            },
            CombatContext::default(),
            30,
        );
        assert!(s.is_reloading());
        run(
            &mut s,
            CombatInput::default(),
            CombatContext {
                sprinting: true,
                ..Default::default()
            },
            1,
        );
        assert!(!s.is_reloading());
        assert_eq!(s.weapon().magazine, 10);
    }

    #[test]
    fn recoil_climbs_then_recovers() {
        let mut s = ready();
        run(
            &mut s,
            CombatInput {
                fire: true,
                ..Default::default()
            },
            CombatContext::default(),
            30,
        );
        let peak = s.recoil;
        assert!(
            peak.x > 3f32.to_radians(),
            "view climbs during a spray: {peak:?}"
        );
        run(&mut s, CombatInput::default(), CombatContext::default(), 90);
        assert!(s.recoil.length() < peak.length() * 0.01);
    }

    #[test]
    fn ads_tightens_spread_and_takes_ads_time() {
        let mut s = ready();
        let ctx = CombatContext {
            move_ratio: 1.0,
            ..Default::default()
        };
        assert!(s.spread_degrees(&ctx) > 3.0);
        run(
            &mut s,
            CombatInput {
                aim: true,
                ..Default::default()
            },
            ctx,
            10,
        );
        assert!(s.ads > 0.5 && s.ads < 1.0);
        run(
            &mut s,
            CombatInput {
                aim: true,
                ..Default::default()
            },
            ctx,
            10,
        );
        assert_eq!(s.ads, 1.0);
        assert_eq!(s.spread_degrees(&ctx), 0.0);
    }

    #[test]
    fn switching_lowers_then_raises() {
        let mut s = ready();
        let events = run(
            &mut s,
            CombatInput {
                switch_to: Some(1),
                ..Default::default()
            },
            CombatContext::default(),
            20,
        );
        assert!(events.contains(&CombatEvent::Switched {
            weapon: WeaponId::Pistol
        }));
        assert_eq!(s.active, 1);
        run(&mut s, CombatInput::default(), CombatContext::default(), 20);
        assert_eq!(s.action, Action::Ready);
    }

    #[test]
    fn damage_falls_off_linearly() {
        assert_eq!(M4.damage_at(10.0), 30.0);
        assert_eq!(M4.damage_at(37.5), 25.5);
        assert_eq!(M4.damage_at(100.0), 21.0);
    }

    #[test]
    fn spread_is_deterministic() {
        let simulate = || {
            let mut s = ready();
            run(
                &mut s,
                CombatInput {
                    fire: true,
                    ..Default::default()
                },
                CombatContext::default(),
                60,
            )
        };
        assert_eq!(simulate(), simulate());
    }
}
