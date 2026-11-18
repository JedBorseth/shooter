//! First-person player: input, simulation, hit detection, camera, and viewmodel.
//!
//! Physics runs at the fixed tick (`physics_process`) and calls the shared
//! simulation in `shooter-sim`. The camera is a top-level node updated every
//! rendered frame: position, recoil, and scope sway are interpolated between the
//! last two ticks, while mouse look is applied immediately, so aiming never
//! waits for a physics tick on high-refresh displays.
//!
//! Gamepad look also runs every rendered frame: stick shaping, turn boost, and
//! aim assist (slowdown, rotational, and target aim assist on zombies) from
//! `shooter_sim::look`.
//!
//! The player picks one input device in Settings (`controller_input`), and the
//! other is locked out: Settings takes its bindings off the input map, and the
//! player ignores mouse look on a controller and stick look on the mouse. Aim
//! assist and vibration only run on a controller.
//!
//! Online, a `NetClient` turns on prediction: every tick's input and result
//! are recorded, and server snapshots are reconciled against them, replaying
//! unconfirmed inputs when the server disagrees. Corrections are blended out
//! visually instead of snapping the camera. The server owns respawns and the
//! demo ammo refill.
//!
//! Online, the server also owns damage, kills, and score. Firing inputs carry
//! the tick other players were drawn at, so the server can check the shot
//! against what this player saw. Hit markers on other players show at once
//! (predicted); the server's verdict then confirms kills, adds any hit the
//! prediction missed, and sends score awards, damage taken, and deaths.

use std::collections::{HashMap, VecDeque};
use std::f32::consts::PI;

use godot::classes::input::MouseMode;
use godot::classes::{
    AudioStreamPlayer, AudioStreamRandomizer, Camera3D, CharacterBody3D, CollisionShape3D, Engine,
    ICharacterBody3D, Input, InputEvent, InputEventJoypadButton, InputEventJoypadMotion,
    InputEventMouseButton, InputEventMouseMotion, Node3D, PackedScene, PhysicsDirectSpaceState3D,
    PhysicsRayQueryParameters3D, PhysicsShapeQueryParameters3D, ShapeCast3D, SphereShape3D, Time,
};
use godot::prelude::*;
use shooter_sim::hitbox::HitZone;
use shooter_sim::look::{
    self, AimAssist, AimAssistParams, AssistTarget, AssistView, StickLook, StickParams,
};
use shooter_sim::melee;
use shooter_sim::movement::{MoveEnv, Stance};
use shooter_sim::net::prediction::{ErrorSmoothing, Prediction, Reconciled};
use shooter_sim::net::protocol::{self, OwnState, Predicted};
use shooter_sim::net::subtick;
use shooter_sim::player::{self, FireAim, PlayerEvent, PlayerInput, PlayerParams, PlayerState};
use shooter_sim::score::{Kill, ScoreRules, ScoreState, ScoreTotals, Scored};
use shooter_sim::weapon::{self, Action, CombatEvent, FireMode, LoadStage, Reload, Slot, WeaponId};
use shooter_sim::{Vec2, Vec3, rng};

use crate::acoustics::AudioEnvironment;
use crate::aim_assist::{AIM_ASSIST_GROUP, AimAssistTarget};
use crate::audio;
use crate::body::{self, BodyParts};
use crate::bullets::ShotTracer;
use crate::convert::{to_godot, to_sim};
use crate::effects::CombatEffects;
use crate::footsteps::{self, Gait, Stride};
use crate::penetration;
use crate::pickup::{INTERACT_LAYER, WeaponPickup};
use crate::player_sounds::{BodyFrame, PlayerSounds};
use crate::spring::{Spring, damping_for, impulse_for_peak, smoothing};
use crate::surfaces::{Surface, surface_at};
use crate::target::{HITBOX_LAYER, WORLD_LAYER};
use crate::viewmodel::{ViewAction, ViewFrame, ViewmodelRig};

use shooter_sim::look::PITCH_LIMIT;

/// Surface used when a collider has no `surface` metadata.
const DEFAULT_SURFACE: &str = "concrete";

/// Camera punch spring: stiff and well damped. A shot's punch peaks after about
/// 35 ms and is roughly half gone by the next M4 round (80 ms), so during a spray
/// the camera sits at most ~1.6 punches away from the aim point.
const PUNCH_STIFFNESS: f32 = 900.0;
const PUNCH_DAMPING: f32 = 48.0;
/// FOV pulse spring (fraction of the FOV): a quick widen on slide starts and a
/// short zoom-in "thump" on kills.
const FOV_KICK_STIFFNESS: f32 = 220.0;
const FOV_KICK_DAMPING: f32 = 22.0;
/// Zoom pulse per shot, as a fraction of the FOV per degree of the weapon's
/// pitch punch: a heavy shot reads as a thump without moving the aim.
const SHOT_ZOOM_PULSE: f32 = 0.015;
/// The largest shot zoom pulse, as a fraction of the FOV (a shotgun's).
const SHOT_ZOOM_PULSE_MAX: f32 = 0.02;
/// How fast a shot's recoil climb lands on the view, in 1/seconds.
const CLIMB_RATE: f32 = 90.0;
/// Per-shot camera shake spring.
const SHAKE_STIFFNESS: f32 = 900.0;
/// The landing dip: a soft spring, so the body sinks into a landing and comes
/// back up over about a third of a second.
const LAND_DIP_STIFFNESS: f32 = 70.0;
const LAND_DIP_ZETA: f32 = 0.65;
/// The landing nod (the look tips down and back) and the hit flinch.
const LAND_NOD_STIFFNESS: f32 = 120.0;
const LAND_NOD_ZETA: f32 = 0.7;
const FLINCH_STIFFNESS: f32 = 160.0;
const FLINCH_DAMPING: f32 = 16.0;
/// One target a shot hit: the node, summed damage, any headshot, the first
/// pellet's direction and distance, and where it landed (point and hit shape).
type ShotTarget = (Gd<Node>, f32, bool, Vector3, f32, Vector3, i64);

/// The death camera: how far behind the body and how high it settles.
const DEATH_CAMERA_BACK: f32 = 2.4;
const DEATH_CAMERA_HEIGHT: f32 = 1.5;

/// Actions whose presses are remembered until the next physics tick, so a tap
/// shorter than one tick (or a mouse-wheel step) is never lost.
const LATCHED_ACTIONS: [&str; 10] = [
    "jump",
    "crouch",
    "sprint",
    "fire",
    "reload",
    "melee",
    "weapon_1",
    "weapon_2",
    "interact",
    "refill_ammo",
];

/// After a mouse-wheel step swaps weapons, further wheel input is ignored for
/// this long, so one flick (several notches, or a high-resolution wheel's
/// partial steps) swaps once instead of swapping back.
const WHEEL_SWAP_COOLDOWN_MS: u64 = 250;
/// Partial wheel steps further apart than this start a fresh notch.
const WHEEL_STEP_GAP_MS: u64 = 200;

/// Turns mouse-wheel events bound to the weapon swap into swaps: one per
/// whole notch, and one per flick.
#[derive(Clone, Copy, Debug, Default)]
struct WheelSwap {
    /// Partial steps toward the next notch.
    steps: f32,
    /// When the wheel last moved, in milliseconds.
    moved_ms: u64,
    /// When the wheel last swapped, in milliseconds.
    swapped_ms: Option<u64>,
}

impl WheelSwap {
    /// One wheel event at `now_ms`; `factor` is the share of a notch it
    /// covers (0 when the platform doesn't say: a whole notch). True when it
    /// swaps.
    fn step(&mut self, now_ms: u64, factor: f32) -> bool {
        let resting = now_ms.saturating_sub(self.moved_ms) > WHEEL_STEP_GAP_MS;
        self.moved_ms = now_ms;
        if self
            .swapped_ms
            .is_some_and(|at| now_ms.saturating_sub(at) < WHEEL_SWAP_COOLDOWN_MS)
        {
            self.steps = 0.0;
            return false;
        }
        if resting {
            self.steps = 0.0;
        }
        self.steps += if factor > 0.0 { factor } else { 1.0 };
        if self.steps < 0.99 {
            return false;
        }
        self.steps = 0.0;
        self.swapped_ms = Some(now_ms);
        true
    }
}

/// The view at a fire press between two ticks, captured on the input event.
#[derive(Clone, Copy, Debug)]
struct FirePress {
    yaw: f32,
    pitch: f32,
    /// Online: the server tick other players were drawn at on that frame.
    view_tick: Option<f64>,
}

#[derive(GodotClass)]
#[class(init, base = CharacterBody3D)]
pub struct FpsPlayer {
    base: Base<CharacterBody3D>,

    /// Radians of view rotation per screen pixel of mouse motion.
    #[export]
    #[init(val = 0.0022)]
    mouse_sensitivity: f32,
    /// Extra sensitivity multiplier while aiming down sights (on top of the
    /// automatic FOV scaling).
    #[export]
    #[init(val = 1.0)]
    ads_sensitivity_scale: f32,
    /// Vertical field of view in degrees at the hip.
    #[export]
    #[init(val = 70.0)]
    base_fov: f32,
    /// FOV multiplier while sprinting or sliding.
    #[export]
    #[init(val = 1.08)]
    sprint_fov_scale: f32,
    /// Peak FOV widening when a slide starts, as a fraction of the FOV.
    #[export]
    #[init(val = 0.06)]
    slide_fov_kick: f32,
    /// Peak FOV zoom-in on a kill, as a fraction of the FOV (headshot kills get 1.6x).
    #[export]
    #[init(val = 0.025)]
    kill_fov_kick: f32,
    /// Fraction of each weapon's camera punch (a roll and a small zoom pulse;
    /// neither moves the screen centre off the aim) kept while fully aimed.
    #[export]
    #[init(val = 0.7)]
    ads_punch_scale: f32,
    /// Head bob and strafe or slide tilt. Landing and damage still move the camera.
    #[export]
    #[init(val = true)]
    camera_motion: bool,
    #[export]
    #[init(val = 3.0)]
    respawn_delay: f32,
    /// Respawn after `respawn_delay`. Off in Zombies, where death ends the match.
    #[export]
    #[init(val = true)]
    auto_respawn: bool,
    /// The T key refills every carried weapon (the demo arena). Off in Zombies.
    #[export]
    #[init(val = true)]
    allow_refill: bool,
    /// Spawn with a primary weapon. Off in Zombies, which starts with only
    /// the sidearm.
    #[export]
    #[init(val = true)]
    start_with_primary: bool,
    /// How far away weapon racks can be used, in metres.
    #[export]
    #[init(val = 2.6)]
    interact_distance: f32,
    /// Horizontal stick turn rate at full deflection, in degrees per second.
    #[export]
    #[init(val = 200.0)]
    stick_yaw_speed: f32,
    /// Vertical stick turn rate at full deflection, in degrees per second.
    #[export]
    #[init(val = 150.0)]
    stick_pitch_speed: f32,
    /// Extra horizontal turn rate after holding the stick at the edge, in
    /// degrees per second. 0 disables the boost.
    #[export]
    #[init(val = 120.0)]
    stick_turn_boost: f32,
    /// Look stick deflection below this is ignored (0 to 1).
    #[export]
    #[init(val = 0.1)]
    stick_deadzone: f32,
    /// Look stick deflection within this of the edge reads as full (0 to 1).
    #[export]
    #[init(val = 0.04)]
    stick_outer_deadzone: f32,
    /// Look stick response curve exponent: 1 is linear, 2 is finer near the center.
    #[export]
    #[init(val = 2.0)]
    stick_curve: f32,
    #[export]
    stick_invert_y: bool,
    /// Mouse up looks down.
    #[export]
    mouse_invert_y: bool,
    /// Controller aim assist. Mouse aiming never gets it.
    #[export]
    #[init(val = true)]
    aim_assist: bool,
    /// Controller vibration when firing.
    #[export]
    #[init(val = true)]
    rumble: bool,
    /// A controller plays: mouse look is ignored, and aim assist, vibration,
    /// and button prompts are on. Off, the sticks are ignored. Set by Settings.
    #[export]
    controller_input: bool,
    /// Bullet tracer, spawned for every Nth round (`tracer_every` in the weapon file).
    #[export]
    tracer_scene: OnEditor<Gd<PackedScene>>,
    /// Solid beam drawn on every shot of an energy weapon (`beam` in the weapon file).
    #[export]
    beam_scene: Option<Gd<PackedScene>>,

