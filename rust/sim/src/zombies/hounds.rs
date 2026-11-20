//! Hound rounds: every few rounds the zombies stay away and a pack of fast
//! four-legged hounds comes instead, each one dropped into the arena in a
//! flash, and the last one killed leaves a Max Ammo. Black Ops' dog rounds
//! under our own name and look.
//!
//! Numbers from Black Ops III's `_zm_ai_dogs.gsc`:
//!
//! - The first comes 4 to 6 rounds into the game (`round_number +
//!   randomintrange(4, 7)` from round 1, so round 5, 6, or 7), then every
//!   4 or 5 rounds (`randomintrange(4, 6)`) (`dog_round_tracker`).
//! - Six hounds a player in the first two hound rounds, eight after
//!   (`dog_round_spawning`).
//! - Health 400, 900, 1300, then 1600 for every hound round after
//!   (`dog_health_increase`).
//! - At most two alive a player (`num_player_valid * 2`).
//! - The first 7 s after the round starts (`wait(1)`, then `wait(6)` for
//!   the announcement), then one every 3, 2.5, 2, or 1.5 s by hound round,
//!   less the share of the pack already sent (`waiting_for_next_dog_spawn`).
//! - A hound shows up 1.5 s after the strike that brings it
//!   (`dog_spawn_fx`), at a spot 10 to 25 m from the player it hunts
//!   (`dog_spawn_factory_logic`: 400 to 1000 units).
//! - It bites for 40 of a player's 100 health (`meleedamage`), with a
//!   longer reach than a zombie's swing (`ai_meleeRange` 100 units).
//!
//! Like zombies, hounds run straight at the nearest standing player: no
//! pathfinding (a project decision).

use glam::Vec3;

use super::ZombieRules;
use crate::hitbox::{Activity, HitboxPose};
use crate::movement::Stance;
use crate::rng;

/// A hound's hit zones at `feet`, facing `yaw`
/// ([`crate::hitbox::hound_hitboxes`]).
pub fn hitbox_pose(feet: Vec3, yaw: f32) -> HitboxPose {
    HitboxPose {
        activity: Activity {
            hound: true,
            ..Activity::default()
        },
        ..HitboxPose::new(feet, yaw, 0.0, Stance::Standing)
    }
}

/// The round of the first hound round: 5, 6, or 7.
pub fn first_hound_round(seed: u32) -> u32 {
    1 + 4 + rng::hash(seed, 0x686f_756e) % 3
}

/// The hound round after one in `round`: 4 or 5 rounds later.
pub fn next_hound_round(seed: u32, round: u32) -> u32 {
    round + 4 + rng::hash(seed, round) % 2
}

/// Hounds in the `nth` hound round of a game (from 1) for `players`.
pub fn hounds_in_round(nth: u32, players: u32) -> u32 {
    let per_player = if nth < 3 { 6 } else { 8 };
    per_player * players.max(1)
}

/// A hound's health in the `nth` hound round.
pub fn hound_health(nth: u32) -> f32 {
    match nth {
        0 | 1 => 400.0,
        2 => 900.0,
        3 => 1300.0,
        _ => 1600.0,
    }
}

/// Most hounds alive at once for `players` standing.
pub fn max_alive(players: u32) -> u32 {
    2 * players.max(1)
}

/// Seconds from the round's start to the first hound's strike.
pub const FIRST_SPAWN_SECONDS: f32 = 7.0;
/// Seconds from the strike to the hound standing there.
pub const STRIKE_SECONDS: f32 = 1.5;
/// A hound's strike lands this far from the player it hunts (metres), when
/// the map has a spot in that ring.
pub const SPAWN_MIN_DISTANCE: f32 = 10.2;
pub const SPAWN_MAX_DISTANCE: f32 = 25.4;

/// Seconds to the next hound after `sent` of `total` in the `nth` hound
/// round.
pub fn spawn_interval(nth: u32, sent: u32, total: u32) -> f32 {
    let base = match nth {
        0 | 1 => 3.0,
        2 => 2.5,
        3 => 2.0,
        _ => 1.5,
    };
    (base - sent as f32 / total.max(1) as f32).max(0.05)
}

/// Share of a player's full health a bite takes (40 of 100).
pub const BITE_SHARE: f32 = 0.4;
/// Bulwark's 250 health against Black Ops' 100: a bite takes 40 of 250.
pub const BULWARK_HEALTH_FACTOR: f32 = 2.5;

/// Damage a hound's bite does to a player with `max_health`: three bites
/// in a row down a player, seven with Bulwark (the zombies' drink rules,
/// [`super::drinks`]).
pub fn bite_damage(max_health: f32, bulwark: bool) -> f32 {
    let share = if bulwark {
        BITE_SHARE / BULWARK_HEALTH_FACTOR
    } else {
        BITE_SHARE
    };
    max_health * share
}

/// How hounds move and bite: faster than any zombie but still a little
/// slower than a sprinting player (7.4 m/s), with a quicker, longer bite.
pub fn rules() -> ZombieRules {
    ZombieRules {
        walk_speed: 6.2,
        jog_speed: 6.2,
        run_speed: 6.2,
        attack_reach: 1.6,
        attack_hit_reach: 2.3,
        attack_windup: 0.3,
        attack_cycle: 0.9,
        ..ZombieRules::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_hound_round_is_round_five_six_or_seven() {
        let mut seen = std::collections::HashSet::new();
        for seed in 0..200 {
            let round = first_hound_round(seed);
            assert!((5..=7).contains(&round));
            seen.insert(round);
            let next = next_hound_round(seed, round);
            assert!((round + 4..=round + 5).contains(&next));
        }
        assert_eq!(seen.len(), 3);
    }

    #[test]
    fn packs_grow_and_toughen() {
        assert_eq!(hounds_in_round(1, 1), 6);
        assert_eq!(hounds_in_round(2, 4), 24);
        assert_eq!(hounds_in_round(3, 2), 16);
        assert_eq!(hound_health(1), 400.0);
        assert_eq!(hound_health(2), 900.0);
        assert_eq!(hound_health(3), 1300.0);
        assert_eq!(hound_health(9), 1600.0);
        assert_eq!(max_alive(3), 6);
    }

    #[test]
    fn hounds_come_faster_as_the_pack_is_sent() {
        assert_eq!(spawn_interval(1, 0, 6), 3.0);
        assert!((spawn_interval(1, 3, 6) - 2.5).abs() < 1e-6);
        assert!((spawn_interval(5, 10, 10) - 0.5).abs() < 1e-6);
        assert!(spawn_interval(4, 20, 8) >= 0.05);
    }

    #[test]
    fn three_bites_down_a_player_without_a_rest() {
        let max = 100.0;
        let mut health = max;
        for _ in 0..2 {
            health -= bite_damage(max, false);
            assert!(health > 0.0);
        }
        health -= bite_damage(max, false);
        assert!(health <= 0.0);
        let bites = |bulwark| {
            let mut health = max;
            let mut n = 0;
            while health > 1e-3 {
                health -= bite_damage(max, bulwark);
                n += 1;
            }
            n
        };
        assert_eq!(bites(true), 7);
        let r = rules();
        assert!(r.run_speed < 7.4, "a sprinting player can get away");
        assert!(r.attack_reach > ZombieRules::default().attack_reach);
    }
}
