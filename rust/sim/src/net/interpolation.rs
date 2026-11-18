//! Snapshot interpolation for remote players.
//!
//! Other players are drawn a little in the past, between two snapshots the
//! client already has, so their motion is smooth even though snapshots arrive
//! at uneven times. [`SnapshotClock`] tracks the server's tick from snapshot
//! arrivals and picks the render time; [`InterpolationBuffer`] holds each
//! remote player's recent states and samples them at that time.

use std::collections::VecDeque;

#[cfg(test)]
use super::TICK_DT;
use super::{TICK_RATE, Tick};

/// How fast the remembered worst snapshot gap shrinks back once packets stop
/// going missing: ticks of gap forgotten per tick that passes (0.6 ticks a
/// second), so a lossy patch keeps the delay wide for several seconds rather
/// than freezing remote players at every burst.
const GAP_DECAY: f64 = 0.005;

/// Estimates the server tick and the delayed tick to render remote players at.
///
/// The delay is as short as the connection allows: room for one lost snapshot
/// (two snapshot intervals), widened to the longest recent gap between
/// snapshots when more go missing, plus a margin for arrival jitter. On a
/// clean link at 60 Hz snapshots that is about 35 to 50 ms. Every tick of delay
/// is time an opponent has already moved on the server, so a shorter delay
/// means less peeker's advantage and fewer deaths after reaching cover.
#[derive(Clone, Debug)]
pub struct SnapshotClock {
    /// Estimated current server tick (fractional).
    server_tick: f64,
    /// Tick being rendered: `server_tick - delay`, moved smoothly.
    render_tick: f64,
    /// Average deviation of snapshot arrivals from the estimate, in ticks.
    jitter: f64,
    /// Ticks between snapshots the server sends.
    interval: f64,
    /// Newest snapshot tick received.
    newest: f64,
    /// Longest recent gap between snapshots received, in ticks, decaying by
    /// [`GAP_DECAY`]. Lost snapshots widen it.
    gap_peak: f64,
    /// Smallest interpolation delay, in ticks.
    min_delay: f64,
    started: bool,
}

impl SnapshotClock {
    /// `min_delay` is a floor on the interpolation delay in seconds. Zero lets
    /// the delay follow the connection alone; a floor is only for testing how
    /// a longer delay plays.
    pub fn new(min_delay: f32) -> Self {
        Self {
            server_tick: 0.0,
            render_tick: 0.0,
            jitter: 0.0,
            interval: 1.0,
            newest: 0.0,
            gap_peak: 0.0,
            min_delay: f64::from(min_delay.max(0.0)) * f64::from(TICK_RATE),
            started: false,
        }
    }

    /// Feeds a snapshot arrival.
    pub fn on_snapshot(&mut self, tick: Tick) {
        let tick = f64::from(tick);
        if !self.started {
            self.started = true;
            self.server_tick = tick;
            self.newest = tick;
            self.render_tick = tick - self.delay();
            return;
        }
        let error = tick - self.server_tick;
        if error.abs() > f64::from(TICK_RATE) / 2.0 {
            // Half a second off: a stall or a server restart. Start over.
            self.server_tick = tick;
            self.newest = tick;
            self.render_tick = tick - self.delay();
            self.jitter = 0.0;
            self.gap_peak = 0.0;
            return;
        }
        if tick > self.newest {
            let gap = tick - self.newest;
            self.gap_peak = gap.max(self.gap_peak - gap * GAP_DECAY);
            self.newest = tick;
        }
        self.jitter += (error.abs() - self.jitter) * 0.05;
        // Only pull forward fully; late packets nudge the estimate back gently.
        let gain = if error > 0.0 { 0.2 } else { 0.05 };
        self.server_tick += error * gain;
    }

    /// Records the server's snapshot interval (in ticks), which widens the delay.
    pub fn set_interval(&mut self, ticks: u32) {
        self.interval = f64::from(ticks.max(1));
    }

    /// Interpolation delay in ticks: room for a lost snapshot or the longest
    /// recent gap between snapshots, whichever is longer, plus jitter.
    ///
    /// Right after snapshot T arrives the client renders at T minus the delay;
    /// the next one arrives a gap later. Remote players keep moving smoothly
    /// only if the delay covers that gap, so a client that is losing packets
    /// draws a little further back until the loss stops.
    pub fn delay(&self) -> f64 {
        let gap = (self.interval * 2.0).max(self.gap_peak);
        self.min_delay.max(gap + self.jitter * 2.5)
    }

