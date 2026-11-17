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
//! It runs Team Deathmatch or Capture the Flag (`shooter_sim::tdm`): it puts
//! each player on a team, keeps the match clock and the team scores, picks
//! spawn points away from enemies, spawns players with the loadout they chose,
//! and sends the roster, the match state, and the scoreboard as game events.
//! In Capture the Flag it also runs the flags (`shooter_sim::ctf`): each
//! team's flag sits at the map's `Flags` marker for that team (or among its
//! spawn points), and the server decides every take, drop, return, and
//! capture and tells everyone.
//!
//! Or it runs Gun Game (`shooter_sim::gun_game`, `mode = "gungame"`): every
//! player for themselves on the same match clock, everyone carrying only
//! their weapon on the ladder. Kills swap a player's weapon for the next one
//! on the spot, a fall takes one back, and the first player through the last
//! weapon wins.
//!
//! With a bot skill set, its bots play the match (`super::bots`): they walk
//! a navigation mesh baked from the map, look for enemies, and aim and shoot
//! through the same inputs players send. Without one, bots are scripted
//! targets for tests.
//!
//! It also answers server browser queries on its query port (the game port
//! plus one) and, given a master server, keeps itself listed there
//! (`super::discovery`).
//!
//! Started by `scripts/net/bootstrap.gd` with `-- --server`, or automatically
//! in a dedicated server export.

use std::collections::VecDeque;
use std::f32::consts::PI;

use godot::classes::{
    ENetConnection, ENetPacketPeer, Engine, INode3D, Node3D, PackedScene,
    PhysicsDirectSpaceState3D, PhysicsRayQueryParameters3D,
};
use godot::global::Error;
use godot::prelude::*;
use shooter_sim::bot::Skill;
use shooter_sim::ctf::{FlagEvent, FlagPlayer, FlagRules, FlagState, Flags};
use shooter_sim::gun_game::{self, Progress, Step};
use shooter_sim::net::discovery::{ServerInfo, query_port_for};
use shooter_sim::net::input_queue::InputQueue;
use shooter_sim::net::lag::{self, LagHistory, Pellet, Target};
use shooter_sim::net::protocol::{
    self, ClientMessage, FlagPlay, FlagStatus, FlagView, GameEvent, MAX_EVENTS_PER_MESSAGE,
    OwnState, PROTOCOL_VERSION, PlayerView, Predicted, ScoreRow, ServerMessage, Snapshot,
};
use shooter_sim::net::snapshot_history::SnapshotHistory;
use shooter_sim::net::{CHANNEL_COUNT, DEFAULT_PORT, MAX_PLAYERS, TICK_DT, TICK_RATE, Tick};
use shooter_sim::player::{self, PlayerEvent, PlayerInput, PlayerParams, PlayerState};
use shooter_sim::score::{Award, Kill, ScoreRules, ScoreState, Scored};
use shooter_sim::tdm::{
    self, MatchRules, MatchState, Mode, Phase, SpawnPoint, SpawnRules, Team, Threat, Transition,
};
use shooter_sim::weapon::{self, CombatEvent, CombatState, Slot, WeaponId};
use shooter_sim::{Vec2, Vec3, rng};

use super::bots::{BotBrain, BotNavigation, BotObjective, BotPlayer, BotWorld};
use super::discovery::QueryResponder;
use super::transport::{self, NetEvent};
use crate::body::{self, BodyParts};
use crate::convert::{to_godot, to_sim};
use crate::penetration;
use crate::pickup::{INTERACT_LAYER, WeaponPickup};
use crate::target::WORLD_LAYER;

/// Map nodes that only make sense on a client, or only offline: the local
/// player and its UI, practice targets, and the sentry turret (it would damage
/// players without the server knowing). Weapon racks stay: they are solid, and
/// players take weapons from them through their input.
const CLIENT_ONLY_NODES: [&str; 6] = [
    "Player",
    "HUD",
    "PauseMenu",
    "Targets",
    "Sentry",
    "SecretLaser",
];

/// Seconds a new connection has to say hello.
const HELLO_TIMEOUT_TICKS: Tick = 5 * TICK_RATE;
/// A little more than the client's weapon rack reach (`FpsPlayer::interact_distance`).
const PICKUP_REACH: f32 = 3.0;
/// Bots get ids from here up, so they never collide with client ids.
const FIRST_BOT_ID: u8 = 200;
/// Seconds a death keeps nearby spawn points unattractive.
const RECENT_DEATH_SECONDS: u32 = 5;
/// Ticks between scoreboard updates when nothing scored.
const SCOREBOARD_INTERVAL: Tick = TICK_RATE;
/// Height of the point enemies must see for a spawn to count as watched.
const SPAWN_CHECK_HEIGHT: f32 = 1.5;
/// Fewest seconds a dead player waits to respawn in Capture the Flag.
const CTF_RESPAWN_SECONDS: f32 = 5.0;
/// Farthest below a carrier's feet a dropped flag looks for the floor.
const FLAG_DROP_FALL: f32 = 12.0;
/// Seconds a Capture the Flag defender bot holds one spot near its flag.
const DEFEND_SPOT_SECONDS: u32 = 7;
/// Metres from its flag a defender bot patrols.
const DEFEND_RADIUS: f32 = 6.0;

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
    /// Private local-match control file; empty on public dedicated servers.
    #[export]
    pause_file: GString,
    /// Whether scripted bots' bursts can hurt players. Off by default, so a
    /// test client is not shot at random. Bots with a skill always can.
    #[export]
    bots_deal_damage: bool,
    /// "easy", "normal", or "hard" for bots that play the match; empty for
    /// scripted bots that strafe and fire bursts (tests).
    #[export]
    bot_skill: GString,
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
    /// Points a team needs to win: kills in Team Deathmatch, captures in
    /// Capture the Flag. 0 uses the mode's usual limit.
    #[export]
    score_limit: i32,
    /// Minutes of play before the team ahead wins.
    #[export]
    #[init(val = 10.0)]
    time_limit_minutes: f32,
    /// Start matches without waiting for players to ready up (tests, and
    /// servers that should just run).
    #[export]
    auto_start: bool,
    /// The game: "tdm" (Team Deathmatch), "ctf" (Capture the Flag), or
    /// "gungame".
    #[export]
    #[init(val = GString::from("tdm"))]
    mode: GString,
    /// Gun Game: kills a player needs with each weapon to move up.
    #[export]
    #[init(val = 2)]
    kills_per_weapon: i32,

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
    /// The map's player start, the fallback for maps without spawn points.
    spawn: Vector3,
    responder: Option<QueryResponder>,
    rules: MatchRules,
    /// The game, parsed from `mode` when the server starts.
    game: Mode,
    match_state: MatchState,
    spawn_points: Vec<SpawnPoint>,
    spawn_rules: SpawnRules,
    /// Where players died recently, and when.
    recent_deaths: VecDeque<(Tick, Vec3)>,
    /// Tick of the next scoreboard update.
    next_scoreboard: Tick,
    /// The navigation mesh bots walk, when they play.
    navigation: Option<BotNavigation>,
    navigation_pending: bool,
    /// Capture the Flag: both flags (none in Team Deathmatch).
    flags: Option<Flags>,
    flag_rules: FlagRules,
    /// The map's flag bases, if it marks them.
    flag_homes: Option<[Vec3; tdm::TEAM_COUNT]>,
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
    /// Ready to start the match (lobby).
    ready: bool,
    /// The primary weapon for the next spawn.
    primary: WeaponId,
}

