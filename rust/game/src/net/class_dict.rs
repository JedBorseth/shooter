//! Classes as Godot dictionaries, the shape the menus and settings use:
//!
//! ```text
//! {
//!     "primary": "m4", "primary_attachments": ["reddot", "grip"],
//!     "secondary": "m1911", "secondary_attachments": [],
//!     "lethals": 1, "tactical": "flash", "tacticals": 2,
//!     "wildcards": ["danger_close"],
//! }
//! ```
//!
//! An empty weapon key leaves the slot empty. Keys a class does not know are
//! ignored, so a saved class may carry its name alongside.

use godot::prelude::*;
use shooter_sim::class::{Attachments, Class, ClassError, Gun, Wildcard, Wildcards};
use shooter_sim::equipment::Kind;
use shooter_sim::weapon::{Attachment, WeaponId};

/// Reads a class. `None` when a key names something that does not exist
/// (a weapon, attachment, tactical, or wildcard); its rules are checked
/// separately ([`Class::validate`]).
pub fn class_from_dict(d: &VarDictionary) -> Option<Class> {
    let string = |key: &str| {
        d.get(key)
            .and_then(|v| v.try_to::<GString>().ok())
            .map(|s| s.to_string())
            .unwrap_or_default()
    };
    // A list of keys: an Array from a script or a PackedStringArray.
    let strings = |key: &str| -> Vec<String> {
        let Some(value) = d.get(key) else {
            return Vec::new();
        };
        if let Ok(packed) = value.try_to::<PackedStringArray>() {
            return packed.as_slice().iter().map(|s| s.to_string()).collect();
        }
        value
            .try_to::<VarArray>()
            .map(|a| {
                a.iter_shared()
                    .filter_map(|v| v.try_to::<GString>().ok())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default()
    };
    let count = |key: &str| {
        d.get(key)
            .and_then(|v| v.try_to::<i64>().ok())
            .map(|n| n.clamp(0, 3) as u8)
            .unwrap_or_default()
    };
    let gun = |weapon: &str, attachments: &str| -> Option<Option<Gun>> {
        let key = string(weapon);
        if key.is_empty() {
            return Some(None);
        }
        let base = WeaponId::by_key(&key)?;
        let mut set = Attachments::NONE;
        for a in strings(attachments) {
            set = set.with(Attachment::from_key(&a)?);
        }
        Some(Some(Gun {
            base,
            attachments: set,
        }))
    };
    let mut wildcards = Wildcards::NONE;
    for w in strings("wildcards") {
        wildcards = wildcards.with(Wildcard::by_key(&w)?);
    }
    Some(Class {
        primary: gun("primary", "primary_attachments")?,
        secondary: gun("secondary", "secondary_attachments")?,
        lethals: count("lethals"),
        tactical: Kind::by_key(&string("tactical")).unwrap_or(Kind::Flash),
        tacticals: count("tacticals"),
        wildcards,
    })
}

pub fn class_to_dict(class: &Class) -> VarDictionary {
    let mut d = VarDictionary::new();
    let mut gun = |weapon: &str, attachments: &str, gun: Option<Gun>| {
        let key = gun.map_or(String::new(), |g| g.base.def().key.clone());
        d.set(weapon, &GString::from(key.as_str()));
        let list: PackedStringArray = gun
            .map(|g| {
                g.attachments
                    .iter()
                    .map(|a| GString::from(a.key()))
                    .collect()
            })
            .unwrap_or_default();
        d.set(attachments, &list);
    };
    gun("primary", "primary_attachments", class.primary);
    gun("secondary", "secondary_attachments", class.secondary);
    d.set("lethals", i64::from(class.lethals));
    d.set("tactical", class.tactical.key());
    d.set("tacticals", i64::from(class.tacticals));
    let wildcards: PackedStringArray = class
        .wildcards
        .iter()
        .map(|w| GString::from(w.key()))
        .collect();
    d.set("wildcards", &wildcards);
    d
}

/// Why a class was refused, in words for the menu.
pub fn error_text(error: ClassError) -> &'static str {
    match error {
        ClassError::OverBudget => "Over 10 points",
        ClassError::NoGun => "Carry at least one weapon",
        ClassError::NotAPrimary => "Not a primary weapon",
        ClassError::NotASecondary => "A primary as secondary needs Two Primaries",
        ClassError::TooManyAttachments => "Too many attachments",
        ClassError::AttachmentNotOffered => "That weapon does not take this attachment",
        ClassError::TwoOptics => "One optic at a time",
        ClassError::NoVariant => "That combination of attachments is not available",
        ClassError::TooManyLethals => "A second lethal needs Extra Lethal",
        ClassError::TooManyTacticals => "Two tacticals at most",
        ClassError::NotATactical => "Not a tactical",
    }
}
