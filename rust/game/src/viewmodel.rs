//! First-person weapon presentation: procedural pose, bob, kick, reload,
//! bolt/pump, and switch motion, muzzle flash, ejected shells, and weapon sounds.
//!
//! Nothing here affects gameplay; the simulation in `shooter-sim` decides when
//! shots happen. The rig only turns simulation state into motion, so the same
//! numbers can later drive other players' third-person weapons.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use godot::classes::{
    AudioStreamPlayer, AudioStreamRandomizer, BaseMaterial3D, Curve, CurveTexture, CylinderMesh,
    GeometryInstance3D, GpuParticles3D, Gradient, GradientTexture1D, INode3D, Material, Mesh,
    MeshInstance3D, Node3D, OmniLight3D, ParticleProcessMaterial, QuadMesh, Shader, ShaderMaterial,
};
use godot::global::randf_range;
use godot::prelude::*;
use shooter_sim::hitbox::carry::MagazinePose;
use shooter_sim::rng;
use shooter_sim::weapon::{Attachment, LoadStage, WeaponId};

use godot::classes::light_3d::Param;

use crate::acoustics::{self, AudioEnvironment};
use crate::audio;
use crate::character::arms::FpArms;
use crate::character::poser::Grips;
use crate::character::poser::Held;
use crate::convert::to_godot;
use crate::effects::{self, CombatEffects};
use crate::sights::{self, IronSights};
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
    /// Seconds left climbing over a ledge (0 when not mantling), and whether
    /// the climb is a vault over cover.
    pub mantle_left: f32,
    pub vaulting: bool,
    /// On a ladder: the hands are on the rungs, so the weapon is put away.
    pub on_ladder: bool,
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
    /// A first raise (`Raise::First`): `elapsed` seconds into one of
    /// `length`. A draw over the weapon's `raise_time`, then the action is
    /// worked.
    FirstRaise {
        elapsed: f32,
        length: f32,
    },
    /// Working the bolt or pump.
    Cycling(f32),
    /// A knife strike; seconds until it lands (negative after).
    Meleeing {
        strike_in: f32,
    },
    /// A grenade throw with the support hand, 0..1
    /// (`equipment::throw_progress`), and what it throws; the weapon dips in
    /// the firing hand.
    Throwing(f32, shooter_sim::equipment::Kind),
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
fn shot_kick(shot: u32, strength: f32, ads: f32, feel: &KickFeel) -> (Vector3, Vector3) {
    let hip_pos = Vector3::new(
        0.0,
        0.002 * strength,
        (0.013 * strength * feel.hip_back).min(0.06),
    );
    // Aimed the gun shoves back into the eye and rises a few millimetres.
    let aim_pos = Vector3::new(0.0, 0.004 * strength, (0.017 * strength).min(0.05));
    let lift = 0.75 + 0.25 * rng::unit(shot, 31);
    let hip_rot = Vector3::new(
        (feel.hip_lift * strength * lift).min(9.0),
        0.4 * strength * rng::signed(shot, 32),
        1.0 * strength * rng::signed(shot, 33),
    );
    let aim_rot = Vector3::new(
        (feel.aim_lift * strength * lift).min(AIM_LIFT_MAX),
        feel.aim_yaw * strength * rng::signed(shot, 35),
        feel.aim_roll * strength * rng::signed(shot, 36),
    );
    (hip_pos.lerp(aim_pos, ads), hip_rot.lerp(aim_rot, ads))
}

/// Added to every weapon's `hip_position`: closer and further right.
const HIP_CLOSER: Vector3 = Vector3::new(0.012, -0.01, 0.05);
/// Added to a long gun's hip pose on top of `HIP_CLOSER` (metres, then
/// degrees): raised and pushed out, and turned a little more across the body,
/// so the rear sight, the receiver, and the firing hand sit in the lower
/// right of the frame instead of below it (only the handguard showed).
const HIP_LONG_POS: Vector3 = Vector3::new(0.0, 0.04, -0.03);
const HIP_LONG_ROT: Vector3 = Vector3::new(0.0, 5.0, 3.0);
/// Added to a long gun's sprint pose: muzzle down and across the body, a
/// run rather than a high ready (degrees, then metres).
const SPRINT_LONG_ROT: Vector3 = Vector3::new(-10.0, -8.0, 4.0);
const SPRINT_LONG_POS: Vector3 = Vector3::new(0.0, 0.015, -0.04);

/// How fast a cut-short clip eases out, in 1/seconds.
const CLIP_FADE_RATE: f32 = 30.0;

/// An ACOG is built this much larger than the sight's own size, so its
/// eyepiece fills about half the screen's height when aimed.
const ACOG_SIZE: f32 = 1.2;
/// Aimed, how far the eye sits behind an ACOG's eyepiece, in metres: close
/// enough that the tube frames a large sight picture, far enough that an
/// aimed shot's shove (`ACOG_AIM_SHOVE`) never brings it inside the near
/// plane (0.03 m).
const ACOG_EYE_RELIEF: f32 = 0.045;
/// Share of an aimed shot's shove toward the eye kept with an ACOG.
const ACOG_AIM_SHOVE: f32 = 0.35;
/// How much higher than a red dot an ACOG's mount stands, in metres.
const ACOG_RISER: f32 = 0.012;
/// From this aimed amount, the ACOG's body ahead of the eyepiece is hidden:
/// it is behind the eyepiece's rim or inside its picture.
const ACOG_BODY_HIDDEN_AT: f32 = 0.9;

/// How far behind the rear sight the aimed weapon turns, in metres.
const AIM_PIVOT_BACK: f32 = 0.12;

/// The most an aimed shot tips the front sight, in degrees.
const AIM_LIFT_MAX: f32 = 1.8;

/// How one weapon's kick looks, on top of its `kick_strength`: the hip lift
/// and shove, the aimed lift, yaw, and roll (degrees per unit of strength),
/// and how bouncy the rotation spring is (`KICK_ROT_STIFFNESS`'s zeta).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KickFeel {
    pub hip_lift: f32,
    pub hip_back: f32,
    pub aim_lift: f32,
    pub aim_yaw: f32,
    pub aim_roll: f32,
    pub zeta: f32,
}

impl KickFeel {
    /// Each weapon's character: the M4 is crisp and straight, the AK rough
    /// and wandering, the LSW-6 heavy and bouncing, the PDW-9 a fast buzz, the
    /// BR-3 tight, the pump and bolt guns one big buck, and the pistol a flip.
    pub fn of(weapon: WeaponId) -> Self {
        let rifle = Self {
            hip_lift: 1.2,
            hip_back: 1.0,
            aim_lift: 0.42,
            aim_yaw: 0.1,
            aim_roll: 0.7,
            zeta: KICK_ROT_ZETA,
        };
        match base_weapon(weapon).def().key.as_str() {
            "ak47" => Self {
                aim_lift: 0.5,
                aim_yaw: 0.22,
                aim_roll: 0.9,
                zeta: 0.5,
                ..rifle
            },
            "lsw6" => Self {
                hip_back: 1.2,
                aim_lift: 0.45,
                aim_yaw: 0.25,
                aim_roll: 1.0,
                zeta: 0.55,
                ..rifle
            },
            "pdw9" => Self {
                aim_lift: 0.42,
                aim_yaw: 0.18,
                aim_roll: 0.5,
                zeta: 0.8,
                ..rifle
            },
            "br3" => Self {
                aim_lift: 0.4,
                aim_yaw: 0.08,
                aim_roll: 0.6,
                ..rifle
            },
            "m870" | "m40" => Self {
                hip_lift: 2.6,
                hip_back: 1.5,
                aim_lift: 0.58,
                aim_yaw: 0.1,
                aim_roll: 0.8,
                zeta: 0.6,
            },
            "m1911" => Self {
                hip_lift: 2.4,
                aim_lift: 0.85,
                aim_yaw: 0.12,
                aim_roll: 0.8,
                zeta: 0.6,
                ..rifle
            },
            _ => rifle,
        }
    }
}

/// A share of each kick that builds up under sustained fire and settles
/// about 200 ms after the trigger is let go (`ride` springs).
const RIDE_SHARE: f32 = 0.2;
const RIDE_STIFFNESS: f32 = 180.0;
const RIDE_ZETA: f32 = 0.9;
/// Clip beats (a magazine seating, a bolt release) and the settle on the
/// sights: a softer spring, so each reads as a distinct thump.
const JOLT_STIFFNESS: f32 = 400.0;
const JOLT_ZETA: f32 = 0.45;

/// Kick springs: stiff, so the shove peaks within about two frames and is
/// back in about 100 ms, and slightly underdamped for a small rebound.
const KICK_POS_STIFFNESS: f32 = 1800.0;
const KICK_POS_ZETA: f32 = 0.62;
const KICK_ROT_STIFFNESS: f32 = 2600.0;
const KICK_ROT_ZETA: f32 = 0.7;

/// The aim-down-sights blend as the weapon moves: eased in and out, so it
/// leaves the hip quickly and slows onto the sights. The world zoom uses it
/// too, so the two arrive together.
pub fn ads_ease(ads: f32) -> f32 {
    shooter_sim::weapon::ads_ease(ads)
}

/// How far the weapon turns against the player's own turning (radians:
/// pitch, yaw, roll) at `look_velocity`, before the spring: it lags the view,
/// and cants into a turn. Much smaller aimed, so the sights stay usable.
fn look_sway_target(look_velocity: Vector2, ads: f32) -> Vector3 {
    let lag = 0.010 + (0.0025 - 0.010) * ads;
    // Aimed the weapon barely turns, so the sights stay on the bullet path;
    // its lag shows as a small shift instead (see `ViewmodelRig::update`).
    // The limits are soft, so a faster flick still swings further.
    let limit = (4.0 + (0.15 - 4.0) * ads).to_radians();
    let roll_limit = (3.5 + (0.4 - 3.5) * ads).to_radians();
    let soft = |v: f32, limit: f32| limit * (v / limit).tanh();
    Vector3::new(
        soft(-look_velocity.y * lag, limit),
        soft(-look_velocity.x * lag, limit),
        soft(look_velocity.x * lag, roll_limit),
    )
}

/// The bob over two steps (`theta` 0 to 2 pi; a footstep lands at 0 and at
/// pi), as (position, rotation in degrees). A figure-8: one sideways sway per
/// two steps, a dip on every footfall. `sprint` adds the running loop.
fn stride_bob(theta: f32, amount: f32, sprint: f32) -> (Vector3, Vector3) {
    let (side, dip) = (theta.sin(), -(2.0 * theta).cos());
    let walk_pos = Vector3::new(0.008 * side, 0.011 * dip, 0.0);
    let walk_rot = Vector3::new(0.9 * dip, 0.4 * side, 0.6 * side);
    let run_pos = Vector3::new(0.022 * side, 0.012 * dip, 0.004 * dip);
    let run_rot = Vector3::new(2.0 * dip, 3.0 * side, 5.0 * side);
    (
        (walk_pos + run_pos * sprint) * amount,
        (walk_rot + run_rot * sprint) * amount,
    )
}

/// How much longer an extended magazine is than the standard one.
const EXTENDED_MAGAZINE: f32 = 1.35;

/// The length of a magazine's top, where it meets the well.
const MAG_TOP_LENGTH: f32 = 0.06;

/// The top of a magazine's meshes in its own space (where it meets the well).
fn magazine_top(mag: &Gd<Node3D>) -> Option<f32> {
    let mut top: Option<f32> = None;
    let mut visit = vec![(mag.clone().upcast::<Node>(), Transform3D::IDENTITY)];
    while let Some((node, xf)) = visit.pop() {
        if let Ok(mesh) = node.clone().try_cast::<MeshInstance3D>() {
            let aabb = mesh.get_aabb();
            for i in 0..8 {
                let y = (xf * aabb.get_corner(i)).y;
                top = Some(top.map_or(y, |t: f32| t.max(y)));
            }
        }
        for child in node.get_children().iter_shared() {
            if let Ok(c) = child.clone().try_cast::<Node3D>() {
                visit.push((child, xf * c.get_transform()));
            }
        }
    }
    top
}

