//! Engine-independent gameplay simulation.
//!
//! Everything here is plain Rust with no Godot types, so the same code runs on
//! the client (for prediction) and on the dedicated server (as the authority),
//! and can be unit-tested without the engine.

pub mod bot;
pub mod camo;
pub mod card;
pub mod care_package;
pub mod challenge;
pub mod chat;
pub mod chopper;
pub mod class;
pub mod cosmetics;
pub mod ctf;
pub mod domination;
pub mod equipment;
pub mod fmath;
pub mod gun_game;
pub mod hardpoint;
pub mod hitbox;
pub mod killcam;
pub mod look;
pub mod melee;
pub mod missile;
pub mod mount;
pub mod movement;
pub mod net;
pub mod penetration;
pub mod perk;
pub mod player;
pub mod progression;
pub mod radar;
pub mod rng;
pub mod scavenger;
pub mod score;
pub mod snd;
pub mod stats;
pub mod streak;
pub mod tdm;
pub mod turret;
pub mod weapon;
pub mod zombies;

pub use glam::{Mat3, Vec2, Vec3};

/// Simulation ticks per second. The engine's physics tick must match
/// (`physics/common/physics_ticks_per_second` in `project.godot`). 120 Hz halves
/// the wait between a button press and the tick that acts on it, compared
/// with 60 Hz. `step` functions take `dt`, so tests may still run at other rates.
pub const TICK_RATE: u32 = 120;

/// Seconds per simulation tick.
pub const TICK_DT: f32 = 1.0 / TICK_RATE as f32;
