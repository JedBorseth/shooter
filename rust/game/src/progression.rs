//! Progression rules for GDScript: `Progression.rank(xp)` and friends. The
//! rules are `shooter_sim::progression`; the player's XP and weapon kills
//! are kept by `godot/scripts/progress_store.gd`.

use godot::prelude::*;
use shooter_sim::camo::Camo;
use shooter_sim::card::Card;
use shooter_sim::challenge::{self, Challenge, Group, Reward};
use shooter_sim::class::{self, Wildcard};
use shooter_sim::equipment::Kind;
use shooter_sim::perk::Perk;
use shooter_sim::progression::{self, Earned, Outcome, PrestigeError};
use shooter_sim::tdm::Mode;

use crate::net::accounts;
use shooter_sim::streak::Streak;
use shooter_sim::weapon::{Attachment, Slot, WeaponId};

#[derive(GodotClass)]
#[class(no_init, base = Object)]
pub struct Progression;

#[godot_api]
impl Progression {
    /// Where a total of XP stands: `level`, `into_level` (XP since reaching
    /// it), and `level_span` (XP from it to the next, 0 at the top).
    #[func]
    fn rank(xp: i64) -> VarDictionary {
        let rank = progression::rank(xp.clamp(0, i64::from(u32::MAX)) as u32);
        let mut d = VarDictionary::new();
        d.set("level", i64::from(rank.level));
        d.set("into_level", i64::from(rank.into_level));
        d.set("level_span", i64::from(rank.level_span));
        d
    }

    #[func]
    fn max_level() -> i32 {
        i32::from(progression::MAX_LEVEL)
    }

    #[func]
    fn create_a_class_level() -> i32 {
        i32::from(progression::CREATE_A_CLASS_LEVEL)
    }

    /// The level that unlocks an item: `kind` is "weapon", "tactical",
    /// "perk", or "wildcard", `key` its key. 1 for anything unknown.
    #[func]
    fn unlock_level(kind: GString, key: GString) -> i32 {
        i32::from(unlock_level(&kind.to_string(), &key.to_string()).unwrap_or(1))
    }

    /// Kills with a weapon that unlock an attachment (by key) on it.
    #[func]
    fn attachment_kills(key: GString) -> i32 {
        Attachment::from_key(&key.to_string())
            .map_or(0, |a| progression::attachment_kills(a) as i32)
    }

    /// The end-of-match bonus for `seconds` played; `outcome` is "win",
    /// "draw", or "loss".
    #[func]
    fn match_bonus(seconds: f32, outcome: GString) -> i32 {
        let outcome = match outcome.to_string().as_str() {
            "win" => Outcome::Win,
            "draw" => Outcome::Draw,
            _ => Outcome::Loss,
        };
        progression::match_bonus(seconds, outcome) as i32
    }

    /// Adds a match to `totals` (`xp`, `prestige`, `weapon_kills`,
    /// `stats`): `xp` (score and bonus), weapon kills, and stats, then the
    /// XP of every challenge it completed. Returns `totals` (the new
    /// totals), `challenge_xp`, and `challenges` (their ids).
    #[func]
    fn add_match(
        totals: VarDictionary,
        xp: i64,
        weapon_kills: VarDictionary,
        stats: VarDictionary,
    ) -> VarDictionary {
        let mut earned = earned(&totals);
        let gain = earned.add_match(
            xp.clamp(0, i64::from(u32::MAX)) as u32,
            &accounts::counts_from(&weapon_kills),
            &accounts::counts_from(&stats),
        );
        let mut d = VarDictionary::new();
        d.set("totals", &totals_dictionary(&earned));
        d.set("challenge_xp", i64::from(gain.challenge_xp));
        let ids: PackedStringArray = gain.challenges.iter().map(GString::from).collect();
        d.set("challenges", &ids);
        d
    }

    /// The last prestige (the master's).
    #[func]
    fn max_prestige() -> i32 {
        i32::from(progression::MAX_PRESTIGE)
    }

    /// "" when `totals` may prestige, otherwise why not.
    #[func]
    fn prestige_block(totals: VarDictionary) -> GString {
        match earned(&totals).can_prestige() {
            Ok(()) => GString::new(),
            Err(error) => GString::from(prestige_error(error)),
        }
    }

    /// `totals` after prestiging (`Earned::prestige_up`), or `totals`
    /// unchanged when it may not.
    #[func]
    fn prestige_up(totals: VarDictionary) -> VarDictionary {
        let mut earned = earned(&totals);
        match earned.prestige_up() {
            Ok(()) => totals_dictionary(&earned),
            Err(_) => totals,
        }
    }

