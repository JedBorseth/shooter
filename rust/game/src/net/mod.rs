//! Networking on the engine side: the ENet transport, the dedicated server,
//! server discovery (query port, master heartbeats, the server browser),
//! the client, and remote players. Protocol, prediction, and interpolation
//! logic live in `shooter_sim::net`.

mod bots;
mod class_dict;
mod client;
mod discovery;
mod horde;
mod horde_view;
pub(crate) mod killcam;
mod machines;
mod match_log;
mod props;
mod remote;
mod server;
mod transport;
