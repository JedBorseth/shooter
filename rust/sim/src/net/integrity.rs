//! Server-side checks on what a client sends: limits that disconnect a
//! client abusing the protocol, and heuristics that only flag a player for a
//! server operator to look at.
//!
//! Most cheats a client could try are already impossible by construction:
//! the server simulates movement, firing, reloads, and grenades from inputs
//! with its own rules (`body::tick`), so a client cannot send a position, a
//! speed, a fire rate, or a magazine. Inputs are capped at one a tick over
//! time (`InputQueue` credit), lunges must point at a visible enemy, pickups
//! are raycast, press aims must lie on the view's path (`subtick::check`),
//! and rewinds are capped by the round trip (`LagHistory::rewind_target_for`).
//! What is left, and what this module watches:
//!
//! - **Packet floods and garbage** (disconnect): a token bucket per client at
//!   2.5 times the 120 a second an honest client sends, with a second of
//!   burst; a client that keeps flooding, or sends packets that do not
//!   decode, is disconnected. An honest client never sends one that does not
//!   decode.
//! - **Aim snaps** (flag): a hit whose shot came after the view turned
//!   [`SNAP_DEGREES`] or more in the [`SNAP_TICKS`] ticks before it, faster
//!   than a stick can turn and faster than all but the hardest mouse flicks.
//!   A player is flagged when snap hits are many and a large share of their
//!   hits.
//! - **View ticks** (flag): firing inputs claiming a moment further back
//!   than the round trip explains, or one not yet sent, which the server
//!   clamps anyway. The rewind cap is not counted, so a very slow connection
//!   is never flagged.
//! - **Lag switching** (flag): inputs that stop for a fifth of a second or
//!   more and then arrive in a burst, over and over.
//!
//! Flags never kick: thresholds are set so that honest play over a bad
//! connection or on a controller stays clear, but a heuristic can still be
//! wrong, so a person decides. Valve's VAC and Call of Duty's Ricochet work
//! the same way, collecting evidence on the server for review rather than
//! acting on one signal.

use std::collections::VecDeque;

use glam::Vec3;

use super::Tick;
use crate::TICK_RATE;

/// Packets a second a client may send on average: 2.5 times the input rate.
pub const PACKET_RATE: f32 = 300.0;
/// Packets a client may send at once: a second's worth, for a burst after a
/// stall.
pub const PACKET_BURST: f32 = 300.0;
/// Dropped packets within [`WINDOW_TICKS`] that disconnect a client.
pub const FLOOD_KICK: u32 = 600;
/// Packets that fail to decode within [`WINDOW_TICKS`] that disconnect a
/// client.
pub const GARBAGE_KICK: u32 = 10;
/// The window the counts above are taken over: ten seconds.
pub const WINDOW_TICKS: Tick = TICK_RATE * 10;

/// A turn this large within [`SNAP_TICKS`] before a hit counts as a snap:
/// 1,000 degrees a second, five times a controller's fastest turn.
pub const SNAP_DEGREES: f32 = 25.0;
pub const SNAP_TICKS: usize = 3;
/// Hits remembered for the snap share.
const HIT_WINDOW: usize = 30;
/// A player is flagged for snaps with at least this many snap hits among
/// their last [`HIT_WINDOW`] hits, making up at least [`SNAP_SHARE`] of them.
pub const SNAP_FLAG_HITS: usize = 5;
pub const SNAP_SHARE: f32 = 0.3;

/// Ticks a view tick may lie outside what the connection explains before it
/// counts as stretched.
pub const VIEW_TICK_SLACK: f32 = 2.0;
/// Firing inputs remembered for the stretched share.
const SHOT_WINDOW: usize = 40;
/// Flagged with at least this many stretched view ticks among the last
/// [`SHOT_WINDOW`] shots, making up at least half of them.
pub const VIEW_TICK_FLAG: usize = 10;

/// Ticks without an input that count as a stall: a fifth of a second.
pub const STALL_TICKS: u32 = TICK_RATE / 5;
/// Flagged for lag switching with this many stalls ended by a burst within
/// a minute.
pub const STALL_FLAG: usize = 10;

