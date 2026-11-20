//! Netcode core, engine-free: wire protocol, server input buffering, client
//! prediction and reconciliation, snapshot interpolation, lag compensation,
//! a network condition simulator, and server discovery.
//!
//! Model: the dedicated server is authoritative and runs the same fixed tick
//! as client prediction (`TICK_RATE`), sending snapshots at 60 Hz.
//! Clients send numbered inputs every tick, predict their own player, and
//! reconcile against snapshots. Other players render in the past by as little
//! as the connection allows: two snapshot intervals plus jitter, about 35 to
//! 50 ms on a clean link, wider while snapshots go missing.
//! A shot fired by a click between ticks leaves from the view at the click
//! (`subtick`), and the server rewinds other players to what the shooter was
//! drawing then (`lag`).
//! The transport is Godot's ENet, used only as an unreliable UDP pipe; all
//! serialization is here.

pub mod bits;
pub mod conditioner;
pub mod discovery;
pub mod horde;
pub mod input_queue;
pub mod interpolation;
pub mod lag;
pub mod prediction;
pub mod protocol;
pub mod snapshot_history;
pub mod subtick;

/// Server tick counter. At 120 Hz a `u32` lasts over a year.
pub type Tick = u32;

/// The simulation tick, shared by the server and client prediction.
pub use crate::{TICK_DT, TICK_RATE};
/// UDP port the dedicated server listens on by default.
pub const DEFAULT_PORT: u16 = 27015;
/// ENet channel for snapshots and inputs (sent unreliable, unsequenced).
pub const CHANNEL_UNRELIABLE: i32 = 0;
/// ENet channel for the handshake and game events (sent reliable).
pub const CHANNEL_RELIABLE: i32 = 1;
pub const CHANNEL_COUNT: i32 = 2;
/// Players per server: 6v6.
pub const MAX_PLAYERS: usize = 12;
