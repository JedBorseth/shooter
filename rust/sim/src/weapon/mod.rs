//! Weapons: definitions (loaded from data files) and the per-tick combat state machine.
//!
//! Modern Call of Duty rules: firing or aiming ends a sprint, and a short
//! "sprint-out" delay follows before the first shot. A reload carries on
//! through a sprint and can be aimed through; its rounds count once the fresh
//! magazine seats, part-way through. Bolt and pump weapons cycle after each
//! shot (and finish a cycle cut short by a switch when drawn again); shotguns
//! load shell by shell and firing interrupts the reload. Every weapon sways a
//! little when aimed; scopes sway more, and holding breath steadies them.
//!
//! Recoil kicks the view along a fixed per-weapon pattern with hashed
//! variation. Most of each kick climbs and stays (the player pulls down to
//! hold a spray on target); a smaller snap settles back after the shot. Both
//! are aim, so the sights and the screen centre always show where the next
//! round goes. Hip fire blooms with each shot. Crouching steadies the kick
//! and the hip cone.
//!
//! Like movement, this is deterministic: spread, pellets, and recoil jitter are
//! hashed from the player's shot counter, and scope sway is a function of
//! accumulated tick time, so client prediction and the server agree.

mod def;
pub mod ttk;

use std::f32::consts::TAU;

use glam::Vec2;

pub use def::{
    Attachment, Attachments, Damage, FingerFlex, FireMode, HandDef, HandsDef, LoadError, RecoilDef,
    Reload, ScopeDef, Slot, Spread, WeaponDef, WeaponId, builtin, install, parse, parse_all, table,
};

use crate::{equipment, fmath, rng};

/// Ammo for one carried weapon.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponState {
    pub id: WeaponId,
    pub magazine: u16,
    pub reserve: u16,
    /// The chamber is empty though rounds are loaded: a bolt or pump cycle,
    /// or the charge after an empty reload, was cut short by a switch. The
    /// weapon works its action when it is next drawn.
    pub needs_cycle: bool,
    /// False for an empty slot (Zombies starts with only a sidearm). `id`
    /// then means nothing.
    pub carried: bool,
    /// Not drawn yet this game or round, or just picked up: the next draw is
    /// a first raise ([`Raise::First`]), which works the action.
    pub fresh: bool,
}

impl WeaponState {
    pub fn full(id: WeaponId) -> Self {
        let def = id.def();
        Self {
            id,
            magazine: def.magazine,
            reserve: def.reserve,
            needs_cycle: false,
            carried: true,
            fresh: false,
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
        kind: Raise,
    },
    /// A knife strike ([`crate::melee`]): seconds until the weapon is back
    /// up, seconds until the strike lands (negative once it has), and whether
    /// it lunged.
    Meleeing {
        remaining: f32,
        strike_in: f32,
        lunge: bool,
    },
    /// A grenade in hand ([`crate::equipment`]), weapon down: seconds since
    /// the pin was pulled (the fuse has burned that long until the throw),
    /// and once the button is let go, seconds until it leaves the hand. That
    /// goes negative through the follow-through, and the weapon is back up
    /// at `-equipment::RECOVERY`.
    Throwing {
        kind: equipment::Kind,
        cooked: f32,
        release_in: Option<f32>,
    },
}

/// Which raise brings a weapon up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Raise {
    /// An ordinary draw: switching, or coming back from a cancelled switch.
    #[default]
    Draw,
    /// The weapon's first draw of a game or round, or just picked up
    /// (`WeaponDef::first_raise_time`): the draw ends by working the action,
    /// as Call of Duty's first raise does.
    First,
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
    /// Aim offset from the recoil snap that settles back, in radians: `x`
    /// pitch up, `y` yaw right. The climb that stays is reported with each
    /// shot ([`CombatEvent::Fired`]) and turns the view itself.
    pub recoil: Vec2,
    /// Extra hip-fire cone from recent shots, in degrees.
    pub bloom: f32,
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
    /// Rounds left in the current burst (burst-fire weapons).
    pub burst_left: u8,
    /// Frag grenades carried.
    pub lethals: u8,
    /// The loadout's tactical, and how many are carried.
    pub tactical: equipment::Kind,
    pub tacticals: u8,
}

impl CombatState {
    pub fn new(loadout: [WeaponId; 2]) -> Self {
        Self {
            slots: loadout.map(WeaponState::full),
            active: 0,
            action: Action::Raising {
                remaining: loadout[0].def().raise_time,
                kind: Raise::Draw,
            },
            ads: 0.0,
            cooldown: 0.0,
            sprint_out: 0.0,
            burst: 0,
            since_shot: f32::MAX,
            recoil: Vec2::ZERO,
            bloom: 0.0,
            shots_fired: 0,
            fire_held: false,
            sway_time: 0.0,
            breath: 1.0,
            winded: 0.0,
            holding_breath: false,
            reload_queued: false,
            burst_left: 0,
            lethals: 1,
            tactical: equipment::Kind::Flash,
            tacticals: 1,
        }
    }

