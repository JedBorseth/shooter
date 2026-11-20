//! The Sentry Gun scorestreak: an automated gun set down where its caller
//! stands, which fires at enemies in front of it until it runs out of time
//! or is destroyed.
//!
//! Black Ops 2's Sentry Gun (`_turret_killstreak.gsc`, the "sentry"
//! auto-turret), the numbers its script gives:
//!
//! - 650 health; bullets do 60% of their damage, explosives double
//!   (a launcher's rocket counts 200, so two destroy it), the knife nothing
//!   ([`TurretHit`]).
//! - It runs for 240 s once placed, and waits 1.6 s after it is set down
//!   before it looks for targets (`turretinitdelay`).
//! - At most four a team and eight in all (`_killstreakrules.gsc`, the
//!   "turret" rule).
//!
//! Its field of fire, reach, turn rate, rate of fire, and damage live in the
//! turret's weapon file, which the scripts do not carry: the numbers here
//! are estimates (a half circle in front, 40 m, a fast traverse, and a
//! light machine gun's fire), recorded in `docs/scorestreaks.md`.

use crate::bot::wrap_angle;
use crate::net::Tick;
use crate::rng;
use crate::streak::seconds_to_ticks;
use crate::{TICK_RATE, Vec2, Vec3};

/// Black Ops 2's `setupturrethealth`: 650 health, bullets at 0.6, blasts
/// at 2.
pub const HEALTH: f32 = 650.0;
pub const BULLET_SCALE: f32 = 0.6;
pub const EXPLOSIVE_SCALE: f32 = 2.0;
/// What a launcher's rocket counts before the scale (Black Ops 2: 200 for
/// the SMAW and FHJ-18).
pub const ROCKET_DAMAGE: f32 = 200.0;
/// Seconds it runs once placed (`level.auto_turret_timeout`), and before it
/// first looks for targets (`turretinitdelay`).
pub const LIFETIME_SECONDS: f32 = 240.0;
pub const ARM_SECONDS: f32 = 1.6;
/// Most up at once, on one side and in all (Black Ops 2's "turret" rule).
pub const MAX_PER_SIDE: usize = 4;
pub const MAX_IN_ALL: usize = 8;
/// Its field of fire: this far either side of the way it was set down
/// facing (radians; a half circle), as far as `RANGE` metres, and its
/// barrel's most elevation and depression. Estimates.
pub const ARC: f32 = std::f32::consts::FRAC_PI_2;
pub const RANGE: f32 = 40.0;
pub const MAX_PITCH: f32 = 0.9;
/// How fast it slews (radians a second), and how near its aim must be to a
/// target to fire (radians). Estimates.
pub const TURN_RATE: f32 = 4.0;
pub const FIRE_CONE: f32 = 0.06;
/// Seconds from first seeing a target to firing, rounds a minute, damage a
/// round, and the spread of its rounds (radians). Estimates: about a
/// third of a second to kill once it is on someone.
pub const SPIN_UP_SECONDS: f32 = 0.35;
pub const ROUNDS_PER_MINUTE: f32 = 600.0;
pub const DAMAGE: f32 = 25.0;
pub const SPREAD: f32 = 0.025;
/// Height of its barrel over its feet (the training sentry's model:
/// slewing ring at 1.1 m, the gun 0.15 m above it).
pub const MUZZLE_HEIGHT: f32 = 1.25;
/// A body's half-width and height, as rounds meet it.
pub const BODY_RADIUS: f32 = 0.35;
pub const BODY_HEIGHT: f32 = 1.8;
/// The sphere around its head that shots and blasts meet.
pub const HIT_RADIUS: f32 = 0.6;
pub const HIT_HEIGHT: f32 = 1.0;
/// Score for destroying an enemy's (as for a UAV; Black Ops 2 publishes
/// none).
pub const DESTROY_POINTS: u32 = 100;

/// What hit a turret, which sets how much of the damage counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurretHit {
    Bullet,
    Explosive,
    Rocket,
    Melee,
}

impl TurretHit {
    /// The damage a hit of `damage` does to a turret.
    pub fn scaled(self, damage: f32) -> f32 {
        match self {
            TurretHit::Bullet => damage * BULLET_SCALE,
            TurretHit::Explosive => damage * EXPLOSIVE_SCALE,
            TurretHit::Rocket => ROCKET_DAMAGE * EXPLOSIVE_SCALE,
            TurretHit::Melee => 0.0,
        }
    }
}

