//! Connection to a dedicated server.
//!
//! Each physics tick, after the local `FpsPlayer` has predicted its move, the
//! client reads server packets, reconciles the player against the newest
//! snapshot, feeds remote players' states into their interpolation buffers,
//! and sends every unacknowledged input. Each rendered frame it draws remote
//! players about 100 ms in the past.
//!
//! An optional link conditioner adds latency, jitter, and loss in both
//! directions, to try the netcode at realistic pings against a local server.

use godot::classes::{ENetConnection, ENetPacketPeer, INode, PackedScene, Time};
use godot::global::Error;
use godot::prelude::*;
use shooter_sim::net::conditioner::{LinkConditioner, LinkSettings};
use shooter_sim::net::interpolation::{InterpolationBuffer, SnapshotClock};
use shooter_sim::net::prediction::Reconciled;
use shooter_sim::net::protocol::{
    ClientMessage, InputPacket, PROTOCOL_VERSION, ServerMessage, Snapshot,
};
use shooter_sim::net::snapshot_history::SnapshotHistory;
use shooter_sim::net::{CHANNEL_COUNT, DEFAULT_PORT, TICK_RATE, Tick};

use super::remote::{RemotePlayer, RemoteSample};
use super::transport::{self, NetEvent};
use crate::convert::to_godot;
use crate::player::FpsPlayer;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Status {
    #[default]
    Connecting,
    Joined,
    Rejected,
    Disconnected,
}

#[derive(GodotClass)]
#[class(init, base = Node)]
pub struct NetClient {
    base: Base<Node>,

    #[export]
    #[init(val = GString::from("127.0.0.1"))]
    address: GString,
    #[export]
    #[init(val = i32::from(DEFAULT_PORT))]
    port: i32,
    /// The local `FpsPlayer` to predict.
    #[export]
    player_path: NodePath,
    /// Instanced for every other player.
    #[export]
    remote_player_scene: Option<Gd<PackedScene>>,
    /// How far in the past remote players are drawn, at least, in milliseconds.
    #[export]
    #[init(val = 100.0)]
    interpolation_ms: f32,
    /// Simulated one-way latency, added in each direction (testing only).
    #[export]
    latency_ms: f32,
    /// Simulated extra random one-way delay (testing only).
    #[export]
    jitter_ms: f32,
    /// Simulated packet loss in each direction, in percent (testing only).
    #[export]
    loss_percent: f32,

    host: Option<Gd<ENetConnection>>,
    server: Option<Gd<ENetPacketPeer>>,
    player: Option<Gd<FpsPlayer>>,
    status: Status,
    player_id: Option<u8>,
    history: SnapshotHistory,
    newest: Option<Tick>,
    clock: Option<SnapshotClock>,
    remotes: Vec<Remote>,
    outgoing: Option<LinkConditioner>,
    incoming: Option<LinkConditioner>,
    net_events: Vec<NetEvent>,
    stats: Stats,
}

struct Remote {
    id: u8,
    node: Gd<RemotePlayer>,
    buffer: InterpolationBuffer<RemoteSample>,
}

#[derive(Default)]
struct Stats {
    snapshots: u32,
    decode_errors: u32,
    bytes_in: u64,
    bytes_out: u64,
    window_start: f64,
    window_in: u64,
    window_out: u64,
    kbps_in: f64,
    kbps_out: f64,
    server_position: Vector3,
}

#[godot_api]
impl INode for NetClient {
    fn ready(&mut self) {
        // Run after the player's tick, so this tick's input goes out right away.
        self.base_mut().set_physics_process_priority(1);
        crate::body::check_tick_rate();
        self.clock = Some(SnapshotClock::new(self.interpolation_ms / 1000.0));
        let path = self.player_path.clone();
        self.player = self.base().try_get_node_as::<FpsPlayer>(&path);
        if self.player.is_none() {
            godot_error!("net client: player_path does not point to an FpsPlayer");
        }

        let link = LinkSettings {
            latency: self.latency_ms / 1000.0,
            jitter: self.jitter_ms / 1000.0,
            loss: self.loss_percent / 100.0,
        };
        if !link.is_perfect() {
            self.outgoing = Some(LinkConditioner::new(link, 1));
            self.incoming = Some(LinkConditioner::new(link, 2));
            godot_print!(
                "net client: simulating {} ms latency, {} ms jitter, {}% loss each way",
                self.latency_ms,
                self.jitter_ms,
                self.loss_percent
            );
        }

        let mut host = ENetConnection::new_gd();
        if host
            .create_host_ex()
            .max_peers(1)
            .max_channels(CHANNEL_COUNT)
            .done()
            != Error::OK
        {
            godot_error!("net client: cannot create an ENet host");
            self.status = Status::Disconnected;
            return;
        }
        self.server = host
            .connect_to_host_ex(&self.address, self.port)
            .channels(CHANNEL_COUNT)
            .done();
        if self.server.is_none() {
            godot_error!(
                "net client: cannot connect to {}:{}",
                self.address,
                self.port
            );
            self.status = Status::Disconnected;
            return;
        }
        godot_print!("net client: connecting to {}:{}", self.address, self.port);
        self.host = Some(host);
    }

