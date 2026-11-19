//! `FpArms`: the player's own arms in the first-person view. The same soldier
//! model and skeleton as the third-person character, showing only its arms
//! mesh, drawn with the viewmodel shader (own FOV, never clips walls), with the
//! hands solved onto the active weapon's grip markers by [`SoldierPoser`].
//! The viewmodel rig moves the weapon; the arms follow it. The hands are life
//! size on the longer arms, and the fingers take the weapon's fitted poses
//! (`WeaponDef::hands`, fitted by [`FpArms::fit_hands`]).

use super::grasp::WeaponFields;
use super::palette::{self, Team};
use super::poser::{
    Grips, HandDepths, Held, LadderLimbs, PoseInput, SoldierPoser, WeaponPlacement,
};
use godot::classes::{
    AnimationLibrary, AnimationPlayer, INode3D, MeshInstance3D, Node3D, Skeleton3D,
};
use godot::prelude::*;
use shooter_sim::weapon::FingerFlex;

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

    /// Where the body sits now, from `body_offset` (`set_body_shift`).
    shift: Vector3,

    #[init(node = "Model")]
    model: OnReady<Gd<Node3D>>,
    poser: Option<Gd<SoldierPoser>>,
    arms: Option<Gd<MeshInstance3D>>,
    /// The weapon in hand's grips, as last given (`fit_hands` reads them).
    grips: Option<Grips>,
    /// The last weapon surface measured for the offline tools, keyed by the
    /// weapon view and its grip rests, so a search over many candidate
    /// poses (`try_hand`) measures it once.
    measured: std::cell::RefCell<Option<(InstanceId, Vector3, Vector3, WeaponFields)>>,
    /// A candidate pose per hand (firing, support) drawn instead of the
    /// weapon's own, for choosing hand-set values by eye
    /// ([`FpArms::preview_hand`]); `None` draws the weapon's.
    preview: [Option<(FingerFlex, [f32; 5], Vector3)>; 2],
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
        // Only the arms are drawn longer than life.
        poser.bind_mut().hand_scale = 1.0 / self.scale;
        skeleton.upcast::<Node>().add_child(&poser);

        // The knife the support hand swings, drawn like the arms.
        let knife = crate::knife::build();
        let shader = load::<godot::classes::Shader>("res://shaders/viewmodel.gdshader");
        crate::viewmodel::convert_to_viewmodel_materials(&knife.clone().upcast(), &shader, 1.0);
        self.base_mut().add_child(&knife);
        poser.bind_mut().knife = Some(knife);
        let grenade = crate::grenade_hold::build();
        crate::viewmodel::convert_to_viewmodel_materials(&grenade.clone().upcast(), &shader, 1.0);
        self.base_mut().add_child(&grenade);
        poser.bind_mut().grenade = Some(grenade);
        self.poser = Some(poser);
        crate::effects::keep_decals_off(&self.base().clone().upcast());
    }
}

#[godot_api]
impl FpArms {
    /// Fits finger poses to the weapon in hand, from where the hands hold it
    /// now (an offline tool: `tests/bake_hand_poses.gd`). Returns `right`,
    /// `left` (15 angles each: index, middle, ring, little, thumb, three
    /// joints each), `index_off` (3), `right_spread` and `left_spread` (5
    /// each: every finger's spread, then the thumb's turn), `right_seat` and
    /// `left_seat` (each hand's move from its marker, weapon space, m), and
    /// the fitted pose's depths as [`FpArms::hand_depths`] gives them; or an
    /// empty dictionary when no weapon is held.
    #[func]
    pub fn fit_hands(&self) -> VarDictionary {
        let mut out = VarDictionary::new();
        let Some((poser, grips, fields)) = self.held_fields(false) else {
            return out;
        };
        let Some(fit) = poser.bind().fit_hands(&fields, &grips) else {
            return out;
        };
        let flat = |f: &[[f32; 3]]| PackedFloat32Array::from_iter(f.iter().flatten().copied());
        out.set("right", &flat(&fit.right));
        out.set("left", &flat(&fit.left));
        out.set("index_off", &flat(&[fit.index_off]));
        out.set(
            "right_spread",
            &PackedFloat32Array::from(&fit.spreads[0][..]),
        );
        out.set(
            "left_spread",
            &PackedFloat32Array::from(&fit.spreads[1][..]),
        );
        out.set("right_seat", fit.seats[0]);
        out.set("left_seat", fit.seats[1]);
        depths_into(&mut out, &fit.depths);
        out
    }

