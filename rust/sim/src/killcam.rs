//! Killcams: after a death, a replay of the last few seconds through the
//! killer's eyes, and at the end of a match (or a Search and Destroy round)
//! the deciding kill replayed for everyone.
//!
//! Timings follow Call of Duty's killcam script (`_killcam.gsc`, the same in
//! Call of Duty 4 and Black Ops 3): the player watches their own death for
//! about two seconds, then the killcam starts 2.5 s before the kill (4.25 s
//! for a grenade, which needs its flight) and runs 2 s past it. A killcam at
//! the end of a match or round starts 5 s before the kill and slows to a
//! quarter speed around it.
//!
//! The replay is server truth at no extra bandwidth: snapshots already carry
//! every player to every client, so each client keeps the last
//! [`TAPE_SECONDS`] of them ([`Tape`]), its own state included, and the server
//! only says which kill to show and when ([`Killcam`], sent as
//! `GameEvent::Killcam`). The server holds the victim's respawn until the
//! killcam is over or skipped ([`respawn_wait`]).

use std::collections::VecDeque;

use crate::hitbox::clock::{BodyClock, BodyState};
use crate::net::interpolation::Interpolate;
use crate::net::lag::RemoteSample;
use crate::net::protocol::{KillCause, PlayerView, Predicted};
use crate::net::{TICK_RATE, Tick};
use crate::tdm::{Mode, Phase};
use crate::weapon::WeaponId;

/// Seconds the victim watches their own death before the killcam (Call of
/// Duty 4: 0.25 s, then `waitForTimeOrNotifies(1.75)`).
pub const DEATH_DELAY: f32 = 2.0;
/// Seconds shown before a kill by a gun or a knife (`camtime = 2.5`).
pub const BEFORE: f32 = 2.5;
/// Seconds shown before a grenade kill, so its throw and flight are in it
/// (`camtime = 4.25` for grenade weapons in Black Ops 3; 4.5 in CoD4).
pub const BEFORE_GRENADE: f32 = 4.25;
/// Seconds shown before the kill in a match- or round-ending killcam
/// (`camtime = 5.0` when the killcam does not lead to a respawn).
pub const BEFORE_FINAL: f32 = 5.0;
/// Seconds shown after the kill (`postdelay = 2`).
pub const AFTER: f32 = 2.0;
/// Seconds a match- or round-ending killcam holds on its last frame before
/// handing over (`wait 1.5` after the final killcam in Black Ops 3).
pub const FINAL_HOLD: f32 = 1.5;
/// Pause between the end of a round or match and its killcam starting
/// (`wait 0.35` after `pre_killcam_transition`), rounded up so the result
/// banner reads first.
pub const FINAL_LEAD: f32 = 1.0;
/// Replay speed at the slowest point of an ending killcam
/// (`setSlowMotion(1.0, 0.25, ...)`).
pub const SLOW_MOTION: f32 = 0.25;
/// Replay seconds before the kill that the slow motion starts easing in.
const SLOW_IN: f32 = 0.5;
/// Replay seconds after the kill that it holds at its slowest.
const SLOW_HOLD: f32 = 0.25;
/// Replay seconds it then takes to ease back to full speed.
const SLOW_OUT: f32 = 0.75;
/// Seconds of snapshots each client keeps: an ending killcam's 7 s, plus the
/// lead before it starts and the time its own playback takes to reach the
/// end, with room for a late event.
pub const TAPE_SECONDS: f32 = 16.0;
/// Longest killcam a client accepts, in ticks (a decoding bound).
pub const MAX_TICKS: u32 = 15 * TICK_RATE;
/// Margin the server adds to a respawn killcam before respawning the victim
/// anyway. The client says when its killcam is over (finished or skipped,
/// `ClientMessage::SkipKillcam`), so this only covers a client that never
/// does: the event's trip, and frames that run late.
const RESPAWN_MARGIN: f32 = 1.0;

