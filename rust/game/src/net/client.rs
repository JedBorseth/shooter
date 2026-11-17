//! Connection to a dedicated server.
//!
//! Each physics tick, after the local `FpsPlayer` has predicted its move, the
//! client reads server packets, reconciles the player against the newest
//! snapshot, feeds remote players' states into their interpolation buffers,
//! and sends every unacknowledged input. Each rendered frame it draws remote
//! players about 100 ms in the past.
//!
//! The server decides hits and kills and sends them as reliable game events:
//! the client passes them to the local player (kill confirmations, score,
//! damage taken) and emits `kill_reported` for every kill, for a kill feed.
//! It also keeps the match state the HUD shows: the mode, who is playing on
//! which team (`roster`), the match phase, clock, and team scores
//! (`match_info`), and the scoreboard, with each player's Gun Game level.
//! With the F4 hitbox overlay on, it also draws, in white, the hitboxes the
//! server rewound for each of our shots, next to the ones drawn here.
//!
//! An optional link conditioner adds latency, jitter, and loss in both
//! directions, to try the netcode at realistic pings against a local server.

use std::collections::VecDeque;

use godot::classes::{ENetConnection, ENetPacketPeer, INode, PackedScene, Time};
use godot::global::Error;
use godot::prelude::*;
use shooter_sim::gun_game;
use shooter_sim::hitbox::HitboxPose;
use shooter_sim::net::conditioner::{LinkConditioner, LinkSettings};
use shooter_sim::net::interpolation::{InterpolationBuffer, SnapshotClock};
use shooter_sim::net::prediction::Reconciled;
use shooter_sim::net::protocol::{
    ClientMessage, GameEvent, InputPacket, PROTOCOL_VERSION, ScoreRow, ServerMessage, Snapshot,
};
use shooter_sim::net::snapshot_history::SnapshotHistory;
use shooter_sim::net::{CHANNEL_COUNT, CHANNEL_RELIABLE, DEFAULT_PORT, TICK_RATE, Tick};
use shooter_sim::tdm::{Mode, Phase, TEAM_COUNT, Team};
use shooter_sim::weapon::WeaponId;

use super::remote::{RemotePlayer, RemoteSample};
use super::transport::{self, NetEvent};
use crate::convert::to_godot;
use crate::hitbox_rig::{self, HitboxRig};
use crate::player::FpsPlayer;

/// Seconds the server's rewound hitboxes stay drawn after a shot.
const GHOST_SECONDS: f32 = 1.5;
/// Most rewound hitbox sets drawn at once (an automatic weapon fires many).
const MAX_GHOSTS: usize = 24;

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
    /// The name other players see.
    #[export]
    player_name: GString,
    /// Primary weapon to ask for when joining (a weapon key), or empty for
    /// the server's default.
    #[export]
    primary: GString,
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
    /// Whether the server was last told to send rewound hitboxes.
    debug_requested: bool,
    /// The server's rewound hitboxes for recent shots, with seconds left.
    ghosts: VecDeque<(Gd<HitboxRig>, f32)>,
    /// Everyone in the game, ourselves included.
    roster: Vec<RosterEntry>,
    match_phase: Option<Phase>,
    match_ends_at: Option<Tick>,
    team_scores: [u32; TEAM_COUNT],
    score_limit: u32,
    /// The server's game, from its welcome.
    mode: Mode,
}

