//! Iron sights and the reflex optic, built in code.
//!
//! The low-poly gun pack models its sights as solid blocks, which read as slabs
//! when aimed through. `IronSights` cuts those blocks out of the imported mesh and
//! builds proper sights in their place: an aperture and winged post for the AR,
//! a notch and hooded post for the AK, a ghost ring and fiber-optic blade for the
//! shotgun, and three-dot sights for the pistol. The red dot and the ACOG draw
//! their modelled housings (tools/weapon_optics.py) round the glass, dot, and
//! reticle built here, which must line up exactly.
//!
//! Everything is in the weapon view's space: metres, the sight line along -Z at
//! `y = 0`, `+X` right. The rear sight's aperture or notch and the front post's
//! tip both sit on the sight line, so they line up exactly at full ADS.

use std::cell::RefCell;
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

use godot::classes::mesh::{ArrayType, PrimitiveType};
use godot::classes::{
    ArrayMesh, INode3D, Mesh, MeshInstance3D, Node3D, Shader, ShaderMaterial, StandardMaterial3D,
};
use godot::prelude::*;

/// Sight colour: a dark matte phosphate finish, darker than the weapon body so
/// the sight picture reads against it.
const STEEL: Color = Color::from_rgb(0.085, 0.088, 0.095);

/// Radius of the reflex sight's window.
const REFLEX_LENS: f32 = 0.0118;
/// How far ahead of the sight's rear (`rear_z`) its window sits, at size 1.
pub const REFLEX_WINDOW_BEHIND: f32 = 0.006;

/// Builds the sights for one weapon view and removes the model's own.
#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct IronSights {
    base: Base<Node3D>,

    /// `ar`, `ak`, `shotgun`, `pistol`, or `reflex`.
    #[export]
    style: GString,
    /// Rear sight (aperture, notch, or optic lens) position along the sight line.
    #[export]
    rear_z: f32,
    /// Front post position along the sight line.
    #[export]
    #[init(val = -0.4)]
    front_z: f32,
    /// Height of the weapon's top surface under the rear sight.
    #[export]
    #[init(val = -0.02)]
    rear_base: f32,
    /// Height of the barrel or weapon top under the front sight.
    #[export]
    #[init(val = -0.02)]
    front_base: f32,
    /// Scales the sights' cross-section (widths and heights, not positions).
    #[export]
    #[init(val = 1.0)]
    size: f32,
    /// Fiber-optic, tritium, or reticle colour. Alpha 0 means none.
    #[export]
    #[init(val = Color::from_rgba(0.0, 0.0, 0.0, 0.0))]
    glow: Color,
    /// Boxes in this space whose triangles are removed from the imported model:
    /// the model's own sights.
    #[export]
    cuts: Array<Aabb>,
}

impl IronSights {
    /// Where the sights sit: (rear z, height of the top under it, size).
    pub fn placement(&self) -> (f32, f32, f32) {
        (self.rear_z, self.rear_base, self.size)
    }

    /// An optic in `style` ("reflex" or "acog") at `rear_z` on a top at
    /// `base`, not yet in the tree.
    pub fn optic(style: &str, rear_z: f32, base: f32, size: f32, glow: Color) -> Gd<IronSights> {
        let mut sights = IronSights::new_alloc();
        {
            let mut s = sights.bind_mut();
            s.style = GString::from(style);
            s.rear_z = rear_z;
            s.front_z = rear_z;
            s.rear_base = base;
            s.front_base = base;
            s.size = size;
            s.glow = glow;
        }
        sights
    }
}

