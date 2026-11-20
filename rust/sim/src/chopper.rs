//! The Attack Chopper scorestreak: an AI helicopter that guards a point its
//! caller marks on the map for a minute, hopping between spots round it and
//! firing its gun on the nearest enemy it can see.
//!
//! Black Ops 2's Stealth Chopper (`_helicopter.gsc`, `helicopter_comlink`),
//! the numbers its script gives:
//!
//! - It guards the marked point for 60 s (`heli_protect_time`), hovering
//!   12 s at a spot (`heli_protect_pos_time`) before moving to another
//!   random spot 20% to 100% of 2,000 units from the point
//!   (`heli_random_point_in_radius`). Our maps are smaller, so the spots are
//!   4 to 20 m out ([`SPOT_RADIUS`]).
//! - 1000 health (`heli_maxhealth`); bullets do half their damage
//!   (`heli_armor_bulletdamage`); any rocket downs it
//!   (`rocketdamageoneshot`).
//! - Its gun: a target picked at most every 0.6 s (`heli_targeting_delay`),
//!   0.7 s spin-up on a new target (`heli_turret_spinup_delay`), bursts of
//!   20 (`heli_turretclipsize`), 1.5 s between bursts
//!   (`heli_turretreloadtime`), out to 3,500 units (89 m,
//!   `heli_visual_range`), and players spawned in the last 5 s are left
//!   alone (`heli_target_spawnprotection`). It goes for the nearest player
//!   it can see. Blind Eye hides a player from it
//!   (`cantargetplayerwithspecialty`).
//! - One a team in the air at a time (`_killstreakrules.gsc`, "chopper").
//!
//! It flies in from the map's edge and leaves the same way. The gun's rate
//! and damage live in its weapon file, which the scripts do not carry:
//! those are estimates, as are the flying speeds.

use crate::bot::{turn_toward, view_gap};
use crate::net::Tick;
use crate::rng;
use crate::streak::seconds_to_ticks;
use crate::{TICK_RATE, Vec2, Vec3};

pub const HEALTH: f32 = 1000.0;
pub const BULLET_SCALE: f32 = 0.5;
pub const LIFETIME_SECONDS: f32 = 60.0;
pub const HOVER_SECONDS: f32 = 12.0;
/// How far from the guarded point its spots are (Black Ops 2's 400 to
/// 2,000 units, scaled to our maps).
pub const SPOT_RADIUS: (f32, f32) = (4.0, 20.0);
/// Metres over the map's highest floor it flies (Black Ops 2's
/// `air_support_height` is about 850 units, 22 m).
pub const HEIGHT: f32 = 22.0;
/// Flying speeds (m/s): in from the edge and away, and between spots.
/// Estimates.
pub const ENTRY_SPEED: f32 = 25.0;
pub const SPOT_SPEED: f32 = 10.0;
/// How far out it comes in from, and how long it takes to go.
pub const ENTRY_DISTANCE: f32 = 90.0;
pub const LEAVE_SECONDS: f32 = 4.0;
pub const TARGETING_SECONDS: f32 = 0.6;
pub const SPIN_UP_SECONDS: f32 = 0.7;
pub const BURST: u8 = 20;
pub const RELOAD_SECONDS: f32 = 1.5;
pub const RANGE: f32 = 89.0;
pub const SPAWN_PROTECTION_SECONDS: f32 = 5.0;
/// Its gun: rounds a minute, damage a round, spread (radians), and how fast
/// it turns onto a target (radians a second). Estimates for a 20 mm chin
/// gun: three rounds kill.
pub const ROUNDS_PER_MINUTE: f32 = 600.0;
pub const DAMAGE: f32 = 40.0;
pub const SPREAD: f32 = 0.035;
pub const TURN_RATE: f32 = 3.0;
pub const FIRE_CONE: f32 = 0.08;
/// The sphere round it that shots meet, and where its gun hangs.
pub const HIT_RADIUS: f32 = 3.5;
pub const GUN_DROP: f32 = 1.2;
/// Score for shooting one down (as for a UAV; Black Ops 2 publishes none).
pub const SHOT_DOWN_POINTS: u32 = 100;

