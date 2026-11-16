//! Dedicated server: the authority for every player.
//!
//! Each 60 Hz physics tick it reads client packets, applies each player's
//! queued inputs to that player's body (the same `body::tick` the client
//! predicts with), then sends every client a snapshot: their own state at full
//! precision plus everyone else, delta-compressed against the snapshot that
//! client last acknowledged.
//!
//! Started by `scripts/net/bootstrap.gd` with `-- --server`, or automatically
//! in a dedicated server export.

use std::f32::consts::PI;

use godot::classes::{
    ENetConnection, ENetPacketPeer, Engine, INode3D, Node3D, PackedScene,
    PhysicsRayQueryParameters3D,
};
use godot::global::Error;
use godot::prelude::*;
use shooter_sim::Vec2;
use shooter_sim::net::input_queue::InputQueue;
use shooter_sim::net::protocol::{
    ClientMessage, OwnState, PROTOCOL_VERSION, PlayerView, Predicted, ServerMessage, Snapshot,
};
use shooter_sim::net::snapshot_history::SnapshotHistory;
use shooter_sim::net::{CHANNEL_COUNT, DEFAULT_PORT, MAX_PLAYERS, TICK_DT, TICK_RATE, Tick};
use shooter_sim::player::PlayerParams;
use shooter_sim::player::{self, PlayerInput, PlayerState};
use shooter_sim::rng;
use shooter_sim::weapon::WeaponId;

use super::transport::{self, NetEvent};
use crate::body::{self, BodyParts};
use crate::convert::{to_godot, to_sim};
use crate::pickup::{INTERACT_LAYER, WeaponPickup};
use crate::target::WORLD_LAYER;

/// Map nodes that only make sense on a client, or only offline: the local
/// player and its UI, practice targets, and the sentry turret (it would damage
/// players without the server knowing). Weapon racks stay: they are solid, and
/// players take weapons from them through their input.
const CLIENT_ONLY_NODES: [&str; 5] = ["Player", "HUD", "PauseMenu", "Targets", "Sentry"];

/// Seconds a new connection has to say hello.
const HELLO_TIMEOUT_TICKS: Tick = 5 * TICK_RATE;
/// A little more than the client's weapon rack reach (`FpsPlayer::interact_distance`).
const PICKUP_REACH: f32 = 3.0;
/// Bots get ids from here up, so they never collide with client ids.
const FIRST_BOT_ID: u8 = 200;

#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct NetServer {
    base: Base<Node3D>,

    /// UDP port to listen on.
    #[export]
    #[init(val = i32::from(DEFAULT_PORT))]
    port: i32,
    /// Scripted players to spawn, for testing with one client.
    #[export]
    bots: i32,
    /// Send a snapshot every this many ticks (1 = 60 Hz).
    #[export(range = (1.0, 6.0))]
    #[init(val = 1)]
    snapshot_interval: i32,
    /// The map to load. Client-only nodes (see `CLIENT_ONLY_NODES`) are removed.
    #[export]
    map_scene: Option<Gd<PackedScene>>,
    /// Seconds a dead player waits before respawning.
    #[export]
    #[init(val = 3.0)]
    respawn_delay: f32,

    host: Option<Gd<ENetConnection>>,
    clients: Vec<ClientSlot>,
    avatars: Vec<Avatar>,
    tick: Tick,
    history: SnapshotHistory,
    params: PlayerParams,
    net_events: Vec<NetEvent>,
    spawn: Vector3,
}

struct ClientSlot {
    peer: Gd<ENetPacketPeer>,
    /// Set once the hello is accepted.
    player: Option<u8>,
    inputs: InputQueue,
    /// Newest snapshot the client has acknowledged.
    ack: Option<Tick>,
    connected_at: Tick,
}

struct Avatar {
    id: u8,
    parts: BodyParts,
    predicted: Predicted,
    /// View angles from the last applied input.
    yaw: f32,
    pitch: f32,
    bot: bool,
    spawn: Vector3,
    respawn_in: f32,
}