/// One Sentry Gun set down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Turret {
    pub id: u16,
    pub owner: u8,
    /// The owner's side (team, or their own id without teams).
    pub side: u8,
    /// Its feet, and the way it was set down facing (yaw; 0 faces -z).
    pub at: Vec3,
    pub facing: f32,
    /// When it was set down, and when it runs out.
    pub placed: Tick,
    pub until: Tick,
    pub damage: f32,
    /// Where its barrel points (yaw, pitch).
    pub aim: Vec2,
    /// The player it is on, and since when.
    pub target: Option<(u8, Tick)>,
    next_shot: Tick,
    shots: u32,
}

impl Turret {
    pub fn muzzle(&self) -> Vec3 {
        self.at + Vec3::Y * MUZZLE_HEIGHT
    }

    /// The centre of the sphere shots meet it in.
    pub fn centre(&self) -> Vec3 {
        self.at + Vec3::Y * HIT_HEIGHT
    }

    /// Whether `point` is in its field of fire: in front within the arc,
    /// in reach, and within the barrel's elevation.
    pub fn covers(&self, point: Vec3) -> bool {
        let d = point - self.muzzle();
        let level = Vec2::new(d.x, d.z).length();
        if d.length() > RANGE || level < 0.01 {
            return false;
        }
        let bearing = crate::fmath::atan2(-d.x, -d.z);
        let pitch = crate::fmath::atan2(d.y, level);
        wrap_angle(bearing - self.facing).abs() <= ARC && pitch.abs() <= MAX_PITCH
    }

    /// Whether it is up and looking (past its arming delay).
    pub fn armed(&self, now: Tick) -> bool {
        now >= self.placed + seconds_to_ticks(ARM_SECONDS)
    }
}

/// A target a turret could fire on: who, their side, their feet, and
/// whether they can be seen from it (the caller casts the ray), and whether
/// sentries pass them by (a perk).
#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    pub id: u8,
    pub side: u8,
    pub feet: Vec3,
    pub ignored: bool,
}

/// A round fired: from which turret and whose, from where, which way, and
/// the player it was fired at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Round {
    pub turret: u16,
    pub owner: u8,
    pub from: Vec3,
    pub direction: Vec3,
    pub at: u8,
}

/// Every Sentry Gun up.
#[derive(Clone, Debug, Default)]
pub struct Turrets {
    pub list: Vec<Turret>,
    next_id: u16,
}

impl Turrets {
    /// Whether `side` may set another down (Black Ops 2's limits).
    pub fn room_for(&self, side: u8) -> bool {
        self.list.len() < MAX_IN_ALL
            && self.list.iter().filter(|t| t.side == side).count() < MAX_PER_SIDE
    }

    /// `owner` on `side` sets one down at `at`, facing `facing`, at `now`.
    pub fn place(&mut self, owner: u8, side: u8, at: Vec3, facing: f32, now: Tick) -> Turret {
        let turret = Turret {
            id: self.next_id,
            owner,
            side,
            at,
            facing,
            placed: now,
            until: now + seconds_to_ticks(LIFETIME_SECONDS),
            damage: 0.0,
            aim: Vec2::new(facing, 0.0),
            target: None,
            next_shot: now,
            shots: 0,
        };
        self.next_id = self.next_id.wrapping_add(1);
        self.list.push(turret);
        turret
    }

    /// Turret `id` takes a hit. Returns it, taken away, if that destroys it.
    pub fn hurt(&mut self, id: u16, hit: TurretHit, damage: f32) -> Option<Turret> {
        let index = self.list.iter().position(|t| t.id == id)?;
        self.list[index].damage += hit.scaled(damage);
        (self.list[index].damage >= HEALTH).then(|| self.list.remove(index))
    }

    /// Takes away every turret that has run out by `now`, and returns them.
    pub fn expire(&mut self, now: Tick) -> Vec<Turret> {
        let (gone, kept): (Vec<Turret>, Vec<Turret>) =
            self.list.iter().partition(|t| t.until <= now);
        self.list = kept;
        gone
    }

