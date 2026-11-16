//! First-person weapon presentation: procedural pose, bob, kick, reload,
//! bolt/pump, and switch motion, muzzle flash, ejected shells, and weapon sounds.
//!
//! Nothing here affects gameplay; the simulation in `shooter-sim` decides when
//! shots happen. The rig only turns simulation state into motion, so the same
//! numbers can later drive other players' third-person weapons.

use std::f32::consts::PI;

use godot::classes::{
    AudioStreamPlayer, AudioStreamRandomizer, BaseMaterial3D, CylinderMesh, INode3D, Material,
    MeshInstance3D, Node3D, OmniLight3D, Shader, ShaderMaterial,
};
use godot::prelude::*;
use shooter_sim::rng;
use shooter_sim::weapon::WeaponId;

use crate::audio;

/// One rendered frame of state the rig animates from.
#[derive(Clone, Copy, Debug)]
pub struct ViewFrame {
    pub weapon: WeaponId,
    /// 0 = hip, 1 = aimed down sights.
    pub ads: f32,
    /// 0 unless aiming a scoped weapon. The weapon hides behind the scope overlay.
    pub scope: f32,
    /// False while the player is dead.
    pub visible: bool,
    pub sprinting: bool,
    /// Horizontal speed relative to walk speed.
    pub speed_ratio: f32,
    pub on_floor: bool,
    pub action: ViewAction,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ViewAction {
    Ready,
    /// Progress 0..1.
    Reloading(f32),
    Lowering(f32),
    Raising(f32),
    /// Working the bolt or pump.
    Cycling(f32),
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

/// Seconds an ejected shell stays visible.
const SHELL_LIFETIME: f32 = 0.8;
const SHELL_POOL: usize = 8;

/// One first-person weapon: model, muzzle, sounds, and its pose offsets.
///
/// The scene's origin is on the sight line, at the rear sight, with the barrel
/// along -Z, so the aimed-down-sights pose is just a distance in front of the eye.
#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct WeaponView {
    base: Base<Node3D>,

    /// Weapon file key this view presents, such as `m4` for `data/weapons/m4.ron`.
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
    /// Offset (metres) at the middle of a bolt or pump cycle.
    #[export]
    #[init(val = Vector3::new(0.0, -0.01, 0.03))]
    cycle_position: Vector3,
    /// Rotation (degrees: pitch, yaw, roll) at the middle of a bolt or pump cycle.
    #[export]
    #[init(val = Vector3::new(4.0, 0.0, 6.0))]
    cycle_rotation: Vector3,
    /// Shell casing colour and size (radius, length in metres).
    #[export]
    #[init(val = Color::from_rgb(0.85, 0.62, 0.25))]
    shell_color: Color,
    #[export]
    #[init(val = Vector2::new(0.005, 0.022))]
    shell_size: Vector2,

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
    /// Optional: bolt or pump sound, and the sound of one round going in.
    cycle_sound: Option<Gd<AudioStreamPlayer>>,
    round_sound: Option<Gd<AudioStreamPlayer>>,
    /// Optional marker where shells leave the weapon.
    ejection: Option<Gd<Node3D>>,
    /// Resolved from `weapon` once the table is loaded.
    id: Option<WeaponId>,
    shell_mesh: Option<Gd<CylinderMesh>>,

    flash_time: f32,
    flash_count: u32,
}

#[godot_api]
impl INode3D for WeaponView {
    fn ready(&mut self) {
        let shader = load::<Shader>("res://shaders/viewmodel.gdshader");
        convert_to_viewmodel_materials(&self.model.clone().upcast(), &shader, self.albedo_boost);
        self.cycle_sound = self.base().try_get_node_as("CycleSound");
        self.round_sound = self.base().try_get_node_as("RoundSound");
        self.ejection = self.base().try_get_node_as("Ejection");
        self.id = WeaponId::by_key(&self.weapon.to_string());
        if self.id.is_none() {
            godot_error!("WeaponView: unknown weapon `{}`", self.weapon);
        }
        self.shell_mesh = Some(shell_mesh(&shader, self.shell_color, self.shell_size));
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
        self.id
    }

