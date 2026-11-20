//! Wire format for every client and server message.
//!
//! The transport (Godot's ENet) only moves bytes; everything about what is in a
//! packet lives here so it can be tested without the engine.
//!
//! - Client → server: a hello, then one [`InputPacket`] per tick. Each carries
//!   every input the server has not acknowledged yet (up to
//!   [`MAX_INPUTS_PER_PACKET`]), so a lost packet costs nothing as long as a later
//!   one arrives.
//! - Server → client: a welcome, then [`Snapshot`]s. A snapshot holds the
//!   receiver's own authoritative state at full precision (prediction compares
//!   it bit for bit) and every other player quantized and delta-compressed
//!   against the last snapshot that client acknowledged.
//! - Server → client, reliable: [`GameEvent`]s (hits, damage, kills, score,
//!   who is playing on which team, the match clock, and the scoreboard),
//!   which happen once and must not be lost.

use glam::{IVec3, Vec2, Vec3};

use super::Tick;
use super::bits::{BitReader, BitSink, BitSource, BitWriter, DecodeError, DecodeResult};
use super::delta::{self, DeltaReader, DeltaWriter};
use super::subtick;
use crate::chat;
use crate::class::{Attachments, Class, Gun, Wildcards};
use crate::domination;
use crate::equipment;
use crate::gun_game;
use crate::hardpoint::Holder;
use crate::hitbox::clock::{self, Progress};
use crate::hitbox::{Activity, HitboxPose};
use crate::killcam::{self, Killcam};
use crate::look::PITCH_LIMIT;
use crate::movement::{MoveState, Stance};
use crate::perk::Perks;
use crate::player::{FireAim, PlayerInput, PlayerState};
use crate::radar;
use crate::score::{Award, ScoreTotals, Scored};
use crate::snd::{RoundEnd, RoundPhase, UseKind};
use crate::stats::{self, Stats};
use crate::streak::{self, Streak};
use crate::tdm::{MAX_NAME_CHARS, Mode, Period, Phase, TEAM_COUNT, Team};
use crate::weapon::{self, Action, CombatState, LoadStage, Raise, WeaponId, WeaponState};
use crate::zombies::drinks::Drinks;

/// Bumped whenever the wire format changes; mismatched clients are rejected.
// Version 5 added Team Deathmatch: names, teams, ready-up, loadouts, the
// match clock, and the scoreboard. Version 6 added burst fire to the
// predicted combat state, and version 7 mantling to the movement state.
// Version 8 added empty weapon slots, version 9 hit flinches for every
// player, version 10 the game mode (Gun Game), player winners, and ladder
// levels, version 11 Capture the Flag: the flags, flag plays and awards,
// and captures and returns on the scoreboard, version 12 Hardpoint: the
// zones, who holds the live one, the objective awards, and time on the hill
// on the scoreboard.
// Version 13 added interactive prop state events (damageable map props).
// Version 14 added text chat.
// Version 15 added Search and Destroy: its mode (the mode field grew to 3
// bits), rounds (event tags grew to 5 bits), and who is alive on the
// scoreboard.
// Version 16 added the server's build name to the welcome.
// Version 17 added Free-for-all.
// Version 18 added hip-fire bloom and each slot's pending bolt cycle.
// Version 19 added the press aim to inputs (sub-tick fire timing).
// Version 20 added melee: the button and lunge in inputs, the strike and
// lunge in the predicted state, who is swinging in player views, and knife
// kills in the kill feed.
// Version 21 changed weapon rules (firing ends a reload once the weapon is
// back up), so older clients would mispredict.
// Version 22 added what each body was doing (sprinting, airborne, mantling,
// reloading, aim, velocity, weapon) to the debug rewound hitbox poses.
// Version 23 added each body's stride phase and reload and mantle progress to
// player views and rewound poses.
// Version 28 added where each body's latest hit landed and how hard
// (`PlayerView::hit_part`), for hit reactions and ragdoll deaths.
// Version 29 added how far each body's aim has turned from its planted feet
// (`PlayerView::turn`) and how far its legs step in place as the feet swing
// round (`PlayerView::step`), which the hit zones' legs follow.
// Version 30 changed how each class of weapon is carried and reloaded
// (`hitbox::carry::CarryClass`), which moves the arm hit zones.
// Version 31 added the frag grenade: the lethal button in inputs, the throw
// and the grenades carried in the predicted state, live grenades in
// snapshots, blasts, and kill causes in the kill feed.
// Version 32 added slides queued in the air and the slide wind-down to the
// movement state.
// Version 33 added the first raise: whether a raise is one, and which
// carried weapons have not been drawn yet.
// Version 34 kept the throw in hand through its follow-through and added how
// far along a throw each body is (`PlayerView::throw`).
// Version 35 added the tactical (flashbang or smoke): its button in inputs,
// the tactical carried in the predicted state, the pick in loadouts, and
// what each body is throwing.
// Version 36 changed Nuketown's collision (the map-edge clip, the non-solid
// shadowcaster doorways, the solid population sign), so older peers would
// mispredict movement there.
// Version 37 added ladders: the ladder a player is on, a slide down it, and
// the regrab delay in the movement state, and the ladder each body faces in
// player views (`PlayerView::ladder`).
// Version 38 added the smoke clouds still hanging to snapshots, so players
// who join after a smoke popped see it.
// Version 39 authored a loop for every direction of travel: the stride
// phase runs forward for backpedals, quicker sideways, and the hit zones pose
// the directional loops, so both ends must agree on them. It also sends knife
// hits with their own force level (`HitPart::KNIFE_FORCE`), so knife-killed
// bodies crumple.
// Version 40 added whether a reload is from an empty magazine
// (`PlayerView::reload_empty`).
// Version 41 moved the support arm's hit zones with a grenade throw
// (`hitbox::action`), and added the throw to the debug rewound poses.
// Version 42 added mounting the weapon on cover (`MoveState::mounted` and
// its yaw, crouch, and blend; `PlayerView::mount`).
// Version 43 added whether a climb is a vault (`PlayerView::vaulting`), so
// other players draw, and the hit zones pose, a vault instead of a mantle.
// Version 44 added killcams: the killcam event (which kill to replay, and
// the ticks around it) and the client's skip.
// Version 45 replaced the loadout pick with a whole class (`crate::class`):
// both weapons and their attachments, grenade counts, and wildcards.
// Version 46 holds a killcam victim's respawn until the client says the
// killcam is over (`ClientMessage::SkipKillcam`, now also sent when it
// finishes), so older clients would wait out the server's fallback.
// Version 47 added Search and Destroy's charge: the interact button held in
// inputs, the charge sites and the charge's whereabouts, plants and defuses
// as events, the two new round ends, and who is planting or defusing in
// player views.
// Version 48 added Search and Destroy's scores and medals (award tags grew
// to 5 bits) and kills by the charge going off.
// Version 49 added Domination: its mode, the flags and their captures,
// halftime, the flag awards, and flag defences on the scoreboard.
// Version 50 added the match period (regulation, second half, or overtime)
// to match events, for Hardpoint overtime.
// Version 51 added Zombies on the dedicated server: the mode, the horde in
// snapshots (`ZombieView`), and the round, zombie hits, points, and windows'
// boards as events (`ZombiesEvent`).
// Version 52 added Zombies' doors, power-ups, and their timers to
// `ZombiesEvent`.
// Version 53 added Zombies' last stand: whether a player is down in the
// predicted state and player views, going down and reviving as events, and
// revives and headshots on the scoreboard.
// Version 54 added perks (`crate::perk`): a class's perks and the Perk Greed
// wildcards, and the perks in the predicted player state.
// Version 55 added whether a player moves quietly (Dead Silence,
// `PlayerView::quiet`).
// Version 56 added Scavenger's packs as events (`ScavengerEvent`).
// Version 57 added Zombies' drinks (`crate::zombies::drinks`) in the
// predicted state, and the power, drink machines, and drinking as events.
// Version 58 added the mystery crate (`ZombiesEvent::Crate`).
// Version 59 added the forge (`ZombiesEvent::Forge`) and upgraded weapons
// (`<key>+forged`) to the weapon table.
// Version 60 added the kneel at Search and Destroy's charge to rewound hitbox
// poses (`Activity::kneel`), so the hit zones follow the plant and defuse.
// Version 61 added the minimap's red dots to snapshots (`RadarView`).
// Version 62 sends the receiver's own state as field-by-field changes from
// the own state in the acknowledged snapshot (`net::delta`).
// Version 63 snaps inputs' view angles to whole steps of 2^-14 radians and
// sends each input after a packet's first as changes from the one before.
// Version 64 sends other players' views field by field through the same
// coder (`net::delta`), positions, angles, and counters as differences, and
// the Zombies horde as changes from the last horde the client acknowledged.
// Version 65 added scorestreaks (`crate::streak`): the loadout and using one
// as client messages, `StreakEvent`, UAV sweep dots, and the Hardline,
// Ghost, and Hard Wired perks (perk sets now take 16 bits).
// Version 66 added the Care Package (`StreakEvent::Crate*`).
// Version 67 added the Airstrike: targets on `UseStreak`,
// `StreakEvent::Airstrike` and `Strike`, and `KillCause::Streak`.
// Version 68 added the CQ-10 and the BP-9 (and their variants) to the
// weapon table, which moves the index of every weapon after them.
// Version 69 added accounts: the hello carries the player's account session
// token, the welcome says whether the server reports progress to the account
// server itself (`ranked`), and `ServerMessage::Denied` says why a server
// turned a player away.
// Version 70 added the KR-12 to the weapon table.
// Version 71 added the DM-7 to the weapon table.
// Version 72 added the SG-12 to the weapon table.
// Version 73 added prone: `Stance::Prone`, the input's prone button, the
// crouch blend's range to 2 (`PlayerView::crouch` and the activity's), and
// the crouch hold in the predicted move state.
// Version 74 added rockets: the RL-4 to the weapon table, and
// `equipment::Kind::Rocket` (code 3) to grenades, blasts, and kill causes.
// Version 75 added `roofed` to `StreakEvent::State`: a map roofed over
// leaves out the streaks that need open sky.
// Version 76 flies the UAVs and Counter-UAVs round a ring over the map
// (`StreakEvent::Sky` carries the ring and each one's id, call tick, and
// phase), lets them be shot down (`StreakEvent::Downed`,
// `Award::ShotDown`), and gives streak events four tag bits.
// Version 77 added `GameEvent::Stats`, a player's match stats.
// Version 78 added the Sentry Gun: `Streak::SentryGun`, and
// `StreakEvent::Turret`, `TurretTarget`, and `TurretGone`.
pub const PROTOCOL_VERSION: u16 = 78;
/// Longest account session token a hello carries, in bytes.
pub const MAX_ACCOUNT_TOKEN_BYTES: usize = 2048;
/// Longest reason a denial carries, in bytes.
pub const MAX_DENIED_BYTES: usize = 160;
/// Longest build name a welcome carries, in bytes.
pub const MAX_BUILD_BYTES: usize = 32;
/// Cap on redundant inputs per packet (200 ms at 120 Hz).
pub const MAX_INPUTS_PER_PACKET: usize = 24;
/// Wish vectors are sent as signed bytes; prediction must use the same values.
const WISH_SCALE: f32 = 127.0;
/// Remote positions are sent in millimetres.
const POSITION_SCALE: f32 = 1000.0;
const ANGLE_STEPS: f32 = 65536.0;
/// View ticks are sent in 1/256 of a tick (about 33 µs at 120 Hz).
const VIEW_TICK_STEPS: f64 = 256.0;
/// View ticks at or past this many ticks do not fit the wire format.
const VIEW_TICK_LIMIT: f64 = 4_294_967_296.0;
/// Cap on events in one reliable message.
pub const MAX_EVENTS_PER_MESSAGE: usize = 64;
/// Cap on players in one [`GameEvent::Rewound`] or [`GameEvent::Scoreboard`].
const MAX_REWOUND_POSES: usize = 32;
const MAX_SCOREBOARD_ROWS: usize = 64;
/// Cap on zones in one [`GameEvent::HardpointZones`].
pub const MAX_HARDPOINT_ZONES: usize = 16;
/// Cap on flags in one [`GameEvent::DominationFlags`] or
/// [`GameEvent::Domination`].
pub const MAX_DOMINATION_FLAGS: usize = domination::MAX_FLAGS;
/// Cap on sites in one [`GameEvent::ChargeSites`].
pub const MAX_CHARGE_SITES: usize = 4;
/// Zone names are at most this many bytes of UTF-8 on the wire.
const MAX_ZONE_NAME_BYTES: usize = 64;

const TAG_BITS: u32 = 3;
/// Client messages past the seventh share tag 7 and add this many bits.
const MORE_TAG_BITS: u32 = 3;
/// Bits for a [`Streak`] on the wire: room for eight.
const STREAK_BITS: u32 = 3;

#[derive(Clone, Debug, PartialEq)]
pub enum ClientMessage {
    Hello {
        version: u16,
        /// The player's name; the server cleans it (`tdm::clean_name`).
        name: String,
        /// The player's account session token (a Nakama session), or empty
        /// when not signed in. A server that checks accounts asks the
        /// account server who it belongs to (docs/online.md).
        account: String,
    },
    Input(InputPacket),
    /// Debug: ask for the hitboxes the server rewinds for each of our shots
    /// ([`GameEvent::Rewound`]). Sent reliably when the F4 overlay toggles.
    Debug {
        rewound_hitboxes: bool,
    },
    /// Ready (or not) to start the match, in the lobby.
    Ready {
        ready: bool,
    },
    /// The class to spawn with from the next spawn on. The server checks it
    /// ([`Class::validate`]) and ignores one that breaks a rule.
    Class(Class),
    /// A chat message, to everyone or only to our team.
    Chat {
        text: String,
        team: bool,
    },
    /// The killcam playing after our death is over, skipped or finished:
    /// respawn as soon as the respawn delay allows (`killcam`).
    SkipKillcam,
    /// The scorestreaks to bring from the next spawn on (`crate::streak`).
    /// The server keeps each streak once and sorts them by cost.
    Streaks(streak::Loadout),
    /// Call in a held `streak` (the first held one of its kind). An
    /// Airstrike carries the points (x, z) marked on the map.
    UseStreak {
        streak: Streak,
        targets: Vec<Vec2>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct InputPacket {
    /// Newest snapshot tick the client has, used as the delta baseline.
    pub ack_snapshot: Option<Tick>,
    /// Sequence number of `inputs[0]`; the rest follow consecutively.
    pub first_seq: u32,
    pub inputs: Vec<PlayerInput>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ServerMessage {
    Welcome {
        player_id: u8,
        tick: Tick,
        /// Ticks between snapshots, which sets the interpolation delay.
        snapshot_interval: u8,
        /// The game the server runs, with its rules.
        mode: Mode,
        /// Which build the server runs (a tag or short commit hash), so
        /// players can tell when it differs from theirs.
        server_build: String,
        /// The server checks accounts and reports each player's match XP to
        /// the account server itself, so the game must not report it too.
        ranked: bool,
    },
    Reject {
        server_version: u16,
    },
    /// The server turned the player away, and why ("Sign in to play on this
    /// server."). Sent before disconnecting.
    Denied {
        reason: String,
    },
    Snapshot(Snapshot),
    /// Things that happened once, sent reliably and in order.
    Events(Vec<GameEvent>),
}

/// A one-off game event. The server decides every hit and kill; clients only
/// present them.
#[derive(Clone, Debug, PartialEq)]
pub enum GameEvent {
    /// To the shooter: their shot number `shot` hit `victim`. A shotgun blast
    /// is one event per victim, with the pellets' damage summed.
    Hit {
        victim: u8,
        shot: u32,
        damage: f32,
        headshot: bool,
        killed: bool,
    },
    /// To the victim: `attacker` (eye at `from`) hit them.
    Hurt {
        attacker: u8,
        from: Vec3,
        damage: f32,
        killed: bool,
    },
    /// To everyone: a kill, for the kill feed.
    Kill {
        killer: u8,
        victim: u8,
        /// The weapon in the killer's hands.
        weapon: WeaponId,
        headshot: bool,
        /// A knife or a grenade shows in the feed instead of the weapon.
        cause: KillCause,
    },
    /// To one player: awards just earned, if any, and their match totals.
    Score {
        awards: Vec<Scored>,
        totals: ScoreTotals,
    },
    /// Debug, to a shooter who asked: every other player's hitbox pose as the
    /// server rewound it to check shot `shot`, at server tick `tick`.
    Rewound {
        shot: u32,
        tick: f64,
        poses: Vec<(u8, HitboxPose)>,
    },
    /// To everyone: a player joined, or changed name or team. A new client
    /// gets one for every player already in the game, itself included.
    Joined {
        id: u8,
        name: String,
        team: Team,
        bot: bool,
    },
    /// To everyone: a player left.
    Left { id: u8 },
    /// To everyone: the match phase or the team scores changed. `ends_at` is
    /// the server tick the phase's clock runs out (none in the lobby, or
    /// while playing a mode without a match clock, as Search and Destroy).
    /// In Gun Game the scores stay 0 and `score_limit` is the number of
    /// weapons. `period`: regulation, the second half, or overtime.
    Match {
        phase: Phase,
        ends_at: Option<Tick>,
        scores: [u32; TEAM_COUNT],
        score_limit: u32,
        period: Period,
    },
    /// To everyone, about once a second: every player's line on the scoreboard.
    Scoreboard { rows: Vec<ScoreRow> },
    /// To one player: they (re)spawned facing `yaw` (radians).
    Spawned { yaw: f32 },
    /// To everyone (Capture the Flag): where both flags are, whenever either
    /// changes hands. Indexed by team.
    Flags { flags: [FlagView; TEAM_COUNT] },
    /// To everyone (Capture the Flag): something happened to team `flag`'s
    /// flag, for the announcer and the feed. `player` did it (none for a flag
    /// that went home by itself).
    FlagPlay {
        flag: Team,
        play: FlagPlay,
        player: Option<u8>,
    },
    /// To everyone (Hardpoint): the map's zones, in the order they go live.
    /// Sent on joining.
    HardpointZones { zones: Vec<ZoneView> },
    /// To everyone (Hardpoint): zone `active` is live until server tick
    /// `moves_at` (none while the clock is stopped), and `holder` has it.
    Hardpoint {
        active: u8,
        holder: Holder,
        moves_at: Option<Tick>,
    },
    /// To everyone: a damageable map prop changed interactive stage, with the
    /// impulse that hit it. The server owns prop health; clients only present
    /// the state and never damage props themselves online.
    PropState {
        /// The prop's id, from its `prop_id` metadata.
        id: u16,
        /// The prop manager's opaque stage (intact, damaged, destroyed, ...).
        stage: u8,
        /// Direction and strength of the hit that changed it.
        impulse: Vec3,
    },
    /// A chat message from player `from`, cleaned by the server. `team`
    /// messages go only to the sender's team.
    Chat { from: u8, text: String, team: bool },
    /// To everyone, in Search and Destroy: the round changed. `ends_at` is
    /// the server tick the phase's clock runs out. The teams swap sides after
    /// round `halftime_after`.
    Round {
        number: u32,
        phase: RoundPhase,
        ends_at: Tick,
        attackers: Team,
        halftime_after: u32,
    },
    /// To everyone (Search and Destroy): the map's charge sites, named A, B,
    /// and so on. Sent on joining.
    ChargeSites { sites: Vec<ZoneView> },
    /// To each player (Search and Destroy), whenever it changes: where the
    /// charge is, as that player may know it.
    Charge(ChargeView),
    /// To everyone: a grenade went off at `at`. `owner` threw it (none once
    /// they have left) as their shot `shot`; clients draw the blast, and
    /// remove the grenade.
    Blast {
        owner: Option<u8>,
        shot: u32,
        kind: equipment::Kind,
        at: Vec3,
    },
    /// A killcam to play: to the victim of a kill, after their death
    /// (`killcam::Kind::Respawn`), or to everyone after the kill that ended
    /// a round or the match. Clients replay it from their own recording of
    /// the snapshots (`killcam::Tape`).
    Killcam(Killcam),
    /// To everyone (Domination): the map's flags, A, B, C. Sent on joining.
    DominationFlags { flags: Vec<FlagPointView> },
    /// To everyone (Domination), whenever a flag changes owner, claim,
    /// contest, or capture speed: every flag as of server tick `tick`.
    /// Clients run each capture on at its rate between events.
    Domination(DominationView),
    /// Zombies: rounds, hits on zombies, points, and windows' boards.
    Zombies(ZombiesEvent),
    /// To everyone: Scavenger's packs dropped, taken, and gone
    /// (`crate::scavenger`). Clients draw them only for a Scavenger player.
    Scavenger(ScavengerEvent),
    /// Scorestreaks (`crate::streak`).
    Streak(StreakEvent),
    /// To one player, every couple of seconds while they change and at the
    /// end of the match: their stats this match (`crate::stats`), which
    /// their game adds to their career and checks challenges against.
    Stats(Stats),
}

/// Scorestreaks' news.
#[derive(Clone, Debug, PartialEq)]
pub enum StreakEvent {
    /// To one player, whenever it changes: their count toward the next
    /// streak this life, the streaks they hold (oldest first), the loadout
    /// they chose, and whether the map is roofed over, which leaves out the
    /// streaks that need open sky ([`Streak::usable`]).
    State {
        progress: u32,
        held: Vec<Streak>,
        loadout: streak::Loadout,
        roofed: bool,
    },
    /// To everyone, whenever it changes and on joining: the ring they fly
    /// over the map, and every UAV and Counter-UAV up.
    Sky {
        orbit: streak::Orbit,
        up: Vec<AirView>,
    },
    /// To everyone, and on joining for each crate still out: a Care
    /// Package is coming (`crate::care_package`). The aircraft lets it go
    /// over `at` at `drop_at`, flying along `heading` (radians), and it
    /// lands at `lands_at`.
    CrateIncoming {
        id: u16,
        owner: u8,
        at: Vec3,
        heading: f32,
        drop_at: Tick,
        lands_at: Tick,
        contents: Streak,
    },
    /// To everyone: player `by` started capturing crate `id`, done at
    /// `done_at` if they keep holding (`by` none: the capture stopped).
    CrateCapture {
        id: u16,
        by: Option<u8>,
        started: Tick,
        done_at: Tick,
    },
    /// To everyone: player `by` took crate `id` and its streak.
    CrateTaken { id: u16, by: u8, contents: Streak },
    /// To everyone: crate `id` ran out.
    CrateGone { id: u16 },
    /// To everyone: player `owner` called an Airstrike: a strike lands on
    /// each point at its tick.
    Airstrike {
        owner: u8,
        strikes: Vec<(Vec3, Tick)>,
    },
    /// To everyone: a strike of `owner`'s Airstrike went off at `at`.
    Strike { owner: u8, at: Vec3 },
    /// To everyone, and on joining for each one up: a Sentry Gun was set
    /// down (`crate::turret`).
    Turret(TurretView),
    /// To everyone: Sentry Gun `id` turned onto player `target` (none: it
    /// has no one).
    TurretTarget { id: u16, target: Option<u8> },
    /// To everyone: Sentry Gun `id` is gone, destroyed by player `by` (none:
    /// it ran out).
    TurretGone { id: u16, by: Option<u8> },
    /// To everyone: player `by` shot down aircraft `id` (`owner`'s
    /// `streak`) at `at`.
    Downed {
        id: u16,
        streak: Streak,
        owner: u8,
        by: u8,
        at: Vec3,
    },
}

/// A Sentry Gun set down: which, whose, on which side, its feet and the way
/// it faces, and when it was set down and runs out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurretView {
    pub id: u16,
    pub owner: u8,
    pub side: u8,
    pub at: Vec3,
    pub facing: f32,
    pub placed: Tick,
    pub until: Tick,
}

/// A UAV or Counter-UAV up: which, whose, on which side, when it was
/// called in and where round the ring it started (`streak::Orbit`), and
/// until when.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AirView {
    pub id: u16,
    pub streak: Streak,
    pub owner: u8,
    pub side: u8,
    pub from: Tick,
    pub phase: u16,
    pub until: Tick,
}

/// A Scavenger pack's comings and goings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScavengerEvent {
    /// Player `dropped_by` died and left pack `id` at `at`.
    Dropped { id: u16, at: Vec3, dropped_by: u8 },
    /// Player `by` took pack `id`.
    Taken { id: u16, by: u8 },
    /// Pack `id` lay too long, or the round ended.
    Gone { id: u16 },
}

/// One Domination flag's circle as clients see it.
#[derive(Clone, Debug, PartialEq)]
pub struct FlagPointView {
    pub name: String,
    /// The circle's middle, `half_height` above its floor.
    pub center: Vec3,
    pub radius: f32,
    pub half_height: f32,
}

/// Every Domination flag ([`GameEvent::Domination`]).
#[derive(Clone, Debug, PartialEq)]
pub struct DominationView {
    /// The server tick this state is from.
    pub tick: Tick,
    pub flags: Vec<FlagPointStatus>,
    /// Halftime has passed: the teams play from each other's sides.
    pub swapped: bool,
}

/// One flag's owner and capture ([`domination::FlagState`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlagPointStatus {
    pub owner: Option<Team>,
    pub claim: Option<Team>,
    /// How far along the claim's capture is, in 255ths.
    pub progress: u8,
    pub contested: bool,
    /// The claiming team's players in the circle (the capture's speed); 0
    /// when stalled.
    pub capturers: u8,
}

/// One flag as clients see it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlagView {
    /// Where its base is.
    pub home: Vec3,
    pub status: FlagStatus,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FlagStatus {
    Home,
    /// Carried by player `by`; clients draw it on that player.
    Carried {
        by: u8,
    },
    /// Lying at `at` until server tick `returns_at`, when it goes home.
    Dropped {
        at: Vec3,
        returns_at: Tick,
    },
}

/// What happened to a flag ([`GameEvent::FlagPlay`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlagPlay {
    Taken,
    Dropped,
    Returned,
    Captured,
}

/// The charge as one player sees it ([`GameEvent::Charge`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChargeView {
    pub status: ChargeStatus,
    /// A plant or defuse under way, sent only to its player's team.
    pub using: Option<UseView>,
}

/// Where the charge is ([`crate::snd::ChargeState`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChargeStatus {
    /// Carried: by `by` for the attackers; defenders do not learn who.
    Carried {
        by: Option<u8>,
    },
    Dropped {
        at: Vec3,
    },
    Planted {
        site: u8,
        at: Vec3,
    },
    Detonated {
        site: u8,
        at: Vec3,
    },
    Defused {
        site: u8,
        at: Vec3,
    },
}

/// A plant or defuse under way: `by` started it at server tick `started_at`,
/// and it finishes at `done_at` if they keep holding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UseView {
    pub by: u8,
    pub kind: UseKind,
    pub started_at: Tick,
    pub done_at: Tick,
}

