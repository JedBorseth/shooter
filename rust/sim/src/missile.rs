//! The Guided Missile scorestreak: a missile launched from high over the
//! map that its caller flies down from its nose camera, boosts, and splits
//! into a cluster of bomblets over the target.
//!
//! Black Ops 2's Hellstorm Missile (`_remotemissile.gsc`), the parts its
//! script gives:
//!
//! - The missile starts high over a point ahead of the caller
//!   (`missileremotelaunchtargetdist` 1500 units, 38 m) and back from it,
//!   18,000 units up and 7,000 back (a dive about 69° below level). Our
//!   maps are smaller and lower, so we scale the height down and keep the
//!   angle ([`LAUNCH_HEIGHT`], [`LAUNCH_BACK`]).
//! - It flies for at most 10 s (`missile_timeout_watch`, 9.95 s).
//! - Aim (ADS) boosts it until the cluster is fired; fire splits it. One
//!   bomblet goes to each enemy within 600 units (15 m) of where the nose
//!   points, Cold-Blooded players excepted (`specialty_nokillstreakreticle`),
//!   then random ones fill to nine, 200 to 550 units (5 to 14 m) out in
//!   sixths of a circle, plus one straight ahead, each a frame (50 ms)
//!   after the one before. The missile itself coasts on, unseen.
//! - Its camera stays on until a second after the first bomblet lands.
//!
//! Speeds, turning, and blasts live in the weapon files the scripts do not
//! carry; the numbers here are estimates recorded in `docs/scorestreaks.md`.

use crate::bot::wrap_angle;
use crate::net::Tick;
use crate::rng;
use crate::streak::seconds_to_ticks;
use crate::{TICK_RATE, Vec2, Vec3};

/// Where it starts: this high over the target point, and this far back
/// from it, away from the caller (Black Ops 2's 18,000 by 7,000 units,
/// scaled to keep the dive).
pub const LAUNCH_HEIGHT: f32 = 110.0;
pub const LAUNCH_BACK: f32 = 43.0;
/// The target point's distance ahead of the caller (1500 units).
pub const TARGET_AHEAD: f32 = 38.0;
/// Its speed (m/s) unboosted and boosted, and how fast its nose comes round
/// to where its pilot looks (radians a second). Estimates: about five
/// seconds down unboosted, two boosted.
pub const SPEED: f32 = 26.0;
pub const BOOST_SPEED: f32 = 65.0;
pub const TURN_RATE: f32 = 1.4;
/// Its steepest climb (it only ever dives) and its shallowest dive.
pub const MAX_PITCH: f32 = -0.17;
pub const MIN_PITCH: f32 = -1.55;
/// Seconds it flies before it goes off in the air.
pub const FLIGHT_SECONDS: f32 = 9.95;
/// The missile's own blast: full damage near, falling to the edge.
pub const BLAST_RADIUS: f32 = 9.0;
pub const BLAST_DAMAGE: f32 = 300.0;
pub const BLAST_OUTER_DAMAGE: f32 = 50.0;
/// The cluster: nine bomblets plus one straight ahead, aimed at enemies
/// within [`CLUSTER_TARGET_RADIUS`] of the aim point first, the rest
/// scattered [`CLUSTER_SCATTER`] metres out; a bomblet a frame.
pub const CLUSTER_SIZE: usize = 9;
pub const CLUSTER_TARGET_RADIUS: f32 = 15.0;
pub const CLUSTER_SCATTER: (f32, f32) = (5.0, 14.0);
pub const BOMBLET_GAP_SECONDS: f32 = 0.05;
pub const BOMBLET_SPEED: f32 = 70.0;
pub const BOMBLET_RADIUS: f32 = 4.5;
pub const BOMBLET_DAMAGE: f32 = 160.0;
pub const BOMBLET_OUTER_DAMAGE: f32 = 30.0;
/// Seconds the pilot's camera stays after the first bomblet lands.
pub const CAMERA_LINGER_SECONDS: f32 = 1.0;

/// One guided missile in the air.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Missile {
    pub id: u16,
    pub owner: u8,
    pub side: u8,
    pub position: Vec3,
    /// Its nose (yaw, pitch; yaw 0 faces -z).
    pub view: Vec2,
    pub boosting: bool,
    pub launched: Tick,
    /// Whether the cluster has gone (no more boost or steering).
    pub split: bool,
}