    /// Spends a token on an item (`kind`, `key`): `totals` (changed or
    /// not) and `error` ("" when it worked).
    #[func]
    fn unlock_permanent(totals: VarDictionary, kind: GString, key: GString) -> VarDictionary {
        let mut earned = earned(&totals);
        let result = earned.unlock_permanent(&kind.to_string(), &key.to_string());
        let mut d = VarDictionary::new();
        d.set("totals", &totals_dictionary(&earned));
        d.set("error", result.err().map_or("", prestige_error));
        d
    }

    /// Whether `totals` has an item (`kind`, `key`) unlocked: by level, for
    /// good with a token, or at the master's prestige.
    #[func]
    fn has_item(totals: VarDictionary, kind: GString, key: GString) -> bool {
        let (kind, key) = (kind.to_string(), key.to_string());
        progression::item_level(&kind, &key)
            .is_none_or(|level| earned(&totals).has(level, &kind, &key))
    }

    /// The prestige's name: "" before the first, "Prestige 3", or "Master".
    #[func]
    fn prestige_name(prestige: i32) -> GString {
        let name = match prestige {
            p if p <= 0 => String::new(),
            p if p >= i32::from(progression::MAX_PRESTIGE) => "Master".to_owned(),
            p => format!("Prestige {p}"),
        };
        GString::from(&name)
    }

    /// Every challenge, with `progress` and `done` for `totals`: `id`,
    /// `name`, `detail`, `target`, `xp`, `group` (`kind`: "weapon",
    /// "mastery", "career", "mode", or "streak"; `key`; `label`; and for a
    /// weapon its `class` and `class_label`), and `reward` (`kind`: "camo",
    /// "card", or ""; `key`; `name`).
    #[func]
    fn challenges(totals: VarDictionary) -> Array<VarDictionary> {
        let earned = earned(&totals);
        challenge::all()
            .iter()
            .map(|c| challenge_dictionary(c, Some(&earned)))
            .collect()
    }

    /// One challenge by id (no progress), or an empty dictionary.
    #[func]
    fn challenge(id: GString) -> VarDictionary {
        challenge::by_id(&id.to_string())
            .map_or_else(VarDictionary::new, |c| challenge_dictionary(c, None))
    }

    /// Every camo: `key`, `name`, and `code`, in ladder order (the factory
    /// finish first).
    #[func]
    fn camos() -> Array<VarDictionary> {
        Camo::all()
            .map(|camo| {
                let mut d = VarDictionary::new();
                d.set("key", camo.key());
                d.set("name", camo.name());
                d.set("code", i64::from(camo.0));
                d
            })
            .collect()
    }

    /// Whether `totals` has unlocked camo `camo` (key) on `weapon` (key).
    #[func]
    fn camo_unlocked(totals: VarDictionary, weapon: GString, camo: GString) -> bool {
        let (Some(weapon), Some(camo)) = (
            WeaponId::by_key(&weapon.to_string()),
            Camo::by_key(&camo.to_string()),
        ) else {
            return false;
        };
        challenge::camo_unlocked(&earned(&totals), weapon, camo)
    }

    /// Every calling card: `key`, `name`, and `code`.
    #[func]
    fn cards() -> Array<VarDictionary> {
        Card::all()
            .map(|card| {
                let mut d = VarDictionary::new();
                d.set("key", card.key());
                d.set("name", card.name());
                d.set("code", i64::from(card.0));
                d
            })
            .collect()
    }

    /// Whether `totals` has unlocked calling card `card` (key).
    #[func]
    fn card_unlocked(totals: VarDictionary, card: GString) -> bool {
        Card::by_key(&card.to_string())
            .is_some_and(|card| challenge::card_unlocked(&earned(&totals), card))
    }

    /// What reaching `level` unlocks: dictionaries with `kind`, `key`, and
    /// `name`, Create-a-Class itself included (kind "feature").
    #[func]
    fn unlocks_at(level: i32) -> Array<VarDictionary> {
        let entry = |kind: &str, key: &str, name: &str| {
            let mut d = VarDictionary::new();
            d.set("kind", kind);
            d.set("key", key);
            d.set("name", name);
            d
        };
        let mut out = Array::new();
        if level == i32::from(progression::CREATE_A_CLASS_LEVEL) {
            out.push(&entry("feature", "create_a_class", "Create-a-Class"));
        }
        for slot in [Slot::Primary, Slot::Secondary] {
            for id in class::class_weapons(slot) {
                let def = id.def();
                if i32::from(def.unlock_level) == level && level > 1 {
                    out.push(&entry("weapon", &def.key, &def.name));
                }
            }
        }
        for kind in Kind::TACTICALS {
            if i32::from(progression::tactical_level(kind)) == level && level > 1 {
                out.push(&entry("tactical", kind.key(), kind.name()));
            }
        }
        for perk in Perk::ALL {
            if i32::from(progression::perk_level(perk)) == level {
                out.push(&entry("perk", perk.key(), perk.label()));
            }
        }
        for wildcard in Wildcard::ALL {
            if i32::from(progression::wildcard_level(wildcard)) == level {
                out.push(&entry("wildcard", wildcard.key(), wildcard.label()));
            }
        }
        for streak in Streak::ALL {
            if i32::from(streak.unlock_level()) == level && level > 1 {
                out.push(&entry("streak", streak.key(), streak.label()));
            }
        }
        out
    }
}

