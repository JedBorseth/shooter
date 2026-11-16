//! Netcode core, engine-free: wire protocol, server input buffering, client
//! prediction and reconciliation, snapshot interpolation, and a network
//! condition simulator.
//!
//! Model: the dedicated server is authoritative and runs a fixed 60 Hz tick.
//! Clients send numbered inputs every tick, predict their own player, and
//! reconcile against snapshots. Other players render about 100 ms in the past.
//! The transport is Godot's ENet, used only as an unreliable UDP pipe; all
//! serialization is here.

pub mod bits;
pub mod conditioner;
pub mod input_queue;
pub mod interpolation;
pub mod prediction;
pub mod protocol;
pub mod snapshot_history;

/// Server tick counter. At 60 Hz a `u32` lasts over two years.
pub type Tick = u32;

/// Simulation ticks per second, on the server and in client prediction.
pub const TICK_RATE: u32 = 60;
/// Seconds per tick.
pub const TICK_DT: f32 = 1.0 / TICK_RATE as f32;
/// UDP port the dedicated server listens on by default.
pub const DEFAULT_PORT: u16 = 27015;
/// ENet channel for snapshots and inputs (sent unreliable, unsequenced).
pub const CHANNEL_UNRELIABLE: i32 = 0;
/// ENet channel for the handshake (sent reliable).
pub const CHANNEL_RELIABLE: i32 = 1;
pub const CHANNEL_COUNT: i32 = 2;
/// Players per server: 6v6.
pub const MAX_PLAYERS: usize = 12;
