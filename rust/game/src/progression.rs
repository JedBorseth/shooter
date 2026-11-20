//! Progression rules for GDScript: `Progression.rank(xp)` and friends. The
//! rules are `shooter_sim::progression`; the player's XP and weapon kills
//! are kept by `godot/scripts/progress_store.gd`.

use godot::prelude::*;
use shooter_sim::class::{self, Wildcard};
use shooter_sim::equipment::Kind;
use shooter_sim::progression::{self, Outcome};
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

    /// The level that unlocks an item: `kind` is "weapon", "tactical", or
    /// "wildcard", `key` its key. 1 for anything unknown.
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
        for wildcard in Wildcard::ALL {
            if i32::from(progression::wildcard_level(wildcard)) == level {
                out.push(&entry("wildcard", wildcard.key(), wildcard.label()));
            }
        }
        out
    }
}

fn unlock_level(kind: &str, key: &str) -> Option<u8> {
    match kind {
        "weapon" => WeaponId::by_key(key).map(|id| id.def().unlock_level),
        "tactical" => Kind::by_key(key).map(progression::tactical_level),
        "wildcard" => Wildcard::by_key(key).map(progression::wildcard_level),
        _ => None,
    }
}