    /// Where shells come out, in the view's parent (rig) space.
    fn ejection_point(&self) -> Vector3 {
        let local = self
            .ejection
            .as_ref()
            .map(|marker| marker.get_position())
            .unwrap_or(Vector3::new(0.02, -0.03, -0.12));
        self.base().get_transform() * local
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

    pub fn on_cycled(&mut self) {
        if let Some(sound) = &mut self.cycle_sound {
            sound.play();
        }
    }

    pub fn on_round_loaded(&mut self) {
        if let Some(sound) = &mut self.round_sound {
            sound.play();
        }
    }
}

#[derive(Clone, Debug)]
struct Shell {
    mesh: Gd<MeshInstance3D>,
    position: Vector3,
    velocity: Vector3,
    spin: Vector3,
    age: f32,
    clinked: bool,
}

/// A casing: a small cylinder drawn with the viewmodel shader.
fn shell_mesh(shader: &Gd<Shader>, color: Color, size: Vector2) -> Gd<CylinderMesh> {
    let mut cylinder = CylinderMesh::new_gd();
    cylinder.set_top_radius(size.x);
    cylinder.set_bottom_radius(size.x);
    cylinder.set_height(size.y);
    cylinder.set_radial_segments(8);
    cylinder.set_rings(1);
    let mut material = ShaderMaterial::new_gd();
    material.set_shader(shader);
    material.set_shader_parameter("albedo", &color.to_variant());
    material.set_shader_parameter("metallic", &0.85.to_variant());
    material.set_shader_parameter("roughness", &0.35.to_variant());
    cylinder.set_material(&material);
    cylinder
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
        if standard.get_feature(godot::classes::base_material_3d::Feature::EMISSION) {
            material.set_shader_parameter("emission", &standard.get_emission().to_variant());
            material.set_shader_parameter(
                "emission_energy",
                &standard.get_emission_energy_multiplier().to_variant(),
            );
        }
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

    views: Vec<Gd<WeaponView>>,
    active: Option<WeaponId>,
    shells: Vec<Shell>,
    next_shell: usize,
    shells_ejected: u32,
    /// Seconds until a bolt or pump ejects its shell.
    pending_shell: Option<(WeaponId, f32)>,
    shell_sound: Option<Gd<AudioStreamPlayer>>,
    low_ammo_sound: Option<Gd<AudioStreamPlayer>>,
    cycle_kick: f32,
    pickup_dip: f32,
    time: f32,
    bob_phase: f32,
    bob_blend: f32,
    sprint_blend: f32,
    reload_blend: f32,
    reload_progress: f32,
    switch_drop: f32,
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
        self.shell_sound = self.make_player("res://assets/audio/player/shell", -16.0);
        self.low_ammo_sound = self.make_player("res://assets/audio/player/low_ammo", -10.0);
    }
}

impl ViewmodelRig {
    fn view(&self, weapon: WeaponId) -> Option<Gd<WeaponView>> {
        self.views
            .iter()
            .find(|v| v.bind().weapon_id() == Some(weapon))
            .cloned()
    }

    /// A shot was fired. Self-loading weapons eject a shell right away; bolt
    /// and pump weapons eject when they cycle.
    pub fn on_fired(&mut self, weapon: WeaponId, ads: f32, eject: bool) {
        let Some(mut view) = self.view(weapon) else {
            return;
        };
        let strength = view.bind().kick_strength * (1.0 - 0.45 * ads);
        view.bind_mut().on_fired();
        self.kick.velocity += Vector3::new(2.2, 0.0, 1.1) * strength;
        if eject {
            self.eject_shell(&view);
        }
    }

    pub fn on_cycled(&mut self, weapon: WeaponId) {
        if let Some(mut view) = self.view(weapon) {
            view.bind_mut().on_cycled();
        }
        // The spent case comes out about a third of the way through the cycle.
        self.pending_shell = Some((weapon, weapon.def().cycle_time * 0.35));
    }

    pub fn on_round_loaded(&mut self, weapon: WeaponId) {
        if let Some(mut view) = self.view(weapon) {
            view.bind_mut().on_round_loaded();
        }
        self.cycle_kick = 1.0;
    }

    pub fn on_low_ammo(&mut self, _weapon: WeaponId) {
        if let Some(sound) = &mut self.low_ammo_sound {
            sound.play();
        }
    }

    pub fn on_picked_up(&mut self, weapon: WeaponId) {
        if let Some(mut view) = self.view(weapon) {
            view.bind_mut().on_reload();
        }
        self.pickup_dip = 1.0;
    }

    fn make_player(&mut self, base: &str, volume_db: f32) -> Option<Gd<AudioStreamPlayer>> {
        let stream: Gd<AudioStreamRandomizer> = audio::variations(base, 1.1)?;
        let mut player = AudioStreamPlayer::new_alloc();
        player.set_stream(&stream);
        player.set_volume_db(volume_db);
        player.set_bus("Effects");
        player.set_max_polyphony(4);
        self.base_mut().add_child(&player);
        Some(player)
    }

    /// Throws a casing out of the ejection port, to the right and up. Shells
    /// live in the rig's (camera) space, so they use the viewmodel shader too.
    fn eject_shell(&mut self, view: &Gd<WeaponView>) {
        let (start, mesh) = {
            let v = view.bind();
            (v.ejection_point(), v.shell_mesh.clone())
        };
        let Some(mesh) = mesh else { return };
        if self.shells.len() < SHELL_POOL {
            let mut mesh = MeshInstance3D::new_alloc();
            mesh.set_cast_shadows_setting(
                godot::classes::geometry_instance_3d::ShadowCastingSetting::OFF,
            );
            self.base_mut().add_child(&mesh);
            self.shells.push(Shell {
                mesh,
                position: Vector3::ZERO,
                velocity: Vector3::ZERO,
                spin: Vector3::ZERO,
                age: SHELL_LIFETIME,
                clinked: true,
            });
        }
        let index = self.next_shell % self.shells.len();
        self.next_shell = self.next_shell.wrapping_add(1);
        self.shells_ejected = self.shells_ejected.wrapping_add(1);
        let seed = self.shells_ejected;

        let shell = &mut self.shells[index];
        shell.mesh.set_mesh(&mesh);
        shell.mesh.set_visible(true);
        shell.position = start;
        shell.velocity = Vector3::new(
            1.3 + rng::unit(seed, 1) * 0.6,
            1.0 + rng::unit(seed, 2) * 0.6,
            0.2 + rng::signed(seed, 3) * 0.2,
        );
        shell.spin = Vector3::new(
            rng::signed(seed, 4) * 25.0,
            rng::signed(seed, 5) * 10.0,
            18.0 + rng::unit(seed, 6) * 12.0,
        );
        shell.age = 0.0;
        shell.clinked = false;
    }