#[godot_api]
impl INode3D for NetServer {
    fn ready(&mut self) {
        if Engine::singleton().is_editor_hint() {
            return;
        }
        body::check_tick_rate();
        // Headless, nothing paces the main loop; don't spin a core.
        Engine::singleton().set_max_fps(TICK_RATE as i32 * 2);
        self.spawn = Vector3::new(0.0, 0.05, 20.0);
        self.load_map();

        let mut host = ENetConnection::new_gd();
        let error = host
            .create_host_bound_ex("*", self.port)
            .max_peers(MAX_PLAYERS as i32)
            .max_channels(CHANNEL_COUNT)
            .done();
        if error != Error::OK {
            godot_error!("server: cannot listen on UDP port {}: {error:?}", self.port);
            self.base().get_tree().quit_ex().exit_code(1).done();
            return;
        }
        godot_print!("server: listening on UDP port {}", self.port);
        self.host = Some(host);

        for i in 0..self.bots.clamp(0, 50) {
            self.spawn_avatar(FIRST_BOT_ID + i as u8, true);
        }
    }

    fn physics_process(&mut self, _delta: f64) {
        let Some(mut host) = self.host.clone() else {
            return;
        };
        let mut events = std::mem::take(&mut self.net_events);
        events.clear();
        transport::service(&mut host, &mut events);
        for event in events.drain(..) {
            self.handle_event(event);
        }
        self.net_events = events;
        self.drop_silent_peers();

        self.simulate();
        self.tick += 1;
        if self
            .tick
            .is_multiple_of(self.snapshot_interval.max(1) as u32)
        {
            self.broadcast();
        }
        host.flush();
    }
}

#[godot_api]
impl NetServer {
    /// Players currently in the game, bots included.
    #[func]
    fn player_count(&self) -> i32 {
        self.avatars.len() as i32
    }

    #[func]
    fn server_tick(&self) -> i64 {
        i64::from(self.tick)
    }
}

impl NetServer {
    fn load_map(&mut self) {
        let Some(map) = self.map_scene.as_ref().and_then(|s| s.instantiate()) else {
            godot_warn!("server: no map scene set");
            return;
        };
        if let Some(player) = map.get_node_or_null("Player")
            && let Ok(player) = player.try_cast::<Node3D>()
        {
            self.spawn = player.get_position();
        }
        for name in CLIENT_ONLY_NODES {
            if let Some(node) = map.get_node_or_null(name) {
                node.free();
            }
        }
        self.base_mut().add_child(&map);
    }

    fn handle_event(&mut self, event: NetEvent) {
        match event {
            NetEvent::Connect(peer) => {
                godot_print!(
                    "server: connection from {}:{}",
                    peer.get_remote_address(),
                    peer.get_remote_port()
                );
                self.clients.push(ClientSlot {
                    peer,
                    player: None,
                    inputs: InputQueue::default(),
                    ack: None,
                    connected_at: self.tick,
                });
            }
            NetEvent::Disconnect(peer) => {
                let Some(index) = self.clients.iter().position(|c| c.peer == peer) else {
                    return;
                };
                let client = self.clients.swap_remove(index);
                if let Some(id) = client.player {
                    godot_print!("server: player {id} left");
                    self.despawn_avatar(id);
                }
            }
            NetEvent::Receive(peer, bytes, _) => {
                let Some(index) = self.clients.iter().position(|c| c.peer == peer) else {
                    return;
                };
                match ClientMessage::decode(&bytes) {
                    Ok(message) => self.handle_message(index, message),
                    Err(error) => {
                        godot_warn!("server: bad packet: {error}");
                    }
                }
            }
        }
    }