    #[init(node = "Camera")]
    camera: OnReady<Gd<Camera3D>>,
    #[init(node = "Camera/Viewmodel")]
    viewmodel: OnReady<Gd<ViewmodelRig>>,
    #[init(node = "Collision")]
    collision: OnReady<Gd<CollisionShape3D>>,
    #[init(node = "Headroom")]
    headroom: OnReady<Gd<ShapeCast3D>>,
    #[init(node = "Footsteps")]
    footsteps: OnReady<Gd<AudioStreamPlayer>>,
    #[init(node = "BodySounds")]
    body_sounds: OnReady<Gd<AudioStreamPlayer>>,

    params: PlayerParams,
    state: PlayerState,
    events: Vec<PlayerEvent>,
    latched: [bool; LATCHED_ACTIONS.len()],
    interact_held: bool,
    /// A swap press (wheel step or button) since the last tick. The swap
    /// toggles, so it fires once per press, never per tick held.
    next_pressed: bool,
    wheel_swap: WheelSwap,
    input_enabled: bool,
    /// Held still by the match (a Search and Destroy round's freeze): the
    /// player can look and aim but not move or fire.
    movement_locked: bool,
    env: MoveEnv,
    /// Floor contact after the last move; the next tick's `MoveEnv::on_floor`.
    on_floor: bool,
    /// Present while connected to a server.
    prediction: Option<Prediction>,
    /// Online: the server tick other players were last drawn at, sent with
    /// firing inputs for lag compensation.
    view_tick: Option<f64>,
    /// The view when fire went down since the last tick, captured on the
    /// press event so the shot leaves from it (`net::subtick`).
    fire_press: Option<FirePress>,
    /// View angles (yaw, pitch) of the last input, which a press aim must
    /// lie between this input's and (`subtick::check`, as on the server).
    previous_view: Option<(f32, f32)>,
    /// Online: recent shots whose hit marker was already shown from local
    /// hitscan, so the server's confirmation does not show it again.
    predicted_hits: VecDeque<u32>,
    smoothing: ErrorSmoothing,
    yaw: f32,
    pitch: f32,
    wish: Vector2,
    previous_position: Vector3,
    step_offset: f32,
    previous_step_offset: f32,
    current_position: Vector3,
    previous_recoil: Vec2,
    previous_sway: Vec2,
    previous_ads: f32,
    /// Aim pitch, aim yaw, and look pitch from the last rendered frame.
    aim_angles: Vector3,
    /// The crouch blend (`MoveState::crouch`) before the last tick, so the
    /// eye moves smoothly between ticks along the same blend the capsule,
    /// hit zones, and shot origin follow.
    previous_crouch: f32,
    fov_sprint: f32,
    /// Visual-only camera roll from each shot (radians, in `z`). Only roll,
    /// so the screen centre stays on the aim.
    punch: Spring,
    /// Recoil climb still to turn the view by (radians: `x` pitch up, `y` yaw
    /// right). Each shot's climb lands over a few frames, like a hand
    /// riding the kick.
    pending_climb: Vec2,
    /// Camera position shake per shot (metres, camera space).
    shake: Spring,
    /// Climb applied to the view this frame, so the weapon's look sway can
    /// tell the player's own turning from the recoil's.
    climb_step: Vec2,
    /// Look angles (yaw, pitch) at the last rendered frame, and the player's
    /// own turn rate since then (radians a second: yaw left, pitch up).
    last_look: Vector2,
    look_velocity: Vector2,
    /// FOV offset as a fraction of the FOV.
    fov_kick: Spring<f32>,
    spawn: Transform3D,
    respawn_in: f32,
    death_blend: f32,
    /// Metres walked toward the next footstep.
    stride: Stride,
    /// Which foot the stride is on (0 or 1), and the stride phase last frame.
    stride_foot: f32,
    last_stride_phase: f32,
    /// What the last footstep landed on (tests).
    last_surface: Surface,
    bob_blend: f32,
    tilt: f32,
    land_dip: Spring<f32>,
    /// The landing's nod: radians the look is tipped down (springs back).
    land_nod: Spring<f32>,
    /// A hit's flinch, in radians: pitch, yaw, and a visual-only roll.
    flinch: Spring<f32>,
    flinch_yaw: Spring<f32>,
    flinch_roll: Spring<f32>,
    /// Where the last shot that hurt us came from, for the death camera.
    last_attacker: Option<Vector3>,
    /// Seconds since death, where it happened (feet), and the view then.
    dead_for: f32,
    death_feet: Vector3,
    death_view: Vector2,
    score: ScoreState,
    score_rules: ScoreRules,
    look_target: Option<WeaponId>,
    /// Points the weapon under the crosshair costs (Zombies wall weapons).
    look_price: i64,
    /// Something else to use under the crosshair: a node with `interact`
    /// and `interact_prompt` methods (Zombies doors).
    look_device: Option<Gd<Node>>,
    /// Max Ammo (Zombies) asked for a refill on the next tick.
    refill_requested: bool,
    sounds: HashMap<String, Option<Gd<AudioStreamRandomizer>>>,
    /// Breath, heart, being hit, and the body's moves (`player_sounds`).
    player_sounds: Option<Gd<PlayerSounds>>,
    /// The controller that last sent input, for vibration.
    gamepad_device: i32,
    /// `controller_input` as of the last stick look, to drop aim assist's
    /// state when the device changes.
    stick_active: bool,
    stick_look: StickLook,
    assist: AimAssist,
    assist_target: Option<u64>,
}

#[godot_api]
impl ICharacterBody3D for FpsPlayer {
    fn ready(&mut self) {
        self.camera.set_as_top_level(true);
        let rid = self.base().get_rid();
        self.headroom.add_exception_rid(rid);
        self.base_mut().add_to_group("players");
        self.state = self.spawned_state();

        self.yaw = self.base().get_rotation().y;
        self.base_mut().set_rotation(Vector3::ZERO);
        self.spawn = self.base().get_global_transform();
        self.reset_view();
        self.input_enabled = true;

        let ticks = Engine::singleton().get_physics_ticks_per_second();
        if ticks != shooter_sim::TICK_RATE as i32 {
            godot_warn!(
                "physics runs at {ticks} Hz but the simulation is tuned for {} Hz",
                shooter_sim::TICK_RATE
            );
        }

        if !Engine::singleton().is_editor_hint() {
            Input::singleton().set_mouse_mode(MouseMode::CAPTURED);
            // Reverb zones and occlusion are judged from this player's camera.
            let mut acoustics = AudioEnvironment::new_alloc();
            acoustics.set_name("AudioEnvironment");
            self.base_mut().add_child(&acoustics);
            // Other players' shots, traced for their near misses and impacts,
            // and the pool every impact sound plays from.
            let mut tracer = ShotTracer::new_alloc();
            tracer.set_name("ShotTracer");
            self.base_mut().add_child(&tracer);
            // Impacts, blood, and the marks they leave, from pools.
            let mut effects = CombatEffects::new_alloc();
            effects.set_name("CombatEffects");
            self.base_mut().add_child(&effects);
            let mut body = PlayerSounds::new_alloc();
            body.set_name("PlayerSounds");
            self.base_mut().add_child(&body);
            self.player_sounds = Some(body);
        }
    }

    fn input(&mut self, event: Gd<InputEvent>) {
        if !self.input_enabled {
            return;
        }
        let mut input = Input::singleton();
        let captured = input.get_mouse_mode() == MouseMode::CAPTURED;

        self.track_gamepad(&event);
        for (latched, action) in self.latched.iter_mut().zip(LATCHED_ACTIONS) {
            if event.is_action_pressed(action) {
                *latched = true;
            }
        }
        if event.is_action_pressed("weapon_next") {
            match event.clone().try_cast::<InputEventMouseButton>() {
                Ok(wheel) => {
                    let now = Time::singleton().get_ticks_msec();
                    self.next_pressed |= self.wheel_swap.step(now, wheel.get_factor());
                }
                Err(_) => self.next_pressed = true,
            }
        }
        // The view at the click: every mouse motion before this event is
        // already applied. The first press since the last tick is the one
        // that fires.
        if event.is_action_pressed("fire") && self.fire_press.is_none() {
            self.fire_press = Some(FirePress {
                yaw: self.yaw,
                pitch: self.pitch,
                view_tick: self.view_tick,
            });
        }
        let event = match event.try_cast::<InputEventMouseMotion>() {
            Ok(motion) => {
                if captured && !self.controller_input {
                    // Screen-relative motion ignores window stretch; scaling by the
                    // aim zoom keeps aim speed consistent through sights and
                    // scopes. Sprint and slide FOV changes and kill pulses do
                    // not change it.
                    let ads = self.state.combat.ads;
                    let zoom = self.aim_zoom() * (1.0 + (self.ads_sensitivity_scale - 1.0) * ads);
                    let delta = motion.get_screen_relative() * self.mouse_sensitivity * zoom;
                    let dy = if self.mouse_invert_y {
                        -delta.y
                    } else {
                        delta.y
                    };
                    self.yaw -= delta.x;
                    self.pitch = (self.pitch - dy).clamp(-PITCH_LIMIT, PITCH_LIMIT);
                }
                return;
            }
            Err(event) => event,
        };
        if let Ok(button) = event.try_cast::<InputEventMouseButton>()
            && button.is_pressed()
            && !captured
        {
            input.set_mouse_mode(MouseMode::CAPTURED);
        }
    }

    fn physics_process(&mut self, delta: f64) {
        let dt = delta as f32;
        self.look_target = None;
        self.look_price = 0;
        self.look_device = None;
        self.find_interactable();
        // Quantized exactly as the server will decode it, and the press aim
        // checked as the server will check it, so prediction matches.
        let input = self.read_input().quantized();
        let input = PlayerInput {
            fire_aim: subtick::check(self.previous_view, &input),
            ..input
        };
        self.previous_view = Some((input.yaw, input.pitch));

        self.score.tick(dt);
        self.previous_recoil = self.state.combat.recoil;
        self.previous_sway = self.state.combat.scope_sway();
        self.previous_ads = self.state.combat.ads;
        self.previous_crouch = self.state.movement.crouch;
        let previous_stance = self.state.movement.stance;
        let mut events = std::mem::take(&mut self.events);
        events.clear();
        if self.refill_pressed() || std::mem::take(&mut self.refill_requested) {
            let mut combat = Vec::new();
            self.state.combat.refill_ammo(&mut combat);
            events.extend(combat.into_iter().map(PlayerEvent::Combat));
        }
        let mut predicted = self.predicted();
        let outcome = body::tick(
            &mut self.parts(),
            &self.params,
            &mut predicted,
            &input,
            &mut events,
        );
        self.previous_step_offset = self.step_offset;
        self.step_offset *= (-14.0 * dt).exp();
        self.step_offset -= outcome.step_height;
        self.state = predicted.state;
        self.on_floor = predicted.on_floor;
        self.env = outcome.env;
        if let Some(prediction) = &mut self.prediction {
            let seq = prediction.next_seq();
            prediction.record(seq, input, predicted);
        }
        let stance = self.state.movement.stance;
        if stance == Stance::Sliding && previous_stance != Stance::Sliding {
            self.play_body("res://assets/audio/player/slide", footsteps::SLIDE_DB);
            self.kick_fov(self.slide_fov_kick);
        }

        let position = to_godot(predicted.position);
        let moved = Vector2::new(
            position.x - self.current_position.x,
            position.z - self.current_position.z,
        )
        .length();
        self.previous_position = self.current_position;
        self.current_position = position;
        self.advance_footsteps(moved);

        for event in &events {
            self.handle_event(event);
        }
        self.events = events;
        if let Some(mut body) = self.player_sounds.clone() {
            let movement = self.state.movement;
            body.bind_mut().update(
                BodyFrame {
                    alive: self.state.alive(),
                    health: self.state.health / self.params.health.max_health.max(1.0),
                    sprinting: movement.sprinting,
                    on_floor: self.env.on_floor,
                    rising: movement.velocity.y,
                    mantling: movement.mantling(),
                    holding_breath: self.state.combat.holding_breath,
                },
                dt,
            );
        }

        // Online, the server respawns the player and the next snapshot moves them.
        if !self.state.alive() && self.prediction.is_none() && self.auto_respawn {
            self.respawn_in -= dt;
            if self.respawn_in <= 0.0 {
                self.respawn();
            }
        }
    }