    /// Advances by `dt` seconds and returns the tick to render remote players at.
    pub fn advance(&mut self, dt: f32) -> f64 {
        let ticks = f64::from(dt) * f64::from(TICK_RATE);
        self.server_tick += ticks;
        let target = self.server_tick - self.delay();
        let error = target - self.render_tick;
        // Run up to 10% fast or slow to close the gap without visible jumps.
        let rate = 1.0 + (error * 0.1).clamp(-0.1, 0.1);
        self.render_tick += ticks * rate;
        if error.abs() > f64::from(TICK_RATE) / 4.0 {
            self.render_tick = target;
        }
        self.render_tick
    }

    pub fn server_tick(&self) -> f64 {
        self.server_tick
    }

    pub fn render_tick(&self) -> f64 {
        self.render_tick
    }

    pub fn jitter_ticks(&self) -> f64 {
        self.jitter
    }

    /// Longest recent gap between snapshots, in ticks.
    pub fn gap_ticks(&self) -> f64 {
        self.gap_peak
    }
}

/// Values that can be blended between two snapshots.
pub trait Interpolate: Clone {
    fn interpolate(&self, to: &Self, t: f32) -> Self;
}

/// One remote entity's states by server tick, oldest first.
#[derive(Clone, Debug)]
pub struct InterpolationBuffer<T> {
    entries: VecDeque<(Tick, T)>,
}

impl<T> Default for InterpolationBuffer<T> {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
        }
    }
}

impl<T: Interpolate> InterpolationBuffer<T> {
    /// Keeps about two seconds of history at most.
    const CAPACITY: usize = 120;

    /// Adds a state. Out-of-order snapshots are inserted in place; duplicates are ignored.
    pub fn push(&mut self, tick: Tick, value: T) {
        let index = self.entries.partition_point(|(t, _)| *t < tick);
        if self.entries.get(index).is_some_and(|(t, _)| *t == tick) {
            return;
        }
        self.entries.insert(index, (tick, value));
        while self.entries.len() > Self::CAPACITY {
            self.entries.pop_front();
        }
    }

    /// The state at `tick`, blended between the two snapshots around it. Past
    /// the newest snapshot it holds the newest one rather than guessing.
    pub fn sample(&mut self, tick: f64) -> Option<T> {
        // Drop everything older than the snapshot just before `tick`.
        while self.entries.len() > 2 && f64::from(self.entries[1].0) <= tick {
            self.entries.pop_front();
        }
        let (first_tick, first) = self.entries.front()?;
        let Some((to_tick, to)) = self.entries.get(1) else {
            return Some(first.clone());
        };
        if tick <= f64::from(*first_tick) {
            return Some(first.clone());
        }
        if tick >= f64::from(*to_tick) {
            // Only reached when `to` is the newest entry.
            return Some(to.clone());
        }
        let t = (tick - f64::from(*first_tick)) / f64::from(to_tick - first_tick);
        Some(first.interpolate(to, t as f32))
    }

