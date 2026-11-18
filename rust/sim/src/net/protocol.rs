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
use super::bits::{BitReader, BitWriter, DecodeError, DecodeResult};
use super::subtick;
use crate::chat;
use crate::gun_game;
use crate::hardpoint::Holder;
use crate::hitbox::clock::{self, Progress};
use crate::hitbox::{Activity, HitboxPose};
use crate::look::PITCH_LIMIT;
use crate::movement::{MoveState, Stance};
use crate::player::{FireAim, PlayerInput, PlayerState};
use crate::score::{Award, ScoreTotals, Scored};
use crate::snd::{RoundEnd, RoundPhase};
use crate::tdm::{MAX_NAME_CHARS, Mode, Phase, TEAM_COUNT, Team};
use crate::weapon::{self, Action, CombatState, LoadStage, WeaponId, WeaponState};

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
pub const PROTOCOL_VERSION: u16 = 28;
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
/// Zone names are at most this many bytes of UTF-8 on the wire.
const MAX_ZONE_NAME_BYTES: usize = 64;

const TAG_BITS: u32 = 3;

#[derive(Clone, Debug, PartialEq)]
pub enum ClientMessage {
    Hello {
        version: u16,
        /// The player's name; the server cleans it (`tdm::clean_name`).
        name: String,
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
    /// The primary weapon to spawn with from the next spawn on (the sidearm is
    /// always the M1911).
    Loadout {
        primary: WeaponId,
    },
    /// A chat message, to everyone or only to our team.
    Chat {
        text: String,
        team: bool,
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
    },
    Reject {
        server_version: u16,
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
        /// A knife kill ([`crate::melee`]): the feed shows the knife instead.
        melee: bool,
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
    /// weapons.
    Match {
        phase: Phase,
        ends_at: Option<Tick>,
        scores: [u32; TEAM_COUNT],
        score_limit: u32,
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
    pub alive: bool,
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
    /// How far down the body is between standing and crouched, 0 to 255
    /// (`MoveState::crouch`): the drawn body and its hit zones follow it.
    pub crouch: u8,
    pub sprinting: bool,
    pub on_floor: bool,
    /// Aim-down-sights amount, 0 to 255.
    pub ads: u8,
    pub weapon: WeaponId,
    pub reloading: bool,
    pub alive: bool,
    /// Rounds fired so far; an increase means the player fired.
    pub shots_fired: u32,
    /// Climbing over a ledge.
    pub mantling: bool,
    /// Swinging the knife.
    pub meleeing: bool,
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
    /// 0 to 15: about a tenth of the damage dealt, low for a knife.
    pub force: u8,
}

impl HitPart {
    /// The force level of `damage` dealt by a gun, or by a knife (`melee`),
    /// which pushes little however much it deals.
    pub fn new(part: Option<u8>, damage: f32, melee: bool) -> Self {
        let force = if melee {
            2
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
            crouch: (p.state.movement.crouch.clamp(0.0, 1.0) * 255.0).round() as u8,
            sprinting: p.state.movement.sprinting,
            on_floor: p.on_floor,
            ads: (combat.ads.clamp(0.0, 1.0) * 255.0).round() as u8,
            weapon: combat.weapon().id,
            reloading: combat.is_reloading(),
            alive: p.state.alive(),
            shots_fired: combat.shots_fired,
            mantling: p.state.movement.mantling(),
            meleeing: combat.meleeing(),
            hits: 0,
            hit_heading: 0,
            hit_part: 0,
            stride: 0,
            reload_progress: 0,
            mantle_progress: 0,
        }
    }

    /// The animation progress this view carries.
    pub fn progress(&self) -> Progress {
        Progress {
            stride: clock::stride_from_u8(self.stride),
            reload: clock::from_u8(self.reload_progress),
            mantle: clock::from_u8(self.mantle_progress),
        }
    }

    /// This view with the animation progress `p`.
    pub fn with_progress(self, p: Progress) -> Self {
        Self {
            stride: clock::stride_to_u8(p.stride),
            reload_progress: clock::to_u8(p.reload),
            mantle_progress: clock::to_u8(p.mantle),
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

    /// The crouch blend, 0 standing to 1 crouched (`MoveState::crouch`).
    pub fn crouch(&self) -> f32 {
        f32::from(self.crouch) / 255.0
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
        self.pitch = self.pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT);
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
            ClientMessage::Hello { version, name } => {
                w.bits(0, TAG_BITS);
                w.u16(*version);
                write_string(&mut w, name, MAX_NAME_BYTES);
            }
            ClientMessage::Input(packet) => {
                w.bits(1, TAG_BITS);
                write_option_tick(&mut w, packet.ack_snapshot);
                // Only the newest inputs fit; number the first one actually sent.
                let count = packet.inputs.len().min(MAX_INPUTS_PER_PACKET);
                let skipped = (packet.inputs.len() - count) as u32;
                w.varint(packet.first_seq.wrapping_add(skipped));
                w.varint(count as u32);
                let mut previous: Option<PlayerInput> = None;
                for input in &packet.inputs[packet.inputs.len() - count..] {
                    let input = PlayerInput {
                        view_tick: input.view_tick.and_then(quantize_view_tick),
                        fire_aim: input.fire_aim.and_then(subtick::quantize),
                        ..*input
                    };
                    let same = previous == Some(input);
                    w.bool(same);
                    if !same {
                        write_input(&mut w, &input, previous.and_then(|p| p.view_tick));
                    }
                    previous = Some(input);
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
            ClientMessage::Loadout { primary } => {
                w.bits(4, TAG_BITS);
                write_weapon(&mut w, *primary);
            }
            ClientMessage::Chat { text, team } => {
                w.bits(5, TAG_BITS);
                w.bool(*team);
                write_string(&mut w, text, chat::MAX_BYTES);
            }
        }
        w.finish()
    }

    pub fn decode(bytes: &[u8]) -> DecodeResult<Self> {
        let mut r = BitReader::new(bytes);
        match r.bits(TAG_BITS)? {
            0 => {
                let version = r.u16()?;
                // An older client sends no name; let the version check reject it.
                let name = if version == PROTOCOL_VERSION {
                    read_string(&mut r, MAX_NAME_BYTES)?
                } else {
                    String::new()
                };
                Ok(ClientMessage::Hello { version, name })
            }
            1 => {
                let ack_snapshot = read_option_tick(&mut r)?;
                let first_seq = r.varint()?;
                let count = r.varint()? as usize;
                if count > MAX_INPUTS_PER_PACKET {
                    return Err(DecodeError::Invalid("input count"));
                }
                let mut inputs: Vec<PlayerInput> = Vec::with_capacity(count);
                for _ in 0..count {
                    let input = if r.bool()? {
                        *inputs.last().ok_or(DecodeError::Invalid("repeat"))?
                    } else {
                        let previous = inputs.last().and_then(|p| p.view_tick);
                        read_input(&mut r, previous)?
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
            4 => Ok(ClientMessage::Loadout {
                primary: read_weapon(&mut r)?,
            }),
            5 => {
                let team = r.bool()?;
                Ok(ClientMessage::Chat {
                    text: read_string(&mut r, chat::MAX_BYTES)?,
                    team,
                })
            }
            _ => Err(DecodeError::Invalid("client message tag")),
        }
    }
}

/// Writes an input. `previous_view` is the view tick of the input before it in
/// the packet: while firing, view ticks advance about one tick per input, so
/// they are sent as a small difference from it.
fn write_input(w: &mut BitWriter, input: &PlayerInput, previous_view: Option<f64>) {
    let byte = |v: f32| ((v.clamp(-1.0, 1.0) * WISH_SCALE).round() as i8) as u8;
    w.u8(byte(input.wish.x));
    w.u8(byte(input.wish.y));
    w.f32(input.yaw);
    w.f32(input.pitch);
    for flag in [
        input.jump,
        input.sprint,
        input.crouch,
        input.fire,
        input.aim,
        input.reload,
        input.melee,
    ] {
        w.bool(flag);
    }
    match input.lunge.and_then(quantize_lunge) {
        Some(offset) => {
            w.bool(true);
            let mm = (offset * LUNGE_SCALE).round();
            w.u16(mm.x as i16 as u16);
            w.u16(mm.y as i16 as u16);
        }
        None => w.bool(false),
    }
    match input.switch_to {
        Some(slot) if slot < 4 => {
            w.bool(true);
            w.bits(u32::from(slot), 2);
        }
        _ => w.bool(false),
    }
    match input.pickup {
        Some(id) if valid_weapon(id) => {
            w.bool(true);
            write_weapon(w, id);
        }
        _ => w.bool(false),
    }
    write_view_tick(w, input.view_tick, previous_view);
    write_fire_aim(w, input.fire_aim);
}

/// The press aim, only on the input where fire went down: a flag, then each
/// axis as signed steps (a byte or two for a tick's worth of turning).
fn write_fire_aim(w: &mut BitWriter, aim: Option<FireAim>) {
    let steps = aim.and_then(|a| Some((subtick::steps(a.yaw)?, subtick::steps(a.pitch)?)));
    w.bool(steps.is_some());
    if let Some((yaw, pitch)) = steps {
        w.varint_signed(yaw);
        w.varint_signed(pitch);
    }
}

fn read_fire_aim(r: &mut BitReader) -> DecodeResult<Option<FireAim>> {
    if !r.bool()? {
        return Ok(None);
    }
    let mut axis = || -> DecodeResult<f32> {
        let steps = r.varint_signed()?;
        if steps.unsigned_abs() > subtick::LIMIT_STEPS.unsigned_abs() {
            return Err(DecodeError::Invalid("fire aim"));
        }
        Ok(subtick::from_steps(steps))
    };
    let yaw = axis()?;
    let pitch = axis()?;
    Ok(Some(FireAim { yaw, pitch }))
}

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

fn read_input(r: &mut BitReader, previous_view: Option<f64>) -> DecodeResult<PlayerInput> {
    let mut axis = || -> DecodeResult<f32> {
        let v = r.u8()? as i8;
        if v == i8::MIN {
            return Err(DecodeError::Invalid("wish"));
        }
        Ok(f32::from(v) / WISH_SCALE)
    };
    let wish = Vec2::new(axis()?, axis()?);
    let yaw = r.f32()?;
    let pitch = r.f32()?;
    if !yaw.is_finite() || !pitch.is_finite() {
        return Err(DecodeError::Invalid("view angles"));
    }
    let jump = r.bool()?;
    let sprint = r.bool()?;
    let crouch = r.bool()?;
    let fire = r.bool()?;
    let aim = r.bool()?;
    let reload = r.bool()?;
    let melee = r.bool()?;
    let lunge = if r.bool()? {
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
    let switch_to = if r.bool()? {
        Some(r.bits(2)? as u8)
    } else {
        None
    };
    let pickup = if r.bool()? {
        Some(read_weapon(r)?)
    } else {
        None
    };
    let view_tick = read_view_tick(r, previous_view)?;
    let fire_aim = read_fire_aim(r)?;
    Ok(PlayerInput {
        wish,
        yaw,
        pitch,
        jump,
        sprint,
        crouch,
        fire,
        aim,
        reload,
        switch_to,
        pickup,
        melee,
        lunge,
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
        let mut w = BitWriter::new();
        match self {
            ServerMessage::Welcome {
                player_id,
                tick,
                snapshot_interval,
                mode,
                server_build,
            } => {
                w.bits(0, TAG_BITS);
                w.u16(PROTOCOL_VERSION);
                w.u8(*player_id);
                w.u32(*tick);
                w.u8(*snapshot_interval);
                write_mode(&mut w, mode);
                write_string(&mut w, server_build, MAX_BUILD_BYTES);
            }
            ServerMessage::Reject { server_version } => {
                w.bits(1, TAG_BITS);
                w.u16(*server_version);
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
                })
            }
            1 => Ok(ServerMessage::Reject {
                server_version: r.u16()?,
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
            melee,
        } => {
            w.bits(2, EVENT_TAG_BITS);
            w.u8(*killer);
            w.u8(*victim);
            write_weapon(w, *weapon);
            w.bool(*headshot);
            w.bool(*melee);
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
                w.bool(row.alive);
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
                        },
                        2,
                    );
                }
            }
            w.u32(*ends_at);
            w.bits(u32::from(*attackers), 1);
            w.varint(*halftime_after);
        }
    }
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
            melee: r.bool()?,
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
            Ok(GameEvent::Match {
                phase,
                ends_at,
                scores,
                score_limit: r.varint()?,
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
                    alive: r.bool()?,
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
                        _ => return Err(DecodeError::Invalid("round end")),
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
        _ => Err(DecodeError::Invalid("event tag")),
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
        _ => Err(DecodeError::Invalid("mode")),
    }
}

const AWARD_TAG_BITS: u32 = 4;

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
        _ => return Err(DecodeError::Invalid("award")),
    })
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

fn write_snapshot(w: &mut BitWriter, s: &Snapshot, baseline: &[PlayerView]) {
    w.u32(s.tick);
    match s.baseline {
        // Baselines are recent, so send the distance back instead of the tick.
        Some(base) => {
            w.bool(true);
            w.varint(s.tick.wrapping_sub(base));
        }
        None => w.bool(false),
    }
    match &s.own {
        Some(own) => {
            w.bool(true);
            w.varint(own.last_input);
            write_predicted(w, &own.predicted);
        }
        None => w.bool(false),
    }
    let baseline = if s.baseline.is_some() { baseline } else { &[] };
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
}

fn read_snapshot<'a>(
    r: &mut BitReader,
    baseline: impl FnOnce(Tick) -> Option<&'a [PlayerView]>,
) -> DecodeResult<Snapshot> {
    let tick = r.u32()?;
    let base_tick = if r.bool()? {
        Some(tick.wrapping_sub(r.varint()?))
    } else {
        None
    };
    let own = if r.bool()? {
        Some(Box::new(OwnState {
            last_input: r.varint()?,
            predicted: read_predicted(r)?,
        }))
    } else {
        None
    };
    let base: &[PlayerView] = match base_tick {
        Some(t) => baseline(t).ok_or(DecodeError::Invalid("unknown baseline"))?,
        None => &[],
    };
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
    Ok(Snapshot {
        tick,
        baseline: base_tick,
        own,
        players,
    })
}

/// Lunge offsets travel in millimetres.
const LUNGE_SCALE: f32 = 1000.0;

/// Field groups of a [`PlayerView`], each sent only when it changed.
fn write_view_delta(w: &mut BitWriter, v: &PlayerView, base: &PlayerView) {
    let changed = v != base;
    w.bool(changed);
    if !changed {
        return;
    }
    for axis in 0..3 {
        let delta = v.position[axis].wrapping_sub(base.position[axis]);
        w.bool(delta != 0);
        if delta != 0 {
            w.varint_signed(delta);
        }
    }
    let look = (v.yaw, v.pitch) != (base.yaw, base.pitch);
    w.bool(look);
    if look {
        w.u16(v.yaw);
        w.u16(v.pitch);
    }
    let flags = |p: &PlayerView| {
        (
            p.stance,
            p.crouch,
            p.sprinting,
            p.on_floor,
            p.ads,
            p.weapon,
            p.reloading,
            p.alive,
            p.mantling,
            p.meleeing,
        )
    };
    let flags_changed = flags(v) != flags(base);
    w.bool(flags_changed);
    if flags_changed {
        write_stance(w, v.stance);
        w.u8(v.crouch);
        w.bool(v.sprinting);
        w.bool(v.on_floor);
        w.u8(v.ads);
        write_weapon(w, v.weapon);
        w.bool(v.reloading);
        w.bool(v.alive);
        w.bool(v.mantling);
        w.bool(v.meleeing);
    }
    let shots = v.shots_fired.wrapping_sub(base.shots_fired);
    w.bool(shots != 0);
    if shots != 0 {
        w.varint(shots);
    }
    let hit = (v.hits, v.hit_heading, v.hit_part) != (base.hits, base.hit_heading, base.hit_part);
    w.bool(hit);
    if hit {
        w.u8(v.hits);
        w.u8(v.hit_heading);
        w.u8(v.hit_part);
    }
    let progress = |p: &PlayerView| (p.stride, p.reload_progress, p.mantle_progress);
    let moved = progress(v) != progress(base);
    w.bool(moved);
    if moved {
        w.u8(v.stride);
        w.u8(v.reload_progress);
        w.u8(v.mantle_progress);
    }
}

fn read_view_delta(r: &mut BitReader, base: &PlayerView) -> DecodeResult<PlayerView> {
    let mut v = *base;
    if !r.bool()? {
        return Ok(v);
    }
    for axis in 0..3 {
        if r.bool()? {
            v.position[axis] = base.position[axis].wrapping_add(r.varint_signed()?);
        }
    }
    if r.bool()? {
        v.yaw = r.u16()?;
        v.pitch = r.u16()?;
    }
    if r.bool()? {
        v.stance = read_stance(r)?;
        v.crouch = r.u8()?;
        v.sprinting = r.bool()?;
        v.on_floor = r.bool()?;
        v.ads = r.u8()?;
        v.weapon = read_weapon(r)?;
        v.reloading = r.bool()?;
        v.alive = r.bool()?;
        v.mantling = r.bool()?;
        v.meleeing = r.bool()?;
    }
    if r.bool()? {
        v.shots_fired = base.shots_fired.wrapping_add(r.varint()?);
    }
    if r.bool()? {
        v.hits = r.u8()?;
        v.hit_heading = r.u8()?;
        v.hit_part = r.u8()?;
    }
    if r.bool()? {
        v.stride = r.u8()?;
        v.reload_progress = r.u8()?;
        v.mantle_progress = r.u8()?;
    }
    Ok(v)
}

// ---------------------------------------------------------------------------
// Full-precision state for prediction

fn write_predicted(w: &mut BitWriter, p: &Predicted) {
    write_vec3(w, p.position);
    w.bool(p.on_floor);
    let s = &p.state;
    w.bool(s.reload_held);
    w.bool(s.melee_held);
    w.f32(s.health);
    w.f32(s.since_damage);
    w.f32(s.fall_speed);
    w.bool(s.airborne);
    write_move_state(w, &s.movement);
    write_combat_state(w, &s.combat);
}

fn read_predicted(r: &mut BitReader) -> DecodeResult<Predicted> {
    let position = read_vec3(r)?;
    let on_floor = r.bool()?;
    let reload_held = r.bool()?;
    let melee_held = r.bool()?;
    let health = r.f32()?;
    let since_damage = r.f32()?;
    let fall_speed = r.f32()?;
    let airborne = r.bool()?;
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
        },
        position,
        on_floor,
    })
}

fn write_move_state(w: &mut BitWriter, m: &MoveState) {
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
}

fn read_move_state(r: &mut BitReader) -> DecodeResult<MoveState> {
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
    })
}

