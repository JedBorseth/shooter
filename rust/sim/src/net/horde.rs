//! The horde as sent: every zombie of the last second of snapshots that
//! carried them ([`ZombieView`]), and zombies sampled from it at any moment.
//!
//! Clients draw zombies from it, and the dedicated server rebuilds zombies'
//! hit zones from it for lag compensation, with the same code, so a shot
//! lands on the zombie the shooter saw. Zombies are drawn
//! [`protocol::zombie_lag`] behind the other players, since they are sent
//! less often ([`protocol::ZOMBIE_SEND_TICKS`]).

use std::collections::VecDeque;

use glam::Vec3;

use super::interpolation::lerp_angle;
use super::protocol::ZombieView;
use super::{TICK_DT, TICK_RATE, Tick};
use crate::hitbox::HitboxPose;
use crate::zombies::chase;

/// Ticks of history kept: comfortably more than the rewind cap plus the
/// zombie lag.
const HISTORY_TICKS: Tick = TICK_RATE;

/// One zombie at one moment, blended between the two sends around it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZombieSample {
    pub id: u8,
    /// Feet position.
    pub position: Vec3,
    /// The velocity between the two sends it is blended from (m/s); zero
    /// outside the history.
    pub velocity: Vec3,
    pub yaw: f32,
    pub alive: bool,
}

impl ZombieSample {
    fn at(view: &ZombieView) -> Self {
        Self {
            id: view.id,
            position: view.position(),
            velocity: Vec3::ZERO,
            yaw: view.yaw(),
            alive: view.alive,
        }
    }

    /// Walking rather than standing, for the pose.
    pub fn walking(&self) -> bool {
        Vec3::new(self.velocity.x, 0.0, self.velocity.z).length() > chase::WALKING_SPEED
    }

    /// Its hit zones, or `None` once dead.
    pub fn hitbox_pose(&self) -> Option<HitboxPose> {
        self.alive
            .then(|| chase::hitbox_pose(self.position, self.yaw, self.walking()))
    }
}

/// The zombies of recent sends, oldest first.
#[derive(Clone, Debug, Default)]
pub struct HordeHistory {
    frames: VecDeque<(Tick, Vec<ZombieView>)>,
}

impl HordeHistory {
    /// Records the zombies sent (or received) at `tick`. Older or repeated
    /// ticks are ignored.
    pub fn record(&mut self, tick: Tick, zombies: Vec<ZombieView>) {
        if self.frames.back().is_some_and(|(t, _)| *t >= tick) {
            return;
        }
        self.frames.push_back((tick, zombies));
        while self
            .frames
            .front()
            .is_some_and(|(t, _)| t + HISTORY_TICKS < tick)
        {
            self.frames.pop_front();
        }
    }

    pub fn clear(&mut self) {
        self.frames.clear();
    }

    /// The newest send, if any.
    pub fn newest(&self) -> Option<&(Tick, Vec<ZombieView>)> {
        self.frames.back()
    }

    /// Every zombie in the newest send.
    pub fn latest(&self) -> &[ZombieView] {
        self.frames.back().map_or(&[], |(_, z)| z.as_slice())
    }

    /// Zombie `id` at `tick`, blended between the sends around it; before
    /// the oldest it holds the oldest, past the newest the newest.
    pub fn sample(&self, id: u8, tick: f64) -> Option<ZombieSample> {
        let mut before: Option<(Tick, &ZombieView)> = None;
        let mut after: Option<(Tick, &ZombieView)> = None;
        for (t, zombies) in &self.frames {
            let Some(view) = zombies.iter().find(|z| z.id == id) else {
                continue;
            };
            if f64::from(*t) <= tick {
                before = Some((*t, view));
            } else {
                after = Some((*t, view));
                break;
            }
        }
        match (before, after) {
            (Some((from_tick, from)), Some((to_tick, to))) => {
                let span = f64::from(to_tick - from_tick);
                let t = ((tick - f64::from(from_tick)) / span) as f32;
                let (a, b) = (from.position(), to.position());
                Some(ZombieSample {
                    id,
                    position: a.lerp(b, t),
                    velocity: (b - a) / (span as f32 * TICK_DT),
                    yaw: lerp_angle(from.yaw(), to.yaw(), t),
                    alive: from.alive,
                })
            }
            (Some((_, only)), None) | (None, Some((_, only))) => Some(ZombieSample::at(only)),
            (None, None) => None,
        }
    }

    /// Every zombie of the newest send at `tick`.
    pub fn sample_all(&self, tick: f64) -> Vec<ZombieSample> {
        self.latest()
            .iter()
            .filter_map(|z| self.sample(z.id, tick))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blends_between_sends_and_knows_when_a_zombie_walks() {
        let mut history = HordeHistory::default();
        history.record(100, vec![ZombieView::new(3, Vec3::ZERO, 0.0, true)]);
        history.record(
            104,
            vec![ZombieView::new(3, Vec3::new(0.0, 0.0, -0.1), 0.0, true)],
        );
        let s = history.sample(3, 102.0).unwrap();
        assert!((s.position.z + 0.05).abs() < 1e-4);
        assert!((s.velocity.z + 3.0).abs() < 1e-3, "{:?}", s.velocity);
        assert!(s.walking());
        let pose = s.hitbox_pose().unwrap();
        assert_eq!(pose.activity.hunch, 1.0);
        // Past the newest send it holds still.
        let held = history.sample(3, 110.0).unwrap();
        assert!(!held.walking());
        assert!(history.sample(4, 102.0).is_none());
    }

    #[test]
    fn the_dead_have_no_hit_zones_and_old_sends_are_dropped() {
        let mut history = HordeHistory::default();
        history.record(0, vec![ZombieView::new(1, Vec3::ZERO, 1.0, false)]);
        assert!(history.sample(1, 0.0).unwrap().hitbox_pose().is_none());
        history.record(HISTORY_TICKS + 8, vec![]);
        assert!(history.sample(1, 0.0).is_none());
        assert!(history.latest().is_empty());
    }

    #[test]
    fn yaw_survives_the_wire() {
        let view = ZombieView::new(1, Vec3::new(1.234, 0.5, -7.891), -2.0, true);
        assert!((view.position() - Vec3::new(1.23, 0.5, -7.89)).length() < 0.006);
        let back = view.yaw();
        let diff = (back - (-2.0f32)).rem_euclid(std::f32::consts::TAU);
        assert!(diff.min(std::f32::consts::TAU - diff) < 0.02, "{back}");
    }
}