    /// The first turret not on `side` that the segment `from` to `to`
    /// passes within its sphere: its id and how far along (0 to 1).
    pub fn first_on_segment(&self, from: Vec3, to: Vec3, side: u8) -> Option<(u16, f32)> {
        self.list
            .iter()
            .filter(|t| t.side != side)
            .filter_map(|t| {
                let at = crate::streak::segment_meets_sphere(from, to, t.centre(), HIT_RADIUS)?;
                Some((t.id, at))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// One tick: each armed turret keeps on (or picks) the nearest enemy in
    /// its field of fire it can see (`sees` casts the ray from its muzzle),
    /// slews toward them, and fires once on them after its spin-up. Returns
    /// the rounds fired.
    pub fn step(
        &mut self,
        now: Tick,
        candidates: &[Candidate],
        mut sees: impl FnMut(Vec3, Vec3) -> bool,
    ) -> Vec<Round> {
        let dt = 1.0 / TICK_RATE as f32;
        let gap = seconds_to_ticks(60.0 / ROUNDS_PER_MINUTE).max(1);
        let mut rounds = Vec::new();
        for turret in &mut self.list {
            if !turret.armed(now) {
                continue;
            }
            let muzzle = turret.muzzle();
            let chest = |c: &Candidate| c.feet + Vec3::Y * (BODY_HEIGHT * 0.7);
            let usable = |c: &Candidate, sees: &mut dyn FnMut(Vec3, Vec3) -> bool| {
                c.side != turret.side
                    && !c.ignored
                    && turret.covers(chest(c))
                    && sees(muzzle, chest(c))
            };
            // Keep the target while it can still be fired on.
            let kept = turret.target.and_then(|(id, since)| {
                let c = candidates.iter().find(|c| c.id == id)?;
                usable(c, &mut sees).then_some((*c, since))
            });
            let picked = kept.or_else(|| {
                candidates
                    .iter()
                    .filter(|c| c.side != turret.side && !c.ignored)
                    .filter(|c| turret.covers(chest(c)))
                    .min_by(|a, b| {
                        muzzle
                            .distance(chest(a))
                            .total_cmp(&muzzle.distance(chest(b)))
                    })
                    .filter(|c| sees(muzzle, chest(c)))
                    .map(|c| (*c, now))
            });
            let Some((target, since)) = picked else {
                turret.target = None;
                // Back to watching the way it was set down.
                let rest = Vec2::new(turret.facing, 0.0);
                turret.aim = crate::bot::turn_toward(turret.aim, rest, TURN_RATE * 0.5, dt);
                continue;
            };
            turret.target = Some((target.id, since));
            let goal = crate::look::bearing(muzzle, chest(&target));
            turret.aim = crate::bot::turn_toward(turret.aim, goal, TURN_RATE, dt);
            let on = crate::bot::view_gap(turret.aim, goal) <= FIRE_CONE;
            let spun = now >= since + seconds_to_ticks(SPIN_UP_SECONDS);
            if on && spun && now >= turret.next_shot {
                turret.next_shot = now + gap;
                turret.shots = turret.shots.wrapping_add(1);
                let seed = u32::from(turret.id) << 16 | turret.shots & 0xffff;
                let spread = Vec2::new(
                    rng::signed(seed, now) * SPREAD,
                    rng::signed(seed, now ^ 0x5bd1_e995) * SPREAD,
                );
                let view = turret.aim + spread;
                rounds.push(Round {
                    turret: turret.id,
                    owner: turret.owner,
                    from: muzzle,
                    direction: crate::look::view_direction(view.x, view.y),
                    at: target.id,
                });
            }
        }
        rounds
    }

    pub fn clear(&mut self) {
        self.list.clear();
    }
}

/// Where a round from `from` along `direction` meets a body standing at
/// `feet`, in metres along it, if it does (within [`RANGE`]).
pub fn round_meets_body(from: Vec3, direction: Vec3, feet: Vec3) -> Option<f32> {
    let to = from + direction * RANGE;
    crate::equipment::segment_hits_body(from, to, feet, BODY_HEIGHT, BODY_RADIUS).map(|t| t * RANGE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enemy(id: u8, feet: Vec3) -> Candidate {
        Candidate {
            id,
            side: 1,
            feet,
            ignored: false,
        }
    }

    #[test]
    fn health_takes_bullets_at_sixty_percent_and_two_rockets() {
        let mut turrets = Turrets::default();
        let t = turrets.place(1, 0, Vec3::ZERO, 0.0, 0).id;
        // 650 / (30 * 0.6) = 36.1: the 37th round of 30 destroys it.
        for _ in 0..36 {
            assert!(turrets.hurt(t, TurretHit::Bullet, 30.0).is_none());
        }
        assert!(turrets.hurt(t, TurretHit::Bullet, 30.0).is_some());
        let r = turrets.place(1, 0, Vec3::ZERO, 0.0, 0).id;
        assert!(turrets.hurt(r, TurretHit::Melee, 500.0).is_none());
        assert!(turrets.hurt(r, TurretHit::Rocket, 0.0).is_none());
        assert!(turrets.hurt(r, TurretHit::Rocket, 0.0).is_some());
    }

    #[test]
    fn it_covers_a_half_circle_in_front_within_reach() {
        let mut turrets = Turrets::default();
        // Facing -z.
        let t = turrets.place(1, 0, Vec3::ZERO, 0.0, 0);
        assert!(t.covers(Vec3::new(0.0, 1.0, -20.0)));
        assert!(t.covers(Vec3::new(15.0, 1.0, -0.5)));
        assert!(!t.covers(Vec3::new(0.0, 1.0, 20.0)), "behind");
        assert!(!t.covers(Vec3::new(0.0, 1.0, -45.0)), "out of reach");
        assert!(!t.covers(Vec3::new(0.0, 30.0, -5.0)), "too high");
    }

    #[test]
    fn it_arms_spins_up_then_fires_on_the_nearest_enemy_in_sight() {
        let mut turrets = Turrets::default();
        turrets.place(1, 0, Vec3::ZERO, 0.0, 0);
        let near = enemy(7, Vec3::new(2.0, 0.0, -10.0));
        let far = enemy(8, Vec3::new(-3.0, 0.0, -25.0));
        let friend = Candidate {
            side: 0,
            ..enemy(9, Vec3::new(0.0, 0.0, -4.0))
        };
        let list = [far, near, friend];
        let arm = seconds_to_ticks(ARM_SECONDS);
        let mut first = None;
        let mut fired_at = Vec::new();
        for now in 0..arm + seconds_to_ticks(2.0) {
            for round in turrets.step(now, &list, |_, _| true) {
                first.get_or_insert(now);
                fired_at.push(round.at);
                assert!(round_meets_body(round.from, round.direction, near.feet).is_some());
            }
        }
        let first = first.expect("it fires");
        assert!(first >= arm + seconds_to_ticks(SPIN_UP_SECONDS));
        assert!(fired_at.iter().all(|id| *id == 7));
        // 600 rounds a minute: about ten a second.
        let per_second = fired_at.len() as f32 / ((arm + 240 - first) as f32 / 120.0);
        assert!((8.0..=11.0).contains(&per_second), "{per_second}");
    }

    #[test]
    fn walls_and_perks_hide_players_from_it() {
        let mut turrets = Turrets::default();
        turrets.place(1, 0, Vec3::ZERO, 0.0, 0);
        let hidden = Candidate {
            ignored: true,
            ..enemy(7, Vec3::new(0.0, 0.0, -10.0))
        };
        let walled = enemy(8, Vec3::new(0.0, 0.0, -12.0));
        for now in 0..seconds_to_ticks(5.0) {
            assert!(turrets.step(now, &[hidden], |_, _| true).is_empty());
            assert!(turrets.step(now, &[walled], |_, _| false).is_empty());
        }
    }

    #[test]
    fn limits_and_running_out() {
        let mut turrets = Turrets::default();
        for i in 0..MAX_PER_SIDE {
            assert!(turrets.room_for(0));
            turrets.place(i as u8, 0, Vec3::ZERO, 0.0, 0);
        }
        assert!(!turrets.room_for(0));
        assert!(turrets.room_for(1));
        let life = seconds_to_ticks(LIFETIME_SECONDS);
        assert!(turrets.expire(life - 1).is_empty());
        assert_eq!(turrets.expire(life).len(), MAX_PER_SIDE);
    }

    #[test]
    fn shots_meet_its_head_from_the_other_side_only() {
        let mut turrets = Turrets::default();
        let t = turrets.place(1, 0, Vec3::new(0.0, 0.0, -10.0), 0.0, 0);
        let from = Vec3::new(0.0, 1.0, 0.0);
        let to = Vec3::new(0.0, 1.0, -20.0);
        assert_eq!(
            turrets.first_on_segment(from, to, 1).map(|h| h.0),
            Some(t.id)
        );
        assert!(turrets.first_on_segment(from, to, 0).is_none());
    }
}