    /// This loadout carrying `count` frags.
    pub fn with_lethals(mut self, count: u8) -> Self {
        self.lethals = count;
        self
    }

    /// This loadout at the start of a game or round: every weapon carried
    /// comes up with a first raise the first time it is drawn, the one in
    /// hand now. Call of Duty plays the first raise at the start of a game
    /// or round and on a pickup, not on every respawn (Modern Warfare III,
    /// 2023, after players asked for it off respawns).
    pub fn first_draw(mut self) -> Self {
        for slot in &mut self.slots {
            slot.fresh = slot.carried;
        }
        if matches!(
            self.action,
            Action::Raising {
                kind: Raise::Draw,
                ..
            }
        ) {
            self.action = draw(&mut self);
        }
        self
    }

    /// This loadout coming back up from nothing in hand, as after a revive
    /// in Zombies: out of aim, the weapon in hand being drawn.
    pub fn raised(mut self) -> Self {
        self.ads = 0.0;
        self.burst_left = 0;
        self.action = draw(&mut self);
        self
    }

    /// This loadout carrying `count` of the tactical `kind`.
    pub fn with_tacticals(mut self, kind: equipment::Kind, count: u8) -> Self {
        self.tactical = kind;
        self.tacticals = count;
        self
    }

    /// How many of `kind` are carried.
    pub fn carried(&self, kind: equipment::Kind) -> u8 {
        if kind.lethal() {
            self.lethals
        } else if kind == self.tactical {
            self.tacticals
        } else {
            0
        }
    }

    /// Whether a grenade of `kind` can come out now: one is carried, and
    /// the hands are free (not switching, knifing, or already throwing). It
    /// cuts a reload or a bolt or pump cycle short, as the knife does.
    pub fn can_throw(&self, kind: equipment::Kind) -> bool {
        self.carried(kind) > 0 && self.can_melee()
    }

    pub fn throwing(&self) -> bool {
        matches!(self.action, Action::Throwing { .. })
    }

    /// The grenade in hand with the pin out, if any: its kind and the
    /// seconds left on its fuse. A player who dies holding it drops it live.
    pub fn live_grenade(&self) -> Option<(equipment::Kind, f32)> {
        match self.action {
            Action::Throwing {
                kind,
                cooked,
                release_in,
            } if kind.cooks() && release_in.is_none_or(|left| left > 0.0) => {
                Some((kind, (kind.fuse() - cooked).max(0.0)))
            }
            _ => None,
        }
    }

    /// How far along a throw is, 0 to 1 ([`equipment::throw_progress`]),
    /// while one is.
    pub fn throw_progress(&self) -> Option<f32> {
        match self.action {
            Action::Throwing {
                cooked, release_in, ..
            } => Some(equipment::throw_progress(cooked, release_in)),
            _ => None,
        }
    }

    /// Empties `slot` (for example the primary, to start with only a
    /// sidearm) and draws the other weapon if that slot was in hand.
    pub fn without(mut self, slot: Slot) -> Self {
        let index = slot.index();
        self.slots[usize::from(index)].carried = false;
        if self.active == index {
            self.active = 1 - index;
            self.action = draw(&mut self);
        }
        self
    }

    /// Whether `weapon` is in one of the slots.
    pub fn carries(&self, weapon: WeaponId) -> bool {
        self.slots.iter().any(|s| s.carried && s.id == weapon)
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
        if ctx.crouched {
            hip *= CROUCH_HIP_SPREAD;
        }
        hip += self.bloom;
        if ctx.airborne {
            hip = hip.max(spread.air);
        }
        hip + (spread.ads - hip) * ads_ease(self.ads)
    }

    /// Whether a melee can start now: not while switching weapons or during
    /// another melee. A melee cuts a reload or a bolt or pump cycle short.
    pub fn can_melee(&self) -> bool {
        !matches!(
            self.action,
            Action::Lowering { .. }
                | Action::Raising { .. }
                | Action::Meleeing { .. }
                | Action::Throwing { .. }
        )
    }

    pub fn meleeing(&self) -> bool {
        matches!(self.action, Action::Meleeing { .. })
    }

