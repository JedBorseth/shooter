//! Networking on the engine side: the ENet transport, the dedicated server,
//! server queries (the query port, and asking a server before joining it),
//! the client, and remote players. Protocol, prediction, and interpolation
//! logic live in `shooter_sim::net`.

mod bots;
mod class_dict;
mod client;
pub(crate) mod killcam;
mod match_log;
mod query;
mod remote;
mod server;
mod snd_plan;
mod transport;