    fn physics_process(&mut self, _delta: f64) {
        let Some(mut host) = self.host.clone() else {
            return;
        };
        let now = Time::singleton().get_ticks_usec() as f64 / 1e6;

        let mut events = std::mem::take(&mut self.net_events);
        events.clear();
        transport::service(&mut host, &mut events);
        for event in events.drain(..) {
            match event {
                NetEvent::Connect(mut peer) => {
                    let hello = ClientMessage::Hello {
                        version: PROTOCOL_VERSION,
                    };
                    transport::send_reliable(&mut peer, &hello.encode());
                }
                NetEvent::Disconnect(_) => self.on_disconnected(),
                NetEvent::Receive(_, bytes) => {
                    self.count_in(bytes.len());
                    match &mut self.incoming {
                        Some(link) => link.send(now, bytes),
                        None => self.handle_packet(&bytes),
                    }
                }
            }
        }
        self.net_events = events;
        if let Some(link) = &mut self.incoming {
            for bytes in link.receive(now) {
                self.handle_packet(&bytes);
            }
        }

        if self.status == Status::Joined {
            self.send_inputs(now);
        }
        if let (Some(link), Some(server)) = (&mut self.outgoing, &mut self.server) {
            for bytes in link.receive(now) {
                transport::send_unreliable(server, &bytes);
            }
        }
        host.flush();
        self.update_bandwidth(now);
    }

    fn process(&mut self, delta: f64) {
        if self.status != Status::Joined || self.newest.is_none() {
            return;
        }
        let Some(clock) = &mut self.clock else {
            return;
        };
        let render_tick = clock.advance(delta as f32);
        for remote in &mut self.remotes {
            if let Some(sample) = remote.buffer.sample(render_tick) {
                remote.node.bind_mut().apply(&sample, delta as f32);
            }
        }
    }

    fn exit_tree(&mut self) {
        if let Some(server) = &mut self.server {
            server.peer_disconnect_now();
        }
        // The player may already be gone when the whole scene is torn down.
        if let Some(player) = &mut self.player
            && player.is_instance_valid()
        {
            player.bind_mut().stop_prediction();
        }
    }
}

#[godot_api]
impl NetClient {
    /// True once the server accepted us.
    #[func]
    fn is_joined(&self) -> bool {
        self.status == Status::Joined
    }

    /// Our player id on the server, or -1.
    #[func]
    fn player_id(&self) -> i32 {
        self.player_id.map_or(-1, i32::from)
    }

    /// Ids of the other players currently shown.
    #[func]
    fn remote_ids(&self) -> PackedInt32Array {
        self.remotes.iter().map(|r| i32::from(r.id)).collect()
    }

    #[func]
    fn remote_player(&self, id: i32) -> Option<Gd<RemotePlayer>> {
        self.remotes
            .iter()
            .find(|r| i32::from(r.id) == id)
            .map(|r| r.node.clone())
    }

    /// Connection and netcode figures for the debug overlay and tests.
    #[func]
    fn stats(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let status = match self.status {
            Status::Connecting => "connecting",
            Status::Joined => "joined",
            Status::Rejected => "rejected (version mismatch)",
            Status::Disconnected => "disconnected",
        };
        d.set("status", status);
        let simulated_rtt = 2.0 * f64::from(self.latency_ms + self.jitter_ms / 2.0);
        let (rtt, loss) = self.server.as_ref().map_or((0.0, 0.0), |peer| {
            (transport::round_trip_ms(peer), transport::packet_loss(peer))
        });
        d.set("ping_ms", rtt + simulated_rtt);
        d.set(
            "loss_percent",
            (loss * 100.0).max(f64::from(self.loss_percent)),
        );
        d.set("snapshots", self.stats.snapshots);
        d.set("decode_errors", self.stats.decode_errors);
        d.set("kbps_in", self.stats.kbps_in);
        d.set("kbps_out", self.stats.kbps_out);
        d.set("bytes_in", self.stats.bytes_in as i64);
        d.set("bytes_out", self.stats.bytes_out as i64);
        d.set("server_position", self.stats.server_position);
        d.set("remote_players", self.remotes.len() as i32);
        if let Some(clock) = &self.clock {
            let ms = |ticks: f64| ticks * 1000.0 / f64::from(TICK_RATE);
            d.set("interp_delay_ms", ms(clock.delay()));
            d.set("jitter_ms", ms(clock.jitter_ticks()));
        }
        if let Some(player) = &self.player
            && let Some(prediction) = player.bind().prediction()
        {
            d.set("corrections", prediction.corrections);
            d.set("last_error_cm", prediction.last_error * 100.0);
            d.set("pending_inputs", prediction.pending() as i32);
        }
        d
    }
}