/// Which killcam this is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// To the victim, after their death, before they respawn.
    #[default]
    Respawn,
    /// To everyone, after the kill that won a Search and Destroy round.
    Round,
    /// To everyone, after the kill that ended the match.
    Final,
}

impl Kind {
    pub fn code(self) -> u8 {
        match self {
            Kind::Respawn => 0,
            Kind::Round => 1,
            Kind::Final => 2,
        }
    }

    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Kind::Respawn),
            1 => Some(Kind::Round),
            2 => Some(Kind::Final),
            _ => None,
        }
    }

    /// Shown to everyone at the end of a round or match.
    pub fn ending(self) -> bool {
        self != Kind::Respawn
    }
}

/// One killcam to play: which kill, and which ticks of the tape around it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Killcam {
    pub kind: Kind,
    pub killer: u8,
    pub victim: u8,
    /// The server tick the kill happened on.
    pub kill_tick: Tick,
    /// Ticks shown before and after the kill.
    pub before: u16,
    pub after: u16,
    /// Ticks behind the killer that the killer was drawing everyone else at
    /// when they fired the killing shot (the lag-compensation rewind): the
    /// replay draws the others that far behind the killer's own view, so it
    /// shows what the killer saw and the shot lands where it did.
    pub rewind: u8,
    /// The weapon in the killer's hands, with its attachment.
    pub weapon: WeaponId,
    pub cause: KillCause,
    pub headshot: bool,
}

impl Killcam {
    /// A killcam of the kill `killer` made on `victim` at `kill_tick`, with
    /// the standard length for its kind and cause.
    pub fn new(kind: Kind, killer: u8, victim: u8, kill_tick: Tick, record: KillRecord) -> Self {
        let before = match (kind, record.cause) {
            (Kind::Round | Kind::Final, _) => BEFORE_FINAL,
            (Kind::Respawn, KillCause::Equipment(_)) => BEFORE_GRENADE,
            (Kind::Respawn, _) => BEFORE,
        };
        Self {
            kind,
            killer,
            victim,
            kill_tick,
            before: seconds_to_ticks(before),
            after: seconds_to_ticks(AFTER),
            rewind: record.rewind,
            weapon: record.weapon,
            cause: record.cause,
            headshot: record.headshot,
        }
    }

    /// The first tick shown.
    pub fn start_tick(&self) -> Tick {
        self.kill_tick.saturating_sub(Tick::from(self.before))
    }

    /// The last tick shown.
    pub fn end_tick(&self) -> Tick {
        self.kill_tick + Tick::from(self.after)
    }

    /// Replay seconds from start to end (slow motion makes playback longer).
    pub fn replay_seconds(&self) -> f32 {
        f32::from(self.before + self.after) / TICK_RATE as f32
    }

    /// Wall-clock seconds playback takes, slow motion included.
    pub fn playback_seconds(&self) -> f32 {
        let mut clock = Playback::new(self);
        let mut seconds = 0.0;
        while !clock.finished() && seconds < 60.0 {
            clock.advance(1.0 / 120.0);
            seconds += 1.0 / 120.0;
        }
        seconds
    }

    /// Replay speed at server tick `tick`: full speed, except that an ending
    /// killcam eases down to [`SLOW_MOTION`] into the kill, holds there just
    /// after it, and eases back out.
    pub fn speed_at(&self, tick: f64) -> f32 {
        if !self.kind.ending() {
            return 1.0;
        }
        let from_kill = ((tick - f64::from(self.kill_tick)) / f64::from(TICK_RATE)) as f32;
        let slow = if from_kill < -SLOW_IN {
            0.0
        } else if from_kill < 0.0 {
            smooth(1.0 + from_kill / SLOW_IN)
        } else if from_kill < SLOW_HOLD {
            1.0
        } else if from_kill < SLOW_HOLD + SLOW_OUT {
            1.0 - smooth((from_kill - SLOW_HOLD) / SLOW_OUT)
        } else {
            0.0
        };
        1.0 + (SLOW_MOTION - 1.0) * slow
    }
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn seconds_to_ticks(seconds: f32) -> u16 {
    (seconds * TICK_RATE as f32).round() as u16
}

/// What the server remembers of a kill to build its killcam.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KillRecord {
    pub killer: u8,
    pub victim: u8,
    pub tick: Tick,
    pub rewind: u8,
    pub weapon: WeaponId,
    pub cause: KillCause,
    pub headshot: bool,
}

