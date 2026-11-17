//! First-person weapon presentation: procedural pose, bob, kick, reload,
//! bolt/pump, and switch motion, muzzle flash, ejected shells, and weapon sounds.
//!
//! Nothing here affects gameplay; the simulation in `shooter-sim` decides when
//! shots happen. The rig only turns simulation state into motion, so the same
//! numbers can later drive other players' third-person weapons.

use std::f32::consts::{PI, TAU};

use godot::classes::{
    AudioStreamPlayer, AudioStreamRandomizer, BaseMaterial3D, Curve, CurveTexture, CylinderMesh,
    GpuParticles3D, Gradient, GradientTexture1D, INode3D, Material, MeshInstance3D, Node3D,
    OmniLight3D, ParticleProcessMaterial, QuadMesh, Shader, ShaderMaterial,
};
use godot::global::randf_range;
use godot::prelude::*;
use shooter_sim::rng;
use shooter_sim::weapon::{Attachment, LoadStage, Slot, WeaponId};

use godot::classes::light_3d::Param;

use crate::acoustics::{self, AudioEnvironment};
use crate::audio;
use crate::character::arms::FpArms;
use crate::character::poser::Grips;
use crate::sights::IronSights;
use crate::spring::{Spring, damping_for, impulse_for_peak, smoothing};
use crate::weapon_clips::{
    self, Anchors, BOLT_BACK, BOLT_LIFT, ClipKind, ClipPose, MagazineReload, PUMP_BACK, Pose,
    keyed, smoothstep,
};
use crate::weapon_foley::{self, WeaponFoley};

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
    pub on_floor: bool,
    pub action: ViewAction,
    /// The magazine is empty (a pistol's slide locks back).
    pub empty: bool,
    /// The player's own turn rate (radians a second: `x` yaw left, `y`
    /// pitch up), without the recoil climb. The weapon lags it.
    pub look_velocity: Vector2,
    /// Velocity in the view's frame (m/s: `x` right, `y` up, `z` forward).
    pub velocity: Vector3,
    /// 0 at a footstep, rising to 1 at the next (`footsteps::Stride`), so the
    /// weapon's bob lands with the camera's and the footstep sounds.
    pub stride_phase: f32,
    /// Sprint-out left, 1 just after a sprint ends to 0 when the weapon can
    /// fire. The weapon is back on target exactly when it can shoot.
    pub sprint_out: f32,
    pub crouched: bool,
    pub sliding: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ViewAction {
    Ready,
    /// A magazine reload, `elapsed` seconds in; `empty` started from an
    /// empty magazine.
    Reloading {
        elapsed: f32,
        empty: bool,
    },
    /// A shell-by-shell reload: `elapsed` seconds into a stage of `length`.
    Loading {
        stage: LoadStage,
        elapsed: f32,
        length: f32,
    },
    /// Progress 0..1.
    Lowering(f32),
    Raising(f32),
    /// Working the bolt or pump.
    Cycling(f32),
}

/// One shot's kick on the weapon model, as peaks: (position in metres: `x`
/// right, `y` up, `z` back toward the eye; rotation in degrees: pitch up,
/// yaw left, roll). `shot` hashes the variation, `strength` is the weapon's
/// `kick_strength`, and `ads` blends from the hip to the aimed kick.
///
/// At the hip the gun shoves back, tips its muzzle up, and twists a little.
/// Aimed it shoves back hard and the front sight kicks up a fraction of a
/// degree about the rear sight, back on the bullet path within about 70 ms,
/// before the next round of any automatic: the view's own climb is the
/// simulation's recoil.
fn shot_kick(shot: u32, strength: f32, ads: f32) -> (Vector3, Vector3) {
    let hip_pos = Vector3::new(0.0, 0.002 * strength, (0.013 * strength).min(0.04));
    let aim_pos = Vector3::new(0.0, 0.0015 * strength, (0.017 * strength).min(0.045));
    let lift = 0.75 + 0.25 * rng::unit(shot, 31);
    let hip_rot = Vector3::new(
        (1.2 * strength * lift).min(4.0),
        0.4 * strength * rng::signed(shot, 32),
        1.0 * strength * rng::signed(shot, 33),
    );
    let aim_rot = Vector3::new(
        (0.18 * strength * lift).min(0.6),
        0.08 * strength * rng::signed(shot, 35),
        0.6 * strength * rng::signed(shot, 36),
    );
    (hip_pos.lerp(aim_pos, ads), hip_rot.lerp(aim_rot, ads))
}

/// Kick springs: stiff, so the shove peaks within about two frames and is
/// back in about 100 ms, and slightly underdamped for a small rebound.
const KICK_POS_STIFFNESS: f32 = 1800.0;
const KICK_POS_ZETA: f32 = 0.62;
const KICK_ROT_STIFFNESS: f32 = 1800.0;
const KICK_ROT_ZETA: f32 = 0.7;

/// The aim-down-sights blend as the weapon moves: eased in and out, so it
/// leaves the hip quickly and slows onto the sights. The world zoom uses it
/// too, so the two arrive together.
pub fn ads_ease(ads: f32) -> f32 {
    let t = ads.clamp(0.0, 1.0);
    // Smoothstep weighted toward the end: a quick start, a long settle.
    let s = t * t * (3.0 - 2.0 * t);
    let out = 1.0 - (1.0 - t).powi(3);
    0.5 * (s + out)
}

/// How far the weapon turns against the player's own turning (radians:
/// pitch, yaw, roll) at `look_velocity`, before the spring: it lags the view,
/// and cants into a turn. Much smaller aimed, so the sights stay usable.
fn look_sway_target(look_velocity: Vector2, ads: f32) -> Vector3 {
    let lag = 0.010 + (0.0025 - 0.010) * ads;
    // Aimed the weapon barely turns, so the sights stay on the bullet path;
    // its lag shows as a small shift instead (see `ViewmodelRig::update`).
    let limit = (2.5 + (0.15 - 2.5) * ads).to_radians();
    let roll_limit = (3.0 + (0.4 - 3.0) * ads).to_radians();
    Vector3::new(
        (-look_velocity.y * lag).clamp(-limit, limit),
        (-look_velocity.x * lag).clamp(-limit, limit),
        (look_velocity.x * lag).clamp(-roll_limit, roll_limit),
    )
}

/// The bob over two steps (`theta` 0 to 2 pi; a footstep lands at 0 and at
/// pi), as (position, rotation in degrees). A figure-8: one sideways sway per
/// two steps, a dip on every footfall. `sprint` adds the running loop.
fn stride_bob(theta: f32, amount: f32, sprint: f32) -> (Vector3, Vector3) {
    let (side, dip) = (theta.sin(), -(2.0 * theta).cos());
    let walk_pos = Vector3::new(0.007 * side, 0.008 * dip, 0.0);
    let walk_rot = Vector3::new(0.6 * dip, 0.35 * side, 0.5 * side);
    let run_pos = Vector3::new(0.022 * side, 0.012 * dip, 0.004 * dip);
    let run_rot = Vector3::new(2.0 * dip, 3.0 * side, 5.0 * side);
    (
        (walk_pos + run_pos * sprint) * amount,
        (walk_rot + run_rot * sprint) * amount,
    )
}

/// The scale the suppressor and foregrip models are built at (the older
/// gun pack's), for weapons whose own model is in metres.
const LEGACY_ATTACHMENT_SCALE: f32 = 0.163;
/// Height an optic needs between its mount and its sight line.
const OPTIC_CLEARANCE: f32 = 0.018;

/// Seconds an ejected shell stays visible.
const SHELL_LIFETIME: f32 = 0.8;
/// Enough for 0.8 s of the fastest automatic (900 rounds a minute) and a margin.
const SHELL_POOL: usize = 24;

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
    /// Kick impulse scale for this weapon's model: how hard it shoves back and,
    /// at the hip, tips up. The view climb comes from the camera, not the model.
    #[export]
    #[init(val = 1.0)]
    kick_strength: f32,
    /// Peak camera punch per shot in degrees: pitch up, random yaw, alternating roll.
    /// Visual only: it moves the camera, never the aim, and settles within a few frames.
    #[export]
    #[init(val = Vector3::new(0.3, 0.12, 0.8))]
    camera_punch: Vector3,
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
    /// Gunshot volume. The layers below are offsets from it, in dB: the crack
    /// above 1.5 kHz, the punch below it, and the echo. At 0/0/0 they add up to
    /// the original recording.
    #[export]
    #[init(val = -6.0)]
    fire_volume_db: f32,
    #[export]
    #[init(val = 2.0)]
    snap_db: f32,
    #[export]
    #[init(val = 0.0)]
    body_db: f32,
    #[export]
    #[init(val = -3.0)]
    tail_db: f32,
    /// The action under every shot (bolt carrier or slide slamming home, and
    /// its spring), in dB from `fire_volume_db`. A suppressor leaves it alone,
    /// so suppressed fire is mostly this.
    #[export]
    #[init(val = -10.0)]
    mech_db: f32,
    /// The hollow ring of a nearly empty magazine, in dB from
    /// `fire_volume_db` on the last round. Quieter for earlier rounds.
    #[export]
    #[init(val = -8.0)]
    hollow_db: f32,
    /// Borrow another weapon's gunshot layers (its key), for a weapon without
    /// its own recordings; empty uses this weapon's.
    #[export]
    sound_from: GString,
    /// Pitch of the gunshot, 1 as recorded (lower sounds heavier).
    #[export]
    #[init(val = 1.0)]
    fire_pitch: f32,
    /// Pistol grip rake in degrees (how far the grip leans back from vertical),
    /// for the hand that holds it. The grip positions are the `GripRight` and
    /// `GripLeft` markers.
    #[export]
    #[init(val = 22.0)]
    grip_rake: f32,
    /// Set before the view enters the tree to use it as a third-person weapon:
    /// normal materials, a camera-facing flash, and no sounds or shells.
    #[export]
    world_model: bool,

    #[init(node = "Model")]
    model: OnReady<Gd<Node3D>>,
    #[init(node = "Muzzle/Flash")]
    flash: OnReady<Gd<MeshInstance3D>>,
    #[init(node = "Muzzle/FlashLight")]
    flash_light: OnReady<Gd<OmniLight3D>>,
    /// Built from `assets/audio/weapons/<weapon>/` when the view is ready.
    gunshot: Option<Gunshot>,
    /// Optional marker where shells leave the weapon.
    ejection: Option<Gd<Node3D>>,
    /// Resolved from `weapon` once the table is loaded.
    id: Option<WeaponId>,
    shell_mesh: Option<Gd<CylinderMesh>>,

    flash_time: f32,
    flash_count: u32,
    /// The flash light's energy as authored (read on the first shot).
    flash_energy: f32,
    /// The flash seen from the side: two crossed quads along the barrel.
    plume: Vec<Gd<MeshInstance3D>>,
    /// Two smoke emitters, restarted in turn so one puff can linger while the
    /// next one starts.
    smoke: Vec<Gd<GpuParticles3D>>,
    smoke_next: usize,
    smoke_cooldown: f32,
    /// The attachment fitted (see `set_attachment`), and one asked for before
    /// the view was ready.
    attachment: Option<Attachment>,
    pending_attachment: Option<Attachment>,
    /// Where the left hand held the weapon before a foregrip moved it.
    grip_left_rest: Option<Vector3>,
    /// How far the weapon hangs below the sight line because an optic sits
    /// above its own sights (see `set_sight_drop`).
    sight_drop: f32,
    suppressed: bool,
    /// The model's moving parts (see `tools/build_weapons.py`), found when ready.
    parts: Option<MovingParts>,
    /// Seconds since the last shot, for the pistol slide's kick.
    since_shot: f32,
    /// Where the support hand rests on this weapon (after any foregrip).
    hand_rest: Option<Vector3>,
    /// Where the firing hand rests, and the bolt handle it reaches for.
    right_rest: Option<Vector3>,
    bolt_handle: Option<Vector3>,
    /// 0..1: how far the firing hand has gone to the bolt (turns the grip).
    bolt_hand: f32,
    /// Plays the handling sounds (`weapon_foley`), first person only.
    foley: Option<Gd<WeaponFoley>>,
    /// An optional `ChargingHandle` marker: where the hand charges the weapon.
    charging_handle: Option<Vector3>,
}

