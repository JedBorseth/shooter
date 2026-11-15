//! Conversions between Godot math types and the simulation's `glam` types.

use godot::prelude::Vector3;
use shooter_sim::Vec3;

pub fn to_godot(v: Vec3) -> Vector3 {
    Vector3::new(v.x, v.y, v.z)
}

pub fn to_sim(v: Vector3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}