/// What the server should do with a packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Accept,
    /// Over the rate: ignore it.
    Drop,
    /// Disconnect the client.
    Kick(KickReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KickReason {
    Flood,
    Garbage,
}

impl KickReason {
    pub fn describe(self) -> &'static str {
        match self {
            KickReason::Flood => "flooding packets",
            KickReason::Garbage => "sending packets that do not decode",
        }
    }
}

/// A heuristic that marked a player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flag {
    AimSnaps,
    ViewTicks,
    LagSwitch,
}

impl Flag {
    pub fn describe(self) -> &'static str {
        match self {
            Flag::AimSnaps => "aim snaps onto hits",
            Flag::ViewTicks => "view ticks older than the round trip",
            Flag::LagSwitch => "inputs stalling and bursting",
        }
    }
}

/// Counts over a sliding window of ticks.
#[derive(Default, Clone)]
struct Window {
    events: VecDeque<Tick>,
}

impl Window {
    fn add(&mut self, now: Tick, span: Tick) -> u32 {
        self.events.push_back(now);
        while self
            .events
            .front()
            .is_some_and(|t| now.saturating_sub(*t) >= span)
        {
            self.events.pop_front();
        }
        self.events.len() as u32
    }
}

/// One client's packet budget.
#[derive(Clone)]
pub struct PacketLimit {
    tokens: f32,
    last: Tick,
    dropped: Window,
    garbage: Window,
    pub dropped_total: u32,
    pub garbage_total: u32,
}

impl PacketLimit {
    pub fn new(now: Tick) -> Self {
        Self {
            tokens: PACKET_BURST,
            last: now,
            dropped: Window::default(),
            garbage: Window::default(),
            dropped_total: 0,
            garbage_total: 0,
        }
    }

    /// A packet arrived on tick `now`.
    pub fn on_packet(&mut self, now: Tick) -> Verdict {
        let elapsed = now.saturating_sub(self.last) as f32 / TICK_RATE as f32;
        self.last = self.last.max(now);
        self.tokens = (self.tokens + elapsed * PACKET_RATE).min(PACKET_BURST);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            return Verdict::Accept;
        }
        self.dropped_total += 1;
        if self.dropped.add(now, WINDOW_TICKS) >= FLOOD_KICK {
            Verdict::Kick(KickReason::Flood)
        } else {
            Verdict::Drop
        }
    }

    /// A packet on tick `now` failed to decode.
    pub fn on_garbage(&mut self, now: Tick) -> Verdict {
        self.garbage_total += 1;
        if self.garbage.add(now, WINDOW_TICKS) >= GARBAGE_KICK {
            Verdict::Kick(KickReason::Garbage)
        } else {
            Verdict::Accept
        }
    }
}

/// One player's aim, timing, and input flow, for the flags.
#[derive(Clone, Default)]
pub struct Watch {
    /// The view direction after each of the last few inputs, newest last.
    views: VecDeque<Vec3>,
    /// Whether each of the last hits was a snap.
    hits: VecDeque<bool>,
    /// Whether each of the last firing inputs had its view tick stretched.
    shots: VecDeque<bool>,
    /// Ticks in a row without an input.
    starved: u32,
    stalls: Window,
    pub snap_hits: u32,
    pub hits_total: u32,
    pub stretched: u32,
    pub stall_bursts: u32,
    /// The latest flag raised and when, so a server logs it at most once a
    /// minute.
    flagged: Option<(Flag, Tick)>,
}

impl Watch {
    /// The view after an input the server applied.
    pub fn on_input(&mut self, yaw: f32, pitch: f32) {
        self.views.push_back(view_direction(yaw, pitch));
        while self.views.len() > SNAP_TICKS + 1 {
            self.views.pop_front();
        }
    }

    /// The degrees the view turned over the last [`SNAP_TICKS`] inputs.
    fn recent_turn(&self) -> f32 {
        match (self.views.front(), self.views.back()) {
            (Some(a), Some(b)) => a.dot(*b).clamp(-1.0, 1.0).acos().to_degrees(),
            _ => 0.0,
        }
    }