/// An Attack Chopper called in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chopper {
    pub id: u16,
    pub owner: u8,
    pub side: u8,
    /// The point it guards, at its flying height.
    pub guard: Vec3,
    /// The heading (radians) it comes in on and leaves along.
    pub heading: f32,
    pub called: Tick,
    pub seed: u32,
    pub damage: f32,
    /// Its gun's aim (yaw, pitch), its target and since when, rounds left
    /// in the burst, and the ticks it may next fire and next pick.
    pub aim: Vec2,
    pub target: Option<(u8, Tick)>,
    burst_left: u8,
    next_shot: Tick,
    next_pick: Tick,
    shots: u32,
}

/// A player the chopper's gun could go for (see `turret::Candidate`):
/// `ignored` for Blind Eye or spawn protection.
#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    pub id: u8,
    pub side: u8,
    pub feet: Vec3,
    pub ignored: bool,
}

/// A round from its gun.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Round {
    pub chopper: u16,
    pub owner: u8,
    pub from: Vec3,
    pub direction: Vec3,
}

impl Chopper {
    pub fn new(id: u16, owner: u8, side: u8, guard: Vec3, called: Tick, seed: u32) -> Self {
        let heading = rng::unit(seed, 1) * std::f32::consts::TAU;
        Self {
            id,
            owner,
            side,
            guard,
            heading,
            called,
            seed,
            damage: 0.0,
            aim: Vec2::new(heading, -0.6),
            target: None,
            burst_left: BURST,
            next_shot: called,
            next_pick: called,
            shots: 0,
        }
    }

    /// The tick it starts to leave, and the tick it is gone.
    pub fn leaves_at(&self) -> Tick {
        self.called + seconds_to_ticks(LIFETIME_SECONDS)
    }

    pub fn gone_at(&self) -> Tick {
        self.leaves_at() + seconds_to_ticks(LEAVE_SECONDS)
    }

    /// The `k`th spot it hovers at: 4 to 20 m from the point, at random.
    pub fn spot(&self, k: u32) -> Vec3 {
        let angle = rng::unit(self.seed, 10 + 2 * k) * std::f32::consts::TAU;
        let r = SPOT_RADIUS.0 + rng::unit(self.seed, 11 + 2 * k) * (SPOT_RADIUS.1 - SPOT_RADIUS.0);
        self.guard + Vec3::new(angle.cos() * r, 0.0, angle.sin() * r)
    }

    /// Where it comes in from (and goes back to): out along its heading.
    pub fn edge(&self) -> Vec3 {
        let out = Vec3::new(-self.heading.sin(), 0.0, -self.heading.cos());
        self.guard - out * ENTRY_DISTANCE
    }

    /// Where it is at `tick` (fractional, for drawing): in from the edge to
    /// its first spot, hovering at each spot then moving to the next, and
    /// at the end back out to the edge. Both ends compute it alike.
    pub fn position(&self, tick: f64) -> Vec3 {
        let rate = f64::from(TICK_RATE);
        let mut t = ((tick - f64::from(self.called)) / rate).max(0.0) as f32;
        let leave = LIFETIME_SECONDS;
        let at_leave = |c: &Chopper| c.path_at(leave);
        if t >= leave {
            let from = at_leave(self);
            let away = (self.edge() - from).normalize_or_zero();
            return from + away * ENTRY_SPEED * (t - leave);
        }
        t = t.min(leave);
        self.path_at(t)
    }

    fn path_at(&self, t: f32) -> Vec3 {
        let mut from = self.edge();
        let mut left = t;
        let mut k = 0;
        let mut speed = ENTRY_SPEED;
        loop {
            let to = self.spot(k);
            let travel = from.distance(to) / speed;
            if left < travel {
                let s = ease(left / travel);
                return from.lerp(to, s);
            }
            left -= travel;
            if left < HOVER_SECONDS {
                return to;
            }
            left -= HOVER_SECONDS;
            from = to;
            k += 1;
            speed = SPOT_SPEED;
        }
    }