/// One Hardpoint zone as clients see it: a box around `center`.
#[derive(Clone, Debug, PartialEq)]
pub struct ZoneView {
    pub name: String,
    pub center: Vec3,
    pub half_extents: Vec3,
}

/// One player's line on the scoreboard.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScoreRow {
    pub id: u8,
    pub score: u32,
    pub kills: u32,
    pub deaths: u32,
    /// Round-trip time to the server in milliseconds (0 for bots).
    pub ping_ms: u16,
    /// Ready to start, in the lobby (bots always are).
    pub ready: bool,
    /// Gun Game: the player's place on the weapon ladder, and kills with
    /// that weapon so far (0 in other modes).
    pub level: u8,
    pub level_kills: u8,
    /// Capture the Flag: flags captured and returned.
    pub captures: u32,
    pub returns: u32,
    /// Hardpoint: whole seconds spent in the live zone this match.
    pub hill_seconds: u32,
    /// Domination: kills defending a flag the team owns (its captures are
    /// in `captures`).
    pub defends: u32,
    pub alive: bool,
    /// Zombies: teammates revived, and headshot kills.
    pub revives: u32,
    pub headshots: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub tick: Tick,
    /// Tick of the snapshot the remote players are delta-encoded against.
    pub baseline: Option<Tick>,
    /// The receiving player's authoritative state, if they are in the game.
    /// Boxed: it is the full simulation state, far larger than the rest.
    pub own: Option<Box<OwnState>>,
    /// Every other player, sorted by id.
    pub players: Vec<PlayerView>,
    /// Every live grenade, in full each time (there are few, and they move
    /// every tick).
    pub grenades: Vec<GrenadeView>,
    /// Every smoke cloud still hanging, in full each time, so a player who
    /// joins after one popped still sees it.
    pub smokes: Vec<SmokeView>,
    /// Zombies: the horde, in full, in every other snapshot
    /// ([`ZOMBIE_SEND_TICKS`]); `None` in the snapshots between, and outside
    /// Zombies.
    pub zombies: Option<Vec<ZombieView>>,
    /// The enemies the receiving player's minimap shows, and whether an
    /// enemy Counter-UAV jams it ([`crate::radar`]).
    pub radar: RadarView,
}

/// What a player's minimap shows of the enemy, decided by the server
/// ([`crate::radar::Radar`]): only these dots, never anyone it does not show.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RadarView {
    pub jammed: bool,
    pub blips: Vec<BlipView>,
}

/// A red dot on the minimap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlipView {
    pub id: u8,
    /// Where (x, z) in decimetres.
    pub x: i16,
    pub z: i16,
    /// Ticks since it was revealed.
    pub age: Tick,
    pub kind: radar::BlipKind,
}

/// Blip positions travel in decimetres: a minimap dot needs no more.
const BLIP_SCALE: f32 = 10.0;

impl BlipView {
    pub fn new(blip: &radar::Blip, now: Tick) -> Self {
        let quantize = |v: f32| (v * BLIP_SCALE).round().clamp(-32768.0, 32767.0) as i16;
        Self {
            id: blip.id,
            x: quantize(blip.at.x),
            z: quantize(blip.at.z),
            age: now.saturating_sub(blip.tick),
            kind: blip.kind,
        }
    }

    /// Where (x, z) in metres.
    pub fn position(&self) -> Vec2 {
        Vec2::new(f32::from(self.x), f32::from(self.z)) / BLIP_SCALE
    }
}

/// What a client already has of the snapshot a new one is delta-encoded
/// against (`Snapshot::baseline`): its remote players, and its own state if
/// it was in the game.
#[derive(Clone, Copy, Debug, Default)]
pub struct Baseline<'a> {
    pub players: &'a [PlayerView],
    pub own: Option<&'a Predicted>,
    /// Zombies: the horde of the newest snapshot carrying it that the client
    /// acknowledged, and that snapshot's tick, so the horde goes as changes.
    pub zombies: Option<(Tick, &'a [ZombieView])>,
}

/// Most zombies a snapshot carries.
pub const MAX_ZOMBIES: usize = 64;
/// Zombies are sent at most this often, in ticks (30 Hz at the 120 Hz tick):
/// a horde is many bodies, and they shamble.
pub const ZOMBIE_SEND_TICKS: Tick = 4;
/// Zombie positions travel in centimetres.
const ZOMBIE_POSITION_SCALE: f32 = 100.0;

/// How far behind the other players clients draw zombies, in ticks: one
/// zombie send interval, so a sent pair always brackets the drawn moment.
/// The server rewinds zombies by the same amount behind a shot's view tick.
pub fn zombie_lag(snapshot_interval: u8) -> f64 {
    f64::from(ZOMBIE_SEND_TICKS.max(u32::from(snapshot_interval)))
}

/// One zombie as clients see it ([`crate::zombies`]): quantized, and only
/// what drawing it needs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ZombieView {
    /// Unique among the zombies in play (ids wrap, but never onto a zombie
    /// still shown).
    pub id: u8,
    /// Feet position in centimetres.
    pub position: IVec3,
    /// Facing in 1/256 turns ([`heading_to_u8`] of the way it faces).
    pub yaw: u8,
    pub alive: bool,
    /// Swings started so far, wrapping at 8: an increase plays the attack
    /// (a swing at a player or at a window's boards).
    pub swings: u8,
    /// Hits taken so far, wrapping at 8: an increase plays the flinch, and
    /// the death falls from the latest one.
    pub hits: u8,
    /// The horizontal direction the latest hit travelled (1/256 turns), and
    /// where it landed and how hard ([`HitPart`]).
    pub hit_heading: u8,
    pub hit_part: u8,
}

/// Counters in [`ZombieView`] wrap at this many.
pub const ZOMBIE_COUNTER_BITS: u32 = 3;

impl ZombieView {
    pub fn new(id: u8, position: Vec3, yaw: f32, alive: bool) -> Self {
        Self {
            id,
            position: (position * ZOMBIE_POSITION_SCALE).round().as_ivec3(),
            yaw: yaw_to_u8(yaw),
            alive,
            ..Self::default()
        }
    }

    /// Feet position in metres.
    pub fn position(&self) -> Vec3 {
        self.position.as_vec3() / ZOMBIE_POSITION_SCALE
    }

    /// Facing in radians (0 faces -Z).
    pub fn yaw(&self) -> f32 {
        f32::from(self.yaw) / 256.0 * std::f32::consts::TAU
    }
}

/// A yaw (radians) as 1/256 turns.
pub fn yaw_to_u8(yaw: f32) -> u8 {
    (yaw / std::f32::consts::TAU * 256.0)
        .round()
        .rem_euclid(256.0) as u8
}

/// What happens in a Zombies game ([`GameEvent::Zombies`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ZombiesEvent {
    /// To everyone, whenever it changes and on joining: round `round` is
    /// counting down (`Break`, until server tick `starts_at`), being fought
    /// with `remaining` zombies still to kill, or was the last (`Over`).
    Round {
        round: u32,
        phase: ZombiesPhase,
        starts_at: Option<Tick>,
        remaining: u32,
    },
    /// To a shooter: their shot (or knife, or grenade) number `shot` hit
    /// zombie `zombie`.
    Hit {
        zombie: u8,
        shot: u32,
        headshot: bool,
        killed: bool,
    },
    /// To one player: their points changed by `change`, to `total`.
    Points { total: u32, change: i32 },
    /// To everyone, whenever it changes and on joining: window `window`
    /// (the map's barriers in order) has `boards` boards up.
    Boards { window: u8, boards: u8 },
    /// To everyone, when it changes and on joining: door `door` (the map's
    /// `Doors` in order) is open, or closed again for a new game.
    Door { door: u8, open: bool },
    /// To everyone: a kill dropped power-up `id` of `kind` at `at`. It lies
    /// there until picked up or [`crate::zombies::POWER_UP_LIFETIME`] runs out.
    PowerUpDropped {
        id: u8,
        kind: crate::zombies::PowerUp,
        at: Vec3,
    },
    /// To everyone: power-up `id` is gone, taken by player `by` (none when
    /// it ran out).
    PowerUpGone { id: u8, by: Option<u8> },
    /// To everyone, when it changes and on joining: player `player` went
    /// down and bleeds out at server tick `bleeds_out_at`, or (none) is no
    /// longer down: revived, or bled out.
    Downed {
        player: u8,
        bleeds_out_at: Option<Tick>,
    },
    /// To everyone: player `by` started reviving downed player `player`, done
    /// at server tick `done_at` if they keep holding; `by` none when the
    /// revive stopped short.
    Revive {
        player: u8,
        by: Option<u8>,
        done_at: Tick,
    },
    /// To everyone, whenever they change: the server ticks Insta-Kill and
    /// Double Points run out (none when off).
    PowerUpTimers {
        insta_kill_until: Option<Tick>,
        double_points_until: Option<Tick>,
    },
    /// To everyone, when it changes and on joining: the power is on (or off
    /// again for a new game). Drink machines and the upgrade machine need it.
    Power { on: bool },
    /// To everyone, when it changes, on joining, and when players come or
    /// go: drink machine `machine` (the map's `DrinkMachines` in order)
    /// sells for `price`, needs the power on (`needs_power`), and can be
    /// used, or has left (`available` false: Second Wind's, once a lone
    /// player has bought it three times).
    Machine {
        machine: u8,
        available: bool,
        price: u32,
        needs_power: bool,
    },
    /// To everyone: player `player` bought and drank `drink`.
    Drank {
        player: u8,
        drink: crate::zombies::drinks::Drink,
    },
    /// To everyone, when it changes and on joining: the mystery crate
    /// ([`crate::zombies::mystery`]) at spot `spot` (the map's
    /// `MysteryCrates` in order) is in `phase`, for player `by` (the buyer),
    /// landing on or offering `weapon` (none for the duck), until server
    /// tick `until`.
    Crate {
        spot: u8,
        phase: CratePhase,
        by: u8,
        weapon: Option<WeaponId>,
        until: Option<Tick>,
    },
    /// To everyone, when it changes and on joining: the forge
    /// ([`crate::zombies::forge`]) is idle, working `weapon` (the upgrade)
    /// for player `by`, or holding it out to them, until server tick
    /// `until`.
    Forge {
        phase: ForgePhase,
        by: u8,
        weapon: Option<WeaponId>,
        until: Option<Tick>,
    },
}

/// What the forge is doing ([`ZombiesEvent::Forge`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForgePhase {
    Idle,
    Working,
    Offering,
}

/// What the mystery crate is doing ([`ZombiesEvent::Crate`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CratePhase {
    /// Closed, waiting for a buyer.
    Idle,
    /// Open, weapons cycling; it lands on `weapon` (or the duck).
    Cycling,
    /// Holding `weapon` out for the buyer to take.
    Offering,
    /// The duck: the crate is about to leave.
    Duck,
    /// Gone from this spot; it shows up at another at `until`.
    Away,
}

impl CratePhase {
    const ALL: [CratePhase; 5] = [
        CratePhase::Idle,
        CratePhase::Cycling,
        CratePhase::Offering,
        CratePhase::Duck,
        CratePhase::Away,
    ];

    pub fn code(self) -> u32 {
        Self::ALL.iter().position(|p| *p == self).unwrap_or(0) as u32
    }

    pub fn from_code(code: u32) -> Option<Self> {
        Self::ALL.get(code as usize).copied()
    }
}

/// Where a Zombies round is ([`crate::zombies::Phase`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZombiesPhase {
    Break,
    Fighting,
    Over,
}

/// A smoke cloud still hanging, as clients see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmokeView {
    /// Who threw it (`None` if they have left) and their shot number for
    /// the throw, the same key as the cloud's `GameEvent::Blast`.
    pub owner: Option<u8>,
    pub shot: u32,
    /// Where the grenade popped, in millimetres (the `Blast`'s `at`).
    pub position: IVec3,
    /// Ticks since it popped.
    pub age: Tick,
}

impl SmokeView {
    pub fn new(owner: Option<u8>, shot: u32, position: Vec3, age: Tick) -> Self {
        Self {
            owner,
            shot,
            position: (position * POSITION_SCALE).round().as_ivec3(),
            age,
        }
    }

    /// Where it popped, in metres.
    pub fn position(&self) -> Vec3 {
        self.position.as_vec3() / POSITION_SCALE
    }
}

/// A live grenade as clients see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrenadeView {
    /// Who threw it.
    pub owner: u8,
    /// The thrower's shot number for the throw (`CombatEvent::Thrown`), so
    /// the thrower's own drawn grenade can be matched to it.
    pub shot: u32,
    pub kind: equipment::Kind,
    /// Centre in millimetres.
    pub position: IVec3,
    /// Lying still.
    pub resting: bool,
}

impl GrenadeView {
    pub fn new(owner: u8, shot: u32, kind: equipment::Kind, position: Vec3, resting: bool) -> Self {
        Self {
            owner,
            shot,
            kind,
            position: (position * POSITION_SCALE).round().as_ivec3(),
            resting,
        }
    }

    /// Centre in metres.
    pub fn position(&self) -> Vec3 {
        self.position.as_vec3() / POSITION_SCALE
    }
}

/// Most grenades a snapshot carries.
pub const MAX_GRENADES: usize = 32;
/// Most smoke clouds a snapshot carries.
pub const MAX_SMOKES: usize = 16;

/// Why a kill happened, for the kill feed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KillCause {
    /// The weapon in the killer's hands.
    #[default]
    Weapon,
    /// A knife strike ([`crate::melee`]).
    Knife,
    /// Thrown equipment.
    Equipment(equipment::Kind),
    /// Search and Destroy's charge going off.
    Charge,
    /// A scorestreak (the Airstrike).
    Streak(Streak),
}

/// Everything client prediction needs to restart from the server's result.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OwnState {
    /// Sequence number of the last input the server applied (0 for none).
    pub last_input: u32,
    pub predicted: Predicted,
}

/// A player's full simulation state plus the parts the engine owns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Predicted {
    pub state: PlayerState,
    /// Feet position.
    pub position: Vec3,
    pub on_floor: bool,
}

/// What other clients see of a player: quantized, and only what rendering needs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerView {
    pub id: u8,
    /// Feet position in millimetres.
    pub position: IVec3,
    /// View angles in 1/65536 turns.
    pub yaw: u16,
    pub pitch: u16,
    pub stance: Stance,
    /// How far down the body is, standing (0) to crouched (128) to prone
    /// (255) (`MoveState::crouch`, [`crouch_to_u8`]): the drawn body and its
    /// hit zones follow it.
    pub crouch: u8,
    pub sprinting: bool,
    pub on_floor: bool,
    /// Aim-down-sights amount, 0 to 255.
    pub ads: u8,
    pub weapon: WeaponId,
    pub reloading: bool,
    /// The reload is from an empty magazine: other players see the old one
    /// dropped instead of stowed.
    pub reload_empty: bool,
    pub alive: bool,
    /// Rounds fired so far; an increase means the player fired.
    pub shots_fired: u32,
    /// Climbing over a ledge.
    pub mantling: bool,
    /// Vaulting over thin cover: the climb is a vault, or the body is
    /// dropping off the cover's far side after one (`MoveState::in_vault`).
    pub vaulting: bool,
    /// Swinging the knife.
    pub meleeing: bool,
    /// Throwing a grenade: 0 when not, otherwise how far along the throw is
    /// as 1 to 255 ([`throw_to_u8`]).
    pub throw: u8,
    /// What the body is throwing, while it is.
    pub throw_kind: equipment::Kind,
    /// Hits taken so far (wrapping); an increase means the player was hit, so
    /// everyone sees the flinch.
    pub hits: u8,
    /// The horizontal direction the latest hit travelled, in 1/256 turns
    /// (see [`hit_direction`]).
    pub hit_heading: u8,
    /// Where the latest hit landed and how hard ([`HitPart`]).
    pub hit_part: u8,
    /// Stride phase in 1/256 strides, and reload and mantle progress out of
    /// 255 (`hitbox::clock`): every client draws the body, and the hit zones
    /// pose it, at the same moment of each animation.
    pub stride: u8,
    pub reload_progress: u8,
    pub mantle_progress: u8,
    /// How far the aim has turned from the feet standing still, in 1/256
    /// turns, signed (`Progress::turn`): the legs stay planted until the
    /// feet step round.
    pub turn: u8,
    /// How far the legs step in place as the feet swing round, out of 255
    /// (`Progress::step`).
    pub step: u8,
    /// On a ladder: the way it is climbed, in 1/256 turns
    /// ([`heading_to_u8`] of `MoveState::ladder_facing`).
    pub ladder: Option<u8>,
    /// How far the weapon is mounted on cover, 0 to 255
    /// (`MoveState::mount_blend`): the body leans onto the gun on the cover.
    pub mount: u8,
    /// Planting or defusing the charge: 0 when not, otherwise how far along
    /// as 1 to 255 ([`throw_to_u8`]); `defusing` says which.
    pub charge_use: u8,
    pub defusing: bool,
    /// Zombies: down in last stand, waiting for a revive.
    pub downed: bool,
    /// Moves quietly (Dead Silence): other players hear its movement far
    /// quieter and only up close (`perk::heard`).
    pub quiet: bool,
}

/// A throw's progress (0 to 1) as [`PlayerView::throw`]: 0 for none.
pub fn throw_to_u8(progress: Option<f32>) -> u8 {
    progress.map_or(0, |p| 1 + (p.clamp(0.0, 1.0) * 254.0).round() as u8)
}

/// A horizontal direction (`x`, `z`) as 1/256 turns, for `hit_heading`.
pub fn heading_to_u8(x: f32, z: f32) -> u8 {
    let turns = x.atan2(z) / std::f32::consts::TAU;
    (turns * 256.0).round().rem_euclid(256.0) as u8
}

/// Where a hit landed and how hard, packed in a byte for `hit_part`: the
/// shape it hit (an index into `hitbox::hitboxes`, 15 for unknown) in the low
/// four bits, and a force level 0 to 15 in the high four. Cosmetic: clients
/// pick the hit reaction and the ragdoll's push from it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HitPart {
    pub part: Option<u8>,
    /// 1 to 15: about a tenth of the damage dealt; [`KNIFE_FORCE`] for a
    /// knife.
    pub force: u8,
}

/// The force level a knife hit carries: a level guns never use (theirs is at
/// least 1), so a knife kill can crumple the body where it stands instead of
/// toppling it like a shot.
pub const KNIFE_FORCE: u8 = 0;

impl HitPart {
    /// The force level of `damage` dealt by a gun, or by a knife (`melee`,
    /// [`KNIFE_FORCE`]), which pushes little however much it deals.
    pub fn new(part: Option<u8>, damage: f32, melee: bool) -> Self {
        let force = if melee {
            KNIFE_FORCE
        } else {
            (damage / 10.0).round().clamp(1.0, 15.0) as u8
        };
        Self {
            part: part.filter(|p| usize::from(*p) < crate::hitbox::COUNT),
            force,
        }
    }

    pub fn to_u8(self) -> u8 {
        (self.force.min(15) << 4) | self.part.unwrap_or(15).min(15)
    }

    pub fn from_u8(v: u8) -> Self {
        let part = v & 15;
        Self {
            part: (usize::from(part) < crate::hitbox::COUNT).then_some(part),
            force: v >> 4,
        }
    }
}

/// The unit direction (`x`, `z`) of a `hit_heading`.
pub fn hit_direction(heading: u8) -> (f32, f32) {
    let angle = f32::from(heading) / 256.0 * std::f32::consts::TAU;
    (angle.sin(), angle.cos())
}

impl PlayerView {
    pub fn from_state(id: u8, p: &Predicted, yaw: f32, pitch: f32) -> Self {
        let combat = &p.state.combat;
        Self {
            id,
            position: (p.position * POSITION_SCALE).round().as_ivec3(),
            yaw: angle_to_u16(yaw),
            pitch: angle_to_u16(pitch),
            stance: p.state.movement.stance,
            crouch: crouch_to_u8(p.state.movement.crouch),
            sprinting: p.state.movement.sprinting,
            on_floor: p.on_floor,
            ads: (combat.ads.clamp(0.0, 1.0) * 255.0).round() as u8,
            weapon: combat.weapon().id,
            reloading: combat.is_reloading(),
            reload_empty: combat.is_empty_reload(),
            alive: p.state.alive(),
            downed: p.state.downed,
            quiet: p.state.perks.has(crate::perk::Perk::DeadSilence),
            shots_fired: combat.shots_fired,
            mantling: p.state.movement.mantling(),
            vaulting: p.state.movement.in_vault(p.on_floor),
            meleeing: combat.meleeing(),
            throw: throw_to_u8(combat.throw_progress()),
            throw_kind: match combat.action {
                Action::Throwing { kind, .. } => kind,
                _ => equipment::Kind::Frag,
            },
            hits: 0,
            hit_heading: 0,
            hit_part: 0,
            stride: 0,
            reload_progress: 0,
            mantle_progress: 0,
            turn: 0,
            step: 0,
            mount: (p.state.movement.mount_blend.clamp(0.0, 1.0) * 255.0).round() as u8,
            ladder: p.state.movement.on_ladder().then(|| {
                heading_to_u8(
                    p.state.movement.ladder_facing.x,
                    p.state.movement.ladder_facing.y,
                )
            }),
            charge_use: 0,
            defusing: false,
        }
    }

    /// Planting or defusing, and how far along (0 to 1), if either.
    pub fn charge_progress(&self) -> Option<(UseKind, f32)> {
        (self.charge_use > 0).then(|| {
            let kind = if self.defusing {
                UseKind::Defuse
            } else {
                UseKind::Plant
            };
            (kind, f32::from(self.charge_use - 1) / 254.0)
        })
    }

    /// The way the ladder this body is on is climbed (`x`, `z`), if it is on one.
    pub fn ladder_facing(&self) -> Option<Vec2> {
        self.ladder.map(|heading| {
            let (x, z) = hit_direction(heading);
            Vec2::new(x, z)
        })
    }

    /// How far along a throw this body is (0 to 1), if it is throwing.
    pub fn throw_progress(&self) -> Option<f32> {
        (self.throw > 0).then(|| f32::from(self.throw - 1) / 254.0)
    }

    /// The animation progress this view carries.
    pub fn progress(&self) -> Progress {
        Progress {
            stride: clock::stride_from_u8(self.stride),
            reload: clock::from_u8(self.reload_progress),
            mantle: clock::from_u8(self.mantle_progress),
            turn: clock::turn_from_u8(self.turn),
            step: clock::from_u8(self.step),
        }
    }

    /// This view with the animation progress `p`.
    pub fn with_progress(self, p: Progress) -> Self {
        Self {
            stride: clock::stride_to_u8(p.stride),
            reload_progress: clock::to_u8(p.reload),
            mantle_progress: clock::to_u8(p.mantle),
            turn: clock::turn_to_u8(p.turn),
            step: clock::to_u8(p.step),
            ..self
        }
    }

    pub fn position(&self) -> Vec3 {
        self.position.as_vec3() / POSITION_SCALE
    }

    /// Yaw in radians, in `[-π, π)`.
    pub fn yaw(&self) -> f32 {
        u16_to_angle(self.yaw)
    }

    /// Pitch in radians, in `[-π, π)`.
    pub fn pitch(&self) -> f32 {
        u16_to_angle(self.pitch)
    }

    pub fn ads(&self) -> f32 {
        f32::from(self.ads) / 255.0
    }

    /// The crouch blend, 0 standing to 1 crouched to 2 prone
    /// (`MoveState::crouch`).
    pub fn crouch(&self) -> f32 {
        crouch_from_u8(self.crouch)
    }
}

fn angle_to_u16(radians: f32) -> u16 {
    let turns = radians / std::f32::consts::TAU;
    let fraction = turns - turns.floor();
    ((fraction * ANGLE_STEPS).round() as u32 % 65536) as u16
}

fn u16_to_angle(value: u16) -> f32 {
    (value as i16) as f32 / ANGLE_STEPS * std::f32::consts::TAU
}

impl PlayerInput {
    /// The input as the server will decode it. The client predicts with this,
    /// so both sides step with exactly the same values.
    pub fn quantized(mut self) -> Self {
        let q = |v: f32| (v.clamp(-1.0, 1.0) * WISH_SCALE).round() / WISH_SCALE;
        self.wish = Vec2::new(q(self.wish.x), q(self.wish.y));
        // The client already clamps; this keeps a tampered pitch off the server.
        self.pitch = view_angle(self.pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT));
        self.yaw = view_angle(self.yaw);
        self.switch_to = self.switch_to.filter(|slot| *slot < 4);
        self.pickup = self.pickup.filter(|id| valid_weapon(*id));
        self.lunge = self.lunge.and_then(quantize_lunge);
        self.view_tick = self.view_tick.and_then(quantize_view_tick);
        self.fire_aim = self.fire_aim.and_then(subtick::quantize);
        self
    }
}

/// A lunge offset as the wire carries it (whole millimetres), or `None` if no
/// honest client would send it.
fn quantize_lunge(offset: Vec2) -> Option<Vec2> {
    (offset.is_finite() && offset.length() <= crate::melee::MAX_LUNGE_OFFSET)
        .then(|| (offset * LUNGE_SCALE).round() / LUNGE_SCALE)
}

/// View angles in inputs travel as whole steps of 2^-14 radians (about
/// 0.0035 degrees, finer than Quake III's and Call of Duty's 16-bit
/// angles), so a turn between two inputs is a small change. Yaw is not
/// wrapped (the client lets it run past a turn), and every step count below
/// [`MAX_VIEW_STEPS`] is exact both as an integer and as an `f32`.
const VIEW_ANGLE_STEPS: f32 = 16384.0;
/// Yaw beyond this many steps (about 10,000 turns) is clamped.
const MAX_VIEW_STEPS: i32 = i32::MAX / 2;

/// An angle snapped to the input grid.
fn view_angle(radians: f32) -> f32 {
    if !radians.is_finite() {
        return 0.0;
    }
    angle_from_steps(angle_steps(radians))
}

fn angle_steps(radians: f32) -> i32 {
    ((radians * VIEW_ANGLE_STEPS).round() as i32).clamp(-MAX_VIEW_STEPS, MAX_VIEW_STEPS)
}

fn angle_from_steps(steps: i32) -> f32 {
    steps as f32 / VIEW_ANGLE_STEPS
}

/// A view tick as the wire carries it, or `None` if it cannot be sent.
fn quantize_view_tick(tick: f64) -> Option<f64> {
    // NaN and infinities fall outside the range too.
    (0.0..VIEW_TICK_LIMIT)
        .contains(&tick)
        .then(|| (tick * VIEW_TICK_STEPS).floor() / VIEW_TICK_STEPS)
}

// ---------------------------------------------------------------------------
// Client messages