struct Avatar {
    id: u8,
    parts: BodyParts,
    predicted: Predicted,
    /// View angles from the last applied input.
    yaw: f32,
    pitch: f32,
    bot: bool,
    name: String,
    team: Team,
    /// The primary weapon this player spawns with.
    primary: WeaponId,
    /// A bot that plays the match decides its inputs here.
    brain: Option<BotBrain>,
    respawn_in: f32,
    /// Tick of the latest spawn: shots rewound to before it cannot hit this life.
    spawned_at: Tick,
    score: ScoreState,
    /// Hits taken (wrapping) and the latest one's heading, for everyone's
    /// flinch (see `PlayerView::hits`).
    hits: u8,
    hit_heading: u8,
    /// Gun Game: this player's place on the weapon ladder.
    progress: Progress,
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
        self.game = match Mode::parse(&self.mode.to_string()) {
            Some(Mode::GunGame(_)) => Mode::GunGame(gun_game::Rules {
                kills_per_weapon: self.kills_per_weapon.clamp(1, 255) as u8,
            }),
            Some(mode) => mode,
            None => {
                godot_warn!(
                    "server: unknown mode \"{}\", running Team Deathmatch",
                    self.mode
                );
                Mode::TeamDeathmatch
            }
        };
        let score_limit = match self.score_limit {
            limit if limit > 0 => limit as u32,
            _ => self.game.default_score_limit(),
        };
        self.rules = MatchRules {
            score_limit,
            time_limit: self.time_limit_minutes.max(0.1) * 60.0,
            ..MatchRules::default()
        };
        godot_print!("server: running {}", self.game.label());
        self.spawn_rules = SpawnRules::for_mode(&self.game);
        if self.game == Mode::CaptureTheFlag {
            // Longer than Team Deathmatch, so clearing a base buys time to
            // take its flag before the defenders are back.
            self.respawn_delay = self.respawn_delay.max(CTF_RESPAWN_SECONDS);
        }
        self.load_map();
        if self.spawn_points.is_empty() {
            self.spawn_points = fallback_spawns(self.spawn);
        }
        if self.game.free_for_all() {
            // Every spawn is anyone's: no sides without teams.
            for point in &mut self.spawn_points {
                point.team = None;
            }
        }
        if self.game == Mode::CaptureTheFlag {
            let homes = self
                .flag_homes
                .unwrap_or_else(|| fallback_flag_homes(&self.spawn_points));
            self.flags = Some(Flags::new(homes));
            godot_print!(
                "server: flags at {} and {}, first to {} captures",
                to_godot(homes[0]),
                to_godot(homes[1]),
                self.rules.score_limit
            );
        }

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