impl Missile {
    /// The missile for a caller standing at `feet` facing `yaw`, whose
    /// target point (the ground ahead) is `target`.
    pub fn launch(id: u16, owner: u8, side: u8, target: Vec3, yaw: f32, now: Tick) -> Self {
        let forward = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
        let position = target + Vec3::Y * LAUNCH_HEIGHT - forward * LAUNCH_BACK;
        let view = crate::look::bearing(position, target);
        Self {
            id,
            owner,
            side,
            position,
            view,
            boosting: false,
            launched: now,
            split: false,
        }
    }

    /// Which way it flies.
    pub fn direction(&self) -> Vec3 {
        crate::look::view_direction(self.view.x, self.view.y)
    }

    pub fn speed(&self) -> f32 {
        if self.boosting && !self.split {
            BOOST_SPEED
        } else {
            SPEED
        }
    }

    /// Whether its time is up at `now`.
    pub fn spent(&self, now: Tick) -> bool {
        now >= self.launched + seconds_to_ticks(FLIGHT_SECONDS)
    }

    /// One tick: the nose turns toward `steer` (the pilot's look, held to
    /// a dive) unless the cluster has gone, boosting as `boost` says, and
    /// it flies on. Returns the move it made (from, to).
    pub fn step(&mut self, steer: Vec2, boost: bool) -> (Vec3, Vec3) {
        let dt = 1.0 / TICK_RATE as f32;
        if !self.split {
            let goal = Vec2::new(steer.x, steer.y.clamp(MIN_PITCH, MAX_PITCH));
            let step = TURN_RATE * dt;
            self.view.x += wrap_angle(goal.x - self.view.x).clamp(-step, step);
            self.view.y += (goal.y - self.view.y).clamp(-step, step);
            self.boosting = boost;
        }
        let from = self.position;
        self.position += self.direction() * self.speed() * dt;
        (from, self.position)
    }

    /// Splits into the cluster at `now`, aimed round `aim` (where the nose
    /// points on the ground): a bomblet at each of `targets` (enemies'
    /// feet near the aim point, nearest first) up to nine, the rest
    /// scattered round it, and one straight ahead. `seed` scatters them.
    pub fn split(&mut self, aim: Vec3, targets: &[Vec3], now: Tick, seed: u32) -> Vec<Bomblet> {
        self.split = true;
        self.boosting = false;
        let gap = seconds_to_ticks(BOMBLET_GAP_SECONDS).max(1);
        let mut near: Vec<Vec3> = targets
            .iter()
            .copied()
            .filter(|t| Vec2::new(t.x - aim.x, t.z - aim.z).length() <= CLUSTER_TARGET_RADIUS)
            .collect();
        near.sort_by(|a, b| aim.distance(*a).total_cmp(&aim.distance(*b)));
        near.truncate(CLUSTER_SIZE);
        let mut bomblets: Vec<Bomblet> = Vec::new();
        let mut wait = 2;
        for t in &near {
            bomblets.push(self.bomblet(*t + Vec3::Y * 0.4, now + gap * wait));
            wait += 1;
        }
        // The straight one, with the first.
        bomblets.push(self.bomblet(self.position + self.direction() * 200.0, now + gap * 2));
        for i in near.len()..CLUSTER_SIZE {
            let sextant = (i % 6) as f32;
            let a = (10.0 + 60.0 * sextant + rng::unit(seed, i as u32) * 40.0).to_radians();
            let r = CLUSTER_SCATTER.0
                + rng::unit(seed, 100 + i as u32) * (CLUSTER_SCATTER.1 - CLUSTER_SCATTER.0);
            let to = aim + Vec3::new(a.cos() * r, 0.0, a.sin() * r);
            bomblets.push(self.bomblet(to, now + gap * wait));
            wait += 1;
        }
        bomblets
    }

    fn bomblet(&self, to: Vec3, fire_at: Tick) -> Bomblet {
        Bomblet {
            missile: self.id,
            owner: self.owner,
            side: self.side,
            from: self.position,
            to,
            fire_at,
        }
    }
}

/// One bomblet of a cluster: it leaves the missile's position at `fire_at`
/// and flies straight at `to` at [`BOMBLET_SPEED`], going off on what it
/// meets or at `to` (on through it, for the straight one, until the map).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bomblet {
    pub missile: u16,
    pub owner: u8,
    pub side: u8,
    pub from: Vec3,
    pub to: Vec3,
    pub fire_at: Tick,
}