/// One moving part: the node, its rest transform, and the transform from its
/// parent's space into this view's space (constant: the parent moves with the view).
struct Part {
    node: Gd<Node3D>,
    rest: Transform3D,
    to_view: Transform3D,
    /// The part's current transform in view space (after `pose`).
    current: Transform3D,
}

impl Part {
    fn find(view: &Gd<Node3D>, name: &str) -> Option<Part> {
        let node = view.try_get_node_as::<Node3D>(&format!("Model/Mesh/{name}"))?;
        let parent = node.get_parent_node_3d()?;
        let to_view = view.get_global_transform().affine_inverse() * parent.get_global_transform();
        let rest = node.get_transform();
        Some(Part {
            current: to_view * rest,
            rest,
            node,
            to_view,
        })
    }

    /// Where a view-space point on the part at rest is now.
    fn track(&self, point: Vector3) -> Vector3 {
        self.current * ((self.to_view * self.rest).affine_inverse() * point)
    }

    /// The part's bounds at rest, in view space: (min, max).
    fn bounds_in_view(&self) -> Option<(Vector3, Vector3)> {
        let mesh = self.node.clone().try_cast::<MeshInstance3D>().ok()?;
        let aabb = mesh.get_aabb();
        let to_view = self.to_view * self.rest;
        let mut min = Vector3::splat(f32::MAX);
        let mut max = Vector3::splat(f32::MIN);
        for i in 0..8 {
            let corner = to_view * aabb.get_corner(i);
            min = min.coord_min(corner);
            max = max.coord_max(corner);
        }
        Some((min, max))
    }

    /// Rest position in view space.
    fn rest_in_view(&self) -> Vector3 {
        self.to_view * self.rest.origin
    }

    /// Moves the part by `offset` (view space) and turns it by `turn` (radians,
    /// about the view's Z axis through `pivot`, a view-space point).
    fn pose(&mut self, offset: Vector3, turn: f32, pivot: Vector3) {
        let rest = self.to_view * self.rest;
        let spin = Basis::from_axis_angle(Vector3::BACK, turn);
        let rotated = Transform3D::new(spin, pivot - spin * pivot) * rest;
        let moved = Transform3D::new(rotated.basis, rotated.origin + offset);
        self.current = moved;
        self.node
            .set_transform(self.to_view.affine_inverse() * moved);
    }
}

#[derive(Default)]
struct MovingParts {
    magazine: Option<Part>,
    /// A copy of the magazine: the fresh one brought in during a reload.
    fresh_magazine: Option<Part>,
    pump: Option<Part>,
    bolt: Option<Part>,
    slide: Option<Part>,
}

/// The firing hand on the bolt handle: off the grip just before the bolt
/// lifts, back once it has turned down.
const BOLT_HAND: [(f32, f32); 4] = [(0.0, 0.0), (0.09, 1.0), (0.8, 1.0), (0.94, 0.0)];
/// From the handle to where the hand's grip centre sits (the fingers wrap
/// it from behind and below).
const BOLT_HAND_OFFSET: Vector3 = Vector3::new(0.012, -0.03, 0.03);
/// Extra rake (degrees) for the hand on the handle.
const BOLT_HAND_RAKE: f32 = 35.0;
/// A pistol's press check over its draw: the slide eased back and let go.
const PRESS_CHECK: [(f32, f32); 4] = [(0.5, 0.0), (0.64, 1.0), (0.72, 1.0), (0.8, 0.0)];

#[godot_api]
impl INode3D for WeaponView {
    fn ready(&mut self) {
        self.id = WeaponId::by_key(&self.weapon.to_string());
        if self.world_model {
            self.ready_world();
            let pending = self.pending_attachment.take();
            self.set_attachment(pending);
            return;
        }
        let shader = load::<Shader>("res://shaders/viewmodel.gdshader");
        convert_to_viewmodel_materials(&self.model.clone().upcast(), &shader, self.albedo_boost);
        if let Some(sights) = self.base().try_get_node_as::<Node>("Sights") {
            convert_to_viewmodel_materials(&sights, &shader, 1.0);
        }
        self.ejection = self.base().try_get_node_as("Ejection");
        if self.id.is_none() {
            godot_error!("WeaponView: unknown weapon `{}`", self.weapon);
        }
        self.shell_mesh = Some(shell_mesh(&shader, self.shell_color, self.shell_size));
        self.gunshot = self.load_gunshot();
        if self.gunshot.is_none() {
            godot_error!("WeaponView: no gunshot layers for `{}`", self.weapon);
        }
        self.flash.set_visible(false);
        self.flash_light.set_visible(false);
        self.show_world_parts(false);
        self.build_muzzle_effects(true);
        let pending = self.pending_attachment.take();
        self.set_attachment(pending);
        self.find_parts();
        self.add_foley();
    }

    fn process(&mut self, delta: f64) {
        self.since_shot += delta as f32;
        self.smoke_cooldown -= delta as f32;
        if self.flash_time > 0.0 {
            self.flash_time -= delta as f32;
            if self.flash_time <= 0.0 {
                self.flash.set_visible(false);
                self.flash_light.set_visible(false);
                for plume in &mut self.plume {
                    plume.set_visible(false);
                }
            }
        }
    }
}

#[godot_api]
impl WeaponView {
    /// A handling beat of the weapon: `mag_out`, `mag_in`, `bolt_release`,
    /// `charge`, `shell_in`, `pump_back`, `pump_forward`, `bolt_up`,
    /// `bolt_back`, `bolt_forward`, `bolt_down`, `draw`, `holster`,
    /// `ads_in`, `ads_out`, or `inspect`. Sounds (`weapon_foley`) play from
    /// it. Each event with a contact (the magazine seating, the bolt slamming
    /// home) is emitted `weapon_foley::lead(name)` seconds before it.
    #[signal]
    fn foley_event(name: StringName);
}

impl WeaponView {
    fn add_foley(&mut self) {
        let mut foley = WeaponFoley::new_alloc();
        foley.set_name("Foley");
        foley.bind_mut().weapon = self.weapon.clone();
        self.base_mut().add_child(&foley);
        let callable = foley.callable("play");
        self.base_mut().connect("foley_event", &callable);
        self.foley = Some(foley);
    }

    pub fn weapon_id(&self) -> Option<WeaponId> {
        self.id
    }

    fn find_parts(&mut self) {
        self.hand_rest = self
            .base()
            .try_get_node_as::<Node3D>("GripLeft")
            .map(|m| m.get_position());
        let view = self.to_gd().upcast::<Node3D>();
        self.right_rest = self
            .base()
            .try_get_node_as::<Node3D>("GripRight")
            .map(|m| m.get_position());
        let bolt = Part::find(&view, "Bolt");
        // The handle is the bolt's far right end, halfway up.
        self.bolt_handle = bolt
            .as_ref()
            .and_then(Part::bounds_in_view)
            .map(|(min, max)| {
                Vector3::new(
                    max.x - 0.006,
                    (min.y + max.y) * 0.5,
                    (min.z + max.z) * 0.5 + 0.01,
                )
            });
        let magazine = Part::find(&view, "Magazine");
        let fresh_magazine = magazine.as_ref().and_then(|mag| {
            let mut copy = mag.node.duplicate_node();
            copy.set_name("FreshMagazine");
            copy.set_visible(false);
            mag.node.get_parent()?.add_child(&copy);
            Part::find(&view, "FreshMagazine")
        });
        self.charging_handle = self
            .base()
            .try_get_node_as::<Node3D>("ChargingHandle")
            .map(|m| m.get_position());
        self.parts = Some(MovingParts {
            magazine,
            fresh_magazine,
            pump: Part::find(&view, "Pump"),
            bolt,
            slide: Part::find(&view, "Slide"),
        });
    }

    /// The points the hands reach for in this weapon's clips.
    pub fn anchors(&self, kind: ClipKind) -> Anchors {
        let mag_rest = self
            .parts
            .as_ref()
            .and_then(|p| p.magazine.as_ref())
            .map(Part::rest_in_view);
        Anchors::new(
            kind,
            self.hand_rest.unwrap_or_default(),
            mag_rest,
            self.charging_handle,
        )
    }

