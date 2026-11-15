//! First-person weapon presentation: procedural pose, sway, bob, kick, reload
//! and switch motion, muzzle flash, and weapon sounds.
//!
//! Nothing here affects gameplay; the simulation in `shooter-sim` decides when
//! shots happen. The rig only turns simulation state into motion, so the same
//! numbers can later drive other players' third-person weapons.

use std::f32::consts::PI;

use godot::classes::{
    AudioStreamPlayer, BaseMaterial3D, INode3D, Material, MeshInstance3D, Node3D, OmniLight3D,
    Shader, ShaderMaterial,
};
use godot::prelude::*;
use shooter_sim::weapon::WeaponId;

/// One rendered frame of state the rig animates from.
#[derive(Clone, Copy, Debug)]
pub struct ViewFrame {
    pub weapon: WeaponId,
    /// 0 = hip, 1 = aimed down sights.
    pub ads: f32,
    pub sprinting: bool,
    /// Horizontal speed relative to walk speed.
    pub speed_ratio: f32,
    pub on_floor: bool,
    /// Mouse look applied this frame, in radians (`x` yaw left, `y` pitch down).
    pub look_delta: Vector2,
    pub action: ViewAction,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ViewAction {
    Ready,
    /// Progress 0..1.
    Reloading(f32),
    Lowering(f32),
    Raising(f32),
}

/// A damped spring, integrated semi-implicitly for stability at any frame rate.
#[derive(Clone, Copy, Debug, Default)]
struct Spring {
    value: Vector3,
    velocity: Vector3,
}

impl Spring {
    fn step(&mut self, stiffness: f32, damping: f32, dt: f32) {
        // Sub-step so long frames don't explode.
        let steps = (dt / (1.0 / 240.0)).ceil().max(1.0);
        let h = dt / steps;
        for _ in 0..steps as u32 {
            self.velocity += (-self.value * stiffness - self.velocity * damping) * h;
            self.value += self.velocity * h;
        }
    }
}

fn smoothing(rate: f32, dt: f32) -> f32 {
    1.0 - (-rate * dt).exp()
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn weapon_from_name(name: &str) -> Option<WeaponId> {
    WeaponId::ALL
        .into_iter()
        .find(|id| id.def().name == name || format!("{id:?}") == name)
}

/// One first-person weapon: model, muzzle, sounds, and its pose offsets.
///
/// The scene's origin is on the sight line, at the rear sight, with the barrel
/// along -Z, so the aimed-down-sights pose is just a distance in front of the eye.
#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct WeaponView {
    base: Base<Node3D>,

    /// Simulation weapon this view presents (`M4` or `Pistol`).
    #[export]
    weapon: GString,
    #[export]
    hip_position: Vector3,
    #[export]
    #[init(val = Vector3::new(0.0, 0.0, -0.12))]
    ads_position: Vector3,
    #[export]
    #[init(val = Vector3::new(-0.04, -0.05, 0.02))]
    sprint_position: Vector3,
    /// Degrees: pitch, yaw, roll.
    #[export]
    #[init(val = Vector3::new(-12.0, 35.0, 10.0))]
    sprint_rotation: Vector3,
    /// Kick impulse scale for this weapon.
    #[export]
    #[init(val = 1.0)]
    kick_strength: f32,
    /// Point the procedural rotations turn around (roughly the firing hand),
    /// relative to the sight origin.
    #[export]
    #[init(val = Vector3::new(0.0, -0.07, 0.06))]
    pivot: Vector3,
    /// Brightens the imported colours; low-poly packs are often very dark.
    #[export]
    #[init(val = 1.0)]
    albedo_boost: f32,

    #[init(node = "Model")]
    model: OnReady<Gd<Node3D>>,
    #[init(node = "Muzzle/Flash")]
    flash: OnReady<Gd<MeshInstance3D>>,
    #[init(node = "Muzzle/FlashLight")]
    flash_light: OnReady<Gd<OmniLight3D>>,
    #[init(node = "FireSound")]
    fire_sound: OnReady<Gd<AudioStreamPlayer>>,
    #[init(node = "ReloadSound")]
    reload_sound: OnReady<Gd<AudioStreamPlayer>>,
    #[init(node = "DryFireSound")]
    dry_fire_sound: OnReady<Gd<AudioStreamPlayer>>,

    flash_time: f32,
    flash_count: u32,
}

#[godot_api]
impl INode3D for WeaponView {
    fn ready(&mut self) {
        let shader = load::<Shader>("res://shaders/viewmodel.gdshader");
        convert_to_viewmodel_materials(&self.model.clone().upcast(), &shader, self.albedo_boost);
        self.flash.set_visible(false);
        self.flash_light.set_visible(false);
    }

    fn process(&mut self, delta: f64) {
        if self.flash_time > 0.0 {
            self.flash_time -= delta as f32;
            if self.flash_time <= 0.0 {
                self.flash.set_visible(false);
                self.flash_light.set_visible(false);
            }
        }
    }
}

impl WeaponView {
    pub fn weapon_id(&self) -> Option<WeaponId> {
        weapon_from_name(&self.weapon.to_string())
    }