impl ClientMessage {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = BitWriter::new();
        match self {
            ClientMessage::Hello {
                version,
                name,
                account,
            } => {
                w.bits(0, TAG_BITS);
                w.u16(*version);
                write_string(&mut w, name, MAX_NAME_BYTES);
                write_string(&mut w, account, MAX_ACCOUNT_TOKEN_BYTES);
            }
            ClientMessage::Input(packet) => {
                w.bits(1, TAG_BITS);
                write_option_tick(&mut w, packet.ack_snapshot);
                // Only the newest inputs fit; number the first one actually sent.
                let count = packet.inputs.len().min(MAX_INPUTS_PER_PACKET);
                let skipped = (packet.inputs.len() - count) as u32;
                w.varint(packet.first_seq.wrapping_add(skipped));
                w.varint(count as u32);
                // The first input whole, each later one as changes from the
                // one before it (`net::delta`): while a player turns and
                // moves, consecutive inputs differ in a few small fields.
                let mut previous: Option<Vec<delta::Field>> = None;
                let mut last: Option<PlayerInput> = None;
                for input in &packet.inputs[packet.inputs.len() - count..] {
                    let input = input.quantized();
                    let same = last == Some(input);
                    w.bool(same);
                    if !same {
                        match &previous {
                            Some(fields) => {
                                write_input(&mut DeltaWriter::new(&mut w, fields), &input);
                            }
                            None => write_input(&mut w, &input),
                        }
                        previous = Some(delta::record(|r| write_input(r, &input)));
                    }
                    last = Some(input);
                }
            }
            ClientMessage::Debug { rewound_hitboxes } => {
                w.bits(2, TAG_BITS);
                w.bool(*rewound_hitboxes);
            }
            ClientMessage::Ready { ready } => {
                w.bits(3, TAG_BITS);
                w.bool(*ready);
            }
            ClientMessage::Class(class) => {
                w.bits(4, TAG_BITS);
                write_class(&mut w, class);
            }
            ClientMessage::Chat { text, team } => {
                w.bits(5, TAG_BITS);
                w.bool(*team);
                write_string(&mut w, text, chat::MAX_BYTES);
            }
            ClientMessage::SkipKillcam => w.bits(6, TAG_BITS),
            ClientMessage::Streaks(loadout) => {
                w.bits(7, TAG_BITS);
                w.bits(0, MORE_TAG_BITS);
                write_loadout(&mut w, *loadout);
            }
            ClientMessage::UseStreak { streak, targets } => {
                w.bits(7, TAG_BITS);
                w.bits(1, MORE_TAG_BITS);
                write_streak(&mut w, *streak);
                let count = targets.len().min(streak::AIRSTRIKE_TARGETS);
                w.varint(count as u32);
                for t in &targets[..count] {
                    w.f32(t.x);
                    w.f32(t.y);
                }
            }
        }
        w.finish()
    }

    pub fn decode(bytes: &[u8]) -> DecodeResult<Self> {
        let mut r = BitReader::new(bytes);
        match r.bits(TAG_BITS)? {
            0 => {
                let version = r.u16()?;
                // An older client sends no name or account; let the version
                // check reject it.
                let (name, account) = if version == PROTOCOL_VERSION {
                    (
                        read_string(&mut r, MAX_NAME_BYTES)?,
                        read_string(&mut r, MAX_ACCOUNT_TOKEN_BYTES)?,
                    )
                } else {
                    (String::new(), String::new())
                };
                Ok(ClientMessage::Hello {
                    version,
                    name,
                    account,
                })
            }
            1 => {
                let ack_snapshot = read_option_tick(&mut r)?;
                let first_seq = r.varint()?;
                let count = r.varint()? as usize;
                if count > MAX_INPUTS_PER_PACKET {
                    return Err(DecodeError::Invalid("input count"));
                }
                let mut inputs: Vec<PlayerInput> = Vec::with_capacity(count);
                let mut previous: Option<Vec<delta::Field>> = None;
                for _ in 0..count {
                    let input = if r.bool()? {
                        *inputs.last().ok_or(DecodeError::Invalid("repeat"))?
                    } else {
                        let input = match &previous {
                            Some(fields) => read_input(&mut DeltaReader::new(&mut r, fields))?,
                            None => read_input(&mut r)?,
                        };
                        previous = Some(delta::record(|w| write_input(w, &input)));
                        input
                    };
                    inputs.push(input);
                }
                Ok(ClientMessage::Input(InputPacket {
                    ack_snapshot,
                    first_seq,
                    inputs,
                }))
            }
            2 => Ok(ClientMessage::Debug {
                rewound_hitboxes: r.bool()?,
            }),
            3 => Ok(ClientMessage::Ready { ready: r.bool()? }),
            4 => Ok(ClientMessage::Class(read_class(&mut r)?)),
            5 => {
                let team = r.bool()?;
                Ok(ClientMessage::Chat {
                    text: read_string(&mut r, chat::MAX_BYTES)?,
                    team,
                })
            }
            6 => Ok(ClientMessage::SkipKillcam),
            7 => match r.bits(MORE_TAG_BITS)? {
                0 => Ok(ClientMessage::Streaks(read_loadout(&mut r)?)),
                1 => {
                    let streak = read_streak(&mut r)?;
                    let count = r.varint()? as usize;
                    if count > streak::AIRSTRIKE_TARGETS {
                        return Err(DecodeError::Invalid("streak targets"));
                    }
                    let mut targets = Vec::with_capacity(count);
                    for _ in 0..count {
                        targets.push(Vec2::new(read_finite(&mut r)?, read_finite(&mut r)?));
                    }
                    Ok(ClientMessage::UseStreak { streak, targets })
                }
                _ => Err(DecodeError::Invalid("client message tag")),
            },
            _ => Err(DecodeError::Invalid("client message tag")),
        }
    }
}

/// Writes a quantized input ([`PlayerInput::quantized`]); written against
/// [`BitSink`] so later inputs in a packet go as changes from the one before.
fn write_input(w: &mut impl BitSink, input: &PlayerInput) {
    let byte = |v: f32| ((v.clamp(-1.0, 1.0) * WISH_SCALE).round() as i8) as u8;
    w.u8(byte(input.wish.x));
    w.u8(byte(input.wish.y));
    w.number(angle_steps(input.yaw) as u32, 32);
    let pitch = angle_steps(input.pitch).clamp(i32::from(i16::MIN), i32::from(i16::MAX));
    w.number(u32::from(pitch as i16 as u16), 16);
    // Every button and which optional parts follow, as one field: from one
    // input to the next it rarely changes, and then costs a bit.
    let lunge = input.lunge.and_then(quantize_lunge);
    let switch_to = input.switch_to.filter(|slot| *slot < 4);
    let pickup = input.pickup.filter(|id| valid_weapon(*id));
    let view_tick = input.view_tick.and_then(quantize_view_tick);
    let fire_aim = input
        .fire_aim
        .and_then(|a| Some((subtick::steps(a.yaw)?, subtick::steps(a.pitch)?)));
    let mut mask = 0;
    for (bit, flag) in [
        input.jump,
        input.sprint,
        input.crouch,
        input.fire,
        input.aim,
        input.reload,
        input.melee,
        input.lethal,
        input.tactical,
        input.interact,
        lunge.is_some(),
        switch_to.is_some(),
        pickup.is_some(),
        view_tick.is_some(),
        input.prone,
    ]
    .into_iter()
    .enumerate()
    {
        mask |= u32::from(flag) << bit;
    }
    w.bits(mask, INPUT_FLAG_BITS);
    if let Some(offset) = lunge {
        let mm = (offset * LUNGE_SCALE).round();
        w.u16(mm.x as i16 as u16);
        w.u16(mm.y as i16 as u16);
    }
    if let Some(slot) = switch_to {
        w.bits(u32::from(slot), 2);
    }
    if let Some(id) = pickup {
        write_weapon(w, id);
    }
    // The whole tick, then the fraction: from one input to the next the
    // whole tick moves by one.
    if let Some(tick) = view_tick {
        let steps = view_steps(tick);
        w.u32((steps >> 8) as u32);
        w.u8(steps as u8);
    }
    // The press aim rides on one input in a burst, so it keeps its own flag
    // rather than changing the mask twice.
    w.bool(fire_aim.is_some());
    if let Some((yaw, pitch)) = fire_aim {
        w.varint_signed(yaw);
        w.varint_signed(pitch);
    }
}

/// Bits of an input's button and optional-part mask.
const INPUT_FLAG_BITS: u32 = 15;

/// View ticks in wire steps. Only called on quantized ticks, which are exact.
fn view_steps(tick: f64) -> i64 {
    (tick * VIEW_TICK_STEPS) as i64
}

fn write_view_tick(w: &mut BitWriter, tick: Option<f64>, previous: Option<f64>) {
    let Some(tick) = tick.and_then(quantize_view_tick) else {
        w.bool(false);
        return;
    };
    w.bool(true);
    let delta = previous
        .map(|p| view_steps(tick) - view_steps(p))
        .and_then(|d| i32::try_from(d).ok());
    w.bool(delta.is_some());
    match delta {
        Some(delta) => w.varint_signed(delta),
        None => {
            let steps = view_steps(tick);
            w.u32((steps >> 8) as u32);
            w.u8(steps as u8);
        }
    }
}

fn read_view_tick(r: &mut BitReader, previous: Option<f64>) -> DecodeResult<Option<f64>> {
    if !r.bool()? {
        return Ok(None);
    }
    let steps = if r.bool()? {
        let previous = previous.ok_or(DecodeError::Invalid("view tick delta"))?;
        view_steps(previous) + i64::from(r.varint_signed()?)
    } else {
        (i64::from(r.u32()?) << 8) | i64::from(r.u8()?)
    };
    let tick = steps as f64 / VIEW_TICK_STEPS;
    if !(0.0..VIEW_TICK_LIMIT).contains(&tick) {
        return Err(DecodeError::Invalid("view tick"));
    }
    Ok(Some(tick))
}

fn read_input(r: &mut impl BitSource) -> DecodeResult<PlayerInput> {
    let mut axis = || -> DecodeResult<f32> {
        let v = r.u8()? as i8;
        if v == i8::MIN {
            return Err(DecodeError::Invalid("wish"));
        }
        Ok(f32::from(v) / WISH_SCALE)
    };
    let wish = Vec2::new(axis()?, axis()?);
    let yaw_steps = r.number(32)? as i32;
    if yaw_steps.unsigned_abs() > MAX_VIEW_STEPS.unsigned_abs() {
        return Err(DecodeError::Invalid("yaw"));
    }
    let yaw = angle_from_steps(yaw_steps);
    let pitch = angle_from_steps(i32::from(r.number(16)? as u16 as i16));
    if pitch.abs() > PITCH_LIMIT + 1.0 / VIEW_ANGLE_STEPS {
        return Err(DecodeError::Invalid("pitch"));
    }
    let mask = r.bits(INPUT_FLAG_BITS)?;
    let flag = |bit: u32| mask & (1 << bit) != 0;
    let lunge = if flag(10) {
        let x = f32::from(r.u16()? as i16);
        let z = f32::from(r.u16()? as i16);
        let offset = Vec2::new(x, z) / LUNGE_SCALE;
        if offset.length() > crate::melee::MAX_LUNGE_OFFSET {
            return Err(DecodeError::Invalid("lunge"));
        }
        Some(offset)
    } else {
        None
    };
    let switch_to = if flag(11) {
        Some(r.bits(2)? as u8)
    } else {
        None
    };
    let pickup = if flag(12) {
        Some(read_weapon(r)?)
    } else {
        None
    };
    let view_tick = if flag(13) {
        let steps = (i64::from(r.u32()?) << 8) | i64::from(r.u8()?);
        Some(steps as f64 / VIEW_TICK_STEPS)
    } else {
        None
    };
    let fire_aim = if r.bool()? {
        let mut axis = || -> DecodeResult<f32> {
            let steps = r.varint_signed()?;
            if steps.unsigned_abs() > subtick::LIMIT_STEPS.unsigned_abs() {
                return Err(DecodeError::Invalid("fire aim"));
            }
            Ok(subtick::from_steps(steps))
        };
        let yaw = axis()?;
        let pitch = axis()?;
        Some(FireAim { yaw, pitch })
    } else {
        None
    };
    let (jump, sprint, crouch, fire, aim) = (flag(0), flag(1), flag(2), flag(3), flag(4));
    let (reload, melee, lethal, tactical, interact) = (flag(5), flag(6), flag(7), flag(8), flag(9));
    Ok(PlayerInput {
        wish,
        yaw,
        pitch,
        jump,
        sprint,
        crouch,
        prone: flag(14),
        fire,
        aim,
        reload,
        switch_to,
        pickup,
        melee,
        lunge,
        lethal,
        tactical,
        interact,
        view_tick,
        fire_aim,
    })
}

// ---------------------------------------------------------------------------
// Server messages

impl ServerMessage {
    /// Encodes the message. `baseline` holds the remote players of the snapshot
    /// named by `Snapshot::baseline`, exactly as this client received them.
    pub fn encode(&self, baseline: &[PlayerView]) -> Vec<u8> {
        self.encode_with(Baseline {
            players: baseline,
            ..Baseline::default()
        })
    }

    /// Encodes the message against everything the client has of the
    /// snapshot named by `Snapshot::baseline`: its remote players and its own
    /// state, so both are sent as changes.
    pub fn encode_with(&self, baseline: Baseline) -> Vec<u8> {
        let mut w = BitWriter::new();
        match self {
            ServerMessage::Welcome {
                player_id,
                tick,
                snapshot_interval,
                mode,
                server_build,
                ranked,
            } => {
                w.bits(0, TAG_BITS);
                w.u16(PROTOCOL_VERSION);
                w.u8(*player_id);
                w.u32(*tick);
                w.u8(*snapshot_interval);
                write_mode(&mut w, mode);
                write_string(&mut w, server_build, MAX_BUILD_BYTES);
                w.bool(*ranked);
            }
            ServerMessage::Reject { server_version } => {
                w.bits(1, TAG_BITS);
                w.u16(*server_version);
            }
            ServerMessage::Denied { reason } => {
                w.bits(4, TAG_BITS);
                write_string(&mut w, reason, MAX_DENIED_BYTES);
            }
            ServerMessage::Snapshot(snapshot) => {
                w.bits(2, TAG_BITS);
                write_snapshot(&mut w, snapshot, baseline);
            }
            ServerMessage::Events(events) => {
                w.bits(3, TAG_BITS);
                let count = events.len().min(MAX_EVENTS_PER_MESSAGE);
                w.varint(count as u32);
                for event in &events[..count] {
                    write_event(&mut w, event);
                }
            }
        }
        w.finish()
    }

    /// Decodes a message. `baseline` looks up the remote players of an earlier
    /// snapshot by tick; a snapshot whose baseline is unknown fails to decode.
    pub fn decode<'a>(
        bytes: &[u8],
        baseline: impl FnOnce(Tick) -> Option<&'a [PlayerView]>,
    ) -> DecodeResult<Self> {
        // Only the players' baseline is looked up: this form never carries
        // own state or horde changes.
        let baseline = std::cell::Cell::new(Some(baseline));
        Self::decode_with(bytes, |tick| {
            let players = baseline.take()?(tick)?;
            Some(Baseline {
                players,
                ..Baseline::default()
            })
        })
    }

    /// Decodes a message. `baseline` looks up what the client has of an
    /// earlier snapshot by tick; a snapshot whose baseline is unknown, or
    /// whose own state is sent as changes from an own state the client does
    /// not have, fails to decode.
    pub fn decode_with<'a>(
        bytes: &[u8],
        baseline: impl Fn(Tick) -> Option<Baseline<'a>>,
    ) -> DecodeResult<Self> {
        let mut r = BitReader::new(bytes);
        match r.bits(TAG_BITS)? {
            0 => {
                let version = r.u16()?;
                if version != PROTOCOL_VERSION {
                    return Ok(ServerMessage::Reject {
                        server_version: version,
                    });
                }
                Ok(ServerMessage::Welcome {
                    player_id: r.u8()?,
                    tick: r.u32()?,
                    snapshot_interval: r.u8()?.max(1),
                    mode: read_mode(&mut r)?,
                    server_build: read_string(&mut r, MAX_BUILD_BYTES)?,
                    ranked: r.bool()?,
                })
            }
            1 => Ok(ServerMessage::Reject {
                server_version: r.u16()?,
            }),
            4 => Ok(ServerMessage::Denied {
                reason: read_string(&mut r, MAX_DENIED_BYTES)?,
            }),
            2 => Ok(ServerMessage::Snapshot(read_snapshot(&mut r, baseline)?)),
            3 => {
                let count = r.varint()? as usize;
                if count > MAX_EVENTS_PER_MESSAGE {
                    return Err(DecodeError::Invalid("event count"));
                }
                let mut events = Vec::with_capacity(count);
                for _ in 0..count {
                    events.push(read_event(&mut r)?);
                }
                Ok(ServerMessage::Events(events))
            }
            _ => Err(DecodeError::Invalid("server message tag")),
        }
    }
}

// ---------------------------------------------------------------------------
// Reliable game events

/// Bits for an event's tag; room for 32 kinds of event.
const EVENT_TAG_BITS: u32 = 5;

fn write_event(w: &mut BitWriter, event: &GameEvent) {
    match event {
        GameEvent::Hit {
            victim,
            shot,
            damage,
            headshot,
            killed,
        } => {
            w.bits(0, EVENT_TAG_BITS);
            w.u8(*victim);
            w.varint(*shot);
            w.f32(*damage);
            w.bool(*headshot);
            w.bool(*killed);
        }
        GameEvent::Hurt {
            attacker,
            from,
            damage,
            killed,
        } => {
            w.bits(1, EVENT_TAG_BITS);
            w.u8(*attacker);
            write_vec3(w, *from);
            w.f32(*damage);
            w.bool(*killed);
        }
        GameEvent::Kill {
            killer,
            victim,
            weapon,
            headshot,
            cause,
        } => {
            w.bits(2, EVENT_TAG_BITS);
            w.u8(*killer);
            w.u8(*victim);
            write_weapon(w, *weapon);
            w.bool(*headshot);
            write_cause(w, *cause);
        }
        GameEvent::Score { awards, totals } => {
            w.bits(3, EVENT_TAG_BITS);
            let count = awards.len().min(MAX_EVENTS_PER_MESSAGE);
            w.varint(count as u32);
            for scored in &awards[..count] {
                write_award(w, scored.award);
                w.varint(scored.points);
            }
            w.varint(totals.score);
            w.varint(totals.kills);
            w.varint(totals.deaths);
            w.varint(totals.streak);
        }
        GameEvent::Rewound { shot, tick, poses } => {
            w.bits(4, EVENT_TAG_BITS);
            w.varint(*shot);
            write_view_tick(w, Some(*tick), None);
            let count = poses.len().min(MAX_REWOUND_POSES);
            w.varint(count as u32);
            for (id, pose) in &poses[..count] {
                w.u8(*id);
                write_vec3(w, pose.feet);
                w.f32(pose.yaw);
                w.f32(pose.pitch);
                write_stance(w, pose.stance);
                write_activity(w, &pose.activity);
            }
        }
        GameEvent::Joined {
            id,
            name,
            team,
            bot,
        } => {
            w.bits(5, EVENT_TAG_BITS);
            w.u8(*id);
            write_string(w, name, MAX_NAME_BYTES);
            w.bits(u32::from(*team), 1);
            w.bool(*bot);
        }
        GameEvent::Left { id } => {
            w.bits(6, EVENT_TAG_BITS);
            w.u8(*id);
        }
        GameEvent::Match {
            phase,
            ends_at,
            scores,
            score_limit,
            period,
        } => {
            w.bits(7, EVENT_TAG_BITS);
            let (tag, winner) = match phase {
                Phase::Lobby => (0, None),
                Phase::Countdown => (1, None),
                Phase::Playing => (2, None),
                Phase::Ended { winner } => (3, Some(*winner)),
            };
            w.bits(tag, 2);
            if let Some(winner) = winner {
                w.bool(winner.is_some());
                if let Some(winner) = winner {
                    w.u8(winner);
                }
            }
            write_option_tick(w, *ends_at);
            for score in scores {
                w.varint(*score);
            }
            w.varint(*score_limit);
            match period {
                Period::Regulation => w.bits(0, 2),
                Period::SecondHalf => w.bits(1, 2),
                Period::Overtime { timed } => {
                    w.bits(2, 2);
                    w.bool(*timed);
                }
            }
        }
        GameEvent::Scoreboard { rows } => {
            w.bits(8, EVENT_TAG_BITS);
            let count = rows.len().min(MAX_SCOREBOARD_ROWS);
            w.varint(count as u32);
            for row in &rows[..count] {
                w.u8(row.id);
                w.varint(row.score);
                w.varint(row.kills);
                w.varint(row.deaths);
                w.u16(row.ping_ms);
                w.bool(row.ready);
                w.u8(row.level);
                w.u8(row.level_kills);
                w.varint(row.captures);
                w.varint(row.returns);
                w.varint(row.hill_seconds);
                w.varint(row.defends);
                w.bool(row.alive);
                w.varint(row.revives);
                w.varint(row.headshots);
            }
        }
        GameEvent::Spawned { yaw } => {
            w.bits(9, EVENT_TAG_BITS);
            w.f32(*yaw);
        }
        GameEvent::Flags { flags } => {
            w.bits(10, EVENT_TAG_BITS);
            for flag in flags {
                write_vec3(w, flag.home);
                match flag.status {
                    FlagStatus::Home => w.bits(0, 2),
                    FlagStatus::Carried { by } => {
                        w.bits(1, 2);
                        w.u8(by);
                    }
                    FlagStatus::Dropped { at, returns_at } => {
                        w.bits(2, 2);
                        write_vec3(w, at);
                        w.varint(returns_at);
                    }
                }
            }
        }
        GameEvent::FlagPlay { flag, play, player } => {
            w.bits(11, EVENT_TAG_BITS);
            w.bits(u32::from(*flag), 1);
            w.bits(
                match play {
                    FlagPlay::Taken => 0,
                    FlagPlay::Dropped => 1,
                    FlagPlay::Returned => 2,
                    FlagPlay::Captured => 3,
                },
                2,
            );
            w.bool(player.is_some());
            if let Some(player) = player {
                w.u8(*player);
            }
        }
        GameEvent::HardpointZones { zones } => {
            w.bits(12, EVENT_TAG_BITS);
            let count = zones.len().min(MAX_HARDPOINT_ZONES);
            w.varint(count as u32);
            for zone in &zones[..count] {
                write_string(w, &zone.name, MAX_ZONE_NAME_BYTES);
                write_vec3(w, zone.center);
                write_vec3(w, zone.half_extents);
            }
        }
        GameEvent::Hardpoint {
            active,
            holder,
            moves_at,
        } => {
            w.bits(13, EVENT_TAG_BITS);
            w.u8(*active);
            match holder {
                Holder::Neutral => w.bits(0, 2),
                Holder::Held(team) => {
                    w.bits(1, 2);
                    w.bits(u32::from(*team), 1);
                }
                Holder::Contested => w.bits(2, 2),
            }
            write_option_tick(w, *moves_at);
        }
        GameEvent::PropState { id, stage, impulse } => {
            w.bits(14, EVENT_TAG_BITS);
            w.varint(u32::from(*id));
            w.u8(*stage);
            write_vec3(w, *impulse);
        }
        GameEvent::Chat { from, text, team } => {
            w.bits(15, EVENT_TAG_BITS);
            w.u8(*from);
            w.bool(*team);
            write_string(w, text, chat::MAX_BYTES);
        }
        GameEvent::Round {
            number,
            phase,
            ends_at,
            attackers,
            halftime_after,
        } => {
            w.bits(16, EVENT_TAG_BITS);
            w.varint(*number);
            match phase {
                RoundPhase::Freeze => w.bits(0, 2),
                RoundPhase::Live => w.bits(1, 2),
                RoundPhase::Over { winner, reason } => {
                    w.bits(2, 2);
                    w.bits(u32::from(*winner), 1);
                    w.bits(
                        match reason {
                            RoundEnd::Eliminated => 0,
                            RoundEnd::Time => 1,
                            RoundEnd::Detonated => 2,
                            RoundEnd::Defused => 3,
                        },
                        2,
                    );
                }
            }
            w.u32(*ends_at);
            w.bits(u32::from(*attackers), 1);
            w.varint(*halftime_after);
        }
        GameEvent::Blast {
            owner,
            shot,
            kind,
            at,
        } => {
            w.bits(17, EVENT_TAG_BITS);
            match owner {
                Some(id) => {
                    w.bool(true);
                    w.u8(*id);
                }
                None => w.bool(false),
            }
            w.varint(*shot);
            w.bits(u32::from(kind.code()), EQUIPMENT_BITS);
            write_vec3(w, *at);
        }
        GameEvent::Killcam(k) => {
            w.bits(18, EVENT_TAG_BITS);
            w.bits(u32::from(k.kind.code()), 2);
            w.u8(k.killer);
            w.u8(k.victim);
            w.u32(k.kill_tick);
            w.u16(k.before);
            w.u16(k.after);
            w.u8(k.rewind);
            write_weapon(w, k.weapon);
            write_cause(w, k.cause);
            w.bool(k.headshot);
        }
        GameEvent::ChargeSites { sites } => {
            w.bits(19, EVENT_TAG_BITS);
            let count = sites.len().min(MAX_CHARGE_SITES);
            w.varint(count as u32);
            for site in &sites[..count] {
                write_string(w, &site.name, MAX_ZONE_NAME_BYTES);
                write_vec3(w, site.center);
                write_vec3(w, site.half_extents);
            }
        }
        GameEvent::Charge(view) => {
            w.bits(20, EVENT_TAG_BITS);
            write_charge(w, view);
        }
        GameEvent::DominationFlags { flags } => {
            w.bits(21, EVENT_TAG_BITS);
            let count = flags.len().min(MAX_DOMINATION_FLAGS);
            w.varint(count as u32);
            for flag in &flags[..count] {
                write_string(w, &flag.name, MAX_ZONE_NAME_BYTES);
                write_vec3(w, flag.center);
                w.f32(flag.radius);
                w.f32(flag.half_height);
            }
        }
        GameEvent::Domination(view) => {
            w.bits(22, EVENT_TAG_BITS);
            w.varint(view.tick);
            w.bool(view.swapped);
            let count = view.flags.len().min(MAX_DOMINATION_FLAGS);
            w.varint(count as u32);
            for flag in &view.flags[..count] {
                write_option_team(w, flag.owner);
                write_option_team(w, flag.claim);
                w.u8(flag.progress);
                w.bool(flag.contested);
                w.u8(flag.capturers);
            }
        }
        GameEvent::Streak(event) => {
            w.bits(STREAK_EVENT_TAG, EVENT_TAG_BITS);
            write_streak_event(w, event);
        }
        GameEvent::Stats(stats) => {
            w.bits(STATS_EVENT_TAG, EVENT_TAG_BITS);
            let entries: Vec<(&String, &u32)> = stats
                .iter()
                .filter(|(key, _)| stats::valid_key(key))
                .take(stats::MAX_MATCH_KEYS)
                .collect();
            w.varint(entries.len() as u32);
            for (key, count) in entries {
                write_string(w, key, stats::MAX_KEY_LEN);
                w.varint(*count);
            }
        }
        GameEvent::Zombies(event) => {
            w.bits(ZOMBIES_EVENT_TAG, EVENT_TAG_BITS);
            write_zombies_event(w, event);
        }
        GameEvent::Scavenger(event) => {
            w.bits(23, EVENT_TAG_BITS);
            match *event {
                ScavengerEvent::Dropped { id, at, dropped_by } => {
                    w.bits(0, 2);
                    w.u16(id);
                    write_vec3(w, at);
                    w.u8(dropped_by);
                }
                ScavengerEvent::Taken { id, by } => {
                    w.bits(1, 2);
                    w.u16(id);
                    w.u8(by);
                }
                ScavengerEvent::Gone { id } => {
                    w.bits(2, 2);
                    w.u16(id);
                }
            }
        }
    }
}