    /// A shot from the latest input hit another player. Returns a flag the
    /// first time the snap share crosses its threshold in a while.
    pub fn on_hit(&mut self, now: Tick) -> Option<Flag> {
        let snap = self.views.len() > SNAP_TICKS && self.recent_turn() >= SNAP_DEGREES;
        self.hits_total += 1;
        self.snap_hits += u32::from(snap);
        self.hits.push_back(snap);
        while self.hits.len() > HIT_WINDOW {
            self.hits.pop_front();
        }
        let snaps = self.hits.iter().filter(|s| **s).count();
        let flagged =
            snaps >= SNAP_FLAG_HITS && snaps as f32 >= SNAP_SHARE * self.hits.len() as f32;
        self.raise(flagged.then_some(Flag::AimSnaps), now)
    }

    /// A firing input's view tick, `stretched` if the connection does not
    /// explain it (`LagHistory::view_tick_unexplained`).
    pub fn on_shot(&mut self, stretched: bool, now: Tick) -> Option<Flag> {
        self.stretched += u32::from(stretched);
        self.shots.push_back(stretched);
        while self.shots.len() > SHOT_WINDOW {
            self.shots.pop_front();
        }
        let count = self.shots.iter().filter(|s| **s).count();
        let flagged = count >= VIEW_TICK_FLAG && count * 2 >= self.shots.len();
        self.raise(flagged.then_some(Flag::ViewTicks), now)
    }

    /// The server applied `applied` of this player's inputs on tick `now`.
    pub fn on_tick(&mut self, applied: usize, now: Tick) -> Option<Flag> {
        if applied == 0 {
            self.starved += 1;
            return None;
        }
        let stalled = self.starved >= STALL_TICKS && applied > 1;
        self.starved = 0;
        if !stalled {
            return None;
        }
        self.stall_bursts += 1;
        let count = self.stalls.add(now, TICK_RATE * 60) as usize;
        self.raise((count >= STALL_FLAG).then_some(Flag::LagSwitch), now)
    }

    /// Hands out `flag` unless the same flag went out in the last minute.
    fn raise(&mut self, flag: Option<Flag>, now: Tick) -> Option<Flag> {
        let flag = flag?;
        if self
            .flagged
            .is_some_and(|(f, at)| f == flag && now.saturating_sub(at) < TICK_RATE * 60)
        {
            return None;
        }
        self.flagged = Some((flag, now));
        Some(flag)
    }

    /// Whether no flag has been raised yet.
    pub fn is_clear(&self) -> bool {
        self.flagged.is_none()
    }
}