#[godot_api]
impl INode3D for IronSights {
    fn ready(&mut self) {
        let Some(style) = Style::from_key(&self.style.to_string()) else {
            godot_error!("IronSights: unknown style `{}`", self.style);
            return;
        };
        let Some(parent) = self.base().get_parent_node_3d() else {
            return;
        };
        let world = parent.get("world_model").try_to::<bool>().unwrap_or(false);
        if !self.cuts.is_empty()
            && let Some(model) = parent.try_get_node_as::<Node3D>("Model")
        {
            let boxes: Vec<Aabb> = self.cuts.iter_shared().collect();
            cut_meshes(&model.upcast(), &parent, &boxes);
        }
        let params = Params {
            rear_z: self.rear_z,
            front_z: self.front_z,
            rear_base: self.rear_base,
            front_base: self.front_base,
            size: self.size,
        };
        // Every soldier carrying this weapon shares one mesh and its materials.
        let key = format!("{style:?} {params:?} {:?} {world}", self.glow);
        let meshes = SIGHT_MESHES.with(|cache| cache.borrow().get(&key).cloned());
        let (mesh, body) = meshes.unwrap_or_else(|| {
            let parts = style.build(&params);
            let mut mesh = parts_mesh(&parts, self.glow, world, self.size);
            let mut body = body_mesh(&parts);
            if let Some(model) = style.model() {
                let place = Transform3D::new(
                    Basis::from_scale(Vector3::splat(self.size)),
                    Vector3::new(0.0, 0.0, self.rear_z),
                );
                if world {
                    add_model_surfaces(&mut mesh, model, "World", place);
                } else if style == Style::Acog {
                    // The eye guard frames the aimed picture; the rest hides.
                    add_model_surfaces(&mut mesh, model, "Eyepiece", place);
                    let mut housing = body.unwrap_or_else(ArrayMesh::new_gd);
                    add_model_surfaces(&mut housing, model, "Body", place);
                    body = Some(housing);
                } else {
                    add_model_surfaces(&mut mesh, model, "Body", place);
                }
            }
            let meshes = (mesh, body);
            SIGHT_MESHES.with(|cache| cache.borrow_mut().insert(key, meshes.clone()));
            meshes
        });
        for (name, mesh) in [("SightMesh", Some(mesh)), (SIGHT_BODY, body)] {
            let Some(mesh) = mesh else {
                continue;
            };
            let mut instance = MeshInstance3D::new_alloc();
            instance.set_name(name);
            instance.set_mesh(&mesh);
            if !world {
                instance.set_cast_shadows_setting(
                    godot::classes::geometry_instance_3d::ShadowCastingSetting::OFF,
                );
            }
            self.base_mut().add_child(&instance);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Style {
    Ar,
    Ak,
    Shotgun,
    Pistol,
    Reflex,
    /// A compact prism scope (the ACOG attachment): aimed, the eye sits just
    /// behind its eyepiece and looks down the tube at a lit chevron while
    /// the whole view zooms.
    Acog,
}

impl Style {
    fn from_key(key: &str) -> Option<Self> {
        Some(match key {
            "ar" => Self::Ar,
            "ak" => Self::Ak,
            "shotgun" => Self::Shotgun,
            "pistol" => Self::Pistol,
            "reflex" => Self::Reflex,
            "acog" => Self::Acog,
            _ => return None,
        })
    }

    /// The modelled housing an optic draws round its glass and reticle.
    fn model(self) -> Option<&'static str> {
        match self {
            Self::Reflex => Some(REFLEX_MODEL),
            Self::Acog => Some(ACOG_MODEL),
            _ => None,
        }
    }

    fn build(self, p: &Params) -> Parts {
        let mut parts = Parts::default();
        match self {
            Self::Ar => ar(p, &mut parts),
            Self::Ak => ak(p, &mut parts),
            Self::Shotgun => shotgun(p, &mut parts),
            Self::Pistol => pistol(p, &mut parts),
            Self::Reflex => reflex(p, &mut parts),
            Self::Acog => acog(p, &mut parts),
        }
        parts
    }
}

#[derive(Clone, Copy, Debug)]
struct Params {
    rear_z: f32,
    front_z: f32,
    rear_base: f32,
    front_base: f32,
    size: f32,
}

/// Triangles for each material of the sights.
#[derive(Default)]
struct Parts {
    steel: Builder,
    glow: Builder,
    glass: Builder,
    /// Steel of an optic's body ahead of its eyepiece, built as its own mesh
    /// (`SightBody`) so first person can hide it when fully aimed.
    body: Builder,
}

/// AR-15 style: a flip-up aperture between two protective ears, and an
/// A-frame front sight whose square post stands between two ears.
fn ar(p: &Params, parts: &mut Parts) {
    let s = p.size;
    let (zr, zf) = (p.rear_z, p.front_z);
    let b = &mut parts.steel;

    // Rear: base clamped to the rail, with chamfered ends.
    let top = -0.0105 * s;
    b.extrude(
        Axis::X,
        &[
            v2(zr + 0.016 * s, p.rear_base),
            v2(zr - 0.014 * s, p.rear_base),
            v2(zr - 0.014 * s, top - 0.0035 * s),
            v2(zr - 0.010 * s, top),
            v2(zr + 0.012 * s, top),
            v2(zr + 0.016 * s, top - 0.0035 * s),
        ],
        -0.0145 * s,
        0.0145 * s,
    );
    // The aperture: a ring on a leaf.
    let (r_in, r_out) = (0.0050 * s, 0.0084 * s);
    let leaf = (zr - 0.0016 * s, zr + 0.0016 * s);
    b.ring(Vector2::ZERO, r_in, r_out, leaf.0, leaf.1, 0.0, TAU, 40);
    b.extrude(
        Axis::Z,
        &rect(-0.0064 * s, top, 0.0064 * s, -0.0064 * s),
        leaf.0,
        leaf.1,
    );
    // Protective ears, taller than the aperture, chamfered on the outside.
    for side in [-1.0, 1.0] {
        let ear = [
            v2(0.0098 * s, top),
            v2(0.0138 * s, top),
            v2(0.0138 * s, 0.0),
            v2(0.0116 * s, 0.0040 * s),
            v2(0.0098 * s, 0.0040 * s),
        ];
        b.extrude(
            Axis::Z,
            &mirror_x(&ear, side),
            zr - 0.009 * s,
            zr + 0.009 * s,
        );
    }

    // Front: the A-frame tower on a barrel collar. Its muzzle-side leg stands
    // upright; the rear leg rakes back toward the shooter, with a crossbar
    // between them.
    let fb = p.front_base;
    let deck = -0.0125 * s;
    b.extrude(
        Axis::Z,
        &rect(-0.0085 * s, fb - 0.007 * s, 0.0085 * s, fb + 0.0015 * s),
        zf - 0.009 * s,
        zf + 0.042 * s,
    );
    let leg = 0.0042 * s;
    b.extrude(
        Axis::X,
        &rect(zf - 0.0075 * s, fb, zf + 0.0015 * s, deck),
        -leg,
        leg,
    );
    b.extrude(
        Axis::X,
        &[
            v2(zf + 0.040 * s, fb),
            v2(zf + 0.030 * s, fb),
            v2(zf - 0.001 * s, deck),
            v2(zf + 0.008 * s, deck),
        ],
        -leg,
        leg,
    );
    let bar = fb + 0.45 * (deck - fb);
    b.extrude(
        Axis::X,
        &rect(zf, bar - 0.0035 * s, zf + 0.020 * s, bar + 0.0035 * s),
        -leg * 0.9,
        leg * 0.9,
    );
    // The ears: one U-shaped plate around the post.
    let (inner, outer, ear_top) = (0.0064 * s, 0.0108 * s, 0.0028 * s);
    b.extrude(
        Axis::Z,
        &[
            v2(-outer, deck),
            v2(outer, deck),
            v2(outer, -0.0012 * s),
            v2(outer - 0.0022 * s, ear_top),
            v2(inner, ear_top),
            v2(inner, -0.0075 * s),
            v2(-inner, -0.0075 * s),
            v2(-inner, ear_top),
            v2(-outer + 0.0022 * s, ear_top),
            v2(-outer, -0.0012 * s),
        ],
        zf - 0.0045 * s,
        zf + 0.0045 * s,
    );
    // The post, its tip on the sight line.
    b.extrude(
        Axis::Z,
        &rect(-0.0024 * s, -0.0080 * s, 0.0024 * s, 0.0),
        zf - 0.0016 * s,
        zf + 0.0016 * s,
    );
}

/// AK style: a wide tangent-sight blade with a U notch, and a post inside a
/// round hood that is open at the top.
fn ak(p: &Params, parts: &mut Parts) {
    let s = p.size;
    let (zr, zf) = (p.rear_z, p.front_z);
    let b = &mut parts.steel;

    // Rear: the tangent base, sloping up toward the leaf.
    let blade_bottom = -0.0068 * s;
    b.extrude(
        Axis::X,
        &[
            v2(zr + 0.045 * s, p.rear_base),
            v2(zr - 0.004 * s, p.rear_base),
            v2(zr - 0.004 * s, -0.0125 * s),
            v2(zr + 0.045 * s, p.rear_base + 0.0025 * s),
        ],
        -0.0095 * s,
        0.0095 * s,
    );
    // The leaf rises from the base and carries a wide blade. The front post's
    // tip lines up with the blade's top, filling the U notch.
    let blade = (zr - 0.0028 * s, zr + 0.0004 * s);
    b.extrude(
        Axis::Z,
        &rect(
            -0.0088 * s,
            -0.0125 * s,
            0.0088 * s,
            blade_bottom + 0.0005 * s,
        ),
        blade.0,
        blade.1,
    );
    let (half, notch, depth) = (0.0170 * s, 0.0022 * s, 0.0034 * s);
    b.extrude(
        Axis::Z,
        &[
            v2(-half, blade_bottom),
            v2(half, blade_bottom),
            v2(half, -0.0022 * s),
            v2(half - 0.0022 * s, 0.0),
            v2(notch + 0.0010 * s, 0.0),
            v2(notch, -0.0012 * s),
            v2(notch * 0.8, -depth),
            v2(-notch * 0.8, -depth),
            v2(-notch, -0.0012 * s),
            v2(-notch - 0.0010 * s, 0.0),
            v2(-half + 0.0022 * s, 0.0),
            v2(-half, -0.0022 * s),
        ],
        blade.0,
        blade.1,
    );

    // Front: block on the barrel, a hood open at the top, and the post.
    let fb = p.front_base;
    b.extrude(
        Axis::Z,
        &rect(-0.0055 * s, fb - 0.006 * s, 0.0055 * s, -0.0085 * s),
        zf - 0.012 * s,
        zf + 0.010 * s,
    );
    let center = v2(0.0, -0.0022 * s);
    let gap = 0.42;
    b.ring(
        center,
        0.0064 * s,
        0.0086 * s,
        zf - 0.0045 * s,
        zf + 0.0045 * s,
        PI / 2.0 + gap,
        PI / 2.0 - gap + TAU,
        28,
    );
    b.extrude(
        Axis::Z,
        &rect(-0.0018 * s, -0.0090 * s, 0.0018 * s, 0.0),
        zf - 0.0016 * s,
        zf + 0.0016 * s,
    );
}

/// Tactical shotgun: a ghost ring between ears, and a ramped front blade
/// topped with a fiber-optic rod.
fn shotgun(p: &Params, parts: &mut Parts) {
    let s = p.size;
    let (zr, zf) = (p.rear_z, p.front_z);
    let b = &mut parts.steel;

    let top = -0.0115 * s;
    b.extrude(
        Axis::X,
        &[
            v2(zr + 0.018 * s, p.rear_base),
            v2(zr - 0.016 * s, p.rear_base),
            v2(zr - 0.016 * s, top - 0.003 * s),
            v2(zr - 0.012 * s, top),
            v2(zr + 0.012 * s, top),
            v2(zr + 0.018 * s, top - 0.003 * s),
        ],
        -0.0135 * s,
        0.0135 * s,
    );
    let (r_in, r_out) = (0.0056 * s, 0.0094 * s);
    let leaf = (zr - 0.0022 * s, zr + 0.0022 * s);
    b.ring(Vector2::ZERO, r_in, r_out, leaf.0, leaf.1, 0.0, TAU, 36);
    b.extrude(
        Axis::Z,
        &rect(-0.0075 * s, top, 0.0075 * s, -0.0072 * s),
        leaf.0,
        leaf.1,
    );
    for side in [-1.0, 1.0] {
        let ear = [
            v2(0.0108 * s, top),
            v2(0.0142 * s, top),
            v2(0.0142 * s, 0.0005 * s),
            v2(0.0124 * s, 0.0040 * s),
            v2(0.0108 * s, 0.0040 * s),
        ];
        b.extrude(
            Axis::Z,
            &mirror_x(&ear, side),
            zr - 0.010 * s,
            zr + 0.010 * s,
        );
    }

    // Front: a ramp rising toward the muzzle, then the fiber along its top.
    let fiber_r = 0.0013 * s;
    let blade_top = -2.0 * fiber_r + 0.0004 * s;
    b.extrude(
        Axis::X,
        &[
            v2(zf + 0.016 * s, p.front_base),
            v2(zf - 0.007 * s, p.front_base),
            v2(zf - 0.007 * s, blade_top),
            v2(zf - 0.001 * s, blade_top),
        ],
        -0.0019 * s,
        0.0019 * s,
    );
    parts.glow.cylinder(
        Axis::Z,
        v2(0.0, -fiber_r),
        fiber_r,
        zf - 0.007 * s,
        zf + 0.001 * s,
        12,
    );
}

/// Pistol: a notched rear blade and a front post, each with tritium dots.
fn pistol(p: &Params, parts: &mut Parts) {
    let s = p.size;
    let (zr, zf) = (p.rear_z, p.front_z);
    // The front post's tip is level with the blade's top: equal height,
    // equal light on either side of it in the notch.
    let (half, notch, depth) = (0.0085 * s, 0.0021 * s, 0.0036 * s);
    parts.steel.extrude(
        Axis::Z,
        &[
            v2(-half, p.rear_base),
            v2(half, p.rear_base),
            v2(half, -0.0015 * s),
            v2(half - 0.0015 * s, 0.0),
            v2(notch, 0.0),
            v2(notch, -depth),
            v2(-notch, -depth),
            v2(-notch, 0.0),
            v2(-half + 0.0015 * s, 0.0),
            v2(-half, -0.0015 * s),
        ],
        zr - 0.009 * s,
        zr + 0.0015 * s,
    );
    let dot = 0.00105 * s;
    for side in [-1.0, 1.0] {
        parts.glow.cylinder(
            Axis::Z,
            v2(side * 0.0053 * s, -0.0016 * s),
            dot,
            zr + 0.0015 * s,
            zr + 0.0019 * s,
            12,
        );
    }
    // Front post, ramped toward the muzzle.
    let post = 0.0018 * s;
    parts.steel.extrude(
        Axis::X,
        &[
            v2(zf + 0.0025 * s, p.front_base),
            v2(zf - 0.008 * s, p.front_base),
            v2(zf - 0.0025 * s, 0.0),
            v2(zf + 0.0025 * s, 0.0),
        ],
        -post,
        post,
    );
    parts.glow.cylinder(
        Axis::Z,
        v2(0.0, -0.0022 * s),
        dot,
        zf + 0.0025 * s,
        zf + 0.0029 * s,
        12,
    );
}

/// A riser under a modelled housing, from the rail (`base`) up to the
/// housing's foot, when the rail sits lower than the foot. `z` is the span
/// along the sight line, `half` the half width, all before `size` except
/// `base`.
fn riser(b: &mut Builder, p: &Params, foot: f32, z: (f32, f32), half: f32) {
    let s = p.size;
    if p.rear_base < foot * s - 1e-4 {
        b.extrude(
            Axis::X,
            &rect(
                p.rear_z + z.0 * s,
                p.rear_base,
                p.rear_z + z.1 * s,
                foot * s,
            ),
            -half * s,
            half * s,
        );
    }
}

/// A red dot: the modelled housing (`REFLEX_MODEL`: body, hooded window,
/// emitter, buttons, mount), a faintly tinted lens in its window, and an
/// illuminated dot on the sight line.
fn reflex(p: &Params, parts: &mut Parts) {
    let s = p.size;
    let zr = p.rear_z;
    riser(&mut parts.steel, p, REFLEX_FOOT, (-0.016, 0.0165), 0.0118);
    // The lens fills the hood: one face toward the eye, so the tint is a
    // single layer.
    parts.glass.disc(
        Vector2::ZERO,
        REFLEX_LENS * s,
        zr - REFLEX_WINDOW_BEHIND * s,
        36,
    );
    // The dot sits just behind the lens, so the tint never dims it. About
    // 6 px across aimed: big enough to find, small enough to stay crisp
    // through the bloom.
    parts.glow.cylinder(
        Axis::Z,
        Vector2::ZERO,
        0.00032 * s,
        zr - 0.0056 * s,
        zr - 0.0054 * s,
        16,
    );
}

/// The ACOG-style prism sight: the modelled housing (`ACOG_MODEL`), the
/// eyepiece's glass, and the reticle.
fn acog(p: &Params, parts: &mut Parts) {
    let s = p.size;
    let zr = p.rear_z;
    // Everything ahead of the eyepiece is the body: aimed, it is hidden
    // behind the eyepiece's rim or inside its picture, so first person hides
    // it at full ADS (`Parts::body`) and the eye sees the whole zoomed view.
    // The model's body joins the riser there; its eye guard joins the steel.
    riser(&mut parts.body, p, ACOG_FOOT, (-0.036, 0.026), 0.0125);
    parts
        .glass
        .disc(Vector2::ZERO, ACOG_BORE * s, zr + 0.03 * s, 36);
    // The reticle, just ahead of the eyepiece: a lit chevron whose tip is the
    // aim point, on a thin dark post with three drop marks below it.
    let zt = zr + ACOG_RETICLE * s;
    let (w, h, t) = (0.0011 * s, 0.0015 * s, 0.0003 * s);
    parts.glow.extrude(
        Axis::Z,
        &[
            v2(0.0, 0.0),
            v2(w, -h),
            v2(w - 1.2 * t, -h),
            v2(0.0, -1.9 * t),
            v2(-(w - 1.2 * t), -h),
            v2(-w, -h),
        ],
        zt - 0.0002 * s,
        zt,
    );
    let line = 0.00011 * s;
    let b = &mut parts.steel;
    b.extrude(
        Axis::Z,
        &rect(-line, -0.0095 * s, line, -h - 0.0005 * s),
        zt - 0.0002 * s,
        zt,
    );
    for (drop, half) in [(0.0035, 0.0011), (0.0055, 0.0008), (0.0075, 0.0005)] {
        b.extrude(
            Axis::Z,
            &rect(-half * s, -drop * s - line, half * s, -drop * s + line),
            zt - 0.0002 * s,
            zt,
        );
    }
}

/// The modelled optic housings (tools/weapon_optics.py), in the optic's
/// frame at `size` 1: the sight line along -Z, the origin at `rear_z`.
const REFLEX_MODEL: &str = "res://assets/weapons/optics/reddot.glb";
const ACOG_MODEL: &str = "res://assets/weapons/optics/acog.glb";
/// Where each housing's foot meets the rail, at `size` 1 (`riser` fills
/// down to a lower rail).
const REFLEX_FOOT: f32 = -0.018;
const ACOG_FOOT: f32 = -0.025;

/// Radius of the ACOG's bore and eyepiece glass, before `size`.
pub const ACOG_BORE: f32 = 0.0105;
/// The back of the ACOG's eyepiece, behind `rear_z`, before `size`.
pub const ACOG_EYEPIECE: f32 = 0.034;
/// The ACOG's reticle plane, behind `rear_z`, before `size`.
pub const ACOG_RETICLE: f32 = 0.012;

fn v2(x: f32, y: f32) -> Vector2 {
    Vector2::new(x, y)
}

fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> [Vector2; 4] {
    [v2(x0, y0), v2(x1, y0), v2(x1, y1), v2(x0, y1)]
}

fn mirror_x(points: &[Vector2], side: f32) -> Vec<Vector2> {
    points.iter().map(|p| v2(p.x * side, p.y)).collect()
}

/// The axis a 2D profile is extruded along.
#[derive(Clone, Copy, Debug)]
enum Axis {
    /// Profile in (z, y), extruded across the weapon.
    X,
    /// Profile in (x, y), extruded along the barrel.
    Z,
}

impl Axis {
    /// A profile point at depth `d` along this axis.
    fn point(self, p: Vector2, d: f32) -> Vector3 {
        match self {
            Self::X => Vector3::new(d, p.y, p.x),
            Self::Z => Vector3::new(p.x, p.y, d),
        }
    }

    /// A direction in the profile plane.
    fn dir(self, p: Vector2) -> Vector3 {
        self.point(p, 0.0)
    }

    fn unit(self) -> Vector3 {
        self.point(Vector2::ZERO, 1.0)
    }
}

/// Flat-shaded triangles, wound the way Godot draws front faces: clockwise
/// seen from outside.
#[derive(Default)]
struct Builder {
    vertices: Vec<Vector3>,
    normals: Vec<Vector3>,
}

impl Builder {
    fn tri(&mut self, a: Vector3, b: Vector3, c: Vector3, outward: Vector3) {
        let cross = (b - a).cross(c - a);
        if cross.length_squared() < 1e-20 {
            return;
        }
        // Counter-clockwise from outside means the cross product points out.
        let (b, c) = if cross.dot(outward) > 0.0 {
            (c, b)
        } else {
            (b, c)
        };
        let mut normal = cross.normalized();
        if normal.dot(outward) < 0.0 {
            normal = -normal;
        }
        self.vertices.extend([a, b, c]);
        self.normals.extend([normal; 3]);
    }

    fn quad(&mut self, a: Vector3, b: Vector3, c: Vector3, d: Vector3, outward: Vector3) {
        self.tri(a, b, c, outward);
        self.tri(a, c, d, outward);
    }

    /// A prism: `profile` (a simple polygon, either winding) swept from depth
    /// `d0` to `d1` along `axis`.
    fn extrude(&mut self, axis: Axis, profile: &[Vector2], d0: f32, d1: f32) {
        let (d0, d1) = (d0.min(d1), d0.max(d1));
        let ccw = signed_area(profile) > 0.0;
        for [i, j, k] in triangulate(profile) {
            let (a, b, c) = (profile[i], profile[j], profile[k]);
            self.tri(
                axis.point(a, d0),
                axis.point(b, d0),
                axis.point(c, d0),
                -axis.unit(),
            );
            self.tri(
                axis.point(a, d1),
                axis.point(b, d1),
                axis.point(c, d1),
                axis.unit(),
            );
        }
        for i in 0..profile.len() {
            let (a, b) = (profile[i], profile[(i + 1) % profile.len()]);
            let edge = b - a;
            let out = if ccw {
                v2(edge.y, -edge.x)
            } else {
                v2(-edge.y, edge.x)
            };
            self.quad(
                axis.point(a, d0),
                axis.point(b, d0),
                axis.point(b, d1),
                axis.point(a, d1),
                axis.dir(out),
            );
        }
    }

    /// A ring (or an arc of one, from angle `a0` to `a1`) around the Z axis
    /// through `center`, from depth `z0` to `z1`.
    #[allow(clippy::too_many_arguments)]
    fn ring(
        &mut self,
        center: Vector2,
        r_in: f32,
        r_out: f32,
        z0: f32,
        z1: f32,
        a0: f32,
        a1: f32,
        segments: usize,
    ) {
        let (z0, z1) = (z0.min(z1), z0.max(z1));
        let at = |angle: f32, r: f32, z: f32| {
            Vector3::new(center.x + r * angle.cos(), center.y + r * angle.sin(), z)
        };
        for i in 0..segments {
            let t0 = a0 + (a1 - a0) * i as f32 / segments as f32;
            let t1 = a0 + (a1 - a0) * (i + 1) as f32 / segments as f32;
            let mid = 0.5 * (t0 + t1);
            let radial = Vector3::new(mid.cos(), mid.sin(), 0.0);
            for (z, out) in [(z0, Vector3::FORWARD), (z1, Vector3::BACK)] {
                self.quad(
                    at(t0, r_in, z),
                    at(t1, r_in, z),
                    at(t1, r_out, z),
                    at(t0, r_out, z),
                    out,
                );
            }
            self.quad(
                at(t0, r_out, z0),
                at(t1, r_out, z0),
                at(t1, r_out, z1),
                at(t0, r_out, z1),
                radial,
            );
            self.quad(
                at(t0, r_in, z0),
                at(t1, r_in, z0),
                at(t1, r_in, z1),
                at(t0, r_in, z1),
                -radial,
            );
        }
        if (a1 - a0).abs() < TAU - 1e-4 {
            for (angle, sign) in [(a0, -1.0), (a1, 1.0)] {
                let tangent = Vector3::new(-angle.sin(), angle.cos(), 0.0) * sign;
                self.quad(
                    at(angle, r_in, z0),
                    at(angle, r_out, z0),
                    at(angle, r_out, z1),
                    at(angle, r_in, z1),
                    tangent,
                );
            }
        }
    }

    /// A cylinder along `axis` through `center` (a point in the profile plane).
    fn cylinder(
        &mut self,
        axis: Axis,
        center: Vector2,
        radius: f32,
        d0: f32,
        d1: f32,
        segments: usize,
    ) {
        let profile: Vec<Vector2> = (0..segments)
            .map(|i| {
                let angle = TAU * i as f32 / segments as f32;
                center + v2(angle.cos(), angle.sin()) * radius
            })
            .collect();
        self.extrude(axis, &profile, d0, d1);
    }

    /// A flat disc facing the eye (+Z) at depth `z`.
    fn disc(&mut self, center: Vector2, radius: f32, z: f32, segments: usize) {
        let middle = Vector3::new(center.x, center.y, z);
        for i in 0..segments {
            let at = |i: usize| {
                let angle = TAU * i as f32 / segments as f32;
                middle + Vector3::new(angle.cos(), angle.sin(), 0.0) * radius
            };
            self.tri(middle, at(i), at(i + 1), Vector3::BACK);
        }
    }

    fn arrays(&self) -> VarArray {
        let mut arrays = VarArray::new();
        arrays.resize(ArrayType::MAX.ord() as usize, &Variant::nil());
        arrays.set(
            ArrayType::VERTEX.ord() as usize,
            &PackedVector3Array::from(self.vertices.as_slice()).to_variant(),
        );
        arrays.set(
            ArrayType::NORMAL.ord() as usize,
            &PackedVector3Array::from(self.normals.as_slice()).to_variant(),
        );
        arrays
    }
}

fn signed_area(polygon: &[Vector2]) -> f32 {
    let mut area = 0.0;
    for i in 0..polygon.len() {
        let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
        area += a.x * b.y - b.x * a.y;
    }
    0.5 * area
}

/// Ear-clipping triangulation of a simple polygon (either winding).
fn triangulate(polygon: &[Vector2]) -> Vec<[usize; 3]> {
    let mut remaining: Vec<usize> = (0..polygon.len()).collect();
    if signed_area(polygon) < 0.0 {
        remaining.reverse();
    }
    let cross = |o: Vector2, a: Vector2, b: Vector2| (a - o).cross(b - o);
    let mut triangles = Vec::with_capacity(polygon.len().saturating_sub(2));
    while remaining.len() > 3 {
        let n = remaining.len();
        let ear = (0..n).find(|&i| {
            let (ia, ib, ic) = (
                remaining[(i + n - 1) % n],
                remaining[i],
                remaining[(i + 1) % n],
            );
            let (a, b, c) = (polygon[ia], polygon[ib], polygon[ic]);
            if cross(a, b, c) <= 0.0 {
                return false;
            }
            !remaining.iter().any(|&j| {
                if j == ia || j == ib || j == ic {
                    return false;
                }
                let p = polygon[j];
                cross(a, b, p) >= 0.0 && cross(b, c, p) >= 0.0 && cross(c, a, p) >= 0.0
            })
        });
        // A degenerate polygon has no ear: clip anything rather than loop forever.
        let i = ear.unwrap_or(0);
        triangles.push([
            remaining[(i + n - 1) % n],
            remaining[i],
            remaining[(i + 1) % n],
        ]);
        remaining.remove(i);
    }
    if remaining.len() == 3 {
        triangles.push([remaining[0], remaining[1], remaining[2]]);
    }
    triangles
}

/// One mesh with a surface per material. The materials are standard ones, which
/// the weapon view converts for first-person drawing; the lens is a shader of its
/// own because it is transparent.
fn parts_mesh(parts: &Parts, glow: Color, world: bool, size: f32) -> Gd<ArrayMesh> {
    let mut mesh = ArrayMesh::new_gd();
    let mut add = |builder: &Builder, material: Gd<godot::classes::Material>| {
        if builder.vertices.is_empty() {
            return;
        }
        mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &builder.arrays());
        let index = mesh.get_surface_count() - 1;
        mesh.surface_set_material(index, &material);
    };

    add(&parts.steel, steel_material());

    if glow.a > 0.0 {
        let mut lit = StandardMaterial3D::new_gd();
        lit.set_albedo(glow);
        lit.set_feature(godot::classes::base_material_3d::Feature::EMISSION, true);
        lit.set_emission(glow);
        lit.set_emission_energy_multiplier(6.0);
        add(&parts.glow, lit.upcast());
    }

    if !parts.glass.vertices.is_empty() {
        let mut glass = ShaderMaterial::new_gd();
        glass.set_shader(&load::<Shader>("res://shaders/sight_glass.gdshader"));
        glass.set_shader_parameter("viewmodel", &(!world).to_variant());
        glass.set_shader_parameter("radius", &(REFLEX_LENS * size).to_variant());
        add(&parts.glass, glass.upcast());
    }
    mesh
}

/// Adds the surfaces of `node` in the model at `path` to `mesh`, moved by
/// `place`, with their own materials. A uniform scale and a shift leave
/// normals and tangents as they are.
fn add_model_surfaces(mesh: &mut Gd<ArrayMesh>, path: &str, node: &str, place: Transform3D) {
    let Some(source) = model_mesh(path, node) else {
        godot_error!("IronSights: no `{node}` in {path}");
        return;
    };
    for surface in 0..source.get_surface_count() {
        let mut arrays = source.surface_get_arrays(surface);
        let vertices: PackedVector3Array = arrays
            .at(ArrayType::VERTEX.ord() as usize)
            .try_to()
            .unwrap_or_default();
        let moved: Vec<Vector3> = vertices.as_slice().iter().map(|v| place * *v).collect();
        arrays.set(
            ArrayType::VERTEX.ord() as usize,
            &PackedVector3Array::from(moved.as_slice()).to_variant(),
        );
        mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);
        if let Some(material) = source.surface_get_material(surface) {
            let index = mesh.get_surface_count() - 1;
            mesh.surface_set_material(index, &material);
        }
    }
}