    /// Poses the moving parts and the hands for this frame: the magazines,
    /// bolt, slide, and support hand from the clip playing (`clip`), the pump
    /// with the hand and the bolt with the firing hand through a cycle, and
    /// the pistol's slide after each shot and when empty.
    pub fn animate(&mut self, frame: &ViewFrame, clip: &ClipPose) {
        let Some(mut parts) = self.parts.take() else {
            return;
        };
        let rest_hand = self.hand_rest.unwrap_or_default();
        let mut hand = clip.hand;
        let mut right_hand = None;
        if let Some(mag) = &mut parts.magazine {
            let shown = clip.old_mag.is_some();
            if mag.node.is_visible() != shown {
                mag.node.set_visible(shown);
            }
            mag.pose(clip.old_mag.unwrap_or_default(), 0.0, Vector3::ZERO);
        }
        if let Some(fresh) = &mut parts.fresh_magazine {
            let shown = clip.fresh_mag.is_some();
            if fresh.node.is_visible() != shown {
                fresh.node.set_visible(shown);
            }
            fresh.pose(clip.fresh_mag.unwrap_or_default(), 0.0, Vector3::ZERO);
        }
        if let Some(pump) = &mut parts.pump {
            let drawn = match frame.action {
                ViewAction::Cycling(p) => keyed(&PUMP_BACK, p),
                _ => clip.pump.unwrap_or(0.0),
            };
            let back = drawn * 0.075;
            // The pump slides back toward the shooter, the hand with it.
            pump.pose(Vector3::new(0.0, 0.0, back), 0.0, Vector3::ZERO);
            if back > 0.0 && clip.pump.is_none() {
                hand = Some(rest_hand + Vector3::new(0.0, 0.0, back));
            }
        }
        if let Some(bolt) = &mut parts.bolt {
            let ((lift, back), reach) = match (frame.action, clip.bolt) {
                (ViewAction::Cycling(p), _) => (
                    (keyed(&BOLT_LIFT, p), keyed(&BOLT_BACK, p)),
                    keyed(&BOLT_HAND, p),
                ),
                (_, Some(worked)) => (worked, clip.bolt_hand),
                _ => ((0.0, 0.0), clip.bolt_hand),
            };
            let center = bolt.rest_in_view();
            // Up about the bore, then back along it.
            let pivot = Vector3::new(0.0, center.y - 0.005, center.z);
            bolt.pose(Vector3::new(0.0, 0.0, back * 0.065), lift * 1.1, pivot);
            // The firing hand leaves the grip, works the handle, and comes back.
            if let (Some(handle), Some(rest)) = (self.bolt_handle, self.right_rest)
                && reach > 0.0
            {
                let on_handle = bolt.track(handle) + BOLT_HAND_OFFSET;
                right_hand = Some(rest.lerp(on_handle, reach));
            }
            self.bolt_hand = reach;
        }
        if let Some(slide) = &mut parts.slide {
            let kick = (1.0 - (self.since_shot / 0.07)).clamp(0.0, 1.0);
            let reloading = matches!(frame.action, ViewAction::Reloading { .. });
            let locked = frame.empty && !reloading;
            // Drawn: a press check, the slide eased back and let go.
            let check = match frame.action {
                ViewAction::Raising(p) => keyed(&PRESS_CHECK, p) * 0.45,
                _ => 0.0,
            };
            let back = match clip.slide {
                Some(held) => held.max(kick),
                None if locked => 1.0,
                None => kick.max(check),
            };
            slide.pose(Vector3::new(0.0, 0.0, back * 0.028), 0.0, Vector3::ZERO);
        }
        if let Some(mut marker) = self.base().try_get_node_as::<Node3D>("GripLeft") {
            marker.set_position(hand.unwrap_or(rest_hand));
        }
        if let (Some(rest), Some(mut marker)) = (
            self.right_rest,
            self.base().try_get_node_as::<Node3D>("GripRight"),
        ) {
            marker.set_position(right_hand.unwrap_or(rest));
        }
        self.parts = Some(parts);
    }

    /// Emits a foley event for a beat of a clip (see the `foley_event`
    /// signal). Deferred to the end of the frame: the beat is found while the
    /// player is updating its view, and a listener may want to ask the player
    /// something.
    pub fn foley(&mut self, name: &str) {
        let args = [
            "foley_event".to_variant(),
            StringName::from(name).to_variant(),
        ];
        self.base_mut().call_deferred("emit_signal", &args);
    }

    /// Asks for an attachment on a view that is not in the tree yet.
    pub fn set_pending_attachment(&mut self, attachment: Option<Attachment>) {
        self.pending_attachment = attachment;
    }

    /// Fits an attachment's parts, or takes them off: a suppressor on the
    /// muzzle (no flash, a quieter shot), a foregrip under the handguard
    /// (the left hand moves onto it), or a red dot or ACOG in place of the
    /// iron sights.
    pub fn set_attachment(&mut self, attachment: Option<Attachment>) {
        if self.attachment == attachment {
            return;
        }
        self.attachment = attachment;
        if let Some(mut old) = self.base().get_node_or_null("Attachment") {
            self.base_mut().remove_child(&old);
            old.queue_free();
        }
        let mut marker = self.base().try_get_node_as::<Node3D>("GripLeft");
        if let (Some(rest), Some(marker)) = (self.grip_left_rest.take(), &mut marker) {
            marker.set_position(rest);
        } else if let (Some(rest), Some(marker)) = (self.hand_rest, &mut marker) {
            // Undo any reload pose before measuring the plain grip.
            marker.set_position(rest);
        }
        let mut parts = Node3D::new_alloc();
        parts.set_name("Attachment");
        let optic = matches!(attachment, Some(Attachment::RedDot | Attachment::Acog));
        // Models built in metres (tools/build_weapons.py) sit unscaled; the
        // older pack models are about 1/6 scale, and so are the suppressor
        // and foregrip models.
        let model_scale = self.model.get_scale().x;
        let scale = if (model_scale - 1.0).abs() < 1e-3 {
            LEGACY_ATTACHMENT_SCALE
        } else {
            model_scale
        };
        // An optic on a weapon with a mount (and no built-in iron sight
        // node) sits on the mount, high enough to clear it: the weapon drops
        // that far below the sight line, so the optic is what lines up.
        let mount = self
            .base()
            .try_get_node_as::<Node3D>("OpticMount")
            .map(|m| m.get_position() + Vector3::new(0.0, self.sight_drop, 0.0));
        let drop = match (optic, mount) {
            (true, Some(m)) => (m.y + OPTIC_CLEARANCE).max(0.0),
            _ => 0.0,
        };
        self.set_sight_drop(drop);
        self.show_part("OpticRail", optic);
        let mut iron = self.base().try_get_node_as::<IronSights>("Sights");
        if let Some(iron) = &mut iron {
            iron.set_visible(!optic);
        }
        self.suppressed = attachment == Some(Attachment::Suppressor);
        if let Some(gunshot) = &mut self.gunshot {
            gunshot.set_suppressed(self.suppressed);
        }
        match attachment {
            Some(Attachment::Suppressor) => {
                let muzzle = self
                    .base()
                    .try_get_node_as::<Node3D>("Muzzle")
                    .map(|m| m.get_position())
                    .unwrap_or_default();
                // The model's thread end (x = -0.095) on the muzzle, body forward.
                if let Some(mut mesh) =
                    instance_model("res://assets/weapons/suppressor/suppressor.glb")
                {
                    mesh.set_rotation(Vector3::new(0.0, PI / 2.0, 0.0));
                    mesh.set_scale(Vector3::splat(scale));
                    mesh.set_position(muzzle - Vector3::new(0.0, 0.0, 0.095 * scale));
                    parts.add_child(&mesh);
                }
            }
            Some(Attachment::Grip) => {
                if let Some(mut marker) = self.base().try_get_node_as::<Node3D>("GripLeft") {
                    let rest = marker.get_position();
                    // The grip's mount (y = 0.423) just under the handguard.
                    if let Some(mut mesh) =
                        instance_model("res://assets/weapons/foregrip/foregrip.glb")
                    {
                        mesh.set_scale(Vector3::splat(scale));
                        mesh.set_position(rest - Vector3::new(0.0, 0.423 * scale + 0.006, 0.0));
                        parts.add_child(&mesh);
                    }
                    self.grip_left_rest = Some(rest);
                    marker.set_position(rest - Vector3::new(0.0, 0.045, -0.004));
                }
            }
            Some(Attachment::RedDot | Attachment::Acog) => {
                let acog = attachment == Some(Attachment::Acog);
                // From the iron sights: in front of the rear sight. From a
                // mount: centred on it.
                let (z, base, size) = match (&iron, mount) {
                    (Some(s), _) => {
                        let (rear_z, base, size) = s.bind().placement();
                        (rear_z - if acog { 0.03 } else { 0.09 }, base, size)
                    }
                    (None, Some(m)) => (m.z + if acog { 0.017 } else { 0.0 }, m.y - drop, 1.0),
                    (None, None) => (-0.09, -0.022, 1.0),
                };
                let (style, glow) = if acog {
                    ("acog", Color::from_rgba(1.0, 0.25, 0.1, 1.0))
                } else {
                    ("reflex", Color::from_rgba(1.0, 0.12, 0.06, 1.0))
                };
                let mut sights = IronSights::optic(style, z, base, size, glow);
                sights.set_name("Optic");
                parts.add_child(&sights);
            }
            Some(Attachment::ExtendedMag) | None => {}
        }
        self.base_mut().add_child(&parts);
        if self.parts.is_some() {
            self.hand_rest = self
                .base()
                .try_get_node_as::<Node3D>("GripLeft")
                .map(|m| m.get_position());
        }
        if !self.world_model {
            let shader = load::<Shader>("res://shaders/viewmodel.gdshader");
            convert_to_viewmodel_materials(&parts.clone().upcast(), &shader, self.albedo_boost);
        }
    }

    /// Lowers the weapon (its model and markers) `drop` metres below the
    /// sight line, from wherever it was.
    fn set_sight_drop(&mut self, drop: f32) {
        let delta = drop - self.sight_drop;
        if delta.abs() < 1e-6 {
            return;
        }
        self.sight_drop = drop;
        let down = Vector3::new(0.0, -delta, 0.0);
        for name in [
            "Model",
            "Muzzle",
            "Ejection",
            "GripRight",
            "GripLeft",
            "OpticMount",
        ] {
            if let Some(mut node) = self.base().try_get_node_as::<Node3D>(name) {
                let at = node.get_position();
                node.set_position(at + down);
            }
        }
        for rest in [
            &mut self.hand_rest,
            &mut self.right_rest,
            &mut self.grip_left_rest,
        ]
        .into_iter()
        .flatten()
        {
            *rest += down;
        }
        if self.parts.is_some() {
            self.find_parts();
        }
    }

