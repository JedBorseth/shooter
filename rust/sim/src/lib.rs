//! Engine-independent gameplay simulation.
//!
//! Everything here is plain Rust with no Godot types, so the same code runs on
//! the client (for prediction) and on the dedicated server (as the authority),
//! and can be unit-tested without the engine.

pub mod look;
pub mod movement;
pub mod player;
pub mod rng;
pub mod weapon;

pub use glam::{Vec2, Vec3};