    fn update_shells(&mut self, dt: f32) {
        if let Some((weapon, delay)) = self.pending_shell {
            let delay = delay - dt;
            self.pending_shell = if delay <= 0.0 {
                if let Some(view) = self.view(weapon) {
                    self.eject_shell(&view);
                }
                None
            } else {
                Some((weapon, delay))
            };
        }
        // Gravity is world-down, expressed in the rig's rotating space.
        let gravity = self.base().get_global_basis().inverse() * Vector3::new(0.0, -9.8, 0.0);
        let mut clink = false;
        for shell in &mut self.shells {
            if shell.age >= SHELL_LIFETIME {
                continue;
            }
            shell.age += dt;
            shell.velocity += gravity * dt;
            shell.position += shell.velocity * dt;
            let rotation = shell.spin * shell.age;
            let basis = Basis::from_euler(EulerOrder::YXZ, rotation);
            shell
                .mesh
                .set_transform(Transform3D::new(basis, shell.position));
            if !shell.clinked && shell.age > 0.45 {
                shell.clinked = true;
                clink = true;
            }
            if shell.age >= SHELL_LIFETIME {
                shell.mesh.set_visible(false);
            }
        }
        if clink && let Some(sound) = &mut self.shell_sound {
            sound.play();
        }
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
        self.update_shells(dt);
        // Hidden while dead, and once the scope overlay covers the screen.
        let shown = frame.visible && frame.scope < 0.85;
        for view in &mut self.views {
            let show = shown && view.bind().weapon_id() == Some(frame.weapon);
            if view.is_visible() != show {
                view.set_visible(show);
            }
        }
        self.active = Some(frame.weapon);
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

        // No look sway: the weapon stays locked to the camera when aiming around,
        // so it only moves from walking, firing, landing, and actions.

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

        // Bolt or pump: a quick push and roll, peaking mid-cycle. A loaded shell
        // gives a small bump.
        let cycle = match frame.action {
            ViewAction::Cycling(p) => (PI * p).sin().max(0.0) * (1.0 - 0.5 * frame.scope),
            _ => 0.0,
        };
        self.cycle_kick = (self.cycle_kick - dt * 6.0).max(0.0);
        self.pickup_dip = (self.pickup_dip - dt * 2.5).max(0.0);
        let bump = smoothstep(self.cycle_kick) * 0.4 + smoothstep(self.pickup_dip) * 0.6;

        let (hip, aim, sprint_pos, sprint_rot, pivot, cycle_pos, cycle_rot) = {
            let v = view.bind();
            (
                v.hip_position,
                v.ads_position,
                v.sprint_position,
                v.sprint_rotation,
                v.pivot,
                v.cycle_position,
                v.cycle_rotation,
            )
        };
        let mut position = hip + (aim - hip) * ads;
        position += sprint_pos * sprint;
        position += bob_offset + Vector3::new(0.0, breathe, 0.0);
        position += Vector3::new(0.0, 0.0, self.kick.value.z * 0.4);
        position += Vector3::new(0.0, self.land.value.y * 0.1, 0.0);
        position += cycle_pos * cycle + Vector3::new(0.0, -0.02 * bump, 0.01 * bump);
        // Reload: tilt the magazine well toward the camera and bring the weapon inward.
        position += Vector3::new(
            -0.03 * dip,
            0.015 * dip - 0.28 * self.switch_drop,
            0.02 * dip,
        );

        let deg = |v: f32| v.to_radians();
        let pitch = deg(sprint_rot.x) * sprint + self.kick.value.x * 0.35 + deg(10.0) * dip
            - deg(35.0) * self.switch_drop
            + deg(cycle_rot.x) * cycle
            + deg(4.0) * bump;
        let yaw = deg(sprint_rot.y) * sprint + deg(cycle_rot.y) * cycle;
        let roll =
            deg(sprint_rot.z) * sprint + bob_roll + deg(32.0) * dip + deg(cycle_rot.z) * cycle;

        // Rotate around the pivot rather than the sight origin.
        let basis = Basis::from_euler(EulerOrder::YXZ, Vector3::new(pitch, yaw, roll));
        let origin = position + pivot - basis * pivot;
        view.set_transform(Transform3D::new(basis, origin));
    }
}