fn write_option_team(w: &mut BitWriter, team: Option<Team>) {
    w.bool(team.is_some());
    if let Some(team) = team {
        w.bits(u32::from(team.min(1)), 1);
    }
}

fn read_option_team(r: &mut BitReader) -> DecodeResult<Option<Team>> {
    Ok(if r.bool()? {
        Some(r.bits(1)? as Team)
    } else {
        None
    })
}

/// The event tag of [`GameEvent::Zombies`], kept at the top of the range so
/// new modes' events fill in below it.
const ZOMBIES_EVENT_TAG: u32 = 31;
/// Bits for a [`ZombiesEvent`]'s own tag.
const ZOMBIES_TAG_BITS: u32 = 5;

fn write_zombies_event(w: &mut BitWriter, event: &ZombiesEvent) {
    match *event {
        ZombiesEvent::Round {
            round,
            phase,
            starts_at,
            remaining,
        } => {
            w.bits(0, ZOMBIES_TAG_BITS);
            w.varint(round);
            w.bits(
                match phase {
                    ZombiesPhase::Break => 0,
                    ZombiesPhase::Fighting => 1,
                    ZombiesPhase::Over => 2,
                },
                2,
            );
            write_option_tick(w, starts_at);
            w.varint(remaining);
        }
        ZombiesEvent::Hit {
            zombie,
            shot,
            headshot,
            killed,
        } => {
            w.bits(1, ZOMBIES_TAG_BITS);
            w.u8(zombie);
            w.varint(shot);
            w.bool(headshot);
            w.bool(killed);
        }
        ZombiesEvent::Points { total, change } => {
            w.bits(2, ZOMBIES_TAG_BITS);
            w.varint(total);
            w.varint_signed(change);
        }
        ZombiesEvent::Boards { window, boards } => {
            w.bits(3, ZOMBIES_TAG_BITS);
            w.u8(window);
            w.u8(boards);
        }
        ZombiesEvent::Door { door, open } => {
            w.bits(4, ZOMBIES_TAG_BITS);
            w.u8(door);
            w.bool(open);
        }
        ZombiesEvent::PowerUpDropped { id, kind, at } => {
            w.bits(5, ZOMBIES_TAG_BITS);
            w.u8(id);
            w.bits(kind.code(), 2);
            write_vec3(w, at);
        }
        ZombiesEvent::PowerUpGone { id, by } => {
            w.bits(6, ZOMBIES_TAG_BITS);
            w.u8(id);
            w.bool(by.is_some());
            if let Some(by) = by {
                w.u8(by);
            }
        }
        ZombiesEvent::Downed {
            player,
            bleeds_out_at,
        } => {
            w.bits(8, ZOMBIES_TAG_BITS);
            w.u8(player);
            write_option_tick(w, bleeds_out_at);
        }
        ZombiesEvent::Revive {
            player,
            by,
            done_at,
        } => {
            w.bits(9, ZOMBIES_TAG_BITS);
            w.u8(player);
            w.bool(by.is_some());
            if let Some(by) = by {
                w.u8(by);
            }
            w.u32(done_at);
        }
        ZombiesEvent::PowerUpTimers {
            insta_kill_until,
            double_points_until,
        } => {
            w.bits(7, ZOMBIES_TAG_BITS);
            write_option_tick(w, insta_kill_until);
            write_option_tick(w, double_points_until);
        }
        ZombiesEvent::Power { on } => {
            w.bits(10, ZOMBIES_TAG_BITS);
            w.bool(on);
        }
        ZombiesEvent::Machine {
            machine,
            available,
            price,
            needs_power,
        } => {
            w.bits(11, ZOMBIES_TAG_BITS);
            w.u8(machine);
            w.bool(available);
            w.varint(price);
            w.bool(needs_power);
        }
        ZombiesEvent::Drank { player, drink } => {
            w.bits(12, ZOMBIES_TAG_BITS);
            w.u8(player);
            w.bits(u32::from(drink.code()), 3);
        }
        ZombiesEvent::Crate {
            spot,
            phase,
            by,
            weapon,
            until,
        } => {
            w.bits(13, ZOMBIES_TAG_BITS);
            w.u8(spot);
            w.bits(phase.code(), 3);
            w.u8(by);
            w.bool(weapon.is_some());
            if let Some(weapon) = weapon {
                write_weapon(w, weapon);
            }
            write_option_tick(w, until);
        }
        ZombiesEvent::Forge {
            phase,
            by,
            weapon,
            until,
        } => {
            w.bits(14, ZOMBIES_TAG_BITS);
            w.bits(
                match phase {
                    ForgePhase::Idle => 0,
                    ForgePhase::Working => 1,
                    ForgePhase::Offering => 2,
                },
                2,
            );
            w.u8(by);
            w.bool(weapon.is_some());
            if let Some(weapon) = weapon {
                write_weapon(w, weapon);
            }
            write_option_tick(w, until);
        }
    }
}

fn read_zombies_event(r: &mut BitReader) -> DecodeResult<ZombiesEvent> {
    Ok(match r.bits(ZOMBIES_TAG_BITS)? {
        0 => ZombiesEvent::Round {
            round: r.varint()?,
            phase: match r.bits(2)? {
                0 => ZombiesPhase::Break,
                1 => ZombiesPhase::Fighting,
                2 => ZombiesPhase::Over,
                _ => return Err(DecodeError::Invalid("zombies phase")),
            },
            starts_at: read_option_tick(r)?,
            remaining: r.varint()?,
        },
        1 => ZombiesEvent::Hit {
            zombie: r.u8()?,
            shot: r.varint()?,
            headshot: r.bool()?,
            killed: r.bool()?,
        },
        2 => ZombiesEvent::Points {
            total: r.varint()?,
            change: r.varint_signed()?,
        },
        3 => ZombiesEvent::Boards {
            window: r.u8()?,
            boards: r.u8()?,
        },
        4 => ZombiesEvent::Door {
            door: r.u8()?,
            open: r.bool()?,
        },
        5 => ZombiesEvent::PowerUpDropped {
            id: r.u8()?,
            kind: crate::zombies::PowerUp::from_code(r.bits(2)?)
                .ok_or(DecodeError::Invalid("power-up"))?,
            at: read_finite_vec3(r)?,
        },
        6 => ZombiesEvent::PowerUpGone {
            id: r.u8()?,
            by: if r.bool()? { Some(r.u8()?) } else { None },
        },
        7 => ZombiesEvent::PowerUpTimers {
            insta_kill_until: read_option_tick(r)?,
            double_points_until: read_option_tick(r)?,
        },
        8 => ZombiesEvent::Downed {
            player: r.u8()?,
            bleeds_out_at: read_option_tick(r)?,
        },
        9 => ZombiesEvent::Revive {
            player: r.u8()?,
            by: if r.bool()? { Some(r.u8()?) } else { None },
            done_at: r.u32()?,
        },
        10 => ZombiesEvent::Power { on: r.bool()? },
        11 => ZombiesEvent::Machine {
            machine: r.u8()?,
            available: r.bool()?,
            price: r.varint()?,
            needs_power: r.bool()?,
        },
        13 => ZombiesEvent::Crate {
            spot: r.u8()?,
            phase: CratePhase::from_code(r.bits(3)?).ok_or(DecodeError::Invalid("crate phase"))?,
            by: r.u8()?,
            weapon: if r.bool()? {
                Some(read_weapon(r)?)
            } else {
                None
            },
            until: read_option_tick(r)?,
        },
        14 => ZombiesEvent::Forge {
            phase: match r.bits(2)? {
                0 => ForgePhase::Idle,
                1 => ForgePhase::Working,
                2 => ForgePhase::Offering,
                _ => return Err(DecodeError::Invalid("forge phase")),
            },
            by: r.u8()?,
            weapon: if r.bool()? {
                Some(read_weapon(r)?)
            } else {
                None
            },
            until: read_option_tick(r)?,
        },
        12 => ZombiesEvent::Drank {
            player: r.u8()?,
            drink: crate::zombies::drinks::Drink::from_code(r.bits(3)? as u8)
                .ok_or(DecodeError::Invalid("drink"))?,
        },
        _ => return Err(DecodeError::Invalid("zombies event")),
    })
}

fn write_charge(w: &mut BitWriter, view: &ChargeView) {
    let site_at = |w: &mut BitWriter, tag: u32, site: u8, at: Vec3| {
        w.bits(tag, 3);
        w.bits(u32::from(site), 2);
        write_vec3(w, at);
    };
    match view.status {
        ChargeStatus::Carried { by } => {
            w.bits(0, 3);
            w.bool(by.is_some());
            if let Some(by) = by {
                w.u8(by);
            }
        }
        ChargeStatus::Dropped { at } => {
            w.bits(1, 3);
            write_vec3(w, at);
        }
        ChargeStatus::Planted { site, at } => site_at(w, 2, site, at),
        ChargeStatus::Detonated { site, at } => site_at(w, 3, site, at),
        ChargeStatus::Defused { site, at } => site_at(w, 4, site, at),
    }
    w.bool(view.using.is_some());
    if let Some(u) = view.using {
        w.u8(u.by);
        w.bool(u.kind == UseKind::Defuse);
        w.u32(u.started_at);
        w.u32(u.done_at);
    }
}

fn read_charge(r: &mut BitReader) -> DecodeResult<ChargeView> {
    let tag = r.bits(3)?;
    let status = match tag {
        0 => ChargeStatus::Carried {
            by: if r.bool()? { Some(r.u8()?) } else { None },
        },
        1 => ChargeStatus::Dropped {
            at: read_finite_vec3(r)?,
        },
        2..=4 => {
            let site = r.bits(2)? as u8;
            let at = read_finite_vec3(r)?;
            match tag {
                2 => ChargeStatus::Planted { site, at },
                3 => ChargeStatus::Detonated { site, at },
                _ => ChargeStatus::Defused { site, at },
            }
        }
        _ => return Err(DecodeError::Invalid("charge status")),
    };
    let using = if r.bool()? {
        let by = r.u8()?;
        let kind = if r.bool()? {
            UseKind::Defuse
        } else {
            UseKind::Plant
        };
        let started_at = r.u32()?;
        let done_at = r.u32()?;
        if done_at < started_at {
            return Err(DecodeError::Invalid("charge use"));
        }
        Some(UseView {
            by,
            kind,
            started_at,
            done_at,
        })
    } else {
        None
    };
    Ok(ChargeView { status, using })
}

fn write_cause(w: &mut BitWriter, cause: KillCause) {
    match cause {
        KillCause::Weapon => w.bits(0, 2),
        KillCause::Knife => w.bits(1, 2),
        KillCause::Equipment(kind) => {
            w.bits(2, 2);
            w.bits(u32::from(kind.code()), EQUIPMENT_BITS);
        }
        KillCause::Charge => {
            w.bits(3, 2);
            w.bool(false);
        }
        KillCause::Streak(streak) => {
            w.bits(3, 2);
            w.bool(true);
            write_streak(w, streak);
        }
    }
}

fn read_cause(r: &mut BitReader) -> DecodeResult<KillCause> {
    Ok(match r.bits(2)? {
        0 => KillCause::Weapon,
        1 => KillCause::Knife,
        2 => KillCause::Equipment(read_equipment(r)?),
        _ if r.bool()? => KillCause::Streak(read_streak(r)?),
        _ => KillCause::Charge,
    })
}

fn read_event(r: &mut BitReader) -> DecodeResult<GameEvent> {
    match r.bits(EVENT_TAG_BITS)? {
        0 => Ok(GameEvent::Hit {
            victim: r.u8()?,
            shot: r.varint()?,
            damage: read_finite(r)?,
            headshot: r.bool()?,
            killed: r.bool()?,
        }),
        1 => Ok(GameEvent::Hurt {
            attacker: r.u8()?,
            from: read_finite_vec3(r)?,
            damage: read_finite(r)?,
            killed: r.bool()?,
        }),
        2 => Ok(GameEvent::Kill {
            killer: r.u8()?,
            victim: r.u8()?,
            weapon: read_weapon(r)?,
            headshot: r.bool()?,
            cause: read_cause(r)?,
        }),
        3 => {
            let count = r.varint()? as usize;
            if count > MAX_EVENTS_PER_MESSAGE {
                return Err(DecodeError::Invalid("award count"));
            }
            let mut awards = Vec::with_capacity(count);
            for _ in 0..count {
                awards.push(Scored {
                    award: read_award(r)?,
                    points: r.varint()?,
                });
            }
            Ok(GameEvent::Score {
                awards,
                totals: ScoreTotals {
                    score: r.varint()?,
                    kills: r.varint()?,
                    deaths: r.varint()?,
                    streak: r.varint()?,
                },
            })
        }
        4 => {
            let shot = r.varint()?;
            let tick = read_view_tick(r, None)?.ok_or(DecodeError::Invalid("rewound tick"))?;
            let count = r.varint()? as usize;
            if count > MAX_REWOUND_POSES {
                return Err(DecodeError::Invalid("rewound count"));
            }
            let mut poses = Vec::with_capacity(count);
            for _ in 0..count {
                let id = r.u8()?;
                let pose = HitboxPose {
                    feet: read_finite_vec3(r)?,
                    yaw: read_finite(r)?,
                    pitch: read_finite(r)?,
                    stance: read_stance(r)?,
                    activity: read_activity(r)?,
                };
                poses.push((id, pose));
            }
            Ok(GameEvent::Rewound { shot, tick, poses })
        }
        5 => Ok(GameEvent::Joined {
            id: r.u8()?,
            name: read_string(r, MAX_NAME_BYTES)?,
            team: r.bits(1)? as Team,
            bot: r.bool()?,
        }),
        6 => Ok(GameEvent::Left { id: r.u8()? }),
        7 => {
            let phase = match r.bits(2)? {
                0 => Phase::Lobby,
                1 => Phase::Countdown,
                2 => Phase::Playing,
                _ => Phase::Ended {
                    winner: if r.bool()? { Some(r.u8()?) } else { None },
                },
            };
            let ends_at = read_option_tick(r)?;
            let mut scores = [0; TEAM_COUNT];
            for score in &mut scores {
                *score = r.varint()?;
            }
            let score_limit = r.varint()?;
            let period = match r.bits(2)? {
                0 => Period::Regulation,
                1 => Period::SecondHalf,
                2 => Period::Overtime { timed: r.bool()? },
                _ => return Err(DecodeError::Invalid("period")),
            };
            Ok(GameEvent::Match {
                phase,
                ends_at,
                scores,
                score_limit,
                period,
            })
        }
        8 => {
            let count = r.varint()? as usize;
            if count > MAX_SCOREBOARD_ROWS {
                return Err(DecodeError::Invalid("scoreboard rows"));
            }
            let mut rows = Vec::with_capacity(count);
            for _ in 0..count {
                rows.push(ScoreRow {
                    id: r.u8()?,
                    score: r.varint()?,
                    kills: r.varint()?,
                    deaths: r.varint()?,
                    ping_ms: r.u16()?,
                    ready: r.bool()?,
                    level: r.u8()?,
                    level_kills: r.u8()?,
                    captures: r.varint()?,
                    returns: r.varint()?,
                    hill_seconds: r.varint()?,
                    defends: r.varint()?,
                    alive: r.bool()?,
                    revives: r.varint()?,
                    headshots: r.varint()?,
                });
            }
            Ok(GameEvent::Scoreboard { rows })
        }
        9 => Ok(GameEvent::Spawned {
            yaw: read_finite(r)?,
        }),
        10 => {
            let mut read_flag = || -> DecodeResult<FlagView> {
                let home = read_finite_vec3(r)?;
                let status = match r.bits(2)? {
                    0 => FlagStatus::Home,
                    1 => FlagStatus::Carried { by: r.u8()? },
                    2 => FlagStatus::Dropped {
                        at: read_finite_vec3(r)?,
                        returns_at: r.varint()?,
                    },
                    _ => return Err(DecodeError::Invalid("flag status")),
                };
                Ok(FlagView { home, status })
            };
            Ok(GameEvent::Flags {
                flags: [read_flag()?, read_flag()?],
            })
        }
        11 => {
            let flag = r.bits(1)? as Team;
            let play = match r.bits(2)? {
                0 => FlagPlay::Taken,
                1 => FlagPlay::Dropped,
                2 => FlagPlay::Returned,
                _ => FlagPlay::Captured,
            };
            let player = if r.bool()? { Some(r.u8()?) } else { None };
            Ok(GameEvent::FlagPlay { flag, play, player })
        }
        12 => {
            let count = r.varint()? as usize;
            if count > MAX_HARDPOINT_ZONES {
                return Err(DecodeError::Invalid("zone count"));
            }
            let mut zones = Vec::with_capacity(count);
            for _ in 0..count {
                zones.push(ZoneView {
                    name: read_string(r, MAX_ZONE_NAME_BYTES)?,
                    center: read_finite_vec3(r)?,
                    half_extents: read_finite_vec3(r)?,
                });
            }
            Ok(GameEvent::HardpointZones { zones })
        }
        13 => Ok(GameEvent::Hardpoint {
            active: r.u8()?,
            holder: match r.bits(2)? {
                0 => Holder::Neutral,
                1 => Holder::Held(r.bits(1)? as Team),
                2 => Holder::Contested,
                _ => return Err(DecodeError::Invalid("holder")),
            },
            moves_at: read_option_tick(r)?,
        }),
        14 => {
            let id = u16::try_from(r.varint()?).map_err(|_| DecodeError::Invalid("prop id"))?;
            Ok(GameEvent::PropState {
                id,
                stage: r.u8()?,
                impulse: read_finite_vec3(r)?,
            })
        }
        15 => {
            let from = r.u8()?;
            let team = r.bool()?;
            Ok(GameEvent::Chat {
                from,
                text: read_string(r, chat::MAX_BYTES)?,
                team,
            })
        }
        16 => {
            let number = r.varint()?;
            let phase = match r.bits(2)? {
                0 => RoundPhase::Freeze,
                1 => RoundPhase::Live,
                2 => RoundPhase::Over {
                    winner: r.bits(1)? as Team,
                    reason: match r.bits(2)? {
                        0 => RoundEnd::Eliminated,
                        1 => RoundEnd::Time,
                        2 => RoundEnd::Detonated,
                        _ => RoundEnd::Defused,
                    },
                },
                _ => return Err(DecodeError::Invalid("round phase")),
            };
            Ok(GameEvent::Round {
                number,
                phase,
                ends_at: r.u32()?,
                attackers: r.bits(1)? as Team,
                halftime_after: r.varint()?,
            })
        }
        17 => Ok(GameEvent::Blast {
            owner: if r.bool()? { Some(r.u8()?) } else { None },
            shot: r.varint()?,
            kind: read_equipment(r)?,
            at: read_finite_vec3(r)?,
        }),
        18 => {
            let kind = killcam::Kind::from_code(r.bits(2)? as u8)
                .ok_or(DecodeError::Invalid("killcam kind"))?;
            let killer = r.u8()?;
            let victim = r.u8()?;
            let kill_tick = r.u32()?;
            let before = r.u16()?;
            let after = r.u16()?;
            if u32::from(before) + u32::from(after) > killcam::MAX_TICKS {
                return Err(DecodeError::Invalid("killcam length"));
            }
            Ok(GameEvent::Killcam(Killcam {
                kind,
                killer,
                victim,
                kill_tick,
                before,
                after,
                rewind: r.u8()?,
                weapon: read_weapon(r)?,
                cause: read_cause(r)?,
                headshot: r.bool()?,
            }))
        }
        19 => {
            let count = r.varint()? as usize;
            if count > MAX_CHARGE_SITES {
                return Err(DecodeError::Invalid("site count"));
            }
            let mut sites = Vec::with_capacity(count);
            for _ in 0..count {
                sites.push(ZoneView {
                    name: read_string(r, MAX_ZONE_NAME_BYTES)?,
                    center: read_finite_vec3(r)?,
                    half_extents: read_finite_vec3(r)?,
                });
            }
            Ok(GameEvent::ChargeSites { sites })
        }
        20 => Ok(GameEvent::Charge(read_charge(r)?)),
        21 => {
            let count = r.varint()? as usize;
            if count > MAX_DOMINATION_FLAGS {
                return Err(DecodeError::Invalid("flag count"));
            }
            let mut flags = Vec::with_capacity(count);
            for _ in 0..count {
                flags.push(FlagPointView {
                    name: read_string(r, MAX_ZONE_NAME_BYTES)?,
                    center: read_finite_vec3(r)?,
                    radius: read_finite(r)?,
                    half_height: read_finite(r)?,
                });
            }
            Ok(GameEvent::DominationFlags { flags })
        }
        22 => {
            let tick = r.varint()?;
            let swapped = r.bool()?;
            let count = r.varint()? as usize;
            if count > MAX_DOMINATION_FLAGS {
                return Err(DecodeError::Invalid("flag count"));
            }
            let mut flags = Vec::with_capacity(count);
            for _ in 0..count {
                flags.push(FlagPointStatus {
                    owner: read_option_team(r)?,
                    claim: read_option_team(r)?,
                    progress: r.u8()?,
                    contested: r.bool()?,
                    capturers: r.u8()?,
                });
            }
            Ok(GameEvent::Domination(DominationView {
                tick,
                flags,
                swapped,
            }))
        }
        23 => Ok(GameEvent::Scavenger(match r.bits(2)? {
            0 => ScavengerEvent::Dropped {
                id: r.u16()?,
                at: read_finite_vec3(r)?,
                dropped_by: r.u8()?,
            },
            1 => ScavengerEvent::Taken {
                id: r.u16()?,
                by: r.u8()?,
            },
            2 => ScavengerEvent::Gone { id: r.u16()? },
            _ => return Err(DecodeError::Invalid("scavenger event")),
        })),
        ZOMBIES_EVENT_TAG => Ok(GameEvent::Zombies(read_zombies_event(r)?)),
        STREAK_EVENT_TAG => Ok(GameEvent::Streak(read_streak_event(r)?)),
        STATS_EVENT_TAG => {
            let len = r.varint()? as usize;
            if len > stats::MAX_MATCH_KEYS {
                return Err(DecodeError::Invalid("stats length"));
            }
            let mut out = Stats::new();
            for _ in 0..len {
                let key = read_string(r, stats::MAX_KEY_LEN)?;
                if !stats::valid_key(&key) {
                    return Err(DecodeError::Invalid("stat key"));
                }
                out.insert(key, r.varint()?);
            }
            Ok(GameEvent::Stats(out))
        }
        _ => Err(DecodeError::Invalid("event tag")),
    }
}

const STREAK_EVENT_TAG: u32 = 24;
const STATS_EVENT_TAG: u32 = 25;
/// Bits for a [`StreakEvent`]'s tag; room for sixteen.
const STREAK_TAG_BITS: u32 = 4;
/// Most streaks one [`StreakEvent::State`] lists, and airborne streaks one
/// [`StreakEvent::Sky`] does.
const MAX_HELD_STREAKS: usize = streak::MAX_HELD;
const MAX_AIRBORNE: usize = 32;

fn write_streak(w: &mut BitWriter, streak: Streak) {
    w.bits(u32::from(streak.code()), STREAK_BITS);
}

fn read_streak(r: &mut BitReader) -> DecodeResult<Streak> {
    Streak::from_code(r.bits(STREAK_BITS)? as u8).ok_or(DecodeError::Invalid("streak"))
}

fn write_loadout(w: &mut BitWriter, loadout: streak::Loadout) {
    for slot in loadout.0 {
        match slot {
            Some(streak) => {
                w.bool(true);
                write_streak(w, streak);
            }
            None => w.bool(false),
        }
    }
}

fn read_loadout(r: &mut BitReader) -> DecodeResult<streak::Loadout> {
    let mut slots = [None; 3];
    for slot in &mut slots {
        if r.bool()? {
            *slot = Some(read_streak(r)?);
        }
    }
    Ok(streak::Loadout(slots))
}

fn write_streak_event(w: &mut BitWriter, event: &StreakEvent) {
    match event {
        StreakEvent::State {
            progress,
            held,
            loadout,
            roofed,
        } => {
            w.bits(0, STREAK_TAG_BITS);
            w.varint(*progress);
            let count = held.len().min(MAX_HELD_STREAKS);
            w.varint(count as u32);
            for streak in &held[..count] {
                write_streak(w, *streak);
            }
            write_loadout(w, *loadout);
            w.bool(*roofed);
        }
        StreakEvent::CrateIncoming {
            id,
            owner,
            at,
            heading,
            drop_at,
            lands_at,
            contents,
        } => {
            w.bits(2, STREAK_TAG_BITS);
            w.u16(*id);
            w.u8(*owner);
            write_vec3(w, *at);
            w.f32(*heading);
            w.u32(*drop_at);
            w.u32(*lands_at);
            write_streak(w, *contents);
        }
        StreakEvent::CrateCapture {
            id,
            by,
            started,
            done_at,
        } => {
            w.bits(3, STREAK_TAG_BITS);
            w.u16(*id);
            match by {
                Some(by) => {
                    w.bool(true);
                    w.u8(*by);
                }
                None => w.bool(false),
            }
            w.u32(*started);
            w.u32(*done_at);
        }
        StreakEvent::CrateTaken { id, by, contents } => {
            w.bits(4, STREAK_TAG_BITS);
            w.u16(*id);
            w.u8(*by);
            write_streak(w, *contents);
        }
        StreakEvent::CrateGone { id } => {
            w.bits(5, STREAK_TAG_BITS);
            w.u16(*id);
        }
        StreakEvent::Airstrike { owner, strikes } => {
            w.bits(6, STREAK_TAG_BITS);
            w.u8(*owner);
            let count = strikes.len().min(streak::AIRSTRIKE_TARGETS);
            w.varint(count as u32);
            for (at, tick) in &strikes[..count] {
                write_vec3(w, *at);
                w.u32(*tick);
            }
        }
        StreakEvent::Strike { owner, at } => {
            w.bits(7, STREAK_TAG_BITS);
            w.u8(*owner);
            write_vec3(w, *at);
        }
        StreakEvent::Sky { orbit, up } => {
            w.bits(1, STREAK_TAG_BITS);
            write_vec3(w, orbit.centre);
            w.f32(orbit.radius);
            let count = up.len().min(MAX_AIRBORNE);
            w.varint(count as u32);
            for air in &up[..count] {
                w.u16(air.id);
                write_streak(w, air.streak);
                w.u8(air.owner);
                w.u8(air.side);
                w.u32(air.from);
                w.u16(air.phase);
                w.u32(air.until);
            }
        }
        StreakEvent::Downed {
            id,
            streak,
            owner,
            by,
            at,
        } => {
            w.bits(8, STREAK_TAG_BITS);
            w.u16(*id);
            write_streak(w, *streak);
            w.u8(*owner);
            w.u8(*by);
            write_vec3(w, *at);
        }
        StreakEvent::Turret(t) => {
            w.bits(9, STREAK_TAG_BITS);
            w.u16(t.id);
            w.u8(t.owner);
            w.u8(t.side);
            write_vec3(w, t.at);
            w.f32(t.facing);
            w.u32(t.placed);
            w.u32(t.until);
        }
        StreakEvent::TurretTarget { id, target } => {
            w.bits(10, STREAK_TAG_BITS);
            w.u16(*id);
            write_option_u8(w, *target);
        }
        StreakEvent::TurretGone { id, by } => {
            w.bits(11, STREAK_TAG_BITS);
            w.u16(*id);
            write_option_u8(w, *by);
        }
    }
}

