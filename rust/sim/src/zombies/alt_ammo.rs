//! Alternate ammo types for upgraded weapons: Black Ops III's AATs under our
//! own names. Re-packing an upgraded weapon at the forge
//! ([`super::forge::REPACK_PRICE`]) gives it one at random, never the one it
//! has; hits then sometimes set it off.
//!
//! Two, from Black Ops III's `scripts/zm/aats/` and `aat_shared.gsc`:
//!
//! - **Arc** (`_zm_aat_dead_wire`): the zombie hit is electrocuted, and the
//!   charge jumps to the nearest zombie in reach, and on, up to
//!   [`ARC_JUMPS`] jumps and [`ARC_KILLS`] kills, its reach starting at
//!   [`ARC_REACH`] and shrinking [`ARC_REACH_DECAY`] a jump, so that in a
//!   tight pack it runs out after six
//!   (`create_lightning_chain_params(8, 9, 120)`, decay 20 units). A 20%
//!   chance a hit; 5 s before the same player's weapon can set it off
//!   again, 2 s before anyone's can.
//! - **Cinder** (`_zm_aat_blast_furnace`): the zombie hit bursts into
//!   flame and dies, and every zombie within [`CINDER_REACH`] catches fire,
//!   losing a sixth of its health every half second, seven times, from a
//!   quarter second on: dead in about three seconds. A 15% chance a hit; 15 s
//!   before the same player's weapon can set it off again.
//!
//! Either can go off on the killing hit (`occurs_on_death`). Everything it
//! kills pays the shooter as a kill.

use glam::Vec3;

use crate::rng;

/// An upgraded weapon's alternate ammo type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AltAmmo {
    Arc,
    Cinder,
}

/// Jumps the Arc makes from the zombie hit, at most.
pub const ARC_JUMPS: usize = 8;
/// Zombies the Arc kills, at most (the one hit included).
pub const ARC_KILLS: usize = 9;
/// The Arc's first jump reaches this far (120 units), each next one
/// [`ARC_REACH_DECAY`] less (20 units).
pub const ARC_REACH: f32 = 3.05;
pub const ARC_REACH_DECAY: f32 = 0.51;
/// Seconds the Arc takes for each jump, for the drawn bolts.
pub const ARC_JUMP_SECONDS: f32 = 0.11;

/// Zombies within this many metres of the one hit catch fire (120 units).
pub const CINDER_REACH: f32 = 3.05;
/// Seconds before the first burn, between burns, and how many there are.
pub const BURN_DELAY: f32 = 0.25;
pub const BURN_INTERVAL: f32 = 0.5;
pub const BURNS: u32 = 7;
/// Each burn takes this share of the health it had when it caught fire.
pub const BURN_SHARE: f32 = 1.0 / 6.0;

impl AltAmmo {
    pub const ALL: [AltAmmo; 2] = [AltAmmo::Arc, AltAmmo::Cinder];

    pub fn code(self) -> u8 {
        match self {
            AltAmmo::Arc => 0,
            AltAmmo::Cinder => 1,
        }
    }

    pub fn from_code(code: u8) -> Option<Self> {
        Self::ALL.get(usize::from(code)).copied()
    }

    pub fn key(self) -> &'static str {
        match self {
            AltAmmo::Arc => "arc",
            AltAmmo::Cinder => "cinder",
        }
    }

    /// The name the HUD shows beside the weapon's.
    pub fn label(self) -> &'static str {
        match self {
            AltAmmo::Arc => "ARC",
            AltAmmo::Cinder => "CINDER",
        }
    }

    /// Chance a hit sets it off.
    pub fn chance(self) -> f32 {
        match self {
            AltAmmo::Arc => 0.2,
            AltAmmo::Cinder => 0.15,
        }
    }

    /// Seconds before the same player's weapon can set it off again
    /// (`cooldown_time_attacker`).
    pub fn shooter_cooldown(self) -> f32 {
        match self {
            AltAmmo::Arc => 5.0,
            AltAmmo::Cinder => 15.0,
        }
    }

    /// Seconds before anyone's can (`cooldown_time_global`).
    pub fn global_cooldown(self) -> f32 {
        match self {
            AltAmmo::Arc => 2.0,
            AltAmmo::Cinder => 0.0,
        }
    }

    /// A re-pack's new type, from `seed`: never the one the weapon has.
    pub fn repack(current: Option<AltAmmo>, seed: u32) -> AltAmmo {
        let choices: Vec<AltAmmo> = Self::ALL
            .into_iter()
            .filter(|a| Some(*a) != current)
            .collect();
        choices[rng::hash(seed, 0x6161_7421) as usize % choices.len()]
    }
}

