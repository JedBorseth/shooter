//! First-person player: input, simulation, hit detection, camera, and viewmodel.
//!
//! Physics runs at the fixed tick (`physics_process`) and calls the shared
//! simulation in `shooter-sim`. The camera is a top-level node updated every
//! rendered frame: position and recoil are interpolated between the last two
//! ticks, while mouse look is applied immediately, so aiming never waits for a
//! physics tick on high-refresh displays.

use std::f32::consts::FRAC_PI_2;

use godot::classes::input::MouseMode;
use godot::classes::{
    Camera3D, CapsuleShape3D, CharacterBody3D, CollisionShape3D, Engine, ICharacterBody3D, Input,
    InputEvent, InputEventMouseButton, InputEventMouseMotion, Node3D, PackedScene,
    PhysicsRayQueryParameters3D, ShapeCast3D,
};
use godot::prelude::*;
use shooter_sim::movement::{MoveEnv, MoveParams, Stance};
use shooter_sim::player::{self, PlayerInput, PlayerState};
use shooter_sim::weapon::{Action, CombatEvent, WeaponId};
use shooter_sim::{Vec2, Vec3};

use crate::convert::{to_godot, to_sim};
use crate::target::{HITBOX_LAYER, WORLD_LAYER};
use crate::viewmodel::{ViewAction, ViewFrame, ViewmodelRig};

const PITCH_LIMIT: f32 = FRAC_PI_2 - 0.01;

/// Actions whose presses are remembered until the next physics tick, so a tap
/// shorter than one tick (or a mouse-wheel step) is never lost.
const LATCHED_ACTIONS: [&str; 8] = [
    "jump",
    "crouch",
    "sprint",
    "fire",
    "reload",
    "weapon_next",
    "weapon_1",
    "weapon_2",
];

#[derive(GodotClass)]
#[class(init, base = CharacterBody3D)]
pub struct FpsPlayer {
    base: Base<CharacterBody3D>,

    /// Radians of view rotation per screen pixel of mouse motion.
    #[export]
    #[init(val = 0.0022)]
    mouse_sensitivity: f32,
    /// Vertical field of view in degrees at the hip.
    #[export]
    #[init(val = 70.0)]
    base_fov: f32,
    /// FOV multiplier while sprinting or sliding.
    #[export]
    #[init(val = 1.08)]
    sprint_fov_scale: f32,
    /// Spawned where bullets hit the world.
    #[export]
    impact_scene: OnEditor<Gd<PackedScene>>,

    #[init(node = "Camera")]
    camera: OnReady<Gd<Camera3D>>,
    #[init(node = "Camera/Viewmodel")]
    viewmodel: OnReady<Gd<ViewmodelRig>>,
    #[init(node = "Collision")]
    collision: OnReady<Gd<CollisionShape3D>>,
    #[init(node = "Headroom")]
    headroom: OnReady<Gd<ShapeCast3D>>,

    params: MoveParams,
    state: PlayerState,
    events: Vec<CombatEvent>,
    latched: [bool; LATCHED_ACTIONS.len()],
    env: MoveEnv,
    yaw: f32,
    pitch: f32,
    look_delta: Vector2,
    previous_position: Vector3,
    current_position: Vector3,
    previous_recoil: Vec2,
    previous_ads: f32,
    eye_height: f32,
    fov_sprint: f32,
    fall_speed: f32,
    shake: f32,
    shake_velocity: f32,
}

#[godot_api]
impl ICharacterBody3D for FpsPlayer {
    fn ready(&mut self) {
        self.camera.set_as_top_level(true);
        let rid = self.base().get_rid();
        self.headroom.add_exception_rid(rid);

        self.yaw = self.base().get_rotation().y;
        self.base_mut().set_rotation(Vector3::ZERO);
        let position = self.base().get_global_position();
        self.previous_position = position;
        self.current_position = position;
        self.eye_height = self.params.eye_height(Stance::Standing);
        self.fov_sprint = 1.0;
        self.apply_stance_shape(Stance::Standing);

        if !Engine::singleton().is_editor_hint() {
            Input::singleton().set_mouse_mode(MouseMode::CAPTURED);
        }
    }

