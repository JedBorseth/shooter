//! The combat knife: its model, how a fist holds it, and the strike's path.
//!
//! The model is built here from a few shapes instead of an imported file: a
//! drop-point blade with a blackened flat grind and a bright sharpened bevel,
//! a steel guard, a stacked-leather grip, and a pommel. Its frame has the grip
//! centre at the origin, the blade along -Z, and the edge down (-Y).
//!
//! The first-person arms and the third-person soldier both swing it in the
//! support hand (`SoldierPoser`), so the gun stays in the firing hand.

use std::f32::consts::FRAC_PI_2;

use godot::classes::base_material_3d::ShadingMode;
use godot::classes::mesh::PrimitiveType;
use godot::classes::{
    BoxMesh, CylinderMesh, Mesh, MeshInstance3D, Node3D, StandardMaterial3D, SurfaceTool,
};
use godot::prelude::*;

/// Where the fist holds the grip, in the hand bone's space (+Y toward the
/// knuckles, +Z out of the palm).
pub const GRIP_IN_HAND: Vector3 = Vector3::new(0.0, 0.08, 0.032);

/// Which side of the left hand (in the hand bone's X) the thumb is on: the
/// blade leaves the fist there.
const THUMB_SIDE: f32 = 1.0;

/// The knife's frame in the left hand bone's frame: the grip across the palm,
/// the blade out past the thumb, the edge toward the curled fingers.
pub fn in_left_hand() -> Transform3D {
    // Knife axes in hand space: blade (-Z) along the thumb side, edge (-Y)
    // out of the palm, so +Y (the spine) into the palm.
    let z = Vector3::new(-THUMB_SIDE, 0.0, 0.0);
    let y = Vector3::new(0.0, 0.0, -1.0);
    let x = y.cross(z);
    Transform3D::new(Basis::from_cols(x, y, z), GRIP_IN_HAND)
}

/// The hand bone's transform that puts the knife at `knife` (both in the
/// same space).
pub fn hand_for(knife: Transform3D) -> Transform3D {
    knife * in_left_hand().affine_inverse()
}

/// A knife frame at `position` with the blade pointing along `blade` and the
/// edge facing as close to `edge` as it can.
pub fn frame(position: Vector3, blade: Vector3, edge: Vector3) -> Transform3D {
    let z = -blade.normalized();
    let mut y = -edge;
    y = (y - z * y.dot(z)).normalized();
    let x = y.cross(z);
    Transform3D::new(Basis::from_cols(x, y, z), position)
}

/// One key of a knife path: time in seconds after the strike (negative
/// before it), position, blade direction, and edge direction.
pub type KnifeKey = (f32, Vector3, Vector3, Vector3);

