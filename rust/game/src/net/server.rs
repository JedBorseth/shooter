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
//! In Hardpoint (`shooter_sim::hardpoint`) kills do not score; instead it
//! rotates the live zone through the map's `Hardpoints`, scores the team that
//! holds it, pays objective awards, and tells everyone which zone is live and
//! who has it.
//! In Domination (`shooter_sim::domination`) kills do not score either: it
//! runs the captures on the map's `DominationFlags` (A, B, C), scores each
//! team a point per owned flag every few seconds, pays objective awards,
//! spawns players near their team's flags, and swaps sides at halftime.
//!
//! Or it runs Gun Game (`shooter_sim::gun_game`, `mode = "gungame"`): every
//! player for themselves on the same match clock, everyone carrying only
//! their weapon on the ladder. Kills swap a player's weapon for the next one
//! on the spot, a fall takes one back, and the first player through the last
//! weapon wins.
//! Or it runs Search and Destroy (`shooter_sim::snd`, `--mode snd`): the
//! match is played in rounds with one life each. Every round starts with
//! everyone back at their side of the map (attackers on side 0, defenders on
//! side 1) and a freeze, when inputs are held still; the dead stay dead until
//! the next round, and joiners wait for it too. The attackers carry the
//! charge (`shooter_sim::snd::Charge`) to one of the map's `ChargeSites`: the
//! server hands it to a random attacker each round, drops it where its
//! carrier dies, holds still whoever is planting or defusing, turns the round
//! clock into the fuse once it is planted, and ends the round when it goes
//! off or is defused.
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
use shooter_sim::chat::{self, ChatLimiter};
use shooter_sim::class::{Class, DefaultClass};
use shooter_sim::ctf::{FlagEvent, FlagPlayer, FlagRules, FlagState, Flags};
use shooter_sim::domination::{self, BotGoal, DominationRules, DominationState, FlagPoint};
use shooter_sim::equipment;
use shooter_sim::gun_game::{self, Progress, Step};
use shooter_sim::hardpoint::{self, HardpointRules, HardpointState, HillTime, Holder, Zone};
use shooter_sim::hitbox::clock::{BodyClock, BodyState};
use shooter_sim::killcam::{self, KillRecord};
use shooter_sim::melee;
use shooter_sim::net::discovery::{ServerInfo, query_port_for};
use shooter_sim::net::input_queue::InputQueue;
use shooter_sim::net::lag::{self, LagHistory, Pellet, Target, TargetHit};
use shooter_sim::net::protocol::{
    self, ChargeStatus, ChargeView, ClientMessage, DominationView, FlagPlay, FlagPointStatus,
    FlagPointView, FlagStatus, FlagView, GameEvent, MAX_CHARGE_SITES, MAX_DOMINATION_FLAGS,
    MAX_EVENTS_PER_MESSAGE, MAX_HARDPOINT_ZONES, OwnState, PROTOCOL_VERSION, PlayerView, Predicted,
    ScoreRow, ServerMessage, Snapshot, UseView, ZoneView,
};
use shooter_sim::net::snapshot_history::SnapshotHistory;
use shooter_sim::net::subtick;
use shooter_sim::net::{CHANNEL_COUNT, DEFAULT_PORT, MAX_PLAYERS, TICK_DT, TICK_RATE, Tick};
use shooter_sim::player::{self, PlayerEvent, PlayerInput, PlayerParams, PlayerState};
use shooter_sim::score::{Award, Kill, ScoreRules, ScoreState, ScoreTotals, Scored};
use shooter_sim::snd::{
    Charge, ChargeContext, ChargeEvent, ChargePlayer, ChargeState, Headcount, RoundEnd, RoundPhase,
    RoundRecord, RoundState, RoundTransition, Site, SndRules, UseKind,
};
use shooter_sim::tdm::{
    self, MatchRules, MatchState, Mode, Period, Phase, SpawnPoint, SpawnRules, Team, Threat,
    Transition,
};
use shooter_sim::weapon::{self, CombatEvent, CombatState, WeaponId};
use shooter_sim::zombies::{self, laststand};
use shooter_sim::{Vec2, Vec3, rng};

use super::bots::{
    BotBrain, BotGrenade, BotNavigation, BotObjective, BotPlayer, BotSmoke, BotWorld,
    THROW_CHECKS_PER_TICK,
};
use super::discovery::QueryResponder;
use super::horde::{Horde, HordeEvent, HordePlayer};
use super::props::{PropExplosion, PropSnapshot, PropsBridge};
use super::transport::{self, NetEvent};
use crate::body::{self, BodyParts};
use crate::convert::{to_godot, to_sim};
use crate::grenades::{self, GrenadeSim, LiveGrenade};
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
/// In Hardpoint, the live zone counts as this many enemies standing at its
/// centre when choosing a spawn, so nobody spawns on the hill.
const ZONE_SPAWN_THREATS: usize = 2;
/// Metres outside a Domination flag's circle from which a bot stops
/// hunting toward it and holds a spot inside.
const DOM_HOLD_REACH: f32 = 8.0;
/// Seconds a Hardpoint bot holds one spot in the zone.
const ZONE_SPOT_SECONDS: u32 = 4;
/// Height above a player's feet that an explosion is checked against.
const EXPLOSION_TARGET_HEIGHT: f32 = 1.0;
/// How far short of a player a wall or prop must be to absorb an explosion.
/// Keeps the exploding prop's own surface from blocking the blast.
const BLAST_CLEARANCE: f32 = 0.15;
/// Search and Destroy's charge going off: Call of Duty 4's `radiusDamage`
/// of 512 units (about 13 m), 200 damage at its heart falling to 20 at the
/// edge, credited to the planter.
const CHARGE_BLAST_RADIUS: f32 = 13.0;
const CHARGE_BLAST_DAMAGE: f32 = 200.0;
const CHARGE_BLAST_EDGE_DAMAGE: f32 = 20.0;

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
    /// Capture the Flag, points in Hardpoint and Domination. 0 uses the
    /// mode's usual limit.
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
    /// The game: "tdm" (Team Deathmatch), "ctf" (Capture the Flag),
    /// "hardpoint", "dom" (Domination), "gungame", or "snd" (Search and
    /// Destroy).
    #[export]
    #[init(val = GString::from("tdm"))]
    mode: GString,
    /// Gun Game: kills a player needs with each weapon to move up.
    #[export]
    #[init(val = 2)]
    kills_per_weapon: i32,
    /// Search and Destroy: rounds a team needs to win the match.
    #[export]
    #[init(val = 6)]
    rounds_to_win: i32,
    /// Search and Destroy: seconds of play in a round (tests shorten it).
    #[export]
    #[init(val = 105.0)]
    round_seconds: f32,
    /// Search and Destroy: seconds from the plant until the charge goes off
    /// (tests shorten it).
    #[export]
    #[init(val = 45.0)]
    fuse_seconds: f32,

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
    snd_rules: SndRules,
    /// The current round, while a Search and Destroy match is on.
    round: Option<RoundState>,
    /// Search and Destroy: the map's charge sites and their names (A, B).
    charge_sites: Vec<Site>,
    charge_site_names: Vec<String>,
    /// The round's charge, while a round is on.
    charge: Option<Charge>,
    /// The server tick the plant or defuse under way started.
    charge_use_started: Tick,
    /// The site the attacking bots go for this round (an index into
    /// `charge_sites`).
    charge_target: usize,
    /// Who planted the charge this round (the explosion's kills are theirs).
    charge_planter: Option<u8>,
    /// The round's kills and last ones standing, for its medals.
    round_record: RoundRecord,
    /// Players whose inputs hold interact whatever they send (tests).
    forced_interact: Vec<u8>,
    spawn_points: Vec<SpawnPoint>,
    spawn_rules: SpawnRules,
    /// Where players died recently, and when.
    recent_deaths: VecDeque<(Tick, Vec3)>,
    /// The map's lanes for bots to follow (`BotLanes`), from one side to the other.
    bot_lanes: Vec<Vec<Vector3>>,
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
    /// Hardpoint: the map's zones in rotation order, the live one, and who
    /// holds it.
    zones: Vec<Zone>,
    hardpoint_rules: HardpointRules,
    hardpoint: HardpointState,
    /// Domination: the map's flags (A, B, C), and who owns each.
    dom_points: Vec<FlagPoint>,
    dom_rules: DominationRules,
    dom: DominationState,
    /// Hardpoint: living players of each team in the live zone, as of the
    /// last tick (for overtime).
    hill_presence: [u32; tdm::TEAM_COUNT],
    /// The map's interactive props manager, when the map has one.
    props: PropsBridge,
    /// Population last sent to the props manager, to avoid redundant calls.
    props_population: Option<i32>,
    /// Ticks until looking for the props manager again (maps without props
    /// never pay more than a group lookup every couple of seconds).
    props_retry: u32,
    /// Every live grenade.
    grenades: GrenadeSim,
    /// Zombies: the horde, the windows, and the rounds.
    horde: Option<Horde>,
    /// The round event last sent to everyone, to send it again only when
    /// it changes.
    zombies_round_sent: Option<protocol::ZombiesEvent>,
    /// Smoke clouds still hanging, oldest first: bots see through them
    /// only as far as they allow, and snapshots carry them so players who
    /// join after one popped still see it.
    smokes: Vec<HangingSmoke>,
    /// The latest kill of the match (or the Search and Destroy round), for
    /// the killcam that ends it.
    last_kill: Option<KillRecord>,
}

/// A smoke cloud still hanging.
struct HangingSmoke {
    /// The thrower and their shot number for the throw (the `Blast`'s key).
    owner: Option<u8>,
    shot: u32,
    /// Where the grenade popped.
    at: Vec3,
    popped: Tick,
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
    /// The class for the next spawn, checked when it arrived.
    class: Class,
    /// Chat rate limit.
    chat: ChatLimiter,
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
    /// The class this player spawns with.
    class: Class,
    /// A bot that plays the match decides its inputs here.
    brain: Option<BotBrain>,
    respawn_in: f32,
    /// The tick this player died on, while a killcam holds their respawn.
    killcam_since: Option<Tick>,
    /// Tick of the latest spawn: shots rewound to before it cannot hit this life.
    spawned_at: Tick,
    score: ScoreState,
    /// Hits taken (wrapping) and the latest one's heading, for everyone's
    /// flinch (see `PlayerView::hits`).
    hits: u8,
    hit_heading: u8,
    /// Where the latest hit landed and how hard (`protocol::HitPart`).
    hit_part: u8,
    /// Stride phase and reload and mantle progress, sent in snapshots.
    clock: BodyClock,
    /// Gun Game: this player's place on the weapon ladder.
    progress: Progress,
    /// Hardpoint: time in the live zone this match.
    hill: HillTime,
    /// Holding interact, as of the last input applied.
    interact: bool,
    /// Zombies: points, kills, and the rest of the co-op scoreboard.
    zombies: ZombiesStats,
    /// Zombies: down in last stand, waiting for a revive.
    last_stand: Option<LastStand>,
}

/// A player down in last stand.
#[derive(Clone, Copy, Debug)]
struct LastStand {
    /// Seconds until they bleed out (the clock stops during a revive).
    bleed_left: f32,
    /// What they carried when they went down, given back on a revive.
    stored: CombatState,
    /// Points they lost going down, which their reviver earns.
    lost: u32,
    /// Who is reviving them, and for how long so far.
    reviver: Option<u8>,
    revived_for: f32,
}

/// One player's Zombies tallies.
#[derive(Clone, Copy, Debug, Default)]
struct ZombiesStats {
    points: u32,
    kills: u32,
    headshots: u32,
    downs: u32,
    revives: u32,
    /// Seconds into the board being put back on a window, and points paid
    /// for rebuilding this round.
    rebuild_for: f32,
    rebuilt: u32,
    /// The last tick they held use at a window.
    rebuild_seen: Option<Tick>,
    /// Zombies ignore them until this tick (just revived).
    ignored_until: Tick,
}

/// A shot fired this tick, waiting to be checked.
struct Shot {
    shooter: u8,
    /// Eye position after the input that fired, as the client computes it.
    eye: Vector3,
    /// View angles the shot leaves from (`CombatEvent::Fired`): the click's
    /// for a press between ticks.
    yaw: f32,
    pitch: f32,
    weapon: WeaponId,
    aim_offset: Vec2,
    shot: u32,
    view_tick: Option<f64>,
    /// The shooter's round trip in seconds (players only), which bounds how
    /// far back their view tick may be.
    rtt: Option<f32>,
}

/// A knife strike that landed this tick, waiting to be checked.
struct Melee {
    attacker: u8,
    /// Feet and eye after the input that struck.
    feet: Vec3,
    eye: Vector3,
    yaw: f32,
    weapon: WeaponId,
    shot: u32,
    view_tick: Option<f64>,
    rtt: Option<f32>,
}

/// Who attacked with what, for applying the hits a shot or strike made.
struct Attack {
    attacker: u8,
    eye: Vector3,
    /// The weapon in the attacker's hands.
    weapon: WeaponId,
    /// The attacker's shot number, which the hit events echo.
    shot: u32,
    melee: bool,
    /// Ticks the targets were rewound for it (lag compensation), which the
    /// killcam replays them behind the attacker by.
    rewind: u8,
}

/// A grenade that left a hand (or fell from a dead one) this tick.
struct Throw {
    owner: u8,
    shot: u32,
    kind: equipment::Kind,
    eye: Vector3,
    yaw: f32,
    pitch: f32,
    /// The thrower's velocity, which the grenade carries.
    carrier: Vec3,
    fuse: f32,
    /// Fell from a dead player's hand rather than thrown.
    dropped: bool,
}

/// What lunge checks need to know about each player this tick.
#[derive(Clone, Copy)]
struct Fighter {
    id: u8,
    side: u8,
    alive: bool,
    spawned_at: Tick,
}

