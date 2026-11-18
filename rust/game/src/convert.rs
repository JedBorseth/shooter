//! Conversions between Godot math types and the simulation's `glam` types.

use godot::prelude::{Basis, Vector3};
use shooter_sim::Vec3;

pub const fn to_godot(v: Vec3) -> Vector3 {
    Vector3::new(v.x, v.y, v.z)
}

/// A rotation matrix (columns are the basis vectors in both).
pub fn to_godot_basis(m: shooter_sim::Mat3) -> Basis {
    Basis::from_cols(to_godot(m.x_axis), to_godot(m.y_axis), to_godot(m.z_axis))
}

pub fn to_sim(v: Vector3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}
