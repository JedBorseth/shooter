//! Care Package: a crate with a random scorestreak inside, dropped from a
//! flyover onto a spot the caller marks.
//!
//! Black Ops 2's Care Package (550): the caller marks a spot, an aircraft
//! flies over it and drops a crate, and whoever holds the use button at the
//! crate long enough takes the streak inside. The caller takes it almost at
//! once; anyone else, a teammate or an enemy stealing it, takes several
//! seconds (Call of Duty's rule since Modern Warfare 2, kept in Black Ops 2).
//! Only one player captures at a time: the first to start, until they let go
//! or step away. A crate nobody takes is gone after [`CRATE_SECONDS`].
//!
//! What is inside is rolled when it is called in, weighted the way Black Ops
//! 2 weights its contents toward the cheap streaks ([`roll`]). A crate never
//! holds another Care Package here, so one cannot chain into the next.
//!
//! The marked spot must be under open sky: the server checks that, and
//! falls back to the caller's feet (`docs/scorestreaks.md`).
//!
//! The server holds the crates and decides every capture ([`Crates`]);
//! clients draw the flyover, the fall, and the capture from events.

use glam::Vec3;

use crate::net::Tick;
use crate::rng;
use crate::streak::{Streak, seconds_to_ticks};

/// Seconds from the call to the aircraft passing over the spot and letting
/// the crate go, and the crate's fall from there.
pub const FLY_SECONDS: f32 = 4.0;
pub const FALL_SECONDS: f32 = 2.5;
/// How high over the spot the aircraft passes (metres), and how fast.
pub const FLY_HEIGHT: f32 = 40.0;
pub const FLY_SPEED: f32 = 45.0;
/// Seconds to capture one's own crate, and anyone else's (Call of Duty: the
/// owner "almost instantly", others about three seconds).
pub const OWNER_CAPTURE_SECONDS: f32 = 0.5;
pub const OTHER_CAPTURE_SECONDS: f32 = 3.0;
/// How near the crate (metres, horizontally, from the feet) a capture holds.
pub const CAPTURE_RADIUS: f32 = 1.8;
/// Seconds a landed crate waits to be taken (an estimate).
pub const CRATE_SECONDS: f32 = 90.0;
/// Crates waiting at once; the oldest goes first.
pub const MAX_CRATES: usize = 8;

/// The streaks a crate may hold and their weights: Black Ops 2's own
/// (`_supplydrop.gsc` `registercratetype`) for the streaks this game has:
/// UAV 100, Counter-UAV 100, Hellstorm Missile 85, Lightning Strike 80,
/// Sentry Gun 80, Stealth Chopper 20. A crate never holds a Care Package.
pub const CONTENTS: [(Streak, u32); 6] = [
    (Streak::Uav, 100),
    (Streak::CounterUav, 100),
    (Streak::GuidedMissile, 85),
    (Streak::Airstrike, 80),
    (Streak::SentryGun, 80),
    (Streak::AttackChopper, 20),
];

/// Rolls what a crate holds from `seed`.
pub fn roll(seed: u32) -> Streak {
    let total: u32 = CONTENTS.iter().map(|(_, w)| w).sum();
    let mut pick = rng::hash(seed, 0x0c4e) % total;
    for (streak, weight) in CONTENTS {
        if pick < weight {
            return streak;
        }
        pick -= weight;
    }
    CONTENTS[0].0
}

/// A capture under way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capture {
    pub by: u8,
    pub started: Tick,
    pub done_at: Tick,
}

/// A crate called in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crate {
    pub id: u16,
    pub owner: u8,
    /// Where it lands (the floor at the marked spot).
    pub at: Vec3,
    /// The way the aircraft flies over it (radians, 0 heading -z).
    pub heading: f32,
    /// When the aircraft lets it go, when it lands, and when it is gone.
    pub drop_at: Tick,
    pub lands_at: Tick,
    pub gone_at: Tick,
    pub contents: Streak,
    pub capture: Option<Capture>,
}

impl Crate {
    pub fn landed(&self, now: Tick) -> bool {
        now >= self.lands_at
    }
}

/// A player near the crates, as the capture rules need them.
#[derive(Clone, Copy, Debug)]
pub struct Taker {
    pub id: u8,
    pub feet: Vec3,
    /// Alive and holding the use button.
    pub holding: bool,
}