    pub fn on_fired(&mut self) {
        self.flash_count = self.flash_count.wrapping_add(1);
        // Cheap variation without a random generator: rotate by a golden-angle step.
        let roll = (self.flash_count as f32 * 2.399_963) % (2.0 * PI);
        let scale = 0.85 + 0.3 * ((self.flash_count * 7 % 10) as f32 / 10.0);
        self.flash.set_rotation(Vector3::new(0.0, 0.0, roll));
        self.flash.set_scale(Vector3::splat(scale));
        self.flash.set_visible(true);
        self.flash_light.set_visible(true);
        self.flash_time = 0.035;

        let pitch = 0.97 + 0.06 * ((self.flash_count * 13 % 10) as f32 / 10.0);
        self.fire_sound.set_pitch_scale(pitch);
        self.fire_sound.play();
    }

    pub fn on_reload(&mut self) {
        self.reload_sound.play();
    }

    pub fn on_dry_fire(&mut self) {
        self.dry_fire_sound.play();
    }
}

/// Replaces each surface's standard material with the viewmodel shader, copying
/// its colour, texture, and surface values, so the weapon keeps its look but uses
/// its own FOV and never clips into walls.
fn convert_to_viewmodel_materials(root: &Gd<Node>, shader: &Gd<Shader>, boost: f32) {
    for child in root.get_children().iter_shared() {
        if let Ok(mut mesh) = child.clone().try_cast::<MeshInstance3D>() {
            mesh.set_cast_shadows_setting(
                godot::classes::geometry_instance_3d::ShadowCastingSetting::OFF,
            );
            for surface in 0..mesh.get_surface_override_material_count() {
                let Some(source) = mesh.get_active_material(surface) else {
                    continue;
                };
                let material = viewmodel_material(&source, shader, boost);
                mesh.set_surface_override_material(surface, &material);
            }
        }
        convert_to_viewmodel_materials(&child, shader, boost);
    }
}

fn viewmodel_material(
    source: &Gd<Material>,
    shader: &Gd<Shader>,
    boost: f32,
) -> Gd<ShaderMaterial> {
    let mut material = ShaderMaterial::new_gd();
    material.set_shader(shader);
    if let Ok(standard) = source.clone().try_cast::<BaseMaterial3D>() {
        let albedo = standard.get_albedo();
        let boosted = Color::from_rgba(
            albedo.r * boost,
            albedo.g * boost,
            albedo.b * boost,
            albedo.a,
        );
        material.set_shader_parameter("albedo", &boosted.to_variant());
        material.set_shader_parameter("roughness", &standard.get_roughness().to_variant());
        material.set_shader_parameter("metallic", &standard.get_metallic().to_variant());
        if let Some(texture) =
            standard.get_texture(godot::classes::base_material_3d::TextureParam::ALBEDO)
        {
            material.set_shader_parameter("albedo_texture", &texture.to_variant());
        }
    }
    material
}

/// Moves the active [`WeaponView`] procedurally. Child of the camera.
#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct ViewmodelRig {
    base: Base<Node3D>,

    /// Metres of sideways bob at walk speed.
    #[export]
    #[init(val = 0.011)]
    bob_amount: f32,
    /// Bob cycles per second at walk speed.
    #[export]
    #[init(val = 1.7)]
    bob_frequency: f32,
    /// How much mouse look drags the weapon.
    #[export]
    #[init(val = 3.5)]
    sway_amount: f32,
    #[export]
    #[init(val = 0.09)]
    sway_limit: f32,

    views: Vec<Gd<WeaponView>>,
    active: Option<WeaponId>,
    time: f32,
    bob_phase: f32,
    bob_blend: f32,
    sprint_blend: f32,
    reload_blend: f32,
    reload_progress: f32,
    switch_drop: f32,
    /// x: yaw, y: pitch (radians), z: unused.
    sway: Spring,
    /// x: pitch up (radians), y: unused, z: push back (metres).
    kick: Spring,
    /// y: vertical dip (metres).
    land: Spring,
}

#[godot_api]
impl INode3D for ViewmodelRig {
    fn ready(&mut self) {
        self.views = self
            .base()
            .get_children()
            .iter_shared()
            .filter_map(|child| child.try_cast::<WeaponView>().ok())
            .collect();
        for view in &mut self.views {
            view.set_visible(false);
        }
    }
}

impl ViewmodelRig {
    fn view(&self, weapon: WeaponId) -> Option<Gd<WeaponView>> {
        self.views
            .iter()
            .find(|v| v.bind().weapon_id() == Some(weapon))
            .cloned()
    }

    pub fn on_fired(&mut self, weapon: WeaponId, ads: f32) {
        let Some(mut view) = self.view(weapon) else {
            return;
        };
        let strength = view.bind().kick_strength * (1.0 - 0.45 * ads);
        view.bind_mut().on_fired();
        self.kick.velocity += Vector3::new(2.2, 0.0, 1.1) * strength;
    }

    pub fn on_reload(&mut self, weapon: WeaponId) {
        if let Some(mut view) = self.view(weapon) {
            view.bind_mut().on_reload();
        }
    }

