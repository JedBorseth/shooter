//! Networking on the engine side: the ENet transport, the dedicated server,
//! the client, and remote players. Protocol, prediction, and interpolation
//! logic live in `shooter_sim::net`.

mod client;
mod remote;
mod server;
mod transport;