/// What changed with a crate on a tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Change {
    /// A capture started or stopped (see [`Crate::capture`]).
    Capture { id: u16 },
    /// Player `by` took crate `id` and the streak in it.
    Taken { id: u16, by: u8, contents: Streak },
    /// Crate `id` ran out or was pushed out by a newer one.
    Gone { id: u16 },
}

/// Every crate called in and not yet taken or gone.
#[derive(Clone, Debug, Default)]
pub struct Crates {
    pub list: Vec<Crate>,
    next_id: u16,
}

impl Crates {
    /// `owner` calls a crate in onto `at` at `now`; `seed` rolls its
    /// contents and the aircraft's heading. Returns the crate, and the one
    /// it pushed out if there were already [`MAX_CRATES`].
    pub fn call(&mut self, owner: u8, at: Vec3, now: Tick, seed: u32) -> (Crate, Option<Crate>) {
        let pushed = (self.list.len() >= MAX_CRATES).then(|| self.list.remove(0));
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let heading = (rng::hash(seed, 0xa1) % 360) as f32 * std::f32::consts::PI / 180.0;
        let drop_at = now + seconds_to_ticks(FLY_SECONDS);
        let lands_at = drop_at + seconds_to_ticks(FALL_SECONDS);
        let crate_ = Crate {
            id,
            owner,
            at,
            heading,
            drop_at,
            lands_at,
            gone_at: lands_at + seconds_to_ticks(CRATE_SECONDS),
            contents: roll(seed),
            capture: None,
        };
        self.list.push(crate_);
        (crate_, pushed)
    }

    /// Runs captures and expiry at `now`.
    pub fn update(&mut self, now: Tick, takers: &[Taker]) -> Vec<Change> {
        let mut changes = Vec::new();
        let near = |c: &Crate, t: &Taker| {
            let d = t.feet - c.at;
            t.holding && Vec3::new(d.x, 0.0, d.z).length() <= CAPTURE_RADIUS && d.y.abs() < 2.0
        };
        let mut taken = Vec::new();
        for c in &mut self.list {
            if !c.landed(now) {
                continue;
            }
            // The capture under way holds while its player stays.
            if let Some(cap) = c.capture {
                let staying = takers.iter().any(|t| t.id == cap.by && near(c, t));
                if !staying {
                    c.capture = None;
                    changes.push(Change::Capture { id: c.id });
                } else if now >= cap.done_at {
                    taken.push(c.id);
                    changes.push(Change::Taken {
                        id: c.id,
                        by: cap.by,
                        contents: c.contents,
                    });
                    continue;
                }
            }
            if c.capture.is_none()
                && let Some(t) = takers.iter().find(|t| near(c, t))
            {
                let seconds = if t.id == c.owner {
                    OWNER_CAPTURE_SECONDS
                } else {
                    OTHER_CAPTURE_SECONDS
                };
                c.capture = Some(Capture {
                    by: t.id,
                    started: now,
                    done_at: now + seconds_to_ticks(seconds),
                });
                changes.push(Change::Capture { id: c.id });
            }
        }
        self.list.retain(|c| !taken.contains(&c.id));
        for c in self.list.iter().filter(|c| now >= c.gone_at) {
            changes.push(Change::Gone { id: c.id });
        }
        self.list.retain(|c| now < c.gone_at);
        changes
    }

    /// Player `id` left: their captures stop, and the crates they called
    /// stay for anyone to take.
    pub fn forget(&mut self, id: u8) -> Vec<Change> {
        let mut changes = Vec::new();
        for c in &mut self.list {
            if c.capture.is_some_and(|cap| cap.by == id) {
                c.capture = None;
                changes.push(Change::Capture { id: c.id });
            }
        }
        changes
    }

    /// Every crate goes (a new match).
    pub fn clear(&mut self) -> Vec<Change> {
        let changes = self
            .list
            .iter()
            .map(|c| Change::Gone { id: c.id })
            .collect();
        self.list.clear();
        changes
    }