/// A player's totals from a progress dictionary (`xp`, `prestige`,
/// `weapon_kills`, `stats`), as `ProgressStore.totals` gives them.
fn earned(totals: &VarDictionary) -> Earned {
    accounts::earned_from(totals)
}

fn totals_dictionary(earned: &Earned) -> VarDictionary {
    let mut d = VarDictionary::new();
    d.set("xp", i64::from(earned.xp));
    d.set("prestige", i64::from(earned.prestige));
    d.set(
        "weapon_kills",
        &accounts::counts_dictionary(&earned.weapon_kills),
    );
    d.set("stats", &accounts::counts_dictionary(&earned.stats));
    d.set("tokens", i64::from(earned.tokens));
    let permanent: PackedStringArray = earned.permanent.iter().map(GString::from).collect();
    d.set("permanent", &permanent);
    d
}

fn prestige_error(error: PrestigeError) -> &'static str {
    match error {
        PrestigeError::NotTopLevel => "Prestige opens at the top level.",
        PrestigeError::Master => "Master is the last prestige.",
        PrestigeError::NoToken => "No unlock tokens left.",
        PrestigeError::AlreadyUnlocked => "Already unlocked.",
        PrestigeError::Unknown => "Not something a token unlocks.",
    }
}

fn group_dictionary(group: Group) -> VarDictionary {
    let mut d = VarDictionary::new();
    let (kind, key, label): (&str, String, String) = match group {
        Group::Weapon(id) => {
            let def = id.def();
            ("weapon", def.key.clone(), def.name.clone())
        }
        Group::Mastery => ("mastery", "mastery".to_owned(), "Mastery".to_owned()),
        Group::Career => ("career", "career".to_owned(), "Career".to_owned()),
        Group::Mode(mode) => (
            "mode",
            mode.to_owned(),
            Mode::parse(mode).map_or(mode.to_owned(), |m| m.label().to_owned()),
        ),
        Group::Scorestreak(streak) => {
            ("streak", streak.key().to_owned(), streak.label().to_owned())
        }
    };
    d.set("kind", kind);
    d.set("key", key.as_str());
    d.set("label", label.as_str());
    if let Group::Weapon(id) = group {
        d.set("class", challenge::class_key(id.def().class));
        d.set("class_label", id.def().class.label());
    }
    d
}

fn challenge_dictionary(c: &Challenge, earned: Option<&Earned>) -> VarDictionary {
    let mut d = VarDictionary::new();
    d.set("id", c.id.as_str());
    d.set("name", c.name.as_str());
    d.set("detail", c.detail.as_str());
    d.set("target", i64::from(c.target));
    d.set("xp", i64::from(c.xp));
    d.set("group", &group_dictionary(c.group));
    let (kind, key, name) = match c.reward {
        Reward::None => ("", "", ""),
        Reward::Camo(camo) => ("camo", camo.key(), camo.name()),
        Reward::Card(card) => ("card", card.key(), card.name()),
    };
    let mut reward = VarDictionary::new();
    reward.set("kind", kind);
    reward.set("key", key);
    reward.set("name", name);
    d.set("reward", &reward);
    if let Some(earned) = earned {
        d.set("progress", i64::from(challenge::progress(earned, c)));
        d.set("done", challenge::done(earned, c));
    }
    d
}

fn unlock_level(kind: &str, key: &str) -> Option<u8> {
    match kind {
        "weapon" => WeaponId::by_key(key).map(|id| id.def().unlock_level),
        "tactical" => Kind::by_key(key).map(progression::tactical_level),
        "perk" => Perk::by_key(key).map(progression::perk_level),
        "wildcard" => Wildcard::by_key(key).map(progression::wildcard_level),
        "streak" => Streak::by_key(key).map(Streak::unlock_level),
        _ => None,
    }
}
