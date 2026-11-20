//! Progression: XP, the player's level, and what each level unlocks.
//!
//! Classic Call of Duty rules (Call of Duty 4 through Black Ops 3):
//!
//! - **Score is XP.** Every point a player scores in a match (a kill's 100,
//!   a headshot's bonus, objective awards) is XP. At the end of a match a
//!   bonus pays for time played ([`match_bonus`]); Black Ops 2 based its
//!   match bonus on the time played and the mode, not on kills, and Modern
//!   Warfare (2019) onward pays more for a win.
//! - **Levels 1 to [`MAX_LEVEL`]** (55, as in Black Ops 2 and 3), each needing
//!   a little more XP than the last ([`xp_to_next`]).
//! - **Create-a-Class opens at level [`CREATE_A_CLASS_LEVEL`]** (4, as in
//!   Black Ops 2 and 3). Before that, players pick from the default classes,
//!   which always carry their items whatever the level.
//! - **Weapons, tacticals, perks, and wildcards unlock by level** (each
//!   weapon's `unlock_level` in its file; [`tactical_level`],
//!   [`perk_level`], [`wildcard_level`]).
//! - **Attachments unlock by kills with that weapon**, as Black Ops 2's
//!   weapon levels do ([`attachment_kills`]).
//!
//! Progress is kept by the client for now (`godot/scripts/progress_store.gd`).
//! The server cannot check a level without accounts (Phase 5), so it checks
//! only the class rules (`crate::class`), not unlocks.

use crate::class::Wildcard;
use crate::equipment;
use crate::perk::Perk;
use crate::weapon::Attachment;

/// The highest level (Black Ops 2 and 3: 55).
pub const MAX_LEVEL: u8 = 55;
/// The level that opens Create-a-Class (Black Ops 2 and 3: 4).
pub const CREATE_A_CLASS_LEVEL: u8 = 4;

/// XP to go from `level` to the next: 600 at level 1, 400 more each level.
/// Level 4 takes about one match against bots; level 55 about 600,000 XP.
/// The shape (a little more each level) is Call of Duty's; the numbers fit
/// our shorter matches.
pub fn xp_to_next(level: u8) -> u32 {
    600 + 400 * u32::from(level.max(1) - 1)
}

/// Total XP to reach `level`.
pub fn xp_for_level(level: u8) -> u32 {
    (1..level.clamp(1, MAX_LEVEL)).map(xp_to_next).sum()
}

/// Where a total of XP stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rank {
    pub level: u8,
    /// XP earned since reaching `level`.
    pub into_level: u32,
    /// XP from `level` to the next (0 at the top).
    pub level_span: u32,
}

/// The level a total of XP reaches, and how far into it.
pub fn rank(xp: u32) -> Rank {
    let mut level = 1;
    let mut floor = 0;
    while level < MAX_LEVEL && xp >= floor + xp_to_next(level) {
        floor += xp_to_next(level);
        level += 1;
    }
    let level_span = if level < MAX_LEVEL {
        xp_to_next(level)
    } else {
        0
    };
    Rank {
        level,
        into_level: xp - floor,
        level_span,
    }
}

/// The level that unlocks a tactical for custom classes.
pub fn tactical_level(kind: equipment::Kind) -> u8 {
    match kind {
        equipment::Kind::Frag | equipment::Kind::Flash => 1,
        equipment::Kind::Smoke => 6,
    }
}

/// The level that unlocks a wildcard.
pub fn wildcard_level(wildcard: Wildcard) -> u8 {
    match wildcard {
        Wildcard::DangerClose => 10,
        Wildcard::PrimaryGunfighter => 19,
        Wildcard::Overkill => 28,
        Wildcard::Perk1Greed => 25,
        Wildcard::Perk2Greed => 31,
        Wildcard::Perk3Greed => 34,
    }
}

/// The level that unlocks a perk. Create-a-Class opens with one perk of each
/// tier, as Black Ops 2's does; the rest follow every few levels.
pub fn perk_level(perk: Perk) -> u8 {
    match perk {
        Perk::Lightweight | Perk::Toughness | Perk::Dexterity => CREATE_A_CLASS_LEVEL,
        Perk::FastHands => 5,
        Perk::FlakJacket => 7,
        Perk::TacticalMask => 9,
        Perk::SleightOfHand => 11,
        Perk::DeadSilence => 14,
        Perk::Scavenger => 17,
        Perk::Awareness => 20,
        Perk::Hardline => 8,
        Perk::Ghost => 15,
        Perk::HardWired => 18,
    }
}