    pub fn on_dry_fire(&mut self, weapon: WeaponId) {
        if let Some(mut view) = self.view(weapon) {
            view.bind_mut().on_dry_fire();
        }
    }

    /// Called on landing; `impact` is the downward speed in m/s.
    pub fn on_landed(&mut self, impact: f32) {
        self.land.velocity.y -= (impact * 0.08).min(0.9);
    }

    pub fn update(&mut self, frame: &ViewFrame, dt: f32) {
        if self.active != Some(frame.weapon) {
            for view in &mut self.views {
                let show = view.bind().weapon_id() == Some(frame.weapon);
                view.set_visible(show);
            }
            self.active = Some(frame.weapon);
        }
        let Some(mut view) = self.view(frame.weapon) else {
            return;
        };
        self.time += dt;

        let ads = smoothstep(frame.ads);
        let hip_amount = 1.0 - ads;

        // Sprint pose.
        let sprint_target = if frame.sprinting { 1.0 } else { 0.0 };
        self.sprint_blend += (sprint_target - self.sprint_blend) * smoothing(10.0, dt);
        let sprint = smoothstep(self.sprint_blend) * hip_amount;

        // Walk bob: faster and bigger while sprinting, nearly gone when aiming.
        let moving = if frame.on_floor {
            frame.speed_ratio.min(1.6)
        } else {
            0.0
        };
        self.bob_blend += (moving - self.bob_blend) * smoothing(8.0, dt);
        let sprint_bob = 1.0 + 0.8 * sprint;
        self.bob_phase += dt * self.bob_frequency * 2.0 * PI * self.bob_blend.max(0.25);
        let bob = self.bob_amount * self.bob_blend * sprint_bob * (1.0 - 0.85 * ads);
        let bob_offset = Vector3::new(
            self.bob_phase.sin() * bob,
            -(self.bob_phase.cos().abs()) * bob * 0.8,
            0.0,
        );
        let bob_roll = self.bob_phase.sin() * bob * 1.4;
        let breathe = (self.time * 1.3).sin() * 0.0015 * hip_amount;

        // Sway: the weapon lags behind mouse movement, then springs back.
        let sway_scale = self.sway_amount * (1.0 - 0.7 * ads);
        self.sway.velocity +=
            Vector3::new(frame.look_delta.x, frame.look_delta.y, 0.0) * sway_scale * 60.0;
        self.sway.step(140.0, 18.0, dt);
        let limit = self.sway_limit;
        self.sway.value = Vector3::new(
            self.sway.value.x.clamp(-limit, limit),
            self.sway.value.y.clamp(-limit, limit),
            0.0,
        );

        self.kick.step(260.0, 22.0, dt);
        self.land.step(120.0, 14.0, dt);

        // Reload: dip and roll the weapon over the reload's duration.
        let (reload_target, progress) = match frame.action {
            ViewAction::Reloading(p) => (1.0, p),
            _ => (0.0, self.reload_progress),
        };
        self.reload_progress = progress;
        self.reload_blend += (reload_target - self.reload_blend) * smoothing(12.0, dt);
        let dip = (PI * self.reload_progress).sin().max(0.0) * self.reload_blend;

        // Switching: drop the old weapon out of view, raise the new one.
        let drop_target = match frame.action {
            ViewAction::Lowering(p) => smoothstep(p),
            ViewAction::Raising(p) => 1.0 - smoothstep(p),
            _ => 0.0,
        };
        self.switch_drop = drop_target;

        let (hip, aim, sprint_pos, sprint_rot, pivot) = {
            let v = view.bind();
            (
                v.hip_position,
                v.ads_position,
                v.sprint_position,
                v.sprint_rotation,
                v.pivot,
            )
        };
        let mut position = hip + (aim - hip) * ads;
        position += sprint_pos * sprint;
        position += bob_offset + Vector3::new(0.0, breathe, 0.0);
        position += Vector3::new(-self.sway.value.x * 0.25, self.sway.value.y * 0.25, 0.0);
        position += Vector3::new(0.0, 0.0, self.kick.value.z * 0.4);
        position += Vector3::new(0.0, self.land.value.y * 0.1, 0.0);
        // Reload: tilt the magazine well toward the camera and bring the weapon inward.
        position += Vector3::new(
            -0.03 * dip,
            0.015 * dip - 0.28 * self.switch_drop,
            0.02 * dip,
        );

        let deg = |v: f32| v.to_radians();
        let pitch = deg(sprint_rot.x) * sprint
            + self.kick.value.x * 0.35
            + self.sway.value.y
            + deg(10.0) * dip
            - deg(35.0) * self.switch_drop;
        let yaw = deg(sprint_rot.y) * sprint + self.sway.value.x;
        let roll =
            deg(sprint_rot.z) * sprint + bob_roll + deg(32.0) * dip + self.sway.value.x * 0.6;

        // Rotate around the pivot rather than the sight origin.
        let basis = Basis::from_euler(EulerOrder::YXZ, Vector3::new(pitch, yaw, roll));
        let origin = position + pivot - basis * pivot;
        view.set_transform(Transform3D::new(basis, origin));
    }
}
