//! The combat knife: its model, how a fist holds it, and the strike's path.
//!
//! The model is built in Blender (`tools/build_knife.py`): a clip-point blade
//! with a blackened flat grind and a bright sharpened bevel, a steel guard, a
//! stacked-leather grip, and a pommel. Its frame has the grip centre at the
//! origin, the blade along -Z, and the edge down (-Y).
//!
//! The first-person arms and the third-person soldier both swing it in the
//! support hand (`SoldierPoser`), so the gun stays in the firing hand.

use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::{MeshInstance3D, Node3D};
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
/// The hand drops off the handguard and comes up cocked out to the left at
/// arm's length (not at the lens, where the forearm filled the frame), the
/// blade tipped forward; it slashes forward and across to the right as the
/// strike lands, and follows through low right, still in view for about a
/// fifth of a second, before it falls away and returns to the weapon. Call
/// of Duty's knife reads the same way: the slash crosses the screen and
/// carries on past the target rather than vanishing at the hit.
pub fn first_person(t: f32, strike: f32) -> (f32, Transform3D) {
    let v = Vector3::new;
    let keys: [KnifeKey; 6] = [
        (
            -strike,
            v(-0.14, -0.4, -0.36),
            v(0.1, 0.7, -0.7),
            v(1.0, 0.0, 0.0),
        ),
        (
            -strike * 0.3,
            v(-0.19, -0.1, -0.46),
            v(0.35, 0.75, -0.55),
            v(1.0, -0.1, -0.2),
        ),
        (
            0.0,
            v(-0.02, -0.11, -0.55),
            v(0.55, 0.15, -0.82),
            v(1.0, 0.0, 0.3),
        ),
        (
            0.08,
            v(0.12, -0.16, -0.5),
            v(0.85, -0.15, -0.5),
            v(0.5, -0.6, 0.6),
        ),
        (
            0.18,
            v(0.17, -0.2, -0.47),
            v(0.65, -0.4, -0.65),
            v(0.2, -0.9, 0.4),
        ),
        (
            0.34,
            v(0.08, -0.5, -0.32),
            v(0.3, -0.7, -0.6),
            v(0.0, -1.0, 0.2),
        ),
    ];
    let blend = crate::weapon_clips::keyed(
        &[
            (0.0, 0.0),
            (strike * 0.45, 1.0),
            (strike + 0.24, 1.0),
            (strike + 0.42, 0.0),
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
    let keys: [KnifeKey; 5] = [
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
        (
            0.22,
            v(0.26, -0.16, -0.28),
            v(0.6, -0.45, -0.65),
            v(0.3, -0.85, 0.4),
        ),
    ];
    let blend = crate::weapon_clips::keyed(
        &[
            (0.0, 0.0),
            (0.06, 1.0),
            (STRIKE + 0.22, 1.0),
            (STRIKE + 0.4, 0.0),
        ],
        t,
    );
    (blend, along(&keys, t - STRIKE))
}

/// The knife model, built in Blender by `tools/build_knife.py`.
const MODEL: &str = "res://assets/weapons/knife/knife.glb";

/// Builds the knife, hidden until a strike shows it. The first-person view
/// swaps its materials for the viewmodel shader.
pub fn build() -> Gd<Node3D> {
    let mut root = Node3D::new_alloc();
    root.set_name("Knife");
    root.set_visible(false);
    match try_load::<PackedScene>(MODEL)
        .ok()
        .and_then(|scene| scene.instantiate())
    {
        Some(model) => {
            for mesh in model
                .find_children_ex("*")
                .type_("MeshInstance3D")
                .done()
                .iter_shared()
            {
                if let Ok(mut mesh) = mesh.try_cast::<MeshInstance3D>() {
                    mesh.set_cast_shadows_setting(ShadowCastingSetting::OFF);
                }
            }
            root.add_child(&model);
        }
        None => {
            godot_error!("knife: cannot load {MODEL}");
        }
    }
    root
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