    /// Where its gun hangs at `tick`.
    pub fn gun(&self, tick: Tick) -> Vec3 {
        self.position(f64::from(tick)) - Vec3::Y * GUN_DROP
    }

    /// Whether `point` is within its gun's reach from `gun`.
    fn reaches(gun: Vec3, point: Vec3) -> bool {
        gun.distance(point) <= RANGE
    }

    /// One tick of its gun. At most every 0.6 s it picks the nearest player
    /// not on its side it can see (`sees` casts the ray), keeping one it is
    /// on while it can still see them. On a new target it spins up for
    /// 0.7 s, then fires bursts of 20 with 1.5 s between. It holds fire
    /// while leaving. Returns the round fired, if any.
    pub fn step(
        &mut self,
        now: Tick,
        candidates: &[Candidate],
        mut sees: impl FnMut(Vec3, Vec3) -> bool,
    ) -> Option<Round> {
        let dt = 1.0 / TICK_RATE as f32;
        if now >= self.leaves_at() {
            self.target = None;
            return None;
        }
        let gun = self.gun(now);
        let chest = |c: &Candidate| c.feet + Vec3::Y * 1.2;
        let usable = |c: &Candidate, sees: &mut dyn FnMut(Vec3, Vec3) -> bool| {
            c.side != self.side && !c.ignored && Self::reaches(gun, chest(c)) && sees(gun, chest(c))
        };
        if now >= self.next_pick {
            self.next_pick = now + seconds_to_ticks(TARGETING_SECONDS);
            let kept = self.target.and_then(|(id, since)| {
                let c = candidates.iter().find(|c| c.id == id)?;
                usable(c, &mut sees).then_some((id, since))
            });
            self.target = kept.or_else(|| {
                let mut near: Vec<&Candidate> = candidates
                    .iter()
                    .filter(|c| c.side != self.side && !c.ignored)
                    .filter(|c| Self::reaches(gun, chest(c)))
                    .collect();
                near.sort_by(|a, b| gun.distance(chest(a)).total_cmp(&gun.distance(chest(b))));
                near.into_iter()
                    .find(|c| sees(gun, chest(c)))
                    .map(|c| (c.id, now))
            });
        }
        let (id, since) = self.target?;
        let Some(target) = candidates.iter().find(|c| c.id == id && !c.ignored) else {
            self.target = None;
            return None;
        };
        let goal = crate::look::bearing(gun, chest(target));
        self.aim = turn_toward(self.aim, goal, TURN_RATE, dt);
        let spun = now >= since + seconds_to_ticks(SPIN_UP_SECONDS);
        if !spun || now < self.next_shot || view_gap(self.aim, goal) > FIRE_CONE {
            return None;
        }
        self.shots = self.shots.wrapping_add(1);
        self.burst_left = self.burst_left.saturating_sub(1);
        self.next_shot = if self.burst_left == 0 {
            self.burst_left = BURST;
            now + seconds_to_ticks(RELOAD_SECONDS)
        } else {
            now + seconds_to_ticks(60.0 / ROUNDS_PER_MINUTE).max(1)
        };
        let seed = u32::from(self.id) << 16 | self.shots & 0xffff;
        let view = self.aim
            + Vec2::new(
                rng::signed(seed, now) * SPREAD,
                rng::signed(seed, now ^ 0x27d4_eb2d) * SPREAD,
            );
        Some(Round {
            chopper: self.id,
            owner: self.owner,
            from: gun,
            direction: crate::look::view_direction(view.x, view.y),
        })
    }
}

