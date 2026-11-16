//! First-person player: input, simulation, hit detection, camera, and viewmodel.
//!
//! Physics runs at the fixed tick (`physics_process`) and calls the shared
//! simulation in `shooter-sim`. The camera is a top-level node updated every
//! rendered frame: position, recoil, and scope sway are interpolated between the
//! last two ticks, while mouse look is applied immediately, so aiming never
//! waits for a physics tick on high-refresh displays.
//!
//! Gamepad look also runs every rendered frame: stick shaping, turn boost, and
//! aim assist (slowdown and rotational) from `shooter_sim::look`. Aim assist
//! only applies while the last input came from a gamepad.

use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI};

use godot::classes::input::MouseMode;
use godot::classes::{
    AudioStreamPlayer, AudioStreamRandomizer, Camera3D, CapsuleShape3D, CharacterBody3D,
    CollisionShape3D, Engine, ICharacterBody3D, Input, InputEvent, InputEventJoypadButton,
    InputEventJoypadMotion, InputEventKey, InputEventMouseButton, InputEventMouseMotion, Node3D,
    PackedScene, PhysicsDirectSpaceState3D, PhysicsRayQueryParameters3D, ShapeCast3D,
};
use godot::prelude::*;
use shooter_sim::look::{
    self, AimAssist, AimAssistParams, AssistTarget, AssistView, StickLook, StickParams,
};
use shooter_sim::movement::{MoveEnv, Stance};
use shooter_sim::player::{self, PlayerEvent, PlayerInput, PlayerParams, PlayerState};
use shooter_sim::weapon::{self, Action, CombatEvent, FireMode, LoadStage, Reload, WeaponId};
use shooter_sim::{Vec2, Vec3, rng};

use crate::aim_assist::{AIM_ASSIST_GROUP, AimAssistTarget};
use crate::audio;
use crate::convert::{to_godot, to_sim};
use crate::pickup::{INTERACT_LAYER, WeaponPickup};
use crate::spring::{Spring, impulse_for_peak, smoothing};
use crate::target::{HITBOX_LAYER, WORLD_LAYER};
use crate::viewmodel::{ViewAction, ViewFrame, ViewmodelRig};

const PITCH_LIMIT: f32 = FRAC_PI_2 - 0.01;
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

/// Actions whose presses are remembered until the next physics tick, so a tap
/// shorter than one tick (or a mouse-wheel step) is never lost.
const LATCHED_ACTIONS: [&str; 10] = [
    "jump",
    "crouch",
    "sprint",
    "fire",
    "reload",
    "weapon_next",
    "weapon_1",
    "weapon_2",
    "interact",
    "refill_ammo",
];

