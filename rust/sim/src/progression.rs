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
//! Progress is kept by the game itself (`godot/scripts/progress_store.gd`).
//! Dedicated servers check only the class rules (`crate::class`).

use std::collections::{BTreeMap, BTreeSet};

use crate::challenge;
use crate::class::Wildcard;
use crate::equipment;
use crate::perk::Perk;
use crate::stats::Stats;
use crate::streak::Streak;
use crate::weapon::{Attachment, WeaponId};

/// The highest level (Black Ops 2 and 3: 55).
pub const MAX_LEVEL: u8 = 55;
/// Prestiges: ten, then the master's (Black Ops 2's Prestige Master), the
/// last.
pub const MAX_PRESTIGE: u8 = 11;
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
        // Not a class item: a launcher fires it.
        equipment::Kind::Rocket => u8::MAX,
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
        Perk::BlindEye => 12,
        Perk::ColdBlooded => 23,
        Perk::Engineer => 26,
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

/// What a player has earned: XP, kills with each weapon by the weapon's
/// plain key ("m4"), and their career stats (`crate::stats`). Levels and
/// attachments follow from the first two; challenges, camos, and calling
/// cards from the stats (`crate::challenge`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Earned {
    pub xp: u32,
    pub weapon_kills: BTreeMap<String, u32>,
    pub stats: Stats,
    /// Times the player has prestiged (0 before the first).
    pub prestige: u8,
    /// Permanent unlock tokens not yet spent: one for each prestige.
    pub tokens: u8,
    /// Items unlocked for good with a token, whatever the level: "kind:key"
    /// ("weapon:m40", "perk:ghost"); kinds as [`unlock_key`] gives them.
    pub permanent: BTreeSet<String>,
}

/// Why the player cannot prestige or spend a token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrestigeError {
    /// Prestige needs the top level.
    NotTopLevel,
    /// The master's prestige is the last.
    Master,
    NoToken,
    /// The item is already unlocked, for good or by level.
    AlreadyUnlocked,
    /// Not an item a token unlocks.
    Unknown,
}

/// The key a permanent unlock is kept under: "weapon:m40".
pub fn unlock_key(kind: &str, key: &str) -> String {
    format!("{kind}:{key}")
}

/// The level that unlocks an item, by kind ("weapon", "tactical", "perk",
/// "wildcard", "streak") and key; none for anything a token cannot unlock.
pub fn item_level(kind: &str, key: &str) -> Option<u8> {
    match kind {
        "weapon" => WeaponId::by_key(key)
            .map(|id| id.def())
            .filter(|def| def.plain() && def.unlock_level > 0)
            .map(|def| def.unlock_level),
        "tactical" => equipment::Kind::by_key(key)
            .filter(|kind| !kind.lethal())
            .map(tactical_level),
        "perk" => Perk::by_key(key).map(perk_level),
        "wildcard" => Wildcard::by_key(key).map(wildcard_level),
        "streak" => Streak::by_key(key).map(Streak::unlock_level),
        _ => None,
    }
}

/// What one match added to a player's [`Earned`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MatchGain {
    /// The match's XP before challenges: score and the match bonus.
    pub xp: u32,
    /// The XP of the challenges the match completed.
    pub challenge_xp: u32,
    /// The ids of those challenges, in list order.
    pub challenges: Vec<String>,
}

impl Earned {
    pub fn level(&self) -> u8 {
        rank(self.xp).level
    }

    /// At the master's prestige every item is unlocked, as Black Ops 2's
    /// Prestige Master unlocks them all.
    pub fn master(&self) -> bool {
        self.prestige >= MAX_PRESTIGE
    }

    /// Whether an item needing `level` (of `kind` and `key`) is unlocked:
    /// by level, by a token, or at the master's prestige.
    pub fn has(&self, level: u8, kind: &str, key: &str) -> bool {
        self.level() >= level || self.master() || self.permanent.contains(&unlock_key(kind, key))
    }

    /// Whether the player may prestige: at the top level, before the
    /// master's.
    pub fn can_prestige(&self) -> Result<(), PrestigeError> {
        if self.master() {
            return Err(PrestigeError::Master);
        }
        if self.level() < MAX_LEVEL {
            return Err(PrestigeError::NotTopLevel);
        }
        Ok(())
    }

    /// Prestiges, as Black Ops 2 does: back to level 1, so level unlocks
    /// lock again (Create-a-Class included), for the next prestige's emblem
    /// and calling card and one permanent unlock token (none at the
    /// master's, which unlocks everything). Kills with each weapon (its
    /// attachments), stats, challenges, camos, cards, and items unlocked
    /// with tokens are kept.
    pub fn prestige_up(&mut self) -> Result<(), PrestigeError> {
        self.can_prestige()?;
        self.xp = 0;
        self.prestige += 1;
        if !self.master() {
            self.tokens = self.tokens.saturating_add(1);
        }
        Ok(())
    }

