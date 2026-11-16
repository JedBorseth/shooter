//! Engine-free maths for posing the character: the shortest rotation between
//! two directions, a basis from two directions, and an analytic two-bone IK
//! solver for the arms. Pure functions on Godot's math types, unit-tested
//! without the engine.

use godot::prelude::{Basis, Quaternion, Vector3};

/// The smallest rotation that turns direction `from` onto direction `to`.
pub fn rotation_between(from: Vector3, to: Vector3) -> Quaternion {
    let (Some(a), Some(b)) = (from.try_normalized(), to.try_normalized()) else {
        return Quaternion::IDENTITY;
    };
    let dot = a.dot(b).clamp(-1.0, 1.0);
    if dot > 1.0 - 1e-6 {
        return Quaternion::IDENTITY;
    }
    if dot < -1.0 + 1e-6 {
        // Opposite: turn half way around any axis perpendicular to `a`.
        let helper = if a.x.abs() < 0.9 {
            Vector3::RIGHT
        } else {
            Vector3::UP
        };
        let axis = a.cross(helper).normalized();
        return Quaternion::from_axis_angle(axis, std::f32::consts::PI);
    }
    let axis = a.cross(b).normalized();
    Quaternion::from_axis_angle(axis, dot.acos())
}

/// An orthonormal basis whose local `+Y` points along `y` and whose local `+Z`
/// points as close to `z` as it can while staying perpendicular to `y`.
pub fn basis_from_y_z(y: Vector3, z: Vector3) -> Basis {
    let y = y.try_normalized().unwrap_or(Vector3::UP);
    let mut z = z - y * z.dot(y);
    if z.length_squared() < 1e-8 {
        z = if y.x.abs() < 0.9 {
            Vector3::RIGHT
        } else {
            Vector3::BACK
        }
        .cross(y);
    }
    let z = z.normalized();
    let x = y.cross(z);
    Basis::from_cols(x, y, z)
}

/// Joint positions of a two-bone chain (shoulder, elbow, wrist), all in the
/// same space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chain {
    pub root: Vector3,
    pub mid: Vector3,
    pub end: Vector3,
}

/// The rotations that bend a [`Chain`] so its end reaches a target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TwoBoneSolution {
    /// Applied to the upper bone's global rotation (and so to everything below it).
    pub upper: Quaternion,
    /// Applied to the lower bone after `upper`.
    pub lower: Quaternion,
    /// Where the elbow ends up.
    pub mid: Vector3,
    /// Where the wrist ends up: the target, or as close as the chain reaches.
    pub end: Vector3,
}