    /// How deep the hands' drawn poses go into the weapon in hand, where they
    /// hold it now (`tests/hand_poses_test.gd`): `finger_depths` (mm, each
    /// finger: right index to thumb, the right index off the trigger, then
    /// left index to thumb) and `palm_depth` (mm, right and left), negative
    /// when clear, both where the hands are aimed; and `reach_miss` (mm,
    /// right and left), how far the drawn hands fell short of that; or an
    /// empty dictionary when no weapon is held.
    #[func]
    pub fn hand_depths(&self) -> VarDictionary {
        let mut out = VarDictionary::new();
        if let Some((poser, grips, fields)) = self.held_fields(false)
            && let Some(depths) = poser.bind().hand_depths(&fields, &grips)
        {
            depths_into(&mut out, &depths);
        }
        out
    }

    /// Measures a candidate pose for one hand (0 firing, 1 support) on the
    /// weapon in hand, where it holds it now, for tuning a pose by hand
    /// (`hand_tuning` in a weapon file): `flex` (15 angles, index to thumb,
    /// three joints each), `spread` (5: each finger's spread, then the
    /// thumb's turn) and `seat` (off its marker, weapon space, m). Returns
    /// `depths` (mm per finger; negative: clear) and `points` (an array per
    /// finger of its points in weapon space), or an empty dictionary.
    #[func]
    pub fn try_hand(
        &self,
        hand: i64,
        flex: PackedFloat32Array,
        spread: PackedFloat32Array,
        seat: Vector3,
    ) -> VarDictionary {
        let mut out = VarDictionary::new();
        let (flex, spread) = (flex.as_slice(), spread.as_slice());
        if !(0..=1).contains(&hand) || flex.len() != 15 || spread.len() != 5 {
            return out;
        }
        let Some((poser, _, fields)) = self.held_fields(true) else {
            return out;
        };
        let mut pose = [[0.0; 3]; 5];
        for (f, finger) in pose.iter_mut().enumerate() {
            finger.copy_from_slice(&flex[f * 3..f * 3 + 3]);
        }
        let mut turns = [0.0; 5];
        turns.copy_from_slice(spread);
        let Some(probe) = poser
            .bind()
            .try_hand(&fields, hand as usize, pose, turns, seat)
        else {
            return out;
        };
        out.set(
            "depths",
            &PackedFloat32Array::from_iter(probe.depths.iter().map(|d| d * 1000.0)),
        );
        let points: VarArray = probe
            .points
            .iter()
            .map(|f| PackedVector3Array::from_iter(f.iter().map(|(p, _)| *p)).to_variant())
            .collect();
        out.set("points", &points);
        out
    }