    /// Models from tools/build_weapons.py carry `World`, every fixed part
    /// merged into one mesh. Third person draws that and the magazine (the
    /// only part a third-person reload moves); first person draws the parts.
    fn show_world_parts(&self, world: bool) {
        let Some(mesh) = self.base().try_get_node_as::<Node3D>("Model/Mesh") else {
            return;
        };
        if !mesh.has_node("World") {
            return;
        }
        for child in mesh.get_children().iter_shared() {
            let Ok(mut part) = child.try_cast::<Node3D>() else {
                continue;
            };
            let name = part.get_name().to_string();
            match name.as_str() {
                "World" => part.set_visible(world),
                // Hidden until an optic shows it, and the spare stays hidden.
                "OpticRail" | "MagazineSpare" => {}
                "Magazine" => {
                    if let Some(mut rounds) = part.try_get_node_as::<Node3D>("Rounds") {
                        rounds.set_visible(!world);
                    }
                }
                _ => part.set_visible(!world),
            }
        }
    }

    /// Shows or hides one of the model's parts, if it has it.
    fn show_part(&self, name: &str, visible: bool) {
        if let Some(mut node) = self
            .base()
            .try_get_node_as::<Node3D>(&format!("Model/Mesh/{name}"))
        {
            node.set_visible(visible);
        }
    }

    /// Where the hands hold this weapon, in its own space.
    pub fn grips(&self) -> Grips {
        // On the bolt the hand turns palm-down over the handle.
        let rake = self.grip_rake + BOLT_HAND_RAKE * self.bolt_hand;
        crate::character::rig::grips_of(&self.to_gd().upcast(), rake)
    }

    /// Third-person setup: brightened standard materials (the low-poly pack is
    /// dark), a flash that faces the camera, a shorter flash light, and no
    /// gunshot layers.
    fn ready_world(&mut self) {
        self.show_world_parts(true);
        brighten_materials(&self.model.clone().upcast(), self.albedo_boost.sqrt());
        if let Some(material) = self.flash.get_active_material(0)
            && let Ok(mut world) = material.duplicate_resource().try_cast::<ShaderMaterial>()
        {
            world.set_shader_parameter("viewmodel", &false.to_variant());
            self.flash.set_surface_override_material(0, &world);
        }
        self.flash.set_scale(Vector3::splat(1.6));
        self.flash_light
            .set_param(godot::classes::light_3d::Param::RANGE, 4.0);
        // The sound players stay but never play: other players' gunfire is
        // positional and comes from their own node.
        self.flash.set_visible(false);
        self.flash_light.set_visible(false);
        self.build_muzzle_effects(false);
    }

    /// The side plume and the smoke emitters, built in code so every weapon
    /// scene gets them. `viewmodel` draws them with the weapon's projection.
    fn build_muzzle_effects(&mut self, viewmodel: bool) {
        let Some(mut muzzle) = self
            .flash
            .get_parent()
            .and_then(|p| p.try_cast::<Node3D>().ok())
        else {
            return;
        };
        let length = if viewmodel { 0.32 } else { 0.5 };
        if let Some(material) = self.flash.get_active_material(0)
            && let Ok(mut side) = material.duplicate_resource().try_cast::<ShaderMaterial>()
        {
            side.set_shader_parameter("shape", &1.to_variant());
            side.set_shader_parameter("viewmodel", &viewmodel.to_variant());
            let mut quad = QuadMesh::new_gd();
            quad.set_size(Vector2::new(length, length * 0.38));
            quad.set_material(&side);
            // Two quads crossed along the barrel (-Z), so the plume shows from
            // above and from the side. The quad's +X points down the barrel.
            for roll in [0.0, PI * 0.5] {
                let mut plume = MeshInstance3D::new_alloc();
                plume.set_mesh(&quad);
                plume.set_cast_shadows_setting(
                    godot::classes::geometry_instance_3d::ShadowCastingSetting::OFF,
                );
                let basis = Basis::from_axis_angle(Vector3::FORWARD, roll)
                    * Basis::from_axis_angle(Vector3::UP, PI * 0.5);
                plume.set_transform(Transform3D::new(
                    basis,
                    Vector3::new(0.0, 0.0, -length * 0.5),
                ));
                plume.set_visible(false);
                muzzle.add_child(&plume);
                self.plume.push(plume);
            }
        }
        let shader = load::<Shader>("res://shaders/muzzle_smoke.gdshader");
        for _ in 0..2 {
            let smoke = smoke_emitter(&shader, viewmodel);
            muzzle.add_child(&smoke);
            self.smoke.push(smoke);
        }
    }

    /// Flash, plume, and maybe a puff of smoke, for one shot.
    fn show_flash(&mut self) {
        self.flash_count = self.flash_count.wrapping_add(1);
        let seed = (self.flash_count % 97) as f32;
        self.flash
            .set_instance_shader_parameter("seed", &seed.to_variant());
        self.flash.set_visible(true);
        self.flash_light.set_visible(true);
        for plume in &mut self.plume {
            plume.set_instance_shader_parameter("seed", &seed.to_variant());
            plume.set_visible(true);
        }
        // About three frames at 60 FPS: visible on every shot, yet off again
        // between rounds at 750 RPM (80 ms), so automatic fire flickers.
        self.flash_time = 0.05;
        if self.smoke_cooldown <= 0.0 && !self.smoke.is_empty() {
            let index = self.smoke_next % self.smoke.len();
            self.smoke[index].restart();
            self.smoke_next = index + 1;
            self.smoke_cooldown = 0.14;
        }
    }

    /// A muzzle flash with no sound, for third-person weapons.
    pub fn flash_world(&mut self) {
        let roll = (self.flash_count as f32 * 2.399_963) % (2.0 * PI);
        self.flash.set_rotation(Vector3::new(0.0, 0.0, roll));
        self.show_flash();
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

    /// A shot left `rounds_left` in a magazine of `magazine`, `ads` of the
    /// way to aimed. Aimed, the flash and its light are smaller, so they do
    /// not wash out the sights.
    pub fn on_fired(&mut self, rounds_left: u16, magazine: u16, ads: f32) {
        // Cheap variation without a random generator: rotate by a golden-angle step.
        let next = self.flash_count.wrapping_add(1);
        let roll = (next as f32 * 2.399_963) % (2.0 * PI);
        let scale = (0.85 + 0.3 * ((next * 7 % 10) as f32 / 10.0)) * (1.0 - 0.6 * ads);
        self.flash.set_rotation(Vector3::new(0.0, 0.0, roll));
        self.flash.set_scale(Vector3::splat(scale));
        if self.flash_energy == 0.0 {
            self.flash_energy = self.flash_light.get_param(Param::ENERGY);
        }
        self.flash_light
            .set_param(Param::ENERGY, self.flash_energy * (1.0 - 0.7 * ads));
        // A suppressor hides the flash.
        if !self.suppressed {
            self.show_flash();
        }
        self.since_shot = 0.0;

        let tree = self.base().get_tree();
        if let Some(gunshot) = &mut self.gunshot {
            gunshot.play(rounds_left, magazine, &tree);
        }
    }

    /// Ammunition or a weapon picked up: magazines into pouches (a new
    /// weapon also gets its draw from the raise).
    pub fn on_picked_up(&mut self) {
        if let Some(foley) = &mut self.foley {
            foley.bind_mut().play("pickup".into());
        }
    }

    fn load_gunshot(&mut self) -> Option<Gunshot> {
        let key = if self.sound_from.is_empty() {
            &self.weapon
        } else {
            &self.sound_from
        };
        let dir = format!("res://assets/audio/weapons/{key}");
        let pitch = self.fire_pitch;
        let (snap_db, body_db, tail_db) = (self.snap_db, self.body_db, self.tail_db);
        let mut layer = |name: &str, node: &str, offset_db: f32, polyphony: i32| {
            // Pitch is set per shot in `Gunshot::play`, the same for every layer.
            let stream = audio::variations(&format!("{dir}/{name}"), 1.0)?;
            let mut player = AudioStreamPlayer::new_alloc();
            player.set_name(node);
            player.set_stream(&stream);
            player.set_volume_db(self.fire_volume_db + offset_db);
            player.set_bus("Weapons");
            player.set_max_polyphony(polyphony);
            self.base_mut().add_child(&player);
            Some(player)
        };
        let snap = layer("snap", "GunshotSnap", snap_db, 4)?;
        let body = layer("body", "GunshotBody", body_db, 4)?;
        // One echo at a time: each shot cuts the last one's tail, so full
        // auto stays tight instead of piling up reverb. Outdoors and indoors
        // each have their own, cross-faded by where the listener is.
        let tail = layer("tail", "GunshotTail", tail_db, 1)?;
        let tail_indoor = layer("tail_indoor", "GunshotTailIndoor", tail_db, 1);
        // The shot as heard from down the range, for its slapback off walls.
        let distant = audio::variations(&format!("{dir}/mid"), 1.0);
        // Optional layers live with the weapon's own model key first (a gun
        // that borrows another's shot still has its own action).
        let own = format!("res://assets/audio/weapons/{}", self.weapon);
        let (mech_db, hollow_db) = (self.mech_db, self.hollow_db);
        let mut optional = |name: &str, node: &str, offset_db: f32| {
            [&own, &dir].into_iter().find_map(|d| {
                let stream = audio::variations(&format!("{d}/{name}"), 1.03)?;
                let mut player = AudioStreamPlayer::new_alloc();
                player.set_name(node);
                player.set_stream(&stream);
                player.set_volume_db(self.fire_volume_db + offset_db);
                player.set_bus("Weapons");
                player.set_max_polyphony(3);
                self.base_mut().add_child(&player);
                Some(player)
            })
        };
        let mech = optional("mech", "GunshotMech", mech_db);
        let hollow = optional("hollow", "GunshotHollow", hollow_db).map(|player| {
            let volume = player.get_volume_db();
            (player, volume)
        });
        Some(Gunshot {
            mech,
            hollow,
            pitch,
            volumes: [
                snap.get_volume_db(),
                body.get_volume_db(),
                tail.get_volume_db(),
            ],
            snap,
            body,
            tail,
            tail_indoor,
            distant,
            suppressed: false,
        })
    }

    pub fn on_dry_fire(&mut self) {
        if let Some(foley) = &mut self.foley {
            foley.bind_mut().play("dry".into());
        }
    }
}

/// The layers of one gunshot, played together. Each picks its own recording,
/// so a few source shots combine into many different-sounding ones.
struct Gunshot {
    /// The action under every shot, if the weapon has one.
    mech: Option<Gd<AudioStreamPlayer>>,
    /// The last-rounds ring, if the weapon has a detachable magazine, and
    /// its volume on the last round.
    hollow: Option<(Gd<AudioStreamPlayer>, f32)>,
    pitch: f32,
    /// Each layer's volume as built, before any suppressor.
    volumes: [f32; 3],
    snap: Gd<AudioStreamPlayer>,
    body: Gd<AudioStreamPlayer>,
    tail: Gd<AudioStreamPlayer>,
    /// The tail indoors (a room's early reflections and decay, no terrain echo).
    tail_indoor: Option<Gd<AudioStreamPlayer>>,
    /// The shot from a distance, played off nearby walls outdoors.
    distant: Option<Gd<AudioStreamRandomizer>>,
    suppressed: bool,
}

impl Gunshot {
    /// A suppressor takes the crack off and most of the echo.
    fn set_suppressed(&mut self, on: bool) {
        self.suppressed = on;
        let cut = if on { [14.0, 7.0, 12.0] } else { [0.0; 3] };
        for ((player, volume), cut) in [&mut self.snap, &mut self.body, &mut self.tail]
            .into_iter()
            .zip(self.volumes)
            .zip(cut)
        {
            player.set_volume_db(volume - cut);
        }
    }