    fn handle_message(&mut self, index: usize, message: ClientMessage) {
        match message {
            ClientMessage::Hello { version } => {
                if self.clients[index].player.is_some() {
                    return;
                }
                let mut peer = self.clients[index].peer.clone();
                if version != PROTOCOL_VERSION {
                    let reject = ServerMessage::Reject {
                        server_version: PROTOCOL_VERSION,
                    };
                    transport::send_reliable(&mut peer, &reject.encode(&[]));
                    peer.peer_disconnect_later();
                    return;
                }
                let Some(id) = (0..MAX_PLAYERS as u8)
                    .find(|id| !self.clients.iter().any(|c| c.player == Some(*id)))
                else {
                    peer.peer_disconnect_later();
                    return;
                };
                self.clients[index].player = Some(id);
                self.spawn_avatar(id, false);
                let welcome = ServerMessage::Welcome {
                    player_id: id,
                    tick: self.tick,
                    snapshot_interval: self.snapshot_interval.clamp(1, 255) as u8,
                };
                transport::send_reliable(&mut peer, &welcome.encode(&[]));
                godot_print!("server: player {id} joined");
            }
            ClientMessage::Input(packet) => {
                let client = &mut self.clients[index];
                if client.player.is_none() {
                    return;
                }
                client.inputs.receive(packet.first_seq, &packet.inputs);
                if let Some(ack) = packet.ack_snapshot
                    && ack <= self.tick
                    && client.ack.is_none_or(|current| ack > current)
                {
                    client.ack = Some(ack);
                }
            }
        }
    }

    /// A connection that never says hello would hold a player slot forever.
    fn drop_silent_peers(&mut self) {
        let tick = self.tick;
        self.clients.retain_mut(|client| {
            let silent = client.player.is_none()
                && tick.saturating_sub(client.connected_at) > HELLO_TIMEOUT_TICKS;
            if silent {
                client.peer.peer_disconnect_now();
            }
            !silent
        });
    }

    fn spawn_avatar(&mut self, id: u8, bot: bool) {
        let slot = f32::from(id % 6) - 2.5;
        let offset = if bot {
            Vector3::new(slot * 3.0, 0.0, -14.0)
        } else {
            Vector3::new(slot * 2.0, 0.0, -f32::from(id / 6) * 2.0)
        };
        let position = self.spawn + offset;
        let mut parts = body::build_body(&format!("Player{id}"));
        self.base_mut().add_child(&parts.body);
        let predicted = Predicted {
            state: PlayerState::spawned(&self.params),
            position: to_sim(position),
            on_floor: false,
        };
        body::place(&mut parts, &self.params.movement, &predicted);
        // Players face down the map (-Z); bots face back toward the spawn.
        let yaw = if bot { PI } else { 0.0 };
        self.avatars.push(Avatar {
            id,
            parts,
            predicted,
            yaw,
            pitch: 0.0,
            bot,
            spawn: position,
            respawn_in: 0.0,
        });
    }

    fn despawn_avatar(&mut self, id: u8) {
        if let Some(index) = self.avatars.iter().position(|a| a.id == id) {
            let avatar = self.avatars.swap_remove(index);
            avatar.parts.body.upcast::<Node>().queue_free();
        }
    }

    /// Applies each player's inputs for this tick.
    fn simulate(&mut self) {
        let mut events = Vec::new();
        let tick = self.tick;
        for avatar in &mut self.avatars {
            let inputs: Vec<PlayerInput> = if avatar.bot {
                vec![bot_input(avatar, tick)]
            } else {
                let Some(client) = self
                    .clients
                    .iter_mut()
                    .find(|c| c.player == Some(avatar.id))
                else {
                    continue;
                };
                client.inputs.take().map(|(_, input)| input).collect()
            };
            for input in &inputs {
                let mut input = *input;
                if let Some(weapon) = input.pickup
                    && !pickup_in_reach(avatar, &input, weapon, &self.params)
                {
                    input.pickup = None;
                }
                let input = &input;
                events.clear();
                // Combat events (shots) are resolved on the server in Phase 3,
                // with lag compensation; for now remote clients see shots
                // through the replicated shot counter.
                body::tick(
                    &mut avatar.parts,
                    &self.params,
                    &mut avatar.predicted,
                    input,
                    &mut events,
                );
                avatar.yaw = input.yaw;
                avatar.pitch = input.pitch;
            }
            if avatar.bot {
                let combat = &mut avatar.predicted.state.combat;
                if combat.weapon().magazine == 0 && combat.weapon().reserve == 0 {
                    combat.refill_ammo(&mut Vec::new());
                }
            }
            if avatar.predicted.state.alive() {
                avatar.respawn_in = self.respawn_delay;
            } else {
                avatar.respawn_in -= TICK_DT;
                if avatar.respawn_in <= 0.0 {
                    avatar.predicted = Predicted {
                        state: PlayerState::spawned(&self.params),
                        position: to_sim(avatar.spawn),
                        on_floor: false,
                    };
                    body::place(&mut avatar.parts, &self.params.movement, &avatar.predicted);
                }
            }
        }
    }