/// The unit vector a view looks along (Godot's convention: yaw 0 looks down
/// -Z, pitch up is positive).
fn view_direction(yaw: f32, pitch: f32) -> Vec3 {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    Vec3::new(-sy * cp, sp, -cy * cp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_honest_client_is_never_limited_even_in_bursts() {
        let mut limit = PacketLimit::new(0);
        // 120 inputs a second plus a few events, with a one-second stall
        // delivered all at once now and then.
        for second in 0..60u32 {
            for tick in 0..TICK_RATE {
                let now = second * TICK_RATE + tick;
                let packets = match (second % 7, tick) {
                    (3, 0..120) => 0,
                    (4, 0) => 121,
                    _ => 1,
                } + u32::from(tick % 40 == 0);
                for _ in 0..packets {
                    assert_eq!(limit.on_packet(now), Verdict::Accept, "second {second}");
                }
            }
        }
        assert_eq!(limit.dropped_total, 0);
    }

    #[test]
    fn a_flood_is_dropped_then_kicked() {
        let mut limit = PacketLimit::new(0);
        let mut verdicts = Vec::new();
        'flood: for now in 0..TICK_RATE * 5 {
            for _ in 0..20 {
                let v = limit.on_packet(now);
                verdicts.push(v);
                if matches!(v, Verdict::Kick(_)) {
                    break 'flood;
                }
            }
        }
        assert!(verdicts.contains(&Verdict::Drop));
        assert_eq!(verdicts.last(), Some(&Verdict::Kick(KickReason::Flood)));
        // 2,400 a second against 300: kicked within a second.
        assert!(verdicts.len() < 2_400, "{} packets", verdicts.len());
    }

    #[test]
    fn garbage_is_kicked_after_a_few() {
        let mut limit = PacketLimit::new(0);
        for i in 0..GARBAGE_KICK - 1 {
            assert_eq!(limit.on_garbage(i * 10), Verdict::Accept);
        }
        assert_eq!(limit.on_garbage(100), Verdict::Kick(KickReason::Garbage));
        // Spread over minutes, a stray few are forgiven.
        let mut limit = PacketLimit::new(0);
        for i in 0..50 {
            assert_eq!(limit.on_garbage(i * WINDOW_TICKS / 4), Verdict::Accept);
        }
    }

    /// Turns the view at `degrees_per_second` for `ticks` inputs.
    fn turn(watch: &mut Watch, yaw: &mut f32, degrees_per_second: f32, ticks: usize) {
        for _ in 0..ticks {
            *yaw += (degrees_per_second / TICK_RATE as f32).to_radians();
            watch.on_input(*yaw, 0.0);
        }
    }

    #[test]
    fn a_fast_controller_or_a_quick_mouse_flick_is_never_flagged() {
        let mut watch = Watch::default();
        let mut yaw = 0.0;
        let mut now = 0;
        for i in 0..200 {
            // A controller at full boost (about 400 degrees a second), or a
            // hard 90-degree flick in a tenth of a second, then a hit.
            let rate = if i % 2 == 0 { 400.0 } else { 900.0 };
            turn(&mut watch, &mut yaw, rate, 12);
            now += 12;
            assert_eq!(watch.on_hit(now), None, "hit {i}");
            turn(&mut watch, &mut yaw, 0.0, 30);
            now += 30;
        }
        assert_eq!(watch.snap_hits, 0);
        assert!(watch.is_clear());
    }

    #[test]
    fn an_aimbot_snapping_onto_its_hits_is_flagged_once() {
        let mut watch = Watch::default();
        let mut yaw = 0.0;
        let mut flags = Vec::new();
        for i in 0..40u32 {
            // Wander slowly, then snap 60 degrees in a tick and hit.
            turn(&mut watch, &mut yaw, 30.0, 40);
            yaw += 60_f32.to_radians() * if i % 2 == 0 { 1.0 } else { -1.0 };
            watch.on_input(yaw, 0.0);
            flags.extend(watch.on_hit(i * 41));
        }
        assert_eq!(flags, vec![Flag::AimSnaps], "flagged once a minute at most");
        assert_eq!(watch.snap_hits, 40);
    }

    #[test]
    fn honest_view_ticks_pass_and_stretched_ones_flag() {
        let mut watch = Watch::default();
        for i in 0..200 {
            // A stray one now and then, as a round trip estimate lags a spike.
            assert_eq!(watch.on_shot(i % 25 == 0, i), None);
        }
        let mut flags = Vec::new();
        for i in 0..40 {
            flags.extend(watch.on_shot(true, 200 + i));
        }
        assert_eq!(flags, vec![Flag::ViewTicks]);
    }

    #[test]
    fn jitter_is_not_lag_switching_but_repeated_stalls_are() {
        let mut watch = Watch::default();
        let mut now = 0;
        // Jitter: a few ticks late, then two at once every half second; and
        // a quarter-second stall every 20 s on a bad connection.
        for i in 0..20_000u32 {
            let applied = match (i % 60, i % 2400) {
                (_, 100..=130) => 0,
                (_, 131) => 3,
                (0..=5, _) => 0,
                (6, _) => 2,
                _ => 1,
            };
            assert_eq!(watch.on_tick(applied, now), None, "tick {i}");
            now += 1;
        }
        // A switch flipped every two seconds: 300 ms out, then a burst.
        let mut flags = Vec::new();
        for _ in 0..30 {
            for _ in 0..36 {
                flags.extend(watch.on_tick(0, now));
                now += 1;
            }
            flags.extend(watch.on_tick(3, now));
            for _ in 0..200 {
                flags.extend(watch.on_tick(1, now));
                now += 1;
            }
        }
        assert_eq!(flags, vec![Flag::LagSwitch]);
    }
}