    /// Plays a shot that left `rounds_left` in a magazine of `magazine`.
    fn play(&mut self, rounds_left: u16, magazine: u16, tree: &Gd<SceneTree>) {
        let pitch = randf_range(0.96, 1.04) as f32 * self.pitch;
        for player in [&mut self.snap, &mut self.body] {
            player.set_pitch_scale(pitch);
            player.play();
        }
        // Outdoors the terrain echo, indoors the room: cross-faded by how far
        // indoors the listener is (`acoustics::listener_indoors`).
        let indoors = match &self.tail_indoor {
            Some(_) => acoustics::listener_indoors(),
            None => 0.0,
        };
        let base_tail = self.volumes[2] - if self.suppressed { 12.0 } else { 0.0 };
        for (player, share) in [
            (Some(&mut self.tail), 1.0 - indoors),
            (self.tail_indoor.as_mut(), indoors),
        ] {
            let Some(player) = player else { continue };
            if share > 0.02 {
                player.set_volume_db(base_tail + 20.0 * share.log10());
                player.set_pitch_scale(pitch);
                player.play();
            }
        }
        // Outdoors, the shot answers off the nearest walls.
        if let (Some(distant), Some(mut environment)) =
            (&self.distant, AudioEnvironment::find(tree))
        {
            let level = self.volumes[0] - 8.0 - if self.suppressed { 10.0 } else { 0.0 };
            environment.bind_mut().slapback(distant, level);
        }
        if let Some(mech) = &mut self.mech {
            mech.play();
        }
        if let (Some((hollow, base)), Some(ring)) =
            (&mut self.hollow, hollow_ring(rounds_left, magazine))
        {
            hollow.set_volume_db(*base + ring.volume_db);
            hollow.set_pitch_scale(ring.pitch);
            hollow.play();
        }
    }
}

/// How the last-rounds ring sounds for one shot: louder and a touch higher as
/// the magazine empties.
#[derive(Clone, Copy, Debug, PartialEq)]
struct HollowRing {
    volume_db: f32,
    pitch: f32,
}

/// The ring for a shot that left `rounds_left` of `magazine`, or None while
/// the magazine is still full enough to sound solid. The ring covers the last
/// fifth of the magazine, at least two and at most five rounds.
fn hollow_ring(rounds_left: u16, magazine: u16) -> Option<HollowRing> {
    let span = (magazine / 5).clamp(2, 5);
    if magazine < 4 || rounds_left >= span {
        return None;
    }
    // 0 on the first ringing shot, 1 on the last round.
    let t = f32::from(span - 1 - rounds_left) / f32::from((span - 1).max(1));
    Some(HollowRing {
        volume_db: -9.0 * (1.0 - t),
        pitch: 1.0 + 0.06 * t,
    })
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
                // Shader materials (a sight's lens) already draw as a viewmodel.
                if source.clone().try_cast::<ShaderMaterial>().is_ok() {
                    continue;
                }
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
        use godot::classes::base_material_3d::TextureParam;
        if let Some(texture) = standard.get_texture(TextureParam::ALBEDO) {
            material.set_shader_parameter("albedo_texture", &texture.to_variant());
        }
        // Weapons from tools/build_weapons.py pack occlusion, roughness, and
        // metallic into one ORM texture, used as the roughness texture.
        if let (Some(normal), Some(orm)) = (
            standard.get_texture(TextureParam::NORMAL),
            standard.get_texture(TextureParam::ROUGHNESS),
        ) {
            material.set_shader_parameter("use_maps", &true.to_variant());
            material.set_shader_parameter("normal_texture", &normal.to_variant());
            material.set_shader_parameter("orm_texture", &orm.to_variant());
        }
    }
    material
}

/// A short-lived puff of gun smoke: a few soft particles that leave the muzzle,
/// slow down, rise, grow, and fade. World-space, so the puff stays where the
/// shot was while the weapon moves on.
fn smoke_emitter(shader: &Gd<Shader>, viewmodel: bool) -> Gd<GpuParticles3D> {
    use godot::classes::particle_process_material::Parameter;
    let mut process = ParticleProcessMaterial::new_gd();
    process.set_direction(Vector3::new(0.0, 0.0, -1.0));
    process.set_spread(18.0);
    process.set_param_min(Parameter::INITIAL_LINEAR_VELOCITY, 0.6);
    process.set_param_max(Parameter::INITIAL_LINEAR_VELOCITY, 1.4);
    process.set_param_min(Parameter::DAMPING, 2.0);
    process.set_param_max(Parameter::DAMPING, 3.0);
    process.set_gravity(Vector3::new(0.0, 0.25, 0.0));
    process.set_param_min(Parameter::SCALE, 0.7);
    process.set_param_max(Parameter::SCALE, 1.2);
    let mut growth = Curve::new_gd();
    growth.add_point(Vector2::new(0.0, 0.35));
    growth.add_point(Vector2::new(1.0, 1.0));
    let mut growth_texture = CurveTexture::new_gd();
    growth_texture.set_curve(&growth);
    process.set_param_texture(Parameter::SCALE, &growth_texture);
    let mut fade = Gradient::new_gd();
    fade.set_color(0, Color::from_rgba(1.0, 1.0, 1.0, 0.9));
    fade.set_color(1, Color::from_rgba(1.0, 1.0, 1.0, 0.0));
    let mut fade_texture = GradientTexture1D::new_gd();
    fade_texture.set_gradient(&fade);
    process.set_color_ramp(&fade_texture);

    let mut material = ShaderMaterial::new_gd();
    material.set_shader(shader);
    material.set_shader_parameter("viewmodel", &viewmodel.to_variant());
    let mut quad = QuadMesh::new_gd();
    let size = if viewmodel { 0.22 } else { 0.4 };
    quad.set_size(Vector2::new(size, size));
    quad.set_material(&material);

    let mut particles = GpuParticles3D::new_alloc();
    particles.set_amount(4);
    particles.set_lifetime(0.9);
    particles.set_one_shot(true);
    particles.set_explosiveness_ratio(0.85);
    particles.set_use_local_coordinates(false);
    particles.set_process_material(&process);
    particles.set_draw_pass_mesh(0, &quad);
    particles
        .set_cast_shadows_setting(godot::classes::geometry_instance_3d::ShadowCastingSetting::OFF);
    particles.set_visibility_aabb(Aabb::new(Vector3::splat(-4.0), Vector3::splat(8.0)));
    particles.set_emitting(false);
    particles
}

/// Multiplies the albedo of every standard material under `root` (on copies).
fn brighten_materials(root: &Gd<Node>, boost: f32) {
    for child in root.get_children().iter_shared() {
        if let Ok(mut mesh) = child.clone().try_cast::<MeshInstance3D>() {
            for surface in 0..mesh.get_surface_override_material_count() {
                let Some(source) = mesh.get_active_material(surface) else {
                    continue;
                };
                let Ok(mut copy) = source.duplicate_resource().try_cast::<BaseMaterial3D>() else {
                    continue;
                };
                let a = copy.get_albedo();
                copy.set_albedo(Color::from_rgba(a.r * boost, a.g * boost, a.b * boost, a.a));
                mesh.set_surface_override_material(surface, &copy);
            }
        }
        brighten_materials(&child, boost);
    }
}

/// Instances a model scene (an attachment's), or none if it is missing.
fn instance_model(path: &str) -> Option<Gd<Node3D>> {
    try_load::<PackedScene>(path)
        .ok()?
        .instantiate()?
        .try_cast::<Node3D>()
        .ok()
}

/// The plain weapon an attachment variant is built on (or the weapon itself).
pub fn base_weapon(id: WeaponId) -> WeaponId {
    WeaponId::by_key(id.def().model_key()).unwrap_or(id)
}

