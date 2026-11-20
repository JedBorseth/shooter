//! Connection to a dedicated server.
//!
//! Each physics tick, after the local `FpsPlayer` has predicted its move, the
//! client reads server packets, reconciles the player against the newest
//! snapshot, feeds remote players' states into their interpolation buffers,
//! and sends every unacknowledged input. Each rendered frame it draws remote
//! players as little in the past as the connection allows (about 35 to 50 ms
//! on a clean link; see `SnapshotClock`).
//!
//! The server decides hits and kills and sends them as reliable game events:
//! the client passes them to the local player (kill confirmations, score,
//! damage taken) and emits `kill_reported` for every kill, for a kill feed.
//! It also keeps the match state the HUD shows: the mode, who is playing on
//! which team (`roster`), the match phase, clock, and team scores
//! (`match_info`), and the scoreboard, with each player's Gun Game level.
//! In Capture the Flag it also keeps where both flags are (`flag_info`), and
//! emits `flag_played` for every take, drop, return, and capture.
//! In Hardpoint it also keeps the map's zones, the live one, and who holds it
//! (`hardpoint_info`), and in Domination the flags and their captures
//! (`domination_info`).
//! In Search and Destroy it also keeps the round (`round_info`), and holds
//! the local player still during a round's freeze, as the server does, and
//! the charge (`charge_info`): its sites, where it is, and who is planting or
//! defusing. It works out each tick whether holding interact would plant or
//! defuse where the local player stands, which holds them still as the server
//! does (`FpsPlayer::set_charge_spot`).
//! With the F4 hitbox overlay on, it also draws, in white, the hitboxes the
//! server rewound for each of our shots, next to the ones drawn here.
//!
//! An optional link conditioner adds latency, jitter, and loss in both
//! directions, to try the netcode at realistic pings against a local server.

use std::collections::VecDeque;

use godot::classes::{ENetConnection, ENetPacketPeer, INode, Input, PackedScene, Time};
use godot::global::Error;
use godot::prelude::*;
use shooter_sim::class::{self, Class, DefaultClass, Gun, Wildcard};
use shooter_sim::domination::DominationRules;
use shooter_sim::equipment::Kind;
use shooter_sim::gun_game;
use shooter_sim::hardpoint::Holder;
use shooter_sim::hitbox::HitboxPose;
use shooter_sim::killcam::{self, Killcam, OwnRecorder, Tape};
use shooter_sim::net::conditioner::{LinkConditioner, LinkSettings};
use shooter_sim::net::discovery::clean_text;
use shooter_sim::net::interpolation::{InterpolationBuffer, SnapshotClock};
use shooter_sim::net::prediction::Reconciled;
use shooter_sim::net::protocol::{
    self as protocol, AirView, Baseline, ChargeStatus, ChargeView, ClientMessage, DominationView,
    FlagPlay, FlagPointView, FlagStatus, FlagView, GameEvent, InputPacket, KillCause,
    MAX_BUILD_BYTES, PROTOCOL_VERSION, RadarView, ScavengerEvent, ScoreRow, ServerMessage,
    Snapshot, StreakEvent, ZombiesEvent, ZoneView,
};
use shooter_sim::net::snapshot_history::SnapshotHistory;
use shooter_sim::net::{CHANNEL_COUNT, CHANNEL_RELIABLE, DEFAULT_PORT, TICK_RATE, Tick};
use shooter_sim::perk::{Perk, Perks};
use shooter_sim::snd::{
    Charge, ChargeContext, ChargePlayer, ChargeState, ChargeUse, RoundEnd, RoundPhase, Site,
    SndRules, UseKind,
};
use shooter_sim::streak::{Loadout, Streak};
use shooter_sim::tdm::{Mode, Period, Phase, TEAM_COUNT, Team};
use shooter_sim::weapon::{Slot, WeaponId};

use super::class_dict::{class_from_dict, class_to_dict, error_text};
use super::horde_view::HordeView;
use super::killcam::{Playing, WeaponCue};
use super::props::{PropSnapshot, PropsBridge};
use super::remote::{RemotePlayer, RemoteSample};
use super::transport::{self, NetEvent};
use crate::convert::{to_godot, to_sim};
use crate::hitbox_rig::{self, HitboxRig};
use crate::player::FpsPlayer;

/// What the kill feed calls a knife kill.
const KNIFE_NAME: &str = "Knife";
/// What the kill feed calls a kill by Search and Destroy's charge going off.
const CHARGE_NAME: &str = "Charge";

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
    /// The server turned us away (`denied_reason`).
    Denied,
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
    /// Our account session token (`Online.session_token()`), or empty when
    /// not signed in. A server that checks accounts needs it.
    #[export]
    account_token: GString,
    /// Primary weapon to ask for when joining (a weapon key, attachment
    /// variants included), or empty for the default class's. Ignored once a
    /// whole class is set (`set_class`).
    #[export]
    primary: GString,
    /// The tactical to ask for ("flash" or "smoke"); anything else is a
    /// flashbang.
    #[export]
    #[init(val = GString::from("flash"))]
    tactical: GString,
    /// The class asked for with `set_class`; until then one is built from
    /// `primary` and `tactical`.
    class: Option<Class>,
    /// The local `FpsPlayer` to predict.
    #[export]
    player_path: NodePath,
    /// Instanced for every other player.
    #[export]
    remote_player_scene: Option<Gd<PackedScene>>,
    /// A floor on how far in the past remote players are drawn, in
    /// milliseconds. Zero lets the delay follow the connection alone (snapshot
    /// rate, lost snapshots, and jitter); raise it only to try a longer delay.
    #[export]
    #[init(val = 0.0)]
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
    /// Respawn without playing killcams after our deaths (the player's
    /// setting); ending killcams still play.
    #[var]
    skip_killcams: bool,

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
    /// Reliable gameplay events wait until a local pause ends.
    paused_packets: Vec<Vec<u8>>,
    stats: Stats,
    /// Whether the server was last told to send rewound hitboxes.
    debug_requested: bool,
    /// The server's rewound hitboxes for recent shots, with seconds left.
    ghosts: VecDeque<(Gd<HitboxRig>, f32)>,
    /// Everyone in the game, ourselves included.
    roster: Vec<RosterEntry>,
    match_phase: Option<Phase>,
    match_ends_at: Option<Tick>,
    /// Regulation, the second half, or overtime.
    match_period: Period,
    /// Capture the Flag: both flags, by team.
    flags: Option<[FlagView; TEAM_COUNT]>,
    team_scores: [u32; TEAM_COUNT],
    score_limit: u32,
    /// The server's game, from its welcome.
    mode: Mode,
    /// The server's build, from its welcome.
    server_build: String,
    /// The server reports our match XP to our account (its welcome).
    ranked: bool,
    /// Why the server turned us away.
    denied_reason: String,
    /// Hardpoint: the map's zones, the live one, who holds it, and the
    /// server tick it moves on.
    zones: Vec<ZoneView>,
    active_zone: u8,
    holder: Holder,
    zone_moves_at: Option<Tick>,
    /// Domination: the map's flags, and who owns each as the server last
    /// said.
    dom_points: Vec<FlagPointView>,
    dom: Option<DominationView>,
    /// The map's interactive props manager, when the map has one.
    props: PropsBridge,
    /// Physics ticks until trying to find the props manager again.
    props_retry: u32,
    /// Population last sent to the props manager.
    props_population: Option<i32>,
    /// The current Search and Destroy round, while a match is being played.
    round: Option<Round>,
    /// The last few seconds of snapshots, ourselves included, for killcams.
    tape: Tape,
    own_recorder: OwnRecorder,
    /// Our view angles in the latest snapshot, for one whose input is gone.
    own_angles: (f32, f32),
    /// The killcam the server sent last, with the time it arrived, until
    /// it starts playing.
    killcam: Option<(Killcam, f64)>,
    /// The killcam playing.
    playing: Option<Playing>,
    /// The kind of the killcam playing, for the HUD.
    playing_kind: Option<killcam::Kind>,
    /// Search and Destroy: the map's charge sites, and the charge as the
    /// server last told us.
    charge_sites: Vec<ZoneView>,
    charge: Option<ChargeView>,
    /// Scavenger's packs on the ground: id, where, and who dropped it.
    scavenger_packs: Vec<(u16, shooter_sim::Vec3, u8)>,
    /// The minimap's enemies from the newest snapshot, and when (seconds,
    /// engine clock) it came.
    radar: RadarView,
    radar_at: f64,
    /// Scorestreaks: the ones chosen (sent on connecting and whenever they
    /// change), our count and held streaks as the server last said, which
    /// held one the use button calls in, and every UAV and Counter-UAV up.
    streak_choice: Loadout,
    streak_progress: u32,
    streak_held: Vec<Streak>,
    streak_loadout: Loadout,
    streak_selected: usize,
    sky: Vec<AirView>,
    /// Care Packages out (`shooter_sim::care_package`), as the server said.
    crates: Vec<CrateView>,
    /// Zombies: the horde as drawn, and the map's director mirroring the
    /// server's game.
    horde: Option<HordeView>,
}