    /// A fit (as [`FpArms::fit_hands`] gives it) with the weapon's hand-set
    /// values (`hand_tuning` in its file) applied, and its finger depths
    /// measured again with them, so the bake checks the poses as the game
    /// draws them. The support hand's tuning is left out on a foregrip
    /// (`foregrip`), which the fit alone poses.
    #[func]
    pub fn with_tuning(&self, fit: VarDictionary, foregrip: bool) -> VarDictionary {
        let out = fit.duplicate_shallow();
        let Some((poser, _, fields)) = self.held_fields(false) else {
            return out;
        };
        let def = poser
            .bind()
            .weapon
            .clone()
            .and_then(|v| v.try_cast::<crate::viewmodel::WeaponView>().ok())
            .and_then(|v| v.bind().weapon_id())
            .map(|id| id.def());
        let Some(tuning) = def.and_then(|d| d.hand_tuning) else {
            return out;
        };
        let mut out = out;
        let Ok(mut depths) = fit
            .get_or_nil("finger_depths")
            .try_to::<PackedFloat32Array>()
        else {
            return out;
        };
        let tunes = [tuning.right, if foregrip { None } else { tuning.left }];
        for (h, tune) in tunes.into_iter().enumerate() {
            let Some(tune) = tune else { continue };
            let side = if h == 0 { "right" } else { "left" };
            let read = |key: &str| {
                fit.get_or_nil(key)
                    .try_to::<PackedFloat32Array>()
                    .unwrap_or_default()
            };
            let (flex, spread) = (read(side), read(&format!("{side}_spread")));
            let seat = fit
                .get_or_nil(format!("{side}_seat").as_str())
                .try_to::<Vector3>()
                .unwrap_or(Vector3::ZERO);
            if flex.len() != 15 || spread.len() != 5 {
                continue;
            }
            let mut hand = shooter_sim::weapon::HandDef {
                fingers: [[0.0; 3]; 5],
                spread: Some([0.0; 5]),
                seat: seat.to_array(),
            };
            for (f, finger) in hand.fingers.iter_mut().enumerate() {
                finger.copy_from_slice(&flex.as_slice()[f * 3..f * 3 + 3]);
            }
            if let Some(s) = hand.spread.as_mut() {
                s.copy_from_slice(spread.as_slice());
            }
            let tuned = tune.apply(hand);
            let turns = tuned.spread.unwrap_or_default();
            let seat = Vector3::from_array(tuned.seat);
            out.set(
                side,
                &PackedFloat32Array::from_iter(tuned.fingers.iter().flatten().copied()),
            );
            out.set(
                format!("{side}_spread").as_str(),
                &PackedFloat32Array::from(&turns[..]),
            );
            out.set(format!("{side}_seat").as_str(), seat);
            let Some(probe) = poser
                .bind()
                .try_hand(&fields, h, tuned.fingers, turns, seat)
            else {
                continue;
            };
            // `finger_depths`: the firing hand's five, its index off the
            // trigger, then the support hand's five.
            let at = if h == 0 { 0 } else { 6 };
            for (f, d) in probe.depths.iter().enumerate() {
                if at + f < depths.len() {
                    depths[at + f] = d * 1000.0;
                }
            }
        }
        out.set("finger_depths", &depths);
        out
    }

    /// Draws a candidate pose for one hand (0 firing, 1 support) instead of
    /// the weapon's own until cleared, as [`FpArms::try_hand`] takes it, so
    /// an offline tool can render it; an empty `flex` clears it.
    #[func]
    pub fn preview_hand(
        &mut self,
        hand: i64,
        flex: PackedFloat32Array,
        spread: PackedFloat32Array,
        seat: Vector3,
    ) {
        let Ok(h) = usize::try_from(hand) else {
            return;
        };
        if h > 1 {
            return;
        }
        let (flex, spread) = (flex.as_slice(), spread.as_slice());
        self.preview[h] = (flex.len() == 15 && spread.len() == 5).then(|| {
            let mut pose = [[0.0; 3]; 5];
            for (f, finger) in pose.iter_mut().enumerate() {
                finger.copy_from_slice(&flex[f * 3..f * 3 + 3]);
            }
            let mut turns = [0.0; 5];
            turns.copy_from_slice(spread);
            (pose, turns, seat)
        });
    }

