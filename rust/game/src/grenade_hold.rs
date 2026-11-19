//! The frag grenade in the support hand: how the fist holds it, and the
//! throw's path in first and third person.
//!
//! Modern Call of Duty throws equipment with the support hand while the
//! firing hand keeps the weapon, dipped: the hand drops off the handguard and
//! brings the grenade up, the pin comes out, the arm cocks back beside the
//! head (and waits there while the grenade cooks), then comes over and
//! forward, lets go, follows through, and returns to the weapon. Both paths
//! are keyed on the throw's progress (`equipment::throw_progress`), which the
//! simulation, the first-person view, and other players' snapshots share, so
//! everyone sees the grenade leave the hand at the moment it does.

use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::{MeshInstance3D, Node3D};
use godot::prelude::*;
use shooter_sim::equipment::{self, Kind};

use crate::knife::{KnifeKey, along};

/// Where the grenade's centre sits in the left hand bone's space (+Y toward
/// the knuckles, +Z out of the palm): in the cupped palm.
const IN_PALM: Vector3 = Vector3::new(0.0, 0.07, 0.05);

/// The grenade's frame in the left hand bone's frame: in the palm, its fuse
/// (+Y) toward the thumb, the spoon under the fingers.
pub fn in_left_hand() -> Transform3D {
    let y = Vector3::new(1.0, 0.0, 0.0);
    let z = Vector3::new(0.0, 0.0, 1.0);
    let x = y.cross(z);
    Transform3D::new(Basis::from_cols(x, y, z), IN_PALM)
}

/// The hand bone's transform that puts the grenade at `grenade` (both in
/// the same space).
pub fn hand_for(grenade: Transform3D) -> Transform3D {
    grenade * in_left_hand().affine_inverse()
}

/// The progress at which the pin comes out.
const PIN_OUT: f32 = 0.3;

/// How far the support hand has left the weapon for the grenade (0..1) at
/// throw progress `p`.
fn blend(p: f32) -> f32 {
    shooter_sim::hitbox::action::throw_hand_blend(p)
}

/// Keys of a path, by progress: position, the fuse's direction, and where
/// the spoon faces.
fn path(keys: &[KnifeKey], p: f32) -> Transform3D {
    // `knife::frame` points -Z along the "blade"; the grenade's fuse is +Y,
    // so turn the frame to put +Y where the key's direction points.
    let at = along(keys, p);
    let turn = Basis::from_axis_angle(Vector3::RIGHT, -std::f32::consts::FRAC_PI_2);
    Transform3D::new(at.basis * turn, at.origin)
}

/// What a throw at progress `p` looks like: the hand's blend onto the
/// grenade, the grenade's frame, whether it is still in the hand, and
/// whether its pin is still in.
#[derive(Clone, Copy, Debug)]
pub struct ThrowPose {
    pub blend: f32,
    pub at: Transform3D,
    pub in_hand: bool,
    pub pinned: bool,
    pub kind: Kind,
}

/// The hand's blend, whether the grenade is in it, and whether its pin is
/// in, at throw progress `p`.
fn phase(p: f32) -> (f32, bool, bool) {
    (blend(p), p < equipment::release_progress(), p < PIN_OUT)
}

fn pose(keys: &[KnifeKey], p: f32, kind: Kind) -> ThrowPose {
    let (blend, in_hand, pinned) = phase(p);
    ThrowPose {
        blend,
        at: path(keys, p),
        in_hand,
        pinned,
        kind,
    }
}

/// The first-person throw at progress `p`, in the view (`ViewmodelRig`)
/// space: up from below the frame on the left, cocked beside the head, over
/// and out to the front, then down and away.
pub fn first_person(p: f32, kind: Kind) -> ThrowPose {
    let v = Vector3::new;
    let release = equipment::release_progress();
    let cocked = equipment::COCKED_PROGRESS;
    let keys: [KnifeKey; 7] = [
        (
            0.0,
            v(-0.16, -0.44, -0.42),
            v(0.2, 0.9, 0.2),
            v(1.0, 0.0, 0.0),
        ),
        (
            0.25,
            v(-0.17, -0.2, -0.46),
            v(0.35, 0.9, 0.2),
            v(1.0, 0.0, 0.2),
        ),
        (
            cocked,
            v(-0.2, -0.14, -0.44),
            v(0.3, 0.85, 0.4),
            v(1.0, 0.1, 0.0),
        ),
        (
            cocked + 0.06,
            v(-0.25, -0.05, -0.38),
            v(0.2, 0.7, 0.7),
            v(1.0, 0.3, 0.0),
        ),
        // The hand lets go high, beside the line of sight and close in,
        // with the forearm standing up under it (as it does cocked), so the
        // hand and the grenade show as it leaves; the follow-through then
        // carries the arm forward and down. Reached out far in front, the
        // forearm lay along the line of sight and hid both.
        (
            release,
            v(-0.12, 0.06, -0.42),
            v(0.1, 0.85, -0.5),
            v(1.0, -0.1, 0.0),
        ),
        (
            release + 0.1,
            v(-0.03, -0.24, -0.52),
            v(0.3, -0.2, -0.9),
            v(0.6, -0.6, 0.4),
        ),
        (
            1.0,
            v(-0.12, -0.5, -0.42),
            v(0.2, 0.6, -0.5),
            v(1.0, 0.0, 0.0),
        ),
    ];
    pose(&keys, p, kind)
}