    fn input(&mut self, event: Gd<InputEvent>) {
        let mut input = Input::singleton();
        let captured = input.get_mouse_mode() == MouseMode::CAPTURED;

        if event.is_action_pressed("ui_cancel") {
            input.set_mouse_mode(MouseMode::VISIBLE);
            return;
        }
        for (latched, action) in self.latched.iter_mut().zip(LATCHED_ACTIONS) {
            if event.is_action_pressed(action) {
                *latched = true;
            }
        }
        let event = match event.try_cast::<InputEventMouseMotion>() {
            Ok(motion) => {
                if captured {
                    // Screen-relative motion ignores window stretch; scaling by the
                    // FOV keeps aim speed consistent when zoomed in (ADS).
                    let zoom = self.camera.get_fov() / self.base_fov;
                    let delta = motion.get_screen_relative() * self.mouse_sensitivity * zoom;
                    self.yaw -= delta.x;
                    let pitch = (self.pitch - delta.y).clamp(-PITCH_LIMIT, PITCH_LIMIT);
                    self.look_delta += Vector2::new(-delta.x, pitch - self.pitch);
                    self.pitch = pitch;
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
        let input = self.read_input();
        self.headroom.force_shapecast_update();
        let on_floor = self.base().is_on_floor();
        self.env = MoveEnv {
            on_floor,
            can_stand: !self.headroom.is_colliding(),
        };

        self.previous_recoil = self.state.combat.recoil;
        self.previous_ads = self.state.combat.ads;
        let previous_stance = self.state.movement.stance;
        let mut events = std::mem::take(&mut self.events);
        events.clear();
        let velocity = player::step(
            &mut self.state,
            &input,
            self.env,
            &self.params,
            delta as f32,
            &mut events,
        );
        if self.params.height(previous_stance) != self.params.height(self.state.movement.stance) {
            self.apply_stance_shape(self.state.movement.stance);
        }

        let mut body = self.base_mut();
        body.set_velocity(to_godot(velocity));
        body.move_and_slide();
        let resolved = body.get_velocity();
        let position = body.get_global_position();
        let landed = !on_floor && body.is_on_floor();
        drop(body);

        if landed {
            self.viewmodel.bind_mut().on_landed(self.fall_speed);
        }
        self.fall_speed = (-velocity.y).max(0.0);
        // Walls and floors clip the velocity; keep the clipped value as next tick's state.
        self.state.movement.velocity = to_sim(resolved);
        self.previous_position = self.current_position;
        self.current_position = position;

        for event in &events {
            self.handle_event(event);
        }
        self.events = events;
    }

    fn process(&mut self, delta: f64) {
        let delta = delta as f32;
        let fraction = Engine::singleton().get_physics_interpolation_fraction() as f32;
        let feet = self.previous_position.lerp(self.current_position, fraction);
        let recoil = self
            .previous_recoil
            .lerp(self.state.combat.recoil, fraction);
        let ads = self.previous_ads + (self.state.combat.ads - self.previous_ads) * fraction;

        let target_eye = self.params.eye_height(self.state.movement.stance);
        self.eye_height += (target_eye - self.eye_height) * smoothing(14.0, delta);

        let movement = &self.state.movement;
        let fast = movement.sprinting || movement.stance == Stance::Sliding;
        let sprint_target = if fast { self.sprint_fov_scale } else { 1.0 };
        self.fov_sprint += (sprint_target - self.fov_sprint) * smoothing(8.0, delta);
        let ads_zoom = 1.0 + (self.state.combat.def().ads_fov_scale - 1.0) * ads;
        let fov = self.base_fov * self.fov_sprint * ads_zoom;

        // Small roll shake per shot, on a spring.
        self.shake_velocity += (-self.shake * 300.0 - self.shake_velocity * 24.0) * delta;
        self.shake += self.shake_velocity * delta;

        let view = Vector3::new(self.pitch + recoil.x, self.yaw - recoil.y, self.shake);
        let basis = Basis::from_euler(EulerOrder::YXZ, view);
        let origin = feet + Vector3::UP * self.eye_height;
        self.camera
            .set_global_transform(Transform3D::new(basis, origin));
        self.camera.set_fov(fov);

        let frame = self.view_frame(ads);
        self.look_delta = Vector2::ZERO;
        self.viewmodel.bind_mut().update(&frame, delta);
    }
}

#[godot_api]
impl FpsPlayer {
    /// Emitted when a shot damages a target.
    #[signal]
    fn hit_confirmed(killed: bool, headshot: bool);

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
            Stance::Sliding => "Sliding",
            Stance::Crouching => "Crouching",
            Stance::Standing if movement.sprinting => "Sprinting",
            Stance::Standing => "Standing",
        };
        label.into()
    }

    #[func]
    fn weapon_name(&self) -> GString {
        self.state.combat.def().name.into()
    }

    #[func]
    fn magazine(&self) -> i32 {
        i32::from(self.state.combat.weapon().magazine)
    }

    #[func]
    fn reserve(&self) -> i32 {
        i32::from(self.state.combat.weapon().reserve)
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

    /// Current hip-fire cone half-angle in radians, for the crosshair.
    #[func]
    fn spread_radians(&self) -> f32 {
        self.state
            .combat
            .spread_degrees(&self.state.combat_context(self.env, &self.params))
            .to_radians()
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
}

impl FpsPlayer {
    fn take_latched(&mut self, action: &str) -> bool {
        let index = LATCHED_ACTIONS
            .iter()
            .position(|a| *a == action)
            .expect("action is latched");
        std::mem::take(&mut self.latched[index])
    }

    fn read_input(&mut self) -> PlayerInput {
        let input = Input::singleton();
        let wish = input.get_vector("move_left", "move_right", "move_back", "move_forward");
        let mut held = |action: &str| input.is_action_pressed(action) | self.take_latched(action);

        let jump = held("jump");
        let sprint = held("sprint");
        let crouch = held("crouch");
        let fire = held("fire");
        let reload = held("reload");
        let next = held("weapon_next");
        let slot_1 = held("weapon_1");
        let slot_2 = held("weapon_2");

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
        }
    }

    fn handle_event(&mut self, event: &CombatEvent) {
        match *event {
            CombatEvent::Fired { weapon, aim_offset } => {
                self.fire_ray(weapon, aim_offset);
                let ads = self.state.combat.ads;
                self.viewmodel.bind_mut().on_fired(weapon, ads);
                self.shake_velocity += if self.state.combat.shots_fired.is_multiple_of(2) {
                    0.35
                } else {
                    -0.35
                } * (1.0 - 0.6 * ads);
            }
            CombatEvent::DryFire { weapon } => self.viewmodel.bind_mut().on_dry_fire(weapon),
            CombatEvent::ReloadStarted { weapon, .. } => {
                self.viewmodel.bind_mut().on_reload(weapon)
            }
            CombatEvent::ReloadFinished { .. } | CombatEvent::Switched { .. } => {}
        }
    }

    /// Hitscan from the eye along the view direction plus recoil and spread.
    fn fire_ray(&mut self, weapon: WeaponId, aim_offset: Vec2) {
        let def = weapon.def();
        let origin = self.current_position + Vector3::UP * self.eye_height;
        let direction = to_godot(player::aim_direction(self.yaw, self.pitch, aim_offset));
        let to = origin + direction * def.range;

        let Some(mut space) = self
            .base()
            .get_world_3d()
            .and_then(|w| w.get_direct_space_state())
        else {
            return;
        };
        let exclude: Array<Rid> = array![self.base().get_rid()];
        let Some(query) = PhysicsRayQueryParameters3D::create_ex(origin, to)
            .collision_mask(WORLD_LAYER | HITBOX_LAYER)
            .exclude(&exclude)
            .done()
        else {
            return;
        };
        let hit = space.intersect_ray(&query);
        if hit.is_empty() {
            return;
        }
        let position = hit.get_or_nil("position").try_to::<Vector3>().unwrap_or(to);
        let normal = hit
            .get_or_nil("normal")
            .try_to::<Vector3>()
            .unwrap_or(Vector3::UP);
        let collider = hit.get_or_nil("collider").try_to::<Gd<Node>>().ok();

        let target = collider.as_ref().and_then(find_damageable);
        if let (Some(collider), Some(mut target)) = (collider, target) {
            let headshot = collider
                .get_meta_ex("hit_zone")
                .default(&"body".to_variant())
                .done()
                .to_string()
                == "head";
            let mut damage = def.damage_at(origin.distance_to(position));
            if headshot {
                damage *= def.headshot_multiplier;
            }
            let killed = target
                .call(
                    "apply_damage",
                    &[damage.to_variant(), headshot.to_variant()],
                )
                .try_to::<bool>()
                .unwrap_or(false);
            self.signals().hit_confirmed().emit(killed, headshot);
            self.spawn_impact(position, normal, false);
        } else {
            self.spawn_impact(position, normal, true);
        }
    }

    fn spawn_impact(&mut self, position: Vector3, normal: Vector3, decal: bool) {
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
        impact.call("setup", &[normal.to_variant(), decal.to_variant()]);
    }

    fn view_frame(&self, ads: f32) -> ViewFrame {
        let combat = &self.state.combat;
        let progress = |remaining: f32, total: f32| 1.0 - (remaining / total).clamp(0.0, 1.0);
        let def = combat.def();
        let action = match combat.action {
            Action::Ready => ViewAction::Ready,
            Action::Reloading { remaining, empty } => {
                let total = if empty {
                    def.reload_empty_time
                } else {
                    def.reload_time
                };
                ViewAction::Reloading(progress(remaining, total))
            }
            Action::Lowering { remaining, .. } => {
                ViewAction::Lowering(progress(remaining, def.lower_time))
            }
            Action::Raising { remaining } => {
                ViewAction::Raising(progress(remaining, def.raise_time))
            }
        };
        let velocity = self.state.movement.velocity;
        ViewFrame {
            weapon: combat.weapon().id,
            ads,
            sprinting: self.state.movement.sprinting,
            speed_ratio: Vec3::new(velocity.x, 0.0, velocity.z).length() / self.params.walk_speed,
            on_floor: self.env.on_floor,
            look_delta: self.look_delta,
            action,
        }
    }

    /// Resizes the capsule for a stance, keeping its bottom at the body origin (the feet).
    fn apply_stance_shape(&mut self, stance: Stance) {
        let height = self.params.height(stance);
        if let Some(shape) = self.collision.get_shape()
            && let Ok(mut capsule) = shape.try_cast::<CapsuleShape3D>()
        {
            capsule.set_height(height);
        }
        self.collision
            .set_position(Vector3::new(0.0, height / 2.0, 0.0));
    }
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

/// Frame-rate independent exponential smoothing factor.
fn smoothing(rate: f32, delta: f32) -> f32 {
    1.0 - (-rate * delta).exp()
}
