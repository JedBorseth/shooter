//! Networking on the engine side: the ENet transport, the dedicated server,
//! server discovery (query port, master heartbeats, the server browser),
//! the client, and remote players. Protocol, prediction, and interpolation
//! logic live in `shooter_sim::net`.

mod client;
mod discovery;
mod remote;
mod server;
mod transport;
