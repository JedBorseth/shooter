//! Networking on the engine side: the ENet transport, the dedicated server,
//! server discovery (query port, master heartbeats, the server browser),
//! the client, and remote players. Protocol, prediction, and interpolation
//! logic live in `shooter_sim::net`.

pub mod accounts;
mod bots;
mod class_dict;
mod client;
mod discovery;
pub(crate) mod killcam;
mod match_log;
mod props;
mod remote;
mod server;
mod snd_plan;
mod transport;