    fn process(&mut self, delta: f64) {
        let dt = delta as f32;
        let fraction = Engine::singleton().get_physics_interpolation_fraction() as f32;
        let feet = self.previous_position.lerp(self.current_position, fraction)
            + to_godot(self.smoothing.update(dt))
            + Vector3::UP
                * (self.previous_step_offset
                    + (self.step_offset - self.previous_step_offset) * fraction);
        let combat = &self.state.combat;
        // Recoil kicks show on the first frame after the shot's tick instead of
        // easing in across a tick; only recovery is interpolated.
        let recoil = if combat.recoil.length_squared() > self.previous_recoil.length_squared() {
            combat.recoil
        } else {
            self.previous_recoil.lerp(combat.recoil, fraction)
        };
        let sway = self.previous_sway.lerp(combat.scope_sway(), fraction);
        let ads = self.previous_ads + (combat.ads - self.previous_ads) * fraction;

        let alive = self.state.alive();
        let death_target = if alive { 0.0 } else { 1.0 };
        self.death_blend += (death_target - self.death_blend) * smoothing(5.0, dt);

        let movement = self.state.movement;
        let crouch = self.previous_crouch + (movement.crouch - self.previous_crouch) * fraction;
        let eye_height = self.params.movement.eye_height_at(crouch);

        let fast = movement.sprinting || movement.stance == Stance::Sliding;
        let sprint_target = if fast { self.sprint_fov_scale } else { 1.0 };
        self.fov_sprint += (sprint_target - self.fov_sprint) * smoothing(8.0, dt);
        // The zoom eases in and out with the weapon (see `viewmodel::ads_ease`).
        let ads_zoom = 1.0 + (combat.def().ads_fov_scale - 1.0) * crate::viewmodel::ads_ease(ads);
        self.fov_kick.step(FOV_KICK_STIFFNESS, FOV_KICK_DAMPING, dt);
        let fov = self.base_fov * self.fov_sprint * ads_zoom * (1.0 + self.fov_kick.value);

        self.punch.step(PUNCH_STIFFNESS, PUNCH_DAMPING, dt);
        self.climb_step = Vec2::ZERO;
        if alive {
            self.apply_climb(dt);
        } else {
            self.pending_climb = Vec2::ZERO;
        }
        self.land_dip.step(
            LAND_DIP_STIFFNESS,
            damping_for(LAND_DIP_STIFFNESS, LAND_DIP_ZETA),
            dt,
        );
        // A hit's flinch and a landing's nod turn the look itself (and spring
        // back), so the sights and the screen centre stay where shots go, on
        // this machine and on the server, which sees it in the look angles.
        let before = Vector2::new(
            self.flinch_yaw.value,
            self.flinch.value - self.land_nod.value,
        );
        self.flinch.step(FLINCH_STIFFNESS, FLINCH_DAMPING, dt);
        self.flinch_yaw.step(FLINCH_STIFFNESS, FLINCH_DAMPING, dt);
        self.flinch_roll.step(FLINCH_STIFFNESS, FLINCH_DAMPING, dt);
        self.land_nod.step(
            LAND_NOD_STIFFNESS,
            damping_for(LAND_NOD_STIFFNESS, LAND_NOD_ZETA),
            dt,
        );
        if alive {
            let after = Vector2::new(
                self.flinch_yaw.value,
                self.flinch.value - self.land_nod.value,
            );
            let turn = after - before;
            self.turn(turn.x, turn.y);
        }
        self.shake
            .step(SHAKE_STIFFNESS, damping_for(SHAKE_STIFFNESS, 0.5), dt);

        if alive && self.input_enabled {
            let eye = feet + Vector3::UP * eye_height;
            self.apply_stick_look(eye, ads, dt);
        }

        // The player's own turning this frame (mouse, stick, aim assist),
        // without the recoil climb, for the weapon's look sway.
        let turned = Vector2::new(
            self.yaw - self.last_look.x + self.climb_step.y,
            self.pitch - self.last_look.y - self.climb_step.x,
        );
        self.last_look = Vector2::new(self.yaw, self.pitch);
        self.look_velocity = if dt > 0.0 && alive {
            // A teleport or respawn is not a turn.
            (turned / dt).clamp(Vector2::splat(-30.0), Vector2::splat(30.0))
        } else {
            Vector2::ZERO
        };

        let (bob, bob_roll, tilt) = self.camera_motion_offsets(dt, ads);
        let eye = eye_height;

        // The camera looks along the aim (look plus the simulated recoil snap
        // and sway), so the screen centre and the sights are where bullets
        // go. The visual punch only rolls the view.
        let punch = self.punch.value;
        self.aim_angles = Vector3::new(
            self.pitch + recoil.x + sway.x,
            self.yaw - recoil.y - sway.y,
            self.pitch,
        );
        let view = Vector3::new(
            self.pitch + recoil.x + sway.x,
            self.yaw - recoil.y - sway.y,
            punch.z + bob_roll + tilt + self.flinch_roll.value,
        );
        let basis = Basis::from_euler(EulerOrder::YXZ, view);
        let origin = feet
            + Vector3::UP * (eye + bob.y + self.land_dip.value)
            + basis.col_a() * bob.x
            + basis * self.shake.value;
        let first_person = Transform3D::new(basis, origin);
        let transform = if alive {
            first_person
        } else {
            self.death_camera(first_person, dt)
        };
        self.camera.set_global_transform(transform);
        self.camera.set_fov(fov);

        let frame = self.view_frame(ads);
        let inspect = self.input_enabled
            && self.state.alive()
            && Input::singleton().is_action_just_pressed("inspect");
        let mut rig = self.viewmodel.bind_mut();
        if inspect {
            rig.start_inspect();
        }
        rig.update(&frame, dt);
    }
}

#[godot_api]
impl FpsPlayer {
    /// Emitted when a shot damages a target (once per shot, even for shotguns).
    #[signal]
    fn hit_confirmed(killed: bool, headshot: bool);

    /// Emitted when the player takes damage. `from` is the attacker's position
    /// (the player's own position for falls).
    #[signal]
    fn damaged(amount: f32, from: Vector3, fall: bool);

    #[signal]
    fn died();

    #[signal]
    fn respawned();

    /// Emitted for each award a kill earns: the kill itself first, then
    /// bonuses and medals. `medal` awards (multi-kills, collaterals, streaks)
    /// also get a HUD banner.
    #[signal]
    fn scored(points: i64, label: GString, medal: bool);

    /// The surface the last footstep landed on (tests).
    #[func]
    fn footstep_surface(&self) -> GString {
        self.last_surface.key().into()
    }

    /// Match score so far.
    #[func]
    fn score(&self) -> i64 {
        i64::from(self.score.score)
    }

    #[func]
    fn kills(&self) -> i64 {
        i64::from(self.score.kills)
    }

    #[func]
    fn deaths(&self) -> i64 {
        i64::from(self.score.deaths)
    }

    /// Kills since the last death.
    #[func]
    fn kill_streak(&self) -> i64 {
        i64::from(self.score.streak)
    }

    /// Horizontal speed in metres per second, for HUD and debugging.
    #[func]
    fn horizontal_speed(&self) -> f32 {
        Vector2::new(
            self.state.movement.velocity.x,
            self.state.movement.velocity.z,
        )
        .length()
    }

    /// Where this tick's shots leave from (world space), for movement traces
    /// (tests): the eye on the simulation's crouch blend.
    #[func]
    fn shot_origin(&self) -> Vector3 {
        self.eye_position()
    }

    /// How far down the body is, 0 standing to 1 crouched (the simulation's
    /// crouch blend), for movement traces (tests).
    #[func]
    fn crouch_amount(&self) -> f32 {
        self.state.movement.crouch
    }

    /// Floor contact recorded by the last predicted or authoritative movement
    /// tick, exposed so tests can compare it with the engine's contact state.
    #[func]
    fn is_grounded(&self) -> bool {
        self.on_floor
    }

    /// Human-readable movement state, for HUD and debugging.
    #[func]
    fn movement_state(&self) -> GString {
        let movement = &self.state.movement;
        let label = match movement.stance {
            _ if !self.state.alive() => "Dead",
            _ if movement.mantling() => "Mantling",
            Stance::Sliding => "Sliding",
            Stance::Crouching => "Crouching",
            Stance::Standing if movement.sprinting => "Sprinting",
            Stance::Standing => "Standing",
        };
        label.into()
    }

    #[func]
    fn weapon_name(&self) -> GString {
        self.state.combat.def().name.as_str().into()
    }

    /// File key of the active weapon, such as `m4`.
    #[func]
    fn weapon_key(&self) -> GString {
        self.state.combat.def().key.as_str().into()
    }

    #[func]
    fn magazine(&self) -> i32 {
        i32::from(self.state.combat.weapon().magazine)
    }

    #[func]
    fn reserve(&self) -> i32 {
        i32::from(self.state.combat.weapon().reserve)
    }

    /// Rounds at or below which the HUD shows a low-ammo warning.
    #[func]
    fn low_ammo_threshold(&self) -> i32 {
        i32::from(self.state.combat.def().low_ammo())
    }

    /// Rounds a full magazine of the active weapon holds, attachments included.
    #[func]
    fn magazine_size(&self) -> i32 {
        i32::from(self.state.combat.def().magazine)
    }

    #[func]
    fn is_reloading(&self) -> bool {
        self.state.combat.is_reloading()
    }

    /// 0 at the hip, 1 fully aimed down sights.
    #[func]
    fn ads_amount(&self) -> f32 {
        self.state.combat.ads
    }

    /// 0 unless aiming a scoped weapon; the HUD draws the scope overlay from it.
    #[func]
    fn scope_amount(&self) -> f32 {
        self.state.combat.scope_amount()
    }

    /// Breath left for steadying a scope, 0 to 1.
    #[func]
    fn breath(&self) -> f32 {
        self.state.combat.breath
    }

    #[func]
    fn is_holding_breath(&self) -> bool {
        self.state.combat.holding_breath
    }

    /// Current hip-fire cone half-angle in radians, for the crosshair.
    #[func]
    fn spread_radians(&self) -> f32 {
        self.state
            .combat
            .spread_degrees(&self.state.combat_context(self.env, &self.params.movement))
            .to_radians()
    }

    #[func]
    fn pellet_count(&self) -> i32 {
        i32::from(self.state.combat.def().pellets)
    }

    /// Each pellet's cone half-angle in radians (0 for single-projectile weapons).
    #[func]
    fn pellet_spread_radians(&self) -> f32 {
        self.state.combat.def().pellet_spread.to_radians()
    }

    #[func]
    fn health(&self) -> f32 {
        self.state.health
    }

    #[func]
    fn max_health(&self) -> f32 {
        self.params.health.max_health
    }

    #[func]
    fn is_alive(&self) -> bool {
        self.state.alive()
    }

    /// Seconds until respawn while dead.
    #[func]
    fn respawn_time_left(&self) -> f32 {
        if self.state.alive() {
            0.0
        } else {
            self.respawn_in.max(0.0)
        }
    }

    /// Whether a knife strike is under way (tests).
    #[func]
    fn is_meleeing(&self) -> bool {
        self.state.combat.meleeing()
    }