/// How far below the support hand's grip marker a foregrip's clamp sits: on
/// the bottom rail of the handguard.
const GRIP_MOUNT_DROP: f32 = 0.006;
/// Height an optic needs between its mount and its sight line.
const OPTIC_CLEARANCE: f32 = 0.018;
/// How far a red dot's window sits in front of the eye when aimed, so it
/// spans about an eighth of the screen's width, as the built-in reflex
/// sights of the PDW-9 and BR-3 already do. A red dot slides along the rail from its
/// mount to sit there (`reflex_seat`), as shooters mount one further back on
/// a long rail, so the weapon's aimed place is unchanged.
const REFLEX_EYE_DISTANCE: f32 = 0.1;
/// A red dot stands on a riser this tall, a lower-third co-witness as on
/// modern carbines: the front sight and the support hand sit lower in its
/// window, below the dot rather than at the centre.
const REFLEX_RISER: f32 = 0.012;
/// How far back along the rail a red dot may slide from its mount, and
/// forward.
const REFLEX_RAIL_BACK: f32 = 0.09;
const REFLEX_RAIL_FORWARD: f32 = 0.02;

/// Where a red dot sits along the rail (its rear, in the weapon's space) on
/// a weapon whose optic mount is at `mount_z` and which is aimed with its
/// origin `ads_z` ahead of the eye: its window `REFLEX_EYE_DISTANCE` from
/// the eye, within reach of the mount.
fn reflex_seat(mount_z: f32, ads_z: f32) -> f32 {
    let window = -REFLEX_EYE_DISTANCE - ads_z;
    (window + sights::REFLEX_WINDOW_BEHIND)
        .clamp(mount_z - REFLEX_RAIL_FORWARD, mount_z + REFLEX_RAIL_BACK)
}

/// A side plume's length as a multiple of its weapon's flash size, in
/// first person and in the world, and its height over its length (the
/// atlas's side frames are drawn for it: tools/build_muzzle_flash.py).
const PLUME_LENGTH: f32 = 1.6;
const PLUME_LENGTH_WORLD: f32 = 2.5;
const PLUME_ASPECT: f32 = 0.5;

/// The flash atlas frame for shot `count`: scrambled, so consecutive shots
/// rarely repeat a frame (the shader takes it modulo 4).
fn flash_seed(count: u32) -> f32 {
    (count.wrapping_mul(2_654_435_761) >> 16) as f32 % 97.0
}

/// Seconds an ejected shell stays visible in the view, where there is no
/// world to hand it to (`CombatEffects`).
const SHELL_LIFETIME: f32 = 0.5;
/// Seconds a case flies in the view before it is handed to the world.
const SHELL_HANDOFF: f32 = 0.09;
/// Rounds in quick succession that heat the barrel enough to smoke once the
/// firing stops, how fast it cools (rounds a second), and how long the wisp
/// rises off it (seconds).
const BARREL_HOT_ROUNDS: f32 = 6.0;
const BARREL_COOLING: f32 = 2.0;
const WISP_TIME: f32 = 3.6;
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
    /// How the weapon turns at the hip, in degrees (pitch up, yaw left,
    /// roll), so its side and the firing hand show rather than only its top.
    /// Fades out as it comes up to aim.
    #[export]
    hip_rotation: Vector3,
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
    /// The fired case this weapon ejects: a node of
    /// `assets/weapons/shared/casings.glb` (`Case556`, `Shell12`, ...). Empty
    /// falls back to a plain cylinder in `shell_color` and `shell_size`.
    #[export]
    shell_model: GString,
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
    shell_mesh: Option<Gd<Mesh>>,
    /// The shell in the support hand while a shotgun loads.
    shell_in_hand: Option<Gd<MeshInstance3D>>,
    /// Third person: the magazine a reload moves (`WorldMagazine`), found
    /// on the first reload, and whether `WorldBody` and it stand in for
    /// `World` now.
    world_magazine: Option<Part>,
    world_split: bool,

    flash_time: f32,
    flash_count: u32,
    /// The last shot was fired aimed.
    flash_aimed: bool,
    /// The flash light's energy as authored (read on the first shot).
    flash_energy: f32,
    /// The flash seen from the side: two crossed quads along the barrel.
    plume: Vec<Gd<MeshInstance3D>>,
    /// Two smoke emitters, restarted in turn so one puff can linger while the
    /// next one starts.
    smoke: Vec<Gd<GpuParticles3D>>,
    smoke_next: usize,
    smoke_cooldown: f32,
    /// A thin wisp off the barrel after sustained fire, and how hot the
    /// barrel is (recent rounds, cooling at `BARREL_COOLING` a second).
    wisp: Option<Gd<GpuParticles3D>>,
    heat: f32,
    /// Seconds the wisp has left to follow the barrel.
    wisp_left: f32,
    /// The attachment fitted (see `set_attachment`), and one asked for before
    /// the view was ready.
    attachment: Option<Attachment>,
    pending_attachment: Option<Attachment>,
    /// Where the left hand held the weapon before a foregrip moved it.
    grip_left_rest: Option<Vector3>,
    /// The back of a fitted ACOG's eyepiece along the sight line (view
    /// space): aimed, the eye sits `ACOG_EYE_RELIEF` behind it.
    eyepiece_z: Option<f32>,
    /// A fitted ACOG's lens: (depth along the sight line past which the
    /// weapon is cut away inside it, its radius), view space.
    lens: Option<(f32, f32)>,
    /// How far the weapon hangs below the sight line because an optic sits
    /// above its own sights (see `set_sight_drop`).
    sight_drop: f32,
    suppressed: bool,
    /// The model's moving parts (see `tools/build_weapons.py`), found when ready.
    parts: Option<MovingParts>,
    /// Seconds since the last shot: the slide's and bolt's kick, the trigger.
    #[init(val = 10.0)]
    since_shot: f32,
    /// Seconds since the dust cover flew open (`None` while closed).
    cover_open: Option<f32>,
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
    /// The magazine's front lip at the well (view space), which an
    /// AK-pattern magazine rocks about.
    mag_lip: Option<Vector3>,
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
        self.pose_about(offset, Vector3::BACK, turn, pivot);
    }

    /// Moves the part by `offset` and turns it by `turn` radians about `axis`
    /// (view space) through `pivot`.
    fn pose_about(&mut self, offset: Vector3, axis: Vector3, turn: f32, pivot: Vector3) {
        let rest = self.to_view * self.rest;
        let spin = Basis::from_axis_angle(axis, turn);
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
    /// The bolt carrier seen through the ejection port: it runs back and
    /// forward with each shot and locks open on an AR when empty.
    carrier: Option<Part>,
    /// The charging handle: drawn back to charge, and on a weapon whose handle
    /// rides the carrier (no separate `BoltCarrier`), with each shot.
    handle: Option<Part>,
    /// An AR's ejection port cover, which flies open on the first shot or
    /// charge and stays open.
    dust_cover: Option<Part>,
    trigger: Option<Part>,
    /// A pistol's hammer, which falls on each shot and is cocked by the slide.
    hammer: Option<Part>,
    /// A pistol's slide stop, which rises while the slide is locked back.
    slide_stop: Option<Part>,
    /// The magazine release, and the way it is pressed (view space).
    mag_release: Option<(Part, Vector3)>,
}

/// How far a bolt carrier (or an AK's handle on it) runs back with a shot, in
/// metres.
const CARRIER_TRAVEL: f32 = 0.05;
/// The longest a shot's carrier stroke takes, back and home, in seconds; a
/// faster-firing weapon's stroke fits inside its cycle.
const CARRIER_STROKE: f32 = 0.065;
/// How far a trigger turns about its pin when pulled, in degrees.
const TRIGGER_PULL: f32 = 14.0;
/// How far a pistol's hammer falls forward on a shot, in degrees.
const HAMMER_FALL: f32 = 55.0;
/// How far a slide stop's lever rises to hold the slide, in degrees.
const SLIDE_STOP_LIFT: f32 = 6.0;
/// How far a magazine release is pressed in, in metres.
const MAG_RELEASE_PRESS: f32 = 0.0025;
/// How far a dust cover swings open, in degrees.
const DUST_COVER_OPEN: f32 = 110.0;
/// The dust cover's swing over the seconds after it is knocked open: past
/// open, a bounce, and still.
const DUST_COVER_SWING: [(f32, f32); 4] = [(0.0, 0.0), (0.04, 1.1), (0.08, 0.95), (0.12, 1.0)];

/// A shot's stroke of the bolt carrier, 0..1 of its travel, `since` seconds
/// after the shot of a weapon cycling every `cycle` seconds: back fast, home
/// a little slower.
fn carrier_stroke(since: f32, cycle: f32) -> f32 {
    let stroke = (cycle * 0.8).min(CARRIER_STROKE);
    let s = since / stroke.max(1e-3);
    if s < 0.4 {
        s / 0.4
    } else if s < 1.0 {
        1.0 - (s - 0.4) / 0.6
    } else {
        0.0
    }
}