impl KillRecord {
    /// The killcam of this kill.
    pub fn killcam(&self, kind: Kind) -> Killcam {
        Killcam::new(kind, self.killer, self.victim, self.tick, *self)
    }
}

/// Whether a death earns its victim a killcam before they respawn: only in
/// modes with respawns, during the match itself (not the warm-up), and only
/// for a kill by someone else (Call of Duty shows none for a suicide or a
/// fall). Search and Destroy shows none on death, by the project's choice
/// (Call of Duty plays one and then lets the dead spectate); its round-ending
/// kill is replayed for everyone instead.
pub fn respawn_killcam(mode: Mode, phase: Phase, killer: u8, victim: u8) -> bool {
    mode != Mode::SearchAndDestroy && phase == Phase::Playing && killer != victim
}

/// Seconds the server waits before respawning the victim of `killcam`: the
/// death delay, the whole replay, and a margin for the event's trip, unless
/// they skip it.
pub fn respawn_wait(killcam: &Killcam) -> f32 {
    DEATH_DELAY + killcam.playback_seconds() + RESPAWN_MARGIN
}

/// Seconds an ending killcam needs before the game moves on: the lead, the
/// playback, and the hold on its last frame.
pub fn ending_seconds(killcam: &Killcam) -> f32 {
    FINAL_LEAD + killcam.playback_seconds() + FINAL_HOLD
}

/// The replay clock: which server tick is shown, advancing at the killcam's
/// speed (slow motion near the kill of an ending killcam).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Playback {
    killcam: Killcam,
    tick: f64,
}

impl Playback {
    pub fn new(killcam: &Killcam) -> Self {
        Self {
            killcam: *killcam,
            tick: f64::from(killcam.start_tick()),
        }
    }

    pub fn killcam(&self) -> &Killcam {
        &self.killcam
    }

    /// The server tick the killer is shown at.
    pub fn tick(&self) -> f64 {
        self.tick
    }

    /// The server tick everyone else is shown at: as far behind the killer
    /// as the killer's own view was when they fired the killing shot.
    pub fn others_tick(&self) -> f64 {
        self.tick - f64::from(self.killcam.rewind)
    }

    /// Advances by `dt` wall-clock seconds; returns the replay seconds moved,
    /// which is less than `dt` in slow motion (for animations).
    pub fn advance(&mut self, dt: f32) -> f32 {
        let speed = self.killcam.speed_at(self.tick);
        let replay = dt * speed;
        self.tick = (self.tick + f64::from(replay) * f64::from(TICK_RATE))
            .min(f64::from(self.killcam.end_tick()));
        replay
    }

    /// 0 at the start, 1 at the end.
    pub fn progress(&self) -> f32 {
        let start = f64::from(self.killcam.start_tick());
        let span = f64::from(self.killcam.end_tick()) - start;
        if span <= 0.0 {
            1.0
        } else {
            ((self.tick - start) / span).clamp(0.0, 1.0) as f32
        }
    }

    /// The kill has happened in the replay.
    pub fn past_kill(&self) -> bool {
        self.tick >= f64::from(self.killcam.kill_tick)
    }

    pub fn finished(&self) -> bool {
        self.tick >= f64::from(self.killcam.end_tick())
    }
}

/// The client's recording of every snapshot it received in the last
/// [`TAPE_SECONDS`]: every player's view, its own included (built from its
/// authoritative state), sorted by id.
#[derive(Default)]
pub struct Tape {
    frames: VecDeque<(Tick, Vec<PlayerView>)>,
}