    /// Whether a weapon inspect is playing (tests).
    #[func]
    fn is_inspecting(&self) -> bool {
        self.viewmodel.bind().inspecting()
    }

    /// Prompt for the pickup or device in front of the player, or empty.
    /// Devices write `{key}` where the interact key goes.
    #[func]
    fn interact_prompt(&self) -> GString {
        let key = self.key_label("interact");
        if let Some(device) = &self.look_device {
            let mut device = device.clone();
            let prompt = device
                .call("interact_prompt", &[])
                .try_to::<GString>()
                .unwrap_or_default();
            return GString::from(&prompt.to_string().replace("{key}", &key));
        }
        let Some(id) = self.look_target else {
            return GString::new();
        };
        let name = &id.def().name;
        let owned = self.state.combat.carries(id);
        let cost = self.pickup_cost(id);
        if cost == 0 {
            return if owned {
                GString::from(&format!("[{key}] Refill {name} ammo"))
            } else {
                GString::from(&format!("[{key}] Take {name}"))
            };
        }
        let what = if owned {
            format!("{name} ammo")
        } else {
            name.clone()
        };
        if self.wallet_points() >= cost {
            GString::from(&format!("[{key}] Buy {what}: {cost}"))
        } else {
            GString::from(&format!("{what}: {cost} points"))
        }
    }

    /// The key or button bound to `action`, for prompts: the gamepad's when
    /// that is in use. From the Settings autoload, which knows the bindings.
    fn key_label(&self, action: &str) -> String {
        let label = self
            .base()
            .get_node_or_null("/root/Settings")
            .map(|mut settings| {
                settings
                    .call(
                        "key_label",
                        &[
                            StringName::from(action).to_variant(),
                            self.controller_input.to_variant(),
                        ],
                    )
                    .to_string()
            });
        label
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| "F".to_owned())
    }

    /// Fills every carried weapon's ammo on the next tick (Zombies' Max Ammo).
    /// Offline only, like the T refill.
    #[func]
    pub fn refill_all_ammo(&mut self) {
        self.refill_requested = true;
    }

    /// Damages the player from a world position. Returns true if it killed them.
    #[func]
    fn take_damage(&mut self, amount: f32, from: Vector3) -> bool {
        let mut events = Vec::new();
        let killed = player::apply_damage(&mut self.state, amount, false, &mut events);
        for event in &events {
            self.handle_vitals(event, from);
        }
        killed
    }

    /// Gives this weapon now, as if taken from a rack (tests and debugging).
    #[func]
    fn give_weapon(&mut self, key: GString) -> bool {
        let Some(id) = WeaponId::by_key(&key.to_string()) else {
            return false;
        };
        let combat = &mut self.state.combat;
        let mut events = Vec::new();
        let input = weapon::CombatInput {
            pickup: Some(id),
            ..Default::default()
        };
        let ctx = weapon::CombatContext::default();
        weapon::step(combat, &input, &ctx, 0.0, &mut events);
        for event in events {
            self.handle_event(&PlayerEvent::Combat(event));
        }
        true
    }

    /// While false, the player ignores devices (menus are open).
    #[func]
    fn set_input_enabled(&mut self, enabled: bool) {
        self.input_enabled = enabled;
        self.latched = Default::default();
        self.next_pressed = false;
        self.wheel_swap.steps = 0.0;
        self.fire_press = None;
        // A key still held when the menu closes is not a fresh press.
        self.interact_held = enabled && Input::singleton().is_action_pressed("interact");
    }

    /// Holds the player still (true) or lets them go: they can still look,
    /// aim, reload, and switch weapons. Online, the server applies the same
    /// filter (`PlayerInput::held_still`).
    #[func]
    pub fn set_movement_locked(&mut self, locked: bool) {
        self.movement_locked = locked;
    }

    #[func]
    fn is_movement_locked(&self) -> bool {
        self.movement_locked
    }

    /// Moves the player instantly and points them along `yaw`, keeping weapons,
    /// health, and score (map secrets, tests). Offline only: online, the server
    /// owns the player's position.
    #[func]
    fn teleport(&mut self, position: Vector3, yaw: f32) {
        self.state.movement.velocity = Vec3::ZERO;
        self.on_floor = false;
        let placed = Predicted {
            state: self.state,
            position: to_sim(position),
            on_floor: false,
        };
        body::place(&mut self.parts(), &self.params.movement, &placed);
        self.step_offset = 0.0;
        self.previous_step_offset = 0.0;
        self.previous_position = position;
        self.current_position = position;
        self.yaw = yaw;
        self.pitch = 0.0;
    }

    /// Points the view: yaw and pitch in radians (spawns, tests, spectating).
    #[func]
    pub(crate) fn set_look(&mut self, yaw: f32, pitch: f32) {
        self.yaw = yaw;
        self.pitch = pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    /// Current camera vertical FOV in degrees, for converting angles to pixels.
    #[func]
    fn camera_fov(&self) -> f32 {
        self.camera.get_fov()
    }

    /// View yaw in radians, without recoil. Used by HUD damage indicators.
    #[func]
    fn view_yaw(&self) -> f32 {
        self.yaw
    }

    /// Where shots go this frame, for feel traces (tests): aim pitch and yaw
    /// in radians (look plus the simulated recoil and sway), and look pitch.
    #[func]
    fn aim_angles(&self) -> Vector3 {
        self.aim_angles
    }

    /// True while a controller plays (the input device setting).
    #[func]
    fn using_gamepad(&self) -> bool {
        self.controller_input
    }

    /// True while controller aim assist is on a target.
    #[func]
    fn aim_assist_engaged(&self) -> bool {
        self.assist_target.is_some()
    }
}

impl FpsPlayer {
    /// Starts recording inputs for prediction (the client joined a server).
    pub(crate) fn start_prediction(&mut self) {
        self.prediction = Some(Prediction::default());
    }

    /// Back to offline play (the connection ended).
    pub(crate) fn stop_prediction(&mut self) {
        self.prediction = None;
        self.view_tick = None;
        self.predicted_hits.clear();
    }

    pub(crate) fn prediction(&self) -> Option<&Prediction> {
        self.prediction.as_ref()
    }

    /// Reconciles with the server's state after one of our inputs. On a
    /// misprediction the body restarts from the server's state and replays
    /// every input the server has not applied yet.
    pub(crate) fn apply_server_state(&mut self, own: &OwnState) -> Reconciled {
        let Some(mut prediction) = self.prediction.take() else {
            return Reconciled::Ignored;
        };
        let mut parts = self.parts();
        let params = self.params;
        let mut discarded = Vec::new();
        let result = prediction.reconcile(own.last_input, &own.predicted, |p, input| {
            // Replays repeat effects the player already saw; drop their events.
            body::place(&mut parts, &params.movement, p);
            discarded.clear();
            body::tick(&mut parts, &params, p, input, &mut discarded);
        });
        self.prediction = Some(prediction);

        if let Reconciled::Corrected { current, .. } = &result {
            body::place(&mut parts, &params.movement, current);
            let respawned = !self.state.alive() && current.state.alive();
            self.state = current.state;
            self.on_floor = current.on_floor;
            let before = self.current_position;
            let after = to_godot(current.position);
            // Keep the tick-to-tick camera interpolation continuous and blend
            // the jump out over a few frames instead.
            self.previous_position += after - before;
            self.current_position = after;
            self.smoothing.correct(to_sim(before), current.position);
            if respawned {
                self.reset_view();
                self.signals().respawned().emit();
            }
        }
        result
    }

    /// Online: the server tick other players are drawn at this frame.
    pub(crate) fn set_view_tick(&mut self, tick: f64) {
        self.view_tick = Some(tick);
    }

    /// Online: the view tick of a fire press the next tick has yet to act
    /// on. Until then the client keeps other players' hitboxes posed at it,
    /// so the local hit test sees what the server will rewind to.
    pub(crate) fn pending_press_view_tick(&self) -> Option<f64> {
        self.fire_press.and_then(|press| press.view_tick)
    }

    /// The server's verdict on one of our shots that hit `victim`.
    pub(crate) fn on_server_hit(&mut self, shot: u32, headshot: bool, killed: bool) {
        if killed {
            let headshot_scale = if headshot { 1.6 } else { 1.0 };
            self.kick_fov(-self.kill_fov_kick * headshot_scale);
            self.signals().hit_confirmed().emit(true, headshot);
        } else if !self.predicted_hits.contains(&shot) {
            // Local hitscan missed but the server hit: show it now.
            self.signals().hit_confirmed().emit(false, headshot);
        }
    }

    /// Another player hit us. Health itself arrives with the next snapshot.
    pub(crate) fn on_server_hurt(&mut self, damage: f32, from: Vector3, killed: bool) {
        self.handle_vitals(
            &PlayerEvent::Damaged {
                amount: damage,
                fall: false,
            },
            from,
        );
        if killed {
            self.handle_vitals(&PlayerEvent::Died, from);
        }
    }

    /// Awards and match totals from the server.
    pub(crate) fn on_server_score(&mut self, awards: &[Scored], totals: ScoreTotals) {
        self.score.set_totals(totals);
        for scored in awards {
            self.signals().scored().emit(
                scored.points as i64,
                &GString::from(scored.award.label().as_str()),
                scored.award.is_medal(),
            );
        }
    }

    /// The state prediction records: simulation plus what the engine owns.
    fn predicted(&self) -> Predicted {
        Predicted {
            state: self.state,
            position: to_sim(self.base().get_global_position()),
            on_floor: self.on_floor,
        }
    }

    fn parts(&self) -> BodyParts {
        BodyParts {
            body: (*self.base()).clone(),
            collision: (*self.collision).clone(),
            headroom: (*self.headroom).clone(),
        }
    }

    fn take_latched(&mut self, action: &str) -> bool {
        let index = LATCHED_ACTIONS
            .iter()
            .position(|a| *a == action)
            .expect("action is latched");
        std::mem::take(&mut self.latched[index])
    }

    /// T in the demo arena refills every carried weapon. This is a demo
    /// convenience applied outside `player::step`, so it is offline only:
    /// online the server owns ammo and a local refill would only mispredict.
    fn refill_pressed(&mut self) -> bool {
        let pressed = Input::singleton().is_action_just_pressed("refill_ammo")
            | self.take_latched("refill_ammo");
        pressed
            && self.allow_refill
            && self.input_enabled
            && self.state.alive()
            && self.prediction.is_none()
    }