fn read_streak_event(r: &mut BitReader) -> DecodeResult<StreakEvent> {
    match r.bits(STREAK_TAG_BITS)? {
        0 => {
            let progress = r.varint()?;
            let count = r.varint()? as usize;
            if count > MAX_HELD_STREAKS {
                return Err(DecodeError::Invalid("held streaks"));
            }
            let mut held = Vec::with_capacity(count);
            for _ in 0..count {
                held.push(read_streak(r)?);
            }
            Ok(StreakEvent::State {
                progress,
                held,
                loadout: read_loadout(r)?,
                roofed: r.bool()?,
            })
        }
        1 => {
            let orbit = streak::Orbit {
                centre: read_finite_vec3(r)?,
                radius: read_finite(r)?,
            };
            let count = r.varint()? as usize;
            if count > MAX_AIRBORNE {
                return Err(DecodeError::Invalid("airborne streaks"));
            }
            let mut up = Vec::with_capacity(count);
            for _ in 0..count {
                up.push(AirView {
                    id: r.u16()?,
                    streak: read_streak(r)?,
                    owner: r.u8()?,
                    side: r.u8()?,
                    from: r.u32()?,
                    phase: r.u16()?,
                    until: r.u32()?,
                });
            }
            Ok(StreakEvent::Sky { orbit, up })
        }
        2 => Ok(StreakEvent::CrateIncoming {
            id: r.u16()?,
            owner: r.u8()?,
            at: read_finite_vec3(r)?,
            heading: read_finite(r)?,
            drop_at: r.u32()?,
            lands_at: r.u32()?,
            contents: read_streak(r)?,
        }),
        3 => Ok(StreakEvent::CrateCapture {
            id: r.u16()?,
            by: if r.bool()? { Some(r.u8()?) } else { None },
            started: r.u32()?,
            done_at: r.u32()?,
        }),
        4 => Ok(StreakEvent::CrateTaken {
            id: r.u16()?,
            by: r.u8()?,
            contents: read_streak(r)?,
        }),
        5 => Ok(StreakEvent::CrateGone { id: r.u16()? }),
        6 => {
            let owner = r.u8()?;
            let count = r.varint()? as usize;
            if count > streak::AIRSTRIKE_TARGETS {
                return Err(DecodeError::Invalid("strikes"));
            }
            let mut strikes = Vec::with_capacity(count);
            for _ in 0..count {
                strikes.push((read_finite_vec3(r)?, r.u32()?));
            }
            Ok(StreakEvent::Airstrike { owner, strikes })
        }
        7 => Ok(StreakEvent::Strike {
            owner: r.u8()?,
            at: read_finite_vec3(r)?,
        }),
        8 => Ok(StreakEvent::Downed {
            id: r.u16()?,
            streak: read_streak(r)?,
            owner: r.u8()?,
            by: r.u8()?,
            at: read_finite_vec3(r)?,
        }),
        9 => Ok(StreakEvent::Turret(TurretView {
            id: r.u16()?,
            owner: r.u8()?,
            side: r.u8()?,
            at: read_finite_vec3(r)?,
            facing: read_finite(r)?,
            placed: r.u32()?,
            until: r.u32()?,
        })),
        10 => Ok(StreakEvent::TurretTarget {
            id: r.u16()?,
            target: read_option_u8(r)?,
        }),
        11 => Ok(StreakEvent::TurretGone {
            id: r.u16()?,
            by: read_option_u8(r)?,
        }),
        _ => Err(DecodeError::Invalid("streak event")),
    }
}

/// Bits for a mode's tag; room for eight modes.
const MODE_TAG_BITS: u32 = 3;

fn write_mode(w: &mut BitWriter, mode: &Mode) {
    match mode {
        Mode::TeamDeathmatch => w.bits(0, MODE_TAG_BITS),
        Mode::GunGame(rules) => {
            w.bits(1, MODE_TAG_BITS);
            w.u8(rules.kills_per_weapon);
        }
        Mode::CaptureTheFlag => w.bits(2, MODE_TAG_BITS),
        Mode::Hardpoint => w.bits(3, MODE_TAG_BITS),
        Mode::SearchAndDestroy => w.bits(4, MODE_TAG_BITS),
        Mode::FreeForAll => w.bits(5, MODE_TAG_BITS),
        Mode::Domination => w.bits(6, MODE_TAG_BITS),
        // At the top of the range, so new modes fill in below it.
        Mode::Zombies => w.bits(7, MODE_TAG_BITS),
    }
}

fn read_mode(r: &mut BitReader) -> DecodeResult<Mode> {
    match r.bits(MODE_TAG_BITS)? {
        0 => Ok(Mode::TeamDeathmatch),
        1 => Ok(Mode::GunGame(gun_game::Rules {
            kills_per_weapon: r.u8()?.max(1),
        })),
        2 => Ok(Mode::CaptureTheFlag),
        3 => Ok(Mode::Hardpoint),
        4 => Ok(Mode::SearchAndDestroy),
        5 => Ok(Mode::FreeForAll),
        6 => Ok(Mode::Domination),
        7 => Ok(Mode::Zombies),
        _ => Err(DecodeError::Invalid("mode")),
    }
}

const AWARD_TAG_BITS: u32 = 5;

fn write_award(w: &mut BitWriter, award: Award) {
    let (tag, count) = match award {
        Award::Kill => (0, None),
        Award::Headshot => (1, None),
        Award::Longshot => (2, None),
        Award::PointBlank => (3, None),
        Award::Collateral => (4, None),
        Award::MultiKill(n) => (5, Some(n)),
        Award::Streak(n) => (6, Some(n)),
        Award::FlagTaken => (7, None),
        Award::FlagCapture => (8, None),
        Award::FlagReturn => (9, None),
        Award::CarrierKill => (10, None),
        Award::HardpointCapture => (11, None),
        Award::HardpointSecure => (12, None),
        Award::HardpointDefend => (13, None),
        Award::HardpointAssault => (14, None),
        Award::ChargePlanted => (15, None),
        Award::ChargeDefused => (16, None),
        Award::WireCutter => (17, None),
        Award::Clutch => (18, None),
        Award::Ace => (19, None),
        Award::DominationCapture => (20, None),
        Award::DominationDefend => (21, None),
        Award::DominationAssault => (22, None),
        Award::ShotDown(streak) => (23, Some(u32::from(streak.code()))),
    };
    w.bits(tag, AWARD_TAG_BITS);
    if let Some(n) = count {
        w.varint(n);
    }
}

fn read_award(r: &mut BitReader) -> DecodeResult<Award> {
    Ok(match r.bits(AWARD_TAG_BITS)? {
        0 => Award::Kill,
        1 => Award::Headshot,
        2 => Award::Longshot,
        3 => Award::PointBlank,
        4 => Award::Collateral,
        5 => Award::MultiKill(r.varint()?),
        6 => Award::Streak(r.varint()?),
        7 => Award::FlagTaken,
        8 => Award::FlagCapture,
        9 => Award::FlagReturn,
        10 => Award::CarrierKill,
        11 => Award::HardpointCapture,
        12 => Award::HardpointSecure,
        13 => Award::HardpointDefend,
        14 => Award::HardpointAssault,
        15 => Award::ChargePlanted,
        16 => Award::ChargeDefused,
        17 => Award::WireCutter,
        18 => Award::Clutch,
        19 => Award::Ace,
        20 => Award::DominationCapture,
        21 => Award::DominationDefend,
        22 => Award::DominationAssault,
        23 => Award::ShotDown(
            u8::try_from(r.varint()?)
                .ok()
                .and_then(Streak::from_code)
                .ok_or(DecodeError::Invalid("award"))?,
        ),
        _ => return Err(DecodeError::Invalid("award")),
    })
}

fn write_option_u8(w: &mut BitWriter, value: Option<u8>) {
    w.bool(value.is_some());
    if let Some(v) = value {
        w.u8(v);
    }
}

fn read_option_u8(r: &mut BitReader) -> DecodeResult<Option<u8>> {
    Ok(if r.bool()? { Some(r.u8()?) } else { None })
}

fn read_finite(r: &mut BitReader) -> DecodeResult<f32> {
    let v = r.f32()?;
    if v.is_finite() {
        Ok(v)
    } else {
        Err(DecodeError::Invalid("non-finite number"))
    }
}

fn read_finite_vec3(r: &mut BitReader) -> DecodeResult<Vec3> {
    Ok(Vec3::new(read_finite(r)?, read_finite(r)?, read_finite(r)?))
}

fn write_snapshot(w: &mut BitWriter, s: &Snapshot, baseline: Baseline) {
    let zombie_base = baseline.zombies;
    w.u32(s.tick);
    match s.baseline {
        // Baselines are recent, so send the distance back instead of the tick.
        Some(base) => {
            w.bool(true);
            w.varint(s.tick.wrapping_sub(base));
        }
        None => w.bool(false),
    }
    let baseline = if s.baseline.is_some() {
        baseline
    } else {
        Baseline::default()
    };
    match &s.own {
        Some(own) => {
            w.bool(true);
            w.varint(own.last_input);
            // The own state as changes from the baseline's, field by field.
            w.bool(baseline.own.is_some());
            match baseline.own {
                Some(base) => {
                    let fields = delta::record(|r| write_predicted(r, base));
                    write_predicted(&mut DeltaWriter::new(w, &fields), &own.predicted);
                }
                None => write_predicted(w, &own.predicted),
            }
        }
        None => w.bool(false),
    }
    let baseline = baseline.players;
    w.varint(s.players.len() as u32);
    for view in &s.players {
        w.u8(view.id);
        let base = baseline
            .iter()
            .find(|b| b.id == view.id)
            .copied()
            .unwrap_or(PlayerView {
                id: view.id,
                ..PlayerView::default()
            });
        write_view_delta(w, view, &base);
    }
    let count = s.grenades.len().min(MAX_GRENADES);
    w.varint(count as u32);
    for g in &s.grenades[..count] {
        w.u8(g.owner);
        w.varint(g.shot);
        w.bits(u32::from(g.kind.code()), EQUIPMENT_BITS);
        w.varint_signed(g.position.x);
        w.varint_signed(g.position.y);
        w.varint_signed(g.position.z);
        w.bool(g.resting);
    }
    let count = s.smokes.len().min(MAX_SMOKES);
    w.varint(count as u32);
    for smoke in &s.smokes[..count] {
        match smoke.owner {
            Some(id) => {
                w.bool(true);
                w.u8(id);
            }
            None => w.bool(false),
        }
        w.varint(smoke.shot);
        w.varint_signed(smoke.position.x);
        w.varint_signed(smoke.position.y);
        w.varint_signed(smoke.position.z);
        w.varint(smoke.age);
    }
    match &s.zombies {
        Some(zombies) => {
            w.bool(true);
            // As changes from the horde the client last acknowledged.
            w.bool(zombie_base.is_some());
            let base: &[ZombieView] = match zombie_base {
                Some((tick, base)) => {
                    w.varint(s.tick.wrapping_sub(tick));
                    base
                }
                None => &[],
            };
            let count = zombies.len().min(MAX_ZOMBIES);
            w.varint(count as u32);
            let mut next = 0u8;
            for z in &zombies[..count] {
                // Ids mostly run in order: a bit for the next one.
                w.bool(z.id == next);
                if z.id != next {
                    w.u8(z.id);
                }
                next = z.id.wrapping_add(1);
                let b = zombie_or_new(base, z.id);
                let changed = *z != b;
                w.bool(changed);
                if changed {
                    let fields = delta::record(|r| write_zombie(r, &b));
                    write_zombie(&mut DeltaWriter::new(w, &fields), z);
                }
            }
        }
        None => w.bool(false),
    }
    w.bool(s.radar.jammed);
    let count = s.radar.blips.len().min(radar::MAX_BLIPS);
    w.varint(count as u32);
    for blip in &s.radar.blips[..count] {
        w.u8(blip.id);
        w.varint_signed(i32::from(blip.x));
        w.varint_signed(i32::from(blip.z));
        w.varint(blip.age);
        w.bits(blip.kind as u32, BLIP_KIND_BITS);
    }
}

const BLIP_KIND_BITS: u32 = 2;

fn read_blip_kind(r: &mut BitReader) -> DecodeResult<radar::BlipKind> {
    match r.bits(BLIP_KIND_BITS)? {
        0 => Ok(radar::BlipKind::Fire),
        1 => Ok(radar::BlipKind::Sweep),
        _ => Err(DecodeError::Invalid("blip kind")),
    }
}

fn read_radar(r: &mut BitReader) -> DecodeResult<RadarView> {
    let jammed = r.bool()?;
    let count = r.varint()? as usize;
    if count > radar::MAX_BLIPS {
        return Err(DecodeError::Invalid("blip count"));
    }
    let mut blips = Vec::with_capacity(count);
    for _ in 0..count {
        let id = r.u8()?;
        let x = i16::try_from(r.varint_signed()?).map_err(|_| DecodeError::Invalid("blip x"))?;
        let z = i16::try_from(r.varint_signed()?).map_err(|_| DecodeError::Invalid("blip z"))?;
        blips.push(BlipView {
            id,
            x,
            z,
            age: r.varint()?,
            kind: read_blip_kind(r)?,
        });
    }
    Ok(RadarView { jammed, blips })
}

/// Zombie `id` in `base`, or a zombie never seen, to encode against.
fn zombie_or_new(base: &[ZombieView], id: u8) -> ZombieView {
    base.iter()
        .find(|b| b.id == id)
        .copied()
        .unwrap_or(ZombieView {
            id,
            ..ZombieView::default()
        })
}

/// Every field of a zombie but its id, written against [`BitSink`] so a
/// horde goes as changes.
fn write_zombie(w: &mut impl BitSink, z: &ZombieView) {
    let counter = (1 << ZOMBIE_COUNTER_BITS) - 1;
    for axis in 0..3 {
        w.number(z.position[axis] as u32, 32);
    }
    w.u8(z.yaw);
    // A zombie never hit has no latest hit to describe.
    let hit = z.hits != 0 || z.hit_heading != 0 || z.hit_part != 0;
    // Whether it is alive or was hit, and both counters, as one field.
    let flags = u32::from(z.alive)
        | u32::from(hit) << 1
        | u32::from(z.swings & counter) << 2
        | u32::from(z.hits & counter) << (2 + ZOMBIE_COUNTER_BITS);
    w.bits(flags, 2 + 2 * ZOMBIE_COUNTER_BITS);
    if hit {
        w.u8(z.hit_heading);
        w.u8(z.hit_part);
    }
}

fn read_zombie(r: &mut impl BitSource, id: u8) -> DecodeResult<ZombieView> {
    let mut position = IVec3::ZERO;
    for axis in 0..3 {
        position[axis] = r.number(32)? as i32;
    }
    let yaw = r.u8()?;
    let counter = (1 << ZOMBIE_COUNTER_BITS) - 1;
    let flags = r.bits(2 + 2 * ZOMBIE_COUNTER_BITS)?;
    let alive = flags & 1 != 0;
    let swings = ((flags >> 2) & counter) as u8;
    let hits = ((flags >> (2 + ZOMBIE_COUNTER_BITS)) & counter) as u8;
    let (hit_heading, hit_part) = if flags & 2 != 0 {
        (r.u8()?, r.u8()?)
    } else {
        (0, 0)
    };
    Ok(ZombieView {
        id,
        position,
        yaw,
        alive,
        swings,
        hits,
        hit_heading,
        hit_part,
    })
}

fn read_snapshot<'a>(
    r: &mut BitReader,
    lookup: impl Fn(Tick) -> Option<Baseline<'a>>,
) -> DecodeResult<Snapshot> {
    let tick = r.u32()?;
    let base_tick = if r.bool()? {
        Some(tick.wrapping_sub(r.varint()?))
    } else {
        None
    };
    let baseline = match base_tick {
        Some(t) => lookup(t).ok_or(DecodeError::Invalid("unknown baseline"))?,
        None => Baseline::default(),
    };
    let own = if r.bool()? {
        let last_input = r.varint()?;
        let predicted = if r.bool()? {
            let base = baseline
                .own
                .ok_or(DecodeError::Invalid("unknown own baseline"))?;
            let fields = delta::record(|w| write_predicted(w, base));
            read_predicted(&mut DeltaReader::new(r, &fields))?
        } else {
            read_predicted(r)?
        };
        Some(Box::new(OwnState {
            last_input,
            predicted,
        }))
    } else {
        None
    };
    let base = baseline.players;
    let count = r.varint()? as usize;
    if count > 255 {
        return Err(DecodeError::Invalid("player count"));
    }
    let mut players = Vec::with_capacity(count);
    for _ in 0..count {
        let id = r.u8()?;
        let b = base
            .iter()
            .find(|b| b.id == id)
            .copied()
            .unwrap_or(PlayerView {
                id,
                ..PlayerView::default()
            });
        players.push(read_view_delta(r, &b)?);
    }
    let count = r.varint()? as usize;
    if count > MAX_GRENADES {
        return Err(DecodeError::Invalid("grenade count"));
    }
    let mut grenades = Vec::with_capacity(count);
    for _ in 0..count {
        grenades.push(GrenadeView {
            owner: r.u8()?,
            shot: r.varint()?,
            kind: read_equipment(r)?,
            position: IVec3::new(r.varint_signed()?, r.varint_signed()?, r.varint_signed()?),
            resting: r.bool()?,
        });
    }
    let count = r.varint()? as usize;
    if count > MAX_SMOKES {
        return Err(DecodeError::Invalid("smoke count"));
    }
    let mut smokes = Vec::with_capacity(count);
    for _ in 0..count {
        smokes.push(SmokeView {
            owner: if r.bool()? { Some(r.u8()?) } else { None },
            shot: r.varint()?,
            position: IVec3::new(r.varint_signed()?, r.varint_signed()?, r.varint_signed()?),
            age: r.varint()?,
        });
    }
    let zombies = if r.bool()? {
        let base: &[ZombieView] = if r.bool()? {
            let at = tick.wrapping_sub(r.varint()?);
            lookup(at)
                .and_then(|b| b.zombies)
                .ok_or(DecodeError::Invalid("unknown zombie baseline"))?
                .1
        } else {
            &[]
        };
        let count = r.varint()? as usize;
        if count > MAX_ZOMBIES {
            return Err(DecodeError::Invalid("zombie count"));
        }
        let mut zombies = Vec::with_capacity(count);
        let mut next = 0u8;
        for _ in 0..count {
            let id = if r.bool()? { next } else { r.u8()? };
            next = id.wrapping_add(1);
            let b = zombie_or_new(base, id);
            let z = if r.bool()? {
                let fields = delta::record(|w| write_zombie(w, &b));
                read_zombie(&mut DeltaReader::new(r, &fields), id)?
            } else {
                b
            };
            zombies.push(z);
        }
        Some(zombies)
    } else {
        None
    };
    let radar = read_radar(r)?;
    Ok(Snapshot {
        tick,
        baseline: base_tick,
        own,
        players,
        grenades,
        smokes,
        zombies,
        radar,
    })
}

/// Lunge offsets travel in millimetres.
const LUNGE_SCALE: f32 = 1000.0;

/// A [`PlayerView`] as changes from `base` (`net::delta`): one bit when
/// nothing changed, otherwise each field against the baseline's. A field
/// added to [`write_view`] and [`read_view`] is delta-compressed with them.
fn write_view_delta(w: &mut BitWriter, v: &PlayerView, base: &PlayerView) {
    let changed = v != base;
    w.bool(changed);
    if changed {
        let fields = delta::record(|r| write_view(r, base));
        write_view(&mut DeltaWriter::new(w, &fields), v);
    }
}

fn read_view_delta(r: &mut BitReader, base: &PlayerView) -> DecodeResult<PlayerView> {
    if !r.bool()? {
        return Ok(*base);
    }
    let fields = delta::record(|w| write_view(w, base));
    let view = read_view(&mut DeltaReader::new(r, &fields))?;
    Ok(PlayerView {
        id: base.id,
        ..view
    })
}

/// Bits of a player view's flag mask.
const VIEW_FLAG_BITS: u32 = 12;

/// Every field of a [`PlayerView`] but its id. Positions, angles, and
/// counters are numbers, so they change by their difference; the flags are
/// one field, so they cost a bit while none of them changes.
fn write_view(w: &mut impl BitSink, v: &PlayerView) {
    for axis in 0..3 {
        w.number(v.position[axis] as u32, 32);
    }
    w.number(u32::from(v.yaw), 16);
    w.number(u32::from(v.pitch), 16);
    let mut mask = 0;
    for (bit, flag) in [
        v.sprinting,
        v.on_floor,
        v.reloading,
        v.reload_empty,
        v.alive,
        v.mantling,
        v.vaulting,
        v.meleeing,
        v.defusing,
        v.downed,
        v.quiet,
        v.ladder.is_some(),
    ]
    .into_iter()
    .enumerate()
    {
        mask |= u32::from(flag) << bit;
    }
    w.bits(mask, VIEW_FLAG_BITS);
    write_stance(w, v.stance);
    w.u8(v.crouch);
    w.u8(v.ads);
    write_weapon(w, v.weapon);
    if let Some(heading) = v.ladder {
        w.u8(heading);
    }
    w.u8(v.mount);
    w.u8(v.throw);
    w.bits(u32::from(v.throw_kind.code()), EQUIPMENT_BITS);
    w.u8(v.charge_use);
    w.number(v.shots_fired, 32);
    w.u8(v.hits);
    w.u8(v.hit_heading);
    w.u8(v.hit_part);
    w.u8(v.stride);
    w.u8(v.reload_progress);
    w.u8(v.mantle_progress);
    w.u8(v.turn);
    w.u8(v.step);
}

fn read_view(r: &mut impl BitSource) -> DecodeResult<PlayerView> {
    let mut position = IVec3::ZERO;
    for axis in 0..3 {
        position[axis] = r.number(32)? as i32;
    }
    let yaw = r.number(16)? as u16;
    let pitch = r.number(16)? as u16;
    let mask = r.bits(VIEW_FLAG_BITS)?;
    let flag = |bit: u32| mask & (1 << bit) != 0;
    let stance = read_stance(r)?;
    let crouch = r.u8()?;
    let ads = r.u8()?;
    let weapon = read_weapon(r)?;
    let ladder = if flag(11) { Some(r.u8()?) } else { None };
    Ok(PlayerView {
        id: 0,
        position,
        yaw,
        pitch,
        stance,
        crouch,
        sprinting: flag(0),
        on_floor: flag(1),
        ads,
        weapon,
        reloading: flag(2),
        reload_empty: flag(3),
        alive: flag(4),
        mantling: flag(5),
        vaulting: flag(6),
        meleeing: flag(7),
        defusing: flag(8),
        downed: flag(9),
        quiet: flag(10),
        ladder,
        mount: r.u8()?,
        throw: r.u8()?,
        throw_kind: read_equipment(r)?,
        charge_use: r.u8()?,
        shots_fired: r.number(32)?,
        hits: r.u8()?,
        hit_heading: r.u8()?,
        hit_part: r.u8()?,
        stride: r.u8()?,
        reload_progress: r.u8()?,
        mantle_progress: r.u8()?,
        turn: r.u8()?,
        step: r.u8()?,
    })
}

// ---------------------------------------------------------------------------
// Full-precision state for prediction

fn write_predicted(w: &mut impl BitSink, p: &Predicted) {
    write_vec3(w, p.position);
    w.bool(p.on_floor);
    let s = &p.state;
    w.bool(s.reload_held);
    w.bool(s.melee_held);
    w.bool(s.lethal_held);
    w.bool(s.tactical_held);
    w.bool(s.downed);
    w.bits(u32::from(s.drinks.bits()), Drinks::BITS);
    w.f32(s.health);
    w.f32(s.since_damage);
    w.f32(s.fall_speed);
    w.bool(s.airborne);
    w.bits(u32::from(s.perks.bits()), Perks::BITS);
    write_move_state(w, &s.movement);
    write_combat_state(w, &s.combat);
}

fn read_predicted(r: &mut impl BitSource) -> DecodeResult<Predicted> {
    let position = read_vec3(r)?;
    let on_floor = r.bool()?;
    let reload_held = r.bool()?;
    let melee_held = r.bool()?;
    let lethal_held = r.bool()?;
    let tactical_held = r.bool()?;
    let downed = r.bool()?;
    let drinks =
        Drinks::from_bits(r.bits(Drinks::BITS)? as u8).ok_or(DecodeError::Invalid("drinks"))?;
    let health = r.f32()?;
    let since_damage = r.f32()?;
    let fall_speed = r.f32()?;
    let airborne = r.bool()?;
    let perks = read_perks(r)?;
    let movement = read_move_state(r)?;
    let combat = read_combat_state(r)?;
    Ok(Predicted {
        state: PlayerState {
            movement,
            combat,
            health,
            since_damage,
            fall_speed,
            airborne,
            reload_held,
            melee_held,
            lethal_held,
            tactical_held,
            downed,
            perks,
            drinks,
        },
        position,
        on_floor,
    })
}