/// The mesh of the node `node` in the imported model at `path`.
fn model_mesh(path: &str, node: &str) -> Option<Gd<Mesh>> {
    let root = try_load::<PackedScene>(path).ok()?.instantiate()?;
    let mesh = root
        .find_child(node)
        .and_then(|n| n.try_cast::<MeshInstance3D>().ok())
        .and_then(|m| m.get_mesh());
    root.free();
    mesh
}

/// The node holding an optic's body ahead of its eyepiece (`Parts::body`).
pub const SIGHT_BODY: &str = "SightBody";

/// An optic's body, in the sights' steel, or `None` if it has none.
fn body_mesh(parts: &Parts) -> Option<Gd<ArrayMesh>> {
    if parts.body.vertices.is_empty() {
        return None;
    }
    let mut mesh = ArrayMesh::new_gd();
    mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &parts.body.arrays());
    mesh.surface_set_material(0, &steel_material());
    Some(mesh)
}

fn steel_material() -> Gd<godot::classes::Material> {
    let mut steel = StandardMaterial3D::new_gd();
    steel.set_albedo(STEEL);
    steel.set_roughness(0.55);
    steel.set_metallic(0.35);
    steel.upcast()
}

/// A built sight's mesh and its optic body's.
type SightMeshes = (Gd<ArrayMesh>, Option<Gd<ArrayMesh>>);