    /// The landed crate nearest `feet` within `range` metres, if any.
    pub fn nearest(&self, feet: Vec3, range: f32, now: Tick) -> Option<&Crate> {
        self.list
            .iter()
            .filter(|c| c.landed(now) && c.at.distance(feet) <= range)
            .min_by(|a, b| a.at.distance(feet).total_cmp(&b.at.distance(feet)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn taker(id: u8, feet: Vec3, holding: bool) -> Taker {
        Taker { id, feet, holding }
    }

    #[test]
    fn a_crate_lands_after_the_flyover_and_fall() {
        let mut crates = Crates::default();
        let (c, pushed) = crates.call(3, Vec3::ZERO, 1000, 7);
        assert!(pushed.is_none());
        assert_eq!(c.drop_at, 1000 + 480);
        assert_eq!(c.lands_at, 1000 + 480 + 300);
        // Nobody can capture it in the air.
        let early = crates.update(c.lands_at - 1, &[taker(3, Vec3::ZERO, true)]);
        assert!(early.is_empty());
        assert!(crates.list[0].capture.is_none());
    }

    #[test]
    fn the_owner_captures_quickly_and_others_slowly() {
        let mut crates = Crates::default();
        let (c, _) = crates.call(3, Vec3::ZERO, 0, 7);
        let t = c.lands_at;
        let changes = crates.update(t, &[taker(3, Vec3::new(1.0, 0.0, 0.0), true)]);
        assert_eq!(changes, vec![Change::Capture { id: c.id }]);
        let done = t + seconds_to_ticks(OWNER_CAPTURE_SECONDS);
        assert!(
            crates
                .update(done - 1, &[taker(3, Vec3::ZERO, true)])
                .is_empty()
        );
        let taken = crates.update(done, &[taker(3, Vec3::ZERO, true)]);
        assert_eq!(
            taken,
            vec![Change::Taken {
                id: c.id,
                by: 3,
                contents: c.contents
            }]
        );
        assert!(crates.list.is_empty());

        let (c, _) = crates.call(3, Vec3::ZERO, 0, 8);
        let t = c.lands_at;
        crates.update(t, &[taker(9, Vec3::ZERO, true)]);
        let cap = crates.list[0].capture.unwrap();
        assert_eq!(cap.by, 9);
        assert_eq!(cap.done_at - t, seconds_to_ticks(OTHER_CAPTURE_SECONDS));
    }

    #[test]
    fn letting_go_or_stepping_away_stops_a_capture_for_the_next_player() {
        let mut crates = Crates::default();
        let (c, _) = crates.call(3, Vec3::ZERO, 0, 7);
        let t = c.lands_at;
        crates.update(t, &[taker(9, Vec3::ZERO, true), taker(4, Vec3::X, true)]);
        assert_eq!(crates.list[0].capture.unwrap().by, 9);
        // 9 steps away: the capture stops, and 4, still holding, starts one.
        let changes = crates.update(
            t + 10,
            &[
                taker(9, Vec3::new(5.0, 0.0, 0.0), true),
                taker(4, Vec3::X, true),
            ],
        );
        assert_eq!(changes.len(), 2);
        assert_eq!(crates.list[0].capture.unwrap().by, 4);
        crates.update(t + 11, &[taker(4, Vec3::X, false)]);
        assert!(crates.list[0].capture.is_none());
    }

    #[test]
    fn an_untaken_crate_goes_and_the_oldest_makes_room() {
        let mut crates = Crates::default();
        let (c, _) = crates.call(3, Vec3::ZERO, 0, 7);
        let gone = crates.update(c.gone_at, &[]);
        assert_eq!(gone, vec![Change::Gone { id: c.id }]);
        for i in 0..MAX_CRATES {
            crates.call(3, Vec3::ZERO, 0, i as u32);
        }
        let (_, pushed) = crates.call(3, Vec3::ZERO, 0, 99);
        assert!(pushed.is_some());
        assert_eq!(crates.list.len(), MAX_CRATES);
    }

    #[test]
    fn contents_follow_the_weights() {
        let mut uavs = 0;
        let mut choppers = 0;
        for seed in 0..4000 {
            match roll(seed) {
                Streak::Uav => uavs += 1,
                Streak::AttackChopper => choppers += 1,
                Streak::CarePackage => panic!("a crate in a crate"),
                _ => {}
            }
        }
        // 100 of 465 (about 21.5%), and 20 of 465 (about 4.3%).
        assert!((760..960).contains(&uavs), "{uavs}");
        assert!((120..230).contains(&choppers), "{choppers}");
    }
}