/// The knife's frame along `keys` at `t`, eased between keys.
pub fn along(keys: &[KnifeKey], t: f32) -> Transform3D {
    let (first, last) = (keys[0], keys[keys.len() - 1]);
    let at = |k: &KnifeKey| frame(k.1, k.2, k.3);
    if t <= first.0 {
        return at(&first);
    }
    if t >= last.0 {
        return at(&last);
    }
    let i = keys.windows(2).position(|w| t < w[1].0).unwrap_or(0);
    let (a, b) = (keys[i], keys[i + 1]);
    let u = ((t - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
    let u = u * u * (3.0 - 2.0 * u);
    let (fa, fb) = (at(&a), at(&b));
    let rotation = fa
        .basis
        .get_quaternion()
        .slerp(fb.basis.get_quaternion(), u);
    Transform3D::new(
        Basis::from_quaternion(rotation),
        fa.origin.lerp(fb.origin, u),
    )
}

/// The first-person strike, `t` seconds after it started with the strike
/// landing at `strike`: how far the support hand has left the weapon for the
/// knife (0..1), and the knife's frame in the view (`ViewmodelRig`) space.
///
/// The hand drops off the handguard, comes up cocked out to the left, and
/// slashes forward and across to the right as the strike lands, then falls
/// away and returns to the weapon.
pub fn first_person(t: f32, strike: f32) -> (f32, Transform3D) {
    let v = Vector3::new;
    let keys: [KnifeKey; 5] = [
        (
            -strike,
            v(-0.12, -0.42, -0.3),
            v(0.1, 0.7, -0.7),
            v(1.0, 0.0, 0.0),
        ),
        (
            -strike * 0.3,
            v(-0.17, -0.05, -0.38),
            v(0.25, 0.9, -0.35),
            v(1.0, -0.1, -0.2),
        ),
        (
            0.0,
            v(0.0, -0.1, -0.5),
            v(0.55, 0.2, -0.8),
            v(1.0, 0.0, 0.3),
        ),
        (
            0.08,
            v(0.15, -0.19, -0.44),
            v(0.85, -0.2, -0.45),
            v(0.5, -0.6, 0.6),
        ),
        (
            0.24,
            v(0.08, -0.45, -0.32),
            v(0.4, -0.6, -0.6),
            v(0.0, -1.0, 0.2),
        ),
    ];
    let blend = crate::weapon_clips::keyed(
        &[
            (0.0, 0.0),
            (strike * 0.45, 1.0),
            (strike + 0.16, 1.0),
            (strike + 0.36, 0.0),
        ],
        t,
    );
    (blend, along(&keys, t - strike))
}

/// The third-person strike, `t` seconds in: the support hand's blend onto the
/// knife and the knife's frame in the space of the weapon resting at the hip
/// (its -Z is the muzzle). A shorter, wider version of the first-person
/// slash.
pub fn third_person(t: f32) -> (f32, Transform3D) {
    const STRIKE: f32 = 0.12;
    let v = Vector3::new;
    let keys: [KnifeKey; 4] = [
        (
            -STRIKE,
            v(-0.15, -0.2, 0.1),
            v(0.0, 0.5, -0.9),
            v(1.0, 0.0, 0.0),
        ),
        (
            -0.04,
            v(-0.3, 0.12, -0.05),
            v(0.3, 0.8, -0.5),
            v(0.9, -0.2, 0.3),
        ),
        (
            0.0,
            v(-0.05, 0.05, -0.5),
            v(0.2, 0.1, -1.0),
            v(1.0, -0.1, 0.0),
        ),
        (
            0.12,
            v(0.2, -0.08, -0.35),
            v(0.7, -0.2, -0.7),
            v(0.6, -0.5, 0.6),
        ),
    ];
    let blend = crate::weapon_clips::keyed(
        &[
            (0.0, 0.0),
            (0.06, 1.0),
            (STRIKE + 0.16, 1.0),
            (STRIKE + 0.36, 0.0),
        ],
        t,
    );
    (blend, along(&keys, t - STRIKE))
}

/// Builds the knife, hidden until a strike shows it. The first-person view
/// swaps its materials for the viewmodel shader.
pub fn build() -> Gd<Node3D> {
    let mut root = Node3D::new_alloc();
    root.set_name("Knife");
    root.set_visible(false);

    let coated = material(Color::from_rgb(0.09, 0.09, 0.095), 0.55, 0.5);
    let steel = material(Color::from_rgb(0.72, 0.73, 0.75), 0.95, 0.22);
    let leather = material(Color::from_rgb(0.30, 0.19, 0.11), 0.0, 0.75);
    let dark = material(Color::from_rgb(0.06, 0.06, 0.065), 0.4, 0.55);
    let rubber = material(Color::from_rgb(0.035, 0.035, 0.04), 0.0, 0.9);

    let (flats, bevels) = blade_meshes();
    add(
        &mut root,
        "Blade",
        flats.upcast(),
        &coated,
        Transform3D::IDENTITY,
    );
    add(
        &mut root,
        "Edge",
        bevels.upcast(),
        &steel,
        Transform3D::IDENTITY,
    );

    // The grip lies along Z: cylinders stand on Y, so turn them over.
    let along_z = Basis::from_axis_angle(Vector3::RIGHT, FRAC_PI_2);
    let mut grip = CylinderMesh::new_gd();
    grip.set_top_radius(0.0125);
    grip.set_bottom_radius(0.0135);
    grip.set_height(0.105);
    grip.set_radial_segments(14);
    add(
        &mut root,
        "Grip",
        grip.upcast(),
        &leather,
        Transform3D::new(along_z, Vector3::new(0.0, 0.0, 0.0)),
    );
    // Stacked-leather rings: thin dark spacers along the grip.
    for i in 0..5 {
        let mut ring = CylinderMesh::new_gd();
        ring.set_top_radius(0.0137);
        ring.set_bottom_radius(0.0137);
        ring.set_height(0.003);
        ring.set_radial_segments(14);
        let z = -0.04 + i as f32 * 0.02;
        add(
            &mut root,
            &format!("Ring{i}"),
            ring.upcast(),
            &rubber,
            Transform3D::new(along_z, Vector3::new(0.0, 0.0, z)),
        );
    }
    let mut guard = BoxMesh::new_gd();
    guard.set_size(Vector3::new(0.011, 0.052, 0.007));
    add(
        &mut root,
        "Guard",
        guard.upcast(),
        &dark,
        Transform3D::new(Basis::IDENTITY, Vector3::new(0.0, -0.002, -0.056)),
    );
    let mut pommel = CylinderMesh::new_gd();
    pommel.set_top_radius(0.0145);
    pommel.set_bottom_radius(0.0125);
    pommel.set_height(0.014);
    pommel.set_radial_segments(14);
    add(
        &mut root,
        "Pommel",
        pommel.upcast(),
        &dark,
        Transform3D::new(along_z, Vector3::new(0.0, 0.0, 0.059)),
    );
    root
}

fn material(albedo: Color, metallic: f32, roughness: f32) -> Gd<StandardMaterial3D> {
    let mut m = StandardMaterial3D::new_gd();
    m.set_albedo(albedo);
    m.set_metallic(metallic);
    m.set_roughness(roughness);
    m.set_shading_mode(ShadingMode::PER_PIXEL);
    m
}

fn add(
    root: &mut Gd<Node3D>,
    name: &str,
    mesh: Gd<Mesh>,
    material: &Gd<StandardMaterial3D>,
    transform: Transform3D,
) {
    let mut part = MeshInstance3D::new_alloc();
    part.set_name(name);
    part.set_mesh(&mesh);
    part.set_surface_override_material(0, material);
    part.set_transform(transform);
    root.add_child(&part);
}

/// Blade profile stations from the guard to the tip: distance along the
/// blade (metres, toward -Z), spine height, grind line height, edge height.
const PROFILE: [(f32, f32, f32, f32); 7] = [
    (0.0, 0.012, -0.004, -0.016),
    (0.03, 0.012, -0.004, -0.0165),
    (0.08, 0.0118, -0.0042, -0.017),
    (0.12, 0.0105, -0.0042, -0.0155),
    (0.145, 0.0075, -0.0035, -0.012),
    (0.162, 0.0035, -0.002, -0.0065),
    (0.175, 0.0, 0.0, 0.0),
];
/// Where the blade leaves the guard.
const BLADE_BASE: f32 = -0.059;
/// Half the blade's thickness at the spine.
const HALF_SPINE: f32 = 0.0025;

/// The blade as two meshes: the coated flats and spine, and the bright
/// sharpened bevels that run to the edge.
fn blade_meshes() -> (Gd<Mesh>, Gd<Mesh>) {
    // Per station: spine left and right, grind left and right, edge.
    let station = |i: usize| {
        let (d, spine, grind, edge) = PROFILE[i];
        let z = BLADE_BASE - d;
        // The blade thins toward the tip.
        let t = HALF_SPINE * (1.0 - d / 0.2).max(0.15);
        (
            Vector3::new(-t, spine, z),
            Vector3::new(t, spine, z),
            Vector3::new(-t, grind, z),
            Vector3::new(t, grind, z),
            Vector3::new(0.0, edge, z),
        )
    };
    let mut flats = SurfaceTool::new_gd();
    flats.begin(PrimitiveType::TRIANGLES);
    flats.set_smooth_group(u32::MAX);
    let mut bevels = SurfaceTool::new_gd();
    bevels.begin(PrimitiveType::TRIANGLES);
    bevels.set_smooth_group(u32::MAX);
    let quad = |st: &mut Gd<SurfaceTool>, a: Vector3, b: Vector3, c: Vector3, d: Vector3| {
        // a-b along the near station, c-d the far one; wound to face out.
        for v in [a, c, b, b, c, d] {
            st.add_vertex(v);
        }
    };
    for i in 0..PROFILE.len() - 1 {
        let (sl0, sr0, gl0, gr0, e0) = station(i);
        let (sl1, sr1, gl1, gr1, e1) = station(i + 1);
        // Spine (top), then each flat from spine to grind, then each bevel.
        quad(&mut flats, sr0, sl0, sr1, sl1);
        quad(&mut flats, sl0, gl0, sl1, gl1);
        quad(&mut flats, gr0, sr0, gr1, sr1);
        quad(&mut bevels, gl0, e0, gl1, e1);
        quad(&mut bevels, e0, gr0, e1, gr1);
    }
    // Close the blade at the guard.
    let (sl, sr, gl, gr, e) = station(0);
    for v in [sl, sr, gr, sl, gr, gl, gl, gr, e] {
        flats.add_vertex(v);
    }
    flats.generate_normals();
    bevels.generate_normals();
    (
        flats.commit().expect("blade mesh").upcast(),
        bevels.commit().expect("edge mesh").upcast(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_points_the_blade_and_edge_where_asked() {
        let f = frame(
            Vector3::new(1.0, 2.0, 3.0),
            Vector3::new(0.0, 0.0, -1.0),
            Vector3::new(1.0, 0.0, 0.0),
        );
        let blade = -f.basis.col_c();
        let edge = -f.basis.col_b();
        assert!((blade - Vector3::new(0.0, 0.0, -1.0)).length() < 1e-5);
        assert!((edge - Vector3::new(1.0, 0.0, 0.0)).length() < 1e-5);
        assert!((f.basis.determinant() - 1.0).abs() < 1e-4, "a rotation");
    }

    #[test]
    fn the_hand_for_a_knife_holds_it_there() {
        let knife = frame(
            Vector3::new(0.1, -0.2, -0.4),
            Vector3::new(0.3, 0.2, -1.0),
            Vector3::new(1.0, -0.2, 0.0),
        );
        let back = hand_for(knife) * in_left_hand();
        assert!((back.origin - knife.origin).length() < 1e-5);
        assert!((back.basis.col_c() - knife.basis.col_c()).length() < 1e-4);
        assert!((in_left_hand().basis.determinant() - 1.0).abs() < 1e-4);
    }
}