/// The third-person throw at progress `p`, in the space of the weapon at
/// the hip as seen from the eyes (as `knife::third_person`): a wider,
/// overhand version of the first-person path. The grenade's positions are
/// `shooter_sim::hitbox::action`'s, which the arm hit zones follow too;
/// these add which way the fuse and the spoon face.
pub fn third_person(p: f32, kind: Kind) -> ThrowPose {
    let v = Vector3::new;
    let turns = [
        (v(0.0, 1.0, 0.0), v(1.0, 0.0, 0.0)),
        (v(0.0, 0.7, 0.7), v(1.0, 0.0, 0.0)),
        (v(0.0, 0.5, 0.9), v(1.0, 0.0, 0.0)),
        (v(0.0, 0.3, -1.0), v(1.0, 0.0, 0.0)),
        (v(0.3, -0.4, -0.8), v(0.6, -0.6, 0.4)),
        (v(0.0, 1.0, 0.0), v(1.0, 0.0, 0.0)),
    ];
    let keys: [KnifeKey; 6] = std::array::from_fn(|i| {
        let (at, at_point) = shooter_sim::hitbox::action::third_person_throw_keys()[i];
        (
            at,
            crate::convert::to_godot(at_point),
            turns[i].0,
            turns[i].1,
        )
    });
    pose(&keys, p, kind)
}

/// How far down the weapon dips while the support hand throws, at progress
/// `p`, as `ViewAction::Lowering` progress: all the way out of the frame in
/// the firing hand, as the reference lowers it, instead of hanging in view
/// with no hand on it.
pub fn weapon_dip(p: f32) -> f32 {
    crate::weapon_clips::keyed(&[(0.0, 0.0), (0.2, 1.0), (0.8, 1.0), (1.0, 0.0)], p)
}

/// Each kind's model, built in Blender by `tools/build_grenade.py`.
pub fn model(kind: Kind) -> &'static str {
    match kind {
        Kind::Frag => "res://assets/equipment/frag/frag.glb",
        Kind::Flash => "res://assets/equipment/flash/flash.glb",
        Kind::Smoke => "res://assets/equipment/smoke/smoke.glb",
    }
}

const KINDS: [Kind; 3] = [Kind::Frag, Kind::Flash, Kind::Smoke];

/// Builds the grenades to carry in the hand: one model of each kind, as
/// children named by kind, all hidden until a throw shows one.
pub fn build() -> Gd<Node3D> {
    let mut root = Node3D::new_alloc();
    root.set_name("HeldGrenade");
    root.set_visible(false);
    for kind in KINDS {
        let path = model(kind);
        let Some(mut model) = try_load::<PackedScene>(path)
            .ok()
            .and_then(|scene| scene.instantiate())
            .and_then(|node| node.try_cast::<Node3D>().ok())
        else {
            godot_error!("grenade: cannot load {path}");
            continue;
        };
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
        model.set_name(kind.name());
        model.set_visible(kind == Kind::Frag);
        root.add_child(&model);
    }
    root
}

/// Shows only `kind`'s model in a grenade built by [`build`], with its pin
/// in or out.
pub fn show(held: &Gd<Node3D>, kind: Kind, pinned: bool) {
    for other in KINDS {
        if let Some(mut node) = held
            .get_node_or_null(other.name())
            .and_then(|node| node.try_cast::<Node3D>().ok())
        {
            let shown = other == kind;
            if node.is_visible() != shown {
                node.set_visible(shown);
            }
            if shown {
                set_pinned(&node, pinned, true);
            }
        }
    }
}

/// Shows or hides the pin (with its ring) on a grenade model, which has it as
/// a child node named `Pin`; the spoon goes with it once the grenade is let go.
pub fn set_pinned(grenade: &Gd<Node3D>, pin: bool, spoon: bool) {
    for (name, shown) in [("Pin", pin), ("Spoon", spoon)] {
        if let Some(mut node) = grenade
            .find_child_ex(name)
            .owned(false)
            .done()
            .and_then(|node| node.try_cast::<Node3D>().ok())
            && node.is_visible() != shown
        {
            node.set_visible(shown);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hand_for_a_grenade_holds_it_there() {
        let at = Transform3D::new(Basis::IDENTITY, Vector3::new(0.1, -0.2, -0.4));
        let back = hand_for(at) * in_left_hand();
        assert!((back.origin - at.origin).length() < 1e-5);
        assert!((in_left_hand().basis.determinant() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn the_grenade_leaves_the_hand_at_the_release() {
        let release = equipment::release_progress();
        let (_, before, pinned) = phase(release - 0.01);
        let (_, after, _) = phase(release + 0.01);
        assert!(before && !after);
        assert!(!pinned, "the pin is out long before");
        assert!(phase(0.1).2, "pinned at the start");
        assert_eq!(phase(1.0).0, 0.0, "back on the weapon");
        assert_eq!(phase(0.5).0, 1.0, "off the weapon mid-throw");
    }

    #[test]
    fn the_weapon_leaves_the_frame_for_the_whole_release() {
        let release = shooter_sim::equipment::release_progress();
        assert_eq!(super::weapon_dip(0.0), 0.0);
        for p in [0.2, 0.5, release, 0.8] {
            assert_eq!(super::weapon_dip(p), 1.0, "fully lowered at {p}");
        }
        assert_eq!(super::weapon_dip(1.0), 0.0, "back up when the throw ends");
    }
}