/// Bends `chain` so its end reaches `target`, with the middle joint bending
/// toward `pole`. Bone lengths are kept; a target out of reach leaves the arm
/// straight and pointing at it.
pub fn solve_two_bone(chain: Chain, target: Vector3, pole: Vector3) -> TwoBoneSolution {
    let upper_len = chain.root.distance_to(chain.mid);
    let lower_len = chain.mid.distance_to(chain.end);
    let to_target = target - chain.root;
    let reach_min = (upper_len - lower_len).abs() + 1e-4;
    let reach_max = upper_len + lower_len - 1e-4;
    let distance = to_target
        .length()
        .clamp(reach_min, reach_max.max(reach_min));
    let n = to_target
        .try_normalized()
        .or_else(|| (chain.end - chain.root).try_normalized())
        .unwrap_or(Vector3::DOWN);

    // Bend plane: the pole direction, made perpendicular to the reach direction.
    // Falls back to the current elbow when the pole is in line with the target.
    let bend = |v: Vector3| {
        let p = v - n * v.dot(n);
        p.try_normalized()
    };
    let m = bend(pole - chain.root)
        .or_else(|| bend(chain.mid - chain.root))
        .unwrap_or_else(|| basis_from_y_z(n, Vector3::BACK).col_a());

    let cos_root = ((upper_len * upper_len + distance * distance - lower_len * lower_len)
        / (2.0 * upper_len * distance))
        .clamp(-1.0, 1.0);
    let sin_root = (1.0 - cos_root * cos_root).max(0.0).sqrt();
    let mid = chain.root + (n * cos_root + m * sin_root) * upper_len;
    let end_goal = chain.root + n * distance;

    let upper = rotation_between(chain.mid - chain.root, mid - chain.root);
    let end_after_upper = chain.root + upper * (chain.end - chain.root);
    let lower = rotation_between(end_after_upper - mid, end_goal - mid);
    let end = mid + lower * (end_after_upper - mid);
    TwoBoneSolution {
        upper,
        lower,
        mid,
        end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vector3, b: Vector3) -> bool {
        a.distance_to(b) < 1e-3
    }

    fn arm() -> Chain {
        Chain {
            root: Vector3::new(0.0, 1.4, 0.0),
            mid: Vector3::new(0.3, 1.4, 0.0),
            end: Vector3::new(0.6, 1.4, 0.0),
        }
    }

    #[test]
    fn rotation_between_turns_one_direction_onto_another() {
        let q = rotation_between(Vector3::RIGHT, Vector3::UP);
        assert!(close(q * Vector3::RIGHT, Vector3::UP));
        let q = rotation_between(Vector3::FORWARD, Vector3::BACK);
        assert!(close(q * Vector3::FORWARD, Vector3::BACK));
        assert_eq!(
            rotation_between(Vector3::UP, Vector3::UP),
            Quaternion::IDENTITY
        );
        assert_eq!(
            rotation_between(Vector3::ZERO, Vector3::UP),
            Quaternion::IDENTITY
        );
    }

    #[test]
    fn basis_from_y_z_is_orthonormal() {
        let b = basis_from_y_z(Vector3::new(0.0, 1.0, -1.0), Vector3::new(1.0, 0.0, 0.0));
        assert!(close(b.col_b(), Vector3::new(0.0, 1.0, -1.0).normalized()));
        assert!(close(b.col_c(), Vector3::RIGHT));
        assert!((b.determinant() - 1.0).abs() < 1e-4);
        // Parallel hint still gives a valid basis.
        let b = basis_from_y_z(Vector3::UP, Vector3::UP);
        assert!((b.determinant() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn reaches_a_target_in_range_and_keeps_lengths() {
        let chain = arm();
        let target = Vector3::new(0.25, 1.1, -0.3);
        let pole = Vector3::new(0.3, 0.6, 0.2);
        let s = solve_two_bone(chain, target, pole);
        assert!(close(s.end, target), "end {:?}", s.end);
        assert!((s.mid.distance_to(chain.root) - 0.3).abs() < 1e-4);
        assert!((s.mid.distance_to(s.end) - 0.3).abs() < 1e-4);
        // The rotations reproduce the joint positions.
        assert!(close(
            chain.root + s.upper * (chain.mid - chain.root),
            s.mid
        ));
        let end_after_upper = chain.root + s.upper * (chain.end - chain.root);
        assert!(close(s.mid + s.lower * (end_after_upper - s.mid), s.end));
    }

    #[test]
    fn elbow_bends_toward_the_pole() {
        let chain = arm();
        let target = Vector3::new(0.0, 1.4, -0.4);
        let down = solve_two_bone(chain, target, Vector3::new(0.0, 0.0, -0.2));
        let up = solve_two_bone(chain, target, Vector3::new(0.0, 3.0, -0.2));
        assert!(down.mid.y < 1.3, "elbow down: {:?}", down.mid);
        assert!(up.mid.y > 1.5, "elbow up: {:?}", up.mid);
    }

    #[test]
    fn out_of_reach_points_the_arm_at_the_target() {
        let chain = arm();
        let target = Vector3::new(0.0, 1.4, -2.0);
        let s = solve_two_bone(chain, target, Vector3::new(0.0, 0.0, 0.0));
        assert!((s.end.distance_to(chain.root) - 0.6).abs() < 1e-3);
        assert!(s.end.z < -0.59 && s.end.x.abs() < 1e-2, "{:?}", s.end);
    }

    #[test]
    fn target_at_the_shoulder_does_not_produce_nan() {
        let chain = arm();
        let s = solve_two_bone(chain, chain.root, chain.root);
        assert!(s.mid.is_finite() && s.end.is_finite());
    }
}