/// How far the trigger is pulled, 0..1, `since` seconds after a shot of a
/// weapon cycling every `cycle` seconds: held through automatic fire, then
/// let out to its reset (a slow weapon's within a moment of the shot).
fn trigger_pull(since: f32, cycle: f32) -> f32 {
    let hold = cycle.clamp(0.06, 0.15) + 0.02;
    1.0 - ((since - hold) / 0.05).clamp(0.0, 1.0)
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
        let shader = load::<Shader>(VIEWMODEL_SHADER);
        convert_to_viewmodel_materials(&self.model.clone().upcast(), &shader, self.albedo_boost);
        if let Some(sights) = self.base().try_get_node_as::<Node>("Sights") {
            convert_to_viewmodel_materials(&sights, &shader, 1.0);
        }
        self.ejection = self.base().try_get_node_as("Ejection");
        if self.id.is_none() {
            godot_error!("WeaponView: unknown weapon `{}`", self.weapon);
        }
        self.shell_mesh = casing_mesh(&self.shell_model.to_string(), &shader)
            .or_else(|| Some(shell_mesh(&shader, self.shell_color, self.shell_size).upcast()));
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
        crate::effects::keep_decals_off(&self.base().clone().upcast());
    }

    fn process(&mut self, delta: f64) {
        self.since_shot += delta as f32;
        if let Some(t) = &mut self.cover_open {
            *t += delta as f32;
        }
        self.smoke_cooldown -= delta as f32;
        self.update_wisp(delta as f32);
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

    /// Fits the attachment with this key (`reddot`, `acog`, `suppressor`,
    /// `grip`, `extmag`), or takes it off for an empty key. For tests and
    /// captures; the game picks attachments through the weapon's variant.
    #[func]
    fn apply_attachment(&mut self, key: GString) {
        let attachment = Attachment::from_key(&key.to_string());
        self.set_attachment(attachment);
    }
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
            // Found again after an optic lowers the weapon: reuse the copy.
            let mut parent = mag.node.get_parent()?;
            if !parent.has_node("FreshMagazine") {
                let mut copy = mag.node.duplicate_node();
                copy.set_name("FreshMagazine");
                copy.set_visible(false);
                parent.add_child(&copy);
            }
            Part::find(&view, "FreshMagazine")
        });
        self.charging_handle = self
            .base()
            .try_get_node_as::<Node3D>("ChargingHandle")
            .map(|m| m.get_position());
        let mag_center = magazine
            .as_ref()
            .and_then(Part::bounds_in_view)
            .map(|(min, max)| (min + max) * 0.5);
        // Where the magazine's front lip meets the well: an AK-pattern
        // magazine rocks about it. A curved magazine's or a drum's bounds
        // reach forward below the well, so the lip is taken a magazine top's
        // length ahead of the rear of its top.
        self.mag_lip = magazine
            .as_ref()
            .and_then(Part::bounds_in_view)
            .map(|(_, max)| Vector3::new(0.0, max.y, max.z - MAG_TOP_LENGTH));
        // A release on the side of the frame (a pistol's button) is pressed
        // across it; a paddle behind the magazine is pushed toward it.
        let mag_release = Part::find(&view, "MagRelease").and_then(|part| {
            let (min, max) = part.bounds_in_view()?;
            let at = (min + max) * 0.5;
            let press = if at.x.abs() > 0.006 {
                Vector3::new(-at.x.signum(), 0.0, 0.0)
            } else {
                let to = mag_center.map_or(-1.0, |m| (m.z - at.z).signum());
                Vector3::new(0.0, 0.0, to)
            };
            Some((part, press))
        });
        self.parts = Some(MovingParts {
            magazine,
            fresh_magazine,
            pump: Part::find(&view, "Pump"),
            bolt,
            slide: Part::find(&view, "Slide"),
            carrier: Part::find(&view, "BoltCarrier"),
            handle: Part::find(&view, "ChargingHandle"),
            dust_cover: Part::find(&view, "DustCover"),
            trigger: Part::find(&view, "Trigger"),
            hammer: Part::find(&view, "Hammer"),
            slide_stop: Part::find(&view, "SlideStop"),
            mag_release,
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
            let lip = self.mag_lip.unwrap_or_default();
            mag.pose_about(
                clip.old_mag.unwrap_or_default(),
                Vector3::RIGHT,
                clip.old_mag_rock,
                lip,
            );
        }
        if let Some(fresh) = &mut parts.fresh_magazine {
            let shown = clip.fresh_mag.is_some();
            if fresh.node.is_visible() != shown {
                fresh.node.set_visible(shown);
            }
            let lip = self.mag_lip.unwrap_or_default();
            fresh.pose_about(
                clip.fresh_mag.unwrap_or_default(),
                Vector3::RIGHT,
                clip.fresh_mag_rock,
                lip,
            );
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
        self.animate_action(&mut parts, frame, clip);
        if self.eyepiece_z.is_some()
            && let Some(mut body) = self
                .base()
                .try_get_node_as::<Node3D>(&format!("Attachment/Optic/{}", sights::SIGHT_BODY))
        {
            let shown = frame.ads < ACOG_BODY_HIDDEN_AT;
            if body.is_visible() != shown {
                body.set_visible(shown);
            }
        }
        self.show_shell_in_hand(clip.shell);
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

    /// The action's small parts: the carrier and charging handle with each
    /// shot and charge, an AR's bolt locked open on empty and its dust cover,
    /// the trigger, a pistol's hammer and slide stop, and the magazine
    /// release.
    fn animate_action(&mut self, parts: &mut MovingParts, frame: &ViewFrame, clip: &ClipPose) {
        let def = frame.weapon.def();
        let kind = ClipKind::of(def);
        let cycle = 60.0 / def.rounds_per_minute.max(1.0);
        // Pump and bolt guns are worked by hand; only self-loaders cycle.
        let self_loading = matches!(kind, ClipKind::Rifle { .. } | ClipKind::Pistol);
        let stroke = if self_loading {
            carrier_stroke(self.since_shot, cycle)
        } else {
            0.0
        };
        let reloading = matches!(frame.action, ViewAction::Reloading { .. });
        // An AR's bolt locks open on the last round.
        let locked =
            matches!(kind, ClipKind::Rifle { charge: false, .. }) && frame.empty && !reloading;
        let held = clip.carrier.unwrap_or(if locked { 1.0 } else { 0.0 });
        let charged = clip.handle / weapon_clips::HANDLE_PULL;
        let has_carrier = parts.carrier.is_some();
        if let Some(carrier) = &mut parts.carrier {
            let back = held.max(stroke).max(charged).min(1.0);
            carrier.pose(
                Vector3::new(0.0, 0.0, back * CARRIER_TRAVEL),
                0.0,
                Vector3::ZERO,
            );
        }
        if let Some(handle) = &mut parts.handle {
            // A handle on the carrier (an AK's) rides every shot; a separate
            // one only moves when it is pulled.
            let rides = if has_carrier {
                0.0
            } else {
                stroke * CARRIER_TRAVEL
            };
            handle.pose(
                Vector3::new(0.0, 0.0, clip.handle.max(rides)),
                0.0,
                Vector3::ZERO,
            );
        }
        if let Some(cover) = &mut parts.dust_cover {
            if self.cover_open.is_none() && (stroke > 0.0 || held > 0.0 || charged > 0.0) {
                self.cover_open = Some(0.0);
            }
            let open = self.cover_open.map_or(0.0, |t| keyed(&DUST_COVER_SWING, t));
            if let Some((min, max)) = cover.bounds_in_view() {
                // Hinged along its lower edge, it swings out to the right.
                let hinge = Vector3::new(max.x - 0.003, min.y + 0.002, 0.0);
                cover.pose(Vector3::ZERO, -(DUST_COVER_OPEN * open).to_radians(), hinge);
            }
        }
        if let Some(trigger) = &mut parts.trigger
            && let Some((min, max)) = trigger.bounds_in_view()
        {
            // Turned about its pin at the top: the blade swings back.
            let pin = Vector3::new((min.x + max.x) * 0.5, max.y, (min.z + max.z) * 0.5);
            let pull = trigger_pull(self.since_shot, cycle);
            trigger.pose_about(
                Vector3::ZERO,
                Vector3::RIGHT,
                -(TRIGGER_PULL * pull).to_radians(),
                pin,
            );
        }
        if let Some(hammer) = &mut parts.hammer {
            // Falls on the shot; the slide running back cocks it again.
            let fall = if frame.empty {
                0.0
            } else {
                keyed(&[(0.0, 1.0), (0.012, 1.0), (0.035, 0.0)], self.since_shot)
            };
            let pin = hammer.rest_in_view();
            hammer.pose_about(
                Vector3::ZERO,
                Vector3::RIGHT,
                -(HAMMER_FALL * fall).to_radians(),
                pin,
            );
        }
        if let Some(stop) = &mut parts.slide_stop {
            let up = clip
                .slide
                .unwrap_or(if frame.empty && !reloading { 1.0 } else { 0.0 });
            let pin = stop.rest_in_view();
            stop.pose_about(
                Vector3::ZERO,
                Vector3::RIGHT,
                -(SLIDE_STOP_LIFT * up).to_radians(),
                pin,
            );
        }
        if let Some((release, press)) = &mut parts.mag_release {
            release.pose(
                *press * (MAG_RELEASE_PRESS * clip.mag_release),
                0.0,
                Vector3::ZERO,
            );
        }
    }

    /// A shell held in the support hand while loading (`ClipPose::shell`),
    /// lying along the bore the way it goes into the port.
    fn show_shell_in_hand(&mut self, at: Option<Vector3>) {
        if self.shell_in_hand.is_none() {
            let Some(mesh) = at.and(self.shell_mesh.clone()) else {
                return;
            };
            let mut node = MeshInstance3D::new_alloc();
            node.set_name("ShellInHand");
            node.set_mesh(&mesh);
            node.set_cast_shadows_setting(
                godot::classes::geometry_instance_3d::ShadowCastingSetting::OFF,
            );
            // Crimp first (the case model stands on +Y, mouth up).
            node.set_rotation(Vector3::new(-FRAC_PI_2, 0.0, 0.0));
            self.base_mut().add_child(&node);
            self.shell_in_hand = Some(node);
        }
        if let Some(node) = &mut self.shell_in_hand {
            node.set_visible(at.is_some());
            if let Some(at) = at {
                node.set_position(at);
            }
        }
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
        self.eyepiece_z = None;
        self.lens = None;
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
        // An optic on a weapon with a mount (and no built-in iron sight
        // node) sits on the mount, high enough to clear it: the weapon drops
        // that far below the sight line, so the optic is what lines up.
        let mount = self
            .base()
            .try_get_node_as::<Node3D>("OpticMount")
            .map(|m| m.get_position() + Vector3::new(0.0, self.sight_drop, 0.0));
        // A prism sight stands on a taller mount than a red dot, as on the
        // real rifle, so the front sight sits below its chevron.
        let riser = match attachment {
            Some(Attachment::Acog) => ACOG_RISER,
            Some(Attachment::RedDot) => REFLEX_RISER,
            _ => 0.0,
        };
        let drop = match (optic, mount) {
            (true, Some(m)) => (m.y + OPTIC_CLEARANCE + riser).max(0.0),
            _ => 0.0,
        };
        self.set_sight_drop(drop);
        // The rail shows under an optic, and always under a built-in one.
        let built_in = self.base().has_node("Sights");
        self.show_part("OpticRail", optic || built_in);
        let mut iron = self.base().try_get_node_as::<IronSights>("Sights");
        if let Some(iron) = &mut iron {
            iron.set_visible(!optic);
        }
        // A flip-up rear sight folds forward, flat on the rail, under an optic.
        if let Some(mut leaf) = self
            .base()
            .try_get_node_as::<Node3D>("Model/Mesh/RearSight")
        {
            let fold = if optic { -FRAC_PI_2 } else { 0.0 };
            leaf.set_rotation(Vector3::new(fold, 0.0, 0.0));
        }
        self.lengthen_magazine(attachment == Some(Attachment::ExtendedMag));
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
                // Built in metres with its rear face on the bore at the
                // origin (tools/build_weapons.py): it threads on at the muzzle.
                if let Some(mut mesh) =
                    instance_model("res://assets/weapons/suppressor/suppressor.glb")
                {
                    mesh.set_position(muzzle);
                    parts.add_child(&mesh);
                }
            }
            Some(Attachment::Grip) => {
                if let Some(mut marker) = self.base().try_get_node_as::<Node3D>("GripLeft") {
                    let rest = marker.get_position();
                    // Its clamp's top (the model's origin) just under the
                    // handguard.
                    if let Some(mut mesh) =
                        instance_model("res://assets/weapons/foregrip/foregrip.glb")
                    {
                        mesh.set_position(rest - Vector3::new(0.0, GRIP_MOUNT_DROP, 0.0));
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
                    (None, Some(m)) => {
                        let z = if acog {
                            m.z + 0.017
                        } else {
                            reflex_seat(m.z, self.ads_position.z)
                        };
                        (z, m.y - drop, 1.0)
                    }
                    (None, None) => (-0.09, -0.022, 1.0),
                };
                let size = if acog { size * ACOG_SIZE } else { size };
                self.eyepiece_z = acog.then_some(z + sights::ACOG_EYEPIECE * size);
                // Cut away just past the reticle.
                self.lens = acog.then_some((
                    z + (sights::ACOG_RETICLE - 0.003) * size,
                    sights::ACOG_BORE * size,
                ));
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
            let shader = load::<Shader>(VIEWMODEL_SHADER);
            convert_to_viewmodel_materials(&parts.clone().upcast(), &shader, self.albedo_boost);
            self.set_lens_clip(self.eyepiece_z.is_some());
        }
        crate::effects::keep_decals_off(&parts.upcast());
    }

    /// Draws this first-person weapon with the shader that cuts a magnified
    /// optic's lens out of it (`lens_clipped_shader`), or with the plain one.
    fn set_lens_clip(&mut self, on: bool) {
        let plain = load::<Shader>(VIEWMODEL_SHADER);
        let clipped = lens_clipped_shader(VIEWMODEL_SHADER);
        let (from, to) = if on {
            (plain, clipped)
        } else {
            (clipped, plain)
        };
        swap_shader(&self.to_gd().upcast(), &from, &to);
    }

    /// An extended magazine: the magazine (and the fresh one a reload brings
    /// up) stretched along its length about its top, so it still seats in
    /// the well. Undone when the attachment changes.
    fn lengthen_magazine(&mut self, extended: bool) {
        let stretch = if extended { EXTENDED_MAGAZINE } else { 1.0 };
        let mut changed = false;
        for name in ["Model/Mesh/Magazine", "Model/Mesh/FreshMagazine"] {
            let Some(mut mag) = self.base().try_get_node_as::<Node3D>(name) else {
                continue;
            };
            let Some(top) = magazine_top(&mag) else {
                continue;
            };
            let old = mag.get_scale().y;
            if (old - stretch).abs() < 1e-4 {
                continue;
            }
            // Keep the top where it is: the origin moves by the change in
            // the top's offset.
            let at = mag.get_position();
            mag.set_position(at + Vector3::new(0.0, top * (old - stretch), 0.0));
            mag.set_scale(Vector3::new(1.0, stretch, 1.0));
            changed = true;
        }
        if changed && self.parts.is_some() {
            self.find_parts();
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
            "ChargingHandle",
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

    /// Models from tools/build_weapons.py carry `World`, the weapon merged
    /// into one mesh (nothing moves on a third-person weapon). Third person
    /// draws that; first person draws the separate parts.
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
                // Third-person reloads swap these in (`pose_world_magazine`).
                "WorldBody" | "WorldMagazine" => part.set_visible(false),
                _ => part.set_visible(!world),
            }
        }
    }

    /// Third person: moves the magazine as a reload takes it out and seats a
    /// fresh one (`carry::Carry::magazine_pose`; `None` when not
    /// reloading). While it moves, `WorldBody` (the gun without it) and
    /// `WorldMagazine` stand in for `World`, so a gun is one draw call the
    /// rest of the time. Models without them keep the magazine in place.
    pub fn pose_world_magazine(&mut self, pose: Option<MagazinePose>) {
        if !self.world_model {
            return;
        }
        let split = !matches!(pose, None | Some(MagazinePose::Seated));
        if split && self.world_magazine.is_none() {
            let view = self.to_gd().upcast::<Node3D>();
            self.world_magazine = Part::find(&view, "WorldMagazine");
        }
        if self.world_magazine.is_none() {
            return;
        }
        if split != self.world_split {
            self.world_split = split;
            self.show_part("World", !split);
            self.show_part("WorldBody", split);
        }
        let Some(magazine) = &mut self.world_magazine else {
            return;
        };
        match pose {
            Some(MagazinePose::At(offset)) => {
                magazine.node.set_visible(true);
                magazine.pose_about(to_godot(offset), Vector3::RIGHT, 0.0, Vector3::ZERO);
            }
            _ => magazine.node.set_visible(false),
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

    /// Where the grip markers rest (firing hand, support hand), before any
    /// clip or pump stroke moves them.
    pub fn grip_rests(&self) -> (Option<Vector3>, Option<Vector3>) {
        (self.right_rest, self.hand_rest)
    }

    /// Where the hands hold this weapon, in its own space.
    pub fn grips(&self) -> Grips {
        // On the bolt the hand turns palm-down over the handle.
        let rake = self.grip_rake + BOLT_HAND_RAKE * self.bolt_hand;
        let mut grips = crate::character::rig::grips_of(&self.to_gd().upcast(), rake);
        grips.rests = [self.right_rest, self.hand_rest];
        grips
    }

    /// Third-person setup: brightened standard materials (the low-poly pack is
    /// dark), a flash that faces the camera, a shorter flash light, and no
    /// gunshot layers.
    fn ready_world(&mut self) {
        self.show_world_parts(true);
        if let Some(mut world) = self
            .base()
            .try_get_node_as::<GeometryInstance3D>("Model/Mesh/World")
        {
            world.set_lod_bias(WORLD_LOD_BIAS);
        }
        // A boost of 1 keeps the shared materials, so racks and other
        // players' copies of a gun draw with one material, not one each.
        if (self.albedo_boost - 1.0).abs() > f32::EPSILON {
            brighten_materials(&self.model.clone().upcast(), self.albedo_boost.sqrt());
        }
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
        // Each class's plume is as long as its flash is big (the view
        // scene's `Quad_flash`): a pistol's short, a shotgun's long.
        let size = self
            .flash
            .get_mesh()
            .and_then(|m| m.try_cast::<QuadMesh>().ok())
            .map_or(0.2, |q| q.get_size().x);
        let length = size
            * if viewmodel {
                PLUME_LENGTH
            } else {
                PLUME_LENGTH_WORLD
            };
        if let Some(material) = self.flash.get_active_material(0)
            && let Ok(mut side) = material.duplicate_resource().try_cast::<ShaderMaterial>()
        {
            side.set_shader_parameter("shape", &1.to_variant());
            side.set_shader_parameter("viewmodel", &viewmodel.to_variant());
            let mut quad = QuadMesh::new_gd();
            quad.set_size(Vector2::new(length, length * PLUME_ASPECT));
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
            let mut smoke = smoke_emitter(&shader, viewmodel);
            // The first-person weapon's puffs are placed in the world where
            // the muzzle shows on screen (`place_in_world`), not under it.
            smoke.set_as_top_level(viewmodel);
            muzzle.add_child(&smoke);
            self.smoke.push(smoke);
        }
        let mut wisp = wisp_emitter(&shader);
        wisp.set_as_top_level(true);
        wisp.set_visible(false);
        muzzle.add_child(&wisp);
        self.wisp = Some(wisp);
    }

    /// Flash, plume, and maybe a puff of smoke, for one shot.
    fn show_flash(&mut self) {
        self.flash_count = self.flash_count.wrapping_add(1);
        let seed = flash_seed(self.flash_count);
        let aimed = if self.flash_aimed { 1.0f32 } else { 0.0 };
        self.flash
            .set_instance_shader_parameter("seed", &seed.to_variant());
        self.flash
            .set_instance_shader_parameter("aimed", &aimed.to_variant());
        self.flash.set_visible(true);
        self.flash_light.set_visible(true);
        for plume in &mut self.plume {
            plume.set_instance_shader_parameter("seed", &seed.to_variant());
            plume.set_instance_shader_parameter("aimed", &aimed.to_variant());
            plume.set_visible(true);
        }
        // About three frames at 60 FPS: visible on every shot, yet off again
        // between rounds at 750 RPM (80 ms), so automatic fire flickers.
        self.flash_time = 0.05;
        self.heat += 1.0;
        if self.smoke_cooldown <= 0.0 && !self.smoke.is_empty() {
            let index = self.smoke_next % self.smoke.len();
            if !self.world_model {
                let at = self.muzzle_in_world();
                self.smoke[index].set_global_transform(at);
            }
            self.smoke[index].restart();
            self.smoke_next = index + 1;
            self.smoke_cooldown = 0.14;
        }
    }

    /// A third-person weapon's fired case, thrown out of its ejection port
    /// to the right and a little up and back, if the camera is close enough
    /// to see it.
    pub fn eject_world_case(&mut self, effects: &mut Gd<CombatEffects>, seed: u32) {
        let model = self.shell_model.to_string();
        if model.is_empty() {
            return;
        }
        let Some(port) = self.base().try_get_node_as::<Node3D>("Ejection") else {
            return;
        };
        let at = port.get_global_position();
        let near = self
            .base()
            .get_viewport()
            .and_then(|v| v.get_camera_3d())
            .is_some_and(|c| c.get_global_position().distance_to(at) < effects::REMOTE_CASE_RANGE);
        if !near {
            return;
        }
        let basis = self.base().get_global_basis().orthonormalized();
        let velocity = basis
            * Vector3::new(
                2.2 + rng::unit(seed, 31) * 0.8,
                0.8 + rng::unit(seed, 32) * 0.6,
                0.3 + rng::signed(seed, 33) * 0.2,
            );
        let spin = Vector3::new(
            rng::signed(seed, 34) * 25.0,
            rng::signed(seed, 35) * 10.0,
            18.0 + rng::unit(seed, 36) * 12.0,
        );
        effects
            .bind_mut()
            .throw_case(&model, at, velocity, basis, spin);
    }

    /// A muzzle flash with no sound, for third-person weapons. Its light
    /// shows only when the camera can see the muzzle, so a shooter behind a
    /// wall does not light the near side of it.
    pub fn flash_world(&mut self) {
        let roll = (self.flash_count as f32 * 2.399_963) % (2.0 * PI);
        self.flash.set_rotation(Vector3::new(0.0, 0.0, roll));
        self.show_flash();
        let seen = self.muzzle_seen();
        self.flash_light.set_visible(seen);
    }

    /// Whether the camera has a clear line to this weapon's muzzle.
    fn muzzle_seen(&self) -> bool {
        let muzzle = self.flash.get_global_position();
        let Some(camera) = self.base().get_viewport().and_then(|v| v.get_camera_3d()) else {
            return true;
        };
        let Some(mut space) = self
            .base()
            .get_world_3d()
            .and_then(|w| w.get_direct_space_state())
        else {
            return true;
        };
        let eye = camera.get_global_position();
        let Some(query) = godot::classes::PhysicsRayQueryParameters3D::create_ex(eye, muzzle)
            .collision_mask(crate::target::WORLD_LAYER)
            .done()
        else {
            return true;
        };
        space.intersect_ray(&query).is_empty()
    }

    /// The first-person muzzle's transform in the world as the player sees
    /// it (`effects::viewmodel_to_world`), for smoke that stays in the world.
    fn muzzle_in_world(&self) -> Transform3D {
        let muzzle = self.flash.get_global_transform();
        let Some(camera) = self.base().get_viewport().and_then(|v| v.get_camera_3d()) else {
            return muzzle;
        };
        let (origin, _) = effects::viewmodel_to_world(
            camera.get_global_transform(),
            camera.get_fov(),
            muzzle.origin,
            Vector3::ZERO,
        );
        Transform3D::new(muzzle.basis.orthonormalized(), origin)
    }

    /// After a sustained burst, a thin wisp of smoke curls off the barrel
    /// for a couple of seconds and follows it.
    fn update_wisp(&mut self, dt: f32) {
        self.heat = (self.heat - BARREL_COOLING * dt).max(0.0);
        if self.heat >= BARREL_HOT_ROUNDS && self.since_shot > 0.2 {
            self.heat = 0.0;
            self.wisp_left = WISP_TIME;
            if let Some(wisp) = &mut self.wisp {
                wisp.set_visible(true);
                wisp.restart();
            }
        }
        if self.wisp_left <= 0.0 {
            return;
        }
        self.wisp_left -= dt;
        if self.wisp_left <= 0.0 {
            // Its last puffs have faded: draw nothing until the next one.
            if let Some(wisp) = &mut self.wisp {
                wisp.set_visible(false);
            }
            return;
        }
        let at = if self.world_model {
            self.flash.get_global_transform()
        } else {
            self.muzzle_in_world()
        };
        if let Some(wisp) = &mut self.wisp {
            wisp.set_global_transform(at);
            if self.since_shot < 0.05 {
                // Firing again blows it away.
                wisp.set_emitting(false);
                wisp.set_visible(false);
                self.wisp_left = 0.0;
            }
        }
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
        // Aimed the flash still blooms round the front sight.
        let scale = (0.85 + 0.3 * ((next * 7 % 10) as f32 / 10.0)) * (1.0 - 0.25 * ads);
        self.flash.set_rotation(Vector3::new(0.0, 0.0, roll));
        self.flash.set_scale(Vector3::splat(scale));
        if self.flash_energy == 0.0 {
            self.flash_energy = self.flash_light.get_param(Param::ENERGY);
        }
        self.flash_light
            .set_param(Param::ENERGY, self.flash_energy * (1.0 - 0.4 * ads));
        // Aimed, the flash draws from the atlas's smaller, tighter frames,
        // except a shotgun's, whose blast stays big round the bead.
        let shotgun = self
            .weapon_id()
            .is_some_and(|id| ClipKind::of(id.def()) == ClipKind::Shotgun);
        self.flash_aimed = ads > 0.5 && !shotgun;
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
        // Your own shot is the loudest thing there is: duck what matters less.
        acoustics::request_duck(1.0);
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
    /// The case's node in `casings.glb`, for the world's copy of it.
    model: String,
    position: Vector3,
    velocity: Vector3,
    spin: Vector3,
    age: f32,
    clinked: bool,
}

const CASINGS: &str = "res://assets/weapons/shared/casings.glb";

/// A modelled fired case from `CASINGS`, drawn with the viewmodel shader.
/// Every weapon that ejects the same case shares one mesh.
fn casing_mesh(name: &str, shader: &Gd<Shader>) -> Option<Gd<Mesh>> {
    if name.is_empty() {
        return None;
    }
    if let Some(mesh) = CASES.with(|c| c.borrow().get(name).cloned()) {
        return Some(mesh);
    }
    let root = try_load::<PackedScene>(CASINGS).ok()?.instantiate()?;
    let node = root.find_child(name)?.try_cast::<MeshInstance3D>().ok();
    let source = node.as_ref().and_then(|n| n.get_mesh());
    root.free();
    let mut mesh = source?.duplicate_resource().try_cast::<Mesh>().ok()?;
    for surface in 0..mesh.get_surface_count() {
        if let Some(material) = mesh.surface_get_material(surface) {
            mesh.surface_set_material(surface, &viewmodel_material(&material, shader, 1.0));
        }
    }
    CASES.with(|c| c.borrow_mut().insert(name.to_string(), mesh.clone()));
    Some(mesh)
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
/// Every surface under `root` drawn with the shader `from` switches to `to`.
fn swap_shader(root: &Gd<Node>, from: &Gd<Shader>, to: &Gd<Shader>) {
    for child in root.get_children().iter_shared() {
        if let Ok(mesh) = child.clone().try_cast::<MeshInstance3D>() {
            for surface in 0..mesh.get_surface_override_material_count() {
                let Some(material) = mesh.get_surface_override_material(surface) else {
                    continue;
                };
                if let Ok(mut material) = material.try_cast::<ShaderMaterial>()
                    && material.get_shader().as_ref() == Some(from)
                {
                    material.set_shader(to);
                }
            }
        }
        swap_shader(&child, from, to);
    }
}

pub(crate) fn convert_to_viewmodel_materials(root: &Gd<Node>, shader: &Gd<Shader>, boost: f32) {
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

    // A random angle per puff, which also picks its flipbook frame.
    process.set_param_min(Parameter::ANGLE, -180.0);
    process.set_param_max(Parameter::ANGLE, 180.0);

    let material = smoke_material(shader);
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

/// The muzzle smoke's material: the lit smoke flipbook.
fn smoke_material(shader: &Gd<Shader>) -> Gd<ShaderMaterial> {
    let mut material = ShaderMaterial::new_gd();
    material.set_shader(shader);
    if let Ok(texture) =
        try_load::<godot::classes::Texture2D>("res://assets/textures/effects/smoke_4x4.png")
    {
        material.set_shader_parameter("flipbook", &texture.to_variant());
    }
    material
}

/// A thin wisp off a hot barrel: a few small, faint puffs that leave slowly
/// over a couple of seconds, rise, and curl as they grow.
fn wisp_emitter(shader: &Gd<Shader>) -> Gd<GpuParticles3D> {
    use godot::classes::particle_process_material::Parameter;
    let mut process = ParticleProcessMaterial::new_gd();
    process.set_direction(Vector3::new(0.0, 1.0, -0.3));
    process.set_spread(12.0);
    process.set_param_min(Parameter::INITIAL_LINEAR_VELOCITY, 0.08);
    process.set_param_max(Parameter::INITIAL_LINEAR_VELOCITY, 0.2);
    process.set_gravity(Vector3::new(0.03, 0.18, 0.0));
    process.set_param_min(Parameter::ANGLE, -180.0);
    process.set_param_max(Parameter::ANGLE, 180.0);
    process.set_param_min(Parameter::SCALE, 0.6);
    process.set_param_max(Parameter::SCALE, 1.0);
    let mut growth = Curve::new_gd();
    growth.add_point(Vector2::new(0.0, 0.3));
    growth.add_point(Vector2::new(1.0, 1.0));
    let mut growth_texture = CurveTexture::new_gd();
    growth_texture.set_curve(&growth);
    process.set_param_texture(Parameter::SCALE, &growth_texture);
    let mut fade = Gradient::new_gd();
    fade.set_color(0, Color::from_rgba(1.0, 1.0, 1.0, 0.0));
    fade.add_point(0.2, Color::from_rgba(1.0, 1.0, 1.0, 0.9));
    let last = fade.get_point_count() - 1;
    fade.set_color(last, Color::from_rgba(1.0, 1.0, 1.0, 0.0));
    let mut fade_texture = GradientTexture1D::new_gd();
    fade_texture.set_gradient(&fade);
    process.set_color_ramp(&fade_texture);

    let mut quad = QuadMesh::new_gd();
    quad.set_size(Vector2::new(0.12, 0.12));
    quad.set_material(&smoke_material(shader));

    let mut particles = GpuParticles3D::new_alloc();
    particles.set_amount(8);
    particles.set_lifetime(1.8);
    particles.set_one_shot(true);
    particles.set_explosiveness_ratio(0.0);
    particles.set_use_local_coordinates(false);
    particles.set_process_material(&process);
    particles.set_draw_pass_mesh(0, &quad);
    particles
        .set_cast_shadows_setting(godot::classes::geometry_instance_3d::ShadowCastingSetting::OFF);
    particles.set_visibility_aabb(Aabb::new(Vector3::splat(-3.0), Vector3::splat(6.0)));
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

/// Third-person weapons switch to their coarser LODs this much sooner than
/// other meshes: a gun is small next to the player holding it.
const WORLD_LOD_BIAS: f32 = 0.35;

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
    /// First-person copies of shaders (`shader_variant`), by the path of the
    /// shader they copy and what was added.
    static VARIANT_SHADERS: std::cell::RefCell<std::collections::HashMap<String, Gd<Shader>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    /// Fired-case meshes by name, shared by every weapon that ejects one.
    static CASES: std::cell::RefCell<std::collections::HashMap<String, Gd<Mesh>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    static WEAPON_SCENES: std::cell::RefCell<std::collections::HashMap<String, Gd<PackedScene>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

const WEAPON_SCENE_DIR: &str = "res://scenes/weapons";

/// The first-person weapon's shader.
const VIEWMODEL_SHADER: &str = "res://shaders/viewmodel.gdshader";

/// A copy of the first-person shader at `path` whose fragments inside a
/// magnified optic's lens, past its reticle, are cut away
/// (`viewmodel_behind_lens` in `shaders/viewmodel_projection.gdshaderinc`),
/// so through the eyepiece the eye sees only the zoomed world. Only the
/// local player's weapon and arms use it, and only while an ACOG is fitted:
/// `discard` would cost every other draw its cheap depth prepass.
pub(crate) fn lens_clipped_shader(path: &str) -> Gd<Shader> {
    shader_variant(path, false, true)
}

/// The shader the local player's first-person arms draw with: a copy of the
/// character shader at `path` compiled with `VIEWMODEL_KEY`, so the gloves
/// and sleeves take the same first-person key light as the weapon
/// (`shaders/viewmodel_key.gdshaderinc`), and with the optic's lens cut out
/// of it while `clipped`. Characters in the world keep the plain shader and
/// pay nothing for either.
pub(crate) fn first_person_shader(path: &str, clipped: bool) -> Gd<Shader> {
    shader_variant(path, true, clipped)
}

/// The shader at `path`, copied with the first-person key compiled in
/// (`keyed`) and the optic's lens cut out (`clipped`).
fn shader_variant(path: &str, keyed: bool, clipped: bool) -> Gd<Shader> {
    if !keyed && !clipped {
        return load::<Shader>(path);
    }
    let key = format!("{path}|{keyed}|{clipped}");
    if let Some(shader) = VARIANT_SHADERS.with_borrow(|cache| cache.get(&key).cloned()) {
        return shader;
    }
    let source = load::<Shader>(path);
    let shader = match variant_code(&source.get_code().to_string(), keyed, clipped) {
        Some(code) => {
            let mut shader = Shader::new_gd();
            shader.set_code(&code);
            shader
        }
        None => {
            godot_warn!("{path}: no shader_type or fragment() to build a first-person copy from");
            source
        }
    };
    VARIANT_SHADERS.with_borrow_mut(|cache| cache.insert(key, shader.clone()));
    shader
}

/// The code of a first-person copy of a shader (see `shader_variant`), or
/// `None` if the code has nowhere to put it.
fn variant_code(code: &str, keyed: bool, clipped: bool) -> Option<String> {
    let mut code = code.to_owned();
    if keyed {
        let anchor = "shader_type spatial;";
        if !code.contains(anchor) {
            return None;
        }
        code = code.replacen(anchor, "shader_type spatial;\n#define VIEWMODEL_KEY", 1);
    }
    if clipped {
        let anchor = "void fragment() {";
        if !code.contains(anchor) {
            return None;
        }
        code = code.replacen(
            anchor,
            "void fragment() {\n\tif (viewmodel_behind_lens(VERTEX)) {\n\t\tdiscard;\n\t}",
            1,
        );
    }
    Some(code)
}

pub fn clear_cache() {
    VARIANT_SHADERS.with_borrow_mut(|cache| cache.clear());
    WEAPON_SCENES.with_borrow_mut(|cache| cache.clear());
    CASES.with_borrow_mut(|cache| cache.clear());
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
    /// The `viewmodel_scope_lens` shader global last set.
    scope_lens: Vector4,
    /// Whether the arms draw with the lens cut out (an ACOG is fitted).
    lens_clip: bool,
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
    /// A knife strike: seconds since it began, and when it lands.
    melee: Option<(f32, f32)>,
    knife_swing: Option<Gd<AudioStreamPlayer>>,
    knife_hit: Option<Gd<AudioStreamPlayer>>,
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
    /// The clip offset last frame (position, rotation in degrees), eased out
    /// when a clip is cut short.
    clip_carry: (Vector3, Vector3),
    /// The point the clip turns the weapon about (`ClipPose::focus`).
    clip_focus: Option<Vector3>,
    /// The current weapon's kick rotation zeta (`KickFeel::zeta`).
    #[init(val = KICK_ROT_ZETA)]
    kick_zeta: f32,
    /// The build-up under sustained fire (see `RIDE_SHARE`).
    ride_pos: Spring,
    ride_rot: Spring,
    /// Clip beats and the settle on the sights (see `JOLT_STIFFNESS`).
    jolt_pos: Spring,
    jolt_rot: Spring,
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
    /// The slide pose's blend (`x`), sprung so it lands with a small
    /// overshoot, and whether the last frame was sliding.
    slide: Spring<f32>,
    was_sliding: bool,
    /// A mantle or vault: its length in seconds (when it started), and the
    /// pose's blend, which hands over to the sprint-out like the sprint pose.
    mantle_total: f32,
    mantle_blend: f32,
    /// How far the weapon is put away for a ladder; it comes back up over
    /// the sprint-out once off it.
    ladder_blend: f32,
    /// Crouched, eased in and out rather than snapped (Black Ops 2's
    /// `posMoveRate`).
    crouch_blend: f32,
    /// Walking speed's share of the walk pose (`STAND_MOVE`), eased.
    walk_blend: f32,
    /// A hit's knock on the weapon: seconds since it, the hit's direction in
    /// the view (`x` right, `y` ahead), and its strength.
    damage: Option<(f32, Vector2, f32)>,
    /// The first raise's work (seconds), while one plays: the bolt, pump,
    /// charging handle, or slide beats are timed over it.
    first_work: Option<f32>,
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
        self.knife_swing = self.make_player("res://assets/audio/foley/knife_swing", -4.0);
        self.knife_hit = self.make_player("res://assets/audio/foley/knife_hit", -1.0);
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
        let (strength, punch, eyepiece) = {
            let v = view.bind();
            (v.kick_strength, v.camera_punch, v.eyepiece_z.is_some())
        };
        view.bind_mut()
            .on_fired(rounds_left, weapon.def().magazine, self.last_ads);
        self.shots = self.shots.wrapping_add(1);
        let feel = KickFeel::of(weapon);
        self.kick_zeta = feel.zeta;
        let (mut pos, rot) = shot_kick(self.shots, strength, ads_ease(self.last_ads), &feel);
        if eyepiece {
            // Aimed, a scope's eyepiece is close to the eye: a smaller shove.
            pos.z *= 1.0 - (1.0 - ACOG_AIM_SHOVE) * ads_ease(self.last_ads);
        }
        let rot = Vector3::new(rot.x.to_radians(), rot.y.to_radians(), rot.z.to_radians());
        self.kick_pos.velocity += peak_impulse(pos, KICK_POS_STIFFNESS, KICK_POS_ZETA);
        self.kick_rot.velocity += peak_impulse(rot, KICK_ROT_STIFFNESS, feel.zeta);
        // The ride: a little of every kick that a long spray stacks up. Only
        // the push back and the hip's lift: aimed it would lift the sights.
        let ride_rot = Vector3::new(rot.x * (1.0 - ads_ease(self.last_ads)), 0.0, 0.0);
        self.ride_pos.velocity += peak_impulse(
            Vector3::new(0.0, 0.0, pos.z) * RIDE_SHARE,
            RIDE_STIFFNESS,
            RIDE_ZETA,
        );
        self.ride_rot.velocity += peak_impulse(ride_rot * RIDE_SHARE, RIDE_STIFFNESS, RIDE_ZETA);
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
    /// A knife strike starts; it lands in `strike_in` seconds. `lunge` makes
    /// it a harder, louder swing.
    pub fn on_melee(&mut self, lunge: bool, strike_in: f32) {
        self.inspect = None;
        self.melee = Some((0.0, strike_in.max(0.05)));
        if let Some(player) = &mut self.knife_swing {
            player.set_volume_db(if lunge { -1.0 } else { -4.0 });
            player.play();
        }
    }

    /// The knife strike landed: on a body (`flesh`), on a wall, or on air.
    pub fn on_melee_struck(&mut self, hit: bool, flesh: bool) {
        if flesh && let Some(player) = &mut self.knife_hit {
            player.play();
        }
        if hit {
            // The blade stops dead: a jolt back through the hands.
            self.kick_pos.velocity += peak_impulse(
                Vector3::new(0.0, 0.004, 0.014),
                KICK_POS_STIFFNESS,
                KICK_POS_ZETA,
            );
            self.kick_rot.velocity += peak_impulse(
                Vector3::new(1.2f32.to_radians(), 0.0, -1.5f32.to_radians()),
                KICK_ROT_STIFFNESS,
                KICK_ROT_ZETA,
            );
        }
    }

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
        let (start, mesh, model) = {
            let v = view.bind();
            (
                v.ejection_point(),
                v.shell_mesh.clone(),
                v.shell_model.to_string(),
            )
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
                model: String::new(),
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

        // Cases fly in the world, not with the view: thrown out to the
        // right, a little up and forward, so they clear the sight picture
        // within a few frames instead of arcing up across it.
        let rig = self.base().get_global_transform();
        let shell = &mut self.shells[index];
        shell.mesh.set_mesh(&mesh);
        shell.mesh.set_visible(true);
        shell.position = rig * start;
        shell.velocity = rig.basis
            * Vector3::new(
                2.6 + rng::unit(seed, 1) * 0.6,
                0.35 + rng::unit(seed, 2) * 0.4,
                -0.05 + rng::signed(seed, 3) * 0.15,
            );
        shell.spin = Vector3::new(
            rng::signed(seed, 4) * 25.0,
            rng::signed(seed, 5) * 10.0,
            18.0 + rng::unit(seed, 6) * 12.0,
        );
        shell.age = 0.0;
        shell.clinked = false;
        shell.model = model;
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
        let gravity = Vector3::new(0.0, -9.8, 0.0);
        let mut clink = false;
        // Cases leave the view for the world once clear of the weapon, where
        // they bounce, land, and stay (`CombatEffects::throw_case`).
        let tree = self.base().get_tree();
        let mut effects = CombatEffects::find(&tree);
        let camera = self.base().get_viewport().and_then(|v| v.get_camera_3d());
        let world = effects.is_some() && camera.is_some();
        let mut handed = Vec::new();
        for shell in &mut self.shells {
            if shell.age >= SHELL_LIFETIME {
                continue;
            }
            shell.age += dt;
            shell.velocity += gravity * dt;
            shell.position += shell.velocity * dt;
            let rotation = shell.spin * shell.age;
            let basis = Basis::from_euler(EulerOrder::YXZ, rotation);
            if world && shell.age >= SHELL_HANDOFF && !shell.model.is_empty() {
                handed.push((
                    shell.model.clone(),
                    shell.position,
                    shell.velocity,
                    basis,
                    shell.spin,
                ));
                shell.age = SHELL_LIFETIME;
                shell.mesh.set_visible(false);
                continue;
            }
            shell
                .mesh
                .set_global_transform(Transform3D::new(basis, shell.position));
            if !shell.clinked && shell.age > 0.45 {
                shell.clinked = true;
                clink = true;
            }
            if shell.age >= SHELL_LIFETIME {
                shell.mesh.set_visible(false);
            }
        }
        if let (Some(effects), Some(camera)) = (effects.as_mut(), camera) {
            let eye = camera.get_global_transform();
            for (model, position, velocity, basis, spin) in handed {
                let (position, velocity) =
                    effects::viewmodel_to_world(eye, camera.get_fov(), position, velocity);
                let mut effects = effects.bind_mut();
                let position = effects.clear_of_walls(eye.origin, position);
                // The world's case clinks for itself.
                effects.throw_case(&model, position, velocity, basis, spin);
            }
        }
        if clink && let Some(sound) = &mut self.shell_sound {
            sound.play();
        }
    }

    /// Tells the viewmodel shaders where a fitted ACOG's lens is while the
    /// player is fully aimed through it, so the weapon and hands inside it
    /// are cut away (`viewmodel_scope_lens` in
    /// `shaders/viewmodel_projection.gdshaderinc`).
    fn update_scope_lens(&mut self, view: &Gd<WeaponView>, frame: &ViewFrame, shown: bool) {
        let lens = {
            let v = view.bind();
            v.eyepiece_z.zip(v.lens)
        };
        if lens.is_some() != self.lens_clip {
            self.lens_clip = lens.is_some();
            crate::character::palette::clip_first_person_lens(self.lens_clip);
        }
        let value = match lens {
            Some((eyepiece, (cut, radius))) if shown && frame.ads >= ACOG_BODY_HIDDEN_AT => {
                let to_eye = self.base().get_transform() * view.get_transform();
                let centre = to_eye * Vector3::new(0.0, 0.0, eyepiece);
                let depth = (-centre.z).max(1e-3);
                let cut = -(to_eye * Vector3::new(0.0, 0.0, cut)).z;
                Vector4::new(centre.x / depth, centre.y / depth, cut, radius / depth)
            }
            _ => Vector4::ZERO,
        };
        if value != self.scope_lens {
            self.scope_lens = value;
            godot::classes::RenderingServer::singleton()
                .global_shader_parameter_set("viewmodel_scope_lens", &value.to_variant());
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
            // A self-loader charged after its empty reload was cut off.
            ViewAction::Cycling(p) if matches!(kind, ClipKind::Rifle { .. }) => {
                weapon_clips::charge_pose(p, &anchors)
            }
            // A pistol only cycles by hand in a first raise: the slide racked.
            ViewAction::Cycling(p) if kind == ClipKind::Pistol => {
                weapon_clips::slide_rack_pose(p, &anchors)
            }
            _ => match self.inspect {
                Some(t) => weapon_clips::inspect_pose(t, kind, &anchors),
                None => ClipPose::rest(),
            },
        }
    }

    /// A thump on the jolt springs: peaks in metres and degrees.
    fn jolt(&mut self, pos: Vector3, rot_degrees: Vector3) {
        let rot = Vector3::new(
            rot_degrees.x.to_radians(),
            rot_degrees.y.to_radians(),
            rot_degrees.z.to_radians(),
        );
        self.jolt_pos.velocity += peak_impulse(pos, JOLT_STIFFNESS, JOLT_ZETA);
        self.jolt_rot.velocity += peak_impulse(rot, JOLT_STIFFNESS, JOLT_ZETA);
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
            ViewAction::Meleeing { .. }
            | ViewAction::FirstRaise { .. }
            | ViewAction::Throwing(..) => (BeatClip::Idle, 0.0),
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
                let length = if let Some(work) = self.first_work {
                    work
                } else if def.cycle_time > 0.0 {
                    def.cycle_time
                } else {
                    0.35
                };
                for b in weapon_clips::cycle_beats(kind) {
                    beat(b.at, length, b.name, b.jolt_pos, b.jolt_rot);
                }
            }
            BeatClip::Lower if since < 0.0 => sounds.push("holster"),
            BeatClip::Raise if since < 0.0 => sounds.push("draw"),
            BeatClip::Inspect => {
                if since < 0.0 {
                    sounds.push("inspect");
                }
                for (at, pos, rot) in weapon_clips::inspect_jolts(kind) {
                    if crossed(at) {
                        jolts.push((pos, rot));
                    }
                }
            }
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
            self.jolt(pos * scale, rot * scale);
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

    /// A hit knocked the player: the weapon deflects away from it and comes
    /// back (Black Ops 2's damage kick: out over 100 ms, back over 400 ms),
    /// mostly at the hip. `from` is the attacker's direction in the view
    /// (`x` right, `y` ahead), `strength` about 1 for a rifle round.
    pub fn on_damaged(&mut self, from: Vector2, strength: f32) {
        self.damage = Some((0.0, from, strength.clamp(0.0, 1.5)));
    }

    /// The hit's knock now (rotation in degrees: pitch, yaw, roll).
    fn damage_kick(&mut self, dt: f32) -> Vector3 {
        let Some((t, from, strength)) = self.damage else {
            return Vector3::ZERO;
        };
        let t = t + dt;
        let envelope = if t < DAMAGE_DEFLECT {
            smoothstep(t / DAMAGE_DEFLECT)
        } else {
            1.0 - smoothstep((t - DAMAGE_DEFLECT) / DAMAGE_RETURN)
        };
        self.damage = (t < DAMAGE_DEFLECT + DAMAGE_RETURN).then_some((t, from, strength));
        // From ahead the muzzle is knocked up; from the side, turned and
        // rolled away (Black Ops 2: half the pitch, the roll as yaw and half
        // as roll).
        let pitch = DAMAGE_KICK * from.y.clamp(-0.6, 1.0) * 0.5;
        let roll = DAMAGE_KICK * from.x;
        Vector3::new(pitch, -roll, roll * 0.5) * strength * envelope
    }

    /// A first raise as the clips play it: the draw over the weapon's
    /// `raise_time`, then the action worked (a `Cycling` of the rest), with
    /// the work's progress and length.
    fn resolve_first_raise(frame: &ViewFrame) -> (ViewFrame, Option<(f32, f32)>) {
        let ViewAction::FirstRaise { elapsed, length } = frame.action else {
            return (*frame, None);
        };
        let draw = frame.weapon.def().raise_time.min(length);
        let work = (length - draw).max(1e-3);
        let mut out = *frame;
        if elapsed < draw {
            out.action = ViewAction::Raising(elapsed / draw.max(1e-3));
            (out, Some((0.0, work)))
        } else {
            let q = ((elapsed - draw) / work).clamp(0.0, 1.0);
            out.action = ViewAction::Cycling(q);
            (out, Some((q, work)))
        }
    }

    pub fn update(&mut self, frame: &ViewFrame, dt: f32) {
        let (resolved, first_raise) = Self::resolve_first_raise(frame);
        let frame = &resolved;
        self.first_work = first_raise.map(|(_, work)| work);
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
            let length = weapon_clips::inspect_length(ClipKind::of(frame.weapon.def()));
            self.inspect = (!busy && t < length).then_some(t);
        }
        // A strike plays out while the simulation is in it.
        if let Some((t, strike)) = self.melee {
            let t = t + dt;
            let meleeing = matches!(frame.action, ViewAction::Meleeing { .. });
            let over = t > strike + shooter_sim::melee::RECOVERY + 0.05;
            self.melee = (frame.visible && !over && (meleeing || t < 0.05)).then_some((t, strike));
        }
        let melee_pose = self.melee.map_or(Pose::ZERO, |(t, strike)| {
            weapon_clips::melee_pose(t, strike)
        });
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
            // Only along the sight line: a pitch here would put the sights
            // off the aim point just when the player fires.
            self.jolt(Vector3::new(0.0, 0.0, -0.008), Vector3::ZERO);
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

        // A mantle or vault: the weapon tucks down and right as the support
        // hand leaves it to plant on the ledge, and comes back up over the
        // sprint-out, as the sprint pose does (Black Ops 2's `vMantleOfs`
        // lerps with the climb; Modern Warfare's support hand plants).
        let mantle_progress = if frame.mantle_left > 0.0 {
            if self.mantle_total <= 0.0 {
                self.mantle_total = frame.mantle_left;
            }
            self.mantle_blend += (1.0 - self.mantle_blend) * smoothing(16.0, dt);
            Some(1.0 - frame.mantle_left / self.mantle_total.max(1e-3))
        } else {
            self.mantle_total = 0.0;
            if frame.sprint_out > 0.0 {
                self.mantle_blend = self.mantle_blend.min(frame.sprint_out * frame.sprint_out);
            } else {
                self.mantle_blend *= 1.0 - smoothing(12.0, dt);
            }
            None
        };
        let mantle = self.mantle_blend * hip_amount;

        // A ladder: the hands go to the rungs, so the weapon drops away as
        // for a switch, and comes back up over the sprint-out (classic IW
        // and Modern Warfare lower the weapon on ladders).
        if frame.on_ladder {
            self.ladder_blend += (1.0 - self.ladder_blend) * smoothing(10.0, dt);
        } else if frame.sprint_out > 0.0 {
            self.ladder_blend = self.ladder_blend.min(frame.sprint_out);
        } else {
            self.ladder_blend *= 1.0 - smoothing(12.0, dt);
        }

        // A slide: the weapon cants hard left and drops, sprung in with a
        // small overshoot and eased back out (Black Ops 3 and Modern
        // Warfare's slide; Black Ops 2's `vSlideOfs`/`vSlideRot` slot).
        let sliding = frame.sliding && frame.on_floor;
        if sliding && !self.was_sliding {
            // The body hits the ground.
            self.jolt(
                Vector3::new(0.0, -0.008, 0.004),
                Vector3::new(-1.5, 0.0, 2.0),
            );
        }
        self.was_sliding = sliding;
        let (slide_k, slide_zeta) = if sliding { (110.0, 0.55) } else { (60.0, 0.9) };
        self.slide.step_toward(
            if sliding { 1.0 } else { 0.0 },
            slide_k,
            damping_for(slide_k, slide_zeta),
            dt,
        );
        let slide = self.slide.value * (1.0 - 0.8 * ads);

        // Crouching lowers and pulls the weapon in, eased (Black Ops 2's
        // `vDuckedOfs`/`vDuckedRot` at `posMoveRate` 6).
        let crouch_target = if frame.crouched { 1.0 } else { 0.0 };
        self.crouch_blend += (crouch_target - self.crouch_blend) * smoothing(POS_MOVE_RATE, dt);
        // Walking pulls it back a touch and tips it (`vStandMove`/`vStandRot`
        // at full walk speed).
        let walk_target = (self.bob_blend.min(1.0) * (1.0 - self.sprint_blend)).max(0.0);
        self.walk_blend += (walk_target - self.walk_blend) * smoothing(POS_MOVE_RATE, dt);
        let damage_rot = self.damage_kick(dt) * (1.0 - 0.7 * ads);

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
        let breath_t = self.time * TAU * 0.25;
        let breath = 1.0 - 0.9 * ads;
        let breathe_pos =
            Vector3::new(0.0012 * (breath_t * 0.5).sin(), 0.004 * breath_t.sin(), 0.0) * breath;
        let breathe_rot = Vector3::new(
            0.4 * (breath_t + 0.6).sin(),
            0.15 * (breath_t * 0.5 + 1.3).sin(),
            0.2 * (breath_t * 0.5).sin(),
        ) * breath;

        // Look sway and movement inertia.
        let look = look_sway_target(frame.look_velocity, ads);
        let follow = smoothing(6.0, dt);
        self.smoothed_velocity += (frame.velocity - self.smoothed_velocity) * follow;
        let change = frame.velocity - self.smoothed_velocity;
        let inertia = 1.0 - 0.7 * ads;
        let floor = if frame.on_floor { 1.0 } else { 0.0 };
        // Aimed, the look lag is mostly this shift (the turn is clamped hard).
        let look_shift = 0.002 - 0.0008 * ads;
        // Moving sideways the weapon trails the body: a shove on each start
        // and stop, and a steady lean while the strafe lasts.
        let sway_pos = Vector3::new(
            -frame.look_velocity.x * look_shift
                - change.x * 0.008
                - frame.velocity.x * STRAFE_SHIFT * floor,
            -frame.look_velocity.y * look_shift * 0.6 - frame.velocity.y * 0.004,
            change.z * 0.003,
        );
        let limit = Vector3::new(0.025, 0.025, 0.015) * inertia;
        let sway_pos = sway_pos.clamp(-limit, limit);
        // Strafing leans the weapon into the move: Black Ops 2's `vStrafeRot`
        // rolls it about 6 degrees at a full-speed strafe.
        let strafe_roll = -(frame.velocity.x / WALK_SPEED).clamp(-1.2, 1.2)
            * STRAFE_ROLL.to_radians()
            * floor
            * inertia;
        // In the air the muzzle tips up on the way up and a little down on
        // the way down.
        let air_pitch = if frame.on_floor {
            0.0
        } else {
            (frame.velocity.y * 0.0055).clamp(-0.01, 0.026) * inertia
        };
        let look = look + Vector3::new(air_pitch, 0.0, strafe_roll);
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
            damping_for(KICK_ROT_STIFFNESS, self.kick_zeta),
            dt,
        );
        let ride_c = damping_for(RIDE_STIFFNESS, RIDE_ZETA);
        self.ride_pos.step(RIDE_STIFFNESS, ride_c, dt);
        self.ride_rot.step(RIDE_STIFFNESS, ride_c, dt);
        let jolt_c = damping_for(JOLT_STIFFNESS, JOLT_ZETA);
        self.jolt_pos.step(JOLT_STIFFNESS, jolt_c, dt);
        self.jolt_rot.step(JOLT_STIFFNESS, jolt_c, dt);
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
            ViewAction::Throwing(p, _) => weapon_clips::pose_at(
                weapon_clips::holster_keys(kind),
                crate::grenade_hold::weapon_dip(p),
            ),
            _ => Pose::ZERO,
        };
        let switch = if self.ladder_blend > 1e-3 {
            let away = weapon_clips::pose_at(
                weapon_clips::holster_keys(kind),
                smoothstep(self.ladder_blend.min(1.0)),
            );
            Pose {
                rot: switch.rot + away.rot,
                pos: switch.pos + away.pos,
            }
        } else {
            switch
        };
        let present = first_raise.map_or(Pose::ZERO, |(q, _)| {
            weapon_clips::first_raise_present(kind, q)
        });
        let (switch_rot, switch_pos) = (
            switch.rot + melee_pose.rot + present.rot,
            switch.pos + melee_pose.pos + present.pos,
        );

        // Bolt or pump: a quick push and roll, peaking mid-cycle, mostly kept
        // off the sight line when aimed. A loaded shell gives a small bump.
        // A pump is two strokes (the gun follows the pump back and forward);
        // a bolt is one smooth throw.
        let cycle = match frame.action {
            ViewAction::Cycling(p) => {
                let stroke = if kind == ClipKind::Shotgun {
                    keyed(&PUMP_BACK, p)
                } else {
                    (PI * p).sin().max(0.0)
                };
                stroke * (1.0 - 0.5 * frame.scope) * (1.0 - 0.75 * ads)
            }
            _ => 0.0,
        };
        self.cycle_kick = (self.cycle_kick - dt * 6.0).max(0.0);
        self.pickup_dip = (self.pickup_dip - dt * 2.5).max(0.0);
        let bump = smoothstep(self.cycle_kick) * 0.4 + smoothstep(self.pickup_dip) * 0.6;

        let (hip, hip_rot, aim, sprint_pos, sprint_rot, pivot, cycle_pos, cycle_rot) = {
            let v = view.bind();
            // An ACOG puts the eye just behind its eyepiece.
            let aim = match v.eyepiece_z {
                Some(eyepiece) => Vector3::new(
                    v.ads_position.x,
                    v.ads_position.y,
                    -ACOG_EYE_RELIEF - eyepiece,
                ),
                None => v.ads_position,
            };
            (
                v.hip_position,
                v.hip_rotation,
                aim,
                v.sprint_position,
                v.sprint_rotation,
                v.pivot,
                v.cycle_position,
                v.cycle_rotation,
            )
        };
        let (sprint_pos, sprint_rot, hip, hip_rot) = if kind == ClipKind::Pistol {
            (sprint_pos, sprint_rot, hip, hip_rot)
        } else {
            (
                sprint_pos + SPRINT_LONG_POS,
                sprint_rot + SPRINT_LONG_ROT,
                hip + HIP_LONG_POS,
                hip_rot + HIP_LONG_ROT,
            )
        };
        // The hip pose sits closer and further right than the scenes' rest,
        // so the receiver and both hands fill the lower right of the frame.
        let hip = hip + HIP_CLOSER;
        let mut position = hip + (aim - hip) * ads;
        position += Vector3::new(-0.012, -0.018, 0.02) * arc;
        position += sprint_pos * sprint;
        position += bob_pos + breathe_pos + self.sway_pos.value;
        position += self.kick_pos.value + self.ride_pos.value + self.jolt_pos.value;
        position += Vector3::new(0.0, land.y, 0.0);
        // Crouched, the weapon sits lower, closer, and a little right; walking
        // pulls it back a touch. Both at the hip only.
        position += DUCKED_POS * (self.crouch_blend * hip_amount);
        position += STAND_MOVE_POS * (self.walk_blend * hip_amount);
        position += SLIDE_POS * slide + MANTLE_POS * mantle;
        position += cycle_pos * cycle + Vector3::new(0.0, -0.02 * bump, 0.01 * bump);
        // A clip cut short (firing out of a reload's settle) eases out over
        // about 100 ms instead of snapping to rest.
        let (clip_pos, clip_rot) = (clip_pose.pos * clip_scale, clip_pose.rot * clip_scale);
        if clip_pos == Vector3::ZERO && clip_rot == Vector3::ZERO {
            let fade = (-dt * CLIP_FADE_RATE).exp();
            self.clip_carry = (self.clip_carry.0 * fade, self.clip_carry.1 * fade);
        } else {
            self.clip_carry = (clip_pos, clip_rot);
            self.clip_focus = clip.focus;
        }
        position += self.clip_carry.0 + switch_pos;
        let ins_rot = self.clip_carry.1;
        let deg = |v: f32| v.to_radians();
        let motion =
            self.sway_rot.value + self.kick_rot.value + self.ride_rot.value + self.jolt_rot.value;
        let pitch = deg(ins_rot.x)
            + deg(hip_rot.x) * hip_amount
            + deg(sprint_rot.x) * sprint
            + deg(bob_rot.x + breathe_rot.x)
            + motion.x
            + land.x
            + deg(-3.0) * arc
            + deg(switch_rot.x)
            + deg(cycle_rot.x) * cycle
            + deg(4.0) * bump
            + deg(DUCKED_ROT.x) * self.crouch_blend * hip_amount
            + deg(STAND_MOVE_ROT.x) * self.walk_blend * hip_amount
            + deg(SLIDE_ROT.x) * slide
            + deg(MANTLE_ROT.x) * mantle
            + deg(damage_rot.x);
        let yaw = deg(ins_rot.y)
            + deg(hip_rot.y) * hip_amount
            + deg(switch_rot.y)
            + deg(sprint_rot.y) * sprint
            + deg(bob_rot.y + breathe_rot.y)
            + motion.y
            + deg(cycle_rot.y) * cycle
            + deg(SLIDE_ROT.y) * slide
            + deg(MANTLE_ROT.y) * mantle
            + deg(damage_rot.y);
        let roll = deg(ins_rot.z)
            + deg(hip_rot.z) * hip_amount
            + deg(sprint_rot.z) * sprint
            + deg(bob_rot.z + breathe_rot.z)
            + motion.z
            + deg(7.0) * arc_roll
            + deg(switch_rot.z)
            + deg(cycle_rot.z) * cycle
            + deg(DUCKED_ROT.z) * self.crouch_blend * hip_amount
            + deg(STAND_MOVE_ROT.z) * self.walk_blend * hip_amount
            + deg(SLIDE_ROT.z) * slide
            + deg(MANTLE_ROT.z) * mantle
            + deg(damage_rot.z);

        // Rotate around the pivot (the firing hand) at the hip. Aimed, turn
        // about a point just behind the rear sight (the cheek on the stock),
        // so a kick tips the front sight up and lifts the rear a hair,
        // without taking the sight picture off the eye.
        let pivot = pivot * hip_amount + Vector3::new(0.0, 0.0, AIM_PIVOT_BACK) * ads;
        let basis = Basis::from_euler(EulerOrder::YXZ, Vector3::new(pitch, yaw, roll));
        let mut origin = position + pivot - basis * pivot;
        // A clip turns the weapon about its own focus (the magazine well),
        // not the hip pivot, so the work stays in the same part of the frame
        // whatever the weapon's length.
        if let Some(focus) = self.clip_focus {
            let turn = Basis::from_euler(
                EulerOrder::YXZ,
                Vector3::new(deg(ins_rot.x), deg(ins_rot.y), deg(ins_rot.z)),
            );
            let arm = focus - pivot;
            origin += arm - turn * arm;
        }
        view.set_transform(Transform3D::new(basis, origin));
        self.update_scope_lens(&view, frame, shown);

        // The knife path is in the rig's (view) space; the hands want it in
        // the weapon's.
        let weapon_in_rig = view.get_transform().affine_inverse();
        let grips = view.bind().grips();
        let item = if let Some((t, strike)) = self.melee {
            let (blend, local) = crate::knife::first_person(t, strike);
            Some((blend, weapon_in_rig * local, Held::Knife))
        } else {
            // The support hand plants on the ledge through a mantle or vault.
            mantle_progress.map(|m| {
                let plant = keyed(&[(0.0, 0.0), (0.22, 1.0), (0.78, 1.0), (1.0, 0.0)], m);
                (
                    plant * hip_amount,
                    weapon_in_rig * plant_hand(m, frame.vaulting),
                    Held::Plant,
                )
            })
        };
        if let Some(arms) = &mut self.arms {
            // The hands follow the grip markers, which the clips move.
            let hold = shown.then(|| {
                let mut grips = grips;
                grips.hold = crate::character::poser::Hold::of(frame.weapon.def());
                grips.fit_to(frame.weapon.def());
                (view.clone().upcast::<Node3D>(), grips)
            });
            let throw = match frame.action {
                ViewAction::Throwing(p, kind) => {
                    let pose = crate::grenade_hold::first_person(p, kind);
                    Some((pose, weapon_in_rig * pose.at))
                }
                _ => None,
            };
            arms.bind_mut().follow(hold, 0.0, None, item, throw);
        }
    }
}

/// The support hand planted on a ledge through a climb, `m` 0..1 through it
/// (the left hand bone's frame in the view's space): reached out low and
/// left, palm down and fingers forward, then pressed down and back as the
/// body rises over it. A vault plants on the cover's top, lower, and the
/// body carries on past it.
fn plant_hand(m: f32, vault: bool) -> Transform3D {
    let (reach, pressed) = if vault {
        (
            Vector3::new(-0.13, -0.17, -0.42),
            Vector3::new(-0.1, -0.3, -0.2),
        )
    } else {
        (
            Vector3::new(-0.14, -0.16, -0.4),
            Vector3::new(-0.12, -0.28, -0.3),
        )
    };
    let at = reach.lerp(pressed, smoothstep(m));
    // Fingers forward and a little out, the palm (+Z) down on the ledge.
    let fingers = Vector3::new(0.25, -0.15, -1.0).normalized();
    let palm = Vector3::new(0.1, -1.0, 0.1);
    let z = (palm - fingers * palm.dot(fingers)).normalized() * PLANT_PALM;
    let y = fingers;
    Transform3D::new(Basis::from_cols(y.cross(z), y, z), at)
}

/// Which way the left hand bone's +Z (`PoseInput` hands) faces from the
/// palm: 1 out of the palm.
const PLANT_PALM: f32 = 1.0;

/// Black Ops 2's `posMoveRate`: how fast the stance and walk poses follow, in
/// 1/seconds.
const POS_MOVE_RATE: f32 = 6.0;
/// Crouched, at the hip (Black Ops 2's `vDuckedOfs`/`vDuckedRot` for a rifle:
/// 1 inch back, 0.3 right, 0.2 down; pitch 2, roll -2).
const DUCKED_POS: Vector3 = Vector3::new(0.008, -0.005, 0.025);
const DUCKED_ROT: Vector3 = Vector3::new(2.0, 0.0, -2.0);
/// Walking at full speed, at the hip (`vStandMove`/`vStandRot`: back 0.2
/// inch; pitch 2, roll -2).
const STAND_MOVE_POS: Vector3 = Vector3::new(0.0, 0.0, 0.005);
const STAND_MOVE_ROT: Vector3 = Vector3::new(2.0, 0.0, -2.0);
/// The strafe lean (degrees at a full walk-speed strafe) and the shift the
/// weapon trails by (metres per m/s; `vStrafeMove` is half an inch).
const STRAFE_ROLL: f32 = 5.0;
const STRAFE_SHIFT: f32 = 0.0022;
/// Sliding: canted hard left, dropped, and pulled in (metres, degrees),
/// as Black Ops 3 and Modern Warfare's slide clips hold a rifle (measured
/// from footage, roughly).
const SLIDE_POS: Vector3 = Vector3::new(-0.012, 0.004, 0.012);
const SLIDE_ROT: Vector3 = Vector3::new(5.0, -3.0, 10.0);
/// Mantling: tucked down and right, canted out of the planting hand's way.
const MANTLE_POS: Vector3 = Vector3::new(0.012, -0.015, 0.008);
const MANTLE_ROT: Vector3 = Vector3::new(-5.0, -4.0, -9.0);
/// A hit's knock on the weapon (Black Ops 2's `DamageKickAngles`): seconds
/// out and back, and its size in degrees for a rifle round.
const DAMAGE_DEFLECT: f32 = 0.1;
const DAMAGE_RETURN: f32 = 0.4;
const DAMAGE_KICK: f32 = 4.0;

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
    #[test]
    fn flash_frames_vary_from_shot_to_shot() {
        let frames: Vec<u32> = (1..=40).map(|n| super::flash_seed(n) as u32 % 4).collect();
        for f in 0..4 {
            assert!(
                frames.iter().filter(|&&x| x == f).count() >= 5,
                "{frames:?}"
            );
        }
        let repeats = frames.windows(2).filter(|w| w[0] == w[1]).count();
        assert!(repeats < 16, "{frames:?}");
    }

    #[test]
    fn a_red_dot_sits_a_tenth_of_a_metre_from_the_eye_within_the_rail() {
        use super::{REFLEX_EYE_DISTANCE, reflex_seat, sights::REFLEX_WINDOW_BEHIND};
        // The M4: aimed 9 cm ahead, mount 8.7 cm further: the dot slides back.
        let z = reflex_seat(-0.0868, -0.09);
        let window = -0.09 + z - REFLEX_WINDOW_BEHIND;
        assert!((window + REFLEX_EYE_DISTANCE).abs() < 1e-5, "{window}");
        assert!(z > -0.0868);
        // The AK: already about there.
        assert!((reflex_seat(0.0424, -0.14) - 0.0424).abs() < 0.01);
        // Never off the rail.
        assert_eq!(reflex_seat(0.0, -0.4), 0.09);
        assert_eq!(reflex_seat(0.0, 0.2), -0.02);
    }

    #[test]
    fn first_person_copies_key_and_clip_where_asked() {
        let code = "shader_type spatial;\nvoid fragment() {\n\tALBEDO = vec3(1.0);\n}\n";
        let keyed = super::variant_code(code, true, false).unwrap();
        assert!(keyed.starts_with("shader_type spatial;\n#define VIEWMODEL_KEY\n"));
        assert!(!keyed.contains("discard"));
        let clipped = super::variant_code(code, false, true).unwrap();
        assert!(!clipped.contains("VIEWMODEL_KEY"));
        assert!(clipped.contains("if (viewmodel_behind_lens(VERTEX)) {\n\t\tdiscard;"));
        let both = super::variant_code(code, true, true).unwrap();
        assert!(both.contains("VIEWMODEL_KEY") && both.contains("discard"));
        assert!(super::variant_code("void fragment() {}", true, false).is_none());
        assert!(super::variant_code("shader_type spatial;", false, true).is_none());
    }

    #[test]
    fn the_carrier_runs_back_and_home_inside_each_cycle() {
        for rpm in [450.0, 750.0, 1000.0] {
            let cycle = 60.0 / rpm;
            assert_eq!(super::carrier_stroke(0.0, cycle), 0.0);
            let peak = (cycle * 0.8).min(super::CARRIER_STROKE) * 0.4;
            assert!((super::carrier_stroke(peak, cycle) - 1.0).abs() < 1e-4);
            // Home before the next round fires.
            assert_eq!(super::carrier_stroke(cycle, cycle), 0.0, "{rpm}");
        }
    }

    #[test]
    fn the_trigger_is_held_through_automatic_fire_and_then_reset() {
        let cycle = 60.0 / 750.0;
        assert_eq!(super::trigger_pull(0.0, cycle), 1.0);
        // Still held when the next round fires.
        assert_eq!(super::trigger_pull(cycle, cycle), 1.0);
        assert_eq!(super::trigger_pull(cycle + 0.1, cycle), 0.0);
        // A slow semi-automatic shot is held long enough to see.
        assert_eq!(super::trigger_pull(0.07, 0.6), 1.0);
        assert_eq!(super::trigger_pull(10.0, 0.6), 0.0);
    }

    use super::*;

    /// Peak of one shot's kick on the weapon, simulated with the kick springs.
    fn kick_peak(shot: u32, strength: f32, ads: f32) -> (Vector3, Vector3) {
        let (pos, rot) = shot_kick(shot, strength, ads, &KickFeel::of(WeaponId::expect("m4")));
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
        // Aimed the front sight tips up visibly, about half a degree.
        assert!((0.35..=0.75).contains(&rot.x), "aimed sight tip {}", rot.x);
        // The shove peaks within two frames at 60 FPS and is gone in ~120 ms.
        let (p, _) = shot_kick(7, 1.35, 1.0, &KickFeel::of(WeaponId::expect("m4")));
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
        let m4 = KickFeel::of(WeaponId::expect("m4"));
        // Aimed the front sight always kicks up visibly, about half a degree
        // on a rifle, and never past the cap even on the heaviest weapons.
        for i in 0..200 {
            let (_, rot) = shot_kick(i, 1.35, 1.0, &m4);
            assert!(rot.x > 0.4 && rot.x <= 0.6, "{rot:?}");
        }
        assert!(shot_kick(3, 9.0, 1.0, &m4).1.x <= AIM_LIFT_MAX + 1e-4);
        // Within 0.05 degrees of the sight line by the next round at 750 rpm.
        let (_, rot) = shot_kick(5, 1.35, 1.0, &m4);
        let mut spring = Spring {
            value: Vector3::ZERO,
            velocity: peak_impulse(rot, KICK_ROT_STIFFNESS, m4.zeta),
        };
        spring.step(
            KICK_ROT_STIFFNESS,
            damping_for(KICK_ROT_STIFFNESS, m4.zeta),
            0.08,
        );
        assert!(spring.value.x.abs() < 0.06, "{}", spring.value.x);
        // A pump gun bucks hard at the hip and aimed; a pistol flips its
        // muzzle aimed.
        let m870 = KickFeel::of(WeaponId::expect("m870"));
        let pump = shot_kick(2, 2.75, 0.0, &m870);
        assert!(pump.1.x > 5.0 && pump.0.z > 0.04, "{pump:?}");
        assert!(shot_kick(2, 2.75, 1.0, &m870).1.x > 1.1);
        let m1911 = KickFeel::of(WeaponId::expect("m1911"));
        let pistol = shot_kick(2, 1.15, 1.0, &m1911);
        assert!(pistol.1.x > 0.7, "{pistol:?}");
        // At the hip the muzzle always tips up, and the roll changes side
        // from shot to shot without a fixed alternation.
        let hip: Vec<Vector3> = (0..32).map(|i| shot_kick(i, 1.35, 0.0, &m4).1).collect();
        assert!(hip.iter().all(|r| r.x > 0.5));
        let flips = hip
            .windows(2)
            .filter(|w| w[0].z.signum() != w[1].z.signum())
            .count();
        assert!((6..=26).contains(&flips), "{flips} side changes in 31");
    }

    #[test]
    fn each_weapon_kicks_its_own_way() {
        let feel = |k: &str| KickFeel::of(WeaponId::expect(k));
        // The AK is rougher than the M4: more lift, more wander, bouncier.
        assert!(feel("ak47").aim_lift > feel("m4").aim_lift);
        assert!(feel("ak47").aim_yaw > 2.0 * feel("m4").aim_yaw);
        assert!(feel("ak47").zeta < feel("m4").zeta);
        // The SMG is a crisp buzz; attachments kick like their weapon.
        assert!(feel("pdw9").zeta > feel("m4").zeta);
        for id in WeaponId::all() {
            assert_eq!(KickFeel::of(id), KickFeel::of(base_weapon(id)));
        }
    }

    #[test]
    fn the_weapon_lags_a_turn_less_when_aimed() {
        // Turning left at 300 degrees a second.
        let turn = Vector2::new(300f32.to_radians(), 0.0);
        let hip = look_sway_target(turn, 0.0);
        let aimed = look_sway_target(turn, 1.0);
        // It points right of the turn (negative yaw) and cants with it.
        assert!(hip.y < 0.0 && hip.z > 0.0);
        // A soft limit: about 2.5 degrees here, more for a faster flick, never
        // past 4.
        assert!(
            (2.0..2.8).contains(&-hip.y.to_degrees()),
            "{}",
            hip.y.to_degrees()
        );
        let faster = look_sway_target(turn * 2.0, 0.0);
        assert!(faster.y < hip.y && faster.y.to_degrees() > -4.0);
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