thread_local! {
    static WEAPON_SCENES: std::cell::RefCell<std::collections::HashMap<String, Gd<PackedScene>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

const WEAPON_SCENE_DIR: &str = "res://scenes/weapons";

pub fn clear_cache() {
    WEAPON_SCENES.with_borrow_mut(|cache| cache.clear());
}

/// The weapon view scene for a weapon: `scenes/weapons/<key>_view.tscn`, or any
/// scene in that folder whose root `weapon` property is the key.
pub fn weapon_scene(id: WeaponId) -> Option<Gd<PackedScene>> {
    let key = id.def().model_key().to_owned();
    if let Some(scene) = WEAPON_SCENES.with(|c| c.borrow().get(&key).cloned()) {
        return Some(scene);
    }
    let mut loader = godot::classes::ResourceLoader::singleton();
    let direct = format!("{WEAPON_SCENE_DIR}/{key}_view.tscn");
    let scene = if loader.exists(&direct) {
        try_load::<PackedScene>(&direct).ok()
    } else {
        loader
            .list_directory(WEAPON_SCENE_DIR)
            .as_slice()
            .iter()
            .map(|f| f.to_string())
            .filter(|f| f.ends_with(".tscn") || f.ends_with(".scn"))
            .filter_map(|f| try_load::<PackedScene>(&format!("{WEAPON_SCENE_DIR}/{f}")).ok())
            .find(|scene| scene_weapon_key(scene).as_deref() == Some(key.as_str()))
    }?;
    WEAPON_SCENES.with(|c| c.borrow_mut().insert(key, scene.clone()));
    Some(scene)
}

/// The `weapon` property of a weapon view scene's root, read without instancing it.
fn scene_weapon_key(scene: &Gd<PackedScene>) -> Option<String> {
    let state = scene.get_state()?;
    (0..state.get_node_property_count(0))
        .find(|&i| state.get_node_property_name(0, i) == "weapon")
        .map(|i| {
            state
                .get_node_property_value(0, i)
                .to::<GString>()
                .to_string()
        })
}

/// A property of a node in a weapon's view scene (`"."` is the root), read
/// without instancing it. None when the scene leaves it at its default.
pub fn view_scene_property(id: WeaponId, node: &str, property: &str) -> Option<Variant> {
    let state = weapon_scene(id)?.get_state()?;
    let index = if node == "." {
        0
    } else {
        let name = StringName::from(node);
        (0..state.get_node_count()).find(|&i| state.get_node_name(i) == name)?
    };
    (0..state.get_node_property_count(index))
        .find(|&i| state.get_node_property_name(index, i) == property)
        .map(|i| state.get_node_property_value(index, i))
}

/// A weapon view set up as a third-person weapon, not yet in the tree.
pub fn instantiate_world_weapon(id: WeaponId) -> Option<Gd<WeaponView>> {
    let mut view = weapon_scene(id)?
        .instantiate()?
        .try_cast::<WeaponView>()
        .ok()?;
    {
        let mut v = view.bind_mut();
        v.world_model = true;
        v.set_pending_attachment(id.def().attachment);
    }
    Some(view)
}

/// Moves the active [`WeaponView`] procedurally. Child of the camera.
#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct ViewmodelRig {
    base: Base<Node3D>,

    views: Vec<Gd<WeaponView>>,
    /// The player's arms, holding the active view.
    arms: Option<Gd<FpArms>>,
    active: Option<WeaponId>,
    shells: Vec<Shell>,
    next_shell: usize,
    shells_ejected: u32,
    /// Seconds until a bolt or pump ejects its shell.
    pending_shell: Option<(WeaponId, f32)>,
    shell_sound: Option<Gd<AudioStreamPlayer>>,
    cycle_kick: f32,
    pickup_dip: f32,
    /// Seconds into a weapon inspect, or `None`.
    inspect: Option<f32>,
    time: f32,
    bob_blend: f32,
    /// Which foot the stride is on (0 or 1), and the stride phase last frame,
    /// to tell two steps apart for the figure-8.
    foot: f32,
    last_stride: f32,
    sprint_blend: f32,
    /// Where the last frame's clip was, to fire each beat once: which clip,
    /// and its time (seconds, or progress for cycles and switches).
    beat_clock: Option<(BeatClip, f32)>,
    /// The shotgun reload in progress began with an empty tube (it pumps at
    /// the end).
    loading_from_empty: bool,
    /// Shot kick: position (metres) and rotation (radians: pitch, yaw, roll).
    kick_pos: Spring,
    kick_rot: Spring,
    /// Shots fired, to hash each kick's variation.
    shots: u32,
    /// Landing: `y` dip (metres) and `x` pitch (radians).
    land: Spring,
    /// Look sway (radians: pitch, yaw, roll) and movement inertia (metres).
    sway_rot: Spring,
    sway_pos: Spring,
    /// Slowly following velocity, so the inertia reacts to changes of speed.
    smoothed_velocity: Vector3,
    last_ads: f32,
    ads_rising: bool,
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
        self.arms = self.base().try_get_node_as::<FpArms>("Arms");
        self.shell_sound = self.make_player("res://assets/audio/player/shell", -16.0);
    }
}

impl ViewmodelRig {
    fn view(&self, weapon: WeaponId) -> Option<Gd<WeaponView>> {
        let base = base_weapon(weapon);
        self.views
            .iter()
            .find(|v| v.bind().weapon_id() == Some(base))
            .cloned()
    }

    /// A shot was fired: kicks the weapon model and returns this weapon's camera
    /// punch (degrees). Self-loading weapons eject a shell right away; bolt and
    /// pump weapons eject when they cycle.
    pub fn on_fired(&mut self, weapon: WeaponId, eject: bool, rounds_left: u16) -> Vector3 {
        let Some(mut view) = self.view(weapon) else {
            return Vector3::ZERO;
        };
        let (strength, punch) = {
            let v = view.bind();
            (v.kick_strength, v.camera_punch)
        };
        view.bind_mut()
            .on_fired(rounds_left, weapon.def().magazine, self.last_ads);
        self.shots = self.shots.wrapping_add(1);
        let (pos, rot) = shot_kick(self.shots, strength, self.last_ads);
        self.kick_pos.velocity += peak_impulse(pos, KICK_POS_STIFFNESS, KICK_POS_ZETA);
        self.kick_rot.velocity += peak_impulse(
            Vector3::new(rot.x.to_radians(), rot.y.to_radians(), rot.z.to_radians()),
            KICK_ROT_STIFFNESS,
            KICK_ROT_ZETA,
        );
        if eject {
            self.eject_shell(&view);
        }
        punch
    }

    pub fn on_cycled(&mut self, weapon: WeaponId) {
        // The bolt or pump sounds come from the view as it animates. The spent case comes out about a third of the way through a bolt or
        // pump cycle. A self-loading weapon's charge has no case to throw.
        if weapon.def().cycle_time > 0.0 {
            self.pending_shell = Some((weapon, weapon.def().cycle_time * 0.35));
        }
    }

    /// A shell went into the tube (its sound is the clip's `shell_in` beat).
    pub fn on_round_loaded(&mut self, _weapon: WeaponId) {
        self.cycle_kick = 1.0;
    }

    /// Turns the weapon over to look at it (the inspect key). Any action
    /// stops it.
    pub fn start_inspect(&mut self) {
        self.inspect = Some(0.0);
    }

    /// Whether an inspect is playing (tests).
    pub fn inspecting(&self) -> bool {
        self.inspect.is_some()
    }