impl Bomblet {
    /// Where it is at `tick` (fractional for drawing), or None before it
    /// fires.
    pub fn at(&self, tick: f64) -> Option<Vec3> {
        let seconds = (tick - f64::from(self.fire_at)) / f64::from(TICK_RATE);
        if seconds < 0.0 {
            return None;
        }
        let dir = (self.to - self.from).normalize_or_zero();
        Some(self.from + dir * BOMBLET_SPEED * seconds as f32)
    }

    /// Seconds from firing to reaching `to`.
    pub fn flight(&self) -> f32 {
        self.from.distance(self.to) / BOMBLET_SPEED
    }

    /// The tick it reaches `to`.
    pub fn lands_at(&self) -> Tick {
        self.fire_at + seconds_to_ticks(self.flight()).max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_launches_high_and_back_diving_at_the_target() {
        let target = Vec3::new(0.0, 2.0, -38.0);
        let m = Missile::launch(1, 3, 0, target, 0.0, 100);
        assert_eq!(m.position, Vec3::new(0.0, 112.0, 5.0));
        // Diving at the target along Black Ops 2's angle.
        let dive = -m.view.y.to_degrees();
        assert!((65.0..72.0).contains(&dive), "{dive}");
        assert!((m.direction() - (target - m.position).normalize()).length() < 1e-3);
    }

    #[test]
    fn it_turns_toward_the_look_and_boosts_until_the_split() {
        let mut m = Missile::launch(1, 3, 0, Vec3::ZERO, 0.0, 0);
        let start = m.view;
        // A full right turn in one tick is held to the turn rate.
        m.step(Vec2::new(start.x - 1.0, start.y), false);
        let turned = wrap_angle(start.x - m.view.x);
        assert!((turned - TURN_RATE / 120.0).abs() < 1e-4);
        // Looking up only levels it out to the shallowest dive.
        for _ in 0..600 {
            m.step(Vec2::new(m.view.x, 1.0), false);
        }
        assert!((m.view.y - MAX_PITCH).abs() < 1e-4);
        let (a, b) = m.step(m.view, true);
        assert!((a.distance(b) - BOOST_SPEED / 120.0).abs() < 1e-4);
        m.split(Vec3::ZERO, &[], 0, 1);
        let (a, b) = m.step(Vec2::new(0.0, -1.0), true);
        assert!((a.distance(b) - SPEED / 120.0).abs() < 1e-4);
        assert!((m.view.y - MAX_PITCH).abs() < 1e-4, "no more steering");
    }

    #[test]
    fn the_cluster_goes_to_enemies_near_the_aim_then_scatters() {
        let mut m = Missile::launch(1, 3, 0, Vec3::ZERO, 0.0, 0);
        m.position = Vec3::new(0.0, 40.0, 0.0);
        let near = [Vec3::new(3.0, 0.0, 2.0), Vec3::new(-8.0, 0.0, 6.0)];
        let far = Vec3::new(30.0, 0.0, 0.0);
        let bomblets = m.split(Vec3::ZERO, &[far, near[1], near[0]], 1000, 7);
        assert_eq!(bomblets.len(), CLUSTER_SIZE + 1);
        // Nearest enemy first, then the next, each a frame apart.
        assert!((bomblets[0].to - (near[0] + Vec3::Y * 0.4)).length() < 1e-4);
        assert!((bomblets[1].to - (near[1] + Vec3::Y * 0.4)).length() < 1e-4);
        assert!(bomblets[1].fire_at > bomblets[0].fire_at);
        assert!(bomblets.iter().all(|b| b.to.distance(far) > 1.0));
        // The scattered ones land 5 to 14 m out.
        for b in &bomblets[3..] {
            let r = Vec2::new(b.to.x, b.to.z).length();
            assert!((4.9..14.1).contains(&r), "{r}");
        }
        // A bomblet flies straight from the missile to its point.
        let b = bomblets[0];
        assert!(b.at(f64::from(b.fire_at) - 1.0).is_none());
        let landed = b.at(f64::from(b.lands_at())).unwrap();
        assert!(landed.distance(b.to) < BOMBLET_SPEED / 120.0 + 1e-3);
    }

    #[test]
    fn its_time_runs_out_after_ten_seconds() {
        let m = Missile::launch(1, 3, 0, Vec3::ZERO, 0.0, 0);
        assert!(!m.spent(seconds_to_ticks(9.9)));
        assert!(m.spent(seconds_to_ticks(9.95)));
    }
}