impl Tape {
    /// Records the players of the snapshot sent at `tick`. Out-of-order and
    /// repeated snapshots are dropped; the views are sorted by id.
    pub fn record(&mut self, tick: Tick, mut players: Vec<PlayerView>) {
        if self.frames.back().is_some_and(|(t, _)| *t >= tick) {
            return;
        }
        players.sort_by_key(|v| v.id);
        self.frames.push_back((tick, players));
        let span = (TAPE_SECONDS * TICK_RATE as f32) as Tick;
        while self.frames.front().is_some_and(|(t, _)| t + span < tick) {
            self.frames.pop_front();
        }
    }

    pub fn clear(&mut self) {
        self.frames.clear();
    }

    pub fn oldest_tick(&self) -> Option<Tick> {
        self.frames.front().map(|(t, _)| *t)
    }

    pub fn newest_tick(&self) -> Option<Tick> {
        self.frames.back().map(|(t, _)| *t)
    }

    /// Whether the tape holds the start of `killcam` (so it can be played).
    pub fn covers(&self, killcam: &Killcam) -> bool {
        self.oldest_tick()
            .is_some_and(|oldest| oldest <= killcam.start_tick() + Tick::from(killcam.before) / 2)
    }

    /// Every player id on the tape between `from` and `to`.
    pub fn ids_between(&self, from: Tick, to: Tick) -> Vec<u8> {
        let mut ids: Vec<u8> = self
            .frames
            .iter()
            .filter(|(t, _)| (from..=to).contains(t))
            .flat_map(|(_, views)| views.iter().map(|v| v.id))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// The view of player `id` in the snapshot at or before `tick`.
    pub fn view_at(&self, id: u8, tick: f64) -> Option<(Tick, PlayerView)> {
        let end = self.frames.partition_point(|(t, _)| f64::from(*t) <= tick);
        self.frames
            .range(..end)
            .rev()
            .find_map(|(t, views)| find(views, id).map(|v| (*t, *v)))
    }

    /// Player `id` as clients drew them at `tick`, blended between the
    /// snapshots around it exactly as live interpolation blends them
    /// ([`crate::net::lag::LagHistory::sample`]): before the first snapshot
    /// it holds the first, past the last it holds the last.
    pub fn sample(&self, id: u8, tick: f64) -> Option<RemoteSample> {
        let split = self.frames.partition_point(|(t, _)| f64::from(*t) <= tick);
        let before = self
            .frames
            .range(..split)
            .rev()
            .find_map(|(t, views)| find(views, id).map(|v| (*t, v)));
        let after = self
            .frames
            .range(split..)
            .find_map(|(t, views)| find(views, id).map(|v| (*t, v)));
        match (before, after) {
            (Some((from_tick, from)), Some((to_tick, to))) => {
                let t = (tick - f64::from(from_tick)) / f64::from(to_tick - from_tick);
                let from = RemoteSample::at(from_tick, from);
                Some(from.interpolate(&RemoteSample::at(to_tick, to), t as f32))
            }
            (Some((at, only)), None) | (None, Some((at, only))) => Some(RemoteSample::at(at, only)),
            (None, None) => None,
        }
    }
}

/// What a body's animation clocks need of a player's state (the server's
/// `body_state` for its avatars, from the same fields).
pub fn body_state(p: &Predicted, yaw: f32) -> BodyState {
    let movement = &p.state.movement;
    BodyState {
        velocity: movement.velocity,
        yaw,
        stance: movement.stance,
        sprinting: movement.sprinting,
        on_floor: p.on_floor,
        alive: p.state.alive(),
        reloading: p.state.combat.is_reloading(),
        weapon: p.state.combat.weapon().id,
        mantle_left: movement.mantle_time,
        ladder: movement
            .on_ladder()
            .then(|| crate::hitbox::gait::yaw_of(movement.ladder_facing)),
    }
}

/// How the client records itself on its own tape: its authoritative state
/// as everyone else's snapshots show it, with the animation clocks the
/// server keeps for it run here (`clock`, advanced by `dt` seconds) and the
/// latest hit it was told about.
#[derive(Default)]
pub struct OwnRecorder {
    clock: BodyClock,
    last_tick: Option<Tick>,
    /// Hits taken (wrapping), and the latest one's heading and part, as in
    /// `PlayerView::hits`.
    pub hits: u8,
    pub hit_heading: u8,
    pub hit_part: u8,
}

impl OwnRecorder {
    /// Player `id`'s view in the snapshot at `tick`, from its own state
    /// `p` and view angles.
    pub fn view(&mut self, id: u8, tick: Tick, p: &Predicted, yaw: f32, pitch: f32) -> PlayerView {
        let dt = self
            .last_tick
            .map_or(0.0, |last| {
                tick.saturating_sub(last) as f32 / TICK_RATE as f32
            })
            .min(0.1);
        self.last_tick = Some(tick);
        let progress = self.clock.tick(&body_state(p, yaw), dt);
        PlayerView {
            hits: self.hits,
            hit_heading: self.hit_heading,
            hit_part: self.hit_part,
            ..PlayerView::from_state(id, p, yaw, pitch)
        }
        .with_progress(progress)
    }
}

fn find(views: &[PlayerView], id: u8) -> Option<&PlayerView> {
    views
        .binary_search_by_key(&id, |v| v.id)
        .ok()
        .map(|i| &views[i])
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::IVec3;

    fn record(cause: KillCause) -> KillRecord {
        KillRecord {
            killer: 3,
            victim: 7,
            tick: 10_000,
            rewind: 6,
            weapon: WeaponId::expect("m4"),
            cause,
            headshot: false,
        }
    }

    #[test]
    fn lengths_follow_call_of_duty() {
        let gun = record(KillCause::Weapon).killcam(Kind::Respawn);
        assert_eq!(gun.before, 300, "2.5 s before a gun kill");
        assert_eq!(gun.after, 240, "2 s after");
        assert_eq!(gun.start_tick(), 9_700);
        assert_eq!(gun.end_tick(), 10_240);
        let knife = record(KillCause::Knife).killcam(Kind::Respawn);
        assert_eq!(knife.before, 300);
        let frag =
            record(KillCause::Equipment(crate::equipment::Kind::Frag)).killcam(Kind::Respawn);
        assert_eq!(frag.before, 510, "4.25 s before a grenade kill");
        let last = record(KillCause::Weapon).killcam(Kind::Final);
        assert_eq!(last.before, 600, "5 s before the match-ending kill");
        assert_eq!(last.after, 240);
    }

    #[test]
    fn respawn_killcams_only_for_kills_by_others_during_a_match() {
        let tdm = Mode::TeamDeathmatch;
        assert!(respawn_killcam(tdm, Phase::Playing, 1, 2));
        assert!(!respawn_killcam(tdm, Phase::Playing, 2, 2), "suicide");
        assert!(!respawn_killcam(tdm, Phase::Lobby, 1, 2), "warm-up");
        assert!(!respawn_killcam(tdm, Phase::Ended { winner: None }, 1, 2));
        assert!(!respawn_killcam(
            Mode::SearchAndDestroy,
            Phase::Playing,
            1,
            2
        ));
        assert!(respawn_killcam(Mode::FreeForAll, Phase::Playing, 1, 2));
        assert!(respawn_killcam(Mode::CaptureTheFlag, Phase::Playing, 1, 2));
    }

    #[test]
    fn a_respawn_killcam_plays_at_full_speed() {
        let killcam = record(KillCause::Weapon).killcam(Kind::Respawn);
        assert!((killcam.playback_seconds() - 4.5).abs() < 0.02);
        assert!((respawn_wait(&killcam) - 7.5).abs() < 0.02);
        let mut clock = Playback::new(&killcam);
        assert_eq!(clock.tick(), 9_700.0);
        assert_eq!(clock.others_tick(), 9_694.0);
        assert!((clock.advance(0.5) - 0.5).abs() < 1e-6);
        assert!((clock.tick() - 9_760.0).abs() < 1e-6);
        assert!(!clock.past_kill());
        clock.advance(10.0);
        assert!(clock.finished());
        assert_eq!(clock.tick(), 10_240.0, "stops at the end");
        assert_eq!(clock.progress(), 1.0);
    }

    #[test]
    fn an_ending_killcam_slows_down_around_the_kill() {
        let killcam = record(KillCause::Weapon).killcam(Kind::Final);
        assert_eq!(killcam.speed_at(9_000.0), 1.0);
        assert!((killcam.speed_at(10_000.0) - SLOW_MOTION).abs() < 1e-6);
        assert!((killcam.speed_at(10_020.0) - SLOW_MOTION).abs() < 1e-6);
        assert_eq!(killcam.speed_at(10_200.0), 1.0);
        let slow = killcam.speed_at(9_970.0);
        assert!(slow < 1.0 && slow > SLOW_MOTION, "eases in: {slow}");
        // 7 s of replay take longer to play.
        let seconds = killcam.playback_seconds();
        assert!(seconds > 8.0 && seconds < 9.5, "{seconds}");
    }

    fn view(id: u8, x: i32) -> PlayerView {
        PlayerView {
            id,
            position: IVec3::new(x, 0, 0),
            alive: true,
            ..Default::default()
        }
    }

    #[test]
    fn the_tape_keeps_sixteen_seconds_and_interpolates_like_live_play() {
        let mut tape = Tape::default();
        for tick in (0..3_000).step_by(2) {
            tape.record(tick, vec![view(9, tick as i32), view(2, -(tick as i32))]);
        }
        assert_eq!(tape.newest_tick(), Some(2_998));
        assert_eq!(tape.oldest_tick(), Some(2_998 - 1_920));
        // Halfway between two snapshots.
        let sample = tape.sample(9, 2_001.0).unwrap();
        assert!((sample.position.x - 2.001).abs() < 1e-5);
        let sample = tape.sample(2, 2_001.0).unwrap();
        assert!((sample.position.x + 2.001).abs() < 1e-5);
        // Out of order: ignored.
        tape.record(2_500, vec![view(9, 0)]);
        assert!((tape.sample(9, 2_500.0).unwrap().position.x - 2.5).abs() < 1e-5);
        // Past the end it holds the last.
        assert!((tape.sample(9, 9_000.0).unwrap().position.x - 2.998).abs() < 1e-5);
        assert!(tape.sample(5, 2_000.0).is_none());
        assert_eq!(tape.ids_between(2_000, 2_100), vec![2, 9]);
        assert_eq!(tape.view_at(9, 2_001.5).unwrap().0, 2_000);
    }

    #[test]
    fn the_tape_bridges_a_player_missing_from_some_snapshots() {
        let mut tape = Tape::default();
        tape.record(10, vec![view(1, 10)]);
        tape.record(12, vec![]);
        tape.record(14, vec![view(1, 14)]);
        let sample = tape.sample(1, 12.0).unwrap();
        assert!((sample.position.x - 0.012).abs() < 1e-6);
    }

    #[test]
    fn the_tape_must_cover_a_killcam_to_play_it() {
        let killcam = record(KillCause::Weapon).killcam(Kind::Respawn);
        let mut tape = Tape::default();
        tape.record(9_900, vec![view(3, 0)]);
        assert!(!tape.covers(&killcam));
        let mut tape = Tape::default();
        tape.record(9_800, vec![view(3, 0)]);
        assert!(tape.covers(&killcam), "half the lead-in is enough");
    }

    #[test]
    fn kinds_round_trip() {
        for kind in [Kind::Respawn, Kind::Round, Kind::Final] {
            assert_eq!(Kind::from_code(kind.code()), Some(kind));
        }
        assert_eq!(Kind::from_code(3), None);
    }
}