    pub fn on_picked_up(&mut self, weapon: WeaponId) {
        if let Some(mut view) = self.view(weapon) {
            view.bind_mut().on_picked_up();
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

    /// The clip playing this frame: a reload or an inspect moving the
    /// weapon, its magazines, and the hands; or everything at rest.
    fn clip(&mut self, frame: &ViewFrame) -> ClipPose {
        let def = frame.weapon.def();
        let kind = ClipKind::of(def);
        let Some(view) = self.view(frame.weapon) else {
            return ClipPose::rest();
        };
        let anchors = view.bind().anchors(kind);
        match frame.action {
            ViewAction::Reloading { elapsed, empty } => MagazineReload::new(def, empty)
                .map_or_else(ClipPose::rest, |clip| clip.pose(elapsed, &anchors)),
            ViewAction::Loading {
                stage,
                elapsed,
                length,
            } => {
                // Remember whether the tube was empty when the reload began:
                // then it ends with a pump.
                if !matches!(self.beat_clock, Some((BeatClip::Loading(_), _))) {
                    self.loading_from_empty = frame.empty;
                }
                weapon_clips::shotgun_pose(
                    stage,
                    elapsed,
                    length,
                    self.loading_from_empty,
                    &anchors,
                )
            }
            _ => match self.inspect {
                Some(t) => weapon_clips::inspect_pose(t, kind, &anchors),
                None => ClipPose::rest(),
            },
        }
    }

    /// Plays the beats the clips crossed since the last frame: a foley event
    /// for each (the `foley_event` signal on the view) and its jolt. Also
    /// stops the reload sound when a switch cuts the reload short.
    fn play_beats(&mut self, frame: &ViewFrame, view: &mut Gd<WeaponView>) {
        let def = frame.weapon.def();
        let kind = ClipKind::of(def);
        let (clip, now) = match frame.action {
            ViewAction::Reloading { elapsed, empty } => (BeatClip::Reload { empty }, elapsed),
            ViewAction::Loading {
                stage,
                elapsed,
                length,
            } => (BeatClip::Loading(stage), elapsed / length.max(1e-3)),
            ViewAction::Cycling(p) => (BeatClip::Cycle, p),
            ViewAction::Lowering(p) => (BeatClip::Lower, p),
            ViewAction::Raising(p) => (BeatClip::Raise, p),
            ViewAction::Ready => match self.inspect {
                Some(t) => (BeatClip::Inspect, t),
                None => (BeatClip::Idle, 0.0),
            },
        };
        let since = match self.beat_clock {
            Some((last, t)) if last == clip && t <= now => t,
            _ => -1.0,
        };
        self.beat_clock = Some((clip, now));
        // Each beat's contact (and jolt) is at `at`; its sound starts
        // `weapon_foley::lead` seconds earlier so the hit lands on the contact.
        // `span` converts seconds to this clip's time units.
        let crossed = |at: f32| since < at && at <= now;
        let mut sounds: Vec<&'static str> = Vec::new();
        let mut jolts: Vec<(Vector3, Vector3)> = Vec::new();
        let mut beat = |at: f32, span: f32, name: &'static str, pos: Vector3, rot: Vector3| {
            if crossed(weapon_foley::emit_fraction(name, at, span)) {
                sounds.push(name);
            }
            if crossed(at) && (pos != Vector3::ZERO || rot != Vector3::ZERO) {
                jolts.push((pos, rot));
            }
        };
        match clip {
            BeatClip::Reload { empty } => {
                if let Some(reload) = MagazineReload::new(def, empty) {
                    for b in reload.beats() {
                        beat(b.at, 1.0, b.name, b.jolt_pos, b.jolt_rot);
                    }
                }
            }
            BeatClip::Loading(stage) => {
                let length = match frame.action {
                    ViewAction::Loading { length, .. } => length,
                    _ => 1.0,
                };
                for &(at, name) in weapon_clips::shotgun_beats(stage, self.loading_from_empty) {
                    let jolt = (Vector3::new(0.0, 0.002, 0.0), Vector3::new(-0.6, 0.0, 0.8));
                    beat(at, length, name, jolt.0, jolt.1);
                }
            }
            BeatClip::Cycle => {
                let length = if def.cycle_time > 0.0 {
                    def.cycle_time
                } else {
                    0.35
                };
                for &(at, name) in weapon_clips::cycle_beats(kind) {
                    beat(at, length, name, Vector3::ZERO, Vector3::ZERO);
                }
            }
            BeatClip::Lower if since < 0.0 => sounds.push("holster"),
            BeatClip::Raise if since < 0.0 => sounds.push("draw"),
            BeatClip::Inspect if since < 0.0 => sounds.push("inspect"),
            _ => {}
        }
        if frame.ads > self.last_ads && self.last_ads <= 0.001 {
            sounds.push("ads_in");
        } else if frame.ads < self.last_ads && self.last_ads >= 0.999 {
            sounds.push("ads_out");
        }
        for name in sounds {
            view.bind_mut().foley(name);
        }
        let scale = 1.0 - 0.5 * ads_ease(frame.ads);
        for (pos, rot) in jolts {
            self.kick_pos.velocity += peak_impulse(pos * scale, KICK_POS_STIFFNESS, KICK_POS_ZETA);
            let rot = Vector3::new(rot.x.to_radians(), rot.y.to_radians(), rot.z.to_radians());
            self.kick_rot.velocity += peak_impulse(rot * scale, KICK_ROT_STIFFNESS * 0.5, 0.5);
        }
    }

    pub fn on_dry_fire(&mut self, weapon: WeaponId) {
        if let Some(mut view) = self.view(weapon) {
            view.bind_mut().on_dry_fire();
        }
    }

    /// Called on landing; `impact` is the downward speed in m/s. The weapon
    /// drops (about 3.5 cm after a jump) and its muzzle dips, then springs back.
    pub fn on_landed(&mut self, impact: f32) {
        let dip = (impact * 0.0075).min(0.045);
        let pitch = (impact * 0.8).min(5.0).to_radians();
        self.land.velocity +=
            peak_impulse(Vector3::new(-pitch, -dip, 0.0), LAND_STIFFNESS, LAND_ZETA);
    }

    pub fn update(&mut self, frame: &ViewFrame, dt: f32) {
        self.update_shells(dt);
        // Hidden while dead, and once the scope overlay covers the screen.
        let shown = frame.visible && frame.scope < 0.85;
        self.time += dt;
        // An inspect plays out unless the player does anything else.
        if let Some(t) = self.inspect {
            let busy = frame.action != ViewAction::Ready
                || frame.ads > 0.05
                || frame.sprinting
                || !frame.visible
                || self.kick_pos.value.length() > 0.004;
            let t = t + dt;
            self.inspect = (!busy && t < weapon_clips::INSPECT_LENGTH).then_some(t);
        }
        let clip = self.clip(frame);
        let base = base_weapon(frame.weapon);
        for view in &mut self.views {
            let show = shown && view.bind().weapon_id() == Some(base);
            if show {
                let mut v = view.bind_mut();
                v.set_attachment(frame.weapon.def().attachment);
                v.animate(frame, &clip);
            }
            if view.is_visible() != show {
                view.set_visible(show);
            }
        }
        self.active = Some(frame.weapon);
        let Some(mut view) = self.view(frame.weapon) else {
            return;
        };
        self.play_beats(frame, &mut view);

        let ads = ads_ease(frame.ads);
        let hip_amount = 1.0 - ads;
        // Reaching the sights: a small settle as the weapon stops on the eye.
        if frame.ads >= 0.999 && self.last_ads < 0.999 {
            self.kick_rot.velocity += peak_impulse(
                Vector3::new(-0.4f32.to_radians(), 0.0, 0.0),
                KICK_ROT_STIFFNESS * 0.3,
                0.45,
            );
            self.kick_pos.velocity += peak_impulse(
                Vector3::new(0.0, 0.0, -0.008),
                KICK_POS_STIFFNESS * 0.3,
                0.45,
            );
        }
        // On the way up the weapon swings through a small arc: dipped, rolled
        // in, and pulled toward the eye at the middle. Lowering is a quicker,
        // flatter drop with half the roll.
        if frame.ads > self.last_ads + 1e-4 {
            self.ads_rising = true;
        } else if frame.ads < self.last_ads - 1e-4 {
            self.ads_rising = false;
        }
        self.last_ads = frame.ads;
        let arc = (PI * ads).sin();
        let arc_roll = if self.ads_rising { arc } else { arc * 0.5 };

        // Sprint: blends in as the run starts, and out exactly over the
        // sprint-out, so the weapon is on target when it can fire.
        if frame.sprinting {
            self.sprint_blend += (1.0 - self.sprint_blend) * smoothing(9.0, dt);
        } else if frame.sprint_out > 0.0 {
            // Leaves the sprint pose at once and slows onto the target.
            let out = frame.sprint_out * frame.sprint_out;
            self.sprint_blend = self.sprint_blend.min(out);
        } else {
            self.sprint_blend *= 1.0 - smoothing(14.0, dt);
        }
        let sprint = self.sprint_blend * hip_amount;

        // Stride bob, on the same phase as the camera and the footsteps.
        let grounded = frame.on_floor && !frame.sliding;
        let moving = if grounded {
            Vector2::new(frame.velocity.x, frame.velocity.z).length() / WALK_SPEED
        } else {
            0.0
        };
        self.bob_blend += (moving.min(1.6) - self.bob_blend) * smoothing(8.0, dt);
        if frame.stride_phase + 0.5 < self.last_stride {
            self.foot = 1.0 - self.foot;
        }
        self.last_stride = frame.stride_phase;
        let theta = PI * (frame.stride_phase + self.foot);
        let bob_amount = self.bob_blend.min(1.3) * (1.0 - 0.85 * ads);
        let (bob_pos, bob_rot) = stride_bob(theta, bob_amount, sprint);

        // Idle breathing at the hip: a slow drift, nearly gone aimed (the
        // aimed sway is in the simulation's aim instead).
        let breath_t = self.time * TAU * 0.22;
        let breath = 1.0 - 0.9 * ads;
        let breathe_pos = Vector3::new(
            0.0006 * (breath_t * 0.5).sin(),
            0.0012 * breath_t.sin(),
            0.0,
        ) * breath;
        let breathe_rot = Vector3::new(
            0.12 * (breath_t + 0.6).sin(),
            0.06 * (breath_t * 0.5 + 1.3).sin(),
            0.1 * (breath_t * 0.5).sin(),
        ) * breath;

        // Look sway and movement inertia.
        let look = look_sway_target(frame.look_velocity, ads);
        let follow = smoothing(6.0, dt);
        self.smoothed_velocity += (frame.velocity - self.smoothed_velocity) * follow;
        let change = frame.velocity - self.smoothed_velocity;
        let inertia = 1.0 - 0.7 * ads;
        let floor = if frame.on_floor { 1.0 } else { 0.0 };
        // Aimed, the look lag is mostly this shift (the turn is clamped hard).
        let look_shift = 0.0008 + 0.0004 * ads;
        let sway_pos = Vector3::new(
            -frame.look_velocity.x * look_shift - change.x * 0.003,
            -frame.look_velocity.y * look_shift * 0.6 - frame.velocity.y * 0.004,
            change.z * 0.003,
        );
        let limit = Vector3::new(0.015, 0.025, 0.015) * inertia;
        let sway_pos = sway_pos.clamp(-limit, limit);
        let strafe_roll = -frame.velocity.x * 0.005 * floor * inertia;
        let look = look + Vector3::new(0.0, 0.0, strafe_roll);
        self.sway_rot
            .step_toward(look, SWAY_STIFFNESS, damping_for(SWAY_STIFFNESS, 0.55), dt);
        self.sway_pos.step_toward(
            sway_pos,
            INERTIA_STIFFNESS,
            damping_for(INERTIA_STIFFNESS, 0.6),
            dt,
        );

        self.kick_pos.step(
            KICK_POS_STIFFNESS,
            damping_for(KICK_POS_STIFFNESS, KICK_POS_ZETA),
            dt,
        );
        self.kick_rot.step(
            KICK_ROT_STIFFNESS,
            damping_for(KICK_ROT_STIFFNESS, KICK_ROT_ZETA),
            dt,
        );
        self.land
            .step(LAND_STIFFNESS, damping_for(LAND_STIFFNESS, LAND_ZETA), dt);
        let land = self.land.value * (1.0 - 0.6 * ads);

        // The clip playing (reload, inspect) moves the whole weapon; much
        // less aimed, so the sights stay up through an aimed reload.
        let clip_pose = clip.weapon;
        let clip_scale = if matches!(
            frame.action,
            ViewAction::Reloading { .. } | ViewAction::Loading { .. }
        ) {
            1.0 - 0.6 * ads
        } else {
            1.0
        };

        // Switching: holster the old weapon, draw the new one.
        let kind = ClipKind::of(frame.weapon.def());
        let switch = match frame.action {
            ViewAction::Lowering(p) => weapon_clips::pose_at(weapon_clips::holster_keys(kind), p),
            ViewAction::Raising(p) => weapon_clips::pose_at(weapon_clips::draw_keys(kind), p),
            _ => Pose::ZERO,
        };
        let (switch_rot, switch_pos) = (switch.rot, switch.pos);

        // Bolt or pump: a quick push and roll, peaking mid-cycle, mostly kept
        // off the sight line when aimed. A loaded shell gives a small bump.
        let cycle = match frame.action {
            ViewAction::Cycling(p) => {
                (PI * p).sin().max(0.0) * (1.0 - 0.5 * frame.scope) * (1.0 - 0.75 * ads)
            }
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
        position += Vector3::new(-0.012, -0.018, 0.02) * arc;
        position += sprint_pos * sprint;
        position += bob_pos + breathe_pos + self.sway_pos.value;
        position += self.kick_pos.value;
        position += Vector3::new(0.0, land.y, 0.0);
        if frame.crouched {
            // Crouched, the weapon sits a touch lower and closer.
            position += Vector3::new(0.0, -0.006, 0.006) * hip_amount;
        }
        position += cycle_pos * cycle + Vector3::new(0.0, -0.02 * bump, 0.01 * bump);
        position += clip_pose.pos * clip_scale + switch_pos;
        let ins_rot = clip_pose.rot * clip_scale;
        let deg = |v: f32| v.to_radians();
        let motion = self.sway_rot.value + self.kick_rot.value;
        let pitch = deg(ins_rot.x)
            + deg(sprint_rot.x) * sprint
            + deg(bob_rot.x + breathe_rot.x)
            + motion.x
            + land.x
            + deg(-3.0) * arc
            + deg(switch_rot.x)
            + deg(cycle_rot.x) * cycle
            + deg(4.0) * bump;
        let yaw = deg(ins_rot.y)
            + deg(switch_rot.y)
            + deg(sprint_rot.y) * sprint
            + deg(bob_rot.y + breathe_rot.y)
            + motion.y
            + deg(cycle_rot.y) * cycle;
        let roll = deg(ins_rot.z)
            + deg(sprint_rot.z) * sprint
            + deg(bob_rot.z + breathe_rot.z)
            + motion.z
            + deg(7.0) * arc_roll
            + deg(switch_rot.z)
            + deg(cycle_rot.z) * cycle;

        // Rotate around the pivot (the firing hand) at the hip. Aimed, turn
        // about the rear sight instead, so motion tips the front sight
        // without lifting the sight picture off the eye.
        let pivot = pivot * hip_amount;
        let basis = Basis::from_euler(EulerOrder::YXZ, Vector3::new(pitch, yaw, roll));
        let origin = position + pivot - basis * pivot;
        view.set_transform(Transform3D::new(basis, origin));

        if let Some(arms) = &mut self.arms {
            // The hands follow the grip markers, which the clips move.
            let support = 0.0;
            let hold = shown.then(|| {
                let mut grips = view.bind().grips();
                grips.pistol = frame.weapon.def().slot == Slot::Secondary;
                (view.clone().upcast::<Node3D>(), grips)
            });
            arms.bind_mut().follow(hold, support);
        }
    }
}

/// The impulse per axis that makes a spring at rest peak at `peak`.
fn peak_impulse(peak: Vector3, stiffness: f32, zeta: f32) -> Vector3 {
    let c = damping_for(stiffness, zeta);
    Vector3::new(
        impulse_for_peak(peak.x, stiffness, c),
        impulse_for_peak(peak.y, stiffness, c),
        impulse_for_peak(peak.z, stiffness, c),
    )
}

/// Which clip a beat clock is timing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BeatClip {
    Idle,
    Reload { empty: bool },
    Loading(LoadStage),
    Cycle,
    Lower,
    Raise,
    Inspect,
}

/// Walk speed (m/s) the bob is scaled to; the movement rules' walk speed.
const WALK_SPEED: f32 = 5.0;
/// Look sway follows the turn with a short lag and a small overshoot.
const SWAY_STIFFNESS: f32 = 150.0;
/// Movement inertia: a heavier, slower spring.
const INERTIA_STIFFNESS: f32 = 90.0;
const LAND_STIFFNESS: f32 = 140.0;
const LAND_ZETA: f32 = 0.5;

#[cfg(test)]
mod tests {
    use super::*;