/// When each alternate ammo type last went off: per shooter, and for anyone.
#[derive(Clone, Debug, Default)]
pub struct Cooldowns {
    /// (shooter, type, seconds into the game it went off).
    shooters: Vec<(u8, AltAmmo, f32)>,
    global: Vec<(AltAmmo, f32)>,
}

impl Cooldowns {
    /// Whether a hit by `shooter` with `kind`, at `now` (seconds into the
    /// game) with `roll` (0..1), sets it off; records it if so.
    pub fn try_fire(&mut self, shooter: u8, kind: AltAmmo, now: f32, roll: f32) -> bool {
        let shooter_ready = self
            .shooters
            .iter()
            .find(|(s, k, _)| *s == shooter && *k == kind)
            .is_none_or(|(_, _, at)| now > at + kind.shooter_cooldown());
        let global_ready = self
            .global
            .iter()
            .find(|(k, _)| *k == kind)
            .is_none_or(|(_, at)| now > at + kind.global_cooldown());
        if !shooter_ready || !global_ready || roll >= kind.chance() {
            return false;
        }
        self.shooters
            .retain(|(s, k, _)| !(*s == shooter && *k == kind));
        self.shooters.push((shooter, kind, now));
        self.global.retain(|(k, _)| *k != kind);
        self.global.push((kind, now));
        true
    }
}

/// The Arc's path from zombie `start` through `zombies` (ids and where they
/// stand, the one hit among them): each jump to the nearest zombie not yet
/// struck within the jump's reach. Returns the ids struck, `start` first.
pub fn arc_path(start: u8, zombies: &[(u8, Vec3)]) -> Vec<u8> {
    let Some(&(_, mut at)) = zombies.iter().find(|(id, _)| *id == start) else {
        return Vec::new();
    };
    let mut struck = vec![start];
    let mut reach = ARC_REACH;
    for _ in 0..ARC_JUMPS {
        if struck.len() >= ARC_KILLS || reach <= 0.0 {
            break;
        }
        let next = zombies
            .iter()
            .filter(|(id, _)| !struck.contains(id))
            .map(|(id, p)| (*id, *p, p.distance(at)))
            .filter(|(_, _, d)| *d <= reach)
            .min_by(|a, b| a.2.total_cmp(&b.2));
        let Some((id, p, _)) = next else {
            break;
        };
        struck.push(id);
        at = p;
        reach -= ARC_REACH_DECAY;
    }
    struck
}

/// The zombies Cinder sets on fire around the one hit at `at`: those
/// within [`CINDER_REACH`], other than `start` itself.
pub fn cinder_targets(start: u8, at: Vec3, zombies: &[(u8, Vec3)]) -> Vec<u8> {
    zombies
        .iter()
        .filter(|(id, p)| *id != start && p.distance(at) <= CINDER_REACH)
        .map(|(id, _)| *id)
        .collect()
}

/// A zombie on fire: the damage each burn does and how many are left, and
/// seconds to the next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Burn {
    pub zombie: u8,
    pub shooter: u8,
    pub damage: f32,
    pub left: u32,
    pub wait: f32,
}

impl Burn {
    /// A zombie with `health` catching fire from `shooter`'s Cinder.
    pub fn new(zombie: u8, shooter: u8, health: f32) -> Self {
        Self {
            zombie,
            shooter,
            damage: health * BURN_SHARE,
            left: BURNS,
            wait: BURN_DELAY,
        }
    }