/// What a ray hit: position, normal, and collider.
struct RayHit {
    position: Vector3,
    normal: Vector3,
    collider: Option<Gd<Node>>,
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
    /// Fraction of each weapon's camera punch kept while fully aimed down sights.
    #[export]
    #[init(val = 0.6)]
    ads_punch_scale: f32,
    /// Head bob and strafe or slide tilt. Landing and damage still move the camera.
    #[export]
    #[init(val = true)]
    camera_motion: bool,
    #[export]
    #[init(val = 3.0)]
    respawn_delay: f32,
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
    /// Controller aim assist: slowdown and rotational, never a snap. Mouse
    /// aiming never gets it.
    #[export]
    #[init(val = true)]
    aim_assist: bool,
    /// Scales aim assist: 0 is none, 1 is the default tuning.
    #[export(range = (0.0, 1.5))]
    #[init(val = 1.0)]
    aim_assist_strength: f32,
    /// Controller vibration when firing.
    #[export]
    #[init(val = true)]
    rumble: bool,
    /// Spawned where bullets hit the world.
    #[export]
    impact_scene: OnEditor<Gd<PackedScene>>,
    /// Bullet tracer, spawned for every Nth round (`tracer_every` in the weapon file).
    #[export]
    tracer_scene: OnEditor<Gd<PackedScene>>,

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
    input_enabled: bool,
    env: MoveEnv,
    yaw: f32,
    pitch: f32,
    wish: Vector2,
    previous_position: Vector3,
    current_position: Vector3,
    previous_recoil: Vec2,
    previous_sway: Vec2,
    previous_ads: f32,
    eye_height: f32,
    fov_sprint: f32,
    /// Visual-only camera offset: x pitch up, y yaw left, z roll (radians).
    punch: Spring,
    /// FOV offset as a fraction of the FOV.
    fov_kick: Spring<f32>,
    spawn: Transform3D,
    respawn_in: f32,
    death_blend: f32,
    /// Metres walked since the last footstep.
    stride: f32,
    stride_length: f32,
    bob_blend: f32,
    tilt: f32,
    land_dip: Spring<f32>,
    flinch: Spring<f32>,
    look_target: Option<WeaponId>,
    sounds: HashMap<String, Option<Gd<AudioStreamRandomizer>>>,
    /// The last gameplay input came from a gamepad (not the mouse or keyboard).
    gamepad: bool,
    gamepad_device: i32,
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
        self.state = PlayerState::spawned(&self.params);

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
        }
    }

    fn input(&mut self, event: Gd<InputEvent>) {
        if !self.input_enabled {
            return;
        }
        let mut input = Input::singleton();
        let captured = input.get_mouse_mode() == MouseMode::CAPTURED;

        self.track_device(&event);
        for (latched, action) in self.latched.iter_mut().zip(LATCHED_ACTIONS) {
            if event.is_action_pressed(action) {
                *latched = true;
            }
        }
        let event = match event.try_cast::<InputEventMouseMotion>() {
            Ok(motion) => {
                if captured {
                    // Screen-relative motion ignores window stretch; scaling by the
                    // FOV keeps aim speed consistent when zoomed in (ADS, scopes).
                    let ads = self.state.combat.ads;
                    let zoom = self.camera.get_fov() / self.base_fov
                        * (1.0 + (self.ads_sensitivity_scale - 1.0) * ads);
                    let delta = motion.get_screen_relative() * self.mouse_sensitivity * zoom;
                    self.yaw -= delta.x;
                    self.pitch = (self.pitch - delta.y).clamp(-PITCH_LIMIT, PITCH_LIMIT);
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
        self.look_target = self.find_pickup();
        let input = self.read_input();
        self.headroom.force_shapecast_update();
        let on_floor = self.base().is_on_floor();
        self.env = MoveEnv {
            on_floor,
            can_stand: !self.headroom.is_colliding(),
        };

        self.previous_recoil = self.state.combat.recoil;
        self.previous_sway = self.state.combat.scope_sway();
        self.previous_ads = self.state.combat.ads;
        let previous_stance = self.state.movement.stance;
        let mut events = std::mem::take(&mut self.events);
        events.clear();
        if self.refill_pressed() {
            let mut combat = Vec::new();
            self.state.combat.refill_ammo(&mut combat);
            events.extend(combat.into_iter().map(PlayerEvent::Combat));
        }
        let velocity = player::step(
            &mut self.state,
            &input,
            self.env,
            &self.params,
            dt,
            &mut events,
        );
        let stance = self.state.movement.stance;
        let movement = &self.params.movement;
        if movement.height(previous_stance) != movement.height(stance) {
            self.apply_stance_shape(stance);
        }
        if stance == Stance::Sliding && previous_stance != Stance::Sliding {
            self.play_body("res://assets/audio/player/slide", 0.0);
            self.kick_fov(self.slide_fov_kick);
        }

        let mut body = self.base_mut();
        body.set_velocity(to_godot(velocity));
        body.move_and_slide();
        let resolved = body.get_velocity();
        let position = body.get_global_position();
        drop(body);

        // Walls and floors clip the velocity; keep the clipped value as next tick's state.
        self.state.movement.velocity = to_sim(resolved);
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

        if !self.state.alive() {
            self.respawn_in -= dt;
            if self.respawn_in <= 0.0 {
                self.respawn();
            }
        }
    }

    fn process(&mut self, delta: f64) {
        let dt = delta as f32;
        let fraction = Engine::singleton().get_physics_interpolation_fraction() as f32;
        let feet = self.previous_position.lerp(self.current_position, fraction);
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
        let target_eye = self.params.movement.eye_height(movement.stance);
        self.eye_height += (target_eye - self.eye_height) * smoothing(14.0, dt);

        let fast = movement.sprinting || movement.stance == Stance::Sliding;
        let sprint_target = if fast { self.sprint_fov_scale } else { 1.0 };
        self.fov_sprint += (sprint_target - self.fov_sprint) * smoothing(8.0, dt);
        let ads_zoom = 1.0 + (combat.def().ads_fov_scale - 1.0) * ads;
        self.fov_kick.step(FOV_KICK_STIFFNESS, FOV_KICK_DAMPING, dt);
        let fov = self.base_fov * self.fov_sprint * ads_zoom * (1.0 + self.fov_kick.value);

        self.punch.step(PUNCH_STIFFNESS, PUNCH_DAMPING, dt);
        self.land_dip.step(90.0, 13.0, dt);
        self.flinch.step(160.0, 16.0, dt);

        if alive && self.input_enabled {
            let eye = feet + Vector3::UP * self.eye_height;
            self.apply_stick_look(eye, ads, dt);
        }

        let (bob, bob_roll, tilt) = self.camera_motion_offsets(dt, ads);
        let death_roll = self.death_blend * 0.45;
        let eye = self.eye_height * (1.0 - self.death_blend * 0.75);

        // Aim (look plus simulated recoil and sway) and the visual punch stay
        // separate: the punch moves the camera for feel, but bullets follow the aim.
        let punch = self.punch.value;
        let view = Vector3::new(
            self.pitch + recoil.x + sway.x + self.flinch.value + punch.x,
            self.yaw - recoil.y - sway.y + punch.y,
            punch.z + bob_roll + tilt + death_roll,
        );
        let basis = Basis::from_euler(EulerOrder::YXZ, view);
        let origin =
            feet + Vector3::UP * (eye + bob.y + self.land_dip.value) + basis.col_a() * bob.x;
        self.camera
            .set_global_transform(Transform3D::new(basis, origin));
        self.camera.set_fov(fov);

        let frame = self.view_frame(ads);
        self.viewmodel.bind_mut().update(&frame, dt);
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

    /// Horizontal speed in metres per second, for HUD and debugging.
    #[func]
    fn horizontal_speed(&self) -> f32 {
        Vector2::new(
            self.state.movement.velocity.x,
            self.state.movement.velocity.z,
        )
        .length()
    }

    /// Human-readable movement state, for HUD and debugging.
    #[func]
    fn movement_state(&self) -> GString {
        let movement = &self.state.movement;
        let label = match movement.stance {
            _ if !self.state.alive() => "Dead",
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

    /// Prompt for the pickup in front of the player, or empty.
    #[func]
    fn interact_prompt(&self) -> GString {
        let Some(id) = self.look_target else {
            return GString::new();
        };
        let name = &id.def().name;
        if self.state.combat.slots.iter().any(|s| s.id == id) {
            GString::from(&format!("[F] Refill {name} ammo"))
        } else {
            GString::from(&format!("[F] Take {name}"))
        }
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
        // A key still held when the menu closes is not a fresh press.
        self.interact_held = enabled && Input::singleton().is_action_pressed("interact");
    }

    /// Points the view: yaw and pitch in radians (spawns, tests, spectating).
    #[func]
    fn set_look(&mut self, yaw: f32, pitch: f32) {
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

    /// True while the last gameplay input came from a gamepad.
    #[func]
    fn using_gamepad(&self) -> bool {
        self.gamepad
    }

    /// True while controller aim assist is on a target.
    #[func]
    fn aim_assist_engaged(&self) -> bool {
        self.assist_target.is_some()
    }
}

impl FpsPlayer {
    fn take_latched(&mut self, action: &str) -> bool {
        let index = LATCHED_ACTIONS
            .iter()
            .position(|a| *a == action)
            .expect("action is latched");
        std::mem::take(&mut self.latched[index])
    }

    /// T in the demo arena refills every carried weapon. This is a demo
    /// convenience applied outside `player::step`; when netcode lands it should
    /// become a server-side debug command, not client input.
    fn refill_pressed(&mut self) -> bool {
        let pressed = Input::singleton().is_action_just_pressed("refill_ammo")
            | self.take_latched("refill_ammo");
        pressed && self.input_enabled && self.state.alive()
    }

    fn read_input(&mut self) -> PlayerInput {
        if !self.input_enabled {
            self.wish = Vector2::ZERO;
            return PlayerInput {
                yaw: self.yaw,
                pitch: self.pitch,
                ..Default::default()
            };
        }
        let input = Input::singleton();
        let wish = input.get_vector("move_left", "move_right", "move_back", "move_forward");
        self.wish = wish;
        let mut held = |action: &str| input.is_action_pressed(action) | self.take_latched(action);

        let jump = held("jump");
        let sprint = held("sprint");
        let crouch = held("crouch");
        let fire = held("fire");
        let reload = held("reload");
        let next = held("weapon_next");
        let slot_1 = held("weapon_1");
        let slot_2 = held("weapon_2");
        let interact_down = held("interact");
        let interact = interact_down && !self.interact_held;
        self.interact_held = interact_down;

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

        PlayerInput {
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
        }
    }

    fn eye_position(&self) -> Vector3 {
        self.current_position
            + Vector3::UP * self.params.movement.eye_height(self.state.movement.stance)
    }

    fn ray(&self, from: Vector3, to: Vector3, mask: u32) -> Option<RayHit> {
        let mut space = self.base().get_world_3d()?.get_direct_space_state()?;
        let exclude: Array<Rid> = array![self.base().get_rid()];
        let query = PhysicsRayQueryParameters3D::create_ex(from, to)
            .collision_mask(mask)
            .exclude(&exclude)
            .done()?;
        let hit = space.intersect_ray(&query);
        if hit.is_empty() {
            return None;
        }
        Some(RayHit {
            position: hit.get_or_nil("position").try_to::<Vector3>().unwrap_or(to),
            normal: hit
                .get_or_nil("normal")
                .try_to::<Vector3>()
                .unwrap_or(Vector3::UP),
            collider: hit.get_or_nil("collider").try_to::<Gd<Node>>().ok(),
        })
    }

    /// The weapon pickup under the crosshair, within reach.
    fn find_pickup(&self) -> Option<WeaponId> {
        if !self.state.alive() {
            return None;
        }
        let from = self.eye_position();
        let direction = to_godot(player::aim_direction(self.yaw, self.pitch, Vec2::ZERO));
        let hit = self.ray(
            from,
            from + direction * self.interact_distance,
            WORLD_LAYER | INTERACT_LAYER,
        )?;
        let pickup = hit.collider?.try_cast::<WeaponPickup>().ok()?;
        pickup.bind().weapon_id()
    }

    /// Switches between gamepad and mouse-and-keyboard on deliberate input,
    /// ignoring stick drift and tiny mouse bumps.
    fn track_device(&mut self, event: &Gd<InputEvent>) {
        let gamepad = if let Ok(motion) = event.clone().try_cast::<InputEventJoypadMotion>() {
            if motion.get_axis_value().abs() < 0.3 {
                return;
            }
            self.gamepad_device = motion.get_device();
            true
        } else if let Ok(button) = event.clone().try_cast::<InputEventJoypadButton>() {
            if !button.is_pressed() {
                return;
            }
            self.gamepad_device = button.get_device();
            true
        } else if let Ok(motion) = event.clone().try_cast::<InputEventMouseMotion>() {
            if motion.get_screen_relative().length() < 4.0 {
                return;
            }
            false
        } else if event.clone().try_cast::<InputEventKey>().is_ok()
            || event.clone().try_cast::<InputEventMouseButton>().is_ok()
        {
            if !event.is_pressed() {
                return;
            }
            false
        } else {
            return;
        };
        if gamepad != self.gamepad {
            self.gamepad = gamepad;
            self.assist.reset();
            self.assist_target = None;
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

    fn assist_params(&self) -> AimAssistParams {
        let k = self.aim_assist_strength.max(0.0);
        let d = AimAssistParams::default();
        let slow = |full: f32| (1.0 - (1.0 - full) * k).clamp(0.0, 1.0);
        AimAssistParams {
            slowdown_hip: slow(d.slowdown_hip),
            slowdown_ads: slow(d.slowdown_ads),
            rotation_hip: (d.rotation_hip * k).min(1.0),
            rotation_ads: (d.rotation_ads * k).min(1.0),
            ..d
        }
    }

    /// Right-stick look for this rendered frame, with aim assist. Scaled by
    /// zoom like the mouse, so aiming down sights turns slower.
    fn apply_stick_look(&mut self, eye: Vector3, ads: f32, delta: f32) {
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
        if stick != Vec2::ZERO {
            self.gamepad = true;
        }
        let zoom = self.camera.get_fov() / self.base_fov;
        let mut rates = self.stick_look.rates(stick, &params, delta) * zoom;

        let assist_on = self.gamepad
            && self.aim_assist
            && self.aim_assist_strength > 0.0
            && !self.state.movement.sprinting;
        if assist_on {
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
            let targets = self.assist_targets(eye);
            let out = self.assist.update(&view, &targets, &self.assist_params());
            self.assist_target = out.target;
            rates *= out.slowdown;
            self.turn(out.rotation.x, out.rotation.y);
        } else if self.assist_target.is_some() {
            self.assist.reset();
            self.assist_target = None;
        }
        self.turn(-rates.x * delta, rates.y * delta);
    }

    /// Turns the view (yaw left and pitch up, in radians).
    fn turn(&mut self, yaw: f32, pitch: f32) {
        self.yaw += yaw;
        self.pitch = (self.pitch + pitch).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    /// Enabled aim assist targets in range with a clear line of sight.
    fn assist_targets(&self, eye: Vector3) -> Vec<AssistTarget> {
        let Some(mut space) = self
            .base()
            .get_world_3d()
            .and_then(|w| w.get_direct_space_state())
        else {
            return Vec::new();
        };
        let range = AimAssistParams::default().range;
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
        if !self.rumble || !self.gamepad {
            return;
        }
        let def = weapon.def();
        let (weak, strong, seconds) = match def.fire_mode {
            FireMode::Auto => (0.35, 0.15, 0.06),
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
                self.land_dip.velocity -= (speed * 0.03).min(0.45);
                if speed > 3.0 {
                    let volume = linear_db(((speed - 3.0) / 9.0).clamp(0.25, 1.0));
                    self.play_body("res://assets/audio/player/land", volume);
                    self.play_footstep(volume - 4.0);
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
                self.flinch.velocity += if fall { -0.6 } else { 0.4 } * (amount / 30.0).min(1.5);
                self.play_body("res://assets/audio/player/hurt", -2.0);
                self.signals().damaged().emit(amount, from, fall);
            }
            PlayerEvent::Died => {
                self.respawn_in = self.respawn_delay;
                self.signals().died().emit();
            }
            _ => {}
        }
    }

    fn handle_combat(&mut self, event: CombatEvent) {
        match event {
            CombatEvent::Fired {
                weapon,
                aim_offset,
                shot,
            } => {
                let def = weapon.def();
                self.fire_shot(weapon, aim_offset, shot);
                self.rumble_on_fire(weapon);
                let ads = self.state.combat.ads;
                let low = self.state.combat.weapon().magazine <= def.low_ammo();
                let mut rig = self.viewmodel.bind_mut();
                let punch = rig.on_fired(weapon, ads, def.cycle_time <= 0.0);
                if low {
                    rig.on_low_ammo(weapon);
                }
                drop(rig);
                self.kick_camera(punch, ads, shot);
            }
            CombatEvent::DryFire { weapon } => self.viewmodel.bind_mut().on_dry_fire(weapon),
            CombatEvent::ReloadStarted { weapon, .. } => {
                self.viewmodel.bind_mut().on_reload(weapon)
            }
            CombatEvent::RoundLoaded { weapon } => {
                self.viewmodel.bind_mut().on_round_loaded(weapon)
            }
            CombatEvent::Cycled { weapon } => self.viewmodel.bind_mut().on_cycled(weapon),
            CombatEvent::PickedUp { weapon } => self.viewmodel.bind_mut().on_picked_up(weapon),
            CombatEvent::ReloadFinished { .. } | CombatEvent::Switched { .. } => {}
        }
    }

    /// Pulses the FOV: positive widens, negative zooms in. `peak` is a fraction of the FOV.
    fn kick_fov(&mut self, peak: f32) {
        self.fov_kick.velocity += impulse_for_peak(peak, FOV_KICK_STIFFNESS, FOV_KICK_DAMPING);
    }

    /// Adds one shot's camera punch. `degrees` is the peak pitch, yaw jitter, and roll.
    fn kick_camera(&mut self, degrees: Vector3, ads: f32, shot: u32) {
        let scale = 1.0 + (self.ads_punch_scale - 1.0) * ads;
        let roll_sign = if shot.is_multiple_of(2) { 1.0 } else { -1.0 };
        let peak = Vector3::new(
            degrees.x,
            degrees.y * rng::signed(shot, 101),
            degrees.z * roll_sign,
        ) * scale;
        let impulse = |deg: f32| impulse_for_peak(deg.to_radians(), PUNCH_STIFFNESS, PUNCH_DAMPING);
        self.punch.velocity += Vector3::new(impulse(peak.x), impulse(peak.y), impulse(peak.z));
    }

    /// Hitscan from the eye along the view direction plus recoil, spread, and
    /// sway, once per pellet. Damage is summed per target so a shotgun blast
    /// confirms one hit, not eight.
    fn fire_shot(&mut self, weapon: WeaponId, aim_offset: Vec2, shot: u32) {
        let def = weapon.def();
        let origin = self.eye_position();
        // Per target: damage, headshot, and the direction of the first pellet to hit.
        let mut targets: Vec<(Gd<Node>, f32, bool, Vector3)> = Vec::new();
        let mut first_end = None;

        for pellet in 0..def.pellets {
            let offset = aim_offset + weapon::pellet_offset(weapon, shot, pellet);
            let direction = to_godot(player::aim_direction(self.yaw, self.pitch, offset));
            let to = origin + direction * def.range;
            let hit = self.ray(origin, to, WORLD_LAYER | HITBOX_LAYER);
            let end = hit.as_ref().map(|h| h.position).unwrap_or(to);
            first_end.get_or_insert(end);
            let Some(hit) = hit else { continue };

            let target = hit.collider.as_ref().and_then(find_damageable);
            if let (Some(collider), Some(target)) = (&hit.collider, target) {
                let headshot = collider
                    .get_meta_ex("hit_zone")
                    .default(&"body".to_variant())
                    .done()
                    .to_string()
                    == "head";
                let mut damage = def.damage_at(origin.distance_to(hit.position));
                if headshot {
                    damage *= def.damage.headshot;
                }
                match targets
                    .iter_mut()
                    .find(|(t, ..)| t.instance_id() == target.instance_id())
                {
                    Some(entry) => {
                        entry.1 += damage;
                        entry.2 |= headshot;
                    }
                    None => targets.push((target, damage, headshot, direction)),
                }
                let surface = surface_meta(Some(collider)).unwrap_or_else(|| "flesh".to_owned());
                self.spawn_impact(hit.position, hit.normal, false, &surface);
            } else {
                let surface = surface_of(hit.collider.as_ref());
                // Shotguns leave a decal for only some pellets to limit clutter.
                let decal = def.pellets == 1 || pellet % 3 == 0;
                self.spawn_impact(hit.position, hit.normal, decal, &surface);
            }
        }

        let mut confirmed = None;
        for (mut target, damage, headshot, direction) in targets {
            let killed = target
                .call(
                    "apply_damage",
                    &[damage.to_variant(), headshot.to_variant()],
                )
                .try_to::<bool>()
                .unwrap_or(false);
            if target.has_method("react_to_hit") {
                target.call(
                    "react_to_hit",
                    &[direction.to_variant(), killed.to_variant()],
                );
            }
            if killed {
                let headshot_scale = if headshot { 1.6 } else { 1.0 };
                self.kick_fov(-self.kill_fov_kick * headshot_scale);
            }
            let (k, h) = confirmed.unwrap_or((false, false));
            confirmed = Some((k | killed, h | headshot));
        }
        if let Some((killed, headshot)) = confirmed {
            self.signals().hit_confirmed().emit(killed, headshot);
        }

        if def.tracer_every > 0
            && shot.is_multiple_of(u32::from(def.tracer_every))
            && let Some(end) = first_end
        {
            self.spawn_tracer(end);
        }
    }

    /// Tracers start near the muzzle: the viewmodel is drawn with its own FOV,
    /// so this is a camera-relative approximation that reads right on screen.
    fn spawn_tracer(&mut self, end: Vector3) {
        let ads = self.state.combat.ads;
        let hip = Vector3::new(0.09, -0.1, -0.55);
        let aimed = Vector3::new(0.0, -0.05, -0.55);
        let start = self.camera.get_global_transform() * hip.lerp(aimed, ads);
        let Some(mut tracer) = self
            .tracer_scene
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

    fn spawn_impact(&mut self, position: Vector3, normal: Vector3, decal: bool, surface: &str) {
        let Some(mut impact) = self
            .impact_scene
            .instantiate()
            .and_then(|n| n.try_cast::<Node3D>().ok())
        else {
            return;
        };
        let Some(mut parent) = self.base().get_parent() else {
            return;
        };
        parent.add_child(&impact);
        impact.set_global_position(position);
        impact.call(
            "setup",
            &[
                normal.to_variant(),
                decal.to_variant(),
                GString::from(surface).to_variant(),
            ],
        );
    }

    /// Steps once per stride on the ground. Stride length shortens with speed so
    /// cadence rises when sprinting; crouching is quieter.
    fn advance_footsteps(&mut self, moved: f32) {
        let movement = self.state.movement;
        if !self.env.on_floor || movement.stance == Stance::Sliding || !self.state.alive() {
            return;
        }
        let (length, volume) = match movement.stance {
            Stance::Crouching => (1.25, -14.0),
            _ if movement.sprinting => (2.3, 0.0),
            _ => (1.9, -5.0),
        };
        self.stride_length = length;
        self.stride += moved;
        if self.stride >= length {
            self.stride -= length;
            self.play_footstep(volume);
        }
        if moved < 0.001 {
            self.stride = (self.stride - 0.02).max(0.0);
        }
    }

    fn play_footstep(&mut self, volume_db: f32) {
        let feet = self.current_position + Vector3::UP * 0.1;
        let surface = self
            .ray(feet, feet + Vector3::DOWN * 0.5, WORLD_LAYER)
            .map(|hit| surface_of(hit.collider.as_ref()))
            .unwrap_or_else(|| DEFAULT_SURFACE.to_owned());
        let path = format!("res://assets/audio/footsteps/{surface}");
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
        let phase = self.stride / self.stride_length.max(0.1) * PI;
        let amount = self.bob_blend * (1.0 - 0.85 * ads);
        let bob = Vector2::new(
            (phase * 0.5).sin() * 0.012 * amount,
            -phase.sin().abs() * 0.022 * amount,
        );
        let roll = (phase * 0.5).sin() * 0.004 * amount;
        (bob, roll, self.tilt)
    }

    fn view_frame(&self, ads: f32) -> ViewFrame {
        let combat = &self.state.combat;
        let progress = |remaining: f32, total: f32| 1.0 - (remaining / total).clamp(0.0, 1.0);
        let def = combat.def();
        let action = match combat.action {
            Action::Ready => ViewAction::Ready,
            Action::Reloading { remaining, empty } => {
                let total = match def.reload {
                    Reload::Magazine { time, empty_time } => {
                        if empty {
                            empty_time
                        } else {
                            time
                        }
                    }
                    Reload::PerRound { .. } => 1.0,
                };
                ViewAction::Reloading(progress(remaining, total))
            }
            // Round-by-round: tilt in, hold while loading, tilt back out.
            Action::Loading { remaining, stage } => {
                let Reload::PerRound { start, end, .. } = def.reload else {
                    return self.frame_with(ads, ViewAction::Ready);
                };
                ViewAction::Reloading(match stage {
                    LoadStage::Opening => 0.5 * progress(remaining, start),
                    LoadStage::Round => 0.5,
                    LoadStage::Closing => 0.5 + 0.5 * progress(remaining, end),
                })
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
        };
        self.frame_with(ads, action)
    }

    fn frame_with(&self, ads: f32, action: ViewAction) -> ViewFrame {
        let combat = &self.state.combat;
        let velocity = self.state.movement.velocity;
        ViewFrame {
            weapon: combat.weapon().id,
            ads,
            scope: combat.scope_amount(),
            visible: self.state.alive(),
            sprinting: self.state.movement.sprinting,
            speed_ratio: Vec3::new(velocity.x, 0.0, velocity.z).length()
                / self.params.movement.walk_speed,
            on_floor: self.env.on_floor,
            action,
        }
    }

    fn respawn(&mut self) {
        self.state = PlayerState::spawned(&self.params);
        let spawn = self.spawn;
        let mut body = self.base_mut();
        body.set_global_position(spawn.origin);
        body.set_velocity(Vector3::ZERO);
        drop(body);
        self.yaw = spawn.basis.get_euler().y;
        self.pitch = 0.0;
        self.reset_view();
        self.signals().respawned().emit();
    }

    fn reset_view(&mut self) {
        let position = self.base().get_global_position();
        self.previous_position = position;
        self.current_position = position;
        self.previous_recoil = Vec2::ZERO;
        self.previous_sway = Vec2::ZERO;
        self.previous_ads = 0.0;
        self.eye_height = self.params.movement.eye_height(Stance::Standing);
        self.fov_sprint = 1.0;
        self.punch = Spring::default();
        self.fov_kick = Spring::default();
        self.stride = 0.0;
        self.stride_length = 2.0;
        self.assist.reset();
        self.assist_target = None;
        self.apply_stance_shape(Stance::Standing);
    }

    /// Resizes the capsule for a stance, keeping its bottom at the body origin (the feet).
    fn apply_stance_shape(&mut self, stance: Stance) {
        let height = self.params.movement.height(stance);
        if let Some(shape) = self.collision.get_shape()
            && let Ok(mut capsule) = shape.try_cast::<CapsuleShape3D>()
        {
            capsule.set_height(height);
        }
        self.collision
            .set_position(Vector3::new(0.0, height / 2.0, 0.0));
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
fn find_damageable(collider: &Gd<Node>) -> Option<Gd<Node>> {
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
fn surface_of(collider: Option<&Gd<Node>>) -> String {
    surface_meta(collider).unwrap_or_else(|| DEFAULT_SURFACE.to_owned())
}

fn linear_db(linear: f32) -> f32 {
    20.0 * linear.max(1e-4).log10()
}