    /// Peak of one shot's kick on the weapon, simulated with the kick springs.
    fn kick_peak(shot: u32, strength: f32, ads: f32) -> (Vector3, Vector3) {
        let (pos, rot) = shot_kick(shot, strength, ads);
        let mut p = Spring {
            value: Vector3::ZERO,
            velocity: peak_impulse(pos, KICK_POS_STIFFNESS, KICK_POS_ZETA),
        };
        let mut r = Spring {
            value: Vector3::ZERO,
            velocity: peak_impulse(rot, KICK_ROT_STIFFNESS, KICK_ROT_ZETA),
        };
        let (mut peak_p, mut peak_r) = (Vector3::ZERO, Vector3::ZERO);
        for _ in 0..60 {
            p.step(
                KICK_POS_STIFFNESS,
                damping_for(KICK_POS_STIFFNESS, KICK_POS_ZETA),
                1.0 / 240.0,
            );
            r.step(
                KICK_ROT_STIFFNESS,
                damping_for(KICK_ROT_STIFFNESS, KICK_ROT_ZETA),
                1.0 / 240.0,
            );
            if p.value.z.abs() > peak_p.z.abs() {
                peak_p = p.value;
            }
            if r.value.x.abs() > peak_r.x.abs() {
                peak_r = r.value;
            }
        }
        (peak_p, peak_r)
    }

    #[test]
    fn a_shot_shoves_the_weapon_back_and_it_returns_fast() {
        // The M4's kick strength.
        let (pos, rot) = kick_peak(7, 1.35, 1.0);
        assert!(
            (0.015..=0.025).contains(&pos.z),
            "aimed push back {}",
            pos.z
        );
        assert!(rot.x.abs() < 0.25, "aimed sights bounce {}", rot.x);
        // The shove peaks within two frames at 60 FPS and is gone in ~120 ms.
        let (p, _) = shot_kick(7, 1.35, 1.0);
        let mut spring = Spring {
            value: Vector3::ZERO,
            velocity: peak_impulse(p, KICK_POS_STIFFNESS, KICK_POS_ZETA),
        };
        let c = damping_for(KICK_POS_STIFFNESS, KICK_POS_ZETA);
        spring.step(KICK_POS_STIFFNESS, c, 2.0 / 60.0);
        assert!(spring.value.z > p.z * 0.6, "{} of {}", spring.value.z, p.z);
        spring.step(KICK_POS_STIFFNESS, c, 0.09);
        assert!(spring.value.z.abs() < p.z * 0.15, "{}", spring.value.z);
    }

    #[test]
    fn aimed_kick_tips_the_front_sight_up_and_is_back_before_the_next_round() {
        // Aimed the front sight always kicks up, a fraction of a degree, and
        // never more than 0.6 degrees even on the heaviest weapons.
        for i in 0..200 {
            let (_, rot) = shot_kick(i, 1.55, 1.0);
            assert!(rot.x > 0.15 && rot.x <= 0.6, "{rot:?}");
        }
        assert!(shot_kick(3, 3.0, 1.0).1.x <= 0.6);
        // Within 0.05 degrees of the sight line by the next round at 750 rpm.
        let (_, rot) = shot_kick(5, 1.35, 1.0);
        let mut spring = Spring {
            value: Vector3::ZERO,
            velocity: peak_impulse(rot, KICK_ROT_STIFFNESS, KICK_ROT_ZETA),
        };
        spring.step(
            KICK_ROT_STIFFNESS,
            damping_for(KICK_ROT_STIFFNESS, KICK_ROT_ZETA),
            0.08,
        );
        assert!(spring.value.x.abs() < 0.05, "{}", spring.value.x);
        // At the hip the muzzle always tips up, and the roll changes side
        // from shot to shot without a fixed alternation.
        let hip: Vec<Vector3> = (0..32).map(|i| shot_kick(i, 1.35, 0.0).1).collect();
        assert!(hip.iter().all(|r| r.x > 0.5));
        let flips = hip
            .windows(2)
            .filter(|w| w[0].z.signum() != w[1].z.signum())
            .count();
        assert!((6..=26).contains(&flips), "{flips} side changes in 31");
    }

    #[test]
    fn the_weapon_lags_a_turn_less_when_aimed() {
        // Turning left at 300 degrees a second.
        let turn = Vector2::new(300f32.to_radians(), 0.0);
        let hip = look_sway_target(turn, 0.0);
        let aimed = look_sway_target(turn, 1.0);
        // It points right of the turn (negative yaw) and cants with it.
        assert!(hip.y < 0.0 && hip.z > 0.0);
        assert!(
            (hip.y.to_degrees() + 2.5).abs() < 1e-3,
            "clamped at 2.5: {}",
            hip.y
        );
        assert!(aimed.y.to_degrees().abs() <= 0.15 + 1e-3);
        assert_eq!(look_sway_target(Vector2::ZERO, 0.0), Vector3::ZERO);
    }

    #[test]
    fn the_stride_bob_dips_on_every_footfall_and_sways_once_per_two_steps() {
        let at = |theta: f32| stride_bob(theta, 1.0, 0.0);
        // Footfalls at 0 and pi: lowest there, highest between.
        assert!(at(0.0).0.y < at(PI * 0.5).0.y);
        assert!((at(0.0).0.y - at(PI).0.y).abs() < 1e-6);
        // Sideways: one way on the first step, the other on the second.
        assert!(at(PI * 0.5).0.x > 0.0 && at(PI * 1.5).0.x < 0.0);
        // Sprinting adds the running loop: about 2.5 cm and 5 degrees.
        let run = stride_bob(PI * 0.5, 1.0, 1.0);
        assert!(run.0.x > 0.025 && run.1.z > 5.0);
    }

    #[test]
    fn aiming_eases_and_arrives_exactly() {
        assert_eq!(ads_ease(0.0), 0.0);
        assert!((ads_ease(1.0) - 1.0).abs() < 1e-6);
        // Leaves the hip quickly, then slows onto the sights.
        assert!(ads_ease(0.5) > 0.6);
        let mut last = 0.0;
        for i in 1..=100 {
            let e = ads_ease(i as f32 / 100.0);
            assert!(e >= last);
            last = e;
        }
    }

    #[test]
    fn the_hollow_ring_covers_the_last_rounds_and_builds_to_the_last() {
        // A 30-round magazine rings on its last five shots.
        assert_eq!(hollow_ring(5, 30), None);
        let first = hollow_ring(4, 30).expect("rings with four left");
        let last = hollow_ring(0, 30).expect("rings on the last round");
        assert_eq!(last.volume_db, 0.0);
        assert!(first.volume_db < last.volume_db);
        assert!(first.pitch < last.pitch);
        // A pistol's 8 rounds ring on the last two; tiny magazines never.
        assert!(hollow_ring(1, 8).is_some());
        assert_eq!(hollow_ring(2, 8), None);
        assert_eq!(hollow_ring(0, 2), None);
    }
}
