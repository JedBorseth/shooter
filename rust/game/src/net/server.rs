//! Dedicated server: the authority for every player.
//!
//! Each physics tick (`TICK_RATE`) it reads client packets, applies each player's
//! queued inputs to that player's body (the same `body::tick` the client
//! predicts with), resolves the shots those inputs fired, then sends every
//! client a snapshot: their own state at full precision plus everyone else,
//! delta-compressed against the snapshot that client last acknowledged.
//!
//! Shots are checked with lag compensation (`shooter_sim::net::lag`): against
//! every other player's hitboxes as the shooter saw them, rebuilt from the
//! snapshots already sent, and against the map's walls in the present. The
//! server applies the damage, keeps score, and tells the players involved
//! through reliable [`GameEvent`]s.
//!
//! It also answers server browser queries on its query port (the game port
//! plus one) and, given a master server, keeps itself listed there
//! (`super::discovery`).
//!
//! Started by `scripts/net/bootstrap.gd` with `-- --server`, or automatically
//! in a dedicated server export.

use std::f32::consts::PI;

use godot::classes::{
    ENetConnection, ENetPacketPeer, Engine, INode3D, Node3D, PackedScene,
    PhysicsDirectSpaceState3D, PhysicsRayQueryParameters3D,
};
use godot::global::Error;
use godot::prelude::*;
use shooter_sim::net::discovery::{ServerInfo, query_port_for};
use shooter_sim::net::input_queue::InputQueue;
use shooter_sim::net::lag::{self, LagHistory, Pellet, Target};
use shooter_sim::net::protocol::{
    ClientMessage, GameEvent, MAX_EVENTS_PER_MESSAGE, OwnState, PROTOCOL_VERSION, PlayerView,
    Predicted, ServerMessage, Snapshot,
};
use shooter_sim::net::snapshot_history::SnapshotHistory;
use shooter_sim::net::{CHANNEL_COUNT, DEFAULT_PORT, MAX_PLAYERS, TICK_DT, TICK_RATE, Tick};
use shooter_sim::player::{self, PlayerEvent, PlayerInput, PlayerParams, PlayerState};
use shooter_sim::score::{Kill, ScoreRules, ScoreState};
use shooter_sim::weapon::{self, CombatEvent, WeaponId};
use shooter_sim::{Vec2, rng};

use super::discovery::QueryResponder;
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
    /// Send a snapshot every this many ticks (2 = 60 Hz at the 120 Hz tick).
    #[export(range = (1.0, 8.0))]
    #[init(val = 2)]
    snapshot_interval: i32,
    /// The map to load. Client-only nodes (see `CLIENT_ONLY_NODES`) are removed.
    #[export]
    map_scene: Option<Gd<PackedScene>>,
    /// Seconds a dead player waits before respawning.
    #[export]
    #[init(val = 3.0)]
    respawn_delay: f32,
    /// Whether bots' bursts can hurt players. Off by default, so a test client
    /// is not shot at random.
    #[export]
    bots_deal_damage: bool,
    /// Name shown in server browsers.
    #[export]
    #[init(val = GString::from("Shooter server"))]
    server_name: GString,
    /// Map name shown in server browsers.
    #[export]
    #[init(val = GString::from("Arena"))]
    map_name: GString,
    /// UDP port that answers server browser queries; 0 means `port + 1`, and
    /// -1 turns queries off.
    #[export]
    query_port: i32,
    /// Master server (`host[:port]`) to stay listed on, or empty for LAN and
    /// direct connects only.
    #[export]
    master_server: GString,

    host: Option<Gd<ENetConnection>>,
    clients: Vec<ClientSlot>,
    avatars: Vec<Avatar>,
    tick: Tick,
    history: SnapshotHistory,
    /// Every snapshot's players for the last second, for lag compensation.
    lag: LagHistory,
    params: PlayerParams,
    score_rules: ScoreRules,
    net_events: Vec<NetEvent>,
    spawn: Vector3,
    responder: Option<QueryResponder>,
}