/// One pellet's hit on a damageable prop, before player hits are known.
struct PropHit {
    id: u16,
    /// Distance from the eye, to compare with player hits.
    distance: f32,
    direction: Vec3,
    damage: f32,
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
            time_limit: Some(self.time_limit_minutes.max(0.1) * 60.0),
            ..MatchRules::default()
        };
        if self.game == Mode::Zombies {
            // The horde ends the game, not a clock or a score.
            self.rules.score_limit = 0;
            self.rules.time_limit = None;
        }
        if self.game == Mode::SearchAndDestroy {
            // Rounds won are the score, and there is no match clock.
            self.snd_rules = SndRules {
                rounds_to_win: self.rounds_to_win.max(1) as u32,
                round_time: self.round_seconds.max(5.0),
                fuse: self.fuse_seconds.max(1.0),
                ..SndRules::default()
            };
            self.rules.score_limit = self.snd_rules.rounds_to_win;
            self.rules.time_limit = None;
        }
        godot_print!(
            "server: build {}, protocol {PROTOCOL_VERSION}, running {}",
            crate::build_info::label(),
            self.game.label()
        );
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
        self.hardpoint_rules = HardpointRules::default();
        self.hardpoint = HardpointState::new(&self.hardpoint_rules);
        if self.game == Mode::Hardpoint && self.zones.is_empty() {
            self.zones = fallback_zones(&self.spawn_points);
        }
        if self.game != Mode::Hardpoint {
            self.zones.clear();
        }
        if self.game == Mode::Domination && self.dom_points.is_empty() {
            self.dom_points = fallback_dom_points(&self.spawn_points);
        }
        if self.game != Mode::Domination {
            self.dom_points.clear();
        }
        self.dom_rules = DominationRules::default();
        self.dom = DominationState::new(self.dom_points.len());
        if self.game == Mode::SearchAndDestroy && self.charge_sites.is_empty() {
            (self.charge_site_names, self.charge_sites) = fallback_charge_sites(&self.spawn_points)
                .into_iter()
                .unzip();
        }
        if self.game != Mode::SearchAndDestroy {
            self.charge_sites.clear();
            self.charge_site_names.clear();
        }
        godot_print!(
            "server: {} to {}{}",
            self.game.label(),
            self.rules.score_limit,
            if !self.zones.is_empty() {
                format!(", {} zones", self.zones.len())
            } else if !self.dom_points.is_empty() {
                format!(", {} flags", self.dom_points.len())
            } else {
                String::new()
            }
        );

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
            self.spawn_avatar(id, true, &format!("Bot {}", i + 1), Class::default(), brain);
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
        for client in &mut self.clients {
            client.chat.tick(TICK_DT);
        }

        if self.navigation_pending && self.tick >= 2 {
            self.navigation_pending = false;
            let mut parent = self.base().clone().upcast::<Node>();
            let map = self.base().get_child(0);
            self.navigation = map.and_then(|map| BotNavigation::bake(&mut parent, &map));
        }
        self.simulate();
        self.update_charge();
        self.update_flags();
        self.update_hardpoint();
        self.update_domination();
        self.update_match();
        self.update_props();
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

    /// "tdm", "ctf", "hardpoint", "gungame", or "snd".
    #[func]
    fn game_mode(&self) -> GString {
        GString::from(self.game.key())
    }

    /// Hardpoint: how many zones the map has (0 in other modes).
    #[func]
    fn zone_count(&self) -> i32 {
        self.zones.len() as i32
    }

    /// Hardpoint: the index of the live zone.
    #[func]
    fn active_zone(&self) -> i32 {
        self.hardpoint.active as i32
    }

    /// Hardpoint (tests): the floor centre of each zone, in rotation order.
    #[func]
    fn zone_floors(&self) -> Array<Vector3> {
        self.zones
            .iter()
            .map(|zone| to_godot(Vec3::new(zone.center.x, zone.floor(), zone.center.z)))
            .collect()
    }

    /// "regulation", "second_half", or "overtime" (tests and debugging).
    #[func]
    fn match_period(&self) -> GString {
        GString::from(match self.match_state.period {
            Period::Regulation => "regulation",
            Period::SecondHalf => "second_half",
            Period::Overtime { .. } => "overtime",
        })
    }

    /// Hardpoint: seconds a player has spent in the live zone this match
    /// (tests), or -1 if absent.
    #[func]
    fn player_hill_seconds(&self, id: i32) -> f64 {
        self.avatars
            .iter()
            .find(|a| i32::from(a.id) == id)
            .map_or(-1.0, |a| f64::from(a.hill.seconds))
    }

    /// Domination (tests): how many flags the map has (0 in other modes).
    #[func]
    fn dom_flag_count(&self) -> i32 {
        self.dom_points.len() as i32
    }

    /// Domination (tests): the team that owns flag `index`, or -1.
    #[func]
    fn dom_flag_owner(&self, index: i32) -> i32 {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.dom.flags.get(i))
            .and_then(|f| f.owner)
            .map_or(-1, i32::from)
    }

    /// Domination (tests): each flag's centre at floor height, in order.
    #[func]
    fn dom_flag_floors(&self) -> Array<Vector3> {
        self.dom_points
            .iter()
            .map(|p| to_godot(p.center - Vec3::Y * p.half_height))
            .collect()
    }

    /// Domination (tests): whether halftime has swapped the sides.
    #[func]
    fn dom_swapped(&self) -> bool {
        self.dom.swapped
    }

    /// Debug: jumps the match clock to `seconds` left (tests).
    #[func]
    fn debug_match_time_left(&mut self, seconds: f32) {
        if self.match_state.phase == Phase::Playing {
            self.match_state.time_left = seconds.max(0.0);
            self.broadcast_match();
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

    /// Debug: player `id`'s inputs hold interact while `held`, whatever
    /// they send (tests).
    #[func]
    fn debug_interact(&mut self, id: i32, held: bool) {
        let Ok(id) = u8::try_from(id) else { return };
        self.forced_interact.retain(|p| *p != id);
        if held {
            self.forced_interact.push(id);
        }
    }

    /// Search and Destroy: where the charge is ("carried", "dropped",
    /// "planted", "detonated", "defused", or "" with no round on).
    #[func]
    fn charge_status(&self) -> GString {
        GString::from(match self.charge.map(|c| c.state) {
            None => "",
            Some(ChargeState::Carried { .. }) => "carried",
            Some(ChargeState::Dropped { .. }) => "dropped",
            Some(ChargeState::Planted { .. }) => "planted",
            Some(ChargeState::Detonated { .. }) => "detonated",
            Some(ChargeState::Defused { .. }) => "defused",
        })
    }

    /// Search and Destroy: who carries the charge, or -1.
    #[func]
    fn charge_carrier(&self) -> i32 {
        self.charge.and_then(|c| c.carrier()).map_or(-1, i32::from)
    }

    /// Search and Destroy: who is planting or defusing, or -1.
    #[func]
    fn charge_user(&self) -> i32 {
        self.charge.and_then(|c| c.user()).map_or(-1, i32::from)
    }

    /// Search and Destroy: where the charge is (its carrier's feet, or where
    /// it lies or stands).
    #[func]
    fn charge_position(&self) -> Vector3 {
        self.charge.map_or(Vector3::ZERO, |c| to_godot(c.position))
    }

    /// Search and Destroy: the sites' centres at floor height, in order.
    #[func]
    fn charge_site_floors(&self) -> Array<Vector3> {
        self.charge_sites
            .iter()
            .map(|s| to_godot(s.center - Vec3::new(0.0, s.half_extents.y, 0.0)))
            .collect()
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

    /// How many lanes the map gives the bots (tests).
    #[func]
    fn bot_lane_count(&self) -> i32 {
        self.bot_lanes.len() as i32
    }

    /// Bot tactics so far (tests): `lanes` taken and times in `cover`.
    #[func]
    fn bot_tactics(&self) -> VarDictionary {
        let brains = self.avatars.iter().filter_map(|a| a.brain.as_ref());
        let (lanes, covers) = brains.fold((0u32, 0u32), |(l, c), b| {
            (l + b.lanes_taken, c + b.covers_taken)
        });
        let mut d = VarDictionary::new();
        d.set("lanes", lanes);
        d.set("cover", covers);
        let sum = |f: fn(&BotBrain) -> u32| -> u32 {
            self.avatars
                .iter()
                .filter_map(|a| a.brain.as_ref())
                .map(f)
                .sum()
        };
        d.set("frags", sum(|b| b.frags_thrown));
        d.set("tacticals", sum(|b| b.tacticals_thrown));
        d.set("flights", sum(|b| b.flights));
        d.set("flashed", sum(|b| b.flashed));
        d
    }

    /// One bot's state (tests): `blind`, `fleeing`, the enemy it has in
    /// `sight` (-1 for none), its view `yaw`, and its `frags` thrown. Empty
    /// for a player or no one.
    #[func]
    fn bot_status(&self, id: i32) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(brain) = self
            .avatars
            .iter()
            .find(|a| i32::from(a.id) == id)
            .and_then(|a| a.brain.as_ref())
        else {
            return d;
        };
        d.set("blind", brain.blind_at(self.tick));
        d.set("fleeing", brain.fleeing());
        d.set("sight", brain.target_in_sight().map_or(-1, i32::from));
        d.set("yaw", brain.view().x);
        d.set("frags", brain.frags_thrown);
        d
    }

    /// Debug: a bot takes every grenade chance (tests).
    #[func]
    fn debug_bot_grenades(&mut self, id: i32, always: bool) {
        if let Some(brain) = self
            .avatars
            .iter_mut()
            .find(|a| i32::from(a.id) == id)
            .and_then(|a| a.brain.as_mut())
        {
            brain.always_throw = always;
        }
    }

    /// Debug: a bot stands still and presses nothing while `passive`, so a
    /// test can use it as a target that never fights back (tests).
    #[func]
    fn debug_bot_passive(&mut self, id: i32, passive: bool) {
        if let Some(brain) = self
            .avatars
            .iter_mut()
            .find(|a| i32::from(a.id) == id)
            .and_then(|a| a.brain.as_mut())
        {
            brain.passive = passive;
        }
    }

    /// Debug: turns a bot to face `point` level (tests).
    #[func]
    fn debug_bot_face(&mut self, id: i32, point: Vector3) {
        if let Some(avatar) = self.avatars.iter_mut().find(|a| i32::from(a.id) == id)
            && let Some(brain) = &mut avatar.brain
        {
            let mut view = shooter_sim::look::bearing(avatar.predicted.position, to_sim(point));
            view.y = 0.0;
            brain.face(view);
        }
    }

    /// Debug: lays a live grenade of `kind` ("frag", "flash", "smoke") at
    /// rest at `position`, thrown by player `owner` (-1 for no one), going
    /// off in `fuse` seconds (tests).
    #[func]
    fn debug_grenade(&mut self, owner: i32, kind: GString, position: Vector3, fuse: f32) {
        let Some(kind) = equipment::Kind::by_key(&kind.to_string()) else {
            return;
        };
        let mut grenade = equipment::Grenade::dropped(kind, to_sim(position), Vec3::ZERO, fuse);
        grenade.resting = true;
        self.grenades.add(LiveGrenade {
            owner: u8::try_from(owner).ok(),
            shot: u32::MAX - self.grenades.live.len() as u32,
            grenade,
        });
    }

    /// The Search and Destroy round being played, from 1, or 0 for none.
    #[func]
    fn round_number(&self) -> i32 {
        self.round.map_or(0, |r| r.number as i32)
    }

    /// "freeze", "live", "over", or "" when no round is on.
    #[func]
    fn round_phase(&self) -> GString {
        GString::from(match self.round.map(|r| r.phase) {
            None => "",
            Some(RoundPhase::Freeze) => "freeze",
            Some(RoundPhase::Live) => "live",
            Some(RoundPhase::Over { .. }) => "over",
        })
    }

    /// The team attacking this round, or -1 when no round is on.
    #[func]
    fn attackers(&self) -> i32 {
        self.round.map_or(-1, |r| i32::from(r.attackers))
    }

    /// A player's feet position (tests), or the origin if absent.
    #[func]
    fn player_position(&self, id: i32) -> Vector3 {
        self.avatars
            .iter()
            .find(|a| i32::from(a.id) == id)
            .map_or(Vector3::ZERO, |a| to_godot(a.predicted.position))
    }

    /// Whether a player is alive (tests), false if absent.
    #[func]
    fn player_alive(&self, id: i32) -> bool {
        self.avatars
            .iter()
            .any(|a| i32::from(a.id) == id && a.predicted.state.alive())
    }

    /// Points a team has this match: kills, captures in Capture the Flag,
    /// Hardpoint points, or rounds won in Search and Destroy.
    #[func]
    fn team_score(&self, team: i32) -> i64 {
        usize::try_from(team)
            .ok()
            .and_then(|t| self.match_state.scores.get(t))
            .map_or(0, |s| i64::from(*s))
    }

    /// Zombies: the round being played or counted down to (0 outside Zombies).
    #[func]
    fn zombies_round(&self) -> i32 {
        self.horde.as_ref().map_or(0, |h| h.round() as i32)
    }

    /// Zombies: "break", "fighting", or "over" ("" outside Zombies).
    #[func]
    fn zombies_phase(&self) -> GString {
        let Some(horde) = &self.horde else {
            return GString::new();
        };
        GString::from(if horde.over() {
            "over"
        } else if horde.fighting() {
            "fighting"
        } else {
            "break"
        })
    }

    /// Zombies: zombies alive now.
    #[func]
    fn zombie_count(&self) -> i32 {
        self.horde.as_ref().map_or(0, |h| h.alive() as i32)
    }

    /// Zombies: every living zombie's id, feet, and health (tests).
    #[func]
    fn zombies_alive(&self) -> Array<VarDictionary> {
        let mut out = Array::new();
        let Some(horde) = &self.horde else {
            return out;
        };
        for (id, feet) in horde.living() {
            out.push(&vdict! {
                "id" => i64::from(id),
                "position" => to_godot(feet),
                "health" => horde.health(id).unwrap_or(0.0),
            });
        }
        out
    }

    /// Zombies: boards up on window `index`, or -1 if there is none.
    #[func]
    fn window_boards(&self, index: i32) -> i32 {
        self.horde
            .as_ref()
            .and_then(|h| h.window_boards(usize::try_from(index).ok()?))
            .map_or(-1, i32::from)
    }

    /// Zombies: windows on the map.
    #[func]
    fn window_count(&self) -> i32 {
        self.horde.as_ref().map_or(0, |h| h.window_count() as i32)
    }

    /// Zombies: a player's points.
    #[func]
    fn player_points(&self, id: i32) -> i64 {
        self.avatars
            .iter()
            .find(|a| i32::from(a.id) == id)
            .map_or(0, |a| i64::from(a.zombies.points))
    }

    /// Zombies: doors on the map, and whether door `index` is open.
    #[func]
    fn door_count(&self) -> i32 {
        self.horde.as_ref().map_or(0, |h| h.door_count() as i32)
    }

    #[func]
    fn door_open(&self, index: i32) -> bool {
        self.horde
            .as_ref()
            .zip(usize::try_from(index).ok())
            .is_some_and(|(h, i)| h.door_open(i))
    }

    /// Zombies: player `id` uses door (`kind` "door") or window ("window")
    /// `index` this tick, as if looking at it with the use button held;
    /// `pressed` on the first tick (tests).
    #[func]
    fn debug_use(&mut self, id: i32, kind: GString, index: i32, pressed: bool) {
        let (Ok(id), Ok(index)) = (u8::try_from(id), usize::try_from(index)) else {
            return;
        };
        let node = self
            .horde
            .as_ref()
            .and_then(|h| match kind.to_string().as_str() {
                "door" => h.door_node(index),
                _ => h.window_node(index),
            });
        if let Some(node) = node {
            self.zombies_uses(vec![(id, node, pressed)]);
        }
    }

    /// Zombies: power-ups on the ground, and seconds of Insta-Kill and
    /// Double Points left.
    #[func]
    fn power_ups(&self) -> VarDictionary {
        let Some(horde) = &self.horde else {
            return VarDictionary::new();
        };
        let (insta, double) = horde.timers();
        vdict! {
            "on_ground" => horde.drops_on_ground() as i64,
            "insta_kill" => insta,
            "double_points" => double,
        }
    }

    /// Zombies: drops a power-up ("max_ammo", "insta_kill", "double_points",
    /// "nuke") at `position` (tests).
    #[func]
    fn debug_drop_power_up(&mut self, kind: GString, position: Vector3) {
        let Some(kind) = zombies::PowerUp::from_key(&kind.to_string()) else {
            return;
        };
        if let Some(horde) = &mut self.horde {
            horde.drop_power_up(kind, to_sim(position));
        }
        self.drain_horde();
    }

    /// Zombies: whether player `id` is down in last stand.
    #[func]
    fn player_downed(&self, id: i32) -> bool {
        self.avatars
            .iter()
            .any(|a| i32::from(a.id) == id && a.last_stand.is_some())
    }

    /// Zombies: teammates player `id` has revived this game.
    #[func]
    fn player_revives(&self, id: i32) -> i32 {
        self.avatars
            .iter()
            .find(|a| i32::from(a.id) == id)
            .map_or(0, |a| a.zombies.revives as i32)
    }

    /// Zombies: puts player `id` down as a zombie's second hit would (tests).
    #[func]
    fn debug_down(&mut self, id: i32) {
        if let Ok(id) = u8::try_from(id) {
            self.go_down(id);
        }
    }

    /// Zombies: gives player `id` points outright (tests).
    #[func]
    fn debug_add_points(&mut self, id: i32, points: i32) {
        let Some(avatar) = self.avatars.iter_mut().find(|a| i32::from(a.id) == id) else {
            return;
        };
        avatar.zombies.points = avatar.zombies.points.saturating_add(points.max(0) as u32);
        let id = avatar.id;
        self.send_points(id, points.max(0));
    }

    /// Zombies: ends the break before the next round now (tests).
    #[func]
    fn debug_skip_zombie_break(&mut self) {
        if let Some(horde) = &mut self.horde {
            horde.skip_break();
        }
    }

    /// Zombies: player `attacker` deals `damage` to zombie `zombie` as if
    /// shot, with the points it pays (tests).
    #[func]
    fn debug_hurt_zombie(&mut self, attacker: i32, zombie: i32, damage: f32, headshot: bool) {
        let (Ok(attacker), Ok(zombie)) = (u8::try_from(attacker), u8::try_from(zombie)) else {
            return;
        };
        let hit = TargetHit {
            id: zombie,
            damage,
            headshot,
            distance: 5.0,
            direction: Vec3::NEG_Z,
            part: shooter_sim::hitbox::CHEST_SHAPE as u8,
        };
        self.hurt_zombies(attacker, 0, &[hit], false);
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
            build: crate::build_info::BUILD.to_owned(),
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
        self.bot_lanes = map_lanes(&map);
        self.flag_homes = map_flags(&map);
        self.zones = map_zones(&map);
        self.dom_points = map_dom_points(&map);
        (self.charge_site_names, self.charge_sites) = map_charge_sites(&map).into_iter().unzip();
        for name in CLIENT_ONLY_NODES {
            if let Some(node) = map.get_node_or_null(name) {
                node.free();
            }
        }
        // The offline Zombies director: the server runs the horde itself.
        if let Some(director) = map.get_node_or_null("Director") {
            director.free();
        }
        map.set_process_mode(godot::classes::node::ProcessMode::PAUSABLE);
        self.base_mut().add_child(&map);
        if self.game == Mode::Zombies {
            let parent = self.base().clone().upcast::<Node3D>();
            self.horde = Horde::new(&map, parent);
        }
        // Groups register when a node enters the tree, so find the props
        // manager after the map is a child.
        self.props = PropsBridge::discover_under(&map);
        self.props_population = None;
        self.props_retry = 0;
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
                    class: Class::default(),
                    chat: ChatLimiter::default(),
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
                // Zombies is for up to four players, as in Black Ops.
                let full = self.game == Mode::Zombies
                    && self.clients.iter().filter(|c| c.player.is_some()).count()
                        >= zombies::MAX_PLAYERS;
                let Some(id) = (0..MAX_PLAYERS as u8)
                    .find(|id| !self.clients.iter().any(|c| c.player == Some(*id)))
                    .filter(|_| !full)
                else {
                    peer.peer_disconnect_later();
                    return;
                };
                self.clients[index].player = Some(id);
                let name = tdm::clean_name(&name, &format!("Player {}", id + 1));
                let class = self.clients[index].class;
                // The welcome goes first, so the client knows its id before
                // the roster arrives.
                let welcome = ServerMessage::Welcome {
                    player_id: id,
                    tick: self.tick,
                    snapshot_interval: self.snapshot_interval.clamp(1, 255) as u8,
                    mode: self.game,
                    server_build: crate::build_info::BUILD.to_owned(),
                };
                transport::send_reliable(&mut peer, &welcome.encode(&[]));
                // Everyone already here, then the newcomer for everyone.
                let roster: Vec<GameEvent> = self.avatars.iter().map(joined_event).collect();
                self.clients[index].outbox.extend(roster);
                self.spawn_avatar(id, false, &name, class, None);
                self.balance_bots();
                let event = self.match_event();
                self.send_event(id, event);
                if let Some(event) = self.flags_event() {
                    self.send_event(id, event);
                }
                if !self.zones.is_empty() {
                    let zones = self.zones_event();
                    self.send_event(id, zones);
                    let status = self.hardpoint_event();
                    self.send_event(id, status);
                }
                if !self.dom_points.is_empty() {
                    let flags = self.dom_flags_event();
                    self.send_event(id, flags);
                    let status = self.domination_event();
                    self.send_event(id, status);
                }
                // A late joiner needs the props as they are now: everyone
                // already here saw each change as it happened.
                for state in self.props.snapshot_states() {
                    self.send_event(id, prop_event(&state));
                }
                if let Some(event) = self.round_event() {
                    self.send_event(id, event);
                }
                if !self.charge_sites.is_empty() {
                    let sites = self.charge_sites_event();
                    self.send_event(id, sites);
                }
                if let Some(event) = self.charge_event_for(id) {
                    self.send_event(id, event);
                }
                if let Some(horde) = &self.horde {
                    let mut events = vec![horde.round_event(self.tick)];
                    events.extend(horde.window_events());
                    events.extend(horde.door_events());
                    events.extend(horde.drop_events());
                    events.push(horde.timers_event(self.tick));
                    for avatar in &self.avatars {
                        if let Some(stand) = &avatar.last_stand {
                            events.push(protocol::ZombiesEvent::Downed {
                                player: avatar.id,
                                bleeds_out_at: Some(self.ticks_from_now(stand.bleed_left)),
                            });
                        }
                    }
                    for event in events {
                        self.send_event(id, GameEvent::Zombies(event));
                    }
                    self.send_points(id, 0);
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
            ClientMessage::Class(class) => {
                // Never trust the client: a class that breaks a rule is
                // ignored, and the player keeps the one they had.
                if let Err(error) = class.validate() {
                    godot_warn!("server: refused a class ({error:?})");
                    return;
                }
                self.clients[index].class = class;
                let freeze = self.round.is_some_and(|r| r.phase == RoundPhase::Freeze);
                let gun_game = matches!(self.game, Mode::GunGame(_));
                if let Some(id) = self.clients[index].player
                    && let Some(avatar) = self.avatars.iter_mut().find(|a| a.id == id)
                {
                    avatar.class = class;
                    // During a round's freeze a new class applies at once.
                    if freeze && !gun_game && avatar.predicted.state.alive() {
                        let movement = avatar.predicted.state.movement;
                        avatar.predicted.state = PlayerState::new(loadout_for(&class));
                        avatar.predicted.state.movement = movement;
                        avatar.predicted.state.combat = class.combat().first_draw();
                    }
                }
            }
            ClientMessage::Chat { text, team } => self.relay_chat(index, &text, team),
            ClientMessage::SkipKillcam => self.skip_killcam(index),
        }
    }

    /// The client skipped its killcam: it respawns as soon as the respawn
    /// delay allows, as in Call of Duty (`_killcam.gsc` ends the killcam on
    /// the use button, and the player spawns).
    fn skip_killcam(&mut self, index: usize) {
        let Some(id) = self.clients[index].player else {
            return;
        };
        let (tick, delay) = (self.tick, self.respawn_delay);
        let Some(avatar) = self.avatars.iter_mut().find(|a| a.id == id) else {
            return;
        };
        let Some(died) = avatar.killcam_since.take() else {
            return;
        };
        let dead_for = tick.saturating_sub(died) as f32 / TICK_RATE as f32;
        avatar.respawn_in = avatar.respawn_in.min((delay - dead_for).max(0.0));
    }

    /// Passes a chat message on, cleaned and rate limited: to everyone, or to
    /// the sender's team (in Free-for-all, a team message reaches only the
    /// sender).
    fn relay_chat(&mut self, index: usize, text: &str, team: bool) {
        let Some(from) = self.clients[index].player else {
            return;
        };
        let Some(text) = chat::clean_message(text) else {
            return;
        };
        if !self.clients[index].chat.allow() {
            return;
        }
        let event = GameEvent::Chat { from, text, team };
        if !team {
            self.broadcast_event(&event);
            return;
        }
        let Some(sender) = self.avatars.iter().find(|a| a.id == from) else {
            return;
        };
        let side = self.side(sender);
        let recipients: Vec<u8> = self
            .avatars
            .iter()
            .filter(|a| self.side(a) == side)
            .map(|a| a.id)
            .collect();
        for id in recipients {
            self.send_event(id, event.clone());
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
        class: Class,
        brain: Option<BotBrain>,
    ) {
        let mut counts = [0; tdm::TEAM_COUNT];
        for avatar in &self.avatars {
            counts[usize::from(avatar.team)] += 1;
        }
        // Without teams, and in co-op, everyone is on team 0.
        let team = if self.game.free_for_all() || self.game == Mode::Zombies {
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
            class,
            brain,
            respawn_in: 0.0,
            killcam_since: None,
            spawned_at: self.tick,
            score: ScoreState::default(),
            hits: 0,
            hit_heading: 0,
            hit_part: 0,
            clock: BodyClock::default(),
            progress: Progress::default(),
            hill: HillTime::default(),
            interact: false,
            zombies: ZombiesStats {
                points: zombies::START_POINTS,
                ..ZombiesStats::default()
            },
            last_stand: None,
        });
        let event = joined_event(self.avatars.last().expect("just pushed"));
        self.broadcast_event(&event);
        // Before a match the team starts together; during one, or without
        // teams, somewhere safe. In a Search and Destroy round, at their
        // side: joiners in a round's freeze play it, and later ones wait for
        // the next round.
        let start =
            (!self.match_state.scoring() && !self.game.free_for_all()) || self.round.is_some();
        let slot = counts[usize::from(team)];
        // A joiner's first spawn is their first of the game.
        self.respawn(id, start.then_some(slot), true);
        // Zombies: a player joining a game under way watches until the next
        // round starts, as in Black Ops.
        let waits = self.round.is_some_and(|r| r.phase != RoundPhase::Freeze)
            || (self.horde.is_some() && self.match_state.scoring());
        if waits && let Some(avatar) = self.avatars.iter_mut().find(|a| a.id == id) {
            avatar.predicted.state.health = 0.0;
        }
        self.next_scoreboard = self.tick;
    }

    /// The side of the map `team` plays from: their own, except in Search
    /// and Destroy, where the attackers take side 0, and in Domination's
    /// second half, when the teams have swapped.
    fn side_of(&self, team: Team) -> Team {
        match self.round {
            Some(round) => round.side_of(team),
            None if self.dom.swapped => 1 - team.min(1),
            None => team,
        }
    }

    /// Whether the dead come back on their own: always, except while a
    /// Search and Destroy match is being played or shows its results.
    fn respawns(&self) -> bool {
        // Zombies: the dead come back when the next round starts.
        !matches!(self.game, Mode::SearchAndDestroy | Mode::Zombies)
            || matches!(self.match_state.phase, Phase::Lobby | Phase::Countdown)
    }

    /// Whether players are held still (a round's freeze).
    fn frozen(&self) -> bool {
        self.round.is_some_and(|r| !r.can_move())
    }

    /// Brings everyone back at their side's start spawns, in join order, at
    /// the start of a match or a round. Without teams, each player spawns
    /// somewhere away from the others.
    fn respawn_all_at_start(&mut self) {
        self.recent_deaths.clear();
        // A fresh match or round starts with no grenades about.
        self.grenades.clear();
        self.smokes.clear();
        let free_for_all = self.game.free_for_all();
        let mut slots = [0; tdm::TEAM_COUNT];
        let mut order: Vec<(Team, u8)> = self.avatars.iter().map(|a| (a.team, a.id)).collect();
        order.sort_unstable();
        for (team, id) in order {
            let slot = &mut slots[usize::from(team)];
            self.respawn(id, (!free_for_all).then_some(*slot), true);
            *slot += 1;
        }
    }

    /// Brings a player back to life at a spawn point with their loadout.
    /// `start_slot` places them among their team's start spawns instead of
    /// at the safest spawn. `first` is the player's first spawn of a game or
    /// round: each weapon's first draw is then a first raise.
    fn respawn(&mut self, id: u8, start_slot: Option<usize>, first: bool) {
        let Some(index) = self.avatars.iter().position(|a| a.id == id) else {
            return;
        };
        let side = self.side_of(self.avatars[index].team);
        let point = match start_slot {
            Some(slot) => tdm::start_spawn(&self.spawn_points, side, slot),
            None => self.safest_spawn(index),
        };
        let Some(point) = point.and_then(|i| self.spawn_points.get(i)).copied() else {
            return;
        };
        let tick = self.tick;
        let gun_game = matches!(self.game, Mode::GunGame(_));
        let avatar = &mut self.avatars[index];
        if let Some(brain) = &mut avatar.brain {
            // Bots pick a default class every life, as Call of Duty's
            // bots do.
            let pick = rng::hash(u32::from(avatar.id), tick) as usize % DefaultClass::ALL.len();
            avatar.class = DefaultClass::ALL[pick].class();
            brain.on_spawn(point.yaw, tick);
        }
        let mut state = if self.game == Mode::Zombies {
            // Zombies: the sidearm and two frags; the rest is bought.
            let mut state = PlayerState::new(loadout_for(&avatar.class));
            state.combat = zombies::start_combat();
            state
        } else if gun_game {
            // Gun Game: the ladder's weapon and nothing else.
            let weapon = gun_game::weapon(avatar.progress.level);
            let mut state = PlayerState::new([weapon, weapon]);
            state.combat = CombatState::only(weapon);
            state
        } else {
            let mut state = PlayerState::new(loadout_for(&avatar.class));
            state.combat = avatar.class.combat();
            state
        };
        if first {
            state.combat = state.combat.first_draw();
        }
        avatar.predicted = Predicted {
            state,
            position: point.position,
            on_floor: false,
        };
        avatar.yaw = point.yaw;
        avatar.pitch = 0.0;
        avatar.spawned_at = tick;
        avatar.last_stand = None;
        avatar.respawn_in = self.respawn_delay;
        avatar.killcam_since = None;
        body::place(&mut avatar.parts, &self.params.movement, &avatar.predicted);
        self.send_event(id, GameEvent::Spawned { yaw: point.yaw });
    }

    /// The spawn point furthest from trouble for the player at `index`.
    fn safest_spawn(&self, index: usize) -> Option<usize> {
        let me = &self.avatars[index];
        let mut threats: Vec<Threat> = self
            .avatars
            .iter()
            .filter(|a| a.id != me.id && a.predicted.state.alive())
            .map(|a| Threat {
                position: a.predicted.position,
                enemy: self.hostile(a, me),
            })
            .collect();
        // The fight is on the hill: keep spawns off it.
        if let Some(zone) = self.zones.get(self.hardpoint.active) {
            threats.extend((0..ZONE_SPAWN_THREATS).map(|_| Threat {
                position: zone.center,
                enemy: true,
            }));
        }
        // Domination: spawn behind the team's own flags, away from the enemy's.
        threats.extend(self.dom.spawn_threats(&self.dom_points, me.team));
        let eyes: Vec<Vector3> = self
            .avatars
            .iter()
            .filter(|a| self.hostile(a, me) && a.predicted.state.alive())
            .map(|a| {
                let movement = &a.predicted.state.movement;
                to_godot(a.predicted.position)
                    + Vector3::UP * movement.eye_height(&self.params.movement)
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
            self.side_of(me.team),
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
                self.spawn_avatar(id, true, &name, Class::default(), Some(brain));
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
        let mut strikes = Vec::new();
        let mut throws: Vec<Throw> = Vec::new();
        let mut fall_deaths = Vec::new();
        let mut respawns = Vec::new();
        let mut spent: Vec<(u8, u32)> = Vec::new();
        let mut uses: Vec<(u8, Gd<Node>, bool)> = Vec::new();
        let mut revivers: Vec<(u8, u8)> = Vec::new();
        // Zombies: who is down, and where, as of the start of the tick.
        let downed: Vec<(u8, Vec3)> = self
            .avatars
            .iter()
            .filter(|a| a.last_stand.is_some())
            .map(|a| (a.id, a.predicted.position))
            .collect();
        let zombies_mode = self.horde.is_some();
        let tick = self.tick;
        // What bots know about everyone this tick.
        let mut players: Vec<BotPlayer> = self
            .avatars
            .iter()
            .map(|a| BotPlayer {
                id: a.id,
                team: self.side(a),
                alive: a.predicted.state.alive(),
                feet: to_godot(a.predicted.position),
                crouch: a.predicted.state.movement.crouch,
                lane: a.brain.as_ref().and_then(|b| b.lane()),
            })
            .collect();
        // Each team's own side, for bots' goals on the enemy's side.
        let mut team_spawns: [Vec<Vector3>; 2] = [Vec::new(), Vec::new()];
        for point in &self.spawn_points {
            if let Some(side) = point.team {
                for team in 0..tdm::TEAM_COUNT as Team {
                    if self.side_of(team) == side.min(1) {
                        team_spawns[usize::from(team)].push(to_godot(point.position));
                    }
                }
            }
        }
        let fighters: Vec<Fighter> = self
            .avatars
            .iter()
            .map(|a| Fighter {
                id: a.id,
                side: self.side(a),
                alive: a.predicted.state.alive(),
                spawned_at: a.spawned_at,
            })
            .collect();
        // Live grenades and smoke, for bots.
        let smoke_ticks = (equipment::SMOKE_SECONDS * TICK_RATE as f32) as Tick;
        self.smokes
            .retain(|smoke| tick.saturating_sub(smoke.popped) < smoke_ticks);
        let smokes: Vec<BotSmoke> = self
            .smokes
            .iter()
            .map(|smoke| BotSmoke {
                center: smoke.at + Vec3::Y * equipment::SMOKE_HEIGHT,
                age: tick.saturating_sub(smoke.popped) as f32 * TICK_DT,
            })
            .collect();
        let bot_grenades: Vec<BotGrenade> = self
            .grenades
            .live
            .iter()
            .map(|g| BotGrenade {
                kind: g.grenade.kind,
                position: to_godot(g.grenade.position),
                owner: g.owner,
                owner_team: g
                    .owner
                    .and_then(|id| players.iter().find(|p| p.id == id))
                    .map(|p| p.team),
            })
            .collect();
        let mut throw_checks = THROW_CHECKS_PER_TICK;
        let objectives = self.bot_objectives();
        let frozen = self.frozen();
        // Planting and defusing hold the player still with the weapon away.
        let charge = self.charge;
        let charge_cx = ChargeContext {
            rules: &self.snd_rules,
            sites: &self.charge_sites,
            attackers: self.round.map_or(0, |r| r.attackers),
            live: self.round.is_some_and(|r| r.live()),
        };
        let respawning = self.respawns();
        let mut space = self
            .base()
            .get_world_3d()
            .and_then(|world| world.get_direct_space_state());
        // Gun Game hands out weapons by the ladder only.
        let racks = !matches!(self.game, Mode::GunGame(_));
        for avatar in &mut self.avatars {
            let inputs: Vec<(PlayerInput, Option<f64>, Option<f32>)> = if avatar.bot {
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
                            lanes: &self.bot_lanes,
                            grenades: &bot_grenades,
                            smokes: &smokes,
                            throw_checks: &mut throw_checks,
                        };
                        let objective = objectives
                            .iter()
                            .find(|(id, _)| *id == avatar.id)
                            .and_then(|(_, o)| *o);
                        let input = brain.think(&me, &avatar.predicted, &mut world, objective);
                        // Bots that think later this tick see the lane this
                        // one just took.
                        if let Some(p) = players.iter_mut().find(|p| p.id == avatar.id) {
                            p.lane = brain.lane();
                        }
                        input
                    }
                    _ => bot_input(avatar, tick),
                };
                vec![(input, None, None)]
            } else {
                let Some(client) = self
                    .clients
                    .iter_mut()
                    .find(|c| c.player == Some(avatar.id))
                else {
                    continue;
                };
                let inputs: Vec<PlayerInput> = client.inputs.take().map(|(_, i)| i).collect();
                let rtt = (transport::round_trip_ms(&client.peer) / 1000.0) as f32;
                inputs
                    .into_iter()
                    .map(|input| {
                        if input.view_tick.is_some() {
                            client.view_tick = input.view_tick;
                        }
                        (input, client.view_tick, Some(rtt))
                    })
                    .collect()
            };
            for (input, view_tick, rtt) in &inputs {
                let mut input = *input;
                if frozen {
                    input = input.held_still();
                }
                if self.forced_interact.contains(&avatar.id) {
                    input.interact = true;
                }
                if let Some(charge) = &charge
                    && input.interact
                    && charge
                        .spot(&charge_cx, &charge_player(avatar, input.interact))
                        .is_some()
                {
                    input = input.using_charge();
                }
                // Zombies: holding the use button beside a teammate who is
                // down revives them, held still with the weapon away (as
                // the client predicts); otherwise at a door or a window.
                let reviving = (zombies_mode && input.interact)
                    .then(|| revive_target(avatar, &downed))
                    .flatten();
                if let Some(target) = reviving {
                    revivers.push((avatar.id, target));
                    input = input.using_charge();
                } else if zombies_mode
                    && input.interact
                    && let Some(node) = looked_at(avatar, &input, &self.params)
                {
                    uses.push((avatar.id, node, !avatar.interact));
                }
                avatar.interact = input.interact;
                // Zombies: a wall weapon costs its price, and its ammo half
                // that, paid when it is taken.
                let mut cost = None;
                if let Some(weapon) = input.pickup {
                    let price = if racks {
                        pickup_in_reach(avatar, &input, weapon, &self.params)
                    } else {
                        None
                    };
                    match price {
                        Some(price) if zombies_mode => {
                            let combat = &avatar.predicted.state.combat;
                            let due = zombies::pickup_cost(price, combat.carries(weapon));
                            if avatar.zombies.points >= due {
                                cost = Some(due);
                            } else {
                                input.pickup = None;
                            }
                        }
                        Some(_) => {}
                        None => input.pickup = None,
                    }
                }
                // A click between ticks may only aim somewhere the view
                // passed through since the last input; the client checked the
                // same way before predicting the shot.
                input.fire_aim = subtick::check(Some((avatar.yaw, avatar.pitch)), &input);
                // A player's lunge must point at an enemy they could see;
                // otherwise it would be a free dash. Bots aim theirs here.
                // Zombies has no enemy players: a lunge goes for a zombie,
                // and a free dash gains nothing against the horde.
                if let Some(offset) = input.lunge
                    && !avatar.bot
                    && !zombies_mode
                    && !lunge_backed(
                        &self.lag,
                        tick,
                        (*view_tick, *rtt),
                        avatar,
                        offset,
                        &fighters,
                    )
                {
                    input.lunge = None;
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
                if let Some(cost) = cost
                    && events
                        .iter()
                        .any(|e| matches!(e, PlayerEvent::Combat(CombatEvent::PickedUp { .. })))
                {
                    avatar.zombies.points -= cost;
                    spent.push((avatar.id, cost));
                }
                avatar.yaw = input.yaw;
                avatar.pitch = input.pitch;
                for event in &events {
                    match *event {
                        PlayerEvent::Combat(CombatEvent::Fired {
                            weapon,
                            yaw,
                            pitch,
                            aim_offset,
                            shot,
                            ..
                        }) => {
                            let p = &avatar.predicted;
                            let eye = to_godot(p.position)
                                + Vector3::UP * p.state.movement.eye_height(&self.params.movement);
                            shots.push(Shot {
                                shooter: avatar.id,
                                eye,
                                yaw,
                                pitch,
                                weapon,
                                aim_offset,
                                shot,
                                view_tick: *view_tick,
                                rtt: *rtt,
                            });
                        }
                        PlayerEvent::Combat(CombatEvent::MeleeStruck { weapon, shot, .. }) => {
                            let p = &avatar.predicted;
                            let eye = to_godot(p.position)
                                + Vector3::UP * p.state.movement.eye_height(&self.params.movement);
                            strikes.push(Melee {
                                attacker: avatar.id,
                                feet: p.position,
                                eye,
                                yaw: input.yaw,
                                weapon,
                                shot,
                                view_tick: *view_tick,
                                rtt: *rtt,
                            });
                        }
                        PlayerEvent::Combat(CombatEvent::Thrown {
                            kind,
                            yaw,
                            pitch,
                            fuse,
                            shot,
                        }) => {
                            let p = &avatar.predicted;
                            let eye = to_godot(p.position)
                                + Vector3::UP * p.state.movement.eye_height(&self.params.movement);
                            throws.push(Throw {
                                owner: avatar.id,
                                shot,
                                kind,
                                eye,
                                yaw,
                                pitch,
                                carrier: p.state.movement.velocity,
                                fuse,
                                dropped: false,
                            });
                        }
                        PlayerEvent::GrenadeDropped { kind, fuse, shot } => {
                            let p = &avatar.predicted;
                            throws.push(Throw {
                                owner: avatar.id,
                                shot,
                                kind,
                                eye: to_godot(p.position) + Vector3::UP * 0.4,
                                yaw: input.yaw,
                                pitch: 0.0,
                                carrier: p.state.movement.velocity * 0.5,
                                fuse,
                                dropped: true,
                            });
                        }
                        // Only falls kill inside the step; shots are resolved below.
                        PlayerEvent::Died => fall_deaths.push(avatar.id),
                        _ => {}
                    }
                }
            }
            avatar.score.tick(TICK_DT);
            avatar.clock.tick(&body_state(avatar), TICK_DT);
            if avatar.bot {
                let combat = &mut avatar.predicted.state.combat;
                if combat.weapon().magazine == 0 && combat.weapon().reserve == 0 {
                    combat.refill_ammo(&mut Vec::new());
                }
            }
            if avatar.predicted.state.alive() || !respawning {
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
            self.respawn(id, None, false);
        }
        for shot in shots {
            self.resolve_shot(&shot);
        }
        for strike in strikes {
            self.resolve_melee(&strike);
        }
        self.update_grenades(throws);
        for (id, cost) in spent {
            self.send_points(id, -(cost as i32));
        }
        self.zombies_uses(uses);
        if zombies_mode && self.match_state.scoring() {
            self.update_last_stands(&revivers);
        }
        self.update_horde();
    }

    /// Zombies: runs the horde one tick during a game, acts on what it did,
    /// tells everyone when the round changes, and ends the game once every
    /// player is down.
    fn update_horde(&mut self) {
        if !self.match_state.scoring() {
            return;
        }
        let Some(horde) = &mut self.horde else {
            return;
        };
        let players: Vec<HordePlayer> = self
            .avatars
            .iter()
            .map(|a| HordePlayer {
                id: a.id,
                feet: a.predicted.position,
                standing: a.predicted.state.alive()
                    && !a.predicted.state.downed
                    && self.tick >= a.zombies.ignored_until,
            })
            .collect();
        horde.step(&players);
        self.drain_horde();
        let everyone_down = !self.avatars.is_empty()
            && self
                .avatars
                .iter()
                .all(|a| !a.predicted.state.alive() || a.predicted.state.downed);
        if let Some(horde) = &mut self.horde
            && everyone_down
            && !horde.over()
        {
            horde.end();
            godot_print!("server: Zombies over in round {}", horde.round());
            let transition = self.match_state.finish(&self.rules, None);
            self.on_transition(transition);
        }
        self.broadcast_zombies_round();
    }

    /// Zombies: a fresh game. Everything back as the map starts, everyone
    /// on 500 points, and every client told.
    fn reset_zombies(&mut self) {
        let Some(horde) = &mut self.horde else {
            return;
        };
        // Power-ups still lying about go from every client.
        let mut events: Vec<protocol::ZombiesEvent> = horde
            .drop_events()
            .into_iter()
            .filter_map(|e| match e {
                protocol::ZombiesEvent::PowerUpDropped { id, .. } => {
                    Some(protocol::ZombiesEvent::PowerUpGone { id, by: None })
                }
                _ => None,
            })
            .collect();
        horde.reset();
        events.extend(horde.window_events());
        events.extend(horde.door_events());
        events.push(horde.timers_event(self.tick));
        for event in events {
            self.broadcast_event(&GameEvent::Zombies(event));
        }
        for avatar in &mut self.avatars {
            avatar.zombies = ZombiesStats {
                points: zombies::START_POINTS,
                ..ZombiesStats::default()
            };
        }
        let ids: Vec<u8> = self.avatars.iter().map(|a| a.id).collect();
        for id in ids {
            self.send_points(id, 0);
        }
    }

    /// Acts on what the horde did since the last call.
    fn drain_horde(&mut self) {
        let Some(horde) = &mut self.horde else {
            return;
        };
        for event in horde.take_events() {
            match event {
                HordeEvent::RoundStarted(round) => self.on_zombies_round(round),
                HordeEvent::Struck { player, from } => self.zombie_strike(player, from),
                HordeEvent::Boards { window, boards } => {
                    self.broadcast_event(&GameEvent::Zombies(protocol::ZombiesEvent::Boards {
                        window,
                        boards,
                    }));
                }
                HordeEvent::PowerUpDropped { id, kind, at } => {
                    self.broadcast_event(&GameEvent::Zombies(
                        protocol::ZombiesEvent::PowerUpDropped { id, kind, at },
                    ));
                }
                HordeEvent::PowerUpExpired { id } => {
                    self.broadcast_event(&GameEvent::Zombies(
                        protocol::ZombiesEvent::PowerUpGone { id, by: None },
                    ));
                }
                HordeEvent::PowerUpTaken {
                    id,
                    kind,
                    by,
                    nuked,
                } => {
                    godot_print!("server: player {by} took {}", kind.label());
                    self.broadcast_event(&GameEvent::Zombies(
                        protocol::ZombiesEvent::PowerUpGone { id, by: Some(by) },
                    ));
                    match kind {
                        // Every player standing gets their ammo back.
                        zombies::PowerUp::MaxAmmo => {
                            for avatar in &mut self.avatars {
                                if avatar.predicted.state.alive() {
                                    avatar.predicted.state.combat.refill_ammo(&mut Vec::new());
                                }
                            }
                        }
                        // Every player is paid, as in Black Ops.
                        zombies::PowerUp::Nuke => {
                            let ids: Vec<u8> = self.avatars.iter().map(|a| a.id).collect();
                            for id in ids {
                                self.award_points(id, zombies::NUKE_POINTS);
                            }
                            if nuked > 0 {
                                self.next_scoreboard = self.tick;
                            }
                        }
                        zombies::PowerUp::InstaKill | zombies::PowerUp::DoublePoints => {}
                    }
                }
                HordeEvent::TimersChanged => {
                    let event = self.horde.as_ref().map(|h| h.timers_event(self.tick));
                    if let Some(event) = event {
                        self.broadcast_event(&GameEvent::Zombies(event));
                    }
                }
            }
        }
    }

    /// Zombies: players holding the use button at a door (a press buys it)
    /// or a window (holding rebuilds it a board at a time, for points).
    fn zombies_uses(&mut self, uses: Vec<(u8, Gd<Node>, bool)>) {
        let tick = self.tick;
        for (id, node, pressed) in uses {
            let Some(horde) = &mut self.horde else {
                return;
            };
            if !self.match_state.scoring() || horde.over() {
                return;
            }
            if let Some(door) = horde.door_of(&node) {
                let Some(price) = horde.door_price(door) else {
                    continue;
                };
                let Some(avatar) = self.avatars.iter_mut().find(|a| a.id == id) else {
                    continue;
                };
                if !pressed || avatar.zombies.points < price || !horde.open_door(door) {
                    continue;
                }
                avatar.zombies.points -= price;
                godot_print!("server: player {id} opened door {door}");
                self.send_points(id, -(price as i32));
                self.broadcast_event(&GameEvent::Zombies(protocol::ZombiesEvent::Door {
                    door: door as u8,
                    open: true,
                }));
            } else if let Some(window) = horde.window_of(&node) {
                let Some(boards) = horde.window_boards(window) else {
                    continue;
                };
                if boards >= super::horde::WINDOW_BOARDS {
                    continue;
                }
                let Some(avatar) = self.avatars.iter_mut().find(|a| a.id == id) else {
                    continue;
                };
                // Letting go, or looking away, starts the board over.
                if avatar
                    .zombies
                    .rebuild_seen
                    .is_none_or(|seen| seen + 1 < tick)
                {
                    avatar.zombies.rebuild_for = 0.0;
                }
                avatar.zombies.rebuild_seen = Some(tick);
                avatar.zombies.rebuild_for += TICK_DT;
                if avatar.zombies.rebuild_for < zombies::REBUILD_SECONDS {
                    continue;
                }
                avatar.zombies.rebuild_for = 0.0;
                let pay = zombies::REBUILD_POINTS
                    .min(zombies::MAX_REBUILD_POINTS.saturating_sub(avatar.zombies.rebuilt));
                avatar.zombies.rebuilt += pay;
                horde.set_boards(window, boards + 1);
                self.broadcast_event(&GameEvent::Zombies(protocol::ZombiesEvent::Boards {
                    window: window as u8,
                    boards: boards + 1,
                }));
                if pay > 0 {
                    self.award_points(id, pay);
                }
            }
        }
    }

    /// Sends the round to everyone if it changed since the last time.
    fn broadcast_zombies_round(&mut self) {
        let Some(horde) = &self.horde else {
            return;
        };
        let event = horde.round_event(self.tick);
        let key = |e: &protocol::ZombiesEvent| match *e {
            protocol::ZombiesEvent::Round {
                round,
                phase,
                remaining,
                ..
            } => Some((round, phase, remaining)),
            _ => None,
        };
        if self.zombies_round_sent.as_ref().and_then(key) == key(&event) {
            return;
        }
        self.zombies_round_sent = Some(event);
        self.broadcast_event(&GameEvent::Zombies(event));
    }

    /// A new Zombies round: everyone standing gets two more frags (after
    /// the first), and everyone watching comes back in, as in Black Ops.
    fn on_zombies_round(&mut self, round: u32) {
        godot_print!("server: Zombies round {round}");
        let mut back = Vec::new();
        for avatar in &mut self.avatars {
            avatar.zombies.rebuilt = 0;
            if avatar.predicted.state.downed {
                continue;
            }
            if avatar.predicted.state.alive() {
                if round > 1 {
                    let combat = &mut avatar.predicted.state.combat;
                    combat.lethals = zombies::restock_frags(combat.lethals);
                }
            } else {
                back.push(avatar.id);
            }
        }
        for id in back {
            self.respawn(id, None, true);
            let Some(avatar) = self.avatars.iter_mut().find(|a| a.id == id) else {
                continue;
            };
            let before = avatar.zombies.points;
            avatar.zombies.points = zombies::respawn_points(round, before);
            let change = avatar.zombies.points as i32 - before as i32;
            self.send_points(id, change);
        }
        self.next_scoreboard = self.tick;
    }

    /// A zombie's swing landed on player `id`: the two-hit rule.
    fn zombie_strike(&mut self, id: u8, from: Vec3) {
        let max = self.params.health.max_health;
        let Some(avatar) = self.avatars.iter_mut().find(|a| a.id == id) else {
            return;
        };
        if !avatar.predicted.state.alive() {
            return;
        }
        let state = &mut avatar.predicted.state;
        if state.downed {
            return;
        }
        let damage = zombies::hit_damage(state.health, max);
        // The second hit downs instead of killing (last stand).
        let downs = damage >= state.health;
        let damage = if downs {
            state.health - laststand::DOWNED_HEALTH
        } else {
            damage
        };
        player::apply_damage(state, damage, false, &mut Vec::new());
        let killed = false;
        let travel = avatar.predicted.position - from;
        avatar.hits = avatar.hits.wrapping_add(1);
        avatar.hit_heading = protocol::heading_to_u8(travel.x, travel.z);
        avatar.hit_part =
            protocol::HitPart::new(Some(shooter_sim::hitbox::CHEST_SHAPE as u8), damage, false)
                .to_u8();
        self.send_event(
            id,
            GameEvent::Hurt {
                attacker: u8::MAX,
                from,
                damage,
                killed,
            },
        );
        if downs {
            self.go_down(id);
        }
    }

    /// Zombies: player `id` goes down in last stand: the sidearm alone, a
    /// crawl, 5% of their points lost (the reviver earns them), and 45 s to
    /// be revived.
    fn go_down(&mut self, id: u8) {
        let bleeds_out_at = self.ticks_from_now(laststand::BLEED_OUT_SECONDS);
        let Some(avatar) = self.avatars.iter_mut().find(|a| a.id == id) else {
            return;
        };
        let state = &mut avatar.predicted.state;
        if state.downed || !state.alive() {
            return;
        }
        let stored = state.combat;
        state.downed = true;
        state.health = laststand::DOWNED_HEALTH;
        state.combat = laststand::pistol();
        state.combat.shots_fired = stored.shots_fired;
        let movement = &mut state.movement;
        movement.stance = shooter_sim::movement::Stance::Crouching;
        movement.sprinting = false;
        movement.mounted = false;
        movement.leave_ladder(&self.params.movement);
        let lost = laststand::down_penalty(avatar.zombies.points);
        avatar.zombies.points -= lost;
        avatar.zombies.downs += 1;
        avatar.score.deaths += 1;
        avatar.last_stand = Some(LastStand {
            bleed_left: laststand::BLEED_OUT_SECONDS,
            stored,
            lost,
            reviver: None,
            revived_for: 0.0,
        });
        godot_print!("server: player {id} is down");
        self.send_points(id, -(lost as i32));
        self.broadcast_event(&GameEvent::Zombies(protocol::ZombiesEvent::Downed {
            player: id,
            bleeds_out_at: Some(bleeds_out_at),
        }));
        self.next_scoreboard = self.tick;
    }

    /// Zombies: runs every last stand one tick: revives under way (the
    /// teammates in `revivers` held use beside someone down this tick), and
    /// the bleed-out clock, which stops during a revive.
    fn update_last_stands(&mut self, revivers: &[(u8, u8)]) {
        let tick = self.tick;
        let mut revived = Vec::new();
        let mut bled_out = Vec::new();
        let mut events = Vec::new();
        for avatar in &mut self.avatars {
            let Some(stand) = &mut avatar.last_stand else {
                continue;
            };
            let helpers: Vec<u8> = revivers
                .iter()
                .filter(|(_, target)| *target == avatar.id)
                .map(|(by, _)| *by)
                .collect();
            let by = stand
                .reviver
                .filter(|r| helpers.contains(r))
                .or_else(|| helpers.first().copied());
            match by {
                Some(by) => {
                    if stand.reviver != Some(by) {
                        stand.reviver = Some(by);
                        stand.revived_for = 0.0;
                        let left = (laststand::REVIVE_SECONDS * TICK_RATE as f32).ceil() as Tick;
                        events.push(protocol::ZombiesEvent::Revive {
                            player: avatar.id,
                            by: Some(by),
                            done_at: tick + left,
                        });
                    }
                    stand.revived_for += TICK_DT;
                    if stand.revived_for >= laststand::REVIVE_SECONDS {
                        revived.push((avatar.id, by));
                    }
                }
                None => {
                    if stand.reviver.take().is_some() {
                        stand.revived_for = 0.0;
                        events.push(protocol::ZombiesEvent::Revive {
                            player: avatar.id,
                            by: None,
                            done_at: tick,
                        });
                    }
                    stand.bleed_left -= TICK_DT;
                    if stand.bleed_left <= 0.0 {
                        bled_out.push(avatar.id);
                    }
                }
            }
        }
        for event in events {
            self.broadcast_event(&GameEvent::Zombies(event));
        }
        for (id, by) in revived {
            self.revive(id, by);
        }
        for id in bled_out {
            self.bleed_out(id);
        }
    }

    /// Player `by` brought player `id` back up: their weapons back, full
    /// health, a moment before zombies notice them, and `by` earns the
    /// points `id` lost going down.
    fn revive(&mut self, id: u8, by: u8) {
        let ignore = self.ticks_from_now(laststand::REVIVED_IGNORE_SECONDS);
        let max = self.params.health.max_health;
        let Some(avatar) = self.avatars.iter_mut().find(|a| a.id == id) else {
            return;
        };
        let Some(stand) = avatar.last_stand.take() else {
            return;
        };
        let state = &mut avatar.predicted.state;
        let shots = state.combat.shots_fired;
        state.downed = false;
        state.health = max;
        state.since_damage = f32::MAX;
        state.combat = stand.stored.raised();
        state.combat.shots_fired = shots;
        state.movement.stance = shooter_sim::movement::Stance::Standing;
        avatar.zombies.ignored_until = ignore;
        godot_print!("server: player {by} revived player {id}");
        if let Some(reviver) = self.avatars.iter_mut().find(|a| a.id == by) {
            reviver.zombies.revives += 1;
            reviver.zombies.points = reviver.zombies.points.saturating_add(stand.lost);
        }
        self.send_points(by, stand.lost as i32);
        self.broadcast_event(&GameEvent::Zombies(protocol::ZombiesEvent::Downed {
            player: id,
            bleeds_out_at: None,
        }));
        self.next_scoreboard = self.tick;
    }

    /// Player `id` bled out: dead until the next round, and every other
    /// player loses 10% of their points.
    fn bleed_out(&mut self, id: u8) {
        let Some(avatar) = self.avatars.iter_mut().find(|a| a.id == id) else {
            return;
        };
        avatar.last_stand = None;
        avatar.predicted.state.downed = false;
        avatar.predicted.state.health = 0.0;
        godot_print!("server: player {id} bled out");
        let mut fined = Vec::new();
        for other in &mut self.avatars {
            if other.id != id {
                let lost = laststand::no_revive_penalty(other.zombies.points);
                other.zombies.points -= lost;
                fined.push((other.id, lost));
            }
        }
        for (other, lost) in fined {
            self.send_points(other, -(lost as i32));
        }
        self.broadcast_event(&GameEvent::Zombies(protocol::ZombiesEvent::Downed {
            player: id,
            bleeds_out_at: None,
        }));
        self.next_scoreboard = self.tick;
    }

    /// Tells player `id` their points, which just changed by `change`.
    fn send_points(&mut self, id: u8, change: i32) {
        let Some(avatar) = self.avatars.iter().find(|a| a.id == id) else {
            return;
        };
        let total = avatar.zombies.points;
        self.send_event(
            id,
            GameEvent::Zombies(protocol::ZombiesEvent::Points { total, change }),
        );
    }

    /// Pays player `id` for hurting zombies, doubled during Double Points.
    fn award_points(&mut self, id: u8, points: u32) {
        let points = if self.horde.as_ref().is_some_and(|h| h.double_points()) {
            points * 2
        } else {
            points
        };
        let Some(avatar) = self.avatars.iter_mut().find(|a| a.id == id) else {
            return;
        };
        avatar.zombies.points = avatar.zombies.points.saturating_add(points);
        self.send_points(id, points as i32);
    }

    /// Applies one attack's hits on zombies: damage, the attacker's hit
    /// markers, and points.
    fn hurt_zombies(&mut self, attacker: u8, shot: u32, hits: &[TargetHit], melee: bool) {
        for hit in hits {
            let Some(horde) = &mut self.horde else {
                return;
            };
            let Some(hurt) = horde.hurt(hit.id, hit.damage, hit.direction, Some(hit.part), melee)
            else {
                continue;
            };
            if let Some(avatar) = self.avatars.iter_mut().find(|a| a.id == attacker)
                && hurt.killed
            {
                avatar.zombies.kills += 1;
                if hit.headshot {
                    avatar.zombies.headshots += 1;
                }
                avatar.score.kills += 1;
            }
            self.send_event(
                attacker,
                GameEvent::Zombies(protocol::ZombiesEvent::Hit {
                    zombie: hit.id,
                    shot,
                    headshot: hit.headshot,
                    killed: hurt.killed,
                }),
            );
            let points = zombies::points::for_hit(hurt.killed, hit.headshot, melee);
            self.award_points(attacker, points);
            if hurt.killed {
                self.next_scoreboard = self.tick;
            }
        }
    }

    /// The tick zombies were drawn at for a shot checked at `rewind`.
    fn zombie_rewind(&self, rewind: f64) -> f64 {
        rewind - protocol::zombie_lag(self.snapshot_interval.clamp(1, 255) as u8)
    }

    /// Zombies: a frag's blast hurts every zombie it reaches.
    fn blast_zombies(
        &mut self,
        space: &mut Gd<PhysicsDirectSpaceState3D>,
        at: Vec3,
        kind: equipment::Kind,
        owner: Option<u8>,
        shot: u32,
    ) {
        let Some(horde) = &self.horde else {
            return;
        };
        if !kind.lethal() {
            return;
        }
        let radius = equipment::blast_radius(kind);
        let center = to_godot(at);
        let hits: Vec<TargetHit> = horde
            .near(at, radius)
            .into_iter()
            .filter(|(_, feet)| grenades::blast_reaches(space, center, to_godot(*feet)))
            .filter_map(|(id, feet)| {
                let chest = feet + Vec3::Y * EXPLOSION_TARGET_HEIGHT;
                let distance = chest.distance(at);
                let damage = equipment::blast_damage(kind, distance);
                (damage > 0.0).then(|| TargetHit {
                    id,
                    damage,
                    headshot: false,
                    distance,
                    direction: (feet - at).normalize_or_zero(),
                    part: shooter_sim::hitbox::CHEST_SHAPE as u8,
                })
            })
            .collect();
        if let Some(owner) = owner {
            self.hurt_zombies(owner, shot, &hits, false);
        } else if let Some(horde) = &mut self.horde {
            for hit in hits {
                horde.hurt(hit.id, hit.damage, hit.direction, Some(hit.part), false);
            }
        }
    }

    /// Adds this tick's throws, flies every grenade, and sets off the ones
    /// whose fuse ran out.
    fn update_grenades(&mut self, throws: Vec<Throw>) {
        if throws.is_empty() && self.grenades.live.is_empty() {
            return;
        }
        let Some(mut space) = self
            .base()
            .get_world_3d()
            .and_then(|world| world.get_direct_space_state())
        else {
            return;
        };
        for throw in throws {
            if throw.dropped {
                self.grenades.add(LiveGrenade {
                    owner: Some(throw.owner),
                    shot: throw.shot,
                    grenade: equipment::Grenade::dropped(
                        throw.kind,
                        to_sim(throw.eye),
                        throw.carrier,
                        throw.fuse,
                    ),
                });
            } else {
                self.grenades.throw(
                    &mut space,
                    Some(throw.owner),
                    throw.shot,
                    throw.kind,
                    throw.eye,
                    (throw.yaw, throw.pitch),
                    throw.carrier,
                    throw.fuse,
                );
            }
        }
        let outcome = self.grenades.step(&mut space, TICK_DT);
        for blast in outcome.blasts {
            self.resolve_grenade(&mut space, &blast);
        }
    }

    /// A grenade goes off: everyone sees it, and every body the blast can
    /// see within its radius takes damage by distance. No friendly fire, but
    /// a thrower's own grenade hurts them. Kills count as the thrower's.
    fn resolve_grenade(&mut self, space: &mut Gd<PhysicsDirectSpaceState3D>, blast: &LiveGrenade) {
        let kind = blast.grenade.kind;
        let at = blast.grenade.position;
        self.broadcast_event(&GameEvent::Blast {
            owner: blast.owner,
            shot: blast.shot,
            kind,
            at,
        });
        match kind {
            equipment::Kind::Flash => self.flash_bots(space, at),
            equipment::Kind::Smoke => self.smokes.push(HangingSmoke {
                owner: blast.owner,
                shot: blast.shot,
                at,
                popped: self.tick,
            }),
            equipment::Kind::Frag => {}
        }
        if !self.attacks_hurt() {
            return;
        }
        self.blast_zombies(space, at, kind, blast.owner, blast.shot);
        // Zombies: frags hurt only zombies, so a player is never killed
        // outside the last-stand rules.
        if self.horde.is_some() {
            return;
        }
        let attacker = blast
            .owner
            .filter(|id| self.avatars.iter().any(|a| a.id == *id));
        let attacker_side = attacker.and_then(|id| {
            self.avatars
                .iter()
                .find(|a| a.id == id)
                .map(|a| self.side(a))
        });
        let center = to_godot(at);
        let radius = equipment::blast_radius(kind);
        let mut victims: Vec<(u8, f32, f32)> = Vec::new();
        for avatar in &self.avatars {
            if !avatar.predicted.state.alive() {
                continue;
            }
            let own = Some(avatar.id) == attacker;
            if !own && attacker_side.is_some_and(|side| side == self.side(avatar)) {
                continue;
            }
            let feet = to_godot(avatar.predicted.position);
            let chest = feet + Vector3::UP * EXPLOSION_TARGET_HEIGHT;
            let distance = center.distance_to(chest);
            if distance > radius || !grenades::blast_reaches(space, center, feet) {
                continue;
            }
            let damage = equipment::blast_damage(kind, distance);
            if damage > 0.0 {
                victims.push((avatar.id, distance, damage));
            }
        }
        // The weapon the thrower holds names nothing in the feed; the grenade does.
        let weapon = attacker
            .and_then(|id| self.avatars.iter().find(|a| a.id == id))
            .map_or_else(default_primary, |a| a.predicted.state.combat.weapon().id);
        let attacker_team =
            attacker.and_then(|id| self.avatars.iter().find(|a| a.id == id).map(|a| a.team));
        let scoring = self.match_state.scoring();
        let mut kills = Vec::new();
        for (id, distance, damage) in victims {
            let Some(victim) = self.avatars.iter_mut().find(|a| a.id == id) else {
                continue;
            };
            let killed =
                player::apply_damage(&mut victim.predicted.state, damage, false, &mut Vec::new());
            let travel = victim.predicted.position - at;
            victim.hits = victim.hits.wrapping_add(1);
            victim.hit_heading = protocol::heading_to_u8(travel.x, travel.z);
            victim.hit_part =
                protocol::HitPart::new(Some(shooter_sim::hitbox::CHEST_SHAPE as u8), damage, false)
                    .to_u8();
            if let Some(attacker) = attacker
                && let Some(brain) = &mut victim.brain
            {
                brain.on_hurt(attacker, center, self.tick);
            }
            self.send_event(
                id,
                GameEvent::Hurt {
                    attacker: attacker.unwrap_or(u8::MAX),
                    from: at,
                    damage,
                    killed,
                },
            );
            if let Some(attacker) = attacker.filter(|a| *a != id) {
                self.send_event(
                    attacker,
                    GameEvent::Hit {
                        victim: id,
                        shot: blast.shot,
                        damage,
                        headshot: false,
                        killed,
                    },
                );
            }
            if !killed {
                continue;
            }
            let credited = attacker.filter(|attacker| *attacker != id);
            self.broadcast_event(&GameEvent::Kill {
                killer: credited.unwrap_or(id),
                victim: id,
                weapon,
                headshot: false,
                cause: protocol::KillCause::Equipment(kind),
            });
            self.note_kill(KillRecord {
                killer: credited.unwrap_or(id),
                victim: id,
                tick: self.tick,
                rewind: 0,
                weapon,
                cause: protocol::KillCause::Equipment(kind),
                headshot: false,
            });
            self.record_death(id);
            if credited.is_some() {
                kills.push(Kill {
                    distance,
                    headshot: false,
                });
            } else {
                self.on_suicide(id);
            }
            let victim_team = self.avatars.iter().find(|a| a.id == id).map(|a| a.team);
            let end = match (self.game, credited) {
                (Mode::GunGame(rules), Some(killer)) => self.climb(killer, &rules),
                (Mode::FreeForAll, Some(killer)) => self.free_for_all_end(killer, kills.len()),
                (Mode::TeamDeathmatch, _) => victim_team.and_then(|victim_team| {
                    self.match_state
                        .on_kill(&self.rules, credited.and(attacker_team), victim_team)
                }),
                _ => None,
            };
            if let Some(end) = end {
                self.on_transition(end);
            } else {
                self.broadcast_match();
            }
        }
        if !kills.is_empty() && scoring {
            let rules = self.score_rules;
            if let Some(attacker) = attacker
                && let Some(thrower) = self.avatars.iter_mut().find(|a| a.id == attacker)
            {
                let awards = thrower.score.on_shot_kills(&rules, &kills);
                let totals = thrower.score.totals();
                self.send_event(attacker, GameEvent::Score { awards, totals });
            }
        }
    }

    /// A flashbang at `at` blinds every bot whose eyes can see it, by
    /// distance and how squarely it faces it, as it does players
    /// (`FpsPlayer::flash_me`). One ray per bot in range.
    fn flash_bots(&mut self, space: &mut Gd<PhysicsDirectSpaceState3D>, at: Vec3) {
        let flash = to_godot(at) + Vector3::UP * equipment::BLAST_LIFT;
        let movement = self.params.movement;
        let tick = self.tick;
        for avatar in &mut self.avatars {
            let Some(brain) = &mut avatar.brain else {
                continue;
            };
            if !avatar.predicted.state.alive() {
                continue;
            }
            let eye = to_godot(avatar.predicted.position)
                + Vector3::UP * avatar.predicted.state.movement.eye_height(&movement);
            let to = flash - eye;
            if to.length() >= equipment::FLASH_RANGE || !grenades::clear_line(space, eye, flash) {
                continue;
            }
            let view = brain.view();
            let forward = to_godot(shooter_sim::look::view_direction(view.x, view.y));
            let amount = equipment::flash_amount(to.length(), forward.dot(to.normalized_or_zero()));
            if amount > 0.02 {
                brain.on_flashed(amount, tick);
            }
        }
    }

    /// Whether attacks hurt anyone right now: not while a match counts down
    /// or shows its results, nor outside a round's live play.
    fn attacks_hurt(&self) -> bool {
        !matches!(
            self.match_state.phase,
            Phase::Countdown | Phase::Ended { .. }
        ) && !self.round.is_some_and(|r| !r.live())
    }

    /// Checks one knife strike against the other players as the attacker
    /// saw them: the nearest enemy in reach in front of them, with no wall
    /// between. A strike that connects kills.
    fn resolve_melee(&mut self, strike: &Melee) {
        let Some(attacker) = self.avatars.iter().find(|a| a.id == strike.attacker) else {
            return;
        };
        if attacker.bot && attacker.brain.is_none() && !self.bots_deal_damage {
            return;
        }
        if !self.attacks_hurt() {
            return;
        }
        let side = self.side(attacker);
        let Some(mut space) = attacker
            .parts
            .body
            .get_world_3d()
            .and_then(|world| world.get_direct_space_state())
        else {
            return;
        };
        let rewind = self
            .lag
            .rewind_target_for(self.tick, strike.view_tick, strike.rtt);
        if let Some(horde) = &self.horde {
            // Zombies: the knife goes for the nearest zombie in front.
            let candidates = horde.melee_candidates(self.zombie_rewind(rewind));
            let Some((zombie, feet)) = melee::pick(
                strike.feet,
                strike.yaw,
                melee::REACH + melee::SERVER_SLACK,
                melee::STRIKE_CONE_DEGREES,
                candidates,
            ) else {
                return;
            };
            let chest = to_godot(feet) + Vector3::UP * melee::CHEST_HEIGHT;
            if world_hit(&mut space, strike.eye, chest).is_some() {
                return;
            }
            let hit = TargetHit {
                id: zombie,
                damage: melee::DAMAGE,
                headshot: false,
                distance: strike.eye.distance_to(chest),
                direction: (feet - strike.feet).normalize_or_zero(),
                part: shooter_sim::hitbox::CHEST_SHAPE as u8,
            };
            self.hurt_zombies(strike.attacker, strike.shot, &[hit], true);
            return;
        }
        let candidates: Vec<(u8, Vec3)> = self
            .avatars
            .iter()
            .filter(|a| {
                a.id != strike.attacker
                    && self.side(a) != side
                    && a.predicted.state.alive()
                    && f64::from(a.spawned_at) <= rewind
            })
            .filter_map(|a| {
                let sample = self.lag.sample(a.id, rewind)?;
                sample.alive.then_some((a.id, sample.position))
            })
            .collect();
        let Some((victim, feet)) = melee::pick(
            strike.feet,
            strike.yaw,
            melee::REACH + melee::SERVER_SLACK,
            melee::STRIKE_CONE_DEGREES,
            candidates,
        ) else {
            return;
        };
        // No knifing through walls: the eye must see the victim's chest.
        let chest = to_godot(feet) + Vector3::UP * melee::CHEST_HEIGHT;
        if world_hit(&mut space, strike.eye, chest).is_some() {
            return;
        }
        let hit = TargetHit {
            id: victim,
            damage: melee::DAMAGE,
            headshot: false,
            distance: strike.eye.distance_to(chest),
            direction: (feet - strike.feet).normalize_or_zero(),
            part: shooter_sim::hitbox::CHEST_SHAPE as u8,
        };
        let attack = Attack {
            attacker: strike.attacker,
            eye: strike.eye,
            weapon: strike.weapon,
            shot: strike.shot,
            melee: true,
            rewind: rewind_ticks(self.tick, rewind),
        };
        self.apply_hits(&attack, &[hit]);
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
        // Nobody gets hurt while a match counts down or shows its results,
        // or outside a round's live play.
        if matches!(
            self.match_state.phase,
            Phase::Countdown | Phase::Ended { .. }
        ) || self.round.is_some_and(|r| !r.live())
        {
            return;
        }
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
        // Maps without props skip the metadata walk entirely.
        let track_props = self.props.is_active();
        let mut prop_hits: Vec<PropHit> = Vec::new();
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
                    // Props are hit in the present like walls; whether the
                    // pellet reached one is decided below, once player hits
                    // are known.
                    if track_props && let Some(id) = prop_id_of(hit.collider.as_ref()) {
                        let distance = shot.eye.distance_to(hit.position);
                        prop_hits.push(PropHit {
                            id,
                            distance,
                            direction,
                            damage: def.damage_at(distance) * hit.damage_scale,
                        });
                    }
                }
                pellet
            })
            .collect();

        let rewind = self
            .lag
            .rewind_target_for(self.tick, shot.view_tick, shot.rtt);
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

        if self.horde.is_some() {
            let zombie_tick = self.zombie_rewind(rewind);
            let zombie_targets = self.horde.as_ref().map(|h| h.targets(zombie_tick));
            let zombie_hits = lag::resolve_shot(def, &pellets, &zombie_targets.unwrap_or_default());
            self.hurt_zombies(shot.shooter, shot.shot, &zombie_hits, false);
        }
        let hits = lag::resolve_shot(def, &pellets, &targets);
        let attack = Attack {
            attacker: shot.shooter,
            eye: shot.eye,
            weapon: shot.weapon,
            shot: shot.shot,
            melee: false,
            rewind: rewind_ticks(self.tick, rewind),
        };
        self.apply_hits(&attack, &hits);
        self.damage_props(shot.shooter, shot.weapon, &prop_hits, &hits);
    }

    /// Applies the hits decided for one attack (a shot's pellets or a knife
    /// strike): damage, hit and kill events, the mode's rules, and score.
    fn apply_hits(&mut self, attack: &Attack, hits: &[TargetHit]) {
        let Some(attacker) = self.avatars.iter().find(|a| a.id == attack.attacker) else {
            return;
        };
        let shooter_team = attacker.team;
        let origin = to_sim(attack.eye);
        // Whether this attack scores, decided before its kills can end the match.
        let scoring = self.match_state.scoring();
        let mut kills = Vec::new();
        let mut carrier_kills = 0;
        let mut objective_kills = Vec::new();
        for hit in hits {
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
                brain.on_hurt(attack.attacker, attack.eye, self.tick);
            }
            let travel = victim.predicted.position - origin;
            victim.hits = victim.hits.wrapping_add(1);
            victim.hit_heading = protocol::heading_to_u8(travel.x, travel.z);
            victim.hit_part =
                protocol::HitPart::new(Some(hit.part), hit.damage, attack.melee).to_u8();
            self.send_event(
                attack.attacker,
                GameEvent::Hit {
                    victim: hit.id,
                    shot: attack.shot,
                    damage: hit.damage,
                    headshot: hit.headshot,
                    killed,
                },
            );
            self.send_event(
                hit.id,
                GameEvent::Hurt {
                    attacker: attack.attacker,
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
                let cause = if attack.melee {
                    protocol::KillCause::Knife
                } else {
                    protocol::KillCause::Weapon
                };
                self.broadcast_event(&GameEvent::Kill {
                    killer: attack.attacker,
                    victim: hit.id,
                    weapon: attack.weapon,
                    headshot: hit.headshot,
                    cause,
                });
                self.note_kill(KillRecord {
                    killer: attack.attacker,
                    victim: hit.id,
                    tick: self.tick,
                    rewind: attack.rewind,
                    weapon: attack.weapon,
                    cause,
                    headshot: hit.headshot,
                });
                self.record_death(hit.id);
                let victim = self
                    .avatars
                    .iter()
                    .find(|a| a.id == hit.id)
                    .map(|a| (a.team, a.predicted.position));
                if let Some((_, victim_feet)) = victim
                    && let Some(zone) = self.zones.get(self.hardpoint.active)
                    && let Some(shooter_feet) = self
                        .avatars
                        .iter()
                        .find(|a| a.id == attack.attacker)
                        .map(|a| a.predicted.position)
                    && let Some(award) = self.hardpoint.objective_kill(
                        &self.hardpoint_rules,
                        zone,
                        shooter_team,
                        shooter_feet,
                        victim_feet,
                    )
                {
                    objective_kills.push(award);
                }
                if let Some((_, victim_feet)) = victim
                    && !self.dom_points.is_empty()
                    && let Some(shooter_feet) = self
                        .avatars
                        .iter()
                        .find(|a| a.id == attack.attacker)
                        .map(|a| a.predicted.position)
                    && let Some(award) = self.dom.objective_kill(
                        &self.dom_rules,
                        &self.dom_points,
                        shooter_team,
                        shooter_feet,
                        victim_feet,
                    )
                {
                    objective_kills.push(award);
                }
                let victim_team = victim.map(|(team, _)| team);
                let end = match self.game {
                    // A knife kill sets the victim back a weapon instead
                    // (the classic humiliation rule).
                    Mode::GunGame(_) if attack.melee => {
                        self.on_humiliated(hit.id);
                        None
                    }
                    Mode::GunGame(rules) => self.climb(attack.attacker, &rules),
                    // Search and Destroy scores rounds, not kills (`update_round`).
                    Mode::SearchAndDestroy => None,
                    Mode::TeamDeathmatch => victim_team.and_then(|victim_team| {
                        self.match_state
                            .on_kill(&self.rules, Some(shooter_team), victim_team)
                    }),
                    // Only captures score for the team.
                    Mode::CaptureTheFlag => None,
                    // Only holding the zone (or the flags) scores for the team.
                    Mode::Hardpoint | Mode::Domination => None,
                    Mode::FreeForAll => self.free_for_all_end(attack.attacker, kills.len()),
                    // Players never hurt each other in Zombies.
                    Mode::Zombies => None,
                };
                if let Some(end) = end {
                    self.on_transition(end);
                } else {
                    self.broadcast_match();
                }
            }
        }
        // Points only count in a match, not in the lobby's warm-up. The kill
        // that ends a match still counts.
        if !kills.is_empty() && scoring {
            let rules = self.score_rules;
            let Some(shooter) = self.avatars.iter_mut().find(|a| a.id == attack.attacker) else {
                return;
            };
            let mut awards = shooter.score.on_shot_kills(&rules, &kills);
            for _ in 0..carrier_kills {
                awards.extend(shooter.score.on_flag_play(&rules, Award::CarrierKill));
            }
            let (zone_points, flag_points) = (
                self.hardpoint_rules.objective_kill_points,
                self.dom_rules.objective_kill_points,
            );
            awards.extend(objective_kills.into_iter().map(|award| {
                let points = match award {
                    Award::DominationDefend | Award::DominationAssault => flag_points,
                    _ => zone_points,
                };
                shooter.score.award(award, points)
            }));
            let totals = shooter.score.totals();
            self.send_event(attack.attacker, GameEvent::Score { awards, totals });
        }
    }

    /// Damages the props this shot struck, once player hits are known. A
    /// pellet that hit a player closer than the prop never reached it. The
    /// manager owns prop health and stages; what it queues is broadcast on
    /// the next tick.
    fn damage_props(
        &self,
        attacker: u8,
        weapon: WeaponId,
        prop_hits: &[PropHit],
        target_hits: &[lag::TargetHit],
    ) {
        if prop_hits.is_empty() || !self.props.is_active() {
            return;
        }
        // Conservative: if anyone was hit closer than a prop, the prop is
        // behind cover from the shooter's point of view on that pellet.
        let nearest_player = target_hits
            .iter()
            .map(|hit| hit.distance)
            .fold(f32::INFINITY, f32::min);
        let mut totals: Vec<(u16, f32, Vec3)> = Vec::new();
        for hit in prop_hits {
            if hit.distance >= nearest_player || hit.damage <= 0.0 {
                continue;
            }
            match totals.iter_mut().find(|(id, _, _)| *id == hit.id) {
                Some((_, damage, impulse)) => {
                    *damage += hit.damage;
                    *impulse += hit.direction * hit.damage;
                }
                None => totals.push((hit.id, hit.damage, hit.direction * hit.damage)),
            }
        }
        for (id, damage, impulse) in totals {
            self.props
                .damage_prop(id, damage, impulse, attacker, weapon);
        }
    }

    /// Drains the props manager once a tick: population, changed prop states
    /// for every client, and explosions to apply to players.
    fn update_props(&mut self) {
        if !self.props.is_active() {
            if self.props_retry > 0 {
                self.props_retry -= 1;
                return;
            }
            self.props_retry = 2 * TICK_RATE;
            // The map manager may have been added after the map loaded (or a
            // map without props may stay that way).
            if let Some(map) = self.base().get_child(0) {
                self.props = PropsBridge::discover_under(&map);
                // A replacement manager needs the population again.
                self.props_population = None;
            }
            if !self.props.is_active() {
                return;
            }
        }
        let population = self.avatars.len().min(i32::MAX as usize) as i32;
        if self.props_population != Some(population) {
            self.props.set_population(population);
            self.props_population = Some(population);
        }
        for state in self.props.drain_events() {
            self.broadcast_event(&prop_event(&state));
        }
        for blast in self.props.drain_explosions() {
            self.resolve_explosion(&blast, protocol::KillCause::Weapon);
        }
    }

    /// Resets every prop and tells clients only the ones that changed. Called
    /// when a match starts and when the server falls back to the lobby, so a
    /// fresh match never inherits last match's debris.
    fn reset_props(&mut self) {
        if !self.props.is_active() {
            return;
        }
        let before = self.props.snapshot_states();
        self.props.reset();
        // The reset may queue the same changes as events; the diff below is
        // complete, so drop them to avoid sending every change twice.
        let _ = self.props.drain_events();
        for state in self.props.snapshot_states() {
            let previous = before.iter().find(|old| old.id == state.id);
            if previous != Some(&state) {
                self.broadcast_event(&prop_event(&state));
            }
        }
    }

    /// Applies one prop explosion on the server: distance falloff, obstruction
    /// by the map, no friendly fire (but self-damage counts), kill credit, and
    /// the same scoring paths as gunfire. Prop-to-prop damage stays in the
    /// manager's chain reaction, so this never feeds back into it.
    fn resolve_explosion(&mut self, blast: &PropExplosion, cause: protocol::KillCause) {
        if matches!(
            self.match_state.phase,
            Phase::Countdown | Phase::Ended { .. }
        ) {
            return;
        }
        if !(blast.radius.is_finite() && blast.radius > 0.0)
            || !(blast.damage.is_finite() && blast.damage > 0.0)
            || !blast.position.is_finite()
        {
            return;
        }
        let attacker = blast
            .attacker
            .filter(|id| self.avatars.iter().any(|a| a.id == *id));
        let attacker_team =
            attacker.and_then(|id| self.avatars.iter().find(|a| a.id == id).map(|a| a.team));
        let Some(mut space) = self
            .base()
            .get_world_3d()
            .and_then(|world| world.get_direct_space_state())
        else {
            return;
        };
        let origin = to_godot(blast.position);
        let mut victims: Vec<(u8, f32, f32)> = Vec::new();
        for avatar in &self.avatars {
            if !avatar.predicted.state.alive() {
                continue;
            }
            let self_hit = Some(avatar.id) == attacker;
            // No friendly fire, but the player who set it off can hurt
            // themselves.
            if !self_hit && attacker_team.is_some_and(|team| team == avatar.team) {
                continue;
            }
            let chest = to_godot(avatar.predicted.position) + Vector3::UP * EXPLOSION_TARGET_HEIGHT;
            let distance = origin.distance_to(chest);
            if distance > blast.radius {
                continue;
            }
            let blocked = world_hit(&mut space, origin, chest)
                .is_some_and(|hit| origin.distance_to(hit) + BLAST_CLEARANCE < distance);
            if blocked {
                continue;
            }
            let damage =
                blast.damage + (blast.outer_damage - blast.damage) * (distance / blast.radius);
            if damage > 0.0 {
                victims.push((avatar.id, distance, damage));
            }
        }

        let weapon = blast.weapon.unwrap_or_else(default_primary);
        let scoring = self.match_state.scoring();
        let mut kills = Vec::new();
        for (id, distance, damage) in victims {
            let Some(victim) = self.avatars.iter_mut().find(|a| a.id == id) else {
                continue;
            };
            let killed =
                player::apply_damage(&mut victim.predicted.state, damage, false, &mut Vec::new());
            if let Some(attacker) = attacker
                && let Some(brain) = &mut victim.brain
            {
                brain.on_hurt(attacker, to_godot(blast.position), self.tick);
            }
            self.send_event(
                id,
                GameEvent::Hurt {
                    // Blasts with no player behind them use an id no roster
                    // has; the HUD only uses the damage and direction.
                    attacker: attacker.unwrap_or(u8::MAX),
                    from: blast.position,
                    damage,
                    killed,
                },
            );
            if killed {
                let credited = attacker.filter(|attacker| *attacker != id);
                self.broadcast_event(&GameEvent::Kill {
                    killer: credited.unwrap_or(id),
                    victim: id,
                    weapon,
                    headshot: false,
                    cause,
                });
                self.note_kill(KillRecord {
                    killer: credited.unwrap_or(id),
                    victim: id,
                    tick: self.tick,
                    rewind: 0,
                    weapon,
                    cause,
                    headshot: false,
                });
                self.record_death(id);
                if credited.is_some() {
                    kills.push(Kill {
                        distance,
                        headshot: false,
                    });
                }
                let victim_team = self.avatars.iter().find(|a| a.id == id).map(|a| a.team);
                let killer_team = credited.and(attacker_team);
                // Only Team Deathmatch scores kills for the team, as with
                // shots and grenades: Search and Destroy scores rounds,
                // Capture the Flag captures, Hardpoint the zone.
                let end = match (self.game, credited) {
                    (Mode::FreeForAll, Some(killer)) => self.free_for_all_end(killer, kills.len()),
                    (Mode::TeamDeathmatch, _) => victim_team.and_then(|victim_team| {
                        self.match_state
                            .on_kill(&self.rules, killer_team, victim_team)
                    }),
                    _ => None,
                };
                if let Some(end) = end {
                    self.on_transition(end);
                } else {
                    self.broadcast_match();
                }
            }
        }
        // Points only count in a match, and only for a credited attacker. The
        // kill that ends a match still counts.
        if !kills.is_empty() && scoring {
            let rules = self.score_rules;
            if let Some(attacker) = attacker
                && let Some(shooter) = self.avatars.iter_mut().find(|a| a.id == attacker)
            {
                let awards = shooter.score.on_shot_kills(&rules, &kills);
                let totals = shooter.score.totals();
                self.send_event(attacker, GameEvent::Score { awards, totals });
            }
        }
    }

    /// Counts a death and tells the player their new totals.
    /// A kill happened: remembers it for the killcam that ends a round or
    /// the match, and sends the victim the killcam to watch before they
    /// respawn, when the mode gives one (`killcam::respawn_killcam`). Bots
    /// watch nothing.
    fn note_kill(&mut self, record: KillRecord) {
        let phase = self.match_state.phase;
        if self.round.is_some_and(|r| r.live()) {
            self.round_record.on_kill(record.killer, record.victim);
        }
        if phase == Phase::Playing && record.killer != record.victim {
            self.last_kill = Some(record);
        }
        if !killcam::respawn_killcam(self.game, phase, record.killer, record.victim)
            || self.client_of(record.victim).is_none()
        {
            return;
        }
        let killcam = record.killcam(killcam::Kind::Respawn);
        let tick = self.tick;
        if let Some(victim) = self.avatars.iter_mut().find(|a| a.id == record.victim) {
            victim.killcam_since = Some(tick);
            // Held until the killcam is over, unless they skip it.
            victim.respawn_in = victim.respawn_in.max(killcam::respawn_wait(&killcam));
        }
        self.send_event(record.victim, GameEvent::Killcam(killcam));
    }

    /// Sends everyone the killcam of the latest kill to end a round or the
    /// match, if it is recent enough to replay, and keeps the round's end
    /// (or the results) up until it has played.
    fn play_ending_killcam(&mut self, kind: killcam::Kind) {
        let Some(killcam) = killcam::ending_killcam(self.last_kill.take(), kind, self.tick) else {
            return;
        };
        let seconds = killcam::ending_seconds(&killcam);
        if let Some(round) = &mut self.round
            && matches!(round.phase, RoundPhase::Over { .. })
            && kind == killcam::Kind::Round
        {
            round.time_left = round.time_left.max(seconds);
        }
        if kind == killcam::Kind::Final {
            // The results stay up as long as usual once it has played.
            self.match_state.time_left += seconds;
        }
        self.broadcast_event(&GameEvent::Killcam(killcam));
    }

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
                // Back in the lobby, put the map back the way it started.
                self.reset_props();
                self.round = None;
                self.charge = None;
            }
            return;
        }
        let all_ready = self.auto_start || humans.iter().all(|c| c.ready);
        let phase = self.match_state.phase;
        let leader = match self.game {
            Mode::TeamDeathmatch
            | Mode::CaptureTheFlag
            | Mode::Hardpoint
            | Mode::Domination
            | Mode::SearchAndDestroy => self.match_state.team_leader(),
            Mode::GunGame(_) => gun_game::leader(self.avatars.iter().map(|a| (a.id, a.progress))),
            Mode::FreeForAll => {
                tdm::kill_leader(self.avatars.iter().map(|a| (a.id, a.score.kills)))
            }
            Mode::Zombies => None,
        };
        if self.game == Mode::Hardpoint
            && let Some(end) = self.hardpoint_overtime(leader)
        {
            self.on_transition(end);
            return;
        }
        let transition = self
            .match_state
            .tick(&self.rules, TICK_DT, all_ready, leader);
        if let Some(transition) = transition {
            self.on_transition(transition);
        } else if self.match_state.phase != phase {
            self.broadcast_match();
        }
        self.update_round();
    }

    /// Hardpoint overtime: when the clock is about to run out with the team
    /// behind in the live zone, play goes on; once no team that is not ahead
    /// stands in it, the match ends with `leader` the winner. Returns that
    /// end.
    fn hardpoint_overtime(&mut self, leader: Option<u8>) -> Option<Transition> {
        let state = self.match_state;
        if state.phase != Phase::Playing || self.rules.time_limit.is_none() {
            return None;
        }
        let holds = hardpoint::overtime_holds(state.scores, self.hill_presence);
        if state.overtime() {
            if holds {
                return None;
            }
            godot_print!(
                "server: overtime over ({} to {})",
                state.scores[0],
                state.scores[1]
            );
            return Some(self.match_state.finish(&self.rules, leader));
        }
        if state.time_left - TICK_DT > 0.0 || !holds {
            return None;
        }
        godot_print!(
            "server: overtime ({} to {}), the team behind is on the hill",
            state.scores[0],
            state.scores[1]
        );
        self.match_state.start_overtime(None);
        self.broadcast_match();
        None
    }

    /// Runs a Search and Destroy round: its clock, wipe-outs, the round's
    /// point, and the next round's start.
    fn update_round(&mut self) {
        if self.match_state.phase != Phase::Playing {
            return;
        }
        let Some(mut round) = self.round else {
            return;
        };
        let mut teams = [Headcount::default(); tdm::TEAM_COUNT];
        for avatar in &self.avatars {
            let count = &mut teams[usize::from(avatar.team)];
            count.players += 1;
            count.alive += u32::from(avatar.predicted.state.alive());
        }
        if round.live() {
            let alive: Vec<(u8, Team, bool)> = self
                .avatars
                .iter()
                .map(|a| (a.id, a.team, a.predicted.state.alive()))
                .collect();
            self.round_record.on_headcount(&alive);
        }
        let transition = round.tick(&self.snd_rules, TICK_DT, teams);
        self.round = Some(round);
        let Some(transition) = transition else {
            return;
        };
        match transition {
            RoundTransition::Live => {
                let roster = self
                    .avatars
                    .iter()
                    .filter(|a| a.predicted.state.alive())
                    .map(|a| (a.id, a.team))
                    .collect();
                self.round_record = RoundRecord::start(roster);
            }
            RoundTransition::Over { winner, reason } => {
                self.round_over(round.number, winner, reason);
                return;
            }
            RoundTransition::Next { swapped } => {
                self.last_kill = None;
                if swapped {
                    godot_print!("server: halftime, the teams swap sides");
                }
                self.respawn_all_at_start();
                self.new_charge();
                self.next_scoreboard = self.tick;
            }
        }
        self.broadcast_round();
    }

    /// Scores a decided round for `winner` and tells everyone.
    fn round_over(&mut self, number: u32, winner: Team, reason: RoundEnd) {
        godot_print!("server: round {number} to team {} ({reason:?})", winner + 1);
        if reason == RoundEnd::Detonated
            && let Some(charge) = &mut self.charge
        {
            charge.detonate();
            let at = charge.position;
            self.broadcast_charge();
            self.detonate_charge(at);
        }
        for (id, award) in self.round_record.medals(winner) {
            godot_print!("server: player {id} earned {}", award.label());
            self.pay_charge_play(id, award);
        }
        let end = self.match_state.add_point(&self.rules, winner);
        // A round won by wiping the other team out replays its last kill for
        // everyone: the round winning kill, or the final killcam when it
        // also wins the match.
        if reason == RoundEnd::Eliminated {
            let kind = if end.is_some() {
                killcam::Kind::Final
            } else {
                killcam::Kind::Round
            };
            self.play_ending_killcam(kind);
        }
        self.broadcast_round();
        match end {
            Some(end) => self.on_transition(end),
            None => self.broadcast_match(),
        }
    }

    /// Hands the round's charge to a random living attacker (or leaves it
    /// at the attackers' side when there is none), and tells everyone.
    fn new_charge(&mut self) {
        let Some(round) = self.round else {
            self.charge = None;
            return;
        };
        let mut attackers: Vec<(u8, Vec3)> = self
            .avatars
            .iter()
            .filter(|a| a.team == round.attackers && a.predicted.state.alive())
            .map(|a| (a.id, a.predicted.position))
            .collect();
        attackers.sort_unstable_by_key(|(id, _)| *id);
        let carrier = (!attackers.is_empty()).then(|| {
            let pick = rng::hash(round.number, self.tick) as usize % attackers.len();
            attackers[pick]
        });
        let fallback = tdm::start_spawn(&self.spawn_points, 0, 0)
            .and_then(|i| self.spawn_points.get(i))
            .map_or(to_sim(self.spawn), |p| p.position);
        self.charge = Some(Charge::new(carrier, fallback));
        self.charge_planter = None;
        self.round_record = RoundRecord::default();
        self.charge_target = rng::hash(round.number, self.tick.wrapping_add(7)) as usize
            % self.charge_sites.len().max(1);
        if let Some((id, _)) = carrier {
            godot_print!(
                "server: player {id} carries the charge in round {}",
                round.number
            );
        }
        self.broadcast_charge();
    }

    /// Search and Destroy: drops, pickups, plants, and defuses, from what
    /// each player holds this tick.
    fn update_charge(&mut self) {
        let Some(mut charge) = self.charge else {
            return;
        };
        let Some(round) = self.round else {
            return;
        };
        let players: Vec<ChargePlayer> = self
            .avatars
            .iter()
            .map(|a| charge_player(a, a.interact))
            .collect();
        let cx = ChargeContext {
            rules: &self.snd_rules,
            sites: &self.charge_sites,
            attackers: round.attackers,
            live: self.match_state.phase == Phase::Playing && round.live(),
        };
        let before = charge_key(&charge);
        let mut events = Vec::new();
        charge.tick(&cx, TICK_DT, &players, &mut events);
        self.charge = Some(charge);
        for event in events {
            match event {
                ChargeEvent::Dropped { by, at } => {
                    let floor = self.floor_below(to_godot(at) + Vector3::UP * 0.5);
                    if let Some(charge) = &mut self.charge {
                        charge.settle(to_sim(floor));
                    }
                    godot_print!("server: player {by} dropped the charge");
                }
                ChargeEvent::PickedUp { by } => {
                    godot_print!("server: player {by} picked up the charge");
                }
                ChargeEvent::Started { .. } => self.charge_use_started = self.tick,
                ChargeEvent::Stopped { .. } => {}
                ChargeEvent::Planted { by, site, .. } => {
                    self.charge_planter = Some(by);
                    self.pay_charge_play(by, Award::ChargePlanted);
                    godot_print!(
                        "server: player {by} planted the charge at site {}",
                        self.charge_site_names
                            .get(usize::from(site))
                            .map_or("?", String::as_str)
                    );
                    if let Some(round) = &mut self.round {
                        round.plant(&self.snd_rules);
                    }
                    self.broadcast_round();
                }
                ChargeEvent::Defused { by, .. } => {
                    godot_print!("server: player {by} defused the charge");
                    self.pay_charge_play(by, Award::ChargeDefused);
                    // A defuse with an attacker still alive to stop it.
                    if self
                        .avatars
                        .iter()
                        .any(|a| a.team == round.attackers && a.predicted.state.alive())
                    {
                        self.pay_charge_play(by, Award::WireCutter);
                    }
                    let decided = self
                        .round
                        .as_mut()
                        .and_then(|round| round.defuse(&self.snd_rules));
                    if let (Some(RoundTransition::Over { winner, reason }), Some(round)) =
                        (decided, self.round)
                    {
                        self.round_over(round.number, winner, reason);
                    }
                }
            }
        }
        if self.charge.is_some_and(|c| charge_key(&c) != before) {
            self.broadcast_charge();
        }
    }

    /// The charge as player `id` may know it: defenders are not told who
    /// carries it, and only a player's own team hears of a plant or defuse
    /// under way.
    fn charge_event_for(&self, id: u8) -> Option<GameEvent> {
        let charge = self.charge?;
        let round = self.round?;
        let team = self.avatars.iter().find(|a| a.id == id)?.team;
        let attacking = team == round.attackers;
        let status = match charge.state {
            ChargeState::Carried { by } => ChargeStatus::Carried {
                by: attacking.then_some(by),
            },
            ChargeState::Dropped { at } => ChargeStatus::Dropped { at },
            ChargeState::Planted { site, at } => ChargeStatus::Planted { site, at },
            ChargeState::Detonated { site, at } => ChargeStatus::Detonated { site, at },
            ChargeState::Defused { site, at } => ChargeStatus::Defused { site, at },
        };
        let using = charge.using.and_then(|u| {
            let user_team = self.avatars.iter().find(|a| a.id == u.by)?.team;
            (user_team == team).then(|| UseView {
                by: u.by,
                kind: u.kind,
                started_at: self.charge_use_started,
                done_at: self.charge_use_started
                    + (u.kind.duration(&self.snd_rules) * TICK_RATE as f32).round() as Tick,
            })
        });
        Some(GameEvent::Charge(ChargeView { status, using }))
    }

    fn broadcast_charge(&mut self) {
        let ids: Vec<u8> = self.clients.iter().filter_map(|c| c.player).collect();
        for id in ids {
            if let Some(event) = self.charge_event_for(id) {
                self.send_event(id, event);
            }
        }
    }

    /// Search and Destroy: the map's charge sites, for a client that just
    /// joined.
    fn charge_sites_event(&self) -> GameEvent {
        GameEvent::ChargeSites {
            sites: self
                .charge_sites
                .iter()
                .zip(&self.charge_site_names)
                .map(|(site, name)| ZoneView {
                    name: name.clone(),
                    center: site.center,
                    half_extents: site.half_extents,
                })
                .collect(),
        }
    }

    fn on_transition(&mut self, transition: Transition) {
        match transition {
            Transition::Started => {
                godot_print!("server: match started");
                if self.horde.is_some() {
                    self.reset_zombies();
                }
                self.last_kill = None;
                self.reset_flags();
                if self.game == Mode::SearchAndDestroy {
                    self.round = Some(RoundState::first(&self.snd_rules));
                    let sites = self.charge_sites_event();
                    self.broadcast_event(&sites);
                }
                // Everyone starts fresh on their own side, in join order;
                // without teams, each somewhere away from the others.
                self.hardpoint = HardpointState::new(&self.hardpoint_rules);
                self.dom = DominationState::new(self.dom_points.len());
                for avatar in &mut self.avatars {
                    avatar.score = ScoreState::default();
                    avatar.progress = Progress::default();
                    avatar.hill = HillTime::default();
                }
                self.respawn_all_at_start();
                self.new_charge();
                self.broadcast_round();
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
                self.reset_props();
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
                // The kill that ended the match, for everyone (Search and
                // Destroy plays its own when the last round is won; Zombies
                // ends on the horde).
                if !matches!(self.game, Mode::SearchAndDestroy | Mode::Zombies) {
                    self.play_ending_killcam(killcam::Kind::Final);
                }
                self.reset_flags();
            }
            Transition::NextCountdown => {
                self.round = None;
                self.charge = None;
            }
        }
        self.broadcast_match();
        if !self.zones.is_empty() {
            let event = self.hardpoint_event();
            self.broadcast_event(&event);
        }
        if !self.dom_points.is_empty() {
            let event = self.domination_event();
            self.broadcast_event(&event);
        }
        self.next_scoreboard = self.tick;
    }

    /// Domination: counts who stands on each flag, runs the captures, pays
    /// capture awards, scores each team a point per owned flag every few
    /// seconds, and swaps sides at halftime. Flags change hands in the lobby
    /// too, for practice, but nothing scores there; they stop through the
    /// countdown and the results.
    fn update_domination(&mut self) {
        if self.dom_points.is_empty() {
            return;
        }
        let phase = self.match_state.phase;
        if !matches!(phase, Phase::Lobby | Phase::Playing) {
            return;
        }
        let scoring = self.match_state.scoring();
        if scoring
            && domination::halftime_due(
                self.match_state.time_left,
                self.rules.time_limit,
                self.dom.swapped,
            )
        {
            godot_print!(
                "server: halftime ({} to {}), the teams swap sides",
                self.match_state.scores[0],
                self.match_state.scores[1]
            );
            self.dom.halftime();
            // The match HUD calls the second half out ("HALFTIME").
            self.match_state.period = Period::SecondHalf;
            self.broadcast_match();
            self.respawn_all_at_start();
            let event = self.domination_event();
            self.broadcast_event(&event);
            return;
        }
        // Which flag each living player stands on, if any.
        let on_flag: Vec<Option<usize>> = self
            .avatars
            .iter()
            .map(|a| {
                let feet = a.predicted.position;
                a.predicted
                    .state
                    .alive()
                    .then(|| self.dom_points.iter().position(|p| p.contains(feet)))
                    .flatten()
            })
            .collect();
        let mut presence = vec![[0u32; tdm::TEAM_COUNT]; self.dom_points.len()];
        for (avatar, flag) in self.avatars.iter().zip(&on_flag) {
            if let Some(flag) = flag {
                presence[*flag][usize::from(avatar.team.min(1))] += 1;
            }
        }
        let step = self.dom.tick(&self.dom_rules, TICK_DT, &presence, scoring);
        for capture in &step.captures {
            godot_print!(
                "server: team {} captured {}",
                capture.team + 1,
                self.dom_points[capture.flag].name
            );
        }
        if scoring && !step.captures.is_empty() {
            let points = self.dom_rules.capture_points;
            let mut payouts: Vec<(u8, Vec<Scored>, ScoreTotals)> = Vec::new();
            for (avatar, flag) in self.avatars.iter_mut().zip(&on_flag) {
                let took = step
                    .captures
                    .iter()
                    .any(|c| Some(c.flag) == *flag && c.team == avatar.team);
                if took {
                    let award = avatar.score.award(Award::DominationCapture, points);
                    payouts.push((avatar.id, vec![award], avatar.score.totals()));
                }
            }
            for (id, awards, totals) in payouts {
                self.send_event(id, GameEvent::Score { awards, totals });
            }
            self.next_scoreboard = self.tick;
        }
        if step.changed {
            let event = self.domination_event();
            self.broadcast_event(&event);
        }
        let mut ended = None;
        for (team, points) in step.points.iter().enumerate() {
            let end = self
                .match_state
                .add_points(&self.rules, team as Team, *points);
            ended = ended.or(end);
        }
        if let Some(end) = ended {
            self.on_transition(end);
        } else if step.points.iter().any(|p| *p > 0) {
            self.broadcast_match();
        }
    }

    /// Domination: the map's flags, for a client that just joined.
    fn dom_flags_event(&self) -> GameEvent {
        GameEvent::DominationFlags {
            flags: self
                .dom_points
                .iter()
                .map(|p| FlagPointView {
                    name: p.name.clone(),
                    center: p.center,
                    radius: p.radius,
                    half_height: p.half_height,
                })
                .collect(),
        }
    }

    /// Domination: who owns each flag and the captures under way.
    fn domination_event(&self) -> GameEvent {
        let running = matches!(self.match_state.phase, Phase::Lobby | Phase::Playing);
        GameEvent::Domination(DominationView {
            tick: self.tick,
            swapped: self.dom.swapped,
            flags: self
                .dom
                .flags
                .iter()
                .map(|f| FlagPointStatus {
                    owner: f.owner,
                    claim: f.claim,
                    progress: (f.fraction(&self.dom_rules) * 255.0).round() as u8,
                    contested: f.contested,
                    // Captures stand still while the clock is stopped.
                    capturers: if running {
                        f.capturers.min(255) as u8
                    } else {
                        0
                    },
                })
                .collect(),
        })
    }

    /// Hardpoint: counts who is in the live zone, scores the team holding
    /// it, pays capture and secure awards, and moves the zone on time. The
    /// zone rotates in the lobby too, for practice, but nothing scores
    /// there; it stops through the countdown and the results.
    fn update_hardpoint(&mut self) {
        let Some(zone) = self.zones.get(self.hardpoint.active).cloned() else {
            return;
        };
        let phase = self.match_state.phase;
        if !matches!(phase, Phase::Lobby | Phase::Playing) {
            return;
        }
        let scoring = self.match_state.scoring();
        let mut presence = [0u32; tdm::TEAM_COUNT];
        let inside: Vec<bool> = self
            .avatars
            .iter()
            .map(|a| a.predicted.state.alive() && zone.contains(a.predicted.position))
            .collect();
        for (avatar, inside) in self.avatars.iter().zip(&inside) {
            if *inside {
                presence[usize::from(avatar.team.min(1))] += 1;
            }
        }
        self.hill_presence = presence;
        let step = self
            .hardpoint
            .tick(&self.hardpoint_rules, self.zones.len(), TICK_DT, presence);

        if scoring {
            let holder = step.captured.or(self.hardpoint.holder.team());
            let rules = self.hardpoint_rules;
            let mut payouts: Vec<(u8, Vec<Scored>)> = Vec::new();
            for (avatar, inside) in self.avatars.iter_mut().zip(&inside) {
                let holding = *inside && holder == Some(avatar.team);
                let mut awards = Vec::new();
                if holding && step.captured == Some(avatar.team) {
                    awards.push(
                        avatar
                            .score
                            .award(Award::HardpointCapture, rules.capture_points),
                    );
                }
                if let Some(points) = avatar.hill.tick(&rules, TICK_DT, *inside, holding) {
                    awards.push(avatar.score.award(Award::HardpointSecure, points));
                }
                if !awards.is_empty() {
                    payouts.push((avatar.id, awards));
                }
            }
            for (id, awards) in payouts {
                let Some(totals) = self
                    .avatars
                    .iter()
                    .find(|a| a.id == id)
                    .map(|a| a.score.totals())
                else {
                    continue;
                };
                self.send_event(id, GameEvent::Score { awards, totals });
            }
            let mut ended = None;
            for (team, points) in step.points.iter().enumerate() {
                if *points == 0 {
                    continue;
                }
                let end = self
                    .match_state
                    .add_points(&self.rules, team as Team, *points);
                ended = ended.or(end);
            }
            if step.points.iter().any(|p| *p > 0) && ended.is_none() {
                self.broadcast_match();
            }
            if step.holder_changed || step.moved {
                let event = self.hardpoint_event();
                self.broadcast_event(&event);
            }
            if let Some(end) = ended {
                self.on_transition(end);
            }
            return;
        }
        if step.holder_changed || step.moved {
            let event = self.hardpoint_event();
            self.broadcast_event(&event);
        }
    }

    /// Hardpoint: the map's zones, for a client that just joined.
    fn zones_event(&self) -> GameEvent {
        GameEvent::HardpointZones {
            zones: self
                .zones
                .iter()
                .map(|zone| ZoneView {
                    name: zone.name.clone(),
                    center: zone.center,
                    half_extents: zone.half_extents,
                })
                .collect(),
        }
    }

    /// Hardpoint: the live zone, who has it, and when it moves.
    fn hardpoint_event(&self) -> GameEvent {
        let running = matches!(self.match_state.phase, Phase::Lobby | Phase::Playing);
        let moves_at = running.then(|| {
            self.tick + (self.hardpoint.time_left.max(0.0) * TICK_RATE as f32).round() as Tick
        });
        GameEvent::Hardpoint {
            active: self.hardpoint.active.min(usize::from(u8::MAX)) as u8,
            holder: if running {
                self.hardpoint.holder
            } else {
                Holder::Neutral
            },
            moves_at,
        }
    }

    fn match_event(&self) -> GameEvent {
        let state = &self.match_state;
        let ends_at = state
            .has_clock(&self.rules)
            .then(|| self.ticks_from_now(state.time_left));
        GameEvent::Match {
            phase: state.phase,
            ends_at,
            scores: state.scores,
            period: state.period,
            score_limit: match self.game {
                Mode::TeamDeathmatch
                | Mode::CaptureTheFlag
                | Mode::Hardpoint
                | Mode::Domination
                | Mode::SearchAndDestroy
                | Mode::FreeForAll
                | Mode::Zombies => self.rules.score_limit,
                Mode::GunGame(_) => u32::from(gun_game::LEVELS),
            },
        }
    }

    fn broadcast_match(&mut self) {
        let event = self.match_event();
        self.broadcast_event(&event);
    }

    /// The server tick `seconds` from now.
    fn ticks_from_now(&self, seconds: f32) -> Tick {
        self.tick + (seconds.max(0.0) * TICK_RATE as f32).round() as Tick
    }

    fn round_event(&self) -> Option<GameEvent> {
        let round = self.round?;
        Some(GameEvent::Round {
            number: round.number,
            phase: round.phase,
            ends_at: self.ticks_from_now(round.time_left),
            attackers: round.attackers,
            halftime_after: self.snd_rules.halftime_after(),
        })
    }

    fn broadcast_round(&mut self) {
        if let Some(event) = self.round_event() {
            self.broadcast_event(&event);
        }
    }

    /// Sends everyone every player's score, kills, deaths, ping, and readiness.
    fn broadcast_scoreboard(&mut self) {
        self.next_scoreboard = self.tick + SCOREBOARD_INTERVAL;
        let rows: Vec<ScoreRow> = self
            .avatars
            .iter()
            .map(|a| {
                let client = self.client_of(a.id);
                // Zombies: points, zombies killed, and times down.
                let zombies = self.horde.is_some();
                ScoreRow {
                    id: a.id,
                    score: if zombies {
                        a.zombies.points
                    } else {
                        a.score.score
                    },
                    kills: if zombies {
                        a.zombies.kills
                    } else {
                        a.score.kills
                    },
                    deaths: if zombies {
                        a.zombies.downs
                    } else {
                        a.score.deaths
                    },
                    ping_ms: client.map_or(0, |c| {
                        transport::round_trip_ms(&c.peer).clamp(0.0, 9999.0) as u16
                    }),
                    ready: client.is_none_or(|c| c.ready),
                    level: a.progress.level,
                    level_kills: a.progress.kills,
                    captures: a.score.captures,
                    returns: a.score.returns,
                    hill_seconds: a.hill.whole_seconds(),
                    defends: a.score.defends,
                    alive: a.predicted.state.alive(),
                    revives: a.zombies.revives,
                    headshots: a.zombies.headshots,
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
    /// Free-for-all: ends the match if `killer` reached the kill limit with
    /// `pending` kills not yet in their score (they are added after the shot
    /// or blast resolves).
    fn free_for_all_end(&mut self, killer: u8, pending: usize) -> Option<Transition> {
        if !self.match_state.scoring() {
            return None;
        }
        let kills = self
            .avatars
            .iter()
            .find(|a| a.id == killer)
            .map_or(0, |a| a.score.kills)
            + pending as u32;
        (kills >= self.rules.score_limit)
            .then(|| self.match_state.finish(&self.rules, Some(killer)))
    }

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
    /// Gun Game: a player was knifed and drops a weapon, as for a suicide.
    fn on_humiliated(&mut self, id: u8) {
        self.on_suicide(id);
    }

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

    /// Search and Destroy: scores a plant, defuse, or round medal for a
    /// player and tells them.
    fn pay_charge_play(&mut self, id: u8, award: Award) {
        let rules = self.score_rules;
        let Some(avatar) = self.avatars.iter_mut().find(|a| a.id == id) else {
            return;
        };
        let Some(scored) = avatar.score.on_charge_play(&rules, award) else {
            return;
        };
        let totals = avatar.score.totals();
        self.send_event(
            id,
            GameEvent::Score {
                awards: vec![scored],
                totals,
            },
        );
        self.next_scoreboard = self.tick;
    }

    /// The charge goes off at `at`: everyone sees the blast, and it hurts
    /// everyone in reach as Call of Duty's does, credited to the planter.
    fn detonate_charge(&mut self, at: Vec3) {
        self.broadcast_event(&GameEvent::Blast {
            owner: None,
            shot: 0,
            kind: equipment::Kind::Frag,
            at: at + Vec3::Y * 0.2,
        });
        let planter = self
            .charge_planter
            .filter(|id| self.avatars.iter().any(|a| a.id == *id));
        let blast = PropExplosion {
            position: at + Vec3::Y * 0.3,
            radius: CHARGE_BLAST_RADIUS,
            damage: CHARGE_BLAST_DAMAGE,
            outer_damage: CHARGE_BLAST_EDGE_DAMAGE,
            attacker: planter,
            weapon: None,
        };
        self.resolve_explosion(&blast, protocol::KillCause::Charge);
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
        if let (Some(round), Some(charge)) = (self.round, self.charge) {
            return self.charge_objectives(round, &charge);
        }
        if let Some(zone) = self.zones.get(self.hardpoint.active) {
            return self.zone_objectives(zone);
        }
        if !self.dom_points.is_empty() {
            return self.dom_objectives();
        }
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

    /// Search and Destroy: what each bot does about the charge.
    ///
    /// - A bot standing where holding interact would plant or defuse holds
    ///   it (and fights first if an enemy shows).
    /// - Attackers: the carrier runs to this round's target site; the
    ///   others push into that site to clear it. A dropped charge sends the
    ///   nearest attacker bot to pick it up. Once planted, they hold spots
    ///   around it.
    /// - Defenders: split between the sites, holding spots in them. Once the
    ///   charge is planted, the nearest runs to defuse it and the rest clear
    ///   the site around it.
    fn charge_objectives(
        &self,
        round: RoundState,
        charge: &Charge,
    ) -> Vec<(u8, Option<BotObjective>)> {
        let Some(target) = self
            .charge_sites
            .get(self.charge_target % self.charge_sites.len().max(1))
        else {
            return Vec::new();
        };
        let cx = ChargeContext {
            rules: &self.snd_rules,
            sites: &self.charge_sites,
            attackers: round.attackers,
            live: round.live(),
        };
        let period = self.tick / (ZONE_SPOT_SECONDS * TICK_RATE);
        // A spot within `reach` of `center` (on the floor), changing every
        // few seconds.
        let spot = |id: u8, center: Vec3, reach: Vec2| {
            let a = rng::unit(u32::from(id), period * 2);
            let b = rng::unit(u32::from(id), period * 2 + 1);
            to_godot(Vec3::new(
                center.x + (a * 2.0 - 1.0) * reach.x,
                center.y + 0.3,
                center.z + (b * 2.0 - 1.0) * reach.y,
            ))
        };
        let floor = |site: &Site| site.center - Vec3::new(0.0, site.half_extents.y, 0.0);
        let inside = |site: &Site| {
            Vec2::new(
                (site.half_extents.x - 1.0).max(0.3),
                (site.half_extents.z - 1.0).max(0.3),
            )
        };
        let bots: Vec<&Avatar> = self
            .avatars
            .iter()
            .filter(|a| a.brain.is_some() && a.predicted.state.alive())
            .collect();
        // The bot of `team` nearest `at`, sent to it.
        let nearest = |team: Team, at: Vec3| {
            bots.iter()
                .filter(|a| a.team == team)
                .min_by(|a, b| {
                    a.predicted
                        .position
                        .distance_squared(at)
                        .total_cmp(&b.predicted.position.distance_squared(at))
                })
                .map(|a| a.id)
        };
        let defenders: Vec<u8> = {
            let mut ids: Vec<u8> = bots
                .iter()
                .filter(|a| a.team != round.attackers)
                .map(|a| a.id)
                .collect();
            ids.sort_unstable();
            ids
        };
        let fetcher = match charge.state {
            ChargeState::Dropped { at } => nearest(round.attackers, at),
            ChargeState::Planted { at, .. } => nearest(1 - round.attackers, at),
            _ => None,
        };
        let mut out = Vec::new();
        for bot in &bots {
            let id = bot.id;
            if charge.spot(&cx, &charge_player(bot, true)).is_some() {
                out.push((
                    id,
                    Some(BotObjective::use_here(to_godot(bot.predicted.position))),
                ));
                continue;
            }
            let objective = if bot.team == round.attackers {
                match charge.state {
                    ChargeState::Carried { by } if by == id => BotObjective::run(to_godot(
                        target.center - Vec3::Y * (target.half_extents.y - 0.3),
                    )),
                    ChargeState::Dropped { at } if fetcher == Some(id) => {
                        BotObjective::run(to_godot(at))
                    }
                    ChargeState::Planted { at, .. } => {
                        BotObjective::hold(spot(id, at, Vec2::splat(4.0)))
                    }
                    _ => BotObjective::hunt(spot(id, floor(target), inside(target))),
                }
            } else {
                match charge.state {
                    ChargeState::Planted { at, .. } if fetcher == Some(id) => {
                        BotObjective::run(to_godot(at))
                    }
                    ChargeState::Planted { at, .. } => {
                        BotObjective::hunt(spot(id, at, Vec2::splat(4.0)))
                    }
                    _ => {
                        let i = defenders.iter().position(|d| *d == id).unwrap_or(0);
                        let site = &self.charge_sites[i % self.charge_sites.len()];
                        BotObjective::hold(spot(id, floor(site), inside(site)))
                    }
                }
            };
            out.push((id, Some(objective)));
        }
        out
    }

    /// Hardpoint: three of every four bots on a team hold spots inside the
    /// live zone, each moving to a new spot every few seconds; the rest hunt
    /// around its edges for enemies on their way in.
    fn zone_objectives(&self, zone: &Zone) -> Vec<(u8, Option<BotObjective>)> {
        let period = self.tick / (ZONE_SPOT_SECONDS * TICK_RATE);
        let mut out = Vec::new();
        for team in 0..tdm::TEAM_COUNT as Team {
            let mut bots: Vec<u8> = self
                .avatars
                .iter()
                .filter(|a| a.team == team && a.brain.is_some())
                .map(|a| a.id)
                .collect();
            bots.sort_unstable();
            for (i, id) in bots.into_iter().enumerate() {
                let a = rng::unit(u32::from(id), period * 2);
                let b = rng::unit(u32::from(id), period * 2 + 1);
                let floor = zone.floor() + 0.3;
                let objective = if i % 4 == 3 {
                    // A spot just outside the zone, on a random side.
                    let angle = a * std::f32::consts::TAU;
                    let reach = zone.half_extents.x.max(zone.half_extents.z) + 6.0;
                    BotObjective::hunt(to_godot(Vec3::new(
                        zone.center.x + angle.cos() * reach,
                        floor,
                        zone.center.z + angle.sin() * reach,
                    )))
                } else {
                    let reach_x = (zone.half_extents.x - 1.0).max(0.3);
                    let reach_z = (zone.half_extents.z - 1.0).max(0.3);
                    BotObjective::hold(to_godot(Vec3::new(
                        zone.center.x + (a * 2.0 - 1.0) * reach_x,
                        floor,
                        zone.center.z + (b * 2.0 - 1.0) * reach_z,
                    )))
                };
                out.push((id, Some(objective)));
            }
        }
        out
    }

    /// Domination: each team's bots take the flags they do not own, guard
    /// the ones they do, and run back to stop an enemy capture
    /// (`domination::bot_goals`). Near its flag a bot holds a spot inside the
    /// circle, moving to a new one every few seconds, so it captures or
    /// contests; further off it hunts its way there.
    fn dom_objectives(&self) -> Vec<(u8, Option<BotObjective>)> {
        let period = self.tick / (ZONE_SPOT_SECONDS * TICK_RATE);
        let mut out = Vec::new();
        for team in 0..tdm::TEAM_COUNT as Team {
            let bots: Vec<(u8, Vec3)> = self
                .avatars
                .iter()
                .filter(|a| a.team == team && a.brain.is_some())
                .map(|a| (a.id, a.predicted.position))
                .collect();
            for (id, goal) in domination::bot_goals(&self.dom_points, &self.dom, team, &bots) {
                let flag = match goal {
                    BotGoal::Capture(i) | BotGoal::Defend(i) => i,
                };
                let Some(point) = self.dom_points.get(flag) else {
                    continue;
                };
                let feet = bots
                    .iter()
                    .find(|(b, _)| *b == id)
                    .map_or(point.center, |(_, f)| *f);
                let across = Vec2::new(feet.x - point.center.x, feet.z - point.center.z).length();
                let floor = point.floor() + 0.3;
                let objective = if across <= point.radius + DOM_HOLD_REACH {
                    let angle = rng::unit(u32::from(id), period * 2) * std::f32::consts::TAU;
                    let reach = rng::unit(u32::from(id), period * 2 + 1).sqrt()
                        * (point.radius - 0.8).max(0.3);
                    BotObjective::hold(to_godot(Vec3::new(
                        point.center.x + angle.cos() * reach,
                        floor,
                        point.center.z + angle.sin() * reach,
                    )))
                } else {
                    BotObjective::hunt(to_godot(Vec3::new(point.center.x, floor, point.center.z)))
                };
                out.push((id, Some(objective)));
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
        let using = self.charge.and_then(|c| c.using);
        let mut views: Vec<PlayerView> = self
            .avatars
            .iter()
            .map(|a| {
                let charge_use = using.filter(|u| u.by == a.id);
                PlayerView {
                    hits: a.hits,
                    hit_heading: a.hit_heading,
                    hit_part: a.hit_part,
                    charge_use: protocol::throw_to_u8(
                        charge_use.map(|u| u.held / u.kind.duration(&self.snd_rules)),
                    ),
                    defusing: charge_use.is_some_and(|u| u.kind == UseKind::Defuse),
                    ..PlayerView::from_state(a.id, &a.predicted, a.yaw, a.pitch)
                }
                .with_progress(a.clock.progress())
            })
            .collect();
        views.sort_by_key(|v| v.id);
        let grenade_views: Vec<protocol::GrenadeView> = self
            .grenades
            .live
            .iter()
            .filter_map(|g| {
                Some(protocol::GrenadeView::new(
                    g.owner?,
                    g.shot,
                    g.grenade.kind,
                    g.grenade.position,
                    g.grenade.resting,
                ))
            })
            .collect();
        // The newest clouds, if there are ever more than a snapshot holds.
        let skip = self.smokes.len().saturating_sub(protocol::MAX_SMOKES);
        let smoke_views: Vec<protocol::SmokeView> = self.smokes[skip..]
            .iter()
            .map(|smoke| {
                protocol::SmokeView::new(
                    smoke.owner,
                    smoke.shot,
                    smoke.at,
                    self.tick.saturating_sub(smoke.popped),
                )
            })
            .collect();
        self.history.insert(self.tick, views.clone());
        self.lag.record(self.tick, views.clone());
        let tick = self.tick;
        let zombie_views = self.horde.as_mut().and_then(|h| h.views_for(tick));

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
                own: Some(Box::new(OwnState {
                    last_input: client.inputs.last_applied(),
                    predicted: avatar.predicted,
                })),
                players: others,
                grenades: grenade_views.clone(),
                smokes: smoke_views.clone(),
                zombies: zombie_views.clone(),
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
/// What an avatar's animation clocks need of it this tick.
fn body_state(avatar: &Avatar) -> BodyState {
    let p = &avatar.predicted;
    BodyState {
        velocity: p.state.movement.velocity,
        yaw: avatar.yaw,
        stance: p.state.movement.stance,
        sprinting: p.state.movement.sprinting,
        on_floor: p.on_floor,
        alive: p.state.alive(),
        reloading: p.state.combat.is_reloading(),
        weapon: p.state.combat.weapon().id,
        mantle_left: p.state.movement.mantle_time,
        ladder: p
            .state
            .movement
            .on_ladder()
            .then(|| shooter_sim::hitbox::gait::yaw_of(p.state.movement.ladder_facing)),
    }
}

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

/// The damageable prop a collider belongs to, from `prop_id` metadata on it
/// or any of its ancestors.
fn prop_id_of(collider: Option<&Gd<Node>>) -> Option<u16> {
    let mut node = collider.cloned();
    while let Some(current) = node {
        // Reading a missing key logs an error, so check first.
        if current.has_meta("prop_id") {
            return current
                .get_meta("prop_id")
                .try_to::<i32>()
                .ok()
                .and_then(|id| u16::try_from(id).ok());
        }
        node = current.get_parent();
    }
    None
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

/// How far from where a lunge points an enemy may stand, as the attacker
/// saw them, for the lunge to count.
const LUNGE_CHECK_SLACK: f32 = 1.0;

/// True if an enemy stood where `offset` points from `avatar`'s feet, as the
/// attacker drew them at their view tick (`view`: view tick and round trip).
/// A lunge with no enemy at its end would be a free dash, so the server drops
/// it from the input.
fn lunge_backed(
    lag: &LagHistory,
    now: Tick,
    view: (Option<f64>, Option<f32>),
    avatar: &Avatar,
    offset: Vec2,
    fighters: &[Fighter],
) -> bool {
    let rewind = lag.rewind_target_for(now, view.0, view.1);
    let Some(me) = fighters.iter().find(|f| f.id == avatar.id) else {
        return false;
    };
    let aim = avatar.predicted.position + Vec3::new(offset.x, 0.0, offset.y);
    fighters
        .iter()
        .filter(|f| f.id != me.id && f.side != me.side && f.alive)
        .filter(|f| f64::from(f.spawned_at) <= rewind)
        .filter_map(|f| lag.sample(f.id, rewind))
        .filter(|sample| sample.alive)
        .any(|sample| {
            let d = sample.position - aim;
            Vec2::new(d.x, d.z).length() <= LUNGE_CHECK_SLACK && d.y.abs() <= melee::MAX_RISE
        })
}

/// True if the weapon rack the input claims is really under this player's
/// crosshair and within reach, as the client saw it before the move. Without
/// this check, a tampered client could take any weapon from anywhere.
/// Zombies: the teammate down in last stand whom this player, standing,
/// is close enough to revive ([`laststand::REVIVE_RADIUS`]), nearest first.
fn revive_target(avatar: &Avatar, downed: &[(u8, Vec3)]) -> Option<u8> {
    let state = &avatar.predicted.state;
    if !state.alive() || state.downed {
        return None;
    }
    let here = avatar.predicted.position;
    downed
        .iter()
        .filter(|(id, _)| *id != avatar.id)
        .map(|(id, feet)| (*id, feet.distance(here)))
        .filter(|(_, distance)| *distance <= laststand::REVIVE_RADIUS)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

/// The collider the player is looking at within reach (a rack, a door, a
/// window's barrier), if any.
fn looked_at(avatar: &Avatar, input: &PlayerInput, params: &PlayerParams) -> Option<Gd<Node>> {
    let p = &avatar.predicted;
    if !p.state.alive() {
        return None;
    }
    let eye = to_godot(p.position) + Vector3::UP * p.state.movement.eye_height(&params.movement);
    let direction = to_godot(player::aim_direction(input.yaw, input.pitch, Vec2::ZERO));
    let mut space = avatar
        .parts
        .body
        .get_world_3d()
        .and_then(|world| world.get_direct_space_state())?;
    let exclude: Array<Rid> = array![avatar.parts.body.get_rid()];
    let query = PhysicsRayQueryParameters3D::create_ex(eye, eye + direction * PICKUP_REACH)
        .collision_mask(WORLD_LAYER | INTERACT_LAYER)
        .exclude(&exclude)
        .done()?;
    space
        .intersect_ray(&query)
        .get_or_nil("collider")
        .try_to::<Gd<Node>>()
        .ok()
}

/// The price of the rack holding `weapon` that the player is looking at
/// within reach, if they are (0 for a free rack).
fn pickup_in_reach(
    avatar: &Avatar,
    input: &PlayerInput,
    weapon: WeaponId,
    params: &PlayerParams,
) -> Option<i64> {
    let pickup = looked_at(avatar, input, params)?
        .try_cast::<WeaponPickup>()
        .ok()?;
    let pickup = pickup.bind();
    (pickup.weapon_id() == Some(weapon)).then(|| pickup.price())
}

fn joined_event(avatar: &Avatar) -> GameEvent {
    GameEvent::Joined {
        id: avatar.id,
        name: avatar.name.clone(),
        team: avatar.team,
        bot: avatar.bot,
    }
}

/// A prop state as the reliable event clients apply.
fn prop_event(state: &PropSnapshot) -> GameEvent {
    GameEvent::PropState {
        id: state.id,
        stage: state.stage,
        impulse: state.impulse,
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

/// Ticks between `now` and a shot's rewound tick, for its killcam.
fn rewind_ticks(now: Tick, rewound: f64) -> u8 {
    (f64::from(now) - rewound)
        .round()
        .clamp(0.0, f64::from(u8::MAX)) as u8
}

fn default_primary() -> WeaponId {
    WeaponId::expect("m4")
}

/// The two weapon slots a class fills, for `PlayerState::new` (whose
/// combat state is then replaced by the class's own).
fn loadout_for(class: &Class) -> [WeaponId; 2] {
    let [primary, secondary] = class.weapons();
    let fallback = primary.or(secondary).unwrap_or_else(default_primary);
    [primary.unwrap_or(fallback), secondary.unwrap_or(fallback)]
}

/// The map's spawn points: `Node3D` children of a `SpawnPoints` node, each
/// facing the way players should, with an optional `team` metadata (0 or 1;
/// none or -1 for either team).
/// The lanes under the map's `BotLanes` node: each child is a lane, and its
/// children are points (in map space) from one team's side to the other's.
fn map_lanes(map: &Gd<Node>) -> Vec<Vec<Vector3>> {
    let Some(parent) = map
        .get_node_or_null("BotLanes")
        .and_then(|n| n.try_cast::<Node3D>().ok())
    else {
        return Vec::new();
    };
    let base = parent.get_transform();
    parent
        .get_children()
        .iter_shared()
        .filter_map(|lane| lane.try_cast::<Node3D>().ok())
        .map(|lane| {
            let lane_base = base * lane.get_transform();
            lane.get_children()
                .iter_shared()
                .filter_map(|point| point.try_cast::<Node3D>().ok())
                .map(|point| (lane_base * point.get_transform()).origin)
                .collect::<Vec<_>>()
        })
        .filter(|points| points.len() >= 2)
        .collect()
}

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

/// The map's Hardpoint zones, in rotation order: `Node3D` children of a
/// `Hardpoints` node, each a box centred on the node with its full size in a
/// `size` metadata (a `Vector3`, default 10 x 4 x 10 m) and its HUD name in a
/// `label` metadata (default: the node's name).
fn map_zones(map: &Gd<Node>) -> Vec<Zone> {
    let Some(parent) = map
        .get_node_or_null("Hardpoints")
        .and_then(|n| n.try_cast::<Node3D>().ok())
    else {
        return Vec::new();
    };
    let base = parent.get_transform();
    parent
        .get_children()
        .iter_shared()
        .filter_map(|child| child.try_cast::<Node3D>().ok())
        .take(MAX_HARDPOINT_ZONES)
        .map(|point| {
            let transform = base * point.get_transform();
            let size = point
                .get_meta_ex("size")
                .default(&Vector3::new(10.0, 4.0, 10.0).to_variant())
                .done()
                .try_to::<Vector3>()
                .unwrap_or(Vector3::new(10.0, 4.0, 10.0));
            let name = point
                .get_meta_ex("label")
                .default(&point.get_name().to_variant())
                .done()
                .try_to::<GString>()
                .map(|label| label.to_string())
                .unwrap_or_else(|_| point.get_name().to_string());
            Zone {
                name,
                center: to_sim(transform.origin),
                half_extents: to_sim(size.abs() * 0.5),
            }
        })
        .collect()
}

/// The map's Domination flags: `Node3D` children of a `DominationFlags`
/// node, each a circle centred on the node's floor position, with its radius
/// in a `radius` metadata (default `domination::DEFAULT_RADIUS`) and its
/// letter in a `label` metadata (default: A, B, C in order).
fn map_dom_points(map: &Gd<Node>) -> Vec<FlagPoint> {
    let Some(parent) = map
        .get_node_or_null("DominationFlags")
        .and_then(|n| n.try_cast::<Node3D>().ok())
    else {
        return Vec::new();
    };
    let base = parent.get_transform();
    parent
        .get_children()
        .iter_shared()
        .filter_map(|child| child.try_cast::<Node3D>().ok())
        .take(MAX_DOMINATION_FLAGS)
        .enumerate()
        .map(|(i, point)| {
            let transform = base * point.get_transform();
            let radius = point
                .get_meta_ex("radius")
                .default(&domination::DEFAULT_RADIUS.to_variant())
                .done()
                .try_to::<f32>()
                .unwrap_or(domination::DEFAULT_RADIUS);
            let name = point
                .get_meta_ex("label")
                .default(&GString::from(site_letter(i).as_str()).to_variant())
                .done()
                .try_to::<GString>()
                .map_or_else(|_| site_letter(i), |label| label.to_string());
            FlagPoint {
                radius: radius.clamp(1.0, 12.0),
                ..FlagPoint::new(&name, to_sim(transform.origin))
            }
        })
        .collect()
}

/// Three flags for maps without `DominationFlags`: A by side 0's spawns, B
/// in the middle, C by side 1's (`domination::fallback_points`).
fn fallback_dom_points(spawns: &[SpawnPoint]) -> Vec<FlagPoint> {
    let side = |team: Team| {
        let points: Vec<Vec3> = spawns
            .iter()
            .filter(|p| p.team == Some(team))
            .map(|p| p.position)
            .collect();
        (!points.is_empty()).then(|| points.iter().sum::<Vec3>() / points.len() as f32)
    };
    match side(0).zip(side(1)) {
        Some((a, b)) => domination::fallback_points(a, b),
        None => Vec::new(),
    }
}

/// What the charge needs to know about a player.
fn charge_player(avatar: &Avatar, interact: bool) -> ChargePlayer {
    ChargePlayer {
        id: avatar.id,
        team: avatar.team,
        alive: avatar.predicted.state.alive(),
        position: avatar.predicted.position,
        interact,
    }
}

/// What clients are told about the charge, to send them news only when it
/// changes: where it is, and who is planting or defusing.
fn charge_key(charge: &Charge) -> (ChargeState, Option<(u8, UseKind)>) {
    (charge.state, charge.using.map(|u| (u.by, u.kind)))
}

/// The map's charge sites: `Node3D` children of a `ChargeSites` node, each a
/// box centred on the node with its full size in a `size` metadata (default
/// 5 x 3.6 x 5 m) and its name in a `label` metadata (default: A, B, ... in
/// order).
fn map_charge_sites(map: &Gd<Node>) -> Vec<(String, Site)> {
    let Some(parent) = map
        .get_node_or_null("ChargeSites")
        .and_then(|n| n.try_cast::<Node3D>().ok())
    else {
        return Vec::new();
    };
    let base = parent.get_transform();
    parent
        .get_children()
        .iter_shared()
        .filter_map(|child| child.try_cast::<Node3D>().ok())
        .take(MAX_CHARGE_SITES)
        .enumerate()
        .map(|(i, point)| {
            let transform = base * point.get_transform();
            let default_size = Vector3::new(5.0, 3.6, 5.0);
            let size = point
                .get_meta_ex("size")
                .default(&default_size.to_variant())
                .done()
                .try_to::<Vector3>()
                .unwrap_or(default_size);
            let name = point
                .get_meta_ex("label")
                .default(&GString::from(site_letter(i).as_str()).to_variant())
                .done()
                .try_to::<GString>()
                .map_or_else(|_| site_letter(i), |label| label.to_string());
            (
                name,
                Site {
                    center: to_sim(transform.origin),
                    half_extents: to_sim(size.abs() * 0.5),
                },
            )
        })
        .collect()
}

/// A, B, C, ...
fn site_letter(index: usize) -> String {
    char::from(b'A' + (index % 26) as u8).to_string()
}

/// Two sites for maps without `ChargeSites`: on the defenders' side (side 1)
/// of the middle, one to each flank, so the testing area and Nuketown play.
fn fallback_charge_sites(spawns: &[SpawnPoint]) -> Vec<(String, Site)> {
    let side = |team: Team| {
        let points: Vec<Vec3> = spawns
            .iter()
            .filter(|p| p.team == Some(team))
            .map(|p| p.position)
            .collect();
        (!points.is_empty()).then(|| points.iter().sum::<Vec3>() / points.len() as f32)
    };
    let Some((attack, defend)) = side(0).zip(side(1)) else {
        return Vec::new();
    };
    let across = Vec3::new(defend.x - attack.x, 0.0, defend.z - attack.z);
    let length = across.length();
    if length < 1.0 {
        return Vec::new();
    }
    let flank = Vec3::new(-across.z, 0.0, across.x) / length * (length * 0.3).min(14.0);
    let middle = attack.lerp(defend, 0.65);
    [("A", middle + flank), ("B", middle - flank)]
        .into_iter()
        .map(|(name, at)| {
            (
                name.to_owned(),
                Site {
                    center: at + Vec3::new(0.0, 1.5, 0.0),
                    half_extents: Vec3::new(2.5, 1.8, 2.5),
                },
            )
        })
        .collect()
}

/// One zone for maps without `Hardpoints`: halfway between the teams' spawns.
fn fallback_zones(spawns: &[SpawnPoint]) -> Vec<Zone> {
    let side = |team: Team| {
        let points: Vec<Vec3> = spawns
            .iter()
            .filter(|p| p.team == Some(team))
            .map(|p| p.position)
            .collect();
        (!points.is_empty()).then(|| points.iter().sum::<Vec3>() / points.len() as f32)
    };
    let Some(center) = side(0)
        .zip(side(1))
        .map(|(a, b)| (a + b) * 0.5)
        .or_else(|| spawns.first().map(|p| p.position))
    else {
        return Vec::new();
    };
    vec![Zone {
        name: "Midfield".to_owned(),
        center: center + Vec3::new(0.0, 1.5, 0.0),
        half_extents: Vec3::new(5.0, 2.0, 5.0),
    }]
}