thread_local! {
    /// Built sights, keyed by everything that shapes them.
    static SIGHT_MESHES: RefCell<HashMap<String, SightMeshes>> = RefCell::new(HashMap::new());
    /// Cut copies of imported meshes, keyed by the source mesh and the cut boxes,
    /// so every soldier carrying a weapon shares one.
    static CUT_MESHES: RefCell<HashMap<(InstanceId, String), Gd<ArrayMesh>>> =
        RefCell::new(HashMap::new());
}

pub fn clear_cache() {
    SIGHT_MESHES.with_borrow_mut(|cache| cache.clear());
    CUT_MESHES.with_borrow_mut(|cache| cache.clear());
}

/// Removes every triangle whose centre lies inside one of `boxes` from the
/// meshes under `root`. The boxes are in `space`'s coordinates.
fn cut_meshes(root: &Gd<Node>, space: &Gd<Node3D>, boxes: &[Aabb]) {
    for child in root.get_children().iter_shared() {
        if let Ok(mut instance) = child.clone().try_cast::<MeshInstance3D>()
            && let Some(source) = instance.get_mesh()
        {
            let to_space = relative_transform(&instance.clone().upcast(), space);
            let key = (source.instance_id(), format!("{boxes:?}"));
            let cut = CUT_MESHES.with(|cache| cache.borrow().get(&key).cloned());
            let cut = cut.unwrap_or_else(|| {
                let mesh = cut_mesh(&source, to_space, boxes);
                CUT_MESHES.with(|cache| cache.borrow_mut().insert(key, mesh.clone()));
                mesh
            });
            instance.set_mesh(&cut);
        }
        cut_meshes(&child, space, boxes);
    }
}