/// A Care Package as the client knows it.
#[derive(Clone, Copy)]
struct CrateView {
    id: u16,
    owner: u8,
    at: shooter_sim::Vec3,
    heading: f32,
    drop_at: Tick,
    lands_at: Tick,
    contents: Streak,
    /// Who is capturing it, from and until which ticks.
    capture: Option<(u8, Tick, Tick)>,
}

#[derive(Clone, Copy)]
struct Round {
    number: u32,
    phase: RoundPhase,
    ends_at: Tick,
    attackers: Team,
    halftime_after: u32,
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
        self.props = PropsBridge::discover_in_tree(&self.base().get_tree());
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
        let paused = self.base().get_tree().is_paused();
        let now = Time::singleton().get_ticks_usec() as f64 / 1e6;
        self.sync_props();

        if !paused {
            for bytes in std::mem::take(&mut self.paused_packets) {
                self.handle_packet(&bytes);
            }
        }
        let mut events = std::mem::take(&mut self.net_events);
        events.clear();
        transport::service(&mut host, &mut events);
        for event in events.drain(..) {
            match event {
                NetEvent::Connect(mut peer) => {
                    let hello = ClientMessage::Hello {
                        version: PROTOCOL_VERSION,
                        name: self.player_name.to_string(),
                        account: self.account_token.to_string(),
                    };
                    transport::send_reliable(&mut peer, &hello.encode());
                    let class = ClientMessage::Class(self.current_class());
                    transport::send_reliable(&mut peer, &class.encode());
                    let streaks = ClientMessage::Streaks(self.streak_choice);
                    transport::send_reliable(&mut peer, &streaks.encode());
                }
                NetEvent::Disconnect(_) => self.on_disconnected(),
                NetEvent::Receive(_, bytes, channel) => {
                    if paused {
                        if channel == CHANNEL_RELIABLE {
                            self.paused_packets.push(bytes);
                        }
                        continue;
                    }
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
                if !paused {
                    self.handle_packet(&bytes);
                }
            }
        }

        if self.status == Status::Joined && !paused {
            let revive = self.revive_spot();
            let spot = self.charge_spot().is_some() || revive.is_some();
            if let Some(player) = &mut self.player {
                player.bind_mut().set_charge_spot(spot);
            }
            if self.horde.is_some() {
                let name = revive.map(|id| self.name_of(i32::from(id)).to_string());
                if let Some(horde) = &mut self.horde {
                    horde
                        .director()
                        .bind_mut()
                        .net_revive_prompt(name.as_deref().unwrap_or(""));
                }
            }
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
        if self.base().get_tree().is_paused()
            || self.status != Status::Joined
            || self.newest.is_none()
        {
            return;
        }
        let Some(clock) = &mut self.clock else {
            return;
        };
        let render_tick = clock.advance(delta as f32);
        // A fire press that no tick has acted on yet aims at the frame it was
        // clicked on, and the server rewinds hitboxes to that frame's tick.
        // Until the tick fires it, hitboxes stay posed there (bodies still
        // move on), so the local hit test agrees with the server's.
        let pressed_at = self
            .player
            .as_ref()
            .and_then(|player| player.bind().pending_press_view_tick())
            .filter(|tick| *tick < render_tick);
        for remote in &mut self.remotes {
            // Sampled first: the buffer drops snapshots older than a sample.
            let held = pressed_at.and_then(|tick| remote.buffer.sample(tick));
            if let Some(sample) = remote.buffer.sample(render_tick) {
                remote.node.bind_mut().apply(&sample, delta as f32);
            }
            if let Some(held) = held {
                remote.node.bind_mut().hold_hitboxes(&held);
            }
        }
        if let Some(horde) = &mut self.horde {
            horde.draw(render_tick);
        }
        // Shots fired before the next frame aim at what was just drawn.
        if let Some(player) = &mut self.player {
            player.bind_mut().set_view_tick(render_tick);
        }
        self.age_ghosts(delta as f32);
        self.update_killcam(delta as f32);
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

    /// Capture the Flag: something happened to team `flag`'s flag. `play` is
    /// "taken", "dropped", "returned", or "captured"; `player` did it (-1 for
    /// a flag that went home by itself).
    #[signal]
    fn flag_played(flag: i32, play: GString, player: i32);
    /// A chat message from player `from` (named `name`), to everyone or to
    /// their team.
    #[signal]
    fn chat_received(from: i32, name: GString, text: GString, team: bool);

    /// Hardpoint: the live zone moved, or changed hands.
    #[signal]
    fn hardpoint_changed();

    /// Domination: a flag changed owner, or a capture started, stalled, or
    /// changed speed.
    #[signal]
    fn domination_changed();

    /// A Search and Destroy round changed phase.
    #[signal]
    fn round_changed();

    /// A killcam started playing: `info` as in `killcam_info`.
    #[signal]
    fn killcam_started(info: VarDictionary);

    /// The killcam ended (finished, skipped, or cut short by a respawn).
    #[signal]
    fn killcam_ended();
    /// Search and Destroy: the charge changed hands, was dropped, planted,
    /// defused, or went off, or someone started or stopped planting or
    /// defusing it (`charge_info`).
    #[signal]
    fn charge_changed();

    /// Scavenger: a pack was dropped, taken, or went (`scavenger_packs`).
    /// `taken_by` is who took one (-1 when none was).
    #[signal]
    fn scavenger_changed(taken_by: i32);

    /// Scorestreaks: our count or held streaks changed (`streak_info`).
    /// `earned` is the key of a streak just earned, or "".
    #[signal]
    fn streaks_changed(earned: GString);

    /// A UAV or Counter-UAV went up or left (`sky_info`). `called` is the
    /// key of one just called in, and `ours` whether by our side ("" and
    /// false when one left).
    #[signal]
    fn sky_changed(called: GString, ours: bool);

    /// A Care Package was called in, captured, taken, or ran out
    /// (`crates`). `taken_by` is who took one and `contents` the streak's
    /// key (-1 and "" otherwise).
    #[signal]
    fn crates_changed(taken_by: i32, contents: GString);

    /// Player `owner` called an Airstrike: a strike lands on each of
    /// `points` in `seconds`.
    #[signal]
    fn airstrike_called(owner: i32, points: PackedVector3Array, seconds: PackedFloat32Array);

    /// A strike of `owner`'s Airstrike went off at `at`.
    #[signal]
    fn strike_landed(owner: i32, at: Vector3);

    /// Every player, ourselves included, as dictionaries with `id`, `name`,
    /// `team`, `bot`, `me`, `score`, `kills`, `deaths`, `ping`, `ready`,
    /// `alive`, for Capture the Flag `captures` and `returns` (Domination:
    /// `captures` and `defends`), for Hardpoint
    /// `hill_seconds`, and for Gun Game `level`, `level_kills`, and `weapon`
    /// (the name of the weapon on their level). Sorted by team and then by
    /// score, or in Gun Game by how far up the ladder, with the winner first
    /// once it ends.
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
                d.set("captures", e.row.captures);
                d.set("returns", e.row.returns);
                d.set("defends", e.row.defends);
                d.set("revives", e.row.revives);
                d.set("headshots", e.row.headshots);
                d.set("ping", i32::from(e.row.ping_ms));
                d.set("ready", e.row.ready);
                d.set("level", i32::from(e.row.level));
                d.set("level_kills", i32::from(e.row.level_kills));
                let weapon = gun_game::weapon(e.row.level).def();
                d.set("weapon", &GString::from(weapon.name.as_str()));
                d.set("hill_seconds", e.row.hill_seconds);
                d.set("alive", e.row.alive);
                d
            })
            .collect()
    }

    /// The match as a dictionary: `mode` ("tdm", "ctf", "hardpoint",
    /// "gungame", or "snd"), `mode_name`, `free_for_all`, `phase` ("lobby",
    /// "countdown", "playing", "ended", or "" before the server said),
    /// `seconds_left` (0 without a clock), `scores` (per team: kills,
    /// captures, points, or rounds won),
    /// `score_limit`, `winner` (a team, or in free-for-all modes a player id;
    /// -1 for a draw), `my_team`, `period` ("regulation", "second_half", or
    /// "overtime"), `timed_overtime` (overtime with a clock of its own), and
    /// for Gun Game `kills_per_weapon`.
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
        d.set("seconds_left", self.seconds_until(self.match_ends_at));
        let (period, timed) = match self.match_period {
            Period::Regulation => ("regulation", false),
            Period::SecondHalf => ("second_half", false),
            Period::Overtime { timed } => ("overtime", timed),
        };
        d.set("period", period);
        d.set("timed_overtime", timed);
        let scores: PackedInt32Array = self.team_scores.iter().map(|s| *s as i32).collect();
        d.set("scores", &scores);
        d.set("score_limit", self.score_limit);
        d.set("my_team", self.local_team().map_or(-1, i32::from));
        d
    }

    /// Capture the Flag: both flags, by team, as dictionaries with `team`,
    /// `status` ("home", "carried", or "dropped"), `carrier` (a player id, or
    /// -1), `home` (its base), `position` (where to draw it now: its base,
    /// where it lies, or its carrier's feet as drawn here), and
    /// `return_seconds` (until a dropped flag goes home). Empty in other modes.
    #[func]
    fn flag_info(&self) -> Array<VarDictionary> {
        let Some(flags) = &self.flags else {
            return Array::new();
        };
        flags
            .iter()
            .enumerate()
            .map(|(team, flag)| {
                let mut d = VarDictionary::new();
                d.set("team", team as i32);
                let home = to_godot(flag.home);
                d.set("home", home);
                let (status, carrier, position, seconds) = match flag.status {
                    FlagStatus::Home => ("home", -1, home, 0.0),
                    FlagStatus::Carried { by } => (
                        "carried",
                        i32::from(by),
                        self.carrier_feet(by).unwrap_or(home),
                        0.0,
                    ),
                    FlagStatus::Dropped { at, returns_at } => {
                        let left = self.newest.map_or(0.0, |now| {
                            f64::from(returns_at.saturating_sub(now)) / f64::from(TICK_RATE)
                        });
                        ("dropped", -1, to_godot(at), left)
                    }
                };
                d.set("status", status);
                d.set("carrier", carrier);
                d.set("position", position);
                d.set("return_seconds", seconds);
                d
            })
            .collect()
    }

    /// Hardpoint, as a dictionary: `zones` (each with `name`, `center`, and
    /// `size`, in rotation order; empty in other modes), `active` and `next`
    /// (indices into `zones`), `holder` ("neutral", "ally", "enemy", or
    /// "contested"), and `seconds_to_move` (-1 while the clock is stopped).
    #[func]
    fn hardpoint_info(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let zones: Array<VarDictionary> = self
            .zones
            .iter()
            .map(|zone| {
                let mut entry = VarDictionary::new();
                entry.set("name", &GString::from(zone.name.as_str()));
                entry.set("center", to_godot(zone.center));
                entry.set("size", to_godot(zone.half_extents * 2.0));
                entry
            })
            .collect();
        d.set("zones", &zones);
        let active = usize::from(self.active_zone);
        d.set("active", active as i32);
        let next = if self.zones.is_empty() {
            0
        } else {
            (active + 1) % self.zones.len()
        };
        d.set("next", next as i32);
        let holder = match self.holder {
            Holder::Neutral => "neutral",
            Holder::Contested => "contested",
            Holder::Held(team) if Some(team) == self.local_team() => "ally",
            Holder::Held(_) => "enemy",
        };
        d.set("holder", holder);
        let seconds = match (self.zone_moves_at, self.newest) {
            (Some(at), Some(now)) => f64::from(at.saturating_sub(now)) / f64::from(TICK_RATE),
            _ => -1.0,
        };
        d.set("seconds_to_move", seconds);
        d
    }

    /// Domination, as a dictionary: `flags` (empty in other modes), each with
    /// `name`, `center`, `floor` (the circle's floor height), `radius`,
    /// `owner` and `claim` ("ally", "enemy", or "neutral"), `progress` (the
    /// capture, 0 to 1, run on from the server's last word at its speed),
    /// `contested`, and `capturers`; and `swapped` (halftime has passed).
    #[func]
    fn domination_info(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let rules = DominationRules::default();
        let local = self.local_team();
        let side = |team: Option<Team>| match team {
            None => "neutral",
            Some(t) if Some(t) == local => "ally",
            Some(_) => "enemy",
        };
        let since = match (&self.dom, self.newest) {
            (Some(view), Some(now)) => {
                f32::from(u16::try_from(now.saturating_sub(view.tick)).unwrap_or(u16::MAX))
                    / TICK_RATE as f32
            }
            _ => 0.0,
        };
        let flags: Array<VarDictionary> = self
            .dom_points
            .iter()
            .enumerate()
            .map(|(i, point)| {
                let status = self
                    .dom
                    .as_ref()
                    .and_then(|v| v.flags.get(i))
                    .copied()
                    .unwrap_or_default();
                let progress = (f32::from(status.progress) / 255.0
                    + since * f32::from(status.capturers) / rules.capture_seconds)
                    .clamp(0.0, 1.0);
                let mut entry = VarDictionary::new();
                entry.set("name", &GString::from(point.name.as_str()));
                entry.set("center", to_godot(point.center));
                entry.set("floor", point.center.y - point.half_height);
                entry.set("radius", point.radius);
                entry.set("owner", side(status.owner));
                entry.set("claim", side(status.claim));
                entry.set("progress", progress);
                entry.set("contested", status.contested);
                entry.set("capturers", i32::from(status.capturers));
                entry
            })
            .collect();
        d.set("flags", &flags);
        d.set("swapped", self.dom.as_ref().is_some_and(|v| v.swapped));
        d
    }

    /// The Search and Destroy round as a dictionary: `number` (0 when no
    /// round is on), `phase` ("freeze", "live", "over", or ""),
    /// `seconds_left` (the fuse, once the charge is planted), `attacking`
    /// (whether our team attacks), `winner` (the team that took the round,
    /// or -1), `reason` ("eliminated", "time", "detonated", or "defused"),
    /// and `halftime` (whether the teams swap sides after this round).
    #[func]
    fn killcam_info(&self) -> VarDictionary {
        let mut info = VarDictionary::new();
        let Some(playing) = &self.playing else {
            return info;
        };
        let playback = &playing.playback;
        let k = playback.killcam();
        let kind = match k.kind {
            killcam::Kind::Respawn => "respawn",
            killcam::Kind::Round => "round",
            killcam::Kind::Final => "final",
        };
        let def = k.weapon.def();
        let base = crate::viewmodel::base_weapon(k.weapon).def().name.clone();
        let weapon = match k.cause {
            KillCause::Knife => KNIFE_NAME.to_owned(),
            KillCause::Equipment(kind) => kind.name().to_owned(),
            KillCause::Charge => CHARGE_NAME.to_owned(),
            KillCause::Streak(streak) => streak.label().to_owned(),
            KillCause::Weapon => base,
        };
        let attachment = match k.cause {
            KillCause::Weapon => def
                .fitted
                .iter()
                .map(|a| a.label())
                .collect::<Vec<_>>()
                .join("  ·  "),
            _ => String::new(),
        };
        info.set("kind", kind);
        info.set("killer", i32::from(k.killer));
        info.set("killer_name", &self.name_of(i32::from(k.killer)));
        info.set("killer_team", self.team_of(i32::from(k.killer)));
        info.set("victim", i32::from(k.victim));
        info.set("victim_name", &self.name_of(i32::from(k.victim)));
        info.set("weapon", weapon.as_str());
        info.set("attachment", attachment.as_str());
        info.set("headshot", k.headshot);
        info.set("progress", playback.progress());
        info.set("past_kill", playback.past_kill());
        info.set("skippable", !k.kind.ending());
        info.set("me_killer", Some(k.killer) == self.player_id);
        info
    }

    /// Whether a killcam is playing.
    #[func]
    fn in_killcam(&self) -> bool {
        self.playing.is_some()
    }

    #[func]
    fn round_info(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(round) = self.round else {
            d.set("number", 0);
            d.set("phase", "");
            d.set("seconds_left", 0.0);
            d.set("attacking", false);
            d.set("winner", -1);
            d.set("reason", "");
            d.set("halftime", false);
            return d;
        };
        let (phase, winner, reason) = match round.phase {
            RoundPhase::Freeze => ("freeze", -1, ""),
            RoundPhase::Live => ("live", -1, ""),
            RoundPhase::Over { winner, reason } => (
                "over",
                i32::from(winner),
                match reason {
                    RoundEnd::Eliminated => "eliminated",
                    RoundEnd::Time => "time",
                    RoundEnd::Detonated => "detonated",
                    RoundEnd::Defused => "defused",
                },
            ),
        };
        d.set("number", round.number);
        d.set("phase", phase);
        d.set("seconds_left", self.seconds_until(Some(round.ends_at)));
        d.set("attacking", self.local_team() == Some(round.attackers));
        d.set("winner", winner);
        d.set("reason", reason);
        d.set("halftime", round.number == round.halftime_after);
        d
    }

    /// Search and Destroy's charge as a dictionary: `sites` (each with
    /// `name`, `center`, and `size`; empty in other modes), `status` ("",
    /// "carried", "dropped", "planted", "detonated", or "defused"),
    /// `carrier` (who carries it, -1 for nobody or someone we are not told
    /// of), `carrying` (we do), `position` (where it lies or stands, or its
    /// carrier's feet when we can see them; `has_position` says whether
    /// there is one), `site` (where it was planted, or -1), `user` (who is
    /// planting or defusing, -1 for nobody we are told of), `use_kind`
    /// ("plant" or "defuse"), `use_progress` (0 to 1), and `spot` ("plant",
    /// "defuse", or "": what holding interact would do where we stand).
    /// The enemies our minimap shows, as the server decided
    /// (`shooter_sim::radar`): dictionaries with `id`, `position` (a
    /// `Vector3` on the ground plane, y 0), `kind` ("fire" or "sweep"), and `alpha`
    /// (1, fading to 0 over the dot's last second).
    #[func]
    fn radar_blips(&self) -> Array<VarDictionary> {
        let now = Time::singleton().get_ticks_usec() as f64 / 1e6;
        let since = (now - self.radar_at).max(0.0) as f32;
        self.radar
            .blips
            .iter()
            .filter_map(|blip| {
                let life = blip.kind.life() as f32 / TICK_RATE as f32;
                let age = blip.age as f32 / TICK_RATE as f32 + since;
                let alpha = (life - age).clamp(0.0, 1.0);
                if alpha <= 0.0 {
                    return None;
                }
                let at = blip.position();
                let mut d = VarDictionary::new();
                d.set("id", i32::from(blip.id));
                d.set("position", Vector3::new(at.x, 0.0, at.y));
                d.set(
                    "kind",
                    match blip.kind {
                        shooter_sim::radar::BlipKind::Fire => "fire",
                        shooter_sim::radar::BlipKind::Sweep => "sweep",
                    },
                );
                d.set("alpha", alpha);
                Some(d)
            })
            .collect()
    }

    /// Our scorestreaks, as a dictionary: `progress` (score toward the next
    /// this life), `next` (its key, "" with none chosen), `next_cost`,
    /// `loadout` (the keys counted toward, cheapest first), `held` (the keys
    /// held, oldest first), and `selected` (the index in `held` the use
    /// button calls in, -1 with none held).
    #[func]
    fn streak_info(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        d.set("progress", self.streak_progress);
        let next = self
            .streak_loadout
            .streaks()
            .find(|s| s.cost() > self.streak_progress);
        d.set("next", next.map_or("", |s| s.key()));
        d.set("next_cost", next.map_or(0, |s| s.cost()));
        let keys = |list: &mut dyn Iterator<Item = Streak>| -> PackedStringArray {
            list.map(|s| GString::from(s.key())).collect()
        };
        d.set("loadout", &keys(&mut self.streak_loadout.streaks()));
        d.set("held", &keys(&mut self.streak_held.iter().copied()));
        d.set(
            "selected",
            if self.streak_held.is_empty() {
                -1
            } else {
                self.streak_selected as i32
            },
        );
        d
    }

    /// Calls in the selected held streak (the server decides).
    #[func]
    fn use_streak(&mut self) {
        if let Some(streak) = self.streak_held.get(self.streak_selected).copied() {
            self.send_reliable(&ClientMessage::UseStreak {
                streak,
                targets: Vec::new(),
            });
        }
    }

    /// Calls in the selected held Airstrike on `targets` (points on the
    /// map; their y is ignored, and only the first three count).
    #[func]
    fn use_airstrike(&mut self, targets: PackedVector3Array) {
        if self.streak_held.get(self.streak_selected) == Some(&Streak::Airstrike) {
            let targets = targets
                .as_slice()
                .iter()
                .map(|t| shooter_sim::Vec2::new(t.x, t.z))
                .collect();
            self.send_reliable(&ClientMessage::UseStreak {
                streak: Streak::Airstrike,
                targets,
            });
        }
    }

    /// Selects the next held streak.
    #[func]
    fn cycle_streak(&mut self) {
        if !self.streak_held.is_empty() {
            self.streak_selected = (self.streak_selected + 1) % self.streak_held.len();
            self.signals().streaks_changed().emit("");
        }
    }

    /// Chooses the scorestreaks to bring from the next spawn (keys; unknown
    /// ones and repeats are dropped, and they sort by cost).
    #[func]
    fn set_streaks(&mut self, keys: PackedStringArray) {
        let mut slots = [None; 3];
        for (slot, key) in slots.iter_mut().zip(keys.as_slice()) {
            *slot = Streak::by_key(&key.to_string());
        }
        self.streak_choice = Loadout(slots).sorted();
        self.send_reliable(&ClientMessage::Streaks(self.streak_choice));
    }

    /// Every scorestreak, cheapest first, as dictionaries with `key`,
    /// `name`, `cost`, `summary`, and `level` (the level it unlocks at).
    #[func]
    fn streak_items() -> Array<VarDictionary> {
        let mut list: Vec<Streak> = Streak::ALL.to_vec();
        list.sort_by_key(|s| s.cost());
        list.into_iter()
            .map(|s| {
                let mut d = VarDictionary::new();
                d.set("key", s.key());
                d.set("name", s.label());
                d.set("cost", s.cost());
                d.set("summary", s.summary());
                d.set("level", s.unlock_level());
                d
            })
            .collect()
    }

    /// The UAVs and Counter-UAVs up, as a dictionary: `uav` and
    /// `counter_uav` (seconds our side's has left, 0 for none), and
    /// `enemy_uav` and `enemy_counter_uav` (the enemy's).
    #[func]
    fn sky_info(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        for (key, streak, ours) in [
            ("uav", Streak::Uav, true),
            ("counter_uav", Streak::CounterUav, true),
            ("enemy_uav", Streak::Uav, false),
            ("enemy_counter_uav", Streak::CounterUav, false),
        ] {
            let until = self
                .sky
                .iter()
                .filter(|a| a.streak == streak && self.ours(a.side) == ours)
                .map(|a| a.until)
                .max();
            d.set(key, self.seconds_until(until));
        }
        d
    }

    /// The Care Packages out, as dictionaries with `id`, `owner`, `mine`
    /// (we called it), `position` (where it lands), `heading` (the
    /// aircraft's, radians), `drop_seconds` and `land_seconds` (until the
    /// aircraft lets it go and it lands; negative after), `contents` (the
    /// streak's key) and `contents_name`, `capture_by` (-1 for nobody), and
    /// `capture_progress` (0 to 1).
    #[func]
    fn crates(&self) -> Array<VarDictionary> {
        let since = |tick: Tick| -> f64 {
            let now = self.newest.unwrap_or(0);
            (f64::from(tick) - f64::from(now)) / f64::from(TICK_RATE)
        };
        self.crates
            .iter()
            .map(|c| {
                let mut d = VarDictionary::new();
                d.set("id", i32::from(c.id));
                d.set("owner", i32::from(c.owner));
                d.set("mine", Some(c.owner) == self.player_id);
                d.set("position", to_godot(c.at));
                d.set("heading", c.heading);
                d.set("drop_seconds", since(c.drop_at));
                d.set("land_seconds", since(c.lands_at));
                d.set("contents", c.contents.key());
                d.set("contents_name", c.contents.label());
                let (by, progress) = match c.capture {
                    Some((by, started, done)) if done > started => {
                        let now = self.newest.unwrap_or(started);
                        let p = f64::from(now.saturating_sub(started)) / f64::from(done - started);
                        (i32::from(by), p.clamp(0.0, 1.0))
                    }
                    _ => (-1, 0.0),
                };
                d.set("capture_by", by);
                d.set("capture_progress", progress);
                d
            })
            .collect()
    }

    /// Whether an enemy Counter-UAV jams our minimap.
    #[func]
    fn radar_jammed(&self) -> bool {
        self.radar.jammed
    }

    /// Scavenger packs this player can see and take: none without the
    /// perk; otherwise every enemy's (anyone else's without teams), as
    /// dictionaries with `id` and `position` (the ground under it).
    #[func]
    fn scavenger_packs(&self) -> Array<VarDictionary> {
        let mine = self
            .player
            .as_ref()
            .map_or(Perks::NONE, |p| p.bind().perks());
        if !mine.has(Perk::Scavenger) {
            return Array::new();
        }
        let local = self.local_team();
        self.scavenger_packs
            .iter()
            .filter(|(_, _, by)| {
                let team = self.roster.iter().find(|e| e.id == *by).map(|e| e.team);
                Some(*by) != self.player_id
                    && (self.mode.free_for_all() || team.is_none() || team != local)
            })
            .map(|(id, at, _)| {
                let mut d = VarDictionary::new();
                d.set("id", i64::from(*id));
                d.set("position", to_godot(*at));
                d
            })
            .collect()
    }

    #[func]
    fn charge_info(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let sites: Array<VarDictionary> = self
            .charge_sites
            .iter()
            .map(|site| {
                let mut entry = VarDictionary::new();
                entry.set("name", &GString::from(site.name.as_str()));
                entry.set("center", to_godot(site.center));
                entry.set("size", to_godot(site.half_extents * 2.0));
                entry
            })
            .collect();
        d.set("sites", &sites);
        let (status, carrier, at, site) = match self.charge.map(|c| c.status) {
            None => ("", None, None, -1),
            Some(ChargeStatus::Carried { by }) => {
                ("carried", by, by.and_then(|id| self.carrier_feet(id)), -1)
            }
            Some(ChargeStatus::Dropped { at }) => ("dropped", None, Some(to_godot(at)), -1),
            Some(ChargeStatus::Planted { site, at }) => {
                ("planted", None, Some(to_godot(at)), i32::from(site))
            }
            Some(ChargeStatus::Detonated { site, at }) => {
                ("detonated", None, Some(to_godot(at)), i32::from(site))
            }
            Some(ChargeStatus::Defused { site, at }) => {
                ("defused", None, Some(to_godot(at)), i32::from(site))
            }
        };
        d.set("status", status);
        d.set("carrier", carrier.map_or(-1, i32::from));
        d.set("carrying", carrier.is_some() && carrier == self.player_id);
        d.set("has_position", at.is_some());
        d.set("position", at.unwrap_or(Vector3::ZERO));
        d.set("site", site);
        let using = self.charge.and_then(|c| c.using);
        d.set("user", using.map_or(-1, |u| i32::from(u.by)));
        d.set(
            "use_kind",
            match using.map(|u| u.kind) {
                Some(UseKind::Plant) => "plant",
                Some(UseKind::Defuse) => "defuse",
                None => "",
            },
        );
        let progress = match (using, self.newest) {
            (Some(u), Some(now)) if u.done_at > u.started_at => {
                f64::from(now.saturating_sub(u.started_at)) / f64::from(u.done_at - u.started_at)
            }
            _ => 0.0,
        };
        d.set("use_progress", progress.clamp(0.0, 1.0));
        d.set(
            "spot",
            match self.charge_spot() {
                Some(UseKind::Plant) => "plant",
                Some(UseKind::Defuse) => "defuse",
                None => "",
            },
        );
        d
    }

    /// The weapons a class may use as its primary, as dictionaries with
    /// `key`, `name`, and `attachments` (each a dictionary with `key`, the
    /// variant's weapon key such as "m4+grip", and `name`), in table order.
    #[func]
    fn loadout_weapons() -> Array<VarDictionary> {
        class::class_weapons(Slot::Primary)
            .into_iter()
            .map(|id| id.def())
            .map(|def| {
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

    /// The primary weapon key we spawn with (an attachment variant's key
    /// for a single attachment), or "" for none.
    #[func]
    fn loadout(&self) -> GString {
        let primary = self.current_class().primary.and_then(Gun::weapon);
        primary.map_or_else(GString::new, |id| GString::from(id.def().key.as_str()))
    }

    /// The class we spawn with, as a dictionary (`class_dict`).
    #[func]
    fn loadout_class(&self) -> VarDictionary {
        class_to_dict(&self.current_class())
    }

    /// Asks for a class (a dictionary, see `class_dict`) from our next
    /// spawn on. Returns "" when sent, or why it breaks a rule (and then
    /// nothing is sent; the server would refuse it too).
    #[func]
    fn set_class(&mut self, class: VarDictionary) -> GString {
        let Some(class) = class_from_dict(&class) else {
            return GString::from("Unknown item");
        };
        if let Err(error) = class.validate() {
            return GString::from(error_text(error));
        }
        self.class = Some(class);
        self.send_reliable(&ClientMessage::Class(class));
        GString::new()
    }

    /// What a class (a dictionary) costs, and whether it is allowed:
    /// `cost` (points), `picks` (the budget), and `error` ("" when valid).
    #[func]
    fn check_class(class: VarDictionary) -> VarDictionary {
        let mut d = VarDictionary::new();
        d.set("picks", i64::from(class::PICKS));
        match class_from_dict(&class) {
            Some(class) => {
                d.set("cost", i64::from(class.cost()));
                let error = class.validate().err().map_or("", error_text);
                d.set("error", error);
            }
            None => {
                d.set("cost", 0);
                d.set("error", "Unknown item");
            }
        }
        d
    }

    /// Everything a class can hold, for the create-a-class menu:
    /// `primaries` and `secondaries` (each `key`, `name`, `class` and
    /// `class_order` (its Create-a-Class group), and `attachments`, a list of
    /// `key` and `name`), `tacticals` (`key`, `name`), `perks`
    /// (`key`, `name`, `summary`, and `tier`, 1 to 3, in tier order), and
    /// `wildcards` (`key`, `name`, `summary`).
    #[func]
    fn class_items() -> VarDictionary {
        let weapons = |slot: Slot| -> Array<VarDictionary> {
            class::class_weapons(slot)
                .into_iter()
                .map(|id| {
                    let def = id.def();
                    let mut d = VarDictionary::new();
                    d.set("key", &GString::from(def.key.as_str()));
                    d.set("name", &GString::from(def.name.as_str()));
                    d.set("class", def.class.label());
                    d.set("class_order", def.class.order() as i64);
                    let attachments: Array<VarDictionary> = def
                        .attachments
                        .iter()
                        .map(|a| {
                            let mut entry = VarDictionary::new();
                            entry.set("key", a.key());
                            entry.set("name", a.label());
                            entry.set("optic", a.optic());
                            entry
                        })
                        .collect();
                    d.set("attachments", &attachments);
                    d
                })
                .collect()
        };
        let mut d = VarDictionary::new();
        d.set("primaries", &weapons(Slot::Primary));
        d.set("secondaries", &weapons(Slot::Secondary));
        d.set("tacticals", &Self::tacticals());
        let wildcards: Array<VarDictionary> = Wildcard::ALL
            .iter()
            .map(|w| {
                let mut entry = VarDictionary::new();
                entry.set("key", w.key());
                entry.set("name", w.label());
                entry.set("summary", w.summary());
                entry
            })
            .collect();
        d.set("wildcards", &wildcards);
        let perks: Array<VarDictionary> = shooter_sim::perk::Tier::ALL
            .into_iter()
            .flat_map(shooter_sim::perk::Perk::of_tier)
            .map(|p| {
                let mut entry = VarDictionary::new();
                entry.set("key", p.key());
                entry.set("name", p.label());
                entry.set("summary", p.summary());
                entry.set("tier", i64::from(p.tier().number()));
                entry
            })
            .collect();
        d.set("perks", &perks);
        d
    }

    /// A perk's tier (1 to 3) by key, or 0 for an unknown key.
    #[func]
    fn perk_tier(key: GString) -> i32 {
        shooter_sim::perk::Perk::by_key(&key.to_string())
            .map_or(0, |p| i32::from(p.tier().number()))
    }

    /// The ready-made classes, each a class dictionary with its `name`.
    #[func]
    fn default_classes() -> Array<VarDictionary> {
        DefaultClass::ALL
            .iter()
            .map(|default| {
                let mut d = class_to_dict(&default.class());
                d.set("name", default.name());
                d
            })
            .collect()
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

    /// Sends a chat message to everyone, or with `team` to our team. The
    /// server cleans and rate-limits it, and echoes it back to us.
    #[func]
    fn send_chat(&mut self, text: GString, team: bool) {
        let text = text.to_string();
        if text.trim().is_empty() {
            return;
        }
        self.send_reliable(&ClientMessage::Chat { text, team });
    }

    /// Tells the server we are ready (or not) to start the match.
    #[func]
    fn set_ready(&mut self, ready: bool) {
        self.send_reliable(&ClientMessage::Ready { ready });
    }

    /// Picks the primary weapon (a weapon key, attachment variants
    /// included) for our next spawn, keeping the rest of the class. Returns
    /// false for a weapon that cannot be a class's primary.
    #[func]
    fn set_loadout(&mut self, primary: GString) -> bool {
        let Some(gun) = gun_of(&primary.to_string()) else {
            return false;
        };
        let mut class = self.current_class();
        class.primary = Some(gun);
        if class.validate().is_err() {
            return false;
        }
        self.primary = primary;
        self.class = Some(class);
        self.send_reliable(&ClientMessage::Class(class));
        true
    }

    /// Picks the tactical ("flash" or "smoke") for our next spawn. Returns
    /// false for anything else.
    #[func]
    fn choose_tactical(&mut self, key: GString) -> bool {
        let Some(kind) = Kind::by_key(&key.to_string()).filter(|kind| !kind.lethal()) else {
            return false;
        };
        self.tactical = key;
        let mut class = self.current_class();
        class.tactical = kind;
        self.class = Some(class);
        self.send_reliable(&ClientMessage::Class(class));
        true
    }

    /// The tactical every menu offers: key and name pairs.
    #[func]
    fn tacticals() -> Array<VarDictionary> {
        Kind::TACTICALS
            .iter()
            .map(|kind| {
                let mut d = VarDictionary::new();
                d.set("key", kind.key());
                d.set("name", kind.name());
                d
            })
            .collect()
    }

    /// Which build the server runs (a tag or short commit hash), or "" before
    /// it welcomes us.
    #[func]
    fn server_build(&self) -> GString {
        self.server_build.as_str().into()
    }

    /// Whether the server reports our match XP to our account itself
    /// (so the game must not report it too).
    #[func]
    fn is_ranked(&self) -> bool {
        self.ranked
    }

    /// Why the server turned us away, or "".
    #[func]
    fn denied_reason(&self) -> GString {
        self.denied_reason.as_str().into()
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

    /// Zombies: zombies drawn now (0 outside Zombies).
    #[func]
    fn drawn_zombies(&self) -> i32 {
        self.horde.as_ref().map_or(0, |h| h.drawn() as i32)
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
            Status::Denied => "denied",
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
            d.set("snapshot_gap_ms", ms(clock.gap_ticks()));
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
        let id = self.player_id.unwrap_or(0);
        let message = match ServerMessage::decode_with(bytes, |tick| {
            Some(Baseline {
                players: history.get(tick)?,
                own: history.own(tick, id),
                zombies: history.zombies(tick).map(|z| (tick, z)),
            })
        }) {
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
                server_build,
                ranked,
                ..
            } => {
                if self.status != Status::Connecting {
                    return;
                }
                self.ranked = ranked;
                self.server_build = clean_text(&server_build, MAX_BUILD_BYTES);
                godot_print!(
                    "net client: joined as player {player_id} (server build {}, ours {})",
                    self.server_build,
                    crate::build_info::BUILD
                );
                self.status = Status::Joined;
                self.player_id = Some(player_id);
                self.mode = mode;
                if let Some(clock) = &mut self.clock {
                    clock.set_interval(u32::from(snapshot_interval));
                }
                if mode == Mode::Zombies {
                    let tree = self.base().get_tree();
                    self.horde = HordeView::attach(&tree, snapshot_interval);
                    if self.horde.is_none() {
                        godot_error!(
                            "net client: the server runs Zombies on a map without a director"
                        );
                    }
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
            ServerMessage::Denied { reason } => {
                godot_print!("net client: the server turned us away: {reason}");
                self.denied_reason = clean_text(&reason, protocol::MAX_DENIED_BYTES);
                self.status = Status::Denied;
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
                // The body falls with our shot's push once the snapshot that
                // killed it is drawn: it carries where the shot landed.
                player.bind_mut().on_server_hit(shot, headshot, killed);
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
                // Recorded on our own body for killcams: the flinch, and the
                // fall's push.
                let recorder = &mut self.own_recorder;
                recorder.hits = recorder.hits.wrapping_add(1);
                let travel = self.stats.server_position - to_godot(from);
                recorder.hit_heading = protocol::heading_to_u8(travel.x, travel.z);
                recorder.hit_part = protocol::HitPart::new(
                    Some(shooter_sim::hitbox::CHEST_SHAPE as u8),
                    damage,
                    false,
                )
                .to_u8();
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
                cause,
            } => {
                let name = match cause {
                    KillCause::Knife => GString::from(KNIFE_NAME),
                    KillCause::Equipment(kind) => GString::from(kind.name()),
                    KillCause::Charge => GString::from(CHARGE_NAME),
                    KillCause::Streak(streak) => GString::from(streak.label()),
                    KillCause::Weapon => GString::from(&weapon.def().name),
                };
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
                period,
            } => {
                if phase == Phase::Playing && self.match_phase != Some(Phase::Playing) {
                    self.props.reset();
                }
                if matches!(phase, Phase::Lobby | Phase::Countdown) {
                    // Between matches there is no round, and no killcam.
                    self.set_round(None);
                    self.killcam = None;
                    self.stop_killcam();
                }
                self.match_phase = Some(phase);
                self.match_ends_at = ends_at;
                self.team_scores = scores;
                self.score_limit = score_limit;
                self.match_period = period;
                self.signals().match_changed().emit();
            }
            GameEvent::Scoreboard { rows } => {
                if let (Some(horde), Some(me)) = (&mut self.horde, self.player_id)
                    && let Some(row) = rows.iter().find(|r| r.id == me)
                {
                    horde.set_tallies(row.kills, row.headshots);
                }
                for row in rows {
                    if let Some(entry) = self.roster.iter_mut().find(|e| e.id == row.id) {
                        entry.row = row;
                    }
                }
                self.signals().scoreboard_changed().emit();
            }
            GameEvent::Zombies(event) => {
                if let ZombiesEvent::Hit {
                    shot,
                    headshot,
                    killed,
                    ..
                } = event
                {
                    self.stats.hits_confirmed += 1;
                    if killed {
                        self.stats.kills_confirmed += 1;
                    }
                    if let Some(player) = &mut self.player {
                        player.bind_mut().on_server_hit(shot, headshot, killed);
                    }
                }
                let newest = self.newest;
                let me = self.player_id;
                if let Some(horde) = &mut self.horde {
                    horde.on_event(&event, me, |end| match newest {
                        Some(now) => f64::from(end.saturating_sub(now)) / f64::from(TICK_RATE),
                        None => 0.0,
                    });
                }
                self.on_last_stand(&event);
            }
            GameEvent::Spawned { yaw } => {
                if let Some(player) = &mut self.player {
                    player.bind_mut().set_look(yaw, 0.0);
                }
            }
            GameEvent::Flags { flags } => {
                self.flags = Some(flags);
            }
            GameEvent::FlagPlay { flag, play, player } => {
                let play = match play {
                    FlagPlay::Taken => "taken",
                    FlagPlay::Dropped => "dropped",
                    FlagPlay::Returned => "returned",
                    FlagPlay::Captured => "captured",
                };
                self.signals().flag_played().emit(
                    i32::from(flag),
                    &GString::from(play),
                    player.map_or(-1, i32::from),
                );
            }
            GameEvent::HardpointZones { zones } => {
                self.zones = zones;
                self.signals().hardpoint_changed().emit();
            }
            GameEvent::Hardpoint {
                active,
                holder,
                moves_at,
            } => {
                self.active_zone = active;
                self.holder = holder;
                self.zone_moves_at = moves_at;
                self.signals().hardpoint_changed().emit();
            }
            GameEvent::DominationFlags { flags } => {
                self.dom_points = flags;
                self.signals().domination_changed().emit();
            }
            GameEvent::Domination(view) => {
                self.dom = Some(view);
                self.signals().domination_changed().emit();
            }
            GameEvent::Round {
                number,
                phase,
                ends_at,
                attackers,
                halftime_after,
            } => {
                if phase == RoundPhase::Freeze {
                    // A new round: whatever replay was left is over.
                    self.killcam = None;
                    self.stop_killcam();
                }
                self.set_round(Some(Round {
                    number,
                    phase,
                    ends_at,
                    attackers,
                    halftime_after,
                }));
            }
            GameEvent::PropState { id, stage, impulse } => {
                // The server owns prop damage: clients only show the states
                // it sends, never damage props themselves online.
                self.props.apply_state(&PropSnapshot { id, stage, impulse });
            }
            GameEvent::Chat { from, text, team } => {
                let name = self.name_of(i32::from(from));
                self.signals().chat_received().emit(
                    i32::from(from),
                    &name,
                    &GString::from(text.as_str()),
                    team,
                );
            }
            GameEvent::Blast {
                owner,
                shot,
                kind,
                at,
            } => {
                if let Some(player) = &mut self.player {
                    player.bind_mut().on_server_blast(owner, shot, kind, at);
                }
            }
            GameEvent::Killcam(killcam) => {
                // A killcam that ends a round or the match replaces one
                // still waiting to play after a death.
                let replaces = match &self.killcam {
                    Some((current, _)) => killcam.kind.ending() || !current.kind.ending(),
                    None => true,
                };
                if killcam.kind == killcam::Kind::Respawn
                    && let Some(player) = &mut self.player
                {
                    player
                        .bind_mut()
                        .hold_respawn(killcam::respawn_wait(&killcam));
                }
                let covered = self.tape.covers(&killcam);
                godot_print!(
                    "net client: {:?} killcam of player {} by player {}{}",
                    killcam.kind,
                    killcam.victim,
                    killcam.killer,
                    if covered {
                        ""
                    } else {
                        " (too old for the tape, not played)"
                    }
                );
                if replaces && covered {
                    let now = Time::singleton().get_ticks_usec() as f64 / 1e6;
                    self.killcam = Some((killcam, now));
                }
            }
            GameEvent::ChargeSites { sites } => {
                self.charge_sites = sites;
                self.signals().charge_changed().emit();
            }
            GameEvent::Scavenger(event) => {
                let taken_by = match event {
                    ScavengerEvent::Dropped { id, at, dropped_by } => {
                        self.scavenger_packs.retain(|(p, _, _)| *p != id);
                        self.scavenger_packs.push((id, at, dropped_by));
                        -1
                    }
                    ScavengerEvent::Taken { id, by } => {
                        self.scavenger_packs.retain(|(p, _, _)| *p != id);
                        i32::from(by)
                    }
                    ScavengerEvent::Gone { id } => {
                        self.scavenger_packs.retain(|(p, _, _)| *p != id);
                        -1
                    }
                };
                self.signals().scavenger_changed().emit(taken_by);
            }
            GameEvent::Streak(StreakEvent::State {
                progress,
                held,
                loadout,
            }) => {
                let earned = if held.len() > self.streak_held.len() {
                    held.last().map_or("", |s| s.key())
                } else {
                    ""
                };
                // The newest earned is selected, as in Black Ops 2;
                // otherwise the selection stays on its streak if it can.
                let selected = self.streak_held.get(self.streak_selected).copied();
                self.streak_selected = if !earned.is_empty() {
                    held.len() - 1
                } else {
                    selected
                        .and_then(|s| held.iter().position(|h| *h == s))
                        .unwrap_or(held.len().saturating_sub(1))
                };
                self.streak_progress = progress;
                self.streak_held = held;
                self.streak_loadout = loadout;
                self.signals().streaks_changed().emit(earned);
            }
            GameEvent::Streak(StreakEvent::CrateIncoming {
                id,
                owner,
                at,
                heading,
                drop_at,
                lands_at,
                contents,
            }) => {
                self.crates.retain(|c| c.id != id);
                self.crates.push(CrateView {
                    id,
                    owner,
                    at,
                    heading,
                    drop_at,
                    lands_at,
                    contents,
                    capture: None,
                });
                self.signals().crates_changed().emit(-1, "");
            }
            GameEvent::Streak(StreakEvent::CrateCapture {
                id,
                by,
                started,
                done_at,
            }) => {
                if let Some(c) = self.crates.iter_mut().find(|c| c.id == id) {
                    c.capture = by.map(|by| (by, started, done_at));
                }
                self.signals().crates_changed().emit(-1, "");
            }
            GameEvent::Streak(StreakEvent::CrateTaken { id, by, contents }) => {
                self.crates.retain(|c| c.id != id);
                self.signals()
                    .crates_changed()
                    .emit(i32::from(by), contents.key());
            }
            GameEvent::Streak(StreakEvent::CrateGone { id }) => {
                self.crates.retain(|c| c.id != id);
                self.signals().crates_changed().emit(-1, "");
            }
            GameEvent::Streak(StreakEvent::Airstrike { owner, strikes }) => {
                let points: PackedVector3Array =
                    strikes.iter().map(|(at, _)| to_godot(*at)).collect();
                let seconds: PackedFloat32Array = strikes
                    .iter()
                    .map(|(_, tick)| self.seconds_until(Some(*tick)) as f32)
                    .collect();
                self.signals()
                    .airstrike_called()
                    .emit(i32::from(owner), &points, &seconds);
            }
            GameEvent::Streak(StreakEvent::Strike { owner, at }) => {
                self.signals()
                    .strike_landed()
                    .emit(i32::from(owner), to_godot(at));
            }
            GameEvent::Streak(StreakEvent::Sky { up }) => {
                let new = up.iter().find(|a| !self.sky.contains(a)).copied();
                self.sky = up;
                let (called, ours) =
                    new.map_or(("", false), |a| (a.streak.key(), self.ours(a.side)));
                self.signals().sky_changed().emit(called, ours);
            }
            GameEvent::Charge(view) => {
                let was_planted = self
                    .charge
                    .is_some_and(|c| matches!(c.status, ChargeStatus::Planted { .. }));
                if let ChargeStatus::Detonated { at, .. } = view.status
                    && was_planted
                {
                    self.charge_blast(to_godot(at));
                }
                self.charge = Some(view);
                self.signals().charge_changed().emit();
            }
        }
    }

    /// What holding interact would do where the local player stands: plant
    /// (carrying the charge, inside a site), defuse (a defender at the
    /// planted charge), or nothing. The same rule the server holds players
    /// still by (`Charge::spot`), from what it last told us.
    /// Zombies: going down, getting up, and revives, for the HUD and the
    /// markers over downed teammates.
    fn on_last_stand(&mut self, event: &ZombiesEvent) {
        let me = self.player_id;
        match *event {
            ZombiesEvent::Downed {
                player,
                bleeds_out_at,
            } => {
                let seconds = bleeds_out_at.map(|t| self.seconds_until(Some(t)));
                if Some(player) == me {
                    if let Some(horde) = &mut self.horde {
                        horde.director().bind_mut().net_downed(seconds);
                    }
                } else if let Some(mut remote) = self.remote_player(i32::from(player)) {
                    remote.bind_mut().set_downed_marker(seconds.is_some());
                }
                // A revive we were part of ends with them up (or gone).
                if bleeds_out_at.is_none()
                    && let Some(horde) = &mut self.horde
                    && Some(player) != me
                {
                    horde.director().bind_mut().net_revive(None);
                }
            }
            ZombiesEvent::Revive {
                player,
                by,
                done_at,
            } => {
                let involved = Some(player) == me || (by.is_some() && by == me);
                if !involved {
                    return;
                }
                let revive = by.map(|by| {
                    let reviving = Some(by) == me;
                    let other = if reviving { player } else { by };
                    // Second Wind alone: getting back up by ourselves.
                    let name = if by == player {
                        String::new()
                    } else {
                        self.name_of(i32::from(other)).to_string()
                    };
                    (name, reviving, self.seconds_until(Some(done_at)))
                });
                if let Some(horde) = &mut self.horde {
                    horde.director().bind_mut().net_revive(revive);
                }
            }
            _ => {}
        }
    }

    /// Zombies: the teammate down in last stand close enough for us to
    /// revive, by the server's rule (`laststand::REVIVE_RADIUS` between
    /// feet), so holding use there is predicted as the server runs it.
    fn revive_spot(&self) -> Option<u8> {
        let horde = self.horde.as_ref()?;
        let player = self.player.as_ref()?;
        let player = player.bind();
        if !player.is_alive() || player.is_downed() {
            return None;
        }
        let here = player.feet_position();
        horde
            .downed()
            .iter()
            .filter(|id| Some(**id) != self.player_id)
            .filter_map(|id| {
                let remote = self.remotes.iter().find(|r| r.id == *id)?;
                let feet = remote.node.get_global_position();
                Some((*id, feet.distance_to(here)))
            })
            .filter(|(_, d)| *d <= shooter_sim::zombies::laststand::REVIVE_RADIUS)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id)
    }

    /// The charge went off at `at`: its own big blast, a rattle by distance,
    /// and, close by, the controller's rumble and ringing ears.
    fn charge_blast(&mut self, at: Vector3) {
        let tree = self.base().get_tree();
        if let Some(mut effects) = crate::effects::CombatEffects::find(&tree) {
            effects.bind_mut().charge_explosion(at);
        }
        if let Some(player) = &mut self.player {
            player.bind_mut().feel_charge_blast(at);
        }
    }

    fn charge_spot(&self) -> Option<UseKind> {
        let view = self.charge?;
        let round = self.round?;
        let me = self.player_id?;
        let team = self.local_team()?;
        let player = self.player.as_ref()?;
        let state = match view.status {
            // Defenders are not told who carries it; they cannot plant anyway.
            ChargeStatus::Carried { by } => ChargeState::Carried {
                by: by.unwrap_or(u8::MAX),
            },
            ChargeStatus::Dropped { at } => ChargeState::Dropped { at },
            ChargeStatus::Planted { site, at } => ChargeState::Planted { site, at },
            ChargeStatus::Detonated { site, at } => ChargeState::Detonated { site, at },
            ChargeStatus::Defused { site, at } => ChargeState::Defused { site, at },
        };
        let charge = Charge {
            state,
            using: view.using.map(|u| ChargeUse {
                by: u.by,
                kind: u.kind,
                held: 0.0,
            }),
            position: shooter_sim::Vec3::ZERO,
        };
        let sites: Vec<Site> = self
            .charge_sites
            .iter()
            .map(|z| Site {
                center: z.center,
                half_extents: z.half_extents,
            })
            .collect();
        let rules = SndRules::default();
        let cx = ChargeContext {
            rules: &rules,
            sites: &sites,
            attackers: round.attackers,
            live: round.phase == RoundPhase::Live,
        };
        let player = player.bind();
        charge.spot(
            &cx,
            &ChargePlayer {
                id: me,
                team,
                alive: player.is_alive(),
                position: to_sim(player.base().get_global_position()),
                interact: true,
            },
        )
    }

    /// Finds the props manager if the map has one and it is not loaded yet,
    /// and keeps its population equal to the roster (bots included).
    fn sync_props(&mut self) {
        if !self.props.is_active() {
            if self.props_retry == 0 {
                self.props = PropsBridge::discover_in_tree(&self.base().get_tree());
                // A replacement manager needs the population again.
                self.props_population = None;
                // Maps without props need no quicker retry than this.
                self.props_retry = 2 * TICK_RATE;
            } else {
                self.props_retry -= 1;
            }
            if !self.props.is_active() {
                return;
            }
        }
        let population = self.roster.len().min(i32::MAX as usize) as i32;
        if self.props_population != Some(population) {
            self.props.set_population(population);
            self.props_population = Some(population);
        }
    }

    /// Keeps the round, and holds the local player still during its freeze.
    /// The server lets go when the freeze ends, before this event arrives,
    /// so the first inputs after it are never held back there.
    fn set_round(&mut self, round: Option<Round>) {
        let changed = self.round.map(|r| (r.number, r.phase)) != round.map(|r| (r.number, r.phase));
        self.round = round;
        let frozen = round.is_some_and(|r| r.phase == RoundPhase::Freeze);
        if let Some(player) = &mut self.player {
            player.bind_mut().set_movement_locked(frozen);
        }
        if changed {
            self.signals().round_changed().emit();
        }
    }

    /// Seconds from the newest snapshot to `tick`, or 0.
    fn seconds_until(&self, tick: Option<Tick>) -> f64 {
        match (tick, self.newest) {
            (Some(end), Some(now)) => f64::from(end.saturating_sub(now)) / f64::from(TICK_RATE),
            _ => 0.0,
        }
    }

    /// Where a flag carrier's feet are drawn: our own player, or a remote
    /// player as interpolated.
    fn carrier_feet(&self, id: u8) -> Option<Vector3> {
        if Some(id) == self.player_id {
            return self.player.as_ref().map(|p| p.get_global_position());
        }
        self.remotes
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.node.bind().feet())
    }

    fn entry(&self, id: i32) -> Option<&RosterEntry> {
        self.roster.iter().find(|e| i32::from(e.id) == id)
    }

    /// Whether streak side `side` is ours: our team, or ourselves without
    /// teams.
    fn ours(&self, side: u8) -> bool {
        if self.mode.free_for_all() {
            Some(side) == self.player_id
        } else {
            self.local_team() == Some(side)
        }
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
        let owns = snapshot
            .own
            .as_ref()
            .map(|own| (self.player_id.unwrap_or(0), own.predicted));
        self.history
            .insert_with_own(tick, snapshot.players.clone(), owns.into_iter().collect());
        if let Some(zombies) = &snapshot.zombies {
            self.history.insert_zombies(tick, zombies.clone());
        }
        let newest = self.newest.is_none_or(|n| tick > n);
        if newest {
            self.record_tape(&snapshot);
        }
        if newest {
            self.newest = Some(tick);
            self.radar = snapshot.radar.clone();
            self.radar_at = Time::singleton().get_ticks_usec() as f64 / 1e6;
            if let Some(clock) = &mut self.clock {
                clock.on_snapshot(tick);
            }
            if let (Some(horde), Some(zombies)) = (&mut self.horde, &snapshot.zombies) {
                horde.on_snapshot(tick, zombies.clone());
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
                .push(tick, RemoteSample::at(tick, view));
            if newest {
                let listener = self
                    .player
                    .as_ref()
                    .map_or(Perks::NONE, |p| p.bind().perks());
                let mut node = self.remotes[index].node.clone();
                node.bind_mut().set_hearing(view.quiet, listener);
            }
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

        if newest && let (Some(id), Some(player)) = (self.player_id, &mut self.player) {
            let mut player = player.bind_mut();
            player.on_server_grenades(id, &snapshot.grenades);
            player.on_server_smokes(&snapshot.smokes);
        }
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

    /// Records the snapshot on the killcam tape, with ourselves in it as
    /// everyone else sees us. Called before the snapshot reconciles our
    /// prediction, while the input it acknowledges is still remembered.
    fn record_tape(&mut self, snapshot: &Snapshot) {
        let mut views = snapshot.players.clone();
        if let (Some(id), Some(own)) = (self.player_id, &snapshot.own) {
            let angles = self
                .player
                .as_ref()
                .and_then(|p| p.bind().prediction().and_then(|p| p.input(own.last_input)))
                .map_or(self.own_angles, |input| (input.yaw, input.pitch));
            self.own_angles = angles;
            let view =
                self.own_recorder
                    .view(id, snapshot.tick, &own.predicted, angles.0, angles.1);
            views.push(view);
        }
        self.tape.record(snapshot.tick, views);
        self.tape
            .record_grenades(snapshot.tick, &snapshot.grenades, &snapshot.smokes);
    }

    /// Starts, plays, and ends killcams. A respawn killcam starts after the
    /// death delay and ends at its last frame, on a skip (the use button:
    /// interact, or the controller's reload), or once we are alive again;
    /// one that ends a round or match starts after a short lead, cannot be
    /// skipped, and holds its last frame (Call of Duty's `_killcam.gsc`).
    fn update_killcam(&mut self, dt: f32) {
        let now = Time::singleton().get_ticks_usec() as f64 / 1e6;
        let alive = self
            .player
            .as_ref()
            .is_some_and(|player| player.bind().is_alive());
        if self.playing.is_none()
            && let Some((pending, arrived)) = self.killcam
        {
            let ending = pending.kind.ending();
            let lead = if ending {
                killcam::FINAL_LEAD
            } else {
                killcam::DEATH_DELAY
            };
            if !ending && alive && now - arrived > 0.5 {
                // Already back in the game.
                self.killcam = None;
            } else if now >= arrived + f64::from(lead) {
                self.killcam = None;
                if !ending && self.skip_killcams {
                    // Skipped by the setting: back in as soon as the respawn
                    // delay allows, after the death camera as usual.
                    self.send_reliable(&ClientMessage::SkipKillcam);
                    if let Some(player) = &mut self.player {
                        player.bind_mut().killcam_skipped();
                    }
                } else {
                    self.start_killcam(&pending);
                }
            }
        }
        let Some(playing) = &mut self.playing else {
            return;
        };
        let kind = playing.playback.killcam().kind;
        let mut cues = Vec::new();
        let view = playing.update(&self.tape, dt, &mut cues);
        if let Some(player) = &mut self.player {
            let mut player = player.bind_mut();
            player.set_killcam(view);
            for cue in cues {
                match cue {
                    WeaponCue::Fired(weapon) => player.killcam_fired(weapon),
                    WeaponCue::Melee => player.killcam_melee(),
                }
            }
        }
        // Players who joined while it plays stay hidden under it.
        for remote in &mut self.remotes {
            if remote.node.is_visible() {
                remote.node.set_visible(false);
                remote.node.bind_mut().set_muted(true);
            }
        }
        let input = Input::singleton();
        let skipped = !kind.ending()
            && (input.is_action_just_pressed("interact") || input.is_action_just_pressed("reload"));
        let done = match kind {
            killcam::Kind::Respawn => playing.playback.finished() || alive,
            _ => playing.held() >= killcam::FINAL_HOLD,
        };
        if view.is_none() || skipped || done {
            if kind == killcam::Kind::Respawn && !alive {
                // Over, skipped or finished: the server respawns us as soon
                // as the respawn delay allows, as Call of Duty spawns the
                // player when the killcam ends.
                self.send_reliable(&ClientMessage::SkipKillcam);
                if let Some(player) = &mut self.player {
                    player.bind_mut().killcam_skipped();
                }
            }
            self.stop_killcam();
        }
    }

    fn start_killcam(&mut self, killcam: &Killcam) {
        let Some(mut parent) = self.base().get_parent() else {
            return;
        };
        let playing = Playing::start(killcam, &self.tape, &mut parent, |id| self.killcam_copy(id));
        for remote in &mut self.remotes {
            remote.node.set_visible(false);
            remote.node.bind_mut().set_muted(true);
        }
        self.playing = Some(playing);
        self.playing_kind = Some(killcam.kind);
        let info = self.killcam_info();
        self.signals().killcam_started().emit(&info);
    }

    fn stop_killcam(&mut self) {
        let Some(playing) = self.playing.take() else {
            return;
        };
        playing.stop();
        self.playing_kind = None;
        if let Some(player) = &mut self.player {
            player.bind_mut().set_killcam(None);
        }
        for remote in &mut self.remotes {
            remote.node.set_visible(true);
            remote.node.bind_mut().set_muted(false);
        }
        self.signals().killcam_ended().emit();
    }

    /// A killcam's copy of player `id` (ourselves included), named and in
    /// the colours we see them in.
    fn killcam_copy(&self, id: u8) -> Option<Gd<RemotePlayer>> {
        let scene = self.remote_player_scene.as_ref()?;
        let mut node = scene.instantiate()?.try_cast::<RemotePlayer>().ok()?;
        node.set_name(&format!("KillcamPlayer{id}"));
        let mut parent = self.base().get_parent()?;
        parent.add_child(&node);
        let entry = self.roster.iter().find(|e| e.id == id);
        let friendly = Some(id) == self.player_id
            || (!self.mode.free_for_all()
                && entry.is_some_and(|e| Some(e.team) == self.local_team()));
        {
            let mut copy = node.bind_mut();
            if let Some(entry) = entry {
                copy.set_display_name(&entry.name);
            }
            copy.set_friendly(friendly);
        }
        Some(node)
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

    /// The tactical asked for.
    fn tactical_kind(&self) -> Kind {
        Kind::by_key(&self.tactical.to_string())
            .filter(|kind| !kind.lethal())
            .unwrap_or(Kind::Flash)
    }

    /// The class to spawn with: the one set, or one built from `primary`
    /// and `tactical` (the M1911, a frag, and one tactical).
    fn current_class(&self) -> Class {
        if let Some(class) = self.class {
            return class;
        }
        let mut class = Class::default();
        if let Some(gun) = gun_of(&self.primary.to_string()) {
            class.primary = Some(gun);
            class.secondary = Some(Gun::plain(WeaponId::expect("m1911")));
            class.lethals = 1;
            class.tacticals = 1;
        }
        class.tactical = self.tactical_kind();
        if class.validate().is_ok() {
            class
        } else {
            Class::default()
        }
    }

    fn on_disconnected(&mut self) {
        if !matches!(self.status, Status::Rejected | Status::Denied) {
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
        self.stop_killcam();
        self.roster.clear();
        self.tape.clear();
        self.killcam = None;
        self.match_phase = None;
        self.match_period = Period::Regulation;
        self.flags = None;
        self.zones.clear();
        self.holder = Holder::Neutral;
        self.zone_moves_at = None;
        self.dom_points.clear();
        self.dom = None;
        if self.props.is_active() {
            self.props.set_population(0);
            self.props_population = Some(0);
        }
        self.set_round(None);
        self.charge_sites.clear();
        self.charge = None;
        if let Some(player) = &mut self.player {
            player.bind_mut().set_charge_spot(false);
        }
        self.signals().roster_changed().emit();
        self.signals().match_changed().emit();
        self.signals().hardpoint_changed().emit();
        self.signals().domination_changed().emit();
        self.signals().charge_changed().emit();
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

/// A weapon key as a class's primary: the plain weapon, or an attachment
/// variant split into its weapon and attachment. `None` for a weapon a class
/// cannot carry as its primary.
fn gun_of(key: &str) -> Option<Gun> {
    let id = WeaponId::by_key(key)?;
    let def = id.def();
    if def.upgraded {
        return None;
    }
    let base = WeaponId::by_key(if def.fitted.is_empty() {
        &def.key
    } else {
        &def.base_key
    })?;
    class::is_class_weapon(base, Slot::Primary).then_some(Gun {
        base,
        attachments: def.fitted,
    })
}