    /// Reloading from an empty magazine (the old one is dropped, not stowed).
    pub fn is_empty_reload(&self) -> bool {
        matches!(self.action, Action::Reloading { empty: true, .. })
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
        for slot in self.slots.iter_mut().filter(|s| s.carried) {
            // A weapon not drawn yet keeps its first raise.
            *slot = WeaponState {
                fresh: slot.fresh,
                ..WeaponState::full(slot.id)
            };
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

    /// A loadout of `weapon` alone, fully loaded and being drawn, with the
    /// other slot empty (Gun Game).
    pub fn only(weapon: WeaponId) -> Self {
        let empty = match weapon.def().slot {
            Slot::Primary => Slot::Secondary,
            Slot::Secondary => Slot::Primary,
        };
        Self::new([weapon, weapon])
            .without(empty)
            .with_lethals(0)
            .with_tacticals(equipment::Kind::Flash, 0)
    }

    /// Takes away whatever the player carries and hands them `weapon` alone
    /// ([`CombatState::only`]), coming up as if just drawn. Gun Game's server
    /// does this when a kill moves a player up the ladder.
    ///
    /// The shot counter, breath, and scope sway carry on, so shot numbers stay
    /// unique and seeds never repeat. A held trigger stays held, so a
    /// semi-automatic weapon does not fire until it is pressed again.
    pub fn rearm(&mut self, weapon: WeaponId) {
        *self = Self {
            lethals: self.lethals,
            tactical: self.tactical,
            tacticals: self.tacticals,
            shots_fired: self.shots_fired,
            fire_held: self.fire_held,
            sway_time: self.sway_time,
            breath: self.breath,
            winded: self.winded,
            ..Self::only(weapon)
        };
    }

    /// Whether a magazine reload has seated its fresh magazine (its rounds
    /// count; what is left is the charge and re-shoulder).
    pub fn magazine_seated(&self) -> bool {
        let Action::Reloading { remaining, empty } = self.action else {
            return false;
        };
        let reload = self.def().reload;
        match (reload.magazine_time(empty), reload.mag_in()) {
            (Some(total), Some(mag_in)) => total - remaining >= mag_in,
            _ => false,
        }
    }

    /// Whether a magazine reload has reached `WeaponDef::reload_ready_at`:
    /// its rounds are in, a round is chambered, and firing may end it.
    pub fn reload_ready(&self) -> bool {
        let Action::Reloading { remaining, empty } = self.action else {
            return false;
        };
        let def = self.def();
        match (def.reload.magazine_time(empty), def.reload_ready_at(empty)) {
            (Some(total), Some(ready)) => total - remaining >= ready && self.magazine_seated(),
            _ => false,
        }
    }

    /// How far into a scope the player is: 0 unless the weapon has one.
    pub fn scope_amount(&self) -> f32 {
        if self.def().scope.is_some() {
            self.ads
        } else {
            0.0
        }
    }

    /// [`Self::scope_sway`] as a weapon mounted `mount` (0 to 1) on cover
    /// sways: steadied by the mount. The camera and the shot both use it.
    pub fn aimed_sway(&self, mount: f32) -> Vec2 {
        self.scope_sway() * crate::mount::sway_scale(mount)
    }

    /// Aimed sway in radians (`x` pitch up, `y` yaw right): a slow drift
    /// while aimed, larger through a scope. The camera and the shot both use
    /// it, so the sights always show where the round goes.
    pub fn scope_sway(&self) -> Vec2 {
        let def = self.def();
        let (amplitude, breath) = match def.scope {
            Some(scope) if scope.sway > 0.0 => {
                let breath = if self.holding_breath {
                    0.1
                } else if self.winded > 0.0 {
                    1.8
                } else {
                    1.0
                };
                (scope.sway, breath)
            }
            _ => (def.spread.ads_sway, 1.0),
        };
        let t = self.sway_time;
        // A slow wander plus a breath (0.3 Hz, mostly vertical, the size
        // of the wander), so the aimed picture is never frozen.
        let pattern = Vec2::new(
            fmath::sin(t * 1.7) * 0.4 + fmath::sin(t * 0.9 + 0.4) * 0.2 + fmath::sin(t * 1.9) * 0.8,
            fmath::sin(t * 1.1 + 1.0) * 0.5
                + fmath::cos(t * 0.6) * 0.25
                + fmath::sin(t * 1.9 + 0.8) * 0.1,
        );
        pattern * amplitude.to_radians() * breath * ads_ease(self.ads)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CombatInput {
    pub fire: bool,
    /// View angles a shot fired this tick leaves from (radians, as in
    /// `PlayerInput::shot_view`), carried into [`CombatEvent::Fired`].
    pub shot_yaw: f32,
    pub shot_pitch: f32,
    pub aim: bool,
    /// Reload was pressed this tick.
    pub reload: bool,
    /// Requested slot (0 or 1), if any.
    pub switch_to: Option<u8>,
    /// Held to steady a scope.
    pub hold_breath: bool,
    /// Take this weapon from a pickup or weapon rack this tick.
    pub pickup: Option<WeaponId>,
    /// A melee starts this tick: seconds until it strikes, and whether it
    /// lunges. The caller checks [`CombatState::can_melee`] first.
    pub melee: Option<(f32, bool)>,
    /// A grenade button went down this tick: pull the pin of one of that
    /// kind if it can come out ([`CombatState::can_throw`]).
    pub throw: Option<equipment::Kind>,
    /// The button of the grenade in hand is held: it stays in hand (a frag
    /// cooks) until it is let go.
    pub throw_held: bool,
}

/// How aimed the weapon is for its rules and its look, from the linear `ads`
/// progress: eased, leaving the hip quickly and slowing onto the sights. The
/// spread, the kick scale, the sway, the world zoom, and the weapon model all
/// follow it, so what the sights show matches what the rules do.
pub fn ads_ease(ads: f32) -> f32 {
    let t = ads.clamp(0.0, 1.0);
    let s = t * t * (3.0 - 2.0 * t);
    let out = 1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t);
    0.5 * (s + out)
}

/// Seconds past the fire interval that hip-fire bloom holds before shrinking.
const BLOOM_HOLD: f32 = 0.1;

/// Hip cone multiplier while crouched.
const CROUCH_HIP_SPREAD: f32 = 0.8;

/// Movement facts the weapon reacts to.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CombatContext {
    pub sprinting: bool,
    pub airborne: bool,
    /// Crouched (not sliding): a steadier kick and a tighter hip cone.
    pub crouched: bool,
    /// Horizontal speed relative to walk speed.
    pub move_ratio: f32,
    /// How far the weapon is mounted on cover, 0 to 1 (`MoveState::mount_blend`):
    /// the kick and the aimed sway shrink with it ([`crate::mount`]).
    pub mount: f32,
    /// How fast reloads, switches, throws, and the sprint-out run, from the
    /// player's perks ([`crate::perk::Perks::handling`]).
    pub handling: crate::perk::Handling,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CombatEvent {
    /// A shot left the barrel. `aim_offset` (radians, `x` pitch up, `y` yaw right)
    /// is recoil, spread, and sway relative to the view direction. Each of the
    /// weapon's pellets adds [`pellet_offset`]`(weapon, shot, i)` to it.
    ///
    /// `climb` (radians, same axes) is the part of this shot's kick that
    /// stays: the shooter's client turns its view by it, as if the mouse had
    /// moved, so later shots leave from the climbed aim.
    ///
    /// `yaw` and `pitch` are the view angles the shot leaves from, before
    /// `aim_offset`: the press's view for a shot fired on the input where the
    /// trigger went down between ticks (`net::subtick`), the tick's otherwise.
    Fired {
        weapon: WeaponId,
        yaw: f32,
        pitch: f32,
        aim_offset: Vec2,
        shot: u32,
        climb: Vec2,
    },
    DryFire {
        weapon: WeaponId,
    },
    ReloadStarted {
        weapon: WeaponId,
        empty: bool,
    },
    /// The fresh magazine seated during a magazine reload: its rounds count
    /// from now, even if the rest of the reload is cut short.
    MagazineSeated {
        weapon: WeaponId,
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
    /// A knife strike began with `weapon` in hand.
    MeleeStarted {
        weapon: WeaponId,
        lunge: bool,
    },
    /// A grenade's pin was pulled with `weapon` in hand, which goes down.
    ThrowStarted {
        weapon: WeaponId,
        kind: equipment::Kind,
    },
    /// The grenade left the hand along the view `yaw` and `pitch` (radians)
    /// with `fuse` seconds left; 0 means it went off in the hand. `shot`
    /// numbers it in the same series as shots, so the thrower's drawn
    /// grenade can be matched to the server's.
    Thrown {
        kind: equipment::Kind,
        yaw: f32,
        pitch: f32,
        fuse: f32,
        shot: u32,
    },
    /// The knife strike landed: whoever runs the hit test checks it now.
    /// `shot` numbers it in the same series as shots, so the server's
    /// verdict can be matched to it.
    MeleeStruck {
        weapon: WeaponId,
        shot: u32,
        lunge: bool,
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
    Vec2::new(fmath::sin(angle), fmath::cos(angle)) * radius
}

/// Advances combat by one tick.
pub fn step(
    state: &mut CombatState,
    input: &CombatInput,
    ctx: &CombatContext,
    dt: f32,
    events: &mut Vec<CombatEvent>,
) {
    // A burst keeps firing after the trigger is let go.
    let was_firing = state.fire_held || state.burst_left > 0;
    let fire_pressed = input.fire && !state.fire_held;
    state.fire_held = input.fire;
    // During a spray a slightly negative cooldown carries into the next shot so the
    // fire rate stays exact even when the interval isn't a whole number of ticks.
    // The carry never exceeds one tick: a trigger held through a reload, switch or
    // sprint must not bank time and then dump it as a burst of back-to-back shots.
    state.cooldown -= dt;
    state.cooldown = state.cooldown.max(if was_firing { -dt } else { 0.0 });
    state.since_shot += dt;

    // Hands full: a pickup waits for the throw to end.
    if let Some(weapon) = input.pickup
        && !state.throwing()
    {
        pick_up(state, weapon, events);
    }

    if ctx.sprinting {
        // A reload carries on through a sprint; a burst does not.
        state.sprint_out = state.def().sprint_out_time;
        state.burst_left = 0;
    } else {
        state.sprint_out = (state.sprint_out - dt * ctx.handling.sprint_out).max(0.0);
    }

    request_switch(state, input);
    advance_throw(state, input, dt, ctx.handling.throw, events);
    advance_action(state, dt, &ctx.handling, events);
    if let Some((strike_in, lunge)) = input.melee
        && state.can_melee()
    {
        start_melee(state, strike_in, lunge, events);
    }
    if let Some(kind) = input.throw
        && state.can_throw(kind)
    {
        start_throw(state, kind, events);
    }

    let def = state.def();
    // A reload can't cut a bolt or pump cycle short: it waits for the cycle to end.
    if input.reload {
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
        FireMode::Burst { .. } => state.burst_left > 0 || fire_pressed,
    };
    // Firing with a round loaded interrupts a shell-by-shell reload, and a
    // magazine reload once the weapon is back on the shoulder.
    if trigger && matches!(state.action, Action::Loading { .. }) && state.weapon().magazine > 0 {
        state.action = Action::Ready;
    }
    if trigger && state.reload_ready() {
        let slot = &mut state.slots[usize::from(state.active)];
        slot.needs_cycle = false;
        state.action = Action::Ready;
        events.push(CombatEvent::ReloadFinished { weapon: slot.id });
    }

    // Aiming works through a reload and a bolt or pump cycle, not a switch
    // or a melee.
    let aim_allowed = !matches!(
        state.action,
        Action::Lowering { .. }
            | Action::Raising { .. }
            | Action::Meleeing { .. }
            | Action::Throwing { .. }
    );
    let can_aim = input.aim && !ctx.sprinting && aim_allowed;
    let ads_step = dt / def.ads_time;
    state.ads = if can_aim {
        (state.ads + ads_step).min(1.0)
    } else {
        (state.ads - ads_step).max(0.0)
    };
    update_breath(state, input.hold_breath, dt);
    // Bloom holds through a spray and shrinks once the trigger rests.
    if state.since_shot > def.fire_interval() + BLOOM_HOLD {
        state.bloom = (state.bloom - def.spread.bloom_recovery * dt).max(0.0);
    }

    let ready = matches!(state.action, Action::Ready) && !ctx.sprinting && state.sprint_out <= 0.0;
    if trigger && ready && state.cooldown <= 0.0 {
        if state.weapon().magazine == 0 {
            state.burst_left = 0;
            if fire_pressed {
                events.push(CombatEvent::DryFire {
                    weapon: state.weapon().id,
                });
            }
            try_start_reload(state, events);
        } else {
            if let FireMode::Burst { rounds, .. } = def.fire_mode
                && state.burst_left == 0
            {
                state.burst_left = rounds.max(1);
            }
            fire(state, input, ctx, events);
        }
    } else if state.weapon().magazine == 0
        && matches!(state.action, Action::Ready)
        && state.cooldown <= 0.0
    {
        // Auto-reload once the last round's cooldown has passed.
        try_start_reload(state, events);
    }

    recover(state, dt);
}

fn pick_up(state: &mut CombatState, weapon: WeaponId, events: &mut Vec<CombatEvent>) {
    let def = weapon.def();
    if let Some(slot) = state.slots.iter_mut().find(|s| s.carried && s.id == weapon) {
        slot.reserve = slot.reserve.max(def.reserve);
        events.push(CombatEvent::PickedUp { weapon });
        return;
    }
    let slot = def.slot.index();
    state.slots[usize::from(slot)] = WeaponState::full(weapon);
    state.active = slot;
    state.burst = 0;
    state.bloom = 0.0;
    state.burst_left = 0;
    state.cooldown = 0.0;
    state.ads = 0.0;
    state.reload_queued = false;
    // A weapon off the ground comes up with a first raise.
    state.slots[usize::from(slot)].fresh = true;
    state.action = draw(state);
    events.push(CombatEvent::PickedUp { weapon });
    events.push(CombatEvent::Switched { weapon });
}

fn request_switch(state: &mut CombatState, input: &CombatInput) {
    let Some(to) = input.switch_to else { return };
    if !state.slots.get(usize::from(to)).is_some_and(|s| s.carried) {
        return;
    }
    match state.action {
        // The knife finishes its strike first, and a grenade its throw.
        Action::Meleeing { .. } | Action::Throwing { .. } => return,
        Action::Lowering { to: target, .. } if target == to => return,
        // Changed their mind: bring the weapon in hand back up from as far
        // as it got, instead of lowering it all the way and drawing it again.
        Action::Lowering { remaining, .. } if to == state.active => {
            let def = state.def();
            let lowered = 1.0 - (remaining / def.lower_time.max(1e-3)).clamp(0.0, 1.0);
            state.action = Action::Raising {
                remaining: def.raise_time * lowered,
                kind: Raise::Draw,
            };
            return;
        }
        Action::Lowering { .. } => {}
        _ if to == state.active => return,
        Action::Cycling { .. } => {
            // The round is not chambered yet: work the action when next drawn.
            state.slots[usize::from(state.active)].needs_cycle = true;
        }
        Action::Reloading { empty: true, .. } if state.magazine_seated() => {
            // An empty reload cut off before the charge.
            state.slots[usize::from(state.active)].needs_cycle = true;
        }
        _ => {}
    }
    state.burst_left = 0;
    state.reload_queued = false;
    state.action = Action::Lowering {
        remaining: state.def().lower_time,
        to,
    };
}

/// Advances the action in progress. Perks run some timers faster
/// (`handling`): a reload at `reload` times real time, a switch's lowering
/// and raising at `switch`.
fn advance_action(
    state: &mut CombatState,
    dt: f32,
    handling: &crate::perk::Handling,
    events: &mut Vec<CombatEvent>,
) {
    match state.action {
        // Advanced by `advance_throw`, which needs the button.
        Action::Ready | Action::Throwing { .. } => {}
        Action::Reloading { remaining, empty } => {
            let was_seated = state.magazine_seated();
            let remaining = remaining - dt * handling.reload;
            state.action = Action::Reloading { remaining, empty };
            if !was_seated && (remaining <= 0.0 || state.magazine_seated()) {
                // The fresh magazine is in: its rounds count from here, even
                // if the rest of the reload is cut short.
                let slot = &mut state.slots[usize::from(state.active)];
                let take = slot
                    .id
                    .def()
                    .magazine
                    .saturating_sub(slot.magazine)
                    .min(slot.reserve);
                slot.magazine += take;
                slot.reserve -= take;
                events.push(CombatEvent::MagazineSeated { weapon: slot.id });
            }
            if remaining <= 0.0 {
                let slot = &mut state.slots[usize::from(state.active)];
                // An empty reload ends with the charge: a round is chambered.
                slot.needs_cycle = false;
                state.action = Action::Ready;
                events.push(CombatEvent::ReloadFinished { weapon: slot.id });
            }
        }
        Action::Loading { remaining, stage } => {
            let remaining = remaining - dt * handling.reload;
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
            let remaining = remaining - dt * handling.switch;
            if remaining > 0.0 {
                state.action = Action::Lowering { remaining, to };
                return;
            }
            state.active = to;
            state.burst = 0;
            state.burst_left = 0;
            state.bloom = 0.0;
            state.cooldown = 0.0;
            state.action = draw(state);
            events.push(CombatEvent::Switched {
                weapon: state.weapon().id,
            });
        }
        Action::Raising { remaining, kind } => {
            let remaining = remaining - dt * handling.switch;
            state.action = if remaining > 0.0 {
                Action::Raising { remaining, kind }
            } else if kind == Raise::First {
                // The first raise worked the action: the chamber is loaded.
                state.slots[usize::from(state.active)].needs_cycle = false;
                Action::Ready
            } else {
                finish_cycle_on_draw(state, events)
            };
        }
        Action::Meleeing {
            remaining,
            strike_in,
            lunge,
        } => {
            let remaining = remaining - dt;
            let strike_in = strike_in - dt;
            if strike_in <= 0.0 && strike_in + dt > 0.0 {
                events.push(CombatEvent::MeleeStruck {
                    weapon: state.weapon().id,
                    shot: state.shots_fired,
                    lunge,
                });
                state.shots_fired = state.shots_fired.wrapping_add(1);
            }
            state.action = if remaining > 0.0 {
                Action::Meleeing {
                    remaining,
                    strike_in,
                    lunge,
                }
            } else {
                // Back up; a cycle or charge the knife cut short comes first.
                finish_cycle_on_draw(state, events)
            };
        }
    }
}

/// Pulls a grenade's pin: the weapon goes down and the fuse starts. Cuts a
/// reload or a bolt or pump cycle short, as the knife does.
fn start_throw(state: &mut CombatState, kind: equipment::Kind, events: &mut Vec<CombatEvent>) {
    let cut_cycle = match state.action {
        Action::Cycling { .. } => true,
        Action::Reloading { empty: true, .. } => state.magazine_seated(),
        _ => false,
    };
    if cut_cycle {
        state.slots[usize::from(state.active)].needs_cycle = true;
    }
    state.burst_left = 0;
    state.reload_queued = false;
    if kind.lethal() {
        state.lethals -= 1;
    } else {
        state.tacticals -= 1;
    }
    state.action = Action::Throwing {
        kind,
        cooked: 0.0,
        release_in: None,
    };
    events.push(CombatEvent::ThrowStarted {
        weapon: state.weapon().id,
        kind,
    });
}

/// Cooks the grenade in hand while the button is held, winds up once it is
/// let go (never before the pin is out), and throws it. A grenade cooked to
/// the end of its fuse goes off in the hand.
/// Advances a throw. The wind-up and follow-through run at `rate` times real
/// time (Fast Hands); the pin and the fuse keep real time.
fn advance_throw(
    state: &mut CombatState,
    input: &CombatInput,
    dt: f32,
    rate: f32,
    events: &mut Vec<CombatEvent>,
) {
    let Action::Throwing {
        kind,
        cooked,
        release_in,
    } = state.action
    else {
        return;
    };
    let was_thrown = release_in.is_some_and(|left| left <= 0.0);
    let cooked = cooked + dt;
    let mut release_in = match release_in {
        Some(left) => Some(left - dt * rate),
        None if !input.throw_held && cooked >= equipment::PIN_TIME => Some(equipment::WIND_UP),
        None => None,
    };
    if !was_thrown {
        // A frag's fuse burns from the pin; a tactical's starts as it leaves.
        let fuse = if kind.cooks() {
            kind.fuse() - cooked
        } else {
            kind.fuse()
        };
        let cooked_off = fuse <= 0.0 && release_in.is_none_or(|left| left > 0.0);
        if cooked_off {
            // Gone off in the hand: the arm drops from there.
            release_in = Some(0.0);
        }
        if release_in.is_some_and(|left| left <= 0.0) {
            events.push(CombatEvent::Thrown {
                kind,
                yaw: input.shot_yaw,
                pitch: input.shot_pitch,
                fuse: fuse.max(0.0),
                shot: state.shots_fired,
            });
            state.shots_fired = state.shots_fired.wrapping_add(1);
        }
    }
    // The follow-through and the weapon coming back up.
    state.action = if release_in.is_some_and(|left| left <= -equipment::RECOVERY) {
        finish_cycle_on_draw(state, events)
    } else {
        Action::Throwing {
            kind,
            cooked,
            release_in,
        }
    };
}

/// Starts a knife strike that lands in `strike_in` seconds. It cuts short a
/// reload (keeping a magazine already seated) or a bolt or pump cycle, which
/// is worked once the weapon is back up.
fn start_melee(
    state: &mut CombatState,
    strike_in: f32,
    lunge: bool,
    events: &mut Vec<CombatEvent>,
) {
    let cut_cycle = match state.action {
        Action::Cycling { .. } => true,
        Action::Reloading { empty: true, .. } => state.magazine_seated(),
        _ => false,
    };
    if cut_cycle {
        state.slots[usize::from(state.active)].needs_cycle = true;
    }
    state.burst_left = 0;
    state.reload_queued = false;
    // Strikes a tick after the press at the soonest, so the strike always
    // follows the start.
    let strike_in = strike_in.max(1e-4);
    state.action = Action::Meleeing {
        remaining: strike_in + crate::melee::RECOVERY,
        strike_in,
        lunge,
    };
    events.push(CombatEvent::MeleeStarted {
        weapon: state.weapon().id,
        lunge,
    });
}

/// The action a weapon is in once drawn: ready, or working the bolt or pump
/// (or charging) if a switch cut that short.
/// The raise that brings up the weapon in the active slot: a first raise if
/// it has not been drawn yet ([`WeaponState::fresh`]), otherwise a draw.
fn draw(state: &mut CombatState) -> Action {
    let slot = &mut state.slots[usize::from(state.active)];
    let def = slot.id.def();
    // A weapon without a first raise of its own just draws.
    if std::mem::take(&mut slot.fresh) && def.first_raise_time > def.raise_time {
        Action::Raising {
            remaining: def.first_raise(),
            kind: Raise::First,
        }
    } else {
        Action::Raising {
            remaining: def.raise_time,
            kind: Raise::Draw,
        }
    }
}

fn finish_cycle_on_draw(state: &mut CombatState, events: &mut Vec<CombatEvent>) -> Action {
    let slot = &mut state.slots[usize::from(state.active)];
    if !std::mem::take(&mut slot.needs_cycle) || slot.magazine == 0 {
        return Action::Ready;
    }
    let def = slot.id.def();
    // Self-loading weapons charge in about the time a bolt would take.
    let remaining = if def.cycle_time > 0.0 {
        def.cycle_time
    } else {
        CHARGE_TIME
    };
    events.push(CombatEvent::Cycled { weapon: slot.id });
    Action::Cycling { remaining }
}

/// Seconds to pull the charging handle of a self-loading weapon whose empty
/// reload was cut off before the charge.
const CHARGE_TIME: f32 = 0.35;

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
    state.burst_left = 0;
    state.action = match def.reload {
        Reload::Magazine {
            time, empty_time, ..
        } => Action::Reloading {
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
    state.holding_breath = false;
    if state.ads > 0.5 {
        state.sway_time += dt;
    }
    // Only a scope with sway can be steadied.
    let scope = state
        .def()
        .scope
        .filter(|s| s.sway > 0.0 && s.hold_breath > 0.0 && state.ads > 0.5);
    if let Some(scope) = scope
        && hold
        && state.winded <= 0.0
        && state.breath > 0.0
    {
        state.holding_breath = true;
        state.breath -= dt / scope.hold_breath;
        if state.breath <= 0.0 {
            state.breath = 0.0;
            state.winded = scope.breath_recovery;
        }
        return;
    }
    if state.winded <= 0.0 && !hold {
        // Breath comes back in about two seconds once the button is let go.
        state.breath = (state.breath + dt * 0.5).min(1.0);
    }
}

fn fire(
    state: &mut CombatState,
    input: &CombatInput,
    ctx: &CombatContext,
    events: &mut Vec<CombatEvent>,
) {
    let def = state.def();
    let seed = state.shots_fired;

    // Spread: uniform point in a cone, from two hashed values.
    let cone = state.spread_degrees(ctx).to_radians();
    let angle = rng::unit(seed, 1) * TAU;
    let radius = rng::unit(seed, 2).sqrt() * cone;
    let spread = Vec2::new(fmath::sin(angle), fmath::cos(angle)) * radius;
    let aim_offset = state.recoil + spread + state.aimed_sway(ctx.mount);

    let recoil = &def.recoil;
    let base = recoil.pattern[usize::from(state.burst).min(recoil.pattern.len() - 1)];
    let jitter = Vec2::new(
        rng::signed(seed, 3) * recoil.jitter.x,
        rng::signed(seed, 4) * recoil.jitter.y,
    );
    let mut scale = 1.0 + (recoil.ads_scale - 1.0) * ads_ease(state.ads);
    if ctx.crouched {
        scale *= recoil.crouch_scale;
    }
    scale *= crate::mount::kick_scale(ctx.mount);
    let kick = Vec2::new(
        (base.x + jitter.x).to_radians(),
        (base.y + jitter.y).to_radians(),
    ) * scale;
    state.recoil += kick * recoil.kick_return;
    events.push(CombatEvent::Fired {
        weapon: state.weapon().id,
        yaw: input.shot_yaw,
        pitch: input.shot_pitch,
        aim_offset,
        shot: seed,
        climb: kick * (1.0 - recoil.kick_return),
    });
    let spread_def = &def.spread;
    state.bloom = (state.bloom + spread_def.bloom).min(spread_def.bloom_max);

    let slot = &mut state.slots[usize::from(state.active)];
    slot.magazine -= 1;
    // Accumulate so the average fire rate is exact even when the interval is not a whole number of ticks.
    state.cooldown += def.fire_interval();
    if let FireMode::Burst { delay, .. } = def.fire_mode {
        state.burst_left = state.burst_left.saturating_sub(1);
        if state.burst_left == 0 || slot.magazine == 0 {
            state.burst_left = 0;
            state.cooldown += delay;
        }
    }
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
    let def = state.def();
    let recoil = &def.recoil;
    // The pattern starts over once the trigger has rested a moment.
    if state.since_shot >= def.fire_interval() + recoil.recovery_delay.max(0.05) {
        state.burst = 0;
    }
    if state.since_shot < recoil.recovery_delay {
        return;
    }
    state.recoil *= fmath::exp(-recoil.recovery_rate * dt);
    if state.recoil.length_squared() < 1e-8 {
        state.recoil = Vec2::ZERO;
    }
}

#[cfg(test)]
mod tests;
