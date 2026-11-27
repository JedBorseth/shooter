//! Progression rules for GDScript: `Progression.rank(xp)` and friends. The
//! rules are `shooter_sim::progression`; the player's XP and weapon kills
//! are kept by `godot/scripts/progress_store.gd`.

use std::collections::BTreeMap;

use godot::prelude::*;
use shooter_sim::class::{self, Wildcard};
use shooter_sim::equipment::Kind;
use shooter_sim::perk::Perk;
use shooter_sim::progression::{self, Earned, Outcome};
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

    /// Adds a match to `totals` (`xp`, `weapon_kills`): `xp` (score and
    /// bonus) and kills with each weapon. Returns the new totals.
    #[func]
    fn add_match(totals: VarDictionary, xp: i64, weapon_kills: VarDictionary) -> VarDictionary {
        let mut earned = earned(&totals);
        earned.add_match(
            xp.clamp(0, i64::from(u32::MAX)) as u32,
            &counts_from(&weapon_kills),
        );
        totals_dictionary(&earned)
    }

    /// Whether `totals` has an item (`kind`, `key`) unlocked by level.
    #[func]
    fn has_item(totals: VarDictionary, kind: GString, key: GString) -> bool {
        let (kind, key) = (kind.to_string(), key.to_string());
        progression::item_level(&kind, &key).is_none_or(|level| earned(&totals).level() >= level)
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

/// A whole count from a dictionary value (numbers read from files may be
/// floats).
fn whole(value: Option<Variant>) -> u32 {
    value
        .and_then(|v| {
            v.try_to::<i64>()
                .ok()
                .or_else(|| v.try_to::<f64>().ok().map(|f| f as i64))
        })
        .unwrap_or(0)
        .clamp(0, i64::from(u32::MAX)) as u32
}

/// Kills by plain weapon key from a dictionary, keeping only known weapons.
fn counts_from(d: &VarDictionary) -> BTreeMap<String, u32> {
    let mut out = BTreeMap::new();
    for (key, count) in d.iter_shared() {
        let Ok(key) = key.try_to::<GString>() else {
            continue;
        };
        let key = key.to_string();
        if WeaponId::by_key(&key).is_some() {
            out.insert(key, whole(Some(count)));
        }
    }
    out
}

fn counts_at(progress: &VarDictionary, key: &str) -> BTreeMap<String, u32> {
    progress
        .get(key)
        .and_then(|v| v.try_to::<VarDictionary>().ok())
        .map_or_else(BTreeMap::new, |d| counts_from(&d))
}

fn counts_dictionary(counts: &BTreeMap<String, u32>) -> VarDictionary {
    let mut d = VarDictionary::new();
    for (key, count) in counts {
        d.set(key.as_str(), i64::from(*count));
    }
    d
}

/// A player's totals from a progress dictionary (`xp`, `weapon_kills`), as
/// `ProgressStore.totals` gives them.
fn earned(totals: &VarDictionary) -> Earned {
    Earned {
        xp: whole(totals.get("xp")),
        weapon_kills: counts_at(totals, "weapon_kills"),
    }
}

fn totals_dictionary(earned: &Earned) -> VarDictionary {
    let mut d = VarDictionary::new();
    d.set("xp", i64::from(earned.xp));
    d.set("weapon_kills", &counts_dictionary(&earned.weapon_kills));
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