    fn read_input(&mut self) -> PlayerInput {
        let press = self.fire_press.take();
        let next = std::mem::take(&mut self.next_pressed);
        if !self.input_enabled {
            self.wish = Vector2::ZERO;
            return PlayerInput {
                yaw: self.yaw,
                pitch: self.pitch,
                ..Default::default()
            };
        }
        let input = Input::singleton();
        let mut wish = input.get_vector("move_left", "move_right", "move_back", "move_forward");
        if self.movement_locked {
            wish = Vector2::ZERO;
        }
        self.wish = wish;
        let mut held = |action: &str| input.is_action_pressed(action) | self.take_latched(action);

        let jump = held("jump");
        let sprint = held("sprint");
        let crouch = held("crouch");
        let fire = held("fire");
        let melee = held("melee");
        let mut reload = held("reload");
        let slot_1 = held("weapon_1");
        let slot_2 = held("weapon_2");
        let mut interact_down = held("interact");
        // On a gamepad the reload button uses whatever is in front of the
        // player (a rack, a door), and reloads otherwise.
        if self.controller_input
            && reload
            && (self.look_device.is_some() || self.look_target.is_some())
        {
            interact_down = true;
            reload = false;
        }
        let mut interact = interact_down && !self.interact_held;
        self.interact_held = interact_down;
        if interact && let Some(device) = &self.look_device {
            let mut device = device.clone();
            let me = self.to_gd().to_variant();
            device.call("interact", &[me]);
            interact = false;
        }
        // A wall weapon with a price is paid for before it is taken.
        if interact
            && let Some(id) = self.look_target
            && self.pickup_cost(id) > 0
            && !self.spend(self.pickup_cost(id))
        {
            interact = false;
        }

        let current = match self.state.combat.action {
            Action::Lowering { to, .. } => to,
            _ => self.state.combat.active,
        };
        let switch_to = if slot_1 {
            Some(0)
        } else if slot_2 {
            Some(1)
        } else if next {
            Some(1 - current)
        } else {
            None
        };

        // A melee press that will start a strike lunges at the enemy in
        // front, if one is close enough.
        let melee_press = melee
            && !self.state.melee_held
            && self.state.alive()
            && self.state.combat.can_melee()
            && !self.state.movement.mantling()
            && !self.movement_locked;
        let lunge = if melee_press {
            self.lunge_target()
        } else {
            None
        };
        let attacking = fire || melee || self.state.combat.meleeing();

        let input = PlayerInput {
            wish: Vec2::new(wish.x, wish.y),
            yaw: self.yaw,
            pitch: self.pitch,
            jump,
            sprint,
            crouch,
            fire,
            aim: input.is_action_pressed("aim"),
            reload,
            switch_to,
            pickup: if interact { self.look_target } else { None },
            melee,
            lunge,
            // A press since the last tick: the server rewinds others to the
            // frame it was clicked on. A knife strike uses the latest view.
            view_tick: match press {
                _ if self.prediction.is_none() => None,
                _ if !fire => {
                    if attacking {
                        self.view_tick
                    } else {
                        None
                    }
                }
                Some(press) => press.view_tick.or(self.view_tick),
                None => self.view_tick,
            },
            fire_aim: press.filter(|_| fire).map(|press| FireAim {
                yaw: press.yaw - self.yaw,
                pitch: press.pitch - self.pitch,
            }),
        };
        if self.movement_locked {
            input.held_still()
        } else {
            input
        }
    }

    /// Where shots leave from: the eye at this tick's crouch blend, as on the
    /// server (the camera sits on the same blend between ticks).
    fn eye_position(&self) -> Vector3 {
        self.current_position + Vector3::UP * self.state.movement.eye_height(&self.params.movement)
    }

    /// The collider a ray from `from` to `to` meets first.
    fn ray(&self, from: Vector3, to: Vector3, mask: u32) -> Option<Gd<Node>> {
        let mut space = self.base().get_world_3d()?.get_direct_space_state()?;
        let exclude: Array<Rid> = array![self.base().get_rid()];
        let query = PhysicsRayQueryParameters3D::create_ex(from, to)
            .collision_mask(mask)
            .exclude(&exclude)
            .done()?;
        space
            .intersect_ray(&query)
            .get_or_nil("collider")
            .try_to::<Gd<Node>>()
            .ok()
    }

    /// The weapon pickup or device under the crosshair, within reach.
    fn find_interactable(&mut self) {
        if !self.state.alive() {
            return;
        }
        let from = self.eye_position();
        let direction = to_godot(player::aim_direction(self.yaw, self.pitch, Vec2::ZERO));
        let Some(collider) = self.ray(
            from,
            from + direction * self.interact_distance,
            WORLD_LAYER | INTERACT_LAYER,
        ) else {
            return;
        };
        match collider.try_cast::<WeaponPickup>() {
            Ok(pickup) => {
                let pickup = pickup.bind();
                self.look_target = pickup.weapon_id();
                self.look_price = pickup.price();
            }
            Err(node) => {
                if node.has_method("interact") && node.has_method("interact_prompt") {
                    self.look_device = Some(node);
                }
            }
        }
    }

    /// What taking `id` from the pickup under the crosshair costs: its price,
    /// or half for ammo of a weapon already carried.
    fn pickup_cost(&self, id: WeaponId) -> i64 {
        if self.state.combat.carries(id) {
            self.look_price / 2
        } else {
            self.look_price
        }
    }

    /// The Zombies wallet (`ZombieDirector`), if this level has one.
    fn wallet(&self) -> Option<Gd<Node>> {
        self.base()
            .get_tree()
            .get_first_node_in_group("zombies_wallet")
    }

    fn wallet_points(&self) -> i64 {
        self.wallet()
            .map(|mut w| w.call("points", &[]).try_to::<i64>().unwrap_or(0))
            .unwrap_or(0)
    }

    /// Pays `amount` from the wallet; false if there is none or too little.
    fn spend(&self, amount: i64) -> bool {
        self.wallet().is_some_and(|mut w| {
            w.call("spend", &[amount.to_variant()])
                .try_to::<bool>()
                .unwrap_or(false)
        })
    }

    /// Remembers which controller is in use, for vibration.
    fn track_gamepad(&mut self, event: &Gd<InputEvent>) {
        if let Ok(motion) = event.clone().try_cast::<InputEventJoypadMotion>() {
            if motion.get_axis_value().abs() >= 0.3 {
                self.gamepad_device = motion.get_device();
            }
        } else if let Ok(button) = event.clone().try_cast::<InputEventJoypadButton>()
            && button.is_pressed()
        {
            self.gamepad_device = button.get_device();
        }
    }

    fn stick_params(&self) -> StickParams {
        StickParams {
            inner_deadzone: self.stick_deadzone.clamp(0.0, 0.9),
            outer_deadzone: self.stick_outer_deadzone.clamp(0.0, 0.5),
            curve: self.stick_curve.max(0.1),
            yaw_speed: self.stick_yaw_speed.to_radians(),
            pitch_speed: self.stick_pitch_speed.to_radians(),
            boost_yaw_speed: self.stick_turn_boost.max(0.0).to_radians(),
            ..StickParams::default()
        }
    }

    /// The aim assist tuning, or `None` when it is off.
    fn assist_params(&self) -> Option<AimAssistParams> {
        self.aim_assist.then(AimAssistParams::default)
    }

    /// Right-stick look for this rendered frame, with aim assist. Scaled by
    /// zoom like the mouse, so aiming down sights turns slower.
    fn apply_stick_look(&mut self, eye: Vector3, ads: f32, delta: f32) {
        if self.controller_input != self.stick_active {
            self.stick_active = self.controller_input;
            self.stick_look = StickLook::default();
            self.assist.reset();
            self.assist_target = None;
        }
        if !self.controller_input {
            return;
        }
        let input = Input::singleton();
        let strength = |action: &str| input.get_action_raw_strength(action);
        let mut raw = Vec2::new(
            strength("look_right") - strength("look_left"),
            strength("look_up") - strength("look_down"),
        );
        if self.stick_invert_y {
            raw.y = -raw.y;
        }
        let params = self.stick_params();
        let stick = look::shape_stick(raw, &params);
        let mut rates = self.stick_look.rates(stick, &params, delta) * self.aim_zoom();

        let assist = self
            .assist_params()
            .filter(|_| !self.state.movement.sprinting);
        if let Some(assist) = assist {
            let moving = input
                .get_vector("move_left", "move_right", "move_back", "move_forward")
                .length()
                > 0.0;
            let view = AssistView {
                eye: to_sim(eye),
                yaw: self.yaw,
                pitch: self.pitch,
                ads,
                active: moving || stick != Vec2::ZERO,
            };
            let targets = self.assist_targets(eye, assist.range);
            let out = self.assist.update(&view, &targets, &assist, delta);
            self.assist_target = out.target;
            rates *= out.slowdown;
            self.turn(out.rotation.x, out.rotation.y);
        } else if self.assist_target.is_some() {
            self.assist.reset();
            self.assist_target = None;
        }
        self.turn(-rates.x * delta, rates.y * delta);
    }

    /// The world FOV multiplier from aiming down sights alone (1 at the hip),
    /// which look sensitivity follows.
    fn aim_zoom(&self) -> f32 {
        let combat = &self.state.combat;
        let ads = crate::viewmodel::ads_ease(combat.ads);
        let aimed_fov = self.base_fov * (1.0 + (combat.def().ads_fov_scale - 1.0) * ads);
        // The ratio of the screen's half-widths, so a pixel of mouse travel
        // covers the same share of the screen at any zoom.
        let half = |fov: f32| (fov.to_radians() * 0.5).tan();
        half(aimed_fov) / half(self.base_fov).max(1e-4)
    }

    /// Turns the view by the recoil climb still pending, a share per frame.
    fn apply_climb(&mut self, dt: f32) {
        if self.pending_climb == Vec2::ZERO {
            return;
        }
        // About 90% lands within 40 ms.
        let step = if self.pending_climb.length() < 1e-5 {
            self.pending_climb
        } else {
            self.pending_climb * smoothing(CLIMB_RATE, dt)
        };
        self.pending_climb -= step;
        self.climb_step += step;
        self.turn(-step.y, step.x);
    }