fn ease(s: f32) -> f32 {
    let s = s.clamp(0.0, 1.0);
    s * s * (3.0 - 2.0 * s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chopper() -> Chopper {
        Chopper::new(1, 3, 0, Vec3::new(0.0, 30.0, 0.0), 0, 99)
    }

    fn enemy(id: u8, feet: Vec3) -> Candidate {
        Candidate {
            id,
            side: 1,
            feet,
            ignored: false,
        }
    }

    #[test]
    fn it_flies_in_hovers_at_spots_round_the_point_and_leaves() {
        let c = chopper();
        assert!(c.position(0.0).distance(c.edge()) < 1e-3);
        let arrive = c.edge().distance(c.spot(0)) / ENTRY_SPEED;
        let first = c.position(f64::from(seconds_to_ticks(arrive + 1.0)));
        assert!(
            first.distance(c.spot(0)) < 1e-3,
            "hovering at the first spot"
        );
        for k in 0..6 {
            let r = Vec2::new(c.spot(k).x, c.spot(k).z).length();
            assert!(
                (SPOT_RADIUS.0 - 1e-3..=SPOT_RADIUS.1 + 1e-3).contains(&r),
                "{r}"
            );
            assert!((c.spot(k).y - 30.0).abs() < 1e-4);
        }
        // Twelve seconds at the spot, then on to the next.
        let moving = c.position(f64::from(seconds_to_ticks(arrive + HOVER_SECONDS + 0.5)));
        assert!(moving.distance(c.spot(0)) > 0.01);
        // After a minute it heads back out and is gone four seconds later.
        let leaving = c.position(f64::from(c.leaves_at() + seconds_to_ticks(2.0)));
        let at_leave = c.position(f64::from(c.leaves_at()));
        assert!(leaving.distance(c.edge()) < at_leave.distance(c.edge()));
        assert_eq!(c.gone_at(), seconds_to_ticks(64.0));
    }

    #[test]
    fn its_gun_spins_up_then_fires_bursts_at_the_nearest_it_sees() {
        let mut c = chopper();
        let arrive = seconds_to_ticks(c.edge().distance(c.spot(0)) / ENTRY_SPEED + 0.1);
        let near = enemy(7, c.spot(0) - Vec3::Y * 30.0 + Vec3::X * 5.0);
        let far = enemy(8, c.spot(0) - Vec3::Y * 30.0 + Vec3::X * 25.0);
        let hidden = Candidate {
            ignored: true,
            ..enemy(9, c.spot(0) - Vec3::Y * 29.0)
        };
        let list = [far, near, hidden];
        let mut fired = Vec::new();
        for now in arrive..arrive + seconds_to_ticks(6.0) {
            if let Some(round) = c.step(now, &list, |_, _| true) {
                fired.push(now);
                let to_near = (near.feet + Vec3::Y * 1.2 - round.from).normalize();
                assert!(round.direction.dot(to_near) > 0.99);
            }
        }
        assert!(c.target.is_some_and(|(id, _)| id == 7));
        let first = fired[0];
        assert!(first >= arrive + seconds_to_ticks(SPIN_UP_SECONDS));
        // A burst of twenty, then a pause of a second and a half.
        let gap = fired[BURST as usize] - fired[BURST as usize - 1];
        assert!(gap >= seconds_to_ticks(RELOAD_SECONDS));
        assert!(fired[1] - fired[0] < seconds_to_ticks(0.2));
    }

    #[test]
    fn it_leaves_out_hidden_unseen_and_far_players() {
        let mut c = chopper();
        let arrive = seconds_to_ticks(c.edge().distance(c.spot(0)) / ENTRY_SPEED + 0.1);
        let hidden = Candidate {
            ignored: true,
            ..enemy(7, Vec3::ZERO)
        };
        let walled = enemy(8, Vec3::new(2.0, 0.0, 0.0));
        let far = enemy(9, Vec3::new(200.0, 0.0, 0.0));
        for now in arrive..arrive + seconds_to_ticks(4.0) {
            assert!(c.step(now, &[hidden], |_, _| true).is_none());
            assert!(c.step(now, &[walled], |_, _| false).is_none());
            assert!(c.step(now, &[far], |_, _| true).is_none());
        }
    }
}