    /// Advances it `dt`: the damage of a burn due now, if one is.
    pub fn step(&mut self, dt: f32) -> Option<f32> {
        if self.left == 0 {
            return None;
        }
        self.wait -= dt;
        if self.wait > 0.0 {
            return None;
        }
        self.wait += BURN_INTERVAL;
        self.left -= 1;
        Some(self.damage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TICK_DT;

    #[test]
    fn a_repack_never_gives_the_same_type() {
        for seed in 0..50 {
            assert_eq!(AltAmmo::repack(Some(AltAmmo::Arc), seed), AltAmmo::Cinder);
            assert_eq!(AltAmmo::repack(Some(AltAmmo::Cinder), seed), AltAmmo::Arc);
        }
        let first: std::collections::HashSet<_> =
            (0..50).map(|s| AltAmmo::repack(None, s)).collect();
        assert_eq!(first.len(), 2);
        for a in AltAmmo::ALL {
            assert_eq!(AltAmmo::from_code(a.code()), Some(a));
        }
    }

    #[test]
    fn cooldowns_hold_per_shooter_and_for_everyone() {
        let mut c = Cooldowns::default();
        assert!(!c.try_fire(1, AltAmmo::Arc, 0.0, 0.5), "rolled over 20%");
        assert!(c.try_fire(1, AltAmmo::Arc, 0.0, 0.1));
        assert!(!c.try_fire(2, AltAmmo::Arc, 1.0, 0.0), "2 s for anyone");
        assert!(c.try_fire(2, AltAmmo::Arc, 2.5, 0.0));
        assert!(
            !c.try_fire(1, AltAmmo::Arc, 4.0, 0.0),
            "5 s for the same shooter"
        );
        assert!(c.try_fire(1, AltAmmo::Arc, 5.5, 0.0));
        assert!(c.try_fire(1, AltAmmo::Cinder, 5.6, 0.1));
        assert!(!c.try_fire(1, AltAmmo::Cinder, 20.0, 0.0));
        assert!(
            c.try_fire(2, AltAmmo::Cinder, 6.0, 0.0),
            "no wait for others"
        );
    }

    #[test]
    fn the_arc_jumps_to_the_nearest_in_ever_shorter_reach() {
        // A line of zombies 2 m apart, and one off to the side out of reach.
        let mut zombies: Vec<(u8, Vec3)> = (0..12)
            .map(|i| (i, Vec3::new(i as f32 * 2.0, 0.0, 0.0)))
            .collect();
        zombies.push((50, Vec3::new(0.0, 0.0, 9.0)));
        let path = arc_path(0, &zombies);
        // Reach 3.05, 2.54, 2.03, then 1.52 m: three jumps of 2 m.
        assert_eq!(path, vec![0, 1, 2, 3]);
        // A tight pack: the reach runs out after six jumps.
        let pack: Vec<(u8, Vec3)> = (0..20)
            .map(|i| {
                (
                    i,
                    Vec3::new((i % 5) as f32 * 0.3, 0.0, (i / 5) as f32 * 0.3),
                )
            })
            .collect();
        assert_eq!(arc_path(7, &pack).len(), 7);
        assert!(arc_path(99, &pack).is_empty());
    }

    #[test]
    fn cinder_burns_its_neighbours_to_death_in_three_seconds() {
        let zombies = [
            (1, Vec3::ZERO),
            (2, Vec3::new(2.0, 0.0, 1.0)),
            (3, Vec3::new(5.0, 0.0, 0.0)),
        ];
        assert_eq!(cinder_targets(1, Vec3::ZERO, &zombies), vec![2]);
        let mut health = 950.0;
        let mut burn = Burn::new(2, 7, health);
        let mut t = 0.0;
        while health > 0.0 && t < 10.0 {
            if let Some(d) = burn.step(TICK_DT) {
                health -= d;
            }
            t += TICK_DT;
        }
        assert!(health <= 0.0);
        assert!((2.6..3.4).contains(&t), "dead in {t} s");
    }
}