struct ClientSlot {
    peer: Gd<ENetPacketPeer>,
    /// Set once the hello is accepted.
    player: Option<u8>,
    inputs: InputQueue,
    /// Newest snapshot the client has acknowledged.
    ack: Option<Tick>,
    connected_at: Tick,
    /// The newest view tick the client sent, for shots fired by inputs
    /// without one.
    view_tick: Option<f64>,
    /// The client asked for the rewound hitboxes of its shots (F4 overlay).
    debug_rewound: bool,
    /// Game events to send reliably at the end of the tick.
    outbox: Vec<GameEvent>,
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
    /// Tick of the latest spawn: shots rewound to before it cannot hit this life.
    spawned_at: Tick,
    score: ScoreState,
}

/// A shot fired this tick, waiting to be checked.
struct Shot {
    shooter: u8,
    /// Eye position after the input that fired, as the client computes it.
    eye: Vector3,
    yaw: f32,
    pitch: f32,
    weapon: WeaponId,
    aim_offset: Vec2,
    shot: u32,
    view_tick: Option<f64>,
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
        self.start_query_responder();
    }

    fn exit_tree(&mut self) {
        if let Some(responder) = &mut self.responder {
            responder.goodbye();
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
        self.send_events();
        host.flush();

        let players = self.clients.iter().filter(|c| c.player.is_some()).count();
        let bots = self.avatars.iter().filter(|a| a.bot).count();
        if let Some(responder) = &mut self.responder {
            responder.info.players = players.min(255) as u8;
            responder.info.bots = bots.min(255) as u8;
            responder.poll();
        }
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

    /// Match score of a player (tests and debugging), or -1 if absent.
    #[func]
    fn player_score(&self, id: i32) -> i64 {
        self.avatars
            .iter()
            .find(|a| i32::from(a.id) == id)
            .map_or(-1, |a| i64::from(a.score.score))
    }
}

impl NetServer {
    fn start_query_responder(&mut self) {
        let Ok(game_port) = u16::try_from(self.port) else {
            return;
        };
        let query_port = match self.query_port {
            0 => query_port_for(game_port),
            port => match u16::try_from(port) {
                Ok(port) => port,
                Err(_) => return,
            },
        };
        let info = ServerInfo {
            instance: QueryResponder::new_instance_id(),
            protocol: PROTOCOL_VERSION,
            game_port,
            name: self.server_name.to_string(),
            map: self.map_name.to_string(),
            mode: "Free-for-all".to_owned(),
            players: 0,
            max_players: MAX_PLAYERS as u8,
            bots: 0,
        };
        match QueryResponder::bind(query_port, info, &self.master_server.to_string()) {
            Ok(responder) => {
                godot_print!(
                    "server: \"{}\" answers server browsers on UDP port {query_port}",
                    responder.info.name
                );
                self.responder = Some(responder);
            }
            // The game still works without it; only browsers cannot see it.
            Err(error) => {
                godot_warn!(
                    "server: cannot answer server browsers on UDP port {query_port}: {error}"
                );
            }
        }
    }

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
                    view_tick: None,
                    debug_rewound: false,
                    outbox: Vec::new(),
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
            ClientMessage::Debug { rewound_hitboxes } => {
                self.clients[index].debug_rewound = rewound_hitboxes;
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
            spawned_at: self.tick,
            score: ScoreState::default(),
        });
    }

    fn despawn_avatar(&mut self, id: u8) {
        if let Some(index) = self.avatars.iter().position(|a| a.id == id) {
            let avatar = self.avatars.swap_remove(index);
            avatar.parts.body.upcast::<Node>().queue_free();
        }
    }

    /// Applies each player's inputs for this tick, then resolves the shots
    /// they fired. Shots fired in the same tick all count, so two players who
    /// fire at each other in the same tick can trade kills.
    fn simulate(&mut self) {
        let mut events = Vec::new();
        let mut shots = Vec::new();
        let mut fall_deaths = Vec::new();
        let tick = self.tick;
        for avatar in &mut self.avatars {
            let inputs: Vec<(PlayerInput, Option<f64>)> = if avatar.bot {
                vec![(bot_input(avatar, tick), None)]
            } else {
                let Some(client) = self
                    .clients
                    .iter_mut()
                    .find(|c| c.player == Some(avatar.id))
                else {
                    continue;
                };
                let inputs: Vec<PlayerInput> = client.inputs.take().map(|(_, i)| i).collect();
                inputs
                    .into_iter()
                    .map(|input| {
                        if input.view_tick.is_some() {
                            client.view_tick = input.view_tick;
                        }
                        (input, client.view_tick)
                    })
                    .collect()
            };
            for (input, view_tick) in &inputs {
                let mut input = *input;
                if let Some(weapon) = input.pickup
                    && !pickup_in_reach(avatar, &input, weapon, &self.params)
                {
                    input.pickup = None;
                }
                let input = &input;
                events.clear();
                body::tick(
                    &mut avatar.parts,
                    &self.params,
                    &mut avatar.predicted,
                    input,
                    &mut events,
                );
                avatar.yaw = input.yaw;
                avatar.pitch = input.pitch;
                for event in &events {
                    match *event {
                        PlayerEvent::Combat(CombatEvent::Fired {
                            weapon,
                            aim_offset,
                            shot,
                        }) => {
                            let p = &avatar.predicted;
                            let eye = to_godot(p.position)
                                + Vector3::UP
                                    * self.params.movement.eye_height(p.state.movement.stance);
                            shots.push(Shot {
                                shooter: avatar.id,
                                eye,
                                yaw: input.yaw,
                                pitch: input.pitch,
                                weapon,
                                aim_offset,
                                shot,
                                view_tick: *view_tick,
                            });
                        }
                        // Only falls kill inside the step; shots are resolved below.
                        PlayerEvent::Died => fall_deaths.push(avatar.id),
                        _ => {}
                    }
                }
            }
            avatar.score.tick(TICK_DT);
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
                    avatar.spawned_at = tick;
                    body::place(&mut avatar.parts, &self.params.movement, &avatar.predicted);
                }
            }
        }
        for id in fall_deaths {
            self.record_death(id);
        }
        for shot in shots {
            self.resolve_shot(&shot);
        }
    }

    /// Checks one shot against the map now and the other players as the
    /// shooter saw them, then applies its damage.
    fn resolve_shot(&mut self, shot: &Shot) {
        let Some(shooter) = self.avatars.iter().find(|a| a.id == shot.shooter) else {
            return;
        };
        if shooter.bot && !self.bots_deal_damage {
            return;
        }
        let Some(mut space) = shooter
            .parts
            .body
            .get_world_3d()
            .and_then(|world| world.get_direct_space_state())
        else {
            return;
        };
        let def = shot.weapon.def();
        let origin = to_sim(shot.eye);
        let pellets: Vec<Pellet> = (0..def.pellets)
            .map(|pellet| {
                let offset =
                    shot.aim_offset + weapon::pellet_offset(shot.weapon, shot.shot, pellet);
                let direction = player::aim_direction(shot.yaw, shot.pitch, offset).normalize();
                let to = shot.eye + to_godot(direction) * def.range;
                // Walls do not move, so they are tested in the present.
                let max_distance = world_hit(&mut space, shot.eye, to)
                    .map_or(def.range, |hit| shot.eye.distance_to(hit));
                Pellet {
                    origin,
                    direction,
                    max_distance,
                }
            })
            .collect();

        let rewind = self.lag.rewind_target(self.tick, shot.view_tick);
        let targets: Vec<Target> = self
            .avatars
            .iter()
            .filter(|a| {
                a.id != shot.shooter
                    && a.predicted.state.alive()
                    && f64::from(a.spawned_at) <= rewind
            })
            .filter_map(|a| {
                let pose = self.lag.sample(a.id, rewind)?.hitbox_pose()?;
                Some(Target { id: a.id, pose })
            })
            .collect();
        if self
            .client_of(shot.shooter)
            .is_some_and(|c| c.debug_rewound)
        {
            let poses = targets.iter().map(|t| (t.id, t.pose)).collect();
            self.send_event(
                shot.shooter,
                GameEvent::Rewound {
                    shot: shot.shot,
                    tick: rewind,
                    poses,
                },
            );
        }

        let mut kills = Vec::new();
        for hit in lag::resolve_shot(def, &pellets, &targets) {
            let Some(victim) = self.avatars.iter_mut().find(|a| a.id == hit.id) else {
                continue;
            };
            let killed = player::apply_damage(
                &mut victim.predicted.state,
                hit.damage,
                false,
                &mut Vec::new(),
            );
            self.send_event(
                shot.shooter,
                GameEvent::Hit {
                    victim: hit.id,
                    shot: shot.shot,
                    damage: hit.damage,
                    headshot: hit.headshot,
                    killed,
                },
            );
            self.send_event(
                hit.id,
                GameEvent::Hurt {
                    attacker: shot.shooter,
                    from: origin,
                    damage: hit.damage,
                    killed,
                },
            );
            if killed {
                kills.push(Kill {
                    distance: hit.distance,
                    headshot: hit.headshot,
                });
                self.broadcast_event(&GameEvent::Kill {
                    killer: shot.shooter,
                    victim: hit.id,
                    weapon: shot.weapon,
                    headshot: hit.headshot,
                });
                self.record_death(hit.id);
            }
        }
        if !kills.is_empty() {
            let rules = self.score_rules;
            let Some(shooter) = self.avatars.iter_mut().find(|a| a.id == shot.shooter) else {
                return;
            };
            let awards = shooter.score.on_shot_kills(&rules, &kills);
            let totals = shooter.score.totals();
            self.send_event(shot.shooter, GameEvent::Score { awards, totals });
        }
    }

    /// Counts a death and tells the player their new totals.
    fn record_death(&mut self, id: u8) {
        let Some(avatar) = self.avatars.iter_mut().find(|a| a.id == id) else {
            return;
        };
        avatar.score.on_death();
        let totals = avatar.score.totals();
        self.send_event(
            id,
            GameEvent::Score {
                awards: Vec::new(),
                totals,
            },
        );
    }

    fn client_of(&self, id: u8) -> Option<&ClientSlot> {
        self.clients.iter().find(|c| c.player == Some(id))
    }

    /// Queues an event for one player's client (bots have none).
    fn send_event(&mut self, id: u8, event: GameEvent) {
        if let Some(client) = self.clients.iter_mut().find(|c| c.player == Some(id)) {
            client.outbox.push(event);
        }
    }

    fn broadcast_event(&mut self, event: &GameEvent) {
        for client in &mut self.clients {
            if client.player.is_some() {
                client.outbox.push(event.clone());
            }
        }
    }

    /// Sends each client the events queued this tick, reliably and in order.
    fn send_events(&mut self) {
        for client in &mut self.clients {
            if client.outbox.is_empty() {
                continue;
            }
            for chunk in client.outbox.chunks(MAX_EVENTS_PER_MESSAGE) {
                let message = ServerMessage::Events(chunk.to_vec());
                transport::send_reliable(&mut client.peer, &message.encode(&[]));
            }
            client.outbox.clear();
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
        self.lag.record(self.tick, views.clone());

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

/// Where a ray from `from` to `to` first meets the map, if it does. Players
/// are on their own layer, so only walls, floors, and props count.
fn world_hit(
    space: &mut Gd<PhysicsDirectSpaceState3D>,
    from: Vector3,
    to: Vector3,
) -> Option<Vector3> {
    let query = PhysicsRayQueryParameters3D::create_ex(from, to)
        .collision_mask(WORLD_LAYER)
        .done()?;
    let hit = space.intersect_ray(&query);
    hit.get_or_nil("position").try_to::<Vector3>().ok()
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