struct RosterEntry {
    id: u8,
    name: String,
    team: Team,
    bot: bool,
    row: ScoreRow,
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
    /// Our shots the server counted as hits, and as kills.
    hits_confirmed: u32,
    kills_confirmed: u32,
    /// Times another player hit us, and killed us.
    times_hurt: u32,
    deaths: u32,
    /// Players' hitboxes the server sent back as it rewound them (debug).
    rewound_received: u32,
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
                        name: self.player_name.to_string(),
                    };
                    transport::send_reliable(&mut peer, &hello.encode());
                    if let Some(primary) = WeaponId::by_key(&self.primary.to_string())
                        && crate::net::server::is_loadout_primary(primary)
                    {
                        let loadout = ClientMessage::Loadout { primary };
                        transport::send_reliable(&mut peer, &loadout.encode());
                    }
                }
                NetEvent::Disconnect(_) => self.on_disconnected(),
                NetEvent::Receive(_, bytes, channel) => {
                    self.count_in(bytes.len());
                    // The handshake is reliable: ENet already resends it, so the
                    // simulated loss only applies to unreliable traffic.
                    match &mut self.incoming {
                        Some(link) if channel != CHANNEL_RELIABLE => link.send(now, bytes),
                        _ => self.handle_packet(&bytes),
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
            self.send_debug_request();
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
        // Shots fired before the next frame aim at what was just drawn.
        if let Some(player) = &mut self.player {
            player.bind_mut().set_view_tick(render_tick);
        }
        self.age_ghosts(delta as f32);
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
    /// A player killed another (both ids as in `player_id`; bots from 200).
    #[signal]
    fn kill_reported(killer: i32, victim: i32, weapon: GString, headshot: bool);

    /// Someone joined, left, or changed team or name.
    #[signal]
    fn roster_changed();

    /// The match phase or the team scores changed.
    #[signal]
    fn match_changed();

    /// New scoreboard figures arrived.
    #[signal]
    fn scoreboard_changed();

    /// Every player, ourselves included, as dictionaries with `id`, `name`,
    /// `team`, `bot`, `me`, `score`, `kills`, `deaths`, `ping`, `ready`, and
    /// for Gun Game `level`, `level_kills`, and `weapon` (the name of the
    /// weapon on their level). Sorted by team and then by score, or in Gun
    /// Game by how far up the ladder, with the winner first once it ends.
    #[func]
    fn roster(&self) -> Array<VarDictionary> {
        let mut entries: Vec<&RosterEntry> = self.roster.iter().collect();
        if self.mode.free_for_all() {
            // The winner tops the results even when they only won a tie by
            // getting there first, which the rows cannot show.
            let winner = match self.match_phase {
                Some(Phase::Ended { winner }) => winner,
                _ => None,
            };
            entries.sort_by_key(|e| {
                (
                    Some(e.id) != winner,
                    std::cmp::Reverse((e.row.level, e.row.level_kills, e.row.kills)),
                    e.row.deaths,
                    e.id,
                )
            });
        } else {
            entries.sort_by_key(|e| (e.team, std::cmp::Reverse(e.row.score), e.id));
        }
        entries
            .into_iter()
            .map(|e| {
                let mut d = VarDictionary::new();
                d.set("id", i32::from(e.id));
                d.set("name", &GString::from(e.name.as_str()));
                d.set("team", i32::from(e.team));
                d.set("bot", e.bot);
                d.set("me", Some(e.id) == self.player_id);
                d.set("score", e.row.score);
                d.set("kills", e.row.kills);
                d.set("deaths", e.row.deaths);
                d.set("ping", i32::from(e.row.ping_ms));
                d.set("ready", e.row.ready);
                d.set("level", i32::from(e.row.level));
                d.set("level_kills", i32::from(e.row.level_kills));
                let weapon = gun_game::weapon(e.row.level).def();
                d.set("weapon", &GString::from(weapon.name.as_str()));
                d
            })
            .collect()
    }

    /// The match as a dictionary: `mode` ("tdm" or "gungame"), `mode_name`,
    /// `free_for_all`, `phase` ("lobby", "countdown", "playing", "ended", or
    /// "" before the server said), `seconds_left`, `scores` (per team),
    /// `score_limit`, `winner` (a team, or in free-for-all modes a player id;
    /// -1 for a draw), `my_team`, and for Gun Game `kills_per_weapon`.
    #[func]
    fn match_info(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        d.set("mode", self.mode.key());
        d.set("mode_name", self.mode.label());
        d.set("free_for_all", self.mode.free_for_all());
        if let Mode::GunGame(rules) = self.mode {
            d.set("kills_per_weapon", i32::from(rules.kills_per_weapon));
        }
        let (phase, winner) = match self.match_phase {
            None => ("", -1),
            Some(Phase::Lobby) => ("lobby", -1),
            Some(Phase::Countdown) => ("countdown", -1),
            Some(Phase::Playing) => ("playing", -1),
            Some(Phase::Ended { winner }) => ("ended", winner.map_or(-1, i32::from)),
        };
        d.set("phase", phase);
        d.set("winner", winner);
        let seconds_left = match (self.match_ends_at, self.newest) {
            (Some(end), Some(now)) => f64::from(end.saturating_sub(now)) / f64::from(TICK_RATE),
            _ => 0.0,
        };
        d.set("seconds_left", seconds_left);
        let scores: PackedInt32Array = self.team_scores.iter().map(|s| *s as i32).collect();
        d.set("scores", &scores);
        d.set("score_limit", self.score_limit);
        d.set("my_team", self.local_team().map_or(-1, i32::from));
        d
    }

    /// The weapons a loadout may use as its primary, as dictionaries with
    /// `key`, `name`, and `attachments` (each a dictionary with `key`, the
    /// variant's weapon key such as "m4+grip", and `name`), in table order.
    #[func]
    fn loadout_weapons() -> Array<VarDictionary> {
        let table = shooter_sim::weapon::table();
        table
            .iter()
            .enumerate()
            .filter(|(i, def)| {
                def.attachment.is_none()
                    && u8::try_from(*i)
                        .is_ok_and(|i| crate::net::server::is_loadout_primary(WeaponId(i)))
            })
            .map(|(_, def)| {
                let mut d = VarDictionary::new();
                d.set("key", &GString::from(def.key.as_str()));
                d.set("name", &GString::from(def.name.as_str()));
                let attachments: Array<VarDictionary> = def
                    .attachments
                    .iter()
                    .map(|a| {
                        let mut entry = VarDictionary::new();
                        entry.set("key", &GString::from(&format!("{}+{}", def.key, a.key())));
                        entry.set("name", a.label());
                        entry
                    })
                    .collect();
                d.set("attachments", &attachments);
                d
            })
            .collect()
    }

    /// The Gun Game ladder: weapon names in the order players climb them.
    #[func]
    fn gun_game_ladder() -> PackedStringArray {
        (0..gun_game::LEVELS)
            .map(|level| GString::from(gun_game::weapon(level).def().name.as_str()))
            .collect()
    }

    /// The primary weapon key we spawn with.
    #[func]
    fn loadout(&self) -> GString {
        self.primary.clone()
    }

    /// A player's name, or "" if unknown.
    #[func]
    fn name_of(&self, id: i32) -> GString {
        self.entry(id)
            .map_or_else(GString::new, |e| GString::from(e.name.as_str()))
    }

    /// A player's team, or -1 if unknown.
    #[func]
    fn team_of(&self, id: i32) -> i32 {
        self.entry(id).map_or(-1, |e| i32::from(e.team))
    }

    /// Tells the server we are ready (or not) to start the match.
    #[func]
    fn set_ready(&mut self, ready: bool) {
        self.send_reliable(&ClientMessage::Ready { ready });
    }

    /// Picks the primary weapon (a weapon key) for our next spawn. Returns
    /// false for a weapon that cannot be a loadout primary.
    #[func]
    fn set_loadout(&mut self, primary: GString) -> bool {
        let Some(id) = WeaponId::by_key(&primary.to_string())
            .filter(|id| crate::net::server::is_loadout_primary(*id))
        else {
            return false;
        };
        self.primary = primary;
        self.send_reliable(&ClientMessage::Loadout { primary: id });
        true
    }

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
        d.set("hits_confirmed", self.stats.hits_confirmed);
        d.set("kills_confirmed", self.stats.kills_confirmed);
        d.set("times_hurt", self.stats.times_hurt);
        d.set("deaths", self.stats.deaths);
        d.set("rewound_received", self.stats.rewound_received);
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
            ServerMessage::Welcome {
                player_id,
                snapshot_interval,
                mode,
                ..
            } => {
                if self.status != Status::Connecting {
                    return;
                }
                godot_print!("net client: joined as player {player_id}");
                self.status = Status::Joined;
                self.player_id = Some(player_id);
                self.mode = mode;
                if let Some(clock) = &mut self.clock {
                    clock.set_interval(u32::from(snapshot_interval));
                }
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
            ServerMessage::Events(events) => {
                if self.status == Status::Joined {
                    for event in events {
                        self.apply_event(event);
                    }
                }
            }
        }
    }

    fn apply_event(&mut self, event: GameEvent) {
        match event {
            GameEvent::Hit {
                victim,
                shot,
                headshot,
                killed,
                ..
            } => {
                self.stats.hits_confirmed += 1;
                if killed {
                    self.stats.kills_confirmed += 1;
                }
                let Some(player) = &mut self.player else {
                    return;
                };
                player.bind_mut().on_server_hit(shot, headshot, killed);
                // The body falls away from our shot.
                if killed && let Some(remote) = self.remotes.iter_mut().find(|r| r.id == victim) {
                    let direction = remote.node.bind().feet() - player.get_global_position();
                    remote.node.bind_mut().react_to_hit(direction, true);
                }
            }
            GameEvent::Hurt {
                from,
                damage,
                killed,
                ..
            } => {
                self.stats.times_hurt += 1;
                if killed {
                    self.stats.deaths += 1;
                }
                if let Some(player) = &mut self.player {
                    player
                        .bind_mut()
                        .on_server_hurt(damage, to_godot(from), killed);
                }
            }
            GameEvent::Kill {
                killer,
                victim,
                weapon,
                headshot,
            } => {
                let name = GString::from(&weapon.def().name);
                self.signals().kill_reported().emit(
                    i32::from(killer),
                    i32::from(victim),
                    &name,
                    headshot,
                );
            }
            GameEvent::Score { awards, totals } => {
                if let Some(player) = &mut self.player {
                    player.bind_mut().on_server_score(&awards, totals);
                }
            }
            GameEvent::Rewound { poses, .. } => {
                self.stats.rewound_received += poses.len() as u32;
                for (_, pose) in poses {
                    self.add_ghost(pose);
                }
            }
            GameEvent::Joined {
                id,
                name,
                team,
                bot,
            } => {
                match self.roster.iter_mut().find(|e| e.id == id) {
                    Some(entry) => {
                        entry.name = name;
                        entry.team = team;
                        entry.bot = bot;
                    }
                    None => self.roster.push(RosterEntry {
                        id,
                        name,
                        team,
                        bot,
                        row: ScoreRow {
                            id,
                            ready: bot,
                            ..ScoreRow::default()
                        },
                    }),
                }
                // Our own team decides everyone's colours.
                if Some(id) == self.player_id {
                    for i in 0..self.remotes.len() {
                        self.dress_remote(i);
                    }
                } else if let Some(i) = self.remotes.iter().position(|r| r.id == id) {
                    self.dress_remote(i);
                }
                self.signals().roster_changed().emit();
            }
            GameEvent::Left { id } => {
                self.roster.retain(|e| e.id != id);
                self.signals().roster_changed().emit();
            }
            GameEvent::Match {
                phase,
                ends_at,
                scores,
                score_limit,
            } => {
                self.match_phase = Some(phase);
                self.match_ends_at = ends_at;
                self.team_scores = scores;
                self.score_limit = score_limit;
                self.signals().match_changed().emit();
            }
            GameEvent::Scoreboard { rows } => {
                for row in rows {
                    if let Some(entry) = self.roster.iter_mut().find(|e| e.id == row.id) {
                        entry.row = row;
                    }
                }
                self.signals().scoreboard_changed().emit();
            }
            GameEvent::Spawned { yaw } => {
                if let Some(player) = &mut self.player {
                    player.bind_mut().set_look(yaw, 0.0);
                }
            }
        }
    }

    fn entry(&self, id: i32) -> Option<&RosterEntry> {
        self.roster.iter().find(|e| i32::from(e.id) == id)
    }

    fn local_team(&self) -> Option<Team> {
        let me = self.player_id?;
        self.roster.iter().find(|e| e.id == me).map(|e| e.team)
    }

    /// Names a remote player and colours them by whether they are on our team.
    fn dress_remote(&mut self, index: usize) {
        let id = self.remotes[index].id;
        let Some(entry) = self.roster.iter().find(|e| e.id == id) else {
            return;
        };
        // Without teams, everyone else is an enemy.
        let friendly = !self.mode.free_for_all() && self.local_team() == Some(entry.team);
        let name = entry.name.clone();
        let mut node = self.remotes[index].node.clone();
        let mut remote = node.bind_mut();
        remote.set_display_name(&name);
        remote.set_friendly(friendly);
    }

    fn send_reliable(&mut self, message: &ClientMessage) {
        if self.status != Status::Joined && self.status != Status::Connecting {
            return;
        }
        if let Some(server) = &mut self.server {
            transport::send_reliable(server, &message.encode());
        }
    }

    /// Asks the server for rewound hitboxes while the F4 overlay is on.
    fn send_debug_request(&mut self) {
        let wanted = hitbox_rig::debug_draw();
        if wanted == self.debug_requested {
            return;
        }
        let Some(server) = &mut self.server else {
            return;
        };
        let message = ClientMessage::Debug {
            rewound_hitboxes: wanted,
        };
        transport::send_reliable(server, &message.encode());
        self.debug_requested = wanted;
    }

    /// Draws one player's hitboxes as the server rewound them for a shot.
    fn add_ghost(&mut self, pose: HitboxPose) {
        if !hitbox_rig::debug_draw() {
            return;
        }
        while self.ghosts.len() >= MAX_GHOSTS {
            if let Some((ghost, _)) = self.ghosts.pop_front() {
                ghost.upcast::<Node>().queue_free();
            }
        }
        let mut ghost = HitboxRig::new_alloc();
        ghost.set_name("ServerHitboxes");
        ghost
            .bind_mut()
            .make_ghost(Color::from_rgba(1.0, 1.0, 1.0, 0.18));
        let Some(mut parent) = self.base().get_parent() else {
            ghost.free();
            return;
        };
        parent.add_child(&ghost);
        ghost.bind_mut().set_pose(Some(pose));
        self.ghosts.push_back((ghost, GHOST_SECONDS));
    }

    fn age_ghosts(&mut self, delta: f32) {
        for (_, left) in &mut self.ghosts {
            *left -= delta;
        }
        while self.ghosts.front().is_some_and(|(_, left)| *left <= 0.0) {
            if let Some((ghost, _)) = self.ghosts.pop_front() {
                ghost.upcast::<Node>().queue_free();
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
                // An old, reordered snapshot may still list a player who has left.
                None if !newest => continue,
                None => {
                    let Some(node) = self.spawn_remote(view.id) else {
                        continue;
                    };
                    self.remotes.push(Remote {
                        id: view.id,
                        node,
                        buffer: InterpolationBuffer::default(),
                    });
                    self.dress_remote(self.remotes.len() - 1);
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
        for (ghost, _) in self.ghosts.drain(..) {
            ghost.upcast::<Node>().queue_free();
        }
        self.roster.clear();
        self.match_phase = None;
        self.signals().roster_changed().emit();
        self.signals().match_changed().emit();
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