    /// Spends a token to unlock an item for good.
    pub fn unlock_permanent(&mut self, kind: &str, key: &str) -> Result<(), PrestigeError> {
        let level = item_level(kind, key).ok_or(PrestigeError::Unknown)?;
        if self.has(level, kind, key) {
            return Err(PrestigeError::AlreadyUnlocked);
        }
        if self.tokens == 0 {
            return Err(PrestigeError::NoToken);
        }
        self.tokens -= 1;
        self.permanent.insert(unlock_key(kind, key));
        Ok(())
    }

    /// Adds a match: its XP (score and bonus), kills with each weapon, and
    /// stats, then the XP of every challenge the match completed, as Call
    /// of Duty pays a challenge in the match that finishes it.
    pub fn add_match(
        &mut self,
        xp: u32,
        weapon_kills: &BTreeMap<String, u32>,
        stats: &Stats,
    ) -> MatchGain {
        let before = self.clone();
        self.xp = self.xp.saturating_add(xp);
        for (weapon, kills) in weapon_kills {
            let entry = self.weapon_kills.entry(weapon.clone()).or_default();
            *entry = entry.saturating_add(*kills);
        }
        crate::stats::merge(&mut self.stats, stats);
        let completed = challenge::completed_between(&before, self);
        let challenge_xp = completed.iter().map(|c| c.xp).fold(0, u32::saturating_add);
        self.xp = self.xp.saturating_add(challenge_xp);
        MatchGain {
            xp,
            challenge_xp,
            challenges: completed.iter().map(|c| c.id.clone()).collect(),
        }
    }

    /// Kills with a weapon (its plain key).
    pub fn kills(&self, weapon: &str) -> u32 {
        self.weapon_kills.get(weapon).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prestige_resets_the_level_and_pays_a_token() {
        let mut earned = Earned {
            xp: xp_for_level(MAX_LEVEL),
            ..Earned::default()
        };
        earned.weapon_kills.insert("m4".to_owned(), 40);
        earned.stats.insert("w.m4.hs".to_owned(), 7);
        assert_eq!(
            Earned::default().can_prestige(),
            Err(PrestigeError::NotTopLevel)
        );
        earned.prestige_up().expect("at the top level");
        assert_eq!((earned.xp, earned.prestige, earned.tokens), (0, 1, 1));
        // Kept: weapon kills and stats.
        assert_eq!(earned.kills("m4"), 40);
        assert_eq!(earned.stats.get("w.m4.hs"), Some(&7));
        // The M40 is locked again, until a token unlocks it for good.
        let m40 = WeaponId::expect("m40").def();
        assert!(!earned.has(m40.unlock_level, "weapon", "m40"));
        earned.unlock_permanent("weapon", "m40").expect("a token");
        assert!(earned.has(m40.unlock_level, "weapon", "m40"));
        assert_eq!(earned.tokens, 0);
        assert_eq!(
            earned.unlock_permanent("perk", "ghost"),
            Err(PrestigeError::NoToken)
        );
        assert_eq!(
            earned.unlock_permanent("weapon", "m4"),
            Err(PrestigeError::AlreadyUnlocked)
        );
        assert_eq!(
            earned.unlock_permanent("weapon", "nope"),
            Err(PrestigeError::Unknown)
        );
    }

    #[test]
    fn the_masters_prestige_is_the_last_and_unlocks_everything() {
        let mut earned = Earned {
            prestige: MAX_PRESTIGE - 1,
            xp: xp_for_level(MAX_LEVEL),
            tokens: 3,
            ..Earned::default()
        };
        earned.prestige_up().expect("the last prestige");
        assert!(earned.master());
        // No token for the master's prestige.
        assert_eq!(earned.tokens, 3);
        assert!(earned.has(MAX_LEVEL, "weapon", "dm7"));
        earned.xp = xp_for_level(MAX_LEVEL);
        assert_eq!(earned.prestige_up(), Err(PrestigeError::Master));
    }

    #[test]
    fn a_match_adds_its_challenges_xp_once() {
        let mut earned = Earned::default();
        let kills = BTreeMap::from([("m4".to_owned(), 6)]);
        let stats = BTreeMap::from([("w.m4.hs".to_owned(), 5), ("kills".to_owned(), 6)]);
        let gain = earned.add_match(1000, &kills, &stats);
        assert_eq!(gain.challenges, ["w.m4.0"]);
        assert_eq!(gain.challenge_xp, 250);
        assert_eq!(earned.xp, 1250);
        assert_eq!(earned.kills("m4"), 6);
        // The next headshot completes nothing new.
        let gain = earned.add_match(
            0,
            &BTreeMap::new(),
            &BTreeMap::from([("w.m4.hs".to_owned(), 1)]),
        );
        assert!(gain.challenges.is_empty());
        assert_eq!(earned.xp, 1250);
    }

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
        let mut order: Vec<u8> = [
            "ak47", "m870", "br3", "bp9", "lsw6", "rl4", "m40", "cq10", "kr12", "sg12", "dm7",
        ]
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