impl NetClient {
    fn handle_packet(&mut self, bytes: &[u8]) {
        let history = &self.history;
        let message = match ServerMessage::decode(bytes, |tick| history.get(tick)) {
            Ok(message) => message,
            Err(error) => {
                self.stats.decode_errors += 1;
                godot_warn!("net client: bad packet: {error}");
                return;
            }
        };
        match message {
            ServerMessage::Welcome { player_id, .. } => {
                if self.status == Status::Joined {
                    return;
                }
                godot_print!("net client: joined as player {player_id}");
                self.status = Status::Joined;
                self.player_id = Some(player_id);
                if let Some(player) = &mut self.player {
                    player.bind_mut().start_prediction();
                }
            }
            ServerMessage::Reject { server_version } => {
                godot_error!(
                    "net client: server speaks protocol {server_version}, this client {PROTOCOL_VERSION}"
                );
                self.status = Status::Rejected;
                if let Some(server) = &mut self.server {
                    server.peer_disconnect();
                }
            }
            ServerMessage::Snapshot(snapshot) => {
                if self.status == Status::Joined {
                    self.apply_snapshot(snapshot);
                }
            }
        }
    }

    fn apply_snapshot(&mut self, snapshot: Snapshot) {
        self.stats.snapshots += 1;
        let tick = snapshot.tick;
        self.history.insert(tick, snapshot.players.clone());
        let newest = self.newest.is_none_or(|n| tick > n);
        if newest {
            self.newest = Some(tick);
            if let Some(clock) = &mut self.clock {
                clock.on_snapshot(tick);
            }
        }

        for view in &snapshot.players {
            let index = match self.remotes.iter().position(|r| r.id == view.id) {
                Some(index) => index,
                None => {
                    let Some(node) = self.spawn_remote(view.id) else {
                        continue;
                    };
                    self.remotes.push(Remote {
                        id: view.id,
                        node,
                        buffer: InterpolationBuffer::default(),
                    });
                    self.remotes.len() - 1
                }
            };
            self.remotes[index]
                .buffer
                .push(tick, RemoteSample::from(view));
        }
        if !newest {
            return;
        }
        // Players missing from the newest snapshot have left.
        self.remotes.retain_mut(|remote| {
            let present = snapshot.players.iter().any(|v| v.id == remote.id);
            if !present {
                remote.node.clone().upcast::<Node>().queue_free();
            }
            present
        });

        if let Some(own) = snapshot.own {
            self.stats.server_position = to_godot(own.predicted.position);
            if let Some(player) = &mut self.player {
                let result = player.bind_mut().apply_server_state(&own);
                if let Reconciled::Corrected { previous, current } = result {
                    let error = previous.distance(current.position);
                    if error > 0.05 {
                        godot_print!(
                            "net client: corrected prediction by {:.1} cm",
                            error * 100.0
                        );
                    }
                }
            }
        }
    }

    fn spawn_remote(&mut self, id: u8) -> Option<Gd<RemotePlayer>> {
        let scene = self.remote_player_scene.as_ref()?;
        let mut node = scene.instantiate()?.try_cast::<RemotePlayer>().ok()?;
        node.set_name(&format!("Remote{id}"));
        let label = if id >= 200 {
            format!("Bot {}", id - 199)
        } else {
            format!("Player {}", id + 1)
        };
        // Remote players live beside the local player, in world space.
        let mut parent = self.base().get_parent()?;
        parent.add_child(&node);
        node.bind_mut().set_display_name(&label);
        Some(node)
    }

    fn send_inputs(&mut self, now: f64) {
        let Some(player) = &self.player else { return };
        let Some((first_seq, inputs)) = player.bind().prediction().map(|p| p.unacked()) else {
            return;
        };
        if inputs.is_empty() {
            return;
        }
        let bytes = ClientMessage::Input(InputPacket {
            ack_snapshot: self.newest,
            first_seq,
            inputs,
        })
        .encode();
        self.count_out(bytes.len());
        match (&mut self.outgoing, &mut self.server) {
            (Some(link), _) => link.send(now, bytes),
            (None, Some(server)) => transport::send_unreliable(server, &bytes),
            (None, None) => {}
        }
    }

    fn on_disconnected(&mut self) {
        if self.status != Status::Rejected {
            godot_warn!("net client: disconnected from the server");
            self.status = Status::Disconnected;
        }
        if let Some(player) = &mut self.player {
            player.bind_mut().stop_prediction();
        }
        for remote in self.remotes.drain(..) {
            remote.node.upcast::<Node>().queue_free();
        }
    }

    fn count_in(&mut self, bytes: usize) {
        self.stats.bytes_in += bytes as u64;
        self.stats.window_in += bytes as u64;
    }

    fn count_out(&mut self, bytes: usize) {
        self.stats.bytes_out += bytes as u64;
        self.stats.window_out += bytes as u64;
    }

    fn update_bandwidth(&mut self, now: f64) {
        let elapsed = now - self.stats.window_start;
        if elapsed >= 1.0 {
            self.stats.kbps_in = self.stats.window_in as f64 * 8.0 / 1000.0 / elapsed;
            self.stats.kbps_out = self.stats.window_out as f64 * 8.0 / 1000.0 / elapsed;
            self.stats.window_in = 0;
            self.stats.window_out = 0;
            self.stats.window_start = now;
        }
    }
}