fn relative_transform(node: &Gd<Node3D>, ancestor: &Gd<Node3D>) -> Transform3D {
    let mut transform = Transform3D::IDENTITY;
    let mut current = Some(node.clone());
    while let Some(n) = current {
        if n == *ancestor {
            break;
        }
        transform = n.get_transform() * transform;
        current = n.get_parent_node_3d();
    }
    transform
}

fn cut_mesh(source: &Gd<Mesh>, to_space: Transform3D, boxes: &[Aabb]) -> Gd<ArrayMesh> {
    let mut mesh = ArrayMesh::new_gd();
    for surface in 0..source.get_surface_count() {
        let mut arrays = source.surface_get_arrays(surface);
        let vertices: PackedVector3Array = arrays
            .at(ArrayType::VERTEX.ord() as usize)
            .try_to()
            .unwrap_or_default();
        let indices: PackedInt32Array = arrays
            .at(ArrayType::INDEX.ord() as usize)
            .try_to()
            .unwrap_or_default();
        let indices: Vec<i32> = if indices.is_empty() {
            (0..vertices.len() as i32).collect()
        } else {
            indices.to_vec()
        };
        let vertices = vertices.as_slice();
        // A triangle goes when its centre is inside a box: the sides of a sight
        // block go with it, while a face it stands on stays.
        let inside = |t: &[i32]| {
            let centre = t
                .iter()
                .map(|&i| to_space * vertices[i as usize])
                .fold(Vector3::ZERO, |sum, p| sum + p)
                / 3.0;
            boxes.iter().any(|b| b.contains_point(centre))
        };
        let (triangles, _) = indices.as_chunks::<3>();
        let kept: Vec<i32> = triangles
            .iter()
            .filter(|t| !inside(&t[..]))
            .flatten()
            .copied()
            .collect();
        arrays.set(
            ArrayType::INDEX.ord() as usize,
            &PackedInt32Array::from(kept.as_slice()).to_variant(),
        );
        mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);
        if let Some(material) = source.surface_get_material(surface) {
            mesh.surface_set_material(surface, &material);
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Volume enclosed by a closed mesh; negative when every face is wound
    /// clockwise seen from outside.
    fn volume(b: &Builder) -> f32 {
        let (triangles, _) = b.vertices.as_chunks::<3>();
        triangles
            .iter()
            .map(|[a, b, c]| a.dot(b.cross(*c)) / 6.0)
            .sum()
    }

    #[test]
    fn box_is_closed_and_wound_clockwise() {
        let mut b = Builder::default();
        b.extrude(Axis::Z, &rect(0.0, 0.0, 2.0, 3.0), -1.0, 3.0);
        assert!((volume(&b) + 24.0).abs() < 1e-4, "{}", volume(&b));
        let mut b = Builder::default();
        b.extrude(Axis::X, &rect(0.0, 0.0, 2.0, 3.0), 0.0, 1.0);
        assert!((volume(&b) + 6.0).abs() < 1e-4, "{}", volume(&b));
    }

    #[test]
    fn winding_of_the_profile_does_not_matter() {
        let mut forward = Builder::default();
        let mut reversed = Builder::default();
        let u = [
            v2(-3.0, 0.0),
            v2(3.0, 0.0),
            v2(3.0, 3.0),
            v2(1.0, 3.0),
            v2(1.0, 1.0),
            v2(-1.0, 1.0),
            v2(-1.0, 3.0),
            v2(-3.0, 3.0),
        ];
        let mut rev = u;
        rev.reverse();
        forward.extrude(Axis::Z, &u, 0.0, 1.0);
        reversed.extrude(Axis::Z, &rev, 0.0, 1.0);
        // A U: 6x3 minus the 2x2 slot.
        assert!((volume(&forward) + 14.0).abs() < 1e-4);
        assert!((volume(&reversed) + 14.0).abs() < 1e-4);
    }

    #[test]
    fn ring_encloses_the_annulus() {
        let mut b = Builder::default();
        b.ring(Vector2::ZERO, 1.0, 2.0, 0.0, 1.0, 0.0, TAU, 256);
        assert!((volume(&b) + 3.0 * PI).abs() < 0.01, "{}", volume(&b));
        let mut half = Builder::default();
        half.ring(Vector2::ZERO, 1.0, 2.0, 0.0, 1.0, 0.0, PI, 128);
        assert!((volume(&half) + 1.5 * PI).abs() < 0.01, "{}", volume(&half));
    }

    #[test]
    fn every_style_builds_closed_solids_on_the_sight_line() {
        let params = Params {
            rear_z: 0.0,
            front_z: -0.4,
            rear_base: -0.03,
            front_base: -0.03,
            size: 1.0,
        };
        for style in [
            Style::Ar,
            Style::Ak,
            Style::Shotgun,
            Style::Pistol,
            Style::Reflex,
        ] {
            let parts = style.build(&params);
            assert!(volume(&parts.steel) < 0.0, "{style:?}");
            // Nothing pokes far above the sight line except ears and hoods.
            let highest = parts
                .steel
                .vertices
                .iter()
                .map(|v| v.y)
                .fold(f32::MIN, f32::max);
            assert!(highest < 0.016, "{style:?} {highest}");
        }
    }

    #[test]
    fn front_post_tips_sit_on_the_sight_line() {
        let params = Params {
            rear_z: 0.0,
            front_z: -0.4,
            rear_base: -0.03,
            front_base: -0.03,
            size: 1.0,
        };
        for style in [Style::Ar, Style::Ak, Style::Pistol] {
            let parts = style.build(&params);
            // The highest point within the post's width at the front sight.
            let tip = parts
                .steel
                .vertices
                .iter()
                .filter(|v| v.x.abs() < 0.0025 && (v.z + 0.4).abs() < 0.01)
                .map(|v| v.y)
                .fold(f32::MIN, f32::max);
            assert!(tip.abs() < 1e-6, "{style:?} {tip}");
        }
    }
}