fn write_combat_state(w: &mut BitWriter, c: &CombatState) {
    for slot in &c.slots {
        write_weapon(w, slot.id);
        w.u16(slot.magazine);
        w.u16(slot.reserve);
        w.bool(slot.needs_cycle);
        w.bool(slot.carried);
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
        Action::Raising { remaining } => {
            w.bits(5, 3);
            w.f32(remaining);
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
    }
    w.f32(c.ads);
    w.f32(c.cooldown);
    w.f32(c.sprint_out);
    w.u16(c.burst);
    w.f32(c.since_shot);
    w.f32(c.recoil.x);
    w.f32(c.recoil.y);
    w.f32(c.bloom);
    w.u32(c.shots_fired);
    w.bool(c.fire_held);
    w.u8(c.burst_left);
    w.f32(c.sway_time);
    w.f32(c.breath);
    w.f32(c.winded);
    w.bool(c.holding_breath);
    w.bool(c.reload_queued);
}

fn read_combat_state(r: &mut BitReader) -> DecodeResult<CombatState> {
    let mut slot = || -> DecodeResult<WeaponState> {
        Ok(WeaponState {
            id: read_weapon(r)?,
            magazine: r.u16()?,
            reserve: r.u16()?,
            needs_cycle: r.bool()?,
            carried: r.bool()?,
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
        },
        6 => Action::Meleeing {
            remaining: r.f32()?,
            strike_in: r.f32()?,
            lunge: r.bool()?,
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
        burst: r.u16()?,
        since_shot: r.f32()?,
        recoil: Vec2::new(r.f32()?, r.f32()?),
        bloom: r.f32()?,
        shots_fired: r.u32()?,
        fire_held: r.bool()?,
        burst_left: r.u8()?,
        sway_time: r.f32()?,
        breath: r.f32()?,
        winded: r.f32()?,
        holding_breath: r.bool()?,
        reload_queued: r.bool()?,
    })
}

fn write_vec3(w: &mut BitWriter, v: Vec3) {
    w.f32(v.x);
    w.f32(v.y);
    w.f32(v.z);
}

fn read_vec3(r: &mut BitReader) -> DecodeResult<Vec3> {
    Ok(Vec3::new(r.f32()?, r.f32()?, r.f32()?))
}

fn write_stance(w: &mut BitWriter, stance: Stance) {
    let value = match stance {
        Stance::Standing => 0,
        Stance::Crouching => 1,
        Stance::Sliding => 2,
    };
    w.bits(value, 2);
}

fn read_stance(r: &mut BitReader) -> DecodeResult<Stance> {
    match r.bits(2)? {
        0 => Ok(Stance::Standing),
        1 => Ok(Stance::Crouching),
        2 => Ok(Stance::Sliding),
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
    w.bool(a.crouch.is_some());
    if let Some(crouch) = a.crouch {
        w.u8(clock::to_u8(crouch));
    }
}

fn read_activity(r: &mut BitReader) -> DecodeResult<Activity> {
    Ok(Activity {
        sprinting: r.bool()?,
        airborne: r.bool()?,
        mantling: r.bool()?,
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
        },
        crouch: if r.bool()? {
            Some(clock::from_u8(r.u8()?))
        } else {
            None
        },
    })
}

fn write_weapon(w: &mut BitWriter, id: WeaponId) {
    w.u8(id.0);
}

fn read_weapon(r: &mut BitReader) -> DecodeResult<WeaponId> {
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
    use crate::movement::MoveEnv;
    use crate::player::{self, PlayerParams};

    fn sample_state(ticks: usize) -> Predicted {
        let params = PlayerParams::default();
        let mut state = PlayerState::default();
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
        assert_eq!(input.quantized().pitch, PITCH_LIMIT);
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
    fn hits_ride_in_the_view_delta() {
        let p = sample_state(30);
        let a = PlayerView::from_state(3, &p, 0.5, -0.1);
        let full = ServerMessage::Snapshot(Snapshot {
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
        });
        let baseline = [a];
        let b = a.with_progress(Progress {
            stride: 0.75,
            reload: 0.4,
            mantle: 0.6,
        });
        let delta = ServerMessage::Snapshot(Snapshot {
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
    }

    #[test]
    fn hit_parts_round_trip() {
        for part in 0..crate::hitbox::COUNT as u8 {
            for (damage, melee, force) in [(30.0, false, 3), (400.0, false, 15), (150.0, true, 2)] {
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
        w.bool(true);
        w.varint_signed(subtick::LIMIT_STEPS + 1);
        w.varint_signed(0);
        let bytes = w.finish();
        let mut r = BitReader::new(&bytes);
        assert!(read_fire_aim(&mut r).is_err());
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
                melee: false,
            },
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
                            reloading: true,
                            ads: 128.0 / 255.0,
                            velocity: Vec3::new(1.5, -0.25, -4.0),
                            weapon: Some(WeaponId(3)),
                            crouch: Some(clock::from_u8(clock::to_u8(0.5))),
                            progress: Progress {
                                stride: 0.25,
                                reload: 51.0 / 255.0,
                                mantle: 0.0,
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
            },
            GameEvent::Match {
                phase: Phase::Ended { winner: Some(1) },
                ends_at: Some(123_456),
                scores: [41, 75],
                score_limit: 75,
            },
            GameEvent::Match {
                phase: Phase::Ended { winner: None },
                ends_at: Some(9),
                scores: [3, 3],
                score_limit: 1,
            },
            // Gun Game: a player wins.
            GameEvent::Match {
                phase: Phase::Ended { winner: Some(203) },
                ends_at: Some(10),
                scores: [0, 0],
                score_limit: 8,
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
                        alive: true,
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
        ] {
            let msg = ServerMessage::Welcome {
                player_id: 3,
                tick: 77_000,
                snapshot_interval: 2,
                mode,
                server_build: "abc1234".to_owned(),
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
            },
            ClientMessage::Ready { ready: true },
            ClientMessage::Chat {
                text: "gg ünïcode".to_owned(),
                team: true,
            },
            ClientMessage::Loadout {
                primary: WeaponId::expect("ak47"),
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
}
