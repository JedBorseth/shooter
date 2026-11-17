//! `FpArms`: the player's own arms in the first-person view. The same soldier
//! model and skeleton as the third-person character, showing only its arms
//! mesh, drawn with the viewmodel shader (own FOV, never clips walls), with the
//! hands solved onto the active weapon's grip markers by [`SoldierPoser`].
//! The viewmodel rig moves the weapon; the arms follow it.

use godot::classes::{
    AnimationLibrary, AnimationPlayer, INode3D, MeshInstance3D, Node3D, Skeleton3D,
};
use godot::prelude::*;

use super::palette::{self, Team};
use super::poser::{Grips, PoseInput, SoldierPoser, WeaponPlacement};

/// Where the support hand reaches during a reload, in weapon space.
const RELOAD_REACH: Vector3 = Vector3::new(-0.06, -0.3, 0.08);

#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct FpArms {
    base: Base<Node3D>,

    /// Where the body sits relative to the camera (the eyes are at the camera).
    #[export]
    #[init(val = Vector3::new(0.03, -1.9, 0.05))]
    body_offset: Vector3,
    /// Turns the body (degrees) so the support shoulder leads, as when
    /// shouldering a rifle.
    #[export]
    #[init(val = -28.0)]
    body_yaw: f32,
    /// First-person arms are drawn a little longer than the body's, so the
    /// shoulders can sit behind the camera and the forearms come in from below.
    #[export]
    #[init(val = 1.12)]
    scale: f32,

    #[init(node = "Model")]
    model: OnReady<Gd<Node3D>>,
    poser: Option<Gd<SoldierPoser>>,
    arms: Option<Gd<MeshInstance3D>>,
}

#[godot_api]
impl INode3D for FpArms {
    fn ready(&mut self) {
        let offset = self.body_offset;
        let yaw = self.body_yaw.to_radians();
        let basis = Basis::from_axis_angle(Vector3::UP, yaw).scaled(Vector3::splat(self.scale));
        self.base_mut()
            .set_transform(Transform3D::new(basis, offset));

        let Some(skeleton) = self
            .model
            .find_child_ex("GeneralSkeleton")
            .owned(false)
            .done()
            .and_then(|n| n.try_cast::<Skeleton3D>().ok())
        else {
            godot_error!("FpArms: no GeneralSkeleton under Model");
            return;
        };
        for child in skeleton.get_children().iter_shared() {
            if let Ok(mut mesh) = child.try_cast::<MeshInstance3D>() {
                let arms = mesh.get_name() == "ArmsFP";
                mesh.set_visible(arms);
                if arms {
                    mesh.set_cast_shadows_setting(
                        godot::classes::geometry_instance_3d::ShadowCastingSetting::OFF,
                    );
                    viewmodel_colours(&mut mesh, Team::Allies);
                    self.arms = Some(mesh);
                }
            }
        }

        // Fingers curl from the two-handed pistol pose; the IK moves the rest.
        let mut player = AnimationPlayer::new_alloc();
        player.set_name("GripPose");
        if let Ok(lib) = try_load::<AnimationLibrary>("res://assets/characters/animations/ual1.glb")
        {
            player.add_animation_library("ual1", &lib);
        }
        self.model.clone().upcast::<Node>().add_child(&player);
        player.set_root_node("..");
        player.play_ex().name("ual1/Pistol_Aim_Neutral").done();

        let mut poser = SoldierPoser::new_alloc();
        poser.set_name("Poser");
        skeleton.upcast::<Node>().add_child(&poser);
        self.poser = Some(poser);
    }
}

#[godot_api]
impl FpArms {
    /// Where the hand IK last put a wrist (world space), or zero if it is off.
    #[func]
    pub fn wrist_position(&self, right: bool) -> Vector3 {
        self.poser
            .as_ref()
            .and_then(|p| p.bind().solved_wrist(right))
            .unwrap_or(Vector3::ZERO)
    }
}

impl FpArms {
    /// Holds `weapon` (or hides the arms with None). `support_off` (0..1) takes
    /// the support hand off the weapon, for reloads.
    pub fn follow(&mut self, weapon: Option<(Gd<Node3D>, Grips)>, support_off: f32) {
        let visible = weapon.is_some();
        if let Some(arms) = &mut self.arms
            && arms.is_visible() != visible
        {
            arms.set_visible(visible);
        }
        let Some(poser) = &mut self.poser else {
            return;
        };
        let mut poser = poser.bind_mut();
        match weapon {
            Some((node, grips)) => {
                poser.weapon = Some(node);
                poser.input = PoseInput {
                    placement: WeaponPlacement::External,
                    grips: Some(grips),
                    ik: 1.0,
                    support_off,
                    support_target: RELOAD_REACH,
                    ..PoseInput::default()
                };
            }
            None => {
                poser.weapon = None;
                poser.input.ik = 0.0;
            }
        }
    }
}

/// Replaces the arms' materials with the team's viewmodel-shader material.
fn viewmodel_colours(mesh: &mut Gd<MeshInstance3D>, team: Team) {
    let Some(source) = mesh.get_mesh() else {
        return;
    };
    for surface in 0..source.get_surface_count() {
        let Some(material) = source.surface_get_material(surface) else {
            continue;
        };
        if let Some(character) = palette::material_for(&material.get_name().to_string(), team, true)
        {
            mesh.set_surface_override_material(surface, &character);
        }
    }
}