    pub fn newest(&self) -> Option<&(Tick, T)> {
        self.entries.back()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Shortest-path blend between two angles in radians.
pub fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let diff = (b - a + PI).rem_euclid(TAU) - PI;
    a + diff * t
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Interpolate for f32 {
        fn interpolate(&self, to: &Self, t: f32) -> Self {
            self + (to - self) * t
        }
    }

    #[test]
    fn samples_between_snapshots() {
        let mut b = InterpolationBuffer::default();
        b.push(10, 0.0f32);
        b.push(14, 4.0);
        b.push(12, 2.0); // out of order
        assert_eq!(b.sample(11.0), Some(1.0));
        assert_eq!(b.sample(13.5), Some(3.5));
        // Past the newest snapshot: hold.
        assert_eq!(b.sample(20.0), Some(4.0));
        // History before the render time has been pruned.
        assert_eq!(b.len(), 2);
    }

    #[test]
    fn a_lost_snapshot_is_bridged() {
        let mut b = InterpolationBuffer::default();
        b.push(1, 0.0f32);
        b.push(4, 3.0);
        assert_eq!(b.sample(2.0), Some(1.0));
    }

    /// Feeds `seconds` of 60 Hz snapshots (every 2 ticks) through `clock`,
    /// rendering at 144 fps with arrivals processed on the next frame. A
    /// snapshot is dropped when `lost(i)` says so and arrives a snapshot late
    /// when `late(i)` does. Returns the newest snapshot sent and the share of
    /// frames, after the first second, whose render tick ran past the newest
    /// snapshot received (remote players frozen).
    fn run_link(
        clock: &mut SnapshotClock,
        first: u32,
        seconds: u32,
        lost: impl Fn(u32) -> bool,
        late: impl Fn(u32) -> bool,
    ) -> (u32, f64) {
        clock.set_interval(2);
        let frame = 1.0 / 144.0;
        let mut now = f64::from(first) * f64::from(TICK_DT);
        let mut received = 0u32;
        let mut frames = 0u32;
        let mut frozen = 0u32;
        let mut pending: Vec<(f64, u32)> = Vec::new();
        let snapshots = seconds * 60;
        let mut sent = first;
        let mut next = 0u32;
        while next < snapshots || !pending.is_empty() {
            now += frame;
            while next < snapshots && f64::from(first + 2 * next) * f64::from(TICK_DT) <= now {
                sent = first + 2 * next;
                if !lost(next) {
                    let delay = if late(next) { 2.0 / 60.0 } else { 0.0 };
                    pending.push((f64::from(sent) * f64::from(TICK_DT) + delay, sent));
                }
                next += 1;
            }
            pending.sort_by(|a, b| a.0.total_cmp(&b.0));
            while pending.first().is_some_and(|(at, _)| *at <= now) {
                let (_, tick) = pending.remove(0);
                clock.on_snapshot(tick);
                received = received.max(tick);
            }
            let render = clock.advance(frame as f32);
            // Skip the first second while the clock settles.
            if f64::from(sent - first) > f64::from(TICK_RATE) {
                frames += 1;
                if render > f64::from(received) {
                    frozen += 1;
                }
            }
        }
        (sent, f64::from(frozen) / f64::from(frames.max(1)))
    }

    #[test]
    fn a_clean_link_renders_close_behind_the_server() {
        let mut clock = SnapshotClock::new(0.0);
        let (newest, frozen) = run_link(&mut clock, 1000, 10, |_| false, |_| false);
        let behind = (clock.server_tick() - clock.render_tick()) / f64::from(TICK_RATE);
        assert!(
            (0.03..0.055).contains(&behind),
            "renders 30 to 55 ms behind, got {behind:.3} s"
        );
        assert!(frozen == 0.0, "never runs out of snapshots: {frozen:.3}");
        assert!((clock.server_tick() - f64::from(newest)).abs() < f64::from(TICK_RATE) * 0.05);
    }

    #[test]
    fn jitter_widens_the_delay() {
        let mut clock = SnapshotClock::new(0.0);
        let late = |i: u32| crate::rng::hash(i, 3) % 2 == 1;
        let (_, frozen) = run_link(&mut clock, 1000, 10, |_| false, late);
        let behind = (clock.server_tick() - clock.render_tick()) / f64::from(TICK_RATE);
        assert!(
            (0.04..0.12).contains(&behind),
            "half the snapshots a snapshot late: got {behind:.3} s"
        );
        assert!(frozen < 0.01, "remote players barely stall: {frozen:.3}");
    }

    #[test]
    fn packet_loss_widens_the_delay_then_it_recovers() {
        let mut clock = SnapshotClock::new(0.0);
        // 10% loss, with bursts of up to three snapshots in a row.
        let lost = |i: u32| {
            crate::rng::hash(i / 3, 11).is_multiple_of(30)
                || crate::rng::hash(i, 7).is_multiple_of(15)
        };
        let (_, frozen) = run_link(&mut clock, 1000, 10, lost, |_| false);
        let lossy = clock.delay();
        assert!(
            lossy >= 6.0,
            "covers a three-snapshot gap, got {lossy:.2} ticks"
        );
        assert!(
            frozen < 0.015,
            "gaps are bridged by blending, not frozen: {frozen:.3}"
        );
        // Twenty clean seconds later the delay is back to the clean-link value.
        let start = 1000 + 2 * 600 + 2;
        run_link(&mut clock, start, 20, |_| false, |_| false);
        assert!(clock.delay() < lossy, "{} < {lossy}", clock.delay());
        assert!(
            clock.delay() < 5.5,
            "recovered to {:.2} ticks",
            clock.delay()
        );
    }

    #[test]
    fn a_floor_holds_the_delay_back() {
        let mut clock = SnapshotClock::new(0.1);
        run_link(&mut clock, 1000, 5, |_| false, |_| false);
        assert!((clock.delay() - 12.0).abs() < 1e-4);
        let behind = (clock.server_tick() - clock.render_tick()) / f64::from(TICK_RATE);
        assert!((0.09..0.12).contains(&behind), "got {behind:.3} s");
    }

    #[test]
    fn clock_resyncs_after_a_stall() {
        let mut clock = SnapshotClock::new(0.0);
        clock.on_snapshot(100);
        clock.advance(TICK_DT);
        clock.on_snapshot(5000);
        let render = clock.advance(TICK_DT);
        assert!((render - (5000.0 - clock.delay())).abs() < 2.0);
    }

    #[test]
    fn angles_take_the_short_way_round() {
        use std::f32::consts::PI;
        let mid = lerp_angle(PI - 0.1, -PI + 0.1, 0.5);
        assert!((mid.abs() - PI).abs() < 1e-4, "got {mid}");
    }
}
