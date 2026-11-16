//! Godot's ENet as a plain packet pipe.
//!
//! `ENetConnection` gives connect, disconnect, and receive events and sends
//! byte arrays on channels. Nothing above this module touches ENet: messages,
//! ticks, and buffers are all in `shooter_sim::net`.

use godot::classes::enet_connection::EventType;
use godot::classes::enet_packet_peer::PeerStatistic;
use godot::classes::{ENetConnection, ENetPacketPeer};
use godot::prelude::*;
use shooter_sim::net::{CHANNEL_RELIABLE, CHANNEL_UNRELIABLE};

pub enum NetEvent {
    Connect(Gd<ENetPacketPeer>),
    Disconnect(Gd<ENetPacketPeer>),
    /// A packet and the channel it came on.
    Receive(Gd<ENetPacketPeer>, Vec<u8>, i32),
}

/// Drains every pending event without blocking.
pub fn service(host: &mut Gd<ENetConnection>, events: &mut Vec<NetEvent>) {
    // Bounded, so a flood of packets cannot stall a tick forever.
    for _ in 0..4096 {
        let event = host.service_ex().timeout(0).done();
        if event.len() < 2 {
            return;
        }
        let Some(kind) = event
            .at(0)
            .try_to::<i32>()
            .ok()
            .and_then(EventType::try_from_ord)
        else {
            return;
        };
        let peer = event.at(1).try_to::<Gd<ENetPacketPeer>>().ok();
        match (kind, peer) {
            (EventType::CONNECT, Some(peer)) => events.push(NetEvent::Connect(peer)),
            (EventType::DISCONNECT, Some(peer)) => events.push(NetEvent::Disconnect(peer)),
            (EventType::RECEIVE, Some(mut peer)) => {
                let packet = peer.get_packet().to_vec();
                let channel = event.at(3).try_to::<i32>().unwrap_or(CHANNEL_UNRELIABLE);
                events.push(NetEvent::Receive(peer, packet, channel));
            }
            (EventType::NONE | EventType::ERROR, _) => return,
            _ => {}
        }
    }
}

/// Sends unreliable and unsequenced: late snapshots and inputs are worthless,
/// and the protocol already repeats what matters.
pub fn send_unreliable(peer: &mut Gd<ENetPacketPeer>, bytes: &[u8]) {
    let flags = ENetPacketPeer::FLAG_UNSEQUENCED | ENetPacketPeer::FLAG_UNRELIABLE_FRAGMENT;
    peer.send(CHANNEL_UNRELIABLE, &PackedByteArray::from(bytes), flags);
}

/// Sends reliably, for the handshake.
pub fn send_reliable(peer: &mut Gd<ENetPacketPeer>, bytes: &[u8]) {
    peer.send(
        CHANNEL_RELIABLE,
        &PackedByteArray::from(bytes),
        ENetPacketPeer::FLAG_RELIABLE,
    );
}

/// ENet's smoothed round-trip time in milliseconds.
pub fn round_trip_ms(peer: &Gd<ENetPacketPeer>) -> f64 {
    peer.get_statistic(PeerStatistic::ROUND_TRIP_TIME)
}

/// ENet's packet loss estimate, 0 to 1.
pub fn packet_loss(peer: &Gd<ENetPacketPeer>) -> f64 {
    peer.get_statistic(PeerStatistic::PACKET_LOSS) / f64::from(ENetPacketPeer::PACKET_LOSS_SCALE)
}