    /// Whether both grip markers of the weapon in hand are at rest (no clip,
    /// pump stroke, or bolt moving a hand), so a pose measured now is the
    /// held pose.
    #[func]
    pub fn hands_settled(&self) -> bool {
        self.grips.is_some_and(|g| {
            [g.right, g.left]
                .iter()
                .zip(g.rests)
                .all(|(at, rest)| rest.is_some_and(|r| at.distance_to(r) < 1e-4))
        })
    }

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
    /// the support hand off the weapon, for reloads, toward `support_target`
    /// in the weapon's space (a magazine pouch if `None`). `item` puts the
    /// support hand on the knife or planted on a ledge instead, by a blend
    /// (0..1), at a transform in the weapon's space. On a ladder, `ladder`
    /// takes both hands off the put-away weapon onto the rungs.
    pub fn follow(
        &mut self,
        weapon: Option<(Gd<Node3D>, Grips)>,
        support_off: f32,
        support_target: Option<Vector3>,
        item: Option<(f32, Transform3D, Held)>,
        throw: Option<(crate::grenade_hold::ThrowPose, Transform3D)>,
        ladder: Option<LadderLimbs>,
    ) {
        let visible = weapon.is_some();
        if let Some(arms) = &mut self.arms
            && arms.is_visible() != visible
        {
            arms.set_visible(visible);
        }
        let weapon = weapon.map(|(node, mut grips)| {
            for (h, preview) in self.preview.iter().enumerate() {
                if let Some((flex, spread, seat)) = preview {
                    grips.fingers[h] = Some(*flex);
                    grips.spreads[h] = Some(*spread);
                    grips.seats[h] = *seat;
                }
            }
            (node, grips)
        });
        self.grips = weapon.as_ref().map(|(_, grips)| *grips);
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
                    ik: 1.0 - ladder.map_or(0.0, |l| l.weight.clamp(0.0, 1.0)),
                    ladder,
                    support_off,
                    support_target: support_target.unwrap_or(RELOAD_REACH),
                    knife: item.map_or(0.0, |(blend, _, _)| blend),
                    knife_at: item.map_or(Transform3D::IDENTITY, |(_, at, _)| at),
                    held: item.map_or(Held::Knife, |(_, _, held)| held),
                    grenade: throw.map_or(0.0, |(pose, _)| pose.blend),
                    grenade_at: throw.map_or(Transform3D::IDENTITY, |(_, at)| at),
                    grenade_in_hand: throw.is_some_and(|(pose, _)| pose.in_hand),
                    grenade_pinned: throw.is_some_and(|(pose, _)| pose.pinned),
                    grenade_kind: throw.map_or(Default::default(), |(pose, _)| pose.kind),
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

impl FpArms {
    /// Moves the body from `body_offset` by `shift` (view space, m), as the
    /// aimed weapon or a throw asks; the shoulders go with it.
    pub fn set_body_shift(&mut self, shift: Vector3) {
        if shift.distance_squared_to(self.shift) < 1e-10 {
            return;
        }
        self.shift = shift;
        let origin = self.body_offset + shift;
        let mut base = self.base_mut();
        let mut t = base.get_transform();
        t.origin = origin;
        base.set_transform(t);
    }

    /// The poser, the grips of the weapon it holds, and that weapon's surface
    /// around its grips (only the offline tools ask). Measured now, or with
    /// `reuse`, kept from the last call on the same view with its grips at
    /// the same rests: a search over candidate poses (`try_hand`) holds one
    /// weapon still, so its surface does not change.
    fn held_fields(&self, reuse: bool) -> Option<(Gd<SoldierPoser>, Grips, WeaponFields)> {
        let poser = self.poser.clone()?;
        let grips = self.grips?;
        let view = poser.bind().weapon.clone()?;
        let (right, left) = view
            .clone()
            .try_cast::<crate::viewmodel::WeaponView>()
            .map(|v| v.bind().grip_rests())
            .unwrap_or_default();
        let (right, left) = (right.unwrap_or(grips.right), left.unwrap_or(grips.left));
        if !reuse {
            let fields = WeaponFields::measure(&view, right, left);
            return Some((poser, grips, fields));
        }
        let id = view.instance_id();
        let mut measured = self.measured.borrow_mut();
        let fields = match measured.as_ref() {
            Some((at, r, l, fields)) if (*at, *r, *l) == (id, right, left) => fields.clone(),
            _ => {
                let fields = WeaponFields::measure(&view, right, left);
                *measured = Some((id, right, left, fields.clone()));
                fields
            }
        };
        Some((poser, grips, fields))
    }
}

/// Writes a pose's depths into a dictionary, in millimetres: `finger_depths`
/// (each finger, see [`FpArms::hand_depths`]) and `palm_depth` (right, left).
fn depths_into(out: &mut VarDictionary, depths: &HandDepths) {
    let each = depths.fingers[0]
        .into_iter()
        .chain([depths.index_off])
        .chain(depths.fingers[1])
        .map(|d| d * 1000.0);
    out.set("finger_depths", &PackedFloat32Array::from_iter(each));
    out.set(
        "palm_depth",
        Vector2::new(depths.palms[0], depths.palms[1]) * 1000.0,
    );
    out.set(
        "reach_miss",
        Vector2::new(depths.misses[0], depths.misses[1]) * 1000.0,
    );
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