fn write_move_state(w: &mut impl BitSink, m: &MoveState) {
    write_vec3(w, m.velocity);
    write_stance(w, m.stance);
    w.bool(m.sprinting);
    w.f32(m.slide_time);
    w.f32(m.slide_cooldown);
    w.bool(m.jump_held);
    w.bool(m.crouch_held);
    w.bool(m.sprint_held);
    w.f32(m.jump_buffer);
    w.f32(m.coyote);
    w.f32(m.mantle_time);
    w.f32(m.mantle_rise);
    w.f32(m.lunge_time);
    w.f32(m.lunge_velocity.x);
    w.f32(m.lunge_velocity.y);
    w.f32(m.crouch);
    w.f32(m.sprint_time);
    w.f32(m.mantle_dir.x);
    w.f32(m.mantle_dir.y);
    w.f32(m.sprint_blend);
    w.f32(m.air_time);
    w.f32(m.land_slow);
    w.bool(m.vaulting);
    w.bool(m.slide_queued);
    w.f32(m.slide_out);
    w.f32(m.ladder_facing.x);
    w.f32(m.ladder_facing.y);
    w.bool(m.ladder_slide);
    w.f32(m.ladder_cooldown);
    w.bool(m.mounted);
    w.f32(m.mount_yaw);
    w.f32(m.mount_crouch);
    w.f32(m.mount_blend);
    w.f32(m.crouch_hold);
    w.bool(m.hold_from_prone);
    w.bool(m.prone_held);
}

fn read_move_state(r: &mut impl BitSource) -> DecodeResult<MoveState> {
    Ok(MoveState {
        velocity: read_vec3(r)?,
        stance: read_stance(r)?,
        sprinting: r.bool()?,
        slide_time: r.f32()?,
        slide_cooldown: r.f32()?,
        jump_held: r.bool()?,
        crouch_held: r.bool()?,
        sprint_held: r.bool()?,
        jump_buffer: r.f32()?,
        coyote: r.f32()?,
        mantle_time: r.f32()?,
        mantle_rise: r.f32()?,
        lunge_time: r.f32()?,
        lunge_velocity: Vec2::new(r.f32()?, r.f32()?),
        crouch: r.f32()?,
        sprint_time: r.f32()?,
        mantle_dir: Vec2::new(r.f32()?, r.f32()?),
        sprint_blend: r.f32()?,
        air_time: r.f32()?,
        land_slow: r.f32()?,
        vaulting: r.bool()?,
        slide_queued: r.bool()?,
        slide_out: r.f32()?,
        ladder_facing: Vec2::new(r.f32()?, r.f32()?),
        ladder_slide: r.bool()?,
        ladder_cooldown: r.f32()?,
        mounted: r.bool()?,
        mount_yaw: r.f32()?,
        mount_crouch: r.f32()?,
        mount_blend: r.f32()?,
        crouch_hold: r.f32()?,
        hold_from_prone: r.bool()?,
        prone_held: r.bool()?,
    })
}

fn write_combat_state(w: &mut impl BitSink, c: &CombatState) {
    for slot in &c.slots {
        write_weapon(w, slot.id);
        w.number(u32::from(slot.magazine), 16);
        w.number(u32::from(slot.reserve), 16);
        w.bool(slot.needs_cycle);
        w.bool(slot.carried);
        w.bool(slot.fresh);
    }
    w.bits(u32::from(c.active), 2);
    match c.action {
        Action::Ready => w.bits(0, 3),
        Action::Reloading { remaining, empty } => {
            w.bits(1, 3);
            w.f32(remaining);
            w.bool(empty);
        }
        Action::Loading { remaining, stage } => {
            w.bits(2, 3);
            w.f32(remaining);
            let stage = match stage {
                LoadStage::Opening => 0,
                LoadStage::Round => 1,
                LoadStage::Closing => 2,
            };
            w.bits(stage, 2);
        }
        Action::Cycling { remaining } => {
            w.bits(3, 3);
            w.f32(remaining);
        }
        Action::Lowering { remaining, to } => {
            w.bits(4, 3);
            w.f32(remaining);
            w.bits(u32::from(to), 2);
        }
        Action::Raising { remaining, kind } => {
            w.bits(5, 3);
            w.f32(remaining);
            w.bool(kind == Raise::First);
        }
        Action::Meleeing {
            remaining,
            strike_in,
            lunge,
        } => {
            w.bits(6, 3);
            w.f32(remaining);
            w.f32(strike_in);
            w.bool(lunge);
        }
        Action::Throwing {
            kind,
            cooked,
            release_in,
        } => {
            w.bits(7, 3);
            w.bits(u32::from(kind.code()), EQUIPMENT_BITS);
            w.f32(cooked);
            match release_in {
                Some(left) => {
                    w.bool(true);
                    w.f32(left);
                }
                None => w.bool(false),
            }
        }
    }
    w.f32(c.ads);
    w.f32(c.cooldown);
    w.f32(c.sprint_out);
    w.number(u32::from(c.burst), 16);
    w.f32(c.since_shot);
    w.f32(c.recoil.x);
    w.f32(c.recoil.y);
    w.f32(c.bloom);
    w.number(c.shots_fired, 32);
    w.bool(c.fire_held);
    w.u8(c.burst_left);
    w.f32(c.sway_time);
    w.f32(c.breath);
    w.f32(c.winded);
    w.bool(c.holding_breath);
    w.bool(c.reload_queued);
    w.u8(c.lethals);
    w.bits(u32::from(c.tactical.code()), EQUIPMENT_BITS);
    w.u8(c.tacticals);
}

fn read_combat_state(r: &mut impl BitSource) -> DecodeResult<CombatState> {
    let mut slot = || -> DecodeResult<WeaponState> {
        Ok(WeaponState {
            id: read_weapon(r)?,
            magazine: r.number(16)? as u16,
            reserve: r.number(16)? as u16,
            needs_cycle: r.bool()?,
            carried: r.bool()?,
            fresh: r.bool()?,
        })
    };
    let slots = [slot()?, slot()?];
    let active = r.bits(2)? as u8;
    if !slots.get(usize::from(active)).is_some_and(|s| s.carried) {
        return Err(DecodeError::Invalid("active slot"));
    }
    let action = match r.bits(3)? {
        0 => Action::Ready,
        1 => Action::Reloading {
            remaining: r.f32()?,
            empty: r.bool()?,
        },
        2 => Action::Loading {
            remaining: r.f32()?,
            stage: match r.bits(2)? {
                0 => LoadStage::Opening,
                1 => LoadStage::Round,
                2 => LoadStage::Closing,
                _ => return Err(DecodeError::Invalid("load stage")),
            },
        },
        3 => Action::Cycling {
            remaining: r.f32()?,
        },
        4 => {
            let remaining = r.f32()?;
            let to = r.bits(2)? as u8;
            if usize::from(to) >= slots.len() {
                return Err(DecodeError::Invalid("switch slot"));
            }
            Action::Lowering { remaining, to }
        }
        5 => Action::Raising {
            remaining: r.f32()?,
            kind: if r.bool()? { Raise::First } else { Raise::Draw },
        },
        6 => Action::Meleeing {
            remaining: r.f32()?,
            strike_in: r.f32()?,
            lunge: r.bool()?,
        },
        7 => Action::Throwing {
            kind: read_equipment(r)?,
            cooked: r.f32()?,
            release_in: if r.bool()? { Some(r.f32()?) } else { None },
        },
        _ => return Err(DecodeError::Invalid("action")),
    };
    Ok(CombatState {
        slots,
        active,
        action,
        ads: r.f32()?,
        cooldown: r.f32()?,
        sprint_out: r.f32()?,
        burst: r.number(16)? as u16,
        since_shot: r.f32()?,
        recoil: Vec2::new(r.f32()?, r.f32()?),
        bloom: r.f32()?,
        shots_fired: r.number(32)?,
        fire_held: r.bool()?,
        burst_left: r.u8()?,
        sway_time: r.f32()?,
        breath: r.f32()?,
        winded: r.f32()?,
        holding_breath: r.bool()?,
        reload_queued: r.bool()?,
        lethals: r.u8()?,
        tactical: read_equipment(r)?,
        tacticals: r.u8()?,
    })
}

/// Bits for a class's grenade counts (0 to 3).
const CLASS_COUNT_BITS: u32 = 2;

fn write_gun(w: &mut BitWriter, gun: Option<Gun>) {
    w.bool(gun.is_some());
    if let Some(gun) = gun {
        write_weapon(w, gun.base);
        w.bits(u32::from(gun.attachments.bits()), Attachments::BITS);
    }
}

fn read_gun(r: &mut BitReader) -> DecodeResult<Option<Gun>> {
    if !r.bool()? {
        return Ok(None);
    }
    let base = read_weapon(r)?;
    let attachments = Attachments::from_bits(r.bits(Attachments::BITS)? as u8)
        .ok_or(DecodeError::Invalid("attachments"))?;
    Ok(Some(Gun { base, attachments }))
}

/// Writes a class. Its rules are the server's to check, not the decoder's.
fn write_class(w: &mut BitWriter, class: &Class) {
    write_gun(w, class.primary);
    write_gun(w, class.secondary);
    w.bits(u32::from(class.lethals.min(3)), CLASS_COUNT_BITS);
    w.bits(u32::from(class.tactical.code()), EQUIPMENT_BITS);
    w.bits(u32::from(class.tacticals.min(3)), CLASS_COUNT_BITS);
    w.bits(u32::from(class.perks.bits()), Perks::BITS);
    w.bits(u32::from(class.wildcards.bits()), Wildcards::BITS);
}

fn read_perks(r: &mut impl BitSource) -> DecodeResult<Perks> {
    Perks::from_bits(r.bits(Perks::BITS)? as u16).ok_or(DecodeError::Invalid("perks"))
}

fn read_class(r: &mut BitReader) -> DecodeResult<Class> {
    Ok(Class {
        primary: read_gun(r)?,
        secondary: read_gun(r)?,
        lethals: r.bits(CLASS_COUNT_BITS)? as u8,
        tactical: read_equipment(r)?,
        tacticals: r.bits(CLASS_COUNT_BITS)? as u8,
        perks: read_perks(r)?,
        wildcards: Wildcards::from_bits(r.bits(Wildcards::BITS)? as u8)
            .ok_or(DecodeError::Invalid("wildcards"))?,
    })
}

/// Bits for an [`equipment::Kind`] on the wire: room for the tacticals.
const EQUIPMENT_BITS: u32 = 2;

fn read_equipment(r: &mut impl BitSource) -> DecodeResult<equipment::Kind> {
    equipment::Kind::from_code(r.bits(EQUIPMENT_BITS)? as u8)
        .ok_or(DecodeError::Invalid("equipment"))
}

fn write_vec3(w: &mut impl BitSink, v: Vec3) {
    w.f32(v.x);
    w.f32(v.y);
    w.f32(v.z);
}

fn read_vec3(r: &mut impl BitSource) -> DecodeResult<Vec3> {
    Ok(Vec3::new(r.f32()?, r.f32()?, r.f32()?))
}

/// A crouch blend (0 standing, 1 crouched, 2 prone) as a byte.
pub fn crouch_to_u8(crouch: f32) -> u8 {
    (crouch.clamp(0.0, 2.0) * 127.5).round() as u8
}

/// A byte from [`crouch_to_u8`] back to the blend.
pub fn crouch_from_u8(value: u8) -> f32 {
    f32::from(value) / 127.5
}

fn write_stance(w: &mut impl BitSink, stance: Stance) {
    let value = match stance {
        Stance::Standing => 0,
        Stance::Crouching => 1,
        Stance::Sliding => 2,
        Stance::Prone => 3,
    };
    w.bits(value, 2);
}

fn read_stance(r: &mut impl BitSource) -> DecodeResult<Stance> {
    match r.bits(2)? {
        0 => Ok(Stance::Standing),
        1 => Ok(Stance::Crouching),
        2 => Ok(Stance::Sliding),
        3 => Ok(Stance::Prone),
        _ => Err(DecodeError::Invalid("stance")),
    }
}

/// Weapons go by their index in the weapon table, which both sides build from
/// the same data files.
/// A rewound hitbox pose's activity, with `ads` quantized as in snapshots.
fn write_activity(w: &mut BitWriter, a: &Activity) {
    w.bool(a.sprinting);
    w.bool(a.airborne);
    w.bool(a.mantling);
    w.bool(a.vaulting);
    w.bool(a.reloading);
    w.u8((a.ads.clamp(0.0, 1.0) * 255.0).round() as u8);
    write_vec3(w, a.velocity);
    w.bool(a.weapon.is_some());
    if let Some(weapon) = a.weapon {
        write_weapon(w, weapon);
    }
    w.u8(clock::stride_to_u8(a.progress.stride));
    w.u8(clock::to_u8(a.progress.reload));
    w.u8(clock::to_u8(a.progress.mantle));
    w.u8(clock::turn_to_u8(a.progress.turn));
    w.u8(clock::to_u8(a.progress.step));
    w.bool(a.crouch.is_some());
    if let Some(crouch) = a.crouch {
        w.u8(crouch_to_u8(crouch));
    }
    w.bool(a.throw.is_some());
    if let Some(throw) = a.throw {
        w.u8(clock::to_u8(throw));
    }
    w.bool(a.kneel.is_some());
    if let Some(kneel) = a.kneel {
        w.u8(clock::to_u8(kneel));
    }
}

fn read_activity(r: &mut BitReader) -> DecodeResult<Activity> {
    Ok(Activity {
        sprinting: r.bool()?,
        airborne: r.bool()?,
        mantling: r.bool()?,
        vaulting: r.bool()?,
        reloading: r.bool()?,
        ads: f32::from(r.u8()?) / 255.0,
        velocity: read_finite_vec3(r)?,
        weapon: if r.bool()? {
            Some(read_weapon(r)?)
        } else {
            None
        },
        progress: Progress {
            stride: clock::stride_from_u8(r.u8()?),
            reload: clock::from_u8(r.u8()?),
            mantle: clock::from_u8(r.u8()?),
            turn: clock::turn_from_u8(r.u8()?),
            step: clock::from_u8(r.u8()?),
        },
        crouch: if r.bool()? {
            Some(crouch_from_u8(r.u8()?))
        } else {
            None
        },
        hunch: 0.0,
        throw: if r.bool()? {
            Some(clock::from_u8(r.u8()?))
        } else {
            None
        },
        kneel: if r.bool()? {
            Some(clock::from_u8(r.u8()?))
        } else {
            None
        },
    })
}

fn write_weapon(w: &mut impl BitSink, id: WeaponId) {
    w.u8(id.0);
}

fn read_weapon(r: &mut impl BitSource) -> DecodeResult<WeaponId> {
    let id = WeaponId(r.u8()?);
    if valid_weapon(id) {
        Ok(id)
    } else {
        Err(DecodeError::Invalid("weapon"))
    }
}

fn valid_weapon(id: WeaponId) -> bool {
    usize::from(id.0) < weapon::table().len()
}

/// Names are at most this many bytes of UTF-8 on the wire.
const MAX_NAME_BYTES: usize = MAX_NAME_CHARS * 4;

/// A UTF-8 string, cut at a character boundary to at most `max` bytes.
fn write_string(w: &mut BitWriter, s: &str, max: usize) {
    let mut end = s.len().min(max);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    let bytes = &s.as_bytes()[..end];
    w.varint(bytes.len() as u32);
    for byte in bytes {
        w.u8(*byte);
    }
}

fn read_string(r: &mut BitReader, max: usize) -> DecodeResult<String> {
    let len = r.varint()? as usize;
    if len > max {
        return Err(DecodeError::Invalid("string length"));
    }
    let mut bytes = Vec::with_capacity(len);
    for _ in 0..len {
        bytes.push(r.u8()?);
    }
    String::from_utf8(bytes).map_err(|_| DecodeError::Invalid("string"))
}

fn write_option_tick(w: &mut BitWriter, tick: Option<Tick>) {
    match tick {
        Some(t) => {
            w.bool(true);
            w.u32(t);
        }
        None => w.bool(false),
    }
}