        let skill = Skill::parse(&self.bot_skill.to_string());
        // CSG builds its collision a frame after entering the tree, so the
        // navigation mesh bakes on an early tick (`physics_process`).
        self.navigation_pending = skill.is_some() && self.bots > 0;
        for i in 0..self.bots.clamp(0, 50) {
            let id = FIRST_BOT_ID + i as u8;
            let brain = skill.map(|skill| BotBrain::new(skill, rng::hash(u32::from(id), 77)));
            self.spawn_avatar(
                id,
                true,
                &format!("Bot {}", i + 1),
                default_primary(),
                brain,
            );
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
        if !self.pause_file.is_empty() {
            let paused = godot::classes::FileAccess::file_exists(&self.pause_file);
            self.base().get_tree().set_pause(paused);
            if paused {
                // Keep ENet heartbeats alive even during a long local pause.
                self.send_events();
                host.flush();
                return;
            }
        }
        self.drop_silent_peers();

        if self.navigation_pending && self.tick >= 2 {
            self.navigation_pending = false;
            let mut parent = self.base().clone().upcast::<Node>();
            let map = self.base().get_child(0);
            self.navigation = map.and_then(|map| BotNavigation::bake(&mut parent, &map));
        }
        self.simulate();
        self.update_flags();
        self.update_match();
        self.tick += 1;
        if self
            .tick
            .is_multiple_of(self.snapshot_interval.max(1) as u32)
        {
            self.broadcast();
        }
        if self.tick >= self.next_scoreboard {
            self.broadcast_scoreboard();
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

    /// A player's team (tests and debugging), or -1 if absent.
    #[func]
    fn player_team(&self, id: i32) -> i32 {
        self.avatars
            .iter()
            .find(|a| i32::from(a.id) == id)
            .map_or(-1, |a| i32::from(a.team))
    }

    /// "lobby", "countdown", "playing", or "ended".
    #[func]
    fn match_phase(&self) -> GString {
        GString::from(phase_name(self.match_state.phase))
    }

    /// Whether the bots have a navigation mesh to walk.
    #[func]
    fn has_bot_navigation(&self) -> bool {
        self.navigation.is_some()
    }

    /// Kills a player has scored this match (tests), or -1 if absent.
    #[func]
    fn player_kills(&self, id: i32) -> i64 {
        self.avatars
            .iter()
            .find(|a| i32::from(a.id) == id)
            .map_or(-1, |a| i64::from(a.score.kills))
    }

    /// A player's place on the Gun Game ladder (tests), or -1 if absent.
    #[func]
    fn player_level(&self, id: i32) -> i32 {
        self.avatars
            .iter()
            .find(|a| i32::from(a.id) == id)
            .map_or(-1, |a| i32::from(a.progress.level))
    }

    /// The weapon key a player holds (tests), or "" if absent.
    #[func]
    fn player_weapon(&self, id: i32) -> GString {
        self.avatars
            .iter()
            .find(|a| i32::from(a.id) == id)
            .map_or_else(GString::new, |a| {
                GString::from(a.predicted.state.combat.weapon().id.def().key.as_str())
            })
    }

    /// The winner of the match that just ended: a team, or a player id in
    /// free-for-all modes; -1 for a draw or no result yet.
    #[func]
    fn match_winner(&self) -> i32 {
        match self.match_state.phase {
            Phase::Ended { winner } => winner.map_or(-1, i32::from),
            _ => -1,
        }
    }

    /// Capture the Flag (tests and debugging): where team `team`'s flag is,
    /// "home", "carried", or "dropped", or "" in other modes.
    #[func]
    fn flag_status(&self, team: i32) -> GString {
        let Some(flags) = &self.flags else {
            return GString::new();
        };
        let Ok(team) = Team::try_from(team) else {
            return GString::new();
        };
        GString::from(match flags.flag(team).state {
            FlagState::Home => "home",
            FlagState::Carried { .. } => "carried",
            FlagState::Dropped { .. } => "dropped",
        })
    }

    /// Captures a player has made this match (tests), or -1 if absent.
    #[func]
    fn player_captures(&self, id: i32) -> i64 {
        self.avatars
            .iter()
            .find(|a| i32::from(a.id) == id)
            .map_or(-1, |a| i64::from(a.score.captures))
    }

    /// Debug: moves a player's feet to `position` (tests).
    #[func]
    fn debug_teleport(&mut self, id: i32, position: Vector3) {
        let params = self.params;
        if let Some(avatar) = self.avatars.iter_mut().find(|a| i32::from(a.id) == id) {
            avatar.predicted.position = to_sim(position);
            body::place(&mut avatar.parts, &params.movement, &avatar.predicted);
        }
    }

    /// Debug: kills a player where they stand (tests).
    #[func]
    fn debug_kill(&mut self, id: i32) {
        let Ok(id) = u8::try_from(id) else { return };
        let Some(avatar) = self.avatars.iter_mut().find(|a| a.id == id) else {
            return;
        };
        let health = avatar.predicted.state.health;
        player::apply_damage(
            &mut avatar.predicted.state,
            health + 1.0,
            false,
            &mut Vec::new(),
        );
        self.record_death(id);
    }

    /// Points a team has this match: kills, or captures in Capture the Flag.
    #[func]
    fn team_score(&self, team: i32) -> i64 {
        usize::try_from(team)
            .ok()
            .and_then(|t| self.match_state.scores.get(t))
            .map_or(0, |s| i64::from(*s))
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
            mode: self.game.label().to_owned(),
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
        let Some(mut map) = self.map_scene.as_ref().and_then(|s| s.instantiate()) else {
            godot_warn!("server: no map scene set");
            return;
        };
        if map.has_method("prepare_server") {
            map.call("prepare_server", &[]);
        }
        if let Some(player) = map.get_node_or_null("Player")
            && let Ok(player) = player.try_cast::<Node3D>()
        {
            self.spawn = player.get_position();
        }
        self.spawn_points = map_spawns(&map);
        self.flag_homes = map_flags(&map);
        for name in CLIENT_ONLY_NODES {
            if let Some(node) = map.get_node_or_null(name) {
                node.free();
            }
        }
        map.set_process_mode(godot::classes::node::ProcessMode::PAUSABLE);
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
                    ready: false,
                    primary: default_primary(),
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
                    self.broadcast_event(&GameEvent::Left { id });
                    self.next_scoreboard = self.tick;
                    self.balance_bots();
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
            ClientMessage::Hello { version, name } => {
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
                let name = tdm::clean_name(&name, &format!("Player {}", id + 1));
                let primary = self.clients[index].primary;
                // The welcome goes first, so the client knows its id before
                // the roster arrives.
                let welcome = ServerMessage::Welcome {
                    player_id: id,
                    tick: self.tick,
                    snapshot_interval: self.snapshot_interval.clamp(1, 255) as u8,
                    mode: self.game,
                };
                transport::send_reliable(&mut peer, &welcome.encode(&[]));
                // Everyone already here, then the newcomer for everyone.
                let roster: Vec<GameEvent> = self.avatars.iter().map(joined_event).collect();
                self.clients[index].outbox.extend(roster);
                self.spawn_avatar(id, false, &name, primary, None);
                self.balance_bots();
                let event = self.match_event();
                self.send_event(id, event);
                if let Some(event) = self.flags_event() {
                    self.send_event(id, event);
                }
                godot_print!("server: player {id} \"{name}\" joined");
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
            ClientMessage::Ready { ready } => {
                self.clients[index].ready = ready;
                self.next_scoreboard = self.tick;
            }
            ClientMessage::Loadout { primary } => {
                if !is_loadout_primary(primary) {
                    return;
                }
                self.clients[index].primary = primary;
                if let Some(id) = self.clients[index].player
                    && let Some(avatar) = self.avatars.iter_mut().find(|a| a.id == id)
                {
                    avatar.primary = primary;
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

    fn spawn_avatar(
        &mut self,
        id: u8,
        bot: bool,
        name: &str,
        primary: WeaponId,
        brain: Option<BotBrain>,
    ) {
        let mut counts = [0; tdm::TEAM_COUNT];
        for avatar in &self.avatars {
            counts[usize::from(avatar.team)] += 1;
        }
        // Without teams, everyone is on team 0.
        let team = if self.game.free_for_all() {
            0
        } else {
            tdm::assign_team(counts, u8::from(bot))
        };
        let mut parts = body::build_body(&format!("Player{id}"));
        self.base_mut().add_child(&parts.body);
        let predicted = Predicted {
            state: PlayerState::spawned(&self.params),
            position: Vec3::ZERO,
            on_floor: false,
        };
        body::place(&mut parts, &self.params.movement, &predicted);
        self.avatars.push(Avatar {
            id,
            parts,
            predicted,
            yaw: 0.0,
            pitch: 0.0,
            bot,
            name: name.to_owned(),
            team,
            primary,
            brain,
            respawn_in: 0.0,
            spawned_at: self.tick,
            score: ScoreState::default(),
            hits: 0,
            hit_heading: 0,
            progress: Progress::default(),
        });
        let event = joined_event(self.avatars.last().expect("just pushed"));
        self.broadcast_event(&event);
        // Before a match the team starts together; during one, or without
        // teams, somewhere safe.
        let start = !self.match_state.scoring() && !self.game.free_for_all();
        let slot = counts[usize::from(team)];
        self.respawn(id, start.then_some(slot));
        self.next_scoreboard = self.tick;
    }

    /// Brings a player back to life at a spawn point with their loadout.
    /// `start_slot` places them among their team's start spawns instead of
    /// at the safest spawn.
    fn respawn(&mut self, id: u8, start_slot: Option<usize>) {
        let Some(index) = self.avatars.iter().position(|a| a.id == id) else {
            return;
        };
        let team = self.avatars[index].team;
        let point = match start_slot {
            Some(slot) => tdm::start_spawn(&self.spawn_points, team, slot),
            None => self.safest_spawn(index),
        };
        let Some(point) = point.and_then(|i| self.spawn_points.get(i)).copied() else {
            return;
        };
        let tick = self.tick;
        let gun_game = matches!(self.game, Mode::GunGame(_));
        let avatar = &mut self.avatars[index];
        if let Some(brain) = &mut avatar.brain {
            // Bots pick a new primary every life.
            let options = loadout_primaries();
            let pick = rng::hash(u32::from(avatar.id), tick) as usize % options.len().max(1);
            if let Some(primary) = options.get(pick) {
                avatar.primary = *primary;
            }
            brain.on_spawn(point.yaw, tick);
        }
        let loadout = if gun_game {
            gun_game::weapon(avatar.progress.level)
        } else {
            avatar.primary
        };
        let mut state = PlayerState::new([loadout, WeaponId::expect("m1911")]);
        if gun_game {
            // Gun Game: the ladder's weapon and nothing else.
            state.combat = CombatState::only(loadout);
        }
        avatar.predicted = Predicted {
            state,
            position: point.position,
            on_floor: false,
        };
        avatar.yaw = point.yaw;
        avatar.pitch = 0.0;
        avatar.spawned_at = tick;
        avatar.respawn_in = self.respawn_delay;
        body::place(&mut avatar.parts, &self.params.movement, &avatar.predicted);
        self.send_event(id, GameEvent::Spawned { yaw: point.yaw });
    }

    /// The spawn point furthest from trouble for the player at `index`.
    fn safest_spawn(&self, index: usize) -> Option<usize> {
        let me = &self.avatars[index];
        let threats: Vec<Threat> = self
            .avatars
            .iter()
            .filter(|a| a.id != me.id && a.predicted.state.alive())
            .map(|a| Threat {
                position: a.predicted.position,
                enemy: self.hostile(a, me),
            })
            .collect();
        let eyes: Vec<Vector3> = self
            .avatars
            .iter()
            .filter(|a| self.hostile(a, me) && a.predicted.state.alive())
            .map(|a| {
                let stance = a.predicted.state.movement.stance;
                to_godot(a.predicted.position)
                    + Vector3::UP * self.params.movement.eye_height(stance)
            })
            .collect();
        let deaths: Vec<Vec3> = self.recent_deaths.iter().map(|(_, p)| *p).collect();
        let mut space = self
            .base()
            .get_world_3d()
            .and_then(|world| world.get_direct_space_state());
        let points = &self.spawn_points;
        let seen = |i: usize| -> u32 {
            let Some(space) = &mut space else { return 0 };
            let target = to_godot(points[i].position) + Vector3::UP * SPAWN_CHECK_HEIGHT;
            eyes.iter()
                .filter(|eye| world_hit(space, **eye, target).is_none())
                .count() as u32
        };
        let seed = rng::hash(u32::from(me.id), self.tick);
        tdm::choose_spawn(
            points,
            me.team,
            &threats,
            &deaths,
            &self.spawn_rules,
            seen,
            seed,
        )
    }

    /// Playing bots fill the teams up to the size the server started with
    /// (its bots plus one player): each player who joins takes a bot's place,
    /// and a bot comes back when a player leaves. Teams stay even.
    fn balance_bots(&mut self) {
        let Some(skill) = Skill::parse(&self.bot_skill.to_string()) else {
            return;
        };
        let target = usize::try_from(self.bots).unwrap_or(0) + 1;
        loop {
            let mut counts = [0usize; tdm::TEAM_COUNT];
            for avatar in &self.avatars {
                counts[usize::from(avatar.team)] += 1;
            }
            let total = counts[0] + counts[1];
            if total > target {
                // A bot leaves the bigger team.
                let team = u8::from(counts[1] > counts[0]);
                let Some(id) = self
                    .avatars
                    .iter()
                    .filter(|a| a.brain.is_some() && a.team == team)
                    .map(|a| a.id)
                    .max()
                else {
                    return;
                };
                self.despawn_avatar(id);
                self.broadcast_event(&GameEvent::Left { id });
            } else if total < target {
                let Some(id) =
                    (FIRST_BOT_ID..=u8::MAX).find(|id| !self.avatars.iter().any(|a| a.id == *id))
                else {
                    return;
                };
                let brain = BotBrain::new(skill, rng::hash(u32::from(id), self.tick));
                let name = format!("Bot {}", id - FIRST_BOT_ID + 1);
                self.spawn_avatar(id, true, &name, default_primary(), Some(brain));
            } else {
                return;
            }
            self.next_scoreboard = self.tick;
        }
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
        let mut respawns = Vec::new();
        let tick = self.tick;
        // What bots know about everyone this tick.
        let players: Vec<BotPlayer> = self
            .avatars
            .iter()
            .map(|a| BotPlayer {
                id: a.id,
                team: self.side(a),
                alive: a.predicted.state.alive(),
                feet: to_godot(a.predicted.position),
                stance: a.predicted.state.movement.stance,
            })
            .collect();
        let mut team_spawns: [Vec<Vector3>; 2] = [Vec::new(), Vec::new()];
        for point in &self.spawn_points {
            if let Some(team) = point.team {
                team_spawns[usize::from(team.min(1))].push(to_godot(point.position));
            }
        }
        let objectives = self.bot_objectives();
        let mut space = self
            .base()
            .get_world_3d()
            .and_then(|world| world.get_direct_space_state());
        // Gun Game hands out weapons by the ladder only.
        let racks = !matches!(self.game, Mode::GunGame(_));
        for avatar in &mut self.avatars {
            let inputs: Vec<(PlayerInput, Option<f64>)> = if avatar.bot {
                let input = match (&mut avatar.brain, &mut self.navigation, &mut space) {
                    (Some(brain), Some(navigation), Some(space)) => {
                        let me = players
                            .iter()
                            .find(|p| p.id == avatar.id)
                            .copied()
                            .expect("every avatar is listed");
                        let mut world = BotWorld {
                            tick,
                            players: &players,
                            params: &self.params,
                            navigation,
                            space,
                            team_spawns: &team_spawns,
                        };
                        let objective = objectives
                            .iter()
                            .find(|(id, _)| *id == avatar.id)
                            .and_then(|(_, o)| *o);
                        brain.think(&me, &avatar.predicted, &mut world, objective)
                    }
                    _ => bot_input(avatar, tick),
                };
                vec![(input, None)]
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
                    && (!racks || !pickup_in_reach(avatar, &input, weapon, &self.params))
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
                    respawns.push(avatar.id);
                }
            }
        }
        for id in fall_deaths {
            self.record_death(id);
            self.on_suicide(id);
        }
        for id in respawns {
            self.respawn(id, None);
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
        if shooter.bot && shooter.brain.is_none() && !self.bots_deal_damage {
            return;
        }
        // Nobody gets hurt while a match counts down or shows its results.
        if matches!(
            self.match_state.phase,
            Phase::Countdown | Phase::Ended { .. }
        ) {
            return;
        }
        let shooter_team = shooter.team;
        let shooter_side = self.side(shooter);
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
                // Walls do not move, so they are tested in the present. The
                // pellet may go on through thin ones, weaker.
                let path = penetration::trace(
                    &mut space,
                    shot.eye,
                    to_godot(direction),
                    def.range,
                    WORLD_LAYER,
                    &Array::new(),
                    def.penetration,
                );
                let mut pellet = Pellet::new(origin, direction, shot.eye.distance_to(path.end));
                let mut scale = 1.0;
                let mut walls = pellet.walls.iter_mut();
                for hit in &path.hits {
                    if hit.damage_scale < scale
                        && let Some(wall) = walls.next()
                    {
                        scale = hit.damage_scale;
                        *wall = (shot.eye.distance_to(hit.position), scale);
                    }
                }
                pellet
            })
            .collect();

        let rewind = self.lag.rewind_target(self.tick, shot.view_tick);
        let targets: Vec<Target> = self
            .avatars
            .iter()
            // No friendly fire: bullets pass through teammates.
            .filter(|a| {
                a.id != shot.shooter
                    && self.side(a) != shooter_side
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
        let mut carrier_kills = 0;
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
            if let Some(brain) = &mut victim.brain {
                brain.on_hurt(shot.shooter, shot.eye, self.tick);
            }
            let travel = victim.predicted.position - origin;
            victim.hits = victim.hits.wrapping_add(1);
            victim.hit_heading = protocol::heading_to_u8(travel.x, travel.z);
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
                // The flag drops on the next flag update; it is still on them.
                if self
                    .flags
                    .as_ref()
                    .and_then(|f| f.carried_by(hit.id))
                    .is_some_and(|flag| flag == shooter_team)
                {
                    carrier_kills += 1;
                }
                self.broadcast_event(&GameEvent::Kill {
                    killer: shot.shooter,
                    victim: hit.id,
                    weapon: shot.weapon,
                    headshot: hit.headshot,
                });
                self.record_death(hit.id);
                let victim_team = self.avatars.iter().find(|a| a.id == hit.id).map(|a| a.team);
                let end = match self.game {
                    Mode::GunGame(rules) => self.climb(shot.shooter, &rules),
                    Mode::TeamDeathmatch => victim_team.and_then(|victim_team| {
                        self.match_state
                            .on_kill(&self.rules, Some(shooter_team), victim_team)
                    }),
                    // Only captures score for the team.
                    Mode::CaptureTheFlag => None,
                };
                if let Some(end) = end {
                    self.on_transition(end);
                } else {
                    self.broadcast_match();
                }
            }
        }
        // Points only count in a match, not in the lobby's warm-up.
        if !kills.is_empty() && self.match_state.scoring() {
            let rules = self.score_rules;
            let Some(shooter) = self.avatars.iter_mut().find(|a| a.id == shot.shooter) else {
                return;
            };
            let mut awards = shooter.score.on_shot_kills(&rules, &kills);
            for _ in 0..carrier_kills {
                awards.extend(shooter.score.on_flag_play(&rules, Award::CarrierKill));
            }
            let totals = shooter.score.totals();
            self.send_event(shot.shooter, GameEvent::Score { awards, totals });
        }
    }

    /// Counts a death and tells the player their new totals.
    fn record_death(&mut self, id: u8) {
        let tick = self.tick;
        let scoring = self.match_state.scoring();
        let Some(avatar) = self.avatars.iter_mut().find(|a| a.id == id) else {
            return;
        };
        self.recent_deaths
            .push_back((tick, avatar.predicted.position));
        self.next_scoreboard = tick;
        if !scoring {
            return;
        }
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

    /// Runs the match clock and starts, ends, and restarts matches.
    fn update_match(&mut self) {
        let horizon = RECENT_DEATH_SECONDS * TICK_RATE;
        while self
            .recent_deaths
            .front()
            .is_some_and(|(t, _)| self.tick.saturating_sub(*t) > horizon)
        {
            self.recent_deaths.pop_front();
        }
        let humans: Vec<&ClientSlot> = self.clients.iter().filter(|c| c.player.is_some()).collect();
        if humans.is_empty() && !self.auto_start {
            if self.match_state.phase != Phase::Lobby {
                self.match_state.reset();
            }
            return;
        }
        let all_ready = self.auto_start || humans.iter().all(|c| c.ready);
        let phase = self.match_state.phase;
        let leader = match self.game {
            Mode::TeamDeathmatch | Mode::CaptureTheFlag => self.match_state.team_leader(),
            Mode::GunGame(_) => gun_game::leader(self.avatars.iter().map(|a| (a.id, a.progress))),
        };
        let transition = self
            .match_state
            .tick(&self.rules, TICK_DT, all_ready, leader);
        if let Some(transition) = transition {
            self.on_transition(transition);
        } else if self.match_state.phase != phase {
            self.broadcast_match();
        }
    }

    fn on_transition(&mut self, transition: Transition) {
        match transition {
            Transition::Started => {
                godot_print!("server: match started");
                self.recent_deaths.clear();
                self.reset_flags();
                // Everyone starts fresh on their own side, in join order;
                // without teams, each somewhere away from the others.
                let free_for_all = self.game.free_for_all();
                let mut slots = [0; tdm::TEAM_COUNT];
                let mut order: Vec<(Team, u8)> =
                    self.avatars.iter().map(|a| (a.team, a.id)).collect();
                order.sort_unstable();
                for (team, id) in order {
                    if let Some(avatar) = self.avatars.iter_mut().find(|a| a.id == id) {
                        avatar.score = ScoreState::default();
                        avatar.progress = Progress::default();
                    }
                    let slot = &mut slots[usize::from(team)];
                    self.respawn(id, (!free_for_all).then_some(*slot));
                    *slot += 1;
                }
                let ids: Vec<u8> = self.avatars.iter().map(|a| a.id).collect();
                for id in ids {
                    self.send_event(
                        id,
                        GameEvent::Score {
                            awards: Vec::new(),
                            totals: ScoreState::default().totals(),
                        },
                    );
                }
            }
            Transition::Ended { winner } => {
                if self.game.free_for_all() {
                    let name = winner
                        .and_then(|id| self.avatars.iter().find(|a| a.id == id))
                        .map(|a| a.name.clone());
                    godot_print!(
                        "server: match over, {}",
                        name.map_or("a draw".to_owned(), |name| format!("{name} won"))
                    );
                } else {
                    godot_print!(
                        "server: match over, {} ({} to {})",
                        winner.map_or("a draw".to_owned(), |t| format!("team {} won", t + 1)),
                        self.match_state.scores[0],
                        self.match_state.scores[1]
                    );
                }
                // Ready-up again only before the first match of a session.
                for client in &mut self.clients {
                    client.ready = true;
                }
                self.reset_flags();
            }
            Transition::NextCountdown => {}
        }
        self.broadcast_match();
        self.next_scoreboard = self.tick;
    }

    fn match_event(&self) -> GameEvent {
        let state = &self.match_state;
        let ends_at = (state.phase != Phase::Lobby)
            .then(|| self.tick + (state.time_left.max(0.0) * TICK_RATE as f32).round() as Tick);
        GameEvent::Match {
            phase: state.phase,
            ends_at,
            scores: state.scores,
            score_limit: match self.game {
                Mode::TeamDeathmatch | Mode::CaptureTheFlag => self.rules.score_limit,
                Mode::GunGame(_) => u32::from(gun_game::LEVELS),
            },
        }
    }

    fn broadcast_match(&mut self) {
        let event = self.match_event();
        self.broadcast_event(&event);
    }

    /// Sends everyone every player's score, kills, deaths, ping, and readiness.
    fn broadcast_scoreboard(&mut self) {
        self.next_scoreboard = self.tick + SCOREBOARD_INTERVAL;
        let rows: Vec<ScoreRow> = self
            .avatars
            .iter()
            .map(|a| {
                let client = self.client_of(a.id);
                ScoreRow {
                    id: a.id,
                    score: a.score.score,
                    kills: a.score.kills,
                    deaths: a.score.deaths,
                    ping_ms: client.map_or(0, |c| {
                        transport::round_trip_ms(&c.peer).clamp(0.0, 9999.0) as u16
                    }),
                    ready: client.is_none_or(|c| c.ready),
                    level: a.progress.level,
                    level_kills: a.progress.kills,
                    captures: a.score.captures,
                    returns: a.score.returns,
                }
            })
            .collect();
        self.broadcast_event(&GameEvent::Scoreboard { rows });
    }

    /// Who a player fights for: their team, or in free-for-all modes
    /// themselves (their id; bots read it as a team).
    fn side(&self, avatar: &Avatar) -> u8 {
        if self.game.free_for_all() {
            avatar.id
        } else {
            avatar.team
        }
    }

    /// Whether `a` and `b` are enemies.
    fn hostile(&self, a: &Avatar, b: &Avatar) -> bool {
        a.id != b.id && self.side(a) != self.side(b)
    }

    /// Gun Game: a kill by `id` counts toward their weapon. Moving up swaps
    /// the weapon in their hands for the next one, and the last weapon's last
    /// kill wins the match.
    fn climb(&mut self, id: u8, rules: &gun_game::Rules) -> Option<Transition> {
        if !self.match_state.scoring() {
            return None;
        }
        let avatar = self.avatars.iter_mut().find(|a| a.id == id)?;
        self.next_scoreboard = self.tick;
        match avatar.progress.on_kill(rules, self.tick) {
            Step::Counted => None,
            Step::Promoted => {
                let weapon = gun_game::weapon(avatar.progress.level);
                avatar.predicted.state.combat.rearm(weapon);
                None
            }
            Step::Won => Some(self.match_state.finish(&self.rules, Some(id))),
        }
    }

    /// Gun Game: a player killed themselves (a fall) and drops a weapon. They
    /// are dead, so the lower weapon comes with the respawn.
    fn on_suicide(&mut self, id: u8) {
        if !matches!(self.game, Mode::GunGame(_)) || !self.match_state.scoring() {
            return;
        }
        if let Some(avatar) = self.avatars.iter_mut().find(|a| a.id == id) {
            avatar.progress.on_suicide(self.tick);
            self.next_scoreboard = self.tick;
        }
    }

    /// Capture the Flag: moves the flags with their carriers and decides
    /// takes, drops, returns, and captures, then pays out and tells everyone.
    fn update_flags(&mut self) {
        let Some(flags) = &mut self.flags else {
            return;
        };
        let players: Vec<FlagPlayer> = self
            .avatars
            .iter()
            .map(|a| FlagPlayer {
                id: a.id,
                team: a.team,
                alive: a.predicted.state.alive(),
                position: a.predicted.position,
            })
            .collect();
        let before = *flags;
        let mut events = Vec::new();
        flags.tick(
            &self.flag_rules,
            TICK_DT,
            self.match_state.scoring(),
            &players,
            &mut events,
        );
        for event in events {
            self.on_flag_event(event);
        }
        if self.flags.is_some_and(|f| status_changed(&before, &f))
            && let Some(event) = self.flags_event()
        {
            self.broadcast_event(&event);
        }
    }

    fn on_flag_event(&mut self, event: FlagEvent) {
        let team_of =
            |avatars: &[Avatar], id: u8| avatars.iter().find(|a| a.id == id).map(|a| a.team);
        let (flag, play, player, award) = match event {
            FlagEvent::Taken {
                flag,
                by,
                from_base,
            } => (
                flag,
                FlagPlay::Taken,
                Some(by),
                // Only from the base, so dropping and retaking pays nothing.
                from_base.then_some(Award::FlagTaken),
            ),
            FlagEvent::Dropped { flag, by, at } => {
                let floor = self.floor_below(to_godot(at));
                if let Some(flags) = &mut self.flags {
                    flags.settle(flag, to_sim(floor));
                }
                (flag, FlagPlay::Dropped, Some(by), None)
            }
            FlagEvent::Returned { flag, by } => {
                (flag, FlagPlay::Returned, by, by.map(|_| Award::FlagReturn))
            }
            FlagEvent::Captured { flag, by } => {
                (flag, FlagPlay::Captured, Some(by), Some(Award::FlagCapture))
            }
        };
        godot_print!(
            "server: team {} flag {play:?}{}",
            flag + 1,
            player.map_or(String::new(), |p| format!(" by player {p}"))
        );
        self.broadcast_event(&GameEvent::FlagPlay { flag, play, player });
        if let Some(id) = player
            && let Some(award) = award
        {
            self.pay_flag_play(id, award);
        }
        if play == FlagPlay::Captured
            && let Some(team) = player.and_then(|id| team_of(&self.avatars, id))
        {
            match self.match_state.on_capture(&self.rules, team) {
                Some(end) => self.on_transition(end),
                None => self.broadcast_match(),
            }
        }
        self.next_scoreboard = self.tick;
    }

    /// Scores a flag play for a player and tells them.
    fn pay_flag_play(&mut self, id: u8, award: Award) {
        let rules = self.score_rules;
        let Some(avatar) = self.avatars.iter_mut().find(|a| a.id == id) else {
            return;
        };
        let Some(scored) = avatar.score.on_flag_play(&rules, award) else {
            return;
        };
        let totals = avatar.score.totals();
        let awards: Vec<Scored> = vec![scored];
        self.send_event(id, GameEvent::Score { awards, totals });
    }

    /// Where something dropped at `at` lands: the floor under it, or `at`.
    fn floor_below(&self, at: Vector3) -> Vector3 {
        let Some(mut space) = self
            .base()
            .get_world_3d()
            .and_then(|world| world.get_direct_space_state())
        else {
            return at;
        };
        let from = at + Vector3::UP * 0.5;
        world_hit(&mut space, from, at - Vector3::UP * FLAG_DROP_FALL).unwrap_or(at)
    }

    /// Both flags home, for the start and end of a match.
    fn reset_flags(&mut self) {
        let Some(flags) = &mut self.flags else {
            return;
        };
        flags.reset();
        if let Some(event) = self.flags_event() {
            self.broadcast_event(&event);
        }
    }

    /// Where both flags are, for clients (Capture the Flag only).
    fn flags_event(&self) -> Option<GameEvent> {
        let flags = self.flags.as_ref()?;
        let view = |team: usize| {
            let flag = &flags.flags[team];
            FlagView {
                home: flag.home,
                status: match flag.state {
                    FlagState::Home => FlagStatus::Home,
                    FlagState::Carried { by } => FlagStatus::Carried { by },
                    FlagState::Dropped { at, return_in } => FlagStatus::Dropped {
                        at,
                        returns_at: self.tick + (return_in.max(0.0) * TICK_RATE as f32) as Tick,
                    },
                },
            }
        };
        Some(GameEvent::Flags {
            flags: [view(0), view(1)],
        })
    }

    /// What each playing bot should go for in Capture the Flag (nothing in
    /// Team Deathmatch, where bots roam and hunt).
    ///
    /// - A carrier runs the flag home.
    /// - When a team's flag lies dropped, its two nearest bots go to return it.
    /// - Otherwise a third of each team (at least one bot in teams of three or
    ///   more) defends: it holds spots near its flag, or chases the enemy
    ///   carrying it. The rest attack: they go for the enemy's flag, or
    ///   escort the teammate carrying it.
    fn bot_objectives(&self) -> Vec<(u8, Option<BotObjective>)> {
        let Some(flags) = &self.flags else {
            return Vec::new();
        };
        let position = |id: u8| {
            self.avatars
                .iter()
                .find(|a| a.id == id)
                .map(|a| to_godot(a.predicted.position))
        };
        let mut out = Vec::new();
        for team in 0..tdm::TEAM_COUNT as Team {
            let own = flags.flag(team);
            let enemy = flags.flag(1 - team);
            let mut bots: Vec<&Avatar> = self
                .avatars
                .iter()
                .filter(|a| a.team == team && a.brain.is_some())
                .collect();
            bots.sort_unstable_by_key(|a| a.id);
            let team_size = self.avatars.iter().filter(|a| a.team == team).count();
            // The bots nearest the team's dropped flag go to return it.
            let returners: Vec<u8> = match own.state {
                FlagState::Dropped { at, .. } => {
                    let mut by_distance: Vec<(f32, u8)> = bots
                        .iter()
                        .map(|a| (a.predicted.position.distance(at), a.id))
                        .collect();
                    by_distance.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
                    by_distance.iter().take(2).map(|(_, id)| *id).collect()
                }
                _ => Vec::new(),
            };
            for (i, bot) in bots.iter().enumerate() {
                let defender = team_size >= 3 && i % 3 == 0;
                let objective = if flags.carried_by(bot.id).is_some() {
                    BotObjective::run(to_godot(own.home))
                } else if returners.contains(&bot.id) {
                    BotObjective::run(to_godot(own.position))
                } else if defender {
                    match own.state {
                        FlagState::Carried { by } => {
                            BotObjective::hunt(position(by).unwrap_or(to_godot(own.home)))
                        }
                        _ => BotObjective::hold(self.defend_spot(bot.id, own.home)),
                    }
                } else {
                    match enemy.state {
                        FlagState::Carried { by } => {
                            BotObjective::hunt(position(by).unwrap_or(to_godot(own.home)))
                        }
                        // Attackers grab a dropped flag as urgently as a returner.
                        FlagState::Dropped { at, .. } => BotObjective::run(to_godot(at)),
                        FlagState::Home => BotObjective::hunt(to_godot(enemy.home)),
                    }
                };
                out.push((bot.id, Some(objective)));
            }
        }
        out
    }

    /// A spot near `home` for defender `id` to hold, changing every few
    /// seconds.
    fn defend_spot(&self, id: u8, home: Vec3) -> Vector3 {
        let period = self.tick / (DEFEND_SPOT_SECONDS * TICK_RATE);
        let angle = rng::unit(u32::from(id), period) * std::f32::consts::TAU;
        let offset = Vector3::new(angle.cos(), 0.0, angle.sin()) * DEFEND_RADIUS;
        to_godot(home) + offset
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
            .map(|a| PlayerView {
                hits: a.hits,
                hit_heading: a.hit_heading,
                ..PlayerView::from_state(a.id, &a.predicted, a.yaw, a.pitch)
            })
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

fn joined_event(avatar: &Avatar) -> GameEvent {
    GameEvent::Joined {
        id: avatar.id,
        name: avatar.name.clone(),
        team: avatar.team,
        bot: avatar.bot,
    }
}

fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Lobby => "lobby",
        Phase::Countdown => "countdown",
        Phase::Playing => "playing",
        Phase::Ended { .. } => "ended",
    }
}

fn default_primary() -> WeaponId {
    WeaponId::expect("m4")
}

/// Every weapon a loadout may use as its primary.
fn loadout_primaries() -> Vec<WeaponId> {
    (0..weapon::table().len())
        .filter_map(|i| u8::try_from(i).ok().map(WeaponId))
        .filter(|id| is_loadout_primary(*id))
        .collect()
}

/// Whether players may choose this weapon as their primary: any primary but
/// the secret laser.
pub(crate) fn is_loadout_primary(id: WeaponId) -> bool {
    let def = id.def();
    def.slot == Slot::Primary && !def.beam
}

/// The map's spawn points: `Node3D` children of a `SpawnPoints` node, each
/// facing the way players should, with an optional `team` metadata (0 or 1;
/// none or -1 for either team).
fn map_spawns(map: &Gd<Node>) -> Vec<SpawnPoint> {
    let Some(parent) = map
        .get_node_or_null("SpawnPoints")
        .and_then(|n| n.try_cast::<Node3D>().ok())
    else {
        return Vec::new();
    };
    let base = parent.get_transform();
    parent
        .get_children()
        .iter_shared()
        .filter_map(|child| child.try_cast::<Node3D>().ok())
        .map(|point| {
            let transform = base * point.get_transform();
            let team = point
                .get_meta_ex("team")
                .default(&(-1).to_variant())
                .done()
                .try_to::<i32>()
                .ok()
                .and_then(|t| Team::try_from(t).ok())
                .filter(|t| usize::from(*t) < tdm::TEAM_COUNT);
            SpawnPoint {
                position: to_sim(transform.origin),
                yaw: transform.basis.get_euler().y,
                team,
            }
        })
        .collect()
}

/// Spawn points for maps that have none: team 1 in a line through the player
/// start, facing down the map (-Z), and team 2 14 metres further on, facing
/// back. The first slots are nearest the middle.
fn fallback_spawns(start: Vector3) -> Vec<SpawnPoint> {
    [0.0, 1.0, -1.0, 2.0, -2.0, 3.0]
        .into_iter()
        .flat_map(|offset: f32| {
            let x = offset * 2.0;
            [
                SpawnPoint {
                    position: to_sim(start + Vector3::new(x, 0.0, 0.0)),
                    yaw: 0.0,
                    team: Some(0),
                },
                SpawnPoint {
                    position: to_sim(start + Vector3::new(x * 1.5, 0.0, -14.0)),
                    yaw: PI,
                    team: Some(1),
                },
            ]
        })
        .collect()
}

/// The map's flag bases for Capture the Flag: `Node3D` children of a `Flags`
/// node with a `team` metadata of 0 or 1. `None` unless both teams have one.
fn map_flags(map: &Gd<Node>) -> Option<[Vec3; tdm::TEAM_COUNT]> {
    let parent = map
        .get_node_or_null("Flags")
        .and_then(|n| n.try_cast::<Node3D>().ok())?;
    let base = parent.get_transform();
    let mut homes: [Option<Vec3>; tdm::TEAM_COUNT] = [None; tdm::TEAM_COUNT];
    for child in parent.get_children().iter_shared() {
        let Ok(marker) = child.try_cast::<Node3D>() else {
            continue;
        };
        let team = marker
            .get_meta_ex("team")
            .default(&(-1).to_variant())
            .done()
            .try_to::<i32>()
            .ok()
            .and_then(|t| usize::try_from(t).ok())
            .filter(|t| *t < tdm::TEAM_COUNT);
        if let Some(team) = team {
            homes[team] = Some(to_sim((base * marker.get_transform()).origin));
        }
    }
    Some([homes[0]?, homes[1]?])
}

/// Flag bases for maps that mark none: the middle of each team's spawn
/// points (or of all of them, for a team without its own).
fn fallback_flag_homes(points: &[SpawnPoint]) -> [Vec3; tdm::TEAM_COUNT] {
    let centre = |team: Team| {
        let own: Vec<Vec3> = points
            .iter()
            .filter(|p| p.team == Some(team))
            .map(|p| p.position)
            .collect();
        let chosen: Vec<Vec3> = if own.is_empty() {
            points.iter().map(|p| p.position).collect()
        } else {
            own
        };
        chosen.iter().copied().sum::<Vec3>() / chosen.len().max(1) as f32
    };
    [centre(0), centre(1)]
}

/// Whether a flag changed hands or place in a way clients must hear about
/// (not just a carrier walking, or a dropped flag's clock running down).
fn status_changed(before: &Flags, after: &Flags) -> bool {
    before.flags.iter().zip(&after.flags).any(|(a, b)| {
        !matches!((a.state, b.state), (FlagState::Home, FlagState::Home))
            && !matches!(
                (a.state, b.state),
                (FlagState::Carried { by: x }, FlagState::Carried { by: y }) if x == y
            )
            && !matches!(
                (a.state, b.state),
                (FlagState::Dropped { at: x, .. }, FlagState::Dropped { at: y, .. }) if x == y
            )
    })
}
