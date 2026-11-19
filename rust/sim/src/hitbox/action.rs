//! Actions that take the support hand off the weapon, keyed the same for the
//! drawn body and its hit zones: a grenade throw, by its progress
//! (`equipment::throw_progress`, `PlayerView::throw`).
//!
//! The third-person throw's path is kept here, engine free, so the soldier
//! the game draws (`grenade_hold::third_person`) and the arm hit zones the
//! server rewinds ([`super::hitboxes`]) follow the same keys: what you see
//! is what you hit, mid-throw too.

use glam::{Mat3, Vec3};

use super::carry::HIP_OFFSET;
use crate::equipment;

/// `keys` (time, value) at `t`, eased between keys (smoothstep), held past
/// the ends.
pub fn keyed(keys: &[(f32, f32)], t: f32) -> f32 {
    let Some(first) = keys.first() else {
        return 0.0;
    };
    if t <= first.0 {
        return first.1;
    }
    for pair in keys.windows(2) {
        let ((t0, v0), (t1, v1)) = (pair[0], pair[1]);
        if t <= t1 {
            let u = ((t - t0) / (t1 - t0).max(1e-4)).clamp(0.0, 1.0);
            return v0 + (v1 - v0) * u * u * (3.0 - 2.0 * u);
        }
    }
    keys.last().map_or(0.0, |k| k.1)
}

/// How far the support hand has left the weapon for the grenade (0..1) at
/// throw progress `p`, in first and third person.
pub fn throw_hand_blend(p: f32) -> f32 {
    keyed(&[(0.0, 0.0), (0.12, 1.0), (0.8, 1.0), (1.0, 0.0)], p)
}

/// How far a third-person body's weapon drops out of the way of a throw
/// (0..1, as a knife swing's; `carry::Carry::swing`) at progress `p`.
pub fn throw_weapon_drop(p: f32) -> f32 {
    keyed(&[(0.0, 0.0), (0.15, 1.0), (0.8, 1.0), (1.0, 0.0)], p)
}

/// The third-person throw's keys: progress, and where the grenade is in the
/// eyes' frame (turned by the view pitch) less the hip carry's offset
/// ([`HIP_OFFSET`]), as `knife::third_person` keys the slash. Big enough to
/// read from 20 m: cocked, the hand is up above the helmet and out behind
/// the shoulder, clear of the body's dark outline; it lets go with the arm
/// reaching high in front.
pub fn third_person_throw_keys() -> [(f32, Vec3); 6] {
    let release = equipment::release_progress();
    let cocked = equipment::COCKED_PROGRESS;
    [
        (0.0, Vec3::new(-0.15, -0.35, -0.1)),
        (cocked, Vec3::new(-0.3, 0.22, 0.16)),
        (cocked + 0.06, Vec3::new(-0.3, 0.26, 0.22)),
        (release, Vec3::new(-0.1, 0.16, -0.48)),
        (release + 0.12, Vec3::new(0.1, -0.2, -0.42)),
        (1.0, Vec3::new(-0.1, -0.4, -0.1)),
    ]
}

/// Where the grenade is along [`third_person_throw_keys`] at progress `p`
/// (same frame), eased between keys as the drawn path is.
pub fn third_person_throw_point(p: f32) -> Vec3 {
    let keys = third_person_throw_keys();
    let (first, last) = (keys[0], keys[keys.len() - 1]);
    if p <= first.0 {
        return first.1;
    }
    if p >= last.0 {
        return last.1;
    }
    let i = keys.windows(2).position(|w| p < w[1].0).unwrap_or(0);
    let (a, b) = (keys[i], keys[i + 1]);
    let u = ((p - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
    a.1.lerp(b.1, u * u * (3.0 - 2.0 * u))
}

/// Where a third-person body's grenade is at throw progress `p`, in body
/// space, for eyes at `eyes` looking `pitch` up: where the drawn support
/// hand holds it.
pub fn third_person_grenade(eyes: Vec3, pitch: f32, p: f32) -> Vec3 {
    eyes + Mat3::from_axis_angle(Vec3::X, pitch) * (HIP_OFFSET + third_person_throw_point(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hand_leaves_the_weapon_and_comes_back() {
        assert_eq!(throw_hand_blend(0.0), 0.0);
        assert_eq!(throw_hand_blend(0.5), 1.0);
        assert_eq!(throw_hand_blend(1.0), 0.0);
        assert!(throw_weapon_drop(0.1) < throw_hand_blend(0.1) + 1e-6);
    }

    #[test]
    fn the_path_passes_through_its_keys() {
        for (p, at) in third_person_throw_keys() {
            assert!(third_person_throw_point(p).distance(at) < 1e-5, "{p}");
        }
        // Cocked above the head and behind; let go in front.
        let eyes = Vec3::new(0.0, 1.6, 0.0);
        let cocked = third_person_grenade(eyes, 0.0, equipment::COCKED_PROGRESS);
        let release = third_person_grenade(eyes, 0.0, equipment::release_progress());
        assert!(cocked.y > eyes.y && cocked.z > eyes.z, "{cocked}");
        assert!(release.z < eyes.z - 0.4, "{release}");
    }
}
