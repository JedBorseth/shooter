//! Snapshot interpolation for remote players.
//!
//! Other players are drawn a little in the past, between two snapshots the
//! client already has, so their motion is smooth even though snapshots arrive
//! at uneven times. [`SnapshotClock`] tracks the server's tick from snapshot
//! arrivals and picks the render time; [`InterpolationBuffer`] holds each
//! remote player's recent states and samples them at that time.

use std::collections::VecDeque;

use super::{TICK_RATE, Tick};

/// Estimates the server tick and the delayed tick to render remote players at.
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
    /// Smallest interpolation delay, in ticks.
    min_delay: f64,
    started: bool,
}

impl SnapshotClock {
    /// `min_delay` is the interpolation delay in seconds (about 0.1 s is typical).
    pub fn new(min_delay: f32) -> Self {
        Self {
            server_tick: 0.0,
            render_tick: 0.0,
            jitter: 0.0,
            interval: 1.0,
            min_delay: f64::from(min_delay) * f64::from(TICK_RATE),
            started: false,
        }
    }

    /// Feeds a snapshot arrival.
    pub fn on_snapshot(&mut self, tick: Tick) {
        let tick = f64::from(tick);
        if !self.started {
            self.started = true;
            self.server_tick = tick;
            self.render_tick = tick - self.delay();
            return;
        }
        let error = tick - self.server_tick;
        if error.abs() > 30.0 {
            // Half a second off: a stall or a server restart. Start over.
            self.server_tick = tick;
            self.render_tick = tick - self.delay();
            self.jitter = 0.0;
            return;
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

    /// Interpolation delay in ticks: room for two snapshots plus jitter.
    pub fn delay(&self) -> f64 {
        self.min_delay.max(self.interval * 2.0 + self.jitter * 2.5)
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
        if error.abs() > 15.0 {
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

    #[test]
    fn clock_renders_behind_the_server_and_tracks_it() {
        let mut clock = SnapshotClock::new(0.1);
        let dt = 1.0 / 60.0;
        let mut render = 0.0;
        for tick in 1000..1600u32 {
            // Snapshots arrive with up to 2 ticks of jitter.
            clock.on_snapshot(tick - (crate::rng::hash(tick, 3) % 3));
            render = clock.advance(dt);
        }
        let behind = clock.server_tick() - render;
        assert!(
            (5.0..12.0).contains(&behind),
            "renders about 100 ms behind, got {behind:.2} ticks"
        );
        assert!((clock.server_tick() - 1599.0).abs() < 3.0);
    }

    #[test]
    fn clock_resyncs_after_a_stall() {
        let mut clock = SnapshotClock::new(0.1);
        clock.on_snapshot(100);
        clock.advance(1.0 / 60.0);
        clock.on_snapshot(5000);
        let render = clock.advance(1.0 / 60.0);
        assert!((render - (5000.0 - clock.delay())).abs() < 2.0);
    }

    #[test]
    fn angles_take_the_short_way_round() {
        use std::f32::consts::PI;
        let mid = lerp_angle(PI - 0.1, -PI + 0.1, 0.5);
        assert!((mid.abs() - PI).abs() < 1e-4, "got {mid}");
    }
}