    /// Turns the view (yaw left and pitch up, in radians).
    fn turn(&mut self, yaw: f32, pitch: f32) {
        self.yaw += yaw;
        self.pitch = (self.pitch + pitch).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    /// Enabled aim assist targets in range with a clear line of sight.
    fn assist_targets(&self, eye: Vector3, range: f32) -> Vec<AssistTarget> {
        let Some(mut space) = self
            .base()
            .get_world_3d()
            .and_then(|w| w.get_direct_space_state())
        else {
            return Vec::new();
        };
        let forward = to_godot(look::view_direction(self.yaw, self.pitch));
        let exclude: Array<Rid> = array![self.base().get_rid()];
        self.base()
            .get_tree()
            .get_nodes_in_group(AIM_ASSIST_GROUP)
            .iter_shared()
            .filter_map(|node| node.try_cast::<AimAssistTarget>().ok())
            .filter_map(|marker| {
                let target = {
                    let marker = marker.bind();
                    if !marker.enabled() {
                        return None;
                    }
                    marker.capsule()
                };
                let to_center = to_godot(target.center()) - eye;
                if to_center.length() > range || to_center.dot(forward) <= 0.0 {
                    return None;
                }
                let visible = [target.center(), target.top].into_iter().any(|point| {
                    line_of_sight(&mut space, eye, to_godot(point), &exclude, &marker)
                });
                visible.then_some(target)
            })
            .collect()
    }

    fn rumble_on_fire(&self, weapon: WeaponId) {
        if !self.rumble || !self.controller_input {
            return;
        }
        let def = weapon.def();
        let (weak, strong, seconds) = match def.fire_mode {
            FireMode::Auto | FireMode::Burst { .. } => (0.35, 0.15, 0.06),
            // Shotguns and snipers: one heavy shot.
            FireMode::Semi if def.damage.near * f32::from(def.pellets) >= 80.0 => (0.8, 0.7, 0.16),
            FireMode::Semi => (0.5, 0.35, 0.09),
        };
        Input::singleton()
            .start_joy_vibration_ex(self.gamepad_device, weak, strong)
            .duration(seconds)
            .done();
    }

    fn handle_event(&mut self, event: &PlayerEvent) {
        match *event {
            PlayerEvent::Combat(combat) => self.handle_combat(combat),
            PlayerEvent::Landed { speed } => {
                self.viewmodel.bind_mut().on_landed(speed);
                // The body sinks into a landing and the head nods: about 5 cm
                // and 1.5 degrees for a jump, more for a longer drop.
                let dip = ((speed - 2.0) * 0.012).clamp(0.0, 0.1);
                let nod = ((speed - 2.0) * 0.35).clamp(0.0, 3.0).to_radians();
                self.land_dip.velocity -= impulse_for_peak(
                    dip,
                    LAND_DIP_STIFFNESS,
                    damping_for(LAND_DIP_STIFFNESS, LAND_DIP_ZETA),
                );
                self.land_nod.velocity += impulse_for_peak(
                    nod,
                    LAND_NOD_STIFFNESS,
                    damping_for(LAND_NOD_STIFFNESS, LAND_NOD_ZETA),
                );
                if speed > 3.0 {
                    let volume = linear_db(((speed - 3.0) / 9.0).clamp(0.25, 1.0));
                    self.play_body("res://assets/audio/player/land", volume);
                    self.play_footstep(volume + footsteps::OWN_STEP_DB);
                }
            }
            PlayerEvent::Damaged { .. } | PlayerEvent::Died => {
                let from = self.current_position;
                self.handle_vitals(event, from);
            }
        }
    }

    fn handle_vitals(&mut self, event: &PlayerEvent, from: Vector3) {
        match *event {
            PlayerEvent::Damaged { amount, fall } => {
                self.flinch_from(amount, fall, from);
                if let Some(mut body) = self.player_sounds.clone() {
                    body.bind_mut().on_damaged(fall);
                }
                self.signals().damaged().emit(amount, from, fall);
            }
            PlayerEvent::Died => {
                self.respawn_in = self.respawn_delay;
                self.dead_for = 0.0;
                self.death_feet = self.current_position;
                self.death_view = Vector2::new(self.yaw, self.pitch);
                // Online, the server counts deaths and sends the totals.
                if self.prediction.is_none() {
                    self.score.on_death();
                }
                if let Some(mut body) = self.player_sounds.clone() {
                    body.bind_mut().on_died();
                }
                self.signals().died().emit();
            }
            _ => {}
        }
    }

    fn handle_combat(&mut self, event: CombatEvent) {
        match event {
            CombatEvent::Fired {
                weapon,
                yaw,
                pitch,
                aim_offset,
                shot,
                climb,
            } => {
                let def = weapon.def();
                self.pending_climb += climb;
                self.fire_shot(weapon, (yaw, pitch), aim_offset, shot);
                self.rumble_on_fire(weapon);
                let ads = self.state.combat.ads;
                let rounds_left = self.state.combat.weapon().magazine;
                let mut rig = self.viewmodel.bind_mut();
                let eject = def.cycle_time <= 0.0 && !def.beam;
                // The last rounds ring hollow in the gunshot itself (`hollow`).
                let punch = rig.on_fired(weapon, eject, rounds_left);
                drop(rig);
                self.kick_camera(punch, ads, shot);
            }
            CombatEvent::DryFire { weapon } => self.viewmodel.bind_mut().on_dry_fire(weapon),
            CombatEvent::RoundLoaded { weapon } => {
                self.viewmodel.bind_mut().on_round_loaded(weapon)
            }
            CombatEvent::Cycled { weapon } => self.viewmodel.bind_mut().on_cycled(weapon),
            CombatEvent::PickedUp { weapon } => self.viewmodel.bind_mut().on_picked_up(weapon),
            CombatEvent::MeleeStarted { lunge, .. } => {
                let strike_in = match self.state.combat.action {
                    Action::Meleeing { strike_in, .. } => strike_in,
                    _ => melee::STRIKE_DELAY,
                };
                self.viewmodel.bind_mut().on_melee(lunge, strike_in);
                if lunge {
                    self.kick_fov(self.slide_fov_kick * 0.8);
                }
            }
            CombatEvent::MeleeStruck { shot, .. } => self.melee_strike(shot),
            // The reload's handling sounds follow its animation (`WeaponView`).
            CombatEvent::ReloadStarted { .. }
            | CombatEvent::ReloadFinished { .. }
            | CombatEvent::Switched { .. }
            | CombatEvent::MagazineSeated { .. } => {}
        }
    }

    /// Pulses the FOV: positive widens, negative zooms in. `peak` is a fraction of the FOV.
    fn kick_fov(&mut self, peak: f32) {
        self.fov_kick.velocity += impulse_for_peak(peak, FOV_KICK_STIFFNESS, FOV_KICK_DAMPING);
    }

    /// Adds one shot's camera punch: a roll whose side and size vary per shot
    /// (hashed, so no metronome), and a small zoom pulse. `degrees.z` is the
    /// peak roll; `degrees.x`, the weapon's pitch punch, sizes the pulse.
    /// Neither moves the screen centre: the sim's recoil turns the view.
    fn kick_camera(&mut self, degrees: Vector3, ads: f32, shot: u32) {
        let scale = 1.0 + (self.ads_punch_scale - 1.0) * ads;
        let side = if rng::unit(shot, 101) < 0.5 {
            -1.0
        } else {
            1.0
        };
        // Aimed, the roll is small: the sight picture should read as the gun
        // kicking, not the world shaking.
        let roll_scale = 1.0 - 0.7 * ads;
        let roll = degrees.z * side * (0.55 + 0.45 * rng::unit(shot, 102)) * roll_scale;
        let impulse = |deg: f32| impulse_for_peak(deg.to_radians(), PUNCH_STIFFNESS, PUNCH_DAMPING);
        self.punch.velocity.z += impulse(roll);
        // A millimetre or two of camera shake: it moves the eye, not the
        // direction it looks, so the aim point stays put.
        let size = 0.0018 * (degrees.z / 0.8).min(2.0);
        let shake = Vector3::new(rng::signed(shot, 103), rng::signed(shot, 104), 0.0) * size;
        let c = damping_for(SHAKE_STIFFNESS, 0.5);
        self.shake.velocity += Vector3::new(
            impulse_for_peak(shake.x, SHAKE_STIFFNESS, c),
            impulse_for_peak(shake.y, SHAKE_STIFFNESS, c),
            0.0,
        );
        self.kick_fov(-(degrees.x * SHOT_ZOOM_PULSE * scale).min(SHOT_ZOOM_PULSE_MAX));
    }

    /// Everything the knife could reach within `range` (horizontal, feet to
    /// feet): living damageable characters that are not friends, each with
    /// its feet, and with nothing solid between the eye and its chest.
    fn melee_candidates(&self, range: f32) -> Vec<(Gd<Node>, Vec3)> {
        let Some(mut space) = self
            .base()
            .get_world_3d()
            .and_then(|world| world.get_direct_space_state())
        else {
            return Vec::new();
        };
        let mut sphere = SphereShape3D::new_gd();
        sphere.set_radius(range + 0.5);
        let mut query = PhysicsShapeQueryParameters3D::new_gd();
        query.set_shape(&sphere);
        query.set_transform(Transform3D::new(
            Basis::IDENTITY,
            self.current_position + Vector3::UP,
        ));
        query.set_collision_mask(HITBOX_LAYER);
        let eye = self.eye_position();
        let me = self.base().instance_id();
        let mut found: Vec<(Gd<Node>, Vec3)> = Vec::new();
        for hit in space
            .intersect_shape_ex(&query)
            .max_results(64)
            .done()
            .iter_shared()
        {
            let Some(target) = hit
                .get("collider")
                .and_then(|c| c.try_to::<Gd<Node>>().ok())
                .as_ref()
                .and_then(find_damageable)
            else {
                continue;
            };
            let id = target.instance_id();
            if id == me
                || found.iter().any(|(t, _)| t.instance_id() == id)
                || target.has_method("apply_prop_hit")
                || (target.has_meta("friendly") && target.get_meta("friendly").booleanize())
            {
                continue;
            }
            let mut probe = target.clone();
            if probe.has_method("is_alive") && !probe.call("is_alive", &[]).booleanize() {
                continue;
            }
            let Ok(node) = target.clone().try_cast::<Node3D>() else {
                continue;
            };
            let feet = node.get_global_position();
            // Solid parts of the target itself (a zombie's body) do not hide it.
            let chest = feet + Vector3::UP * melee::CHEST_HEIGHT;
            let blocked = self.ray(eye, chest, WORLD_LAYER).is_some_and(|hit| {
                hit.instance_id() != target.instance_id() && !target.is_ancestor_of(&hit)
            });
            if blocked {
                continue;
            }
            found.push((target, to_sim(feet)));
        }
        found
    }

    /// The lunge for a melee pressed now: the offset to the enemy the knife
    /// goes for, if one is in lunge range in front (see `shooter_sim::melee`).
    fn lunge_target(&self) -> Option<Vec2> {
        let feet = to_sim(self.current_position);
        let candidates = self.melee_candidates(melee::LUNGE_RANGE);
        let (_, at) = melee::pick(
            feet,
            self.yaw,
            melee::LUNGE_RANGE,
            melee::LUNGE_CONE_DEGREES,
            candidates.iter().map(|(t, at)| (t.instance_id(), *at)),
        )?;
        let offset = melee::lunge_offset(feet, at);
        melee::lunges(offset).then_some(offset)
    }

    /// The knife strike lands (`shot` numbers it): the nearest enemy in reach
    /// in front of the player takes the knife's damage. Online this predicts
    /// the hit marker; the server decides the kill. A strike that meets a wall
    /// instead marks it.
    fn melee_strike(&mut self, shot: u32) {
        let feet = to_sim(self.current_position);
        let eye = self.eye_position();
        let candidates = self.melee_candidates(melee::REACH);
        let picked = melee::pick(
            feet,
            self.yaw,
            melee::REACH,
            melee::STRIKE_CONE_DEGREES,
            candidates.iter().enumerate().map(|(i, (_, at))| (i, *at)),
        );
        let Some((index, at)) = picked else {
            let forward = to_godot(player::aim_direction(self.yaw, self.pitch, Vec2::ZERO));
            let wall = self.base().get_world_3d().and_then(|world| {
                let mut space = world.get_direct_space_state()?;
                let exclude: Array<Rid> = array![self.base().get_rid()];
                let query = PhysicsRayQueryParameters3D::create_ex(
                    eye,
                    eye + forward * (melee::REACH * 0.8),
                )
                .collision_mask(WORLD_LAYER)
                .exclude(&exclude)
                .done()?;
                let hit = space.intersect_ray(&query);
                let position = hit.get("position")?.try_to::<Vector3>().ok()?;
                let normal = hit.get("normal")?.try_to::<Vector3>().ok()?;
                let collider = hit
                    .get("collider")
                    .and_then(|c| c.try_to::<Gd<Node>>().ok());
                Some((position, normal, collider))
            });
            let struck_wall = wall.is_some();
            if let Some((position, normal, collider)) = wall {
                let surface = surface_of(collider.as_ref());
                self.spawn_impact(position, normal, false, &surface, &surface, forward);
            }
            self.viewmodel
                .bind_mut()
                .on_melee_struck(struck_wall, false);
            return;
        };
        let mut target = candidates[index].0.clone();
        let target_feet = to_godot(at);
        let chest = target_feet + Vector3::UP * melee::CHEST_HEIGHT;
        let direction = (target_feet - self.current_position).normalized_or_zero();
        let killed = if target.has_method("apply_melee") {
            target.call("apply_melee", &[melee::DAMAGE.to_variant()])
        } else {
            target.call(
                "apply_damage",
                &[melee::DAMAGE.to_variant(), false.to_variant()],
            )
        }
        .try_to::<bool>()
        .unwrap_or(false);
        if target.has_method("react_to_hit") {
            let force = protocol::HitPart::new(None, melee::DAMAGE, true).force;
            target.call(
                "react_to_hit",
                &[
                    direction.to_variant(),
                    killed.to_variant(),
                    chest.to_variant(),
                    (shooter_sim::hitbox::CHEST_SHAPE as i64).to_variant(),
                    f32::from(force).to_variant(),
                ],
            );
        }
        let normal = -direction;
        self.spawn_impact(
            chest - direction * 0.25,
            normal,
            false,
            "flesh",
            "flesh",
            direction,
        );
        self.viewmodel.bind_mut().on_melee_struck(true, true);
        self.signals().hit_confirmed().emit(killed, false);
        if self.prediction.is_some() {
            self.predicted_hits.push_back(shot);
            if self.predicted_hits.len() > 64 {
                self.predicted_hits.pop_front();
            }
        }
        if killed {
            self.kick_fov(-self.kill_fov_kick);
            let kill = Kill {
                distance: eye.distance_to(chest),
                headshot: false,
            };
            for scored in self.score.on_shot_kills(&self.score_rules, &[kill]) {
                self.signals().scored().emit(
                    scored.points as i64,
                    &GString::from(scored.award.label().as_str()),
                    scored.award.is_medal(),
                );
            }
        }
    }

    /// Hitscan from the eye along the shot's view angles (the click's, for a
    /// press between ticks) plus recoil, spread, and sway, once per pellet.
    /// Damage is summed per target so a shotgun blast confirms one hit, not
    /// eight.
    fn fire_shot(&mut self, weapon: WeaponId, view: (f32, f32), aim_offset: Vec2, shot: u32) {
        let def = weapon.def();
        let origin = self.eye_position();
        // Per target: summed damage, any headshot, direction of the first pellet
        // to hit, its distance, and where it landed (point and hit shape).
        let mut targets: Vec<ShotTarget> = Vec::new();
        let mut first_end = None;

        let Some(mut space) = self
            .base()
            .get_world_3d()
            .and_then(|world| world.get_direct_space_state())
        else {
            return;
        };
        let exclude: Array<Rid> = array![self.base().get_rid()];

        for pellet in 0..def.pellets {
            let offset = aim_offset + weapon::pellet_offset(weapon, shot, pellet);
            let direction = to_godot(player::aim_direction(view.0, view.1, offset));
            // Through thin cover, losing damage at each wall.
            let path = penetration::trace(
                &mut space,
                origin,
                direction,
                def.range,
                WORLD_LAYER | HITBOX_LAYER,
                &exclude,
                def.penetration,
            );
            first_end.get_or_insert(path.end);
            for hit in path.hits {
                // Teammates take no damage online, so they confirm no hits either.
                let target =
                    hit.collider.as_ref().and_then(find_damageable).filter(|t| {
                        !t.has_meta("friendly") || !t.get_meta("friendly").booleanize()
                    });
                if let (Some(collider), Some(target)) = (&hit.collider, target) {
                    // World props do not award character hit/kill confirmations.
                    // Online their manager ignores this predicted hit; the server
                    // owns damage.
                    if target.has_method("apply_prop_hit") {
                        let distance = origin.distance_to(hit.position);
                        let mut prop = target;
                        prop.call(
                            "apply_prop_hit",
                            &[
                                (def.damage_at(distance) * hit.damage_scale).to_variant(),
                                direction.to_variant(),
                            ],
                        );
                        let surface = surface_of(Some(collider));
                        self.spawn_impact(
                            hit.position,
                            hit.normal,
                            false,
                            &surface,
                            &surface,
                            direction,
                        );
                        continue;
                    }
                    // Untagged damageable colliders (the sentry) count as the chest.
                    let zone = HitZone::from_name(
                        &collider
                            .get_meta_ex("hit_zone")
                            .default(&"chest".to_variant())
                            .done()
                            .to_string(),
                    )
                    .unwrap_or(HitZone::Chest);
                    let headshot = zone == HitZone::Head;
                    let distance = origin.distance_to(hit.position);
                    let damage = def.damage_at(distance) * def.zone_scale(zone) * hit.damage_scale;
                    match targets
                        .iter_mut()
                        .find(|(t, ..)| t.instance_id() == target.instance_id())
                    {
                        Some(entry) => {
                            entry.1 += damage;
                            entry.2 |= headshot;
                        }
                        None => {
                            let part = collider
                                .get_meta_ex("hit_part")
                                .default(&(-1i64).to_variant())
                                .done()
                                .try_to::<i64>()
                                .unwrap_or(-1);
                            targets.push((
                                target,
                                damage,
                                headshot,
                                direction,
                                distance,
                                hit.position,
                                part,
                            ));
                        }
                    }
                    let surface =
                        surface_meta(Some(collider)).unwrap_or_else(|| "flesh".to_owned());
                    self.spawn_impact(
                        hit.position,
                        hit.normal,
                        false,
                        &surface,
                        &surface,
                        direction,
                    );
                } else {
                    let surface = surface_at(hit.collider.as_ref(), hit.face_index);
                    // Shotguns leave a decal for only some pellets to limit clutter.
                    let decal = def.pellets == 1 || pellet % 3 == 0;
                    if hit.exit {
                        self.spawn_exit(hit.position, hit.normal, decal, surface);
                    } else {
                        self.spawn_impact(
                            hit.position,
                            hit.normal,
                            decal,
                            surface.impact_key(),
                            surface.key(),
                            direction,
                        );
                    }
                }
            }
        }

        let mut confirmed = None;
        let mut kills = Vec::new();
        for (mut target, damage, headshot, direction, distance, point, part) in targets {
            let killed = target
                .call(
                    "apply_damage",
                    &[damage.to_variant(), headshot.to_variant()],
                )
                .try_to::<bool>()
                .unwrap_or(false);
            if target.has_method("react_to_hit") {
                let force = protocol::HitPart::new(None, damage, false).force;
                target.call(
                    "react_to_hit",
                    &[
                        direction.to_variant(),
                        killed.to_variant(),
                        point.to_variant(),
                        part.to_variant(),
                        f32::from(force).to_variant(),
                    ],
                );
            }
            if killed {
                let headshot_scale = if headshot { 1.6 } else { 1.0 };
                self.kick_fov(-self.kill_fov_kick * headshot_scale);
                kills.push(Kill { distance, headshot });
            }
            let (k, h) = confirmed.unwrap_or((false, false));
            confirmed = Some((k | killed, h | headshot));
        }
        if let Some((killed, headshot)) = confirmed {
            self.signals().hit_confirmed().emit(killed, headshot);
            if self.prediction.is_some() {
                self.predicted_hits.push_back(shot);
                if self.predicted_hits.len() > 64 {
                    self.predicted_hits.pop_front();
                }
            }
        }
        for scored in self.score.on_shot_kills(&self.score_rules, &kills) {
            self.signals().scored().emit(
                scored.points as i64,
                &GString::from(scored.award.label().as_str()),
                scored.award.is_medal(),
            );
        }

        let Some(end) = first_end else { return };
        if def.beam {
            if let Some(scene) = self.beam_scene.clone() {
                self.spawn_streak(&scene, end);
            }
        } else if def.tracer_every > 0 && shot.is_multiple_of(u32::from(def.tracer_every)) {
            let scene = (*self.tracer_scene).clone();
            self.spawn_streak(&scene, end);
        }
    }

    /// Tracers and beams start near the muzzle: the viewmodel is drawn with its
    /// own FOV, so this is a camera-relative approximation that reads right on
    /// screen.
    fn spawn_streak(&mut self, scene: &Gd<PackedScene>, end: Vector3) {
        let ads = self.state.combat.ads;
        let hip = Vector3::new(0.09, -0.1, -0.55);
        let aimed = Vector3::new(0.0, -0.05, -0.55);
        let start = self.camera.get_global_transform() * hip.lerp(aimed, ads);
        let Some(mut tracer) = scene
            .instantiate()
            .and_then(|n| n.try_cast::<Node3D>().ok())
        else {
            return;
        };
        let Some(mut parent) = self.base().get_parent() else {
            return;
        };
        parent.add_child(&tracer);
        tracer.call("setup", &[start.to_variant(), end.to_variant()]);
    }

    /// An impact effect on `surface` (its look, or a `Surface` tag), and the
    /// impact sound of `sound` (a `Surface` key: every surface has its own).
    /// Flesh bleeds along the round's `direction`; the world keeps a bullet
    /// hole if `decal`.
    fn spawn_impact(
        &mut self,
        position: Vector3,
        normal: Vector3,
        decal: bool,
        surface: &str,
        sound: &str,
        direction: Vector3,
    ) {
        // The effect is visual; its sound plays from the tracer's pool.
        if let Some(mut tracer) = self.base().try_get_node_as::<ShotTracer>("ShotTracer") {
            tracer.bind_mut().impact(position, GString::from(sound));
        }
        let Some(mut effects) = self
            .base()
            .try_get_node_as::<CombatEffects>("CombatEffects")
        else {
            return;
        };
        let look = Surface::from_tag(surface).map_or("concrete", |s| s.impact_key());
        if surface == "flesh" {
            effects.bind_mut().blood(position, direction);
        } else {
            effects
                .bind_mut()
                .impact(position, normal, GString::from(look), decal);
        }
    }

    /// Where a round leaves the far side of cover: the exit hole and its
    /// spray along the round, and the surface's impact sound.
    fn spawn_exit(&mut self, position: Vector3, normal: Vector3, decal: bool, surface: Surface) {
        if let Some(mut tracer) = self.base().try_get_node_as::<ShotTracer>("ShotTracer") {
            tracer
                .bind_mut()
                .impact(position, GString::from(surface.key()));
        }
        if let Some(mut effects) = self
            .base()
            .try_get_node_as::<CombatEffects>("CombatEffects")
        {
            effects
                .bind_mut()
                .impact_at(position, normal, surface.impact_key(), decal, true);
        }
    }

    /// Steps once per stride on the ground, with the gait rules every player
    /// shares (`footsteps::Gait`): sprinting strides longer and louder,
    /// crouching and slow walking (aiming down sights) are quieter.
    fn advance_footsteps(&mut self, moved: f32) {
        let movement = self.state.movement;
        if !self.env.on_floor || movement.stance == Stance::Sliding || !self.state.alive() {
            return;
        }
        let speed = Vector2::new(movement.velocity.x, movement.velocity.z).length();
        let gait = Gait::of(
            movement.stance,
            movement.sprinting,
            speed,
            self.params.movement.walk_speed,
        );
        if self.stride.advance(moved, gait)
            && let Some(gait) = gait
        {
            let volume = gait.sound().level_db + footsteps::OWN_STEP_DB;
            self.play_footstep(volume);
            if let Some(mut body) = self.player_sounds.clone() {
                body.bind_mut().on_step(gait, volume);
            }
        }
    }

    fn play_footstep(&mut self, volume_db: f32) {
        let surface = footsteps::floor_surface(&self.to_gd().upcast(), self.current_position);
        self.last_surface = surface;
        let path = format!("res://assets/audio/footsteps/{}", surface.footstep_key());
        let Some(stream) = self.sound(&path, 1.08) else {
            return;
        };
        self.footsteps.set_stream(&stream);
        self.footsteps.set_volume_db(volume_db);
        self.footsteps.play();
    }

    fn play_body(&mut self, base: &str, volume_db: f32) {
        let Some(stream) = self.sound(base, 1.05) else {
            return;
        };
        self.body_sounds.set_stream(&stream);
        self.body_sounds.set_volume_db(volume_db);
        self.body_sounds.play();
    }

    fn sound(&mut self, base: &str, pitch: f32) -> Option<Gd<AudioStreamRandomizer>> {
        self.sounds
            .entry(base.to_owned())
            .or_insert_with(|| audio::variations(base, pitch))
            .clone()
    }

    /// A hit's flinch, away from where it came from: a shot from the right
    /// turns the head left and rolls it, one from in front tips it up, one
    /// from behind tips it down. A fall jolts the head down.
    fn flinch_from(&mut self, amount: f32, fall: bool, from: Vector3) {
        let strength = (amount / 30.0).min(1.5);
        if fall {
            self.flinch.velocity -= 0.6 * strength;
            return;
        }
        self.last_attacker = Some(from);
        let to = from - self.current_position;
        let flat = Vector2::new(to.x, to.z);
        if flat.length_squared() < 1e-4 {
            self.flinch.velocity += 0.4 * strength;
            return;
        }
        // The attacker in the view's frame: x right, y forward.
        let (forward, right) = shooter_sim::movement::basis_from_yaw(self.yaw);
        let dir = flat.normalized();
        let side = dir.x * right.x + dir.y * right.z;
        let ahead = dir.x * forward.x + dir.y * forward.z;
        self.flinch.velocity += 0.4 * strength * ahead.clamp(-0.6, 1.0);
        // Positive yaw turns left: a hit from the right pushes the head left.
        self.flinch_yaw.velocity += 0.35 * strength * side;
        self.flinch_roll.velocity -= 0.5 * strength * side;
    }

    /// The camera once dead: it drops with the body for a moment, then rises
    /// behind where the player fell and turns to face whoever killed them
    /// (or keeps the last view, for a fall), clear of walls. Eased in from
    /// the first-person view by `death_blend`.
    fn death_camera(&mut self, first_person: Transform3D, dt: f32) -> Transform3D {
        self.dead_for += dt;
        let feet = self.death_feet;
        let target = self.last_attacker.map_or_else(
            || {
                let (forward, _) = shooter_sim::movement::basis_from_yaw(self.death_view.x);
                feet + to_godot(forward) * 6.0 + Vector3::UP * 1.2
            },
            |a| a + Vector3::UP * 1.4,
        );
        let flat = Vector3::new(target.x - feet.x, 0.0, target.z - feet.z);
        let toward = flat.try_normalized().unwrap_or(Vector3::FORWARD);
        // Drop first (the body falls), then rise and back off.
        let fall = smoothing(6.0, self.dead_for.min(0.4)) / smoothing(6.0, 0.4);
        let rise = smoothing(3.0, (self.dead_for - 0.35).max(0.0));
        let low = feet + Vector3::UP * 0.5;
        let high = feet - toward * DEATH_CAMERA_BACK + Vector3::UP * DEATH_CAMERA_HEIGHT;
        let mut wanted = first_person.origin.lerp(low, fall * 0.8).lerp(high, rise);
        // Not through a wall: stop short of anything between the body and it.
        let from = feet + Vector3::UP * 0.9;
        if let Some(hit) = self.ray_point(from, wanted) {
            wanted = hit + (from - hit).normalized() * 0.25;
        }
        let looking = Transform3D::new(Basis::IDENTITY, wanted).looking_at(target);
        let turn = smoothing(2.5, self.dead_for);
        let basis = first_person
            .basis
            .get_quaternion()
            .slerp(looking.basis.get_quaternion(), turn);
        Transform3D::new(Basis::from_quaternion(basis), wanted)
    }

    /// The point where a ray from `from` to `to` first meets the world.
    fn ray_point(&self, from: Vector3, to: Vector3) -> Option<Vector3> {
        let mut space = self.base().get_world_3d()?.get_direct_space_state()?;
        let exclude: Array<Rid> = array![self.base().get_rid()];
        let query = PhysicsRayQueryParameters3D::create_ex(from, to)
            .collision_mask(WORLD_LAYER)
            .exclude(&exclude)
            .done()?;
        space
            .intersect_ray(&query)
            .get("position")
            .and_then(|p| p.try_to::<Vector3>().ok())
    }

    /// Head bob, strafe tilt, and slide tilt: (bob offset, bob roll, tilt) in
    /// metres and radians. Zero when camera motion is turned off.
    fn camera_motion_offsets(&mut self, dt: f32, ads: f32) -> (Vector2, f32, f32) {
        let movement = self.state.movement;
        let speed = Vector2::new(movement.velocity.x, movement.velocity.z).length();
        let walking = self.env.on_floor && movement.stance != Stance::Sliding && self.state.alive();
        let target = if walking {
            (speed / self.params.movement.walk_speed).min(1.5)
        } else {
            0.0
        };
        self.bob_blend += (target - self.bob_blend) * smoothing(10.0, dt);

        let slide_tilt = if movement.stance == Stance::Sliding {
            0.07
        } else {
            0.0
        };
        let strafe_tilt = -self.wish.x * 0.012 * (1.0 - ads);
        self.tilt += (slide_tilt + strafe_tilt - self.tilt) * smoothing(8.0, dt);
        if !self.camera_motion {
            return (Vector2::ZERO, 0.0, 0.0);
        }

        // One dip per footstep: the stride phase drives the bob, so the camera
        // and the footstep sounds stay in sync.
        // Two steps make one sideways sway; tracking which foot is down keeps
        // the sway and roll continuous across each footstep.
        let step = self.stride.phase();
        if step + 0.5 < self.last_stride_phase {
            self.stride_foot = 1.0 - self.stride_foot;
        }
        self.last_stride_phase = step;
        let theta = (step + self.stride_foot) * PI;
        let amount = self.bob_blend * (1.0 - 0.85 * ads);
        // The head is lowest just after each footfall, as the knee takes the
        // weight, and highest mid-stride: a smooth dip, never a snap.
        let dip = 0.5 + 0.5 * ((step - 0.08) * 2.0 * PI).cos();
        let bob = Vector2::new(theta.sin() * 0.012 * amount, -dip * 0.022 * amount);
        let roll = theta.sin() * 0.004 * amount;
        (bob, roll, self.tilt)
    }

    fn view_frame(&self, ads: f32) -> ViewFrame {
        let combat = &self.state.combat;
        let progress = |remaining: f32, total: f32| 1.0 - (remaining / total).clamp(0.0, 1.0);
        let def = combat.def();
        let action = match combat.action {
            Action::Ready => ViewAction::Ready,
            Action::Reloading { remaining, empty } => {
                let total = def.reload.magazine_time(empty).unwrap_or(remaining);
                ViewAction::Reloading {
                    elapsed: (total - remaining).max(0.0),
                    empty,
                }
            }
            // Round-by-round: the time into the current stage.
            Action::Loading { remaining, stage } => {
                let Reload::PerRound {
                    start,
                    per_round,
                    end,
                } = def.reload
                else {
                    return self.frame_with(ads, ViewAction::Ready);
                };
                let length = match stage {
                    LoadStage::Opening => start,
                    LoadStage::Round => per_round,
                    LoadStage::Closing => end,
                };
                ViewAction::Loading {
                    stage,
                    elapsed: (length - remaining).clamp(0.0, length),
                    length,
                }
            }
            Action::Cycling { remaining } => {
                ViewAction::Cycling(progress(remaining, def.cycle_time))
            }
            Action::Lowering { remaining, .. } => {
                ViewAction::Lowering(progress(remaining, def.lower_time))
            }
            Action::Raising { remaining } => {
                ViewAction::Raising(progress(remaining, def.raise_time))
            }
            Action::Meleeing { strike_in, .. } => ViewAction::Meleeing { strike_in },
        };
        self.frame_with(ads, action)
    }

    fn frame_with(&self, ads: f32, action: ViewAction) -> ViewFrame {
        let combat = &self.state.combat;
        let velocity = self.state.movement.velocity;
        // Velocity in the view's frame: x right, y up, z forward.
        let (sin, cos) = self.yaw.sin_cos();
        let local = Vector3::new(
            velocity.x * cos - velocity.z * sin,
            velocity.y,
            -(velocity.x * sin + velocity.z * cos),
        );
        let def = combat.def();
        let stance = self.state.movement.stance;
        ViewFrame {
            look_velocity: self.look_velocity,
            velocity: local,
            stride_phase: self.stride.phase(),
            sprint_out: if def.sprint_out_time > 0.0 {
                (combat.sprint_out / def.sprint_out_time).clamp(0.0, 1.0)
            } else {
                0.0
            },
            crouched: stance == Stance::Crouching,
            sliding: stance == Stance::Sliding,
            weapon: combat.weapon().id,
            ads,
            scope: combat.scope_amount(),
            visible: self.state.alive(),
            sprinting: self.state.movement.sprinting,
            on_floor: self.env.on_floor,
            action,
            empty: combat.weapon().magazine == 0,
        }
    }

    /// A fresh player state: full health and the starting weapons.
    fn spawned_state(&self) -> PlayerState {
        let mut state = PlayerState::spawned(&self.params);
        if !self.start_with_primary {
            state.combat = state.combat.without(Slot::Primary);
        }
        state
    }

    fn respawn(&mut self) {
        self.state = self.spawned_state();
        let spawn = self.spawn;
        let mut body = self.base_mut();
        body.set_global_position(spawn.origin);
        body.set_velocity(Vector3::ZERO);
        drop(body);
        self.on_floor = false;
        self.yaw = spawn.basis.get_euler().y;
        self.pitch = 0.0;
        self.reset_view();
        self.signals().respawned().emit();
    }

    fn reset_view(&mut self) {
        let position = self.base().get_global_position();
        self.step_offset = 0.0;
        self.previous_step_offset = 0.0;
        self.previous_position = position;
        self.current_position = position;
        self.previous_recoil = Vec2::ZERO;
        self.previous_sway = Vec2::ZERO;
        self.previous_ads = 0.0;

        self.previous_crouch = self.state.movement.crouch;
        self.fov_sprint = 1.0;
        self.punch = Spring::default();
        self.pending_climb = Vec2::ZERO;
        self.shake = Spring::default();
        self.fov_kick = Spring::default();
        // Every camera offset starts over: no death slump, landing dip,
        // flinch, tilt, or bob carried into the new life.
        self.death_blend = 0.0;
        self.land_dip = Spring::default();
        self.land_nod = Spring::default();
        self.flinch = Spring::default();
        self.flinch_yaw = Spring::default();
        self.flinch_roll = Spring::default();
        self.last_attacker = None;
        self.dead_for = 0.0;
        self.tilt = 0.0;
        self.bob_blend = 0.0;
        self.stride_foot = 0.0;
        self.last_stride_phase = 0.0;
        self.climb_step = Vec2::ZERO;
        self.look_velocity = Vector2::ZERO;
        self.last_look = Vector2::new(self.yaw, self.pitch);
        self.stride = Stride {
            travelled: 0.0,
            length: 2.0,
        };
        self.assist.reset();
        self.assist_target = None;
        let crouch = self.state.movement.crouch;
        body::apply_stance_shape(&mut self.parts(), &self.params.movement, crouch);
    }
}

/// True if nothing in the world blocks the line from `eye` to `point`, other
/// than the target's own body.
fn line_of_sight(
    space: &mut Gd<PhysicsDirectSpaceState3D>,
    eye: Vector3,
    point: Vector3,
    exclude: &Array<Rid>,
    marker: &Gd<AimAssistTarget>,
) -> bool {
    let Some(query) = PhysicsRayQueryParameters3D::create_ex(eye, point)
        .collision_mask(WORLD_LAYER)
        .exclude(exclude)
        .done()
    else {
        return false;
    };
    let hit = space.intersect_ray(&query);
    if hit.is_empty() {
        return true;
    }
    hit.get_or_nil("collider")
        .try_to::<Gd<Node>>()
        .is_ok_and(|collider| collider.is_ancestor_of(marker))
}

/// Walks up from a hit collider to the first node that can take damage.
pub(crate) fn find_damageable(collider: &Gd<Node>) -> Option<Gd<Node>> {
    let mut node = Some(collider.clone());
    while let Some(current) = node {
        if current.has_method("apply_damage") {
            return Some(current);
        }
        node = current.get_parent();
    }
    None
}

/// The `surface` metadata on a collider or its nearest ancestor that has it.
fn surface_meta(collider: Option<&Gd<Node>>) -> Option<String> {
    let mut node = collider.cloned();
    while let Some(current) = node {
        if current.has_meta("surface") {
            return Some(current.get_meta("surface").to_string());
        }
        node = current.get_parent();
    }
    None
}

/// Like [`surface_meta`], defaulting to concrete for untagged world geometry.
pub(crate) fn surface_of(collider: Option<&Gd<Node>>) -> String {
    surface_meta(collider).unwrap_or_else(|| DEFAULT_SURFACE.to_owned())
}

fn linear_db(linear: f32) -> f32 {
    20.0 * linear.max(1e-4).log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wheel_notch_swaps_once() {
        let mut wheel = WheelSwap::default();
        assert!(wheel.step(1_000, 1.0));
        // The rest of the flick lands within the cooldown.
        assert!(!wheel.step(1_016, 1.0));
        assert!(!wheel.step(1_040, 1.0));
        // A deliberate second scroll swaps back.
        assert!(wheel.step(1_400, 1.0));
    }

    #[test]
    fn partial_steps_add_up_to_one_swap() {
        let mut wheel = WheelSwap::default();
        let swaps = (0..8).filter(|i| wheel.step(1_000 + i * 8, 0.125)).count();
        assert_eq!(swaps, 1);
    }

    #[test]
    fn stray_partial_steps_do_not_add_up_over_time() {
        let mut wheel = WheelSwap::default();
        for i in 0..8 {
            assert!(!wheel.step(1_000 + i * 500, 0.25));
        }
    }

    #[test]
    fn an_unsized_step_is_a_whole_notch() {
        let mut wheel = WheelSwap::default();
        assert!(wheel.step(1_000, 0.0));
    }
}