fn read_option_tick(r: &mut BitReader) -> DecodeResult<Option<Tick>> {
    Ok(if r.bool()? { Some(r.u32()?) } else { None })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class::Wildcard;
    use crate::movement::MoveEnv;
    use crate::player::{self, PlayerParams};

    fn sample_state(ticks: usize) -> Predicted {
        let params = PlayerParams::default();
        let mut state = PlayerState {
            perks: Perks::of(&[crate::perk::Perk::Lightweight, crate::perk::Perk::Dexterity]),
            drinks: Drinks::NONE
                .with(crate::zombies::drinks::Drink::Quickload)
                .with(crate::zombies::drinks::Drink::LongStride),
            ..PlayerState::default()
        };
        let mut events = Vec::new();
        for i in 0..ticks {
            let input = PlayerInput {
                wish: Vec2::new(0.3, 1.0),
                yaw: i as f32 * 0.01,
                fire: i > 40,
                sprint: i == 3,
                ..Default::default()
            };
            player::step(
                &mut state,
                &input,
                MoveEnv::default(),
                &params,
                1.0 / 60.0,
                &mut events,
            );
        }
        Predicted {
            state,
            position: Vec3::new(1.5, 0.25, -7.125),
            on_floor: true,
        }
    }

    #[test]
    fn own_state_rides_as_changes_from_the_baseline() {
        let before = sample_state(60);
        let after = sample_state(62);
        let snapshot = |own: Predicted, baseline: Option<Tick>| {
            ServerMessage::Snapshot(Snapshot {
                tick: 102,
                baseline,
                own: Some(Box::new(OwnState {
                    last_input: 9,
                    predicted: own,
                })),
                players: Vec::new(),
                grenades: Vec::new(),
                smokes: Vec::new(),
                zombies: None,
                radar: RadarView::default(),
            })
        };
        let base = Baseline {
            players: &[],
            own: Some(&before),
            zombies: None,
        };
        let full = snapshot(after, None).encode_with(base);
        let delta_msg = snapshot(after, Some(100));
        let delta = delta_msg.encode_with(base);
        assert!(
            delta.len() * 3 < full.len(),
            "{} bytes as changes, {} in full",
            delta.len(),
            full.len()
        );
        let decoded = ServerMessage::decode_with(&delta, |tick| {
            assert_eq!(tick, 100);
            Some(base)
        })
        .unwrap();
        assert_eq!(decoded, delta_msg);
        // Unchanged, the own state is little more than a flag a field.
        let same = snapshot(before, Some(100)).encode_with(base);
        assert!(same.len() < 24, "{} bytes unchanged", same.len());
        // Without the baseline's own state the client cannot decode it.
        let missing = Baseline::default();
        assert_eq!(
            ServerMessage::decode_with(&delta, |_| Some(missing)),
            Err(DecodeError::Invalid("unknown own baseline"))
        );
    }

    #[test]
    fn the_horde_rides_as_changes_from_the_acknowledged_one() {
        let horde = |t: f32| -> Vec<ZombieView> {
            (0..24u8)
                .map(|id| {
                    let f = f32::from(id);
                    ZombieView::new(
                        id,
                        Vec3::new(f * 1.3 + t * 1.5, 0.0, -f + t * 0.9),
                        f * 0.2,
                        true,
                    )
                })
                .collect()
        };
        let before = horde(0.0);
        // 80 ms on, two zombies hit and one killed.
        let mut now = horde(0.08);
        now[3].hits = 1;
        now[3].hit_part = 9;
        now[7].alive = false;
        let snapshot = |zombies: Vec<ZombieView>| {
            ServerMessage::Snapshot(Snapshot {
                tick: 120,
                baseline: None,
                own: None,
                players: Vec::new(),
                grenades: Vec::new(),
                smokes: Vec::new(),
                zombies: Some(zombies),
                radar: RadarView::default(),
            })
        };
        let full = snapshot(now.clone()).encode(&[]);
        let base = Baseline {
            zombies: Some((112, &before)),
            ..Baseline::default()
        };
        let msg = snapshot(now);
        let delta = msg.encode_with(base);
        // About 4 bytes a walking zombie, against 7 or 8 in full.
        assert!(
            delta.len() <= 24 * 4 + 4 && delta.len() * 3 < full.len() * 2,
            "{} bytes as changes, {} in full",
            delta.len(),
            full.len()
        );
        let decoded = ServerMessage::decode_with(&delta, |tick| {
            assert_eq!(tick, 112);
            Some(base)
        })
        .unwrap();
        assert_eq!(decoded, msg);
        assert_eq!(
            ServerMessage::decode_with(&delta, |_| Some(Baseline::default())),
            Err(DecodeError::Invalid("unknown zombie baseline"))
        );
    }

    #[test]
    fn a_running_turning_player_costs_a_few_bytes() {
        let base = PlayerView {
            id: 4,
            position: IVec3::new(12_000, 0, -3_000),
            yaw: 0xFFF0,
            pitch: 0,
            alive: true,
            on_floor: true,
            sprinting: true,
            weapon: WeaponId::expect("m1911"),
            stride: 200,
            shots_fired: 40,
            ..Default::default()
        };
        // 80 ms later: 0.5 m on, turned through zero, looking a hair down.
        let now = PlayerView {
            position: IVec3::new(12_400, 0, -2_700),
            yaw: 0x0040,
            pitch: 0xFFFC,
            stride: 30,
            ..base
        };
        let mut w = BitWriter::new();
        write_view_delta(&mut w, &now, &base);
        let bits = w.len_bits();
        let bytes = w.finish();
        let decoded = read_view_delta(&mut BitReader::new(&bytes), &base).unwrap();
        assert_eq!(decoded, now);
        // The grouped format took about 130 bits for the same change.
        assert!(bits <= 88, "{bits} bits");
    }

    #[test]
    fn inputs_round_trip_with_redundancy() {
        let a = PlayerInput {
            wish: Vec2::new(-0.5, 1.0),
            yaw: 1.234,
            pitch: -0.3,
            jump: true,
            fire: true,
            switch_to: Some(1),
            pickup: Some(WeaponId::expect("m1911")),
            ..Default::default()
        }
        .quantized();
        let b = PlayerInput { jump: false, ..a };
        let msg = ClientMessage::Input(InputPacket {
            ack_snapshot: Some(77),
            first_seq: 1000,
            inputs: vec![a, a, a, b, b],
        });
        let bytes = msg.encode();
        assert!(
            bytes.len() < 36,
            "repeats cost one bit each: {} bytes",
            bytes.len()
        );
        assert_eq!(ClientMessage::decode(&bytes).unwrap(), msg);
    }

    #[test]
    fn a_lunging_knife_strike_round_trips_bit_exact() {
        let params = PlayerParams::default();
        let mut state = PlayerState::default();
        state.combat.action = Action::Ready;
        let knife = PlayerInput {
            melee: true,
            lunge: Some(Vec2::new(0.5, -2.5)),
            ..Default::default()
        }
        .quantized();
        player::step(
            &mut state,
            &knife,
            MoveEnv::default(),
            &params,
            1.0 / 120.0,
            &mut Vec::new(),
        );
        assert!(state.combat.meleeing() && state.movement.lunging());
        let p = Predicted {
            state,
            position: Vec3::new(1.0, 0.0, 2.0),
            on_floor: true,
        };
        let mut w = BitWriter::new();
        write_predicted(&mut w, &p);
        let bytes = w.finish();
        let decoded = read_predicted(&mut BitReader::new(&bytes)).unwrap();
        assert_eq!(decoded, p);
        assert!(PlayerView::from_state(1, &p, 0.0, 0.0).meleeing);
    }

    #[test]
    fn a_mounted_weapon_round_trips_and_shows_on_other_players() {
        let mut state = PlayerState::default();
        state.movement.mounted = true;
        state.movement.mount_yaw = -2.5;
        state.movement.mount_crouch = 0.42;
        state.movement.mount_blend = 0.75;
        let p = Predicted {
            state,
            position: Vec3::new(1.0, 0.0, 3.0),
            on_floor: true,
        };
        let mut w = BitWriter::new();
        write_predicted(&mut w, &p);
        let bytes = w.finish();
        assert_eq!(read_predicted(&mut BitReader::new(&bytes)).unwrap(), p);

        let view = PlayerView::from_state(0, &p, 0.0, 0.0);
        let base = PlayerView::default();
        let mut w = BitWriter::new();
        write_view_delta(&mut w, &view, &base);
        let bytes = w.finish();
        let decoded = read_view_delta(&mut BitReader::new(&bytes), &base).unwrap();
        assert_eq!(decoded, view);
        assert_eq!(decoded.mount, 191);
        // Settling in alone (no other flag changing) is still sent.
        let settled = PlayerView { mount: 255, ..view };
        let mut w = BitWriter::new();
        write_view_delta(&mut w, &settled, &view);
        let bytes = w.finish();
        let decoded = read_view_delta(&mut BitReader::new(&bytes), &view).unwrap();
        assert_eq!(decoded.mount, 255);
    }

    #[test]
    fn streak_events_round_trip() {
        let msg = ServerMessage::Events(vec![
            GameEvent::Streak(StreakEvent::State {
                progress: 310,
                held: vec![Streak::Uav, Streak::CounterUav, Streak::Uav],
                loadout: streak::Loadout::default(),
                roofed: true,
            }),
            GameEvent::Streak(StreakEvent::Sky {
                orbit: streak::Orbit {
                    centre: Vec3::new(1.0, 50.0, -3.0),
                    radius: 32.0,
                },
                up: vec![
                    AirView {
                        id: 7,
                        streak: Streak::Uav,
                        owner: 3,
                        side: 1,
                        from: 80_000,
                        phase: 40_000,
                        until: 90_000,
                    },
                    AirView {
                        id: 65_535,
                        streak: Streak::CounterUav,
                        owner: 200,
                        side: 200,
                        from: 0,
                        phase: 0,
                        until: 4,
                    },
                ],
            }),
            GameEvent::Streak(StreakEvent::Sky {
                orbit: streak::Orbit::default(),
                up: Vec::new(),
            }),
            GameEvent::Streak(StreakEvent::Downed {
                id: 7,
                streak: Streak::Uav,
                owner: 3,
                by: 9,
                at: Vec3::new(20.0, 50.5, -4.0),
            }),
            GameEvent::Streak(StreakEvent::Turret(TurretView {
                id: 3,
                owner: 4,
                side: 1,
                at: Vec3::new(-3.0, 0.5, 12.25),
                facing: -1.25,
                placed: 5000,
                until: 33_800,
            })),
            GameEvent::Streak(StreakEvent::TurretTarget {
                id: 3,
                target: Some(8),
            }),
            GameEvent::Streak(StreakEvent::TurretTarget {
                id: 3,
                target: None,
            }),
            GameEvent::Streak(StreakEvent::TurretGone { id: 3, by: Some(8) }),
            GameEvent::Streak(StreakEvent::TurretGone { id: 4, by: None }),
            GameEvent::Streak(StreakEvent::CrateIncoming {
                id: 600,
                owner: 4,
                at: Vec3::new(1.5, -0.25, 30.0),
                heading: 2.5,
                drop_at: 1000,
                lands_at: 1300,
                contents: Streak::CounterUav,
            }),
            GameEvent::Streak(StreakEvent::CrateCapture {
                id: 600,
                by: Some(9),
                started: 1400,
                done_at: 1760,
            }),
            GameEvent::Streak(StreakEvent::CrateCapture {
                id: 600,
                by: None,
                started: 0,
                done_at: 0,
            }),
            GameEvent::Streak(StreakEvent::CrateTaken {
                id: 600,
                by: 9,
                contents: Streak::Uav,
            }),
            GameEvent::Streak(StreakEvent::CrateGone { id: 1 }),
            GameEvent::Streak(StreakEvent::Airstrike {
                owner: 2,
                strikes: vec![
                    (Vec3::new(1.0, 2.0, 3.0), 500),
                    (Vec3::new(-4.0, 0.0, 9.5), 620),
                ],
            }),
            GameEvent::Streak(StreakEvent::Strike {
                owner: 2,
                at: Vec3::new(1.0, 2.0, 3.0),
            }),
        ]);
        assert_eq!(
            ServerMessage::decode(&msg.encode(&[]), |_| None).unwrap(),
            msg
        );
    }

    #[test]
    fn scavenger_events_round_trip() {
        let msg = ServerMessage::Events(vec![
            GameEvent::Scavenger(ScavengerEvent::Dropped {
                id: 65_000,
                at: Vec3::new(-3.5, 0.25, 12.0),
                dropped_by: 7,
            }),
            GameEvent::Scavenger(ScavengerEvent::Taken { id: 65_000, by: 2 }),
            GameEvent::Scavenger(ScavengerEvent::Gone { id: 4 }),
        ]);
        assert_eq!(
            ServerMessage::decode(&msg.encode(&[]), |_| None).unwrap(),
            msg
        );
    }

    #[test]
    fn dead_silence_shows_as_quiet_to_other_players() {
        let mut p = sample_state(5);
        p.state.perks = crate::perk::Perks::of(&[crate::perk::Perk::DeadSilence]);
        let view = PlayerView::from_state(0, &p, 0.0, 0.0);
        assert!(view.quiet);
        let base = PlayerView::default();
        let mut w = BitWriter::new();
        write_view_delta(&mut w, &view, &base);
        let bytes = w.finish();
        assert_eq!(
            read_view_delta(&mut BitReader::new(&bytes), &base).unwrap(),
            view
        );
        // Only the perk changing is still sent.
        let loud = PlayerView {
            quiet: false,
            ..view
        };
        let mut w = BitWriter::new();
        write_view_delta(&mut w, &loud, &view);
        let bytes = w.finish();
        assert!(
            !read_view_delta(&mut BitReader::new(&bytes), &view)
                .unwrap()
                .quiet
        );
    }

    #[test]
    fn a_vault_shows_on_other_players_until_the_drop_lands() {
        let mut state = PlayerState::default();
        state.movement.mantle_time = 0.2;
        state.movement.vaulting = true;
        let rising = Predicted {
            state,
            position: Vec3::new(1.0, 0.5, 3.0),
            on_floor: false,
        };
        let view = PlayerView::from_state(0, &rising, 0.0, 0.0);
        assert!(view.mantling && view.vaulting);
        let base = PlayerView::default();
        let mut w = BitWriter::new();
        write_view_delta(&mut w, &view, &base);
        let bytes = w.finish();
        assert_eq!(
            read_view_delta(&mut BitReader::new(&bytes), &base).unwrap(),
            view
        );
        // Over the top and dropping: still a vault, no longer a climb.
        let mut dropping = rising;
        dropping.state.movement.mantle_time = 0.0;
        dropping.state.movement.vaulting = false;
        dropping.state.movement.air_time = -0.3;
        let view = PlayerView::from_state(0, &dropping, 0.0, 0.0);
        assert!(!view.mantling && view.vaulting);
        // A mantle is not a vault.
        let mut mantle = rising;
        mantle.state.movement.vaulting = false;
        assert!(!PlayerView::from_state(0, &mantle, 0.0, 0.0).vaulting);
    }

    #[test]
    fn a_climber_round_trips_and_shows_the_ladder_it_faces() {
        let mut state = PlayerState::default();
        state.movement.ladder_facing = Vec2::new(0.6, -0.8);
        state.movement.ladder_slide = true;
        state.movement.ladder_cooldown = 0.25;
        let p = Predicted {
            state,
            position: Vec3::new(1.0, 2.0, 3.0),
            on_floor: false,
        };
        let mut w = BitWriter::new();
        write_predicted(&mut w, &p);
        let bytes = w.finish();
        assert_eq!(read_predicted(&mut BitReader::new(&bytes)).unwrap(), p);

        let view = PlayerView::from_state(0, &p, 0.0, 0.0);
        let base = PlayerView::default();
        let mut w = BitWriter::new();
        write_view_delta(&mut w, &view, &base);
        let bytes = w.finish();
        let decoded = read_view_delta(&mut BitReader::new(&bytes), &base).unwrap();
        assert_eq!(decoded, view);
        let facing = decoded.ladder_facing().expect("on a ladder");
        assert!(facing.distance(Vec2::new(0.6, -0.8)) < 0.02, "{facing}");
        // Off the ladder again, the view says so.
        let mut off = view;
        off.ladder = None;
        let mut w = BitWriter::new();
        write_view_delta(&mut w, &off, &view);
        let bytes = w.finish();
        assert_eq!(
            read_view_delta(&mut BitReader::new(&bytes), &view)
                .unwrap()
                .ladder,
            None
        );
    }

    #[test]
    fn melee_inputs_carry_the_lunge_in_millimetres() {
        let input = PlayerInput {
            melee: true,
            lunge: Some(Vec2::new(0.123_45, -2.987_65)),
            ..Default::default()
        }
        .quantized();
        assert_eq!(input.lunge, Some(Vec2::new(0.123, -2.988)));
        let msg = ClientMessage::Input(InputPacket {
            ack_snapshot: None,
            first_seq: 5,
            inputs: vec![input, PlayerInput::default()],
        });
        assert_eq!(ClientMessage::decode(&msg.encode()).unwrap(), msg);
        // No honest client lunges across the map.
        let dash = PlayerInput {
            lunge: Some(Vec2::new(0.0, 30.0)),
            ..Default::default()
        };
        assert_eq!(dash.quantized().lunge, None);
    }

    #[test]
    fn oversize_packets_keep_the_newest_inputs_numbered_correctly() {
        let extra = 6;
        let inputs: Vec<PlayerInput> = (0..MAX_INPUTS_PER_PACKET + extra)
            .map(|i| PlayerInput {
                yaw: i as f32,
                ..Default::default()
            })
            .collect();
        let msg = ClientMessage::Input(InputPacket {
            ack_snapshot: None,
            first_seq: 100,
            inputs,
        });
        let ClientMessage::Input(decoded) = ClientMessage::decode(&msg.encode()).unwrap() else {
            panic!("expected input");
        };
        assert_eq!(decoded.inputs.len(), MAX_INPUTS_PER_PACKET);
        assert_eq!(decoded.first_seq, 100 + extra as u32);
        assert_eq!(decoded.inputs[0].yaw, extra as f32);
    }

    #[test]
    fn tampered_pitch_is_clamped() {
        let input = PlayerInput {
            pitch: 3.0,
            ..Default::default()
        };
        let pitch = input.quantized().pitch;
        assert!(
            (pitch - PITCH_LIMIT).abs() <= 0.5 / VIEW_ANGLE_STEPS,
            "{pitch}"
        );
    }

    #[test]
    fn view_angles_snap_to_a_grid_the_wire_keeps_exactly() {
        for yaw in [0.0, -0.0, 1e-6, -3.0, 6.3, 1234.567, -58_765.43, 1e9] {
            let q = PlayerInput {
                yaw,
                pitch: -yaw.clamp(-1.0, 1.0),
                ..Default::default()
            }
            .quantized();
            assert_eq!(q.quantized(), q, "snapping twice changes nothing");
            if yaw.abs() < 60_000.0 {
                assert!((q.yaw - yaw).abs() <= 0.5 / VIEW_ANGLE_STEPS, "{yaw}");
            }
            let msg = ClientMessage::Input(InputPacket {
                ack_snapshot: None,
                first_seq: 1,
                inputs: vec![q],
            });
            assert_eq!(ClientMessage::decode(&msg.encode()).unwrap(), msg);
        }
    }

    #[test]
    fn turning_and_moving_inputs_cost_a_few_bytes_each() {
        // A player strafing and tracking a target: every input differs.
        let inputs: Vec<PlayerInput> = (0..10)
            .map(|i| {
                PlayerInput {
                    wish: Vec2::new(if i < 5 { 1.0 } else { -1.0 }, 1.0),
                    yaw: 2.0 + i as f32 * 0.021,
                    pitch: -0.1 + i as f32 * 0.003,
                    sprint: true,
                    ..Default::default()
                }
                .quantized()
            })
            .collect();
        let msg = ClientMessage::Input(InputPacket {
            ack_snapshot: Some(500),
            first_seq: 7,
            inputs,
        });
        let bytes = msg.encode();
        assert_eq!(ClientMessage::decode(&bytes).unwrap(), msg);
        // The first input whole (about 10 bytes), then about 4 to 5 bytes
        // each, mostly the turn (32-bit floats were 11 bytes each).
        assert!(bytes.len() <= 56, "{} bytes", bytes.len());
    }

    #[test]
    fn quantized_inputs_survive_the_wire_exactly() {
        let input = PlayerInput {
            wish: Vec2::new(0.123_456, -0.987_654),
            ..Default::default()
        };
        let msg = ClientMessage::Input(InputPacket {
            ack_snapshot: None,
            first_seq: 1,
            inputs: vec![input.quantized()],
        });
        let ClientMessage::Input(decoded) = ClientMessage::decode(&msg.encode()).unwrap() else {
            panic!("expected input");
        };
        assert_eq!(decoded.inputs[0], input.quantized());
        assert_ne!(decoded.inputs[0], input);
    }

    #[test]
    fn own_state_is_bit_exact() {
        let predicted = sample_state(70);
        let snapshot = ServerMessage::Snapshot(Snapshot {
            grenades: Vec::new(),
            smokes: Vec::new(),
            zombies: None,
            radar: RadarView::default(),
            tick: 500,
            baseline: None,
            own: Some(Box::new(OwnState {
                last_input: 42,
                predicted,
            })),
            players: vec![],
        });
        let bytes = snapshot.encode(&[]);
        let decoded = ServerMessage::decode(&bytes, |_| None).unwrap();
        assert_eq!(decoded, snapshot);
    }

    #[test]
    fn grenades_and_a_thrower_mid_cook_survive_the_wire() {
        let mut predicted = sample_state(70);
        predicted.state.combat.action = Action::Throwing {
            kind: equipment::Kind::Frag,
            cooked: 1.25,
            release_in: Some(0.05),
        };
        predicted.state.combat.lethals = 2;
        predicted.state.lethal_held = true;
        let snapshot = ServerMessage::Snapshot(Snapshot {
            grenades: vec![
                GrenadeView::new(
                    4,
                    90_210,
                    equipment::Kind::Frag,
                    Vec3::new(-12.5, 0.045, 33.25),
                    true,
                ),
                GrenadeView::new(
                    0,
                    1,
                    equipment::Kind::Frag,
                    Vec3::new(1.0, 4.0, -2.0),
                    false,
                ),
            ],
            smokes: Vec::new(),
            zombies: None,
            radar: RadarView::default(),
            tick: 900,
            baseline: None,
            own: Some(Box::new(OwnState {
                last_input: 7,
                predicted,
            })),
            players: vec![],
        });
        let decoded = ServerMessage::decode(&snapshot.encode(&[]), |_| None).unwrap();
        assert_eq!(decoded, snapshot);
        let ServerMessage::Snapshot(decoded) = decoded else {
            panic!("expected a snapshot");
        };
        assert!((decoded.grenades[0].position() - Vec3::new(-12.5, 0.045, 33.25)).length() < 1e-3);
    }

    #[test]
    fn hanging_smoke_clouds_survive_the_wire() {
        let smokes = vec![
            SmokeView::new(Some(3), 77, Vec3::new(10.25, 0.04, -6.5), 600),
            // The thrower left; the cloud stays.
            SmokeView::new(None, 2, Vec3::new(-40.0, 3.2, 18.0), 0),
        ];
        let snapshot = ServerMessage::Snapshot(Snapshot {
            grenades: Vec::new(),
            smokes: smokes.clone(),
            zombies: None,
            radar: RadarView::default(),
            tick: 1200,
            baseline: None,
            own: None,
            players: vec![],
        });
        let decoded = ServerMessage::decode(&snapshot.encode(&[]), |_| None).unwrap();
        assert_eq!(decoded, snapshot);
        let ServerMessage::Snapshot(decoded) = decoded else {
            panic!("expected a snapshot");
        };
        assert!((decoded.smokes[0].position() - Vec3::new(10.25, 0.04, -6.5)).length() < 1e-3);
        assert_eq!(decoded.smokes[0].age, 600);
        assert_eq!(decoded.smokes[1].owner, None);
    }

    #[test]
    fn a_snapshot_carries_at_most_max_smokes() {
        let snapshot = Snapshot {
            grenades: Vec::new(),
            smokes: vec![SmokeView::new(Some(1), 1, Vec3::ZERO, 5); MAX_SMOKES + 4],
            zombies: None,
            radar: RadarView::default(),
            tick: 10,
            baseline: None,
            own: None,
            players: vec![],
        };
        let decoded =
            ServerMessage::decode(&ServerMessage::Snapshot(snapshot).encode(&[]), |_| None)
                .unwrap();
        let ServerMessage::Snapshot(decoded) = decoded else {
            panic!("expected a snapshot");
        };
        assert_eq!(decoded.smokes.len(), MAX_SMOKES);
    }

    #[test]
    fn a_snapshot_carries_the_minimap_dots_to_decimetres() {
        let blip = radar::Blip {
            id: 7,
            at: Vec3::new(-12.34, 3.0, 45.67),
            tick: 90,
            kind: radar::BlipKind::Fire,
        };
        let radar_view = RadarView {
            jammed: true,
            blips: vec![BlipView::new(&blip, 100)],
        };
        let snapshot = Snapshot {
            grenades: Vec::new(),
            smokes: Vec::new(),
            zombies: None,
            radar: radar_view.clone(),
            tick: 100,
            baseline: None,
            own: None,
            players: vec![],
        };
        let decoded =
            ServerMessage::decode(&ServerMessage::Snapshot(snapshot).encode(&[]), |_| None)
                .unwrap();
        let ServerMessage::Snapshot(decoded) = decoded else {
            panic!("expected a snapshot");
        };
        assert_eq!(decoded.radar, radar_view);
        let dot = decoded.radar.blips[0];
        assert_eq!(dot.age, 10);
        assert!((dot.position() - Vec2::new(-12.3, 45.7)).length() < 0.06);
    }

    #[test]
    fn the_held_interact_button_rides_in_inputs() {
        let input = PlayerInput {
            interact: true,
            ..Default::default()
        };
        let msg = ClientMessage::Input(InputPacket {
            ack_snapshot: None,
            first_seq: 1,
            inputs: vec![input, PlayerInput::default()],
        });
        let ClientMessage::Input(decoded) = ClientMessage::decode(&msg.encode()).unwrap() else {
            panic!("expected input");
        };
        assert!(decoded.inputs[0].interact);
        assert!(!decoded.inputs[1].interact);
    }

    #[test]
    fn planting_and_defusing_ride_in_player_views() {
        let base = PlayerView::default();
        let planting = PlayerView {
            charge_use: throw_to_u8(Some(0.5)),
            ..base
        };
        let mut w = BitWriter::new();
        write_view_delta(&mut w, &planting, &base);
        let bytes = w.finish();
        let decoded = read_view_delta(&mut BitReader::new(&bytes), &base).unwrap();
        assert_eq!(decoded, planting);
        let (kind, progress) = decoded.charge_progress().unwrap();
        assert_eq!(kind, UseKind::Plant);
        assert!((progress - 0.5).abs() < 0.01);
        let defusing = PlayerView {
            defusing: true,
            ..planting
        };
        let mut w = BitWriter::new();
        write_view_delta(&mut w, &defusing, &planting);
        let bytes = w.finish();
        let decoded = read_view_delta(&mut BitReader::new(&bytes), &planting).unwrap();
        assert_eq!(
            decoded.charge_progress().map(|(k, _)| k),
            Some(UseKind::Defuse)
        );
        assert_eq!(base.charge_progress(), None);
    }

    #[test]
    fn the_lethal_button_rides_in_inputs() {
        let input = PlayerInput {
            lethal: true,
            ..Default::default()
        };
        let msg = ClientMessage::Input(InputPacket {
            ack_snapshot: None,
            first_seq: 1,
            inputs: vec![input],
        });
        let ClientMessage::Input(decoded) = ClientMessage::decode(&msg.encode()).unwrap() else {
            panic!("expected input");
        };
        assert!(decoded.inputs[0].lethal);
    }

    #[test]
    fn a_throw_rides_in_the_view_delta() {
        let a = PlayerView::from_state(3, &sample_state(30), 0.5, -0.1);
        assert_eq!(a.throw_progress(), None);
        let mut throwing = a;
        throwing.throw = throw_to_u8(Some(0.6));
        assert!((throwing.throw_progress().unwrap() - 0.6).abs() < 0.01);
        assert_eq!(throw_to_u8(Some(0.0)), 1, "a throw just begun still shows");
        let delta = ServerMessage::Snapshot(Snapshot {
            grenades: Vec::new(),
            smokes: Vec::new(),
            zombies: None,
            radar: RadarView::default(),
            tick: 101,
            baseline: Some(100),
            own: None,
            players: vec![throwing],
        });
        let baseline = [a];
        let decoded = ServerMessage::decode(&delta.encode(&baseline), |_| Some(&baseline[..]));
        assert_eq!(decoded.unwrap(), delta);
    }

    #[test]
    fn an_empty_reload_rides_in_the_view_delta() {
        let a = PlayerView::from_state(3, &sample_state(30), 0.5, -0.1);
        assert!(!a.reload_empty);
        let mut empty = a;
        empty.reloading = true;
        empty.reload_empty = true;
        let delta = ServerMessage::Snapshot(Snapshot {
            grenades: Vec::new(),
            smokes: Vec::new(),
            zombies: None,
            radar: RadarView::default(),
            tick: 101,
            baseline: Some(100),
            own: None,
            players: vec![empty],
        });
        let baseline = [a];
        let decoded = ServerMessage::decode(&delta.encode(&baseline), |_| Some(&baseline[..]));
        assert_eq!(decoded.unwrap(), delta);
        // The server sets it from the combat state.
        let mut p = sample_state(30);
        p.state.combat.action = crate::weapon::Action::Reloading {
            remaining: 1.0,
            empty: true,
        };
        let view = PlayerView::from_state(3, &p, 0.0, 0.0);
        assert!(view.reloading && view.reload_empty);
    }

    #[test]
    fn hits_ride_in_the_view_delta() {
        let p = sample_state(30);
        let a = PlayerView::from_state(3, &p, 0.5, -0.1);
        let full = ServerMessage::Snapshot(Snapshot {
            grenades: Vec::new(),
            smokes: Vec::new(),
            zombies: None,
            radar: RadarView::default(),
            tick: 100,
            baseline: None,
            own: None,
            players: vec![a],
        });
        assert_eq!(
            ServerMessage::decode(&full.encode(&[]), |_| None).unwrap(),
            full
        );
        let mut hit = a;
        hit.hits = a.hits.wrapping_add(1);
        hit.hit_heading = heading_to_u8(-1.0, 0.0);
        hit.hit_part = HitPart::new(Some(0), 120.0, false).to_u8();
        let delta = ServerMessage::Snapshot(Snapshot {
            grenades: Vec::new(),
            smokes: Vec::new(),
            zombies: None,
            radar: RadarView::default(),
            tick: 101,
            baseline: Some(100),
            own: None,
            players: vec![hit],
        });
        let baseline = [a];
        let decoded = ServerMessage::decode(&delta.encode(&baseline), |_| Some(&baseline[..]));
        assert_eq!(decoded.unwrap(), delta);
    }

    #[test]
    fn animation_progress_rides_in_the_view_delta() {
        let p = sample_state(30);
        let a = PlayerView::from_state(3, &p, 0.5, -0.1).with_progress(Progress {
            stride: 0.25,
            reload: 0.0,
            mantle: 0.0,
            turn: 0.0,
            step: 0.0,
        });
        let baseline = [a];
        let b = a.with_progress(Progress {
            stride: 0.75,
            reload: 0.4,
            mantle: 0.6,
            turn: 0.5,
            step: 0.25,
        });
        let delta = ServerMessage::Snapshot(Snapshot {
            grenades: Vec::new(),
            smokes: Vec::new(),
            zombies: None,
            radar: RadarView::default(),
            tick: 101,
            baseline: Some(100),
            own: None,
            players: vec![b],
        });
        let decoded = ServerMessage::decode(&delta.encode(&baseline), |_| Some(&baseline[..]));
        assert_eq!(decoded.unwrap(), delta);
        let progress = b.progress();
        assert_eq!(progress.stride, 0.75);
        assert!((progress.reload - 0.4).abs() < 0.5 / 255.0);
        assert!((progress.mantle - 0.6).abs() < 0.5 / 255.0);
        assert!((progress.turn - 0.5).abs() < std::f32::consts::TAU / 512.0);
        assert!((progress.step - 0.25).abs() < 0.5 / 255.0);
        assert!((clock::turn_from_u8(clock::turn_to_u8(-1.2)) + 1.2).abs() < 0.013);
    }

    #[test]
    fn hit_parts_round_trip() {
        for part in 0..crate::hitbox::COUNT as u8 {
            for (damage, melee, force) in [
                (30.0, false, 3),
                (400.0, false, 15),
                (150.0, true, KNIFE_FORCE),
            ] {
                let packed = HitPart::new(Some(part), damage, melee);
                assert_eq!(HitPart::from_u8(packed.to_u8()), packed);
                assert_eq!(packed.force, force);
            }
        }
        assert_eq!(
            HitPart::from_u8(HitPart::new(None, 30.0, false).to_u8()).part,
            None
        );
        assert_eq!(HitPart::new(Some(99), 30.0, false).part, None);
        assert_eq!(
            HitPart::from_u8(0).part,
            Some(0),
            "a fresh view reads as the head, force 0"
        );
    }

    #[test]
    fn hit_headings_round_trip() {
        for (x, z) in [
            (0.0, 1.0),
            (1.0, 0.0),
            (0.0, -1.0),
            (-0.6, -0.8),
            (0.28, 0.96),
        ] {
            let (rx, rz) = hit_direction(heading_to_u8(x, z));
            assert!(
                (rx - x).abs() < 0.02 && (rz - z).abs() < 0.02,
                "{x},{z} -> {rx},{rz}"
            );
        }
    }

    #[test]
    fn remote_players_delta_against_the_baseline() {
        let p = sample_state(30);
        let a = PlayerView::from_state(3, &p, 0.5, -0.1);
        let b = PlayerView::from_state(9, &p, 2.0, 0.2);
        let full = ServerMessage::Snapshot(Snapshot {
            grenades: Vec::new(),
            smokes: Vec::new(),
            zombies: None,
            radar: RadarView::default(),
            tick: 100,
            baseline: None,
            own: None,
            players: vec![a, b],
        });
        let full_bytes = full.encode(&[]);

        // Next tick: player 3 moved a little, player 9 is unchanged, player 12 joined.
        let mut a2 = a;
        a2.position.x += 37;
        let c = PlayerView::from_state(12, &p, 0.0, 0.0);
        let delta = ServerMessage::Snapshot(Snapshot {
            grenades: Vec::new(),
            smokes: Vec::new(),
            zombies: None,
            radar: RadarView::default(),
            tick: 101,
            baseline: Some(100),
            own: None,
            players: vec![a2, b, c],
        });
        let baseline = [a, b];
        let delta_bytes = delta.encode(&baseline);
        let decoded = ServerMessage::decode(&delta_bytes, |tick| {
            assert_eq!(tick, 100);
            Some(&baseline[..])
        })
        .unwrap();
        assert_eq!(decoded, delta);
        let unchanged = ServerMessage::Snapshot(Snapshot {
            grenades: Vec::new(),
            smokes: Vec::new(),
            zombies: None,
            radar: RadarView::default(),
            tick: 102,
            baseline: Some(100),
            own: None,
            players: vec![a, b],
        });
        let unchanged_bytes = unchanged.encode(&baseline);
        assert!(
            unchanged_bytes.len() < full_bytes.len() / 3,
            "unchanged players cost a few bits: {} vs {} bytes",
            unchanged_bytes.len(),
            full_bytes.len()
        );
    }

    #[test]
    fn unknown_baseline_is_an_error() {
        let msg = ServerMessage::Snapshot(Snapshot {
            grenades: Vec::new(),
            smokes: Vec::new(),
            zombies: None,
            radar: RadarView::default(),
            tick: 10,
            baseline: Some(5),
            own: None,
            players: vec![],
        });
        assert_eq!(
            ServerMessage::decode(&msg.encode(&[]), |_| None),
            Err(DecodeError::Invalid("unknown baseline"))
        );
    }

    #[test]
    fn view_angles_wrap_and_round_trip() {
        for yaw in [-3.0, -0.5, 0.0, 0.7, 3.1, 7.0, -9.0] {
            let p = sample_state(1);
            let v = PlayerView::from_state(0, &p, yaw, 0.0);
            let back = v.yaw();
            let diff = (back - yaw).rem_euclid(std::f32::consts::TAU);
            let diff = diff.min(std::f32::consts::TAU - diff);
            assert!(diff < 1e-3, "yaw {yaw} came back as {back}");
        }
    }

    #[test]
    fn view_ticks_ride_along_as_small_deltas() {
        let firing: Vec<PlayerInput> = (0..12)
            .map(|i| {
                PlayerInput {
                    // Aiming moves the view every tick, so no input repeats.
                    yaw: 0.5 + i as f32 * 0.01,
                    fire: true,
                    view_tick: Some(123_456.0 + f64::from(i) * 1.013),
                    ..Default::default()
                }
                .quantized()
            })
            .collect();
        let without: Vec<PlayerInput> = firing
            .iter()
            .map(|i| PlayerInput {
                view_tick: None,
                ..*i
            })
            .collect();
        let packet = |inputs: Vec<PlayerInput>| {
            ClientMessage::Input(InputPacket {
                ack_snapshot: Some(9),
                first_seq: 40,
                inputs,
            })
        };
        let msg = packet(firing.clone());
        let bytes = msg.encode();
        assert_eq!(ClientMessage::decode(&bytes).unwrap(), msg);
        let plain = packet(without).encode();
        assert!(
            bytes.len() <= plain.len() + 12 * 2 + 5,
            "{} bytes with view ticks, {} without",
            bytes.len(),
            plain.len()
        );
        let expected = ((123_456.0 + 3.0 * 1.013) * 256.0_f64).floor() / 256.0;
        assert_eq!(firing[3].view_tick, Some(expected), "1/256 tick steps");
    }

    #[test]
    fn press_aim_rides_only_on_the_press_input() {
        // A click mid-flick, then the trigger held: the press input carries
        // its aim, every input after it is resent without one.
        let inputs: Vec<PlayerInput> = (0..16)
            .map(|i| {
                PlayerInput {
                    yaw: 0.5 + i as f32 * 0.02,
                    pitch: -0.1,
                    fire: i >= 3,
                    view_tick: (i >= 3).then(|| 5_000.0 + f64::from(i)),
                    fire_aim: (i == 3).then_some(FireAim {
                        yaw: -0.012_345,
                        pitch: 0.001,
                    }),
                    ..Default::default()
                }
                .quantized()
            })
            .collect();
        let packet = |inputs: Vec<PlayerInput>| {
            ClientMessage::Input(InputPacket {
                ack_snapshot: Some(3),
                first_seq: 900,
                inputs,
            })
        };
        let msg = packet(inputs.clone());
        let bytes = msg.encode();
        let ClientMessage::Input(decoded) = ClientMessage::decode(&bytes).unwrap() else {
            panic!("expected input");
        };
        // Bit-identical, so the server's shot leaves from exactly the angles
        // the client predicted with.
        assert_eq!(decoded.inputs, inputs);
        let aim = decoded.inputs[3].fire_aim.unwrap();
        assert_eq!(
            aim.yaw,
            subtick::from_steps(subtick::steps(-0.012_345).unwrap())
        );
        assert_eq!(
            decoded.inputs[3].shot_view(),
            (inputs[3].yaw + aim.yaw, inputs[3].pitch + aim.pitch)
        );
        assert!(
            decoded
                .inputs
                .iter()
                .filter(|i| i.fire_aim.is_some())
                .count()
                == 1
        );
        // One flag bit per input, plus a few bytes on the press.
        let without: Vec<PlayerInput> = inputs
            .iter()
            .map(|i| PlayerInput {
                fire_aim: None,
                ..*i
            })
            .collect();
        let plain = packet(without).encode();
        assert!(
            bytes.len() <= plain.len() + 4,
            "{} bytes with the press aim, {} without",
            bytes.len(),
            plain.len()
        );
    }

    #[test]
    fn press_aim_is_quantized_like_the_wire() {
        let input = PlayerInput {
            fire: true,
            fire_aim: Some(FireAim {
                yaw: 0.123_456_7,
                pitch: -9.0,
            }),
            ..Default::default()
        }
        .quantized();
        let aim = input.fire_aim.unwrap();
        assert_eq!(aim.pitch, -subtick::LIMIT, "clamped to the wire's range");
        assert_eq!(input.quantized(), input, "quantizing is idempotent");
        let msg = ClientMessage::Input(InputPacket {
            ack_snapshot: None,
            first_seq: 1,
            inputs: vec![input],
        });
        assert_eq!(ClientMessage::decode(&msg.encode()).unwrap(), msg);
        let broken = PlayerInput {
            fire_aim: Some(FireAim {
                yaw: f32::NAN,
                pitch: 0.0,
            }),
            ..Default::default()
        };
        assert_eq!(broken.quantized().fire_aim, None);
    }

    #[test]
    fn press_aim_past_the_limit_is_rejected() {
        let mut w = BitWriter::new();
        w.u8(0);
        w.u8(0);
        w.u32(0);
        w.u16(0);
        w.bits(0, INPUT_FLAG_BITS);
        w.bool(true);
        w.varint_signed(subtick::LIMIT_STEPS + 1);
        w.varint_signed(0);
        let bytes = w.finish();
        let mut r = BitReader::new(&bytes);
        assert_eq!(read_input(&mut r), Err(DecodeError::Invalid("fire aim")));
    }

    #[test]
    fn view_ticks_that_cannot_be_sent_are_dropped() {
        for bad in [-1.0, f64::NAN, f64::INFINITY, 5e9] {
            let input = PlayerInput {
                view_tick: Some(bad),
                ..Default::default()
            };
            assert_eq!(input.quantized().view_tick, None, "{bad}");
        }
        let input = PlayerInput {
            view_tick: Some(4_294_967_295.999),
            ..Default::default()
        }
        .quantized();
        let msg = ClientMessage::Input(InputPacket {
            ack_snapshot: None,
            first_seq: 1,
            inputs: vec![input],
        });
        assert_eq!(ClientMessage::decode(&msg.encode()).unwrap(), msg);
    }

    #[test]
    fn debug_requests_round_trip() {
        for on in [true, false] {
            let msg = ClientMessage::Debug {
                rewound_hitboxes: on,
            };
            assert_eq!(ClientMessage::decode(&msg.encode()).unwrap(), msg);
        }
    }

    #[test]
    fn game_events_round_trip() {
        let msg = ServerMessage::Events(vec![
            GameEvent::Hit {
                victim: 201,
                shot: 77_000,
                damage: 41.5,
                headshot: true,
                killed: false,
            },
            GameEvent::Hurt {
                attacker: 3,
                from: Vec3::new(1.0, 1.6, -20.25),
                damage: 27.0,
                killed: true,
            },
            GameEvent::Kill {
                killer: 3,
                victim: 0,
                weapon: WeaponId::expect("m40"),
                headshot: true,
                cause: KillCause::Weapon,
            },
            GameEvent::Kill {
                killer: 3,
                victim: 3,
                weapon: WeaponId::expect("m4"),
                headshot: false,
                cause: KillCause::Equipment(equipment::Kind::Frag),
            },
            GameEvent::Stats(
                [("kills", 12), ("w.m4.hs", 3), ("time", 600)]
                    .into_iter()
                    .map(|(k, v)| (k.to_owned(), v))
                    .collect(),
            ),
            GameEvent::Blast {
                owner: Some(3),
                shot: 41,
                kind: equipment::Kind::Frag,
                at: Vec3::new(2.5, 0.05, -7.125),
            },
            GameEvent::Killcam(
                killcam::KillRecord {
                    killer: 3,
                    victim: 0,
                    tick: 123_456,
                    rewind: 9,
                    weapon: WeaponId::expect("m4"),
                    cause: KillCause::Equipment(equipment::Kind::Frag),
                    headshot: false,
                }
                .killcam(killcam::Kind::Respawn),
            ),
            GameEvent::Killcam(
                killcam::KillRecord {
                    killer: 201,
                    victim: 4,
                    tick: 99,
                    rewind: 0,
                    weapon: WeaponId::expect("m40"),
                    cause: KillCause::Weapon,
                    headshot: true,
                }
                .killcam(killcam::Kind::Final),
            ),
            GameEvent::Score {
                awards: vec![
                    Scored {
                        award: Award::Kill,
                        points: 100,
                    },
                    Scored {
                        award: Award::MultiKill(3),
                        points: 50,
                    },
                    Scored {
                        award: Award::Streak(10),
                        points: 100,
                    },
                    Scored {
                        award: Award::FlagTaken,
                        points: 100,
                    },
                    Scored {
                        award: Award::FlagCapture,
                        points: 500,
                    },
                    Scored {
                        award: Award::FlagReturn,
                        points: 100,
                    },
                    Scored {
                        award: Award::CarrierKill,
                        points: 100,
                    },
                    Scored {
                        award: Award::HardpointCapture,
                        points: 100,
                    },
                    Scored {
                        award: Award::HardpointAssault,
                        points: 50,
                    },
                    Scored {
                        award: Award::DominationCapture,
                        points: 150,
                    },
                    Scored {
                        award: Award::DominationDefend,
                        points: 50,
                    },
                    Scored {
                        award: Award::ShotDown(Streak::CounterUav),
                        points: 100,
                    },
                ],
                totals: ScoreTotals {
                    score: 2350,
                    kills: 14,
                    deaths: 2,
                    streak: 10,
                },
            },
            GameEvent::Rewound {
                shot: 5,
                tick: 9000.5,
                poses: vec![(
                    7,
                    HitboxPose {
                        feet: Vec3::new(2.0, 0.0, 3.0),
                        yaw: 1.0,
                        pitch: -0.2,
                        stance: Stance::Crouching,
                        activity: Activity {
                            sprinting: false,
                            airborne: true,
                            mantling: false,
                            vaulting: false,
                            reloading: true,
                            ads: 128.0 / 255.0,
                            velocity: Vec3::new(1.5, -0.25, -4.0),
                            weapon: Some(WeaponId(3)),
                            crouch: Some(clock::from_u8(clock::to_u8(0.5))),
                            hunch: 0.0,
                            throw: Some(clock::from_u8(clock::to_u8(0.62))),
                            kneel: Some(clock::from_u8(clock::to_u8(0.4))),
                            progress: Progress {
                                stride: 0.25,
                                reload: 51.0 / 255.0,
                                mantle: 0.0,
                                turn: 0.0,
                                step: 0.0,
                            },
                        },
                    },
                )],
            },
            GameEvent::Joined {
                id: 4,
                name: "Ünïcode ace".to_owned(),
                team: 1,
                bot: false,
            },
            GameEvent::Left { id: 201 },
            GameEvent::Spawned { yaw: -2.5 },
            GameEvent::Flags {
                flags: [
                    FlagView {
                        home: Vec3::new(-50.5, 0.0, 0.0),
                        status: FlagStatus::Carried { by: 203 },
                    },
                    FlagView {
                        home: Vec3::new(50.5, 0.0, 0.0),
                        status: FlagStatus::Dropped {
                            at: Vec3::new(3.0, -1.8, 2.0),
                            returns_at: 77_000,
                        },
                    },
                ],
            },
            GameEvent::FlagPlay {
                flag: 1,
                play: FlagPlay::Captured,
                player: Some(4),
            },
            GameEvent::FlagPlay {
                flag: 0,
                play: FlagPlay::Returned,
                player: None,
            },
            GameEvent::PropState {
                id: 0,
                stage: 0,
                impulse: Vec3::ZERO,
            },
            GameEvent::PropState {
                id: 65_535,
                stage: 255,
                impulse: Vec3::new(-1.5, 2.25, -0.125),
            },
            GameEvent::Chat {
                from: 7,
                text: "nice shot".to_owned(),
                team: false,
            },
            GameEvent::Match {
                phase: Phase::Lobby,
                ends_at: None,
                scores: [0, 0],
                score_limit: 75,
                period: Period::Regulation,
            },
            GameEvent::Match {
                phase: Phase::Ended { winner: Some(1) },
                ends_at: Some(123_456),
                scores: [41, 75],
                score_limit: 75,
                period: Period::Regulation,
            },
            GameEvent::Match {
                phase: Phase::Ended { winner: None },
                ends_at: Some(9),
                scores: [3, 3],
                score_limit: 1,
                period: Period::SecondHalf,
            },
            GameEvent::Match {
                phase: Phase::Playing,
                ends_at: None,
                scores: [200, 190],
                score_limit: 250,
                period: Period::Overtime { timed: false },
            },
            GameEvent::Match {
                phase: Phase::Playing,
                ends_at: Some(77),
                scores: [2, 2],
                score_limit: 3,
                period: Period::Overtime { timed: true },
            },
            // Gun Game: a player wins.
            GameEvent::Match {
                phase: Phase::Ended { winner: Some(203) },
                ends_at: Some(10),
                scores: [0, 0],
                score_limit: 8,
                period: Period::Regulation,
            },
            GameEvent::Scoreboard {
                rows: vec![
                    ScoreRow {
                        id: 0,
                        score: 1450,
                        kills: 12,
                        deaths: 4,
                        ping_ms: 38,
                        ready: true,
                        level: 7,
                        level_kills: 1,
                        captures: 2,
                        returns: 5,
                        hill_seconds: 74,
                        defends: 3,
                        alive: true,
                        revives: 2,
                        headshots: 9,
                    },
                    ScoreRow {
                        id: 203,
                        ..ScoreRow::default()
                    },
                ],
            },
            GameEvent::HardpointZones {
                zones: vec![
                    ZoneView {
                        name: "Skate Bowl".to_owned(),
                        center: Vec3::new(0.0, -0.3, 0.0),
                        half_extents: Vec3::new(7.0, 2.0, 4.0),
                    },
                    ZoneView {
                        name: "Laundromat".to_owned(),
                        center: Vec3::new(0.0, 1.5, 17.25),
                        half_extents: Vec3::new(10.0, 1.6, 5.25),
                    },
                ],
            },
            GameEvent::Hardpoint {
                active: 1,
                holder: Holder::Held(1),
                moves_at: Some(40_000),
            },
            GameEvent::Hardpoint {
                active: 0,
                holder: Holder::Contested,
                moves_at: None,
            },
            GameEvent::Hardpoint {
                active: 3,
                holder: Holder::Neutral,
                moves_at: Some(1),
            },
            GameEvent::DominationFlags {
                flags: vec![
                    FlagPointView {
                        name: "A".to_owned(),
                        center: Vec3::new(-20.0, 2.0, 4.5),
                        radius: 3.5,
                        half_height: 2.0,
                    },
                    FlagPointView {
                        name: "B".to_owned(),
                        center: Vec3::new(0.0, 2.6, 0.0),
                        radius: 4.0,
                        half_height: 2.0,
                    },
                ],
            },
            GameEvent::Domination(DominationView {
                tick: 91_234,
                swapped: true,
                flags: vec![
                    FlagPointStatus::default(),
                    FlagPointStatus {
                        owner: Some(1),
                        claim: Some(0),
                        progress: 128,
                        contested: false,
                        capturers: 2,
                    },
                    FlagPointStatus {
                        owner: Some(0),
                        claim: Some(1),
                        progress: 40,
                        contested: true,
                        capturers: 0,
                    },
                ],
            }),
            GameEvent::Round {
                number: 1,
                phase: RoundPhase::Freeze,
                ends_at: 600,
                attackers: 0,
                halftime_after: 5,
            },
            GameEvent::Round {
                number: 7,
                phase: RoundPhase::Live,
                ends_at: 1_234_567,
                attackers: 1,
                halftime_after: 5,
            },
            GameEvent::Round {
                number: 11,
                phase: RoundPhase::Over {
                    winner: 1,
                    reason: RoundEnd::Time,
                },
                ends_at: 99,
                attackers: 0,
                halftime_after: 3,
            },
            GameEvent::Round {
                number: 2,
                phase: RoundPhase::Over {
                    winner: 0,
                    reason: RoundEnd::Eliminated,
                },
                ends_at: 5,
                attackers: 0,
                halftime_after: 5,
            },
            GameEvent::Round {
                number: 3,
                phase: RoundPhase::Over {
                    winner: 1,
                    reason: RoundEnd::Detonated,
                },
                ends_at: 5,
                attackers: 1,
                halftime_after: 5,
            },
            GameEvent::Round {
                number: 4,
                phase: RoundPhase::Over {
                    winner: 0,
                    reason: RoundEnd::Defused,
                },
                ends_at: 5,
                attackers: 1,
                halftime_after: 5,
            },
            GameEvent::ChargeSites {
                sites: vec![
                    ZoneView {
                        name: "A".to_owned(),
                        center: Vec3::new(5.0, 1.9, -27.5),
                        half_extents: Vec3::new(2.5, 1.8, 2.5),
                    },
                    ZoneView {
                        name: "B".to_owned(),
                        center: Vec3::new(2.0, 1.9, 29.75),
                        half_extents: Vec3::new(2.5, 1.8, 2.5),
                    },
                ],
            },
            GameEvent::Charge(ChargeView {
                status: ChargeStatus::Carried { by: Some(201) },
                using: Some(UseView {
                    by: 201,
                    kind: UseKind::Plant,
                    started_at: 1000,
                    done_at: 1480,
                }),
            }),
            GameEvent::Charge(ChargeView {
                status: ChargeStatus::Carried { by: None },
                using: None,
            }),
            GameEvent::Charge(ChargeView {
                status: ChargeStatus::Dropped {
                    at: Vec3::new(-3.0, 0.15, 7.5),
                },
                using: None,
            }),
            GameEvent::Charge(ChargeView {
                status: ChargeStatus::Planted {
                    site: 1,
                    at: Vec3::new(2.0, 0.15, 30.0),
                },
                using: Some(UseView {
                    by: 3,
                    kind: UseKind::Defuse,
                    started_at: 5000,
                    done_at: 5840,
                }),
            }),
            GameEvent::Charge(ChargeView {
                status: ChargeStatus::Detonated {
                    site: 0,
                    at: Vec3::new(5.0, 0.15, -27.0),
                },
                using: None,
            }),
            GameEvent::Charge(ChargeView {
                status: ChargeStatus::Defused {
                    site: 1,
                    at: Vec3::new(2.0, 0.15, 30.0),
                },
                using: None,
            }),
        ]);
        let bytes = msg.encode(&[]);
        assert_eq!(ServerMessage::decode(&bytes, |_| None).unwrap(), msg);
    }

    #[test]
    fn welcomes_carry_the_mode() {
        for mode in [
            Mode::TeamDeathmatch,
            Mode::GunGame(gun_game::Rules {
                kills_per_weapon: 3,
            }),
            Mode::CaptureTheFlag,
            Mode::Hardpoint,
            Mode::SearchAndDestroy,
            Mode::FreeForAll,
            Mode::Domination,
        ] {
            let msg = ServerMessage::Welcome {
                player_id: 3,
                tick: 77_000,
                snapshot_interval: 2,
                mode,
                server_build: "abc1234".to_owned(),
                ranked: mode == Mode::Domination,
            };
            assert_eq!(
                ServerMessage::decode(&msg.encode(&[]), |_| None).unwrap(),
                msg
            );
        }
    }

    #[test]
    fn prop_states_round_trip_across_the_whole_range() {
        let events: Vec<GameEvent> = [0u16, 1, 255, 256, 4096, 65_535]
            .into_iter()
            .enumerate()
            .map(|(i, id)| GameEvent::PropState {
                id,
                stage: (i * 51) as u8,
                impulse: Vec3::new(i as f32 * 12.5, -(i as f32), 0.001 * i as f32),
            })
            .collect();
        let msg = ServerMessage::Events(events);
        assert_eq!(
            ServerMessage::decode(&msg.encode(&[]), |_| None).unwrap(),
            msg
        );
    }

    #[test]
    fn oversized_prop_ids_are_rejected() {
        let mut w = BitWriter::new();
        // An event tag of 14, then a varint id that does not fit `u16`.
        w.bits(14, EVENT_TAG_BITS);
        w.varint(70_000);
        w.u8(1);
        write_vec3(&mut w, Vec3::ZERO);
        assert_eq!(
            read_event(&mut BitReader::new(&w.finish())),
            Err(DecodeError::Invalid("prop id"))
        );
    }

    #[test]
    fn non_finite_prop_impulses_are_rejected() {
        let mut w = BitWriter::new();
        w.bits(14, EVENT_TAG_BITS);
        w.varint(4);
        w.u8(2);
        write_vec3(&mut w, Vec3::new(f32::NAN, 0.0, 0.0));
        assert_eq!(
            read_event(&mut BitReader::new(&w.finish())),
            Err(DecodeError::Invalid("non-finite number"))
        );
    }

    #[test]
    fn lobby_messages_round_trip() {
        for msg in [
            ClientMessage::Hello {
                version: PROTOCOL_VERSION,
                name: "Josh".to_owned(),
                account: "eyJhbGciOiJIUzI1NiJ9.eyJ1aWQiOiJhYmMifQ.c2ln".to_owned(),
            },
            ClientMessage::Ready { ready: true },
            ClientMessage::Chat {
                text: "gg ünïcode".to_owned(),
                team: true,
            },
            ClientMessage::Class(Class {
                primary: Some(Gun::with(
                    WeaponId::expect("ak47"),
                    &[weapon::Attachment::Grip, weapon::Attachment::Acog],
                )),
                secondary: None,
                lethals: 2,
                tactical: equipment::Kind::Smoke,
                tacticals: 1,
                perks: crate::perk::Perks::of(&[
                    crate::perk::Perk::FlakJacket,
                    crate::perk::Perk::Lightweight,
                    crate::perk::Perk::TacticalMask,
                ]),
                wildcards: Wildcards::of(&[
                    Wildcard::DangerClose,
                    Wildcard::Overkill,
                    Wildcard::Perk1Greed,
                ]),
            }),
            ClientMessage::Class(Class::default()),
            ClientMessage::SkipKillcam,
            ClientMessage::Streaks(streak::Loadout::default()),
            ClientMessage::Streaks(streak::Loadout([None, Some(Streak::CounterUav), None])),
            ClientMessage::UseStreak {
                streak: Streak::CounterUav,
                targets: Vec::new(),
            },
            ClientMessage::UseStreak {
                streak: Streak::Airstrike,
                targets: vec![Vec2::new(1.5, -20.0), Vec2::new(30.25, 4.0)],
            },
        ] {
            assert_eq!(ClientMessage::decode(&msg.encode()).unwrap(), msg);
        }
    }

    #[test]
    fn long_names_are_cut_at_a_character_boundary() {
        let msg = ClientMessage::Hello {
            version: PROTOCOL_VERSION,
            name: "é".repeat(100),
            account: String::new(),
        };
        let ClientMessage::Hello { name, .. } = ClientMessage::decode(&msg.encode()).unwrap()
        else {
            panic!("not a hello");
        };
        assert_eq!(name, "é".repeat(MAX_NAME_BYTES / 2));
    }

    #[test]
    fn garbage_never_panics() {
        for seed in 0..2000u32 {
            let len = (crate::rng::hash(seed, 0) % 64) as usize;
            let bytes: Vec<u8> = (0..len)
                .map(|i| crate::rng::hash(seed, i as u32 + 1) as u8)
                .collect();
            let _ = ClientMessage::decode(&bytes);
            let _ = ServerMessage::decode(&bytes, |_| Some(&[]));
        }
    }

    #[test]
    fn zombies_round_trip_in_snapshots_and_events() {
        let mut hit = ZombieView::new(9, Vec3::new(-12.345, 0.02, 30.5), 1.5, false);
        hit.swings = 5;
        hit.hits = 3;
        hit.hit_heading = 200;
        hit.hit_part = 0x83;
        let zombies = vec![ZombieView::new(1, Vec3::ZERO, 0.0, true), hit];
        let msg = ServerMessage::Snapshot(Snapshot {
            tick: 400,
            baseline: None,
            own: None,
            players: Vec::new(),
            grenades: Vec::new(),
            smokes: Vec::new(),
            zombies: Some(zombies.clone()),
            radar: RadarView::default(),
        });
        let bytes = msg.encode(&[]);
        // A big horde stays small: about ten bytes a zombie.
        assert!(bytes.len() < 40, "{} bytes", bytes.len());
        let ServerMessage::Snapshot(decoded) = ServerMessage::decode(&bytes, |_| None).unwrap()
        else {
            panic!("not a snapshot");
        };
        assert_eq!(decoded.zombies, Some(zombies));
        let horde =
            vec![ZombieView::new(1, Vec3::new(20.0, 0.0, -25.0), 3.0, true); MAX_ZOMBIES + 3];
        let msg = ServerMessage::Snapshot(Snapshot {
            tick: 404,
            baseline: None,
            own: None,
            players: Vec::new(),
            grenades: Vec::new(),
            smokes: Vec::new(),
            zombies: Some(horde),
            radar: RadarView::default(),
        });
        let bytes = msg.encode(&[]);
        assert!(bytes.len() < MAX_ZOMBIES * 11, "{} bytes", bytes.len());
        let ServerMessage::Snapshot(decoded) = ServerMessage::decode(&bytes, |_| None).unwrap()
        else {
            panic!("not a snapshot");
        };
        assert_eq!(decoded.zombies.map(|z| z.len()), Some(MAX_ZOMBIES));

        let msg = ServerMessage::Events(vec![
            GameEvent::Zombies(ZombiesEvent::Round {
                round: 12,
                phase: ZombiesPhase::Break,
                starts_at: Some(9000),
                remaining: 0,
            }),
            GameEvent::Zombies(ZombiesEvent::Round {
                round: 3,
                phase: ZombiesPhase::Over,
                starts_at: None,
                remaining: 7,
            }),
            GameEvent::Zombies(ZombiesEvent::Hit {
                zombie: 250,
                shot: 1234,
                headshot: true,
                killed: true,
            }),
            GameEvent::Zombies(ZombiesEvent::Points {
                total: 1490,
                change: -1250,
            }),
            GameEvent::Zombies(ZombiesEvent::Boards {
                window: 7,
                boards: 2,
            }),
            GameEvent::Zombies(ZombiesEvent::Door {
                door: 1,
                open: true,
            }),
            GameEvent::Zombies(ZombiesEvent::PowerUpDropped {
                id: 4,
                kind: crate::zombies::PowerUp::Nuke,
                at: Vec3::new(1.0, 0.0, -3.5),
            }),
            GameEvent::Zombies(ZombiesEvent::PowerUpGone { id: 4, by: Some(2) }),
            GameEvent::Zombies(ZombiesEvent::PowerUpGone { id: 5, by: None }),
            GameEvent::Zombies(ZombiesEvent::PowerUpTimers {
                insta_kill_until: Some(5000),
                double_points_until: None,
            }),
            GameEvent::Zombies(ZombiesEvent::Downed {
                player: 3,
                bleeds_out_at: Some(77_000),
            }),
            GameEvent::Zombies(ZombiesEvent::Downed {
                player: 3,
                bleeds_out_at: None,
            }),
            GameEvent::Zombies(ZombiesEvent::Revive {
                player: 3,
                by: Some(1),
                done_at: 77_360,
            }),
            GameEvent::Zombies(ZombiesEvent::Power { on: true }),
            GameEvent::Zombies(ZombiesEvent::Machine {
                machine: 2,
                available: false,
                price: 500,
                needs_power: false,
            }),
            GameEvent::Zombies(ZombiesEvent::Drank {
                player: 1,
                drink: crate::zombies::drinks::Drink::LongStride,
            }),
            GameEvent::Zombies(ZombiesEvent::Crate {
                spot: 1,
                phase: CratePhase::Offering,
                by: 2,
                weapon: Some(WeaponId::expect("ak47")),
                until: Some(9000),
            }),
            GameEvent::Zombies(ZombiesEvent::Forge {
                phase: ForgePhase::Working,
                by: 3,
                weapon: WeaponId::expect("m4").upgraded(),
                until: Some(800),
            }),
            GameEvent::Zombies(ZombiesEvent::Forge {
                phase: ForgePhase::Idle,
                by: 0,
                weapon: None,
                until: None,
            }),
            GameEvent::Zombies(ZombiesEvent::Crate {
                spot: 0,
                phase: CratePhase::Away,
                by: 0,
                weapon: None,
                until: None,
            }),
        ]);
        assert_eq!(
            ServerMessage::decode(&msg.encode(&[]), |_| None).unwrap(),
            msg
        );
        let welcome = ServerMessage::Welcome {
            player_id: 0,
            tick: 1,
            snapshot_interval: 2,
            mode: Mode::Zombies,
            server_build: "dev".into(),
            ranked: false,
        };
        assert_eq!(
            ServerMessage::decode(&welcome.encode(&[]), |_| None).unwrap(),
            welcome
        );
        let denied = ServerMessage::Denied {
            reason: "Sign in to play on this server.".into(),
        };
        assert_eq!(
            ServerMessage::decode(&denied.encode(&[]), |_| None).unwrap(),
            denied
        );
        assert_eq!(zombie_lag(2), 4.0);
    }
}