    /// Sends every joined client its snapshot for this tick.
    fn broadcast(&mut self) {
        let mut views: Vec<PlayerView> = self
            .avatars
            .iter()
            .map(|a| PlayerView::from_state(a.id, &a.predicted, a.yaw, a.pitch))
            .collect();
        views.sort_by_key(|v| v.id);
        self.history.insert(self.tick, views.clone());

        for client in &mut self.clients {
            let Some(id) = client.player else { continue };
            let Some(avatar) = self.avatars.iter().find(|a| a.id == id) else {
                continue;
            };
            let others: Vec<PlayerView> = views.iter().filter(|v| v.id != id).copied().collect();
            let baseline: Option<(Tick, Vec<PlayerView>)> = client.ack.and_then(|tick| {
                let views = self.history.get(tick)?;
                Some((tick, views.iter().filter(|v| v.id != id).copied().collect()))
            });
            let snapshot = ServerMessage::Snapshot(Snapshot {
                tick: self.tick,
                baseline: baseline.as_ref().map(|(tick, _)| *tick),
                own: Some(OwnState {
                    last_input: client.inputs.last_applied(),
                    predicted: avatar.predicted,
                }),
                players: others,
            });
            let base = baseline
                .as_ref()
                .map_or(&[][..], |(_, views)| views.as_slice());
            transport::send_unreliable(&mut client.peer, &snapshot.encode(base));
        }
    }
}

/// A scripted player: strafes back and forth, crouches and jumps now and
/// then, and fires bursts toward the spawn, so a single client has something
/// to watch and shoot at.
fn bot_input(avatar: &Avatar, tick: Tick) -> PlayerInput {
    let seconds = tick as f32 / TICK_RATE as f32 + f32::from(avatar.id) * 1.7;
    let period = (seconds / 2.5) as u32;
    let sway = rng::signed(u32::from(avatar.id), period) * 0.4;
    PlayerInput {
        wish: Vec2::new((seconds * 1.3).sin().signum(), 0.0),
        yaw: PI + sway,
        pitch: -0.05,
        jump: tick % 240 == 120 + u32::from(avatar.id % 7) * 10,
        crouch: tick % 480 < 2 || tick % 480 == 240,
        fire: seconds % 4.0 < 0.5,
        ..Default::default()
    }
    .quantized()
}

/// True if the weapon rack the input claims is really under this player's
/// crosshair and within reach, as the client saw it before the move. Without
/// this check, a tampered client could take any weapon from anywhere.
fn pickup_in_reach(
    avatar: &Avatar,
    input: &PlayerInput,
    weapon: WeaponId,
    params: &PlayerParams,
) -> bool {
    let p = &avatar.predicted;
    if !p.state.alive() {
        return false;
    }
    let eye =
        to_godot(p.position) + Vector3::UP * params.movement.eye_height(p.state.movement.stance);
    let direction = to_godot(player::aim_direction(input.yaw, input.pitch, Vec2::ZERO));
    let Some(mut space) = avatar
        .parts
        .body
        .get_world_3d()
        .and_then(|world| world.get_direct_space_state())
    else {
        return false;
    };
    let exclude: Array<Rid> = array![avatar.parts.body.get_rid()];
    let Some(query) = PhysicsRayQueryParameters3D::create_ex(eye, eye + direction * PICKUP_REACH)
        .collision_mask(WORLD_LAYER | INTERACT_LAYER)
        .exclude(&exclude)
        .done()
    else {
        return false;
    };
    let hit = space.intersect_ray(&query);
    hit.get_or_nil("collider")
        .try_to::<Gd<WeaponPickup>>()
        .is_ok_and(|pickup| pickup.bind().weapon_id() == Some(weapon))
}
