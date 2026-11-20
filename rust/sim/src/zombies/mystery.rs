//! The mystery crate: a random weapon for points, Black Ops' mystery box
//! under our own name and look.
//!
//! A press pays [`PRICE`] and opens the crate; weapons cycle in it for
//! [`CYCLE_SECONDS`], slowing down, and it lands on one drawn from the
//! weighted pool ([`POOL`]), never one the buyer already carries. The buyer
//! has [`TAKE_SECONDS`] to take it, then the crate closes. After enough uses
//! ([`should_move`]) it lands on the duck instead: the buyer gets the points
//! back, and the crate leaves for another of the map's spots.
//!
//! Numbers from Black Ops III's `_zm_magicbox.gsc`: the 950 cost, the
//! 40-step cycle (20 at 0.05 s, 10 at 0.1 s, 5 at 0.2 s, 3 at 0.3 s, then
//! 0.1 s), the 12 s `treasure_chest_timeout`, and `treasure_chest_should_move`'s
//! odds.

use crate::rng;
use crate::weapon::{CombatState, WeaponId};

/// Points a spin costs (`zombie_treasure_chest_cost`, 950).
pub const PRICE: u32 = 950;
/// Seconds weapons cycle before the crate lands on one: 20 x 0.05 +
/// 10 x 0.1 + 5 x 0.2 + 3 x 0.3 + 0.1.
pub const CYCLE_SECONDS: f32 = 4.0;
/// Seconds the buyer has to take the weapon (`treasure_chest_timeout`).
pub const TAKE_SECONDS: f32 = 12.0;
/// Seconds the duck floats over the crate before it leaves.
pub const DUCK_SECONDS: f32 = 3.0;
/// Seconds the crate is gone between leaving one spot and showing up at
/// the next (Black Ops' fly-away and drop-in, about this long).
pub const AWAY_SECONDS: f32 = 6.0;
/// Uses at one spot before the crate can move (`chest_min_move_usage`).
pub const MIN_USES_BEFORE_MOVE: u32 = 4;

/// The weapons the crate gives and their weights (a weapon's chance is its
/// weight over the total of those the buyer can take). Black Ops weights
/// its ordinary weapons equally; the sidearm every player starts with is
/// not in it.
pub const POOL: [(&str, u32); 8] = [
    ("m4", 10),
    ("ak47", 10),
    ("pdw9", 10),
    ("m870", 10),
    ("m40", 10),
    ("br3", 10),
    ("lsw6", 10),
    ("cq10", 10),
];

/// Whether this spin lands on the duck and the crate moves, after `uses`
/// spins at this spot (counting this one) and `moves` moves this game, with
/// `roll` a number from 0 to 99 (`treasure_chest_should_move`):
///
/// - fewer than 4 uses here: never;
/// - 4 to 7 uses: 15%;
/// - the crate has never moved and 8 or more uses: always;
/// - it has moved before: 30% from 8 to 12 uses, 50% from 13.
pub fn should_move(uses: u32, moves: u32, roll: u32) -> bool {
    if uses < MIN_USES_BEFORE_MOVE {
        return false;
    }
    let chance = match uses {
        4..=7 => 15,
        _ if moves == 0 => 100,
        8..=12 => 30,
        _ => 50,
    };
    roll < chance
}

/// The weapon a spin gives a player carrying `combat`, from `seed`: a
/// weighted draw from [`POOL`] among those they do not carry (plain or
/// upgraded). None if they carry every one.
pub fn draw(combat: &CombatState, seed: u32) -> Option<WeaponId> {
    let offered: Vec<(WeaponId, u32)> = POOL
        .iter()
        .filter_map(|(key, weight)| Some((WeaponId::by_key(key)?, *weight)))
        .filter(|(id, _)| !carries_any(combat, *id))
        .collect();
    let total: u32 = offered.iter().map(|(_, w)| w).sum();
    if total == 0 {
        return None;
    }
    let mut pick = rng::hash(seed, 0x6372_6174) % total;
    for (id, weight) in offered {
        if pick < weight {
            return Some(id);
        }
        pick -= weight;
    }
    None
}

/// Whether `combat` carries `weapon` in any form: itself, with
/// attachments, or upgraded.
pub fn carries_any(combat: &CombatState, weapon: WeaponId) -> bool {
    let key = weapon.def().model_key();
    combat
        .slots
        .iter()
        .any(|s| s.carried && s.id.def().model_key() == key)
}

/// The roll `should_move` takes for the `uses`th spin at a spot, from the
/// game's `seed`.
pub fn move_roll(seed: u32, uses: u32, moves: u32) -> u32 {
    rng::hash(
        seed ^ 0x6475_636b,
        uses.wrapping_mul(97).wrapping_add(moves),
    ) % 100
}

/// Seconds into the cycle at which each weapon shown in turn appears, for
/// clients to cycle the display the way Black Ops slows it down.
pub fn cycle_steps() -> impl Iterator<Item = f32> {
    let mut t = 0.0;
    (0..38).map(move |i| {
        let start = t;
        t += match i {
            0..20 => 0.05,
            20..30 => 0.1,
            30..35 => 0.2,
            _ => 0.3,
        };
        start
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crate_never_moves_in_its_first_three_uses() {
        for uses in 0..4 {
            for roll in 0..100 {
                assert!(!should_move(uses, 0, roll));
            }
        }
    }

    #[test]
    fn move_odds_follow_black_ops_iii() {
        let odds = |uses, moves| (0..100).filter(|r| should_move(uses, moves, *r)).count();
        assert_eq!(odds(4, 0), 15);
        assert_eq!(odds(7, 3), 15);
        assert_eq!(
            odds(8, 0),
            100,
            "the first move is certain by the eighth use"
        );
        assert_eq!(odds(8, 1), 30);
        assert_eq!(odds(12, 1), 30);
        assert_eq!(odds(13, 2), 50);
    }

    #[test]
    fn draws_skip_what_the_buyer_carries() {
        let combat = crate::zombies::start_combat();
        let mut seen = std::collections::HashSet::new();
        for seed in 0..500 {
            let id = draw(&combat, seed).expect("a weapon");
            assert_ne!(id.def().key, "m1911");
            seen.insert(id);
        }
        assert_eq!(seen.len(), POOL.len(), "every weapon in the pool comes up");
        let carrying = CombatState::new([WeaponId::expect("ak47"), WeaponId::expect("m1911")]);
        for seed in 0..200 {
            assert_ne!(draw(&carrying, seed), Some(WeaponId::expect("ak47")));
        }
    }

    #[test]
    fn the_cycle_lasts_about_four_seconds() {
        let steps: Vec<f32> = cycle_steps().collect();
        assert_eq!(steps.len(), 38);
        let last = steps[steps.len() - 1] + 0.3 + 0.1;
        assert!((last - CYCLE_SECONDS).abs() < 0.01, "{last}");
    }
}