/// Kills with a weapon that unlock an attachment on it. The same order on
/// every weapon: sights and handling first, the suppressor last.
pub fn attachment_kills(attachment: Attachment) -> u32 {
    match attachment {
        Attachment::RedDot => 5,
        Attachment::Grip => 15,
        Attachment::ExtendedMag => 30,
        Attachment::Acog => 50,
        Attachment::Suppressor => 80,
    }
}

/// How a match ended for the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Win,
    Draw,
    Loss,
}

/// XP a minute in the match, paid as the match bonus.
pub const BONUS_PER_MINUTE: f32 = 100.0;
/// The bonus for a win, as a share of the time bonus on top.
pub const WIN_BONUS: f32 = 0.5;

/// The end-of-match bonus: [`BONUS_PER_MINUTE`] for each minute played (to
/// the second), and half again for a win. Nothing for leaving early: a
/// player who quits keeps the score they made but no bonus.
pub fn match_bonus(seconds_played: f32, outcome: Outcome) -> u32 {
    let minutes = (seconds_played.max(0.0) / 60.0).min(60.0);
    let base = minutes * BONUS_PER_MINUTE;
    let total = match outcome {
        Outcome::Win => base * (1.0 + WIN_BONUS),
        Outcome::Draw | Outcome::Loss => base,
    };
    total.round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapon::WeaponId;

    #[test]
    fn levels_take_a_little_more_xp_each() {
        assert_eq!(xp_to_next(1), 600);
        assert_eq!(xp_to_next(2), 1000);
        assert!(xp_to_next(40) > xp_to_next(39));
        assert_eq!(xp_for_level(1), 0);
        assert_eq!(xp_for_level(4), 600 + 1000 + 1400);
        assert_eq!(xp_for_level(MAX_LEVEL), 604_800);
    }

    #[test]
    fn rank_finds_the_level_and_the_way_into_it() {
        assert_eq!(
            rank(0),
            Rank {
                level: 1,
                into_level: 0,
                level_span: 600
            }
        );
        assert_eq!(rank(599).level, 1);
        let r = rank(700);
        assert_eq!((r.level, r.into_level, r.level_span), (2, 100, 1000));
        assert_eq!(rank(xp_for_level(4)).level, 4);
        let top = rank(u32::MAX / 2);
        assert_eq!((top.level, top.level_span), (MAX_LEVEL, 0));
        for level in 1..=MAX_LEVEL {
            assert_eq!(rank(xp_for_level(level)).level, level);
        }
    }

    #[test]
    fn a_match_pays_for_time_and_more_for_a_win() {
        assert_eq!(match_bonus(600.0, Outcome::Loss), 1000);
        assert_eq!(match_bonus(600.0, Outcome::Draw), 1000);
        assert_eq!(match_bonus(600.0, Outcome::Win), 1500);
        assert_eq!(match_bonus(90.0, Outcome::Loss), 150);
        assert_eq!(match_bonus(-5.0, Outcome::Win), 0);
    }

    #[test]
    fn the_starting_weapons_are_unlocked_and_the_rest_come_in_order() {
        let level = |key: &str| WeaponId::expect(key).def().unlock_level;
        for key in ["m4", "pdw9", "m1911"] {
            assert_eq!(level(key), 1, "{key}");
        }
        let mut order: Vec<u8> = ["ak47", "m870", "br3", "lsw6", "m40"]
            .iter()
            .map(|k| level(k))
            .collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
        order.push(MAX_LEVEL);
        assert!(order.iter().all(|l| *l <= MAX_LEVEL));
        // Every variant inherits its weapon's level.
        assert_eq!(level("m4+grip"), 1);
        assert_eq!(level("ak47+reddot+grip"), level("ak47"));
        assert!(CREATE_A_CLASS_LEVEL <= level("ak47"));
    }

    #[test]
    fn wildcards_and_attachments_unlock_later() {
        for w in Wildcard::ALL {
            assert!(wildcard_level(w) > CREATE_A_CLASS_LEVEL);
        }
        assert_eq!(tactical_level(equipment::Kind::Flash), 1);
        let kills: Vec<u32> = Attachment::ALL
            .iter()
            .map(|a| attachment_kills(*a))
            .collect();
        assert!(kills.iter().all(|k| *k > 0));
    }

    #[test]
    fn create_a_class_opens_with_a_perk_of_each_tier() {
        use crate::perk::Tier;
        for tier in Tier::ALL {
            let first = Perk::of_tier(tier).map(perk_level).min().unwrap();
            assert_eq!(first, CREATE_A_CLASS_LEVEL, "{tier:?}");
        }
        for perk in Perk::ALL {
            assert!((CREATE_A_CLASS_LEVEL..=MAX_LEVEL).contains(&perk_level(perk)));
        }
    }
}
