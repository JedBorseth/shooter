//! Weapon camos: finishes a player earns with each weapon's challenges
//! (`crate::challenge`) and picks per weapon in Create-a-Class.
//!
//! Black Ops 2's ladder: fourteen camos per weapon, each from one weapon
//! challenge, then a gold finish for completing all fourteen, then a
//! mastery finish (Black Ops 2's Diamond) for gold on every weapon of a
//! class. Black Ops 3 added a final finish (Dark Matter) for mastery in
//! every class; it ends the ladder here too. The names and the designs are
//! our own (docs/progression.md).

/// A camo by its wire code. 0 is the weapon's own finish.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Camo(pub u8);

/// Camos earned by a weapon's ladder challenges, in ladder order.
pub const LADDER: usize = 14;

impl Camo {
    pub const NONE: Camo = Camo(0);
    /// Every ladder challenge of one weapon done.
    pub const GOLD: Camo = Camo(LADDER as u8 + 1);
    /// Gold on every weapon of a class.
    pub const MASTERY: Camo = Camo(LADDER as u8 + 2);
    /// Mastery in every class.
    pub const FINAL: Camo = Camo(LADDER as u8 + 3);
    /// The highest code.
    pub const LAST: Camo = Self::FINAL;

    /// The camo the `rung`th ladder challenge earns (0-based).
    pub fn rung(rung: usize) -> Camo {
        Camo(rung.min(LADDER - 1) as u8 + 1)
    }

    pub fn all() -> impl Iterator<Item = Camo> {
        (0..=Self::LAST.0).map(Camo)
    }

    pub fn from_code(code: u8) -> Option<Camo> {
        (code <= Self::LAST.0).then_some(Camo(code))
    }

    /// The key files and settings use (its texture is
    /// `godot/assets/camos/<key>.png`).
    pub fn key(self) -> &'static str {
        INFO[usize::from(self.0.min(Self::LAST.0))].0
    }

    pub fn by_key(key: &str) -> Option<Camo> {
        Self::all().find(|c| c.key() == key)
    }

    /// The name players see.
    pub fn name(self) -> &'static str {
        INFO[usize::from(self.0.min(Self::LAST.0))].1
    }
}

/// Each camo's key and name, by code.
const INFO: [(&str, &str); Camo::LAST.0 as usize + 1] = [
    ("none", "Factory"),
    ("gravel", "Gravel"),
    ("dune", "Dune"),
    ("timber", "Timber"),
    ("tundra", "Tundra"),
    ("cocoa", "Cocoa"),
    ("riptide", "Riptide"),
    ("ember", "Ember"),
    ("static", "Static"),
    ("scales", "Scales"),
    ("weave", "Weave"),
    ("bloom", "Bloom"),
    ("ink", "Ink"),
    ("hazard", "Hazard"),
    ("circuit", "Circuit"),
    ("gold", "Gold"),
    ("prism", "Prism"),
    ("nebula", "Nebula"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_keys_and_names_round_trip() {
        assert_eq!(Camo::all().count(), LADDER + 4);
        for camo in Camo::all() {
            assert_eq!(Camo::by_key(camo.key()), Some(camo));
            assert_eq!(Camo::from_code(camo.0), Some(camo));
            assert!(!camo.name().is_empty());
        }
        assert_eq!(Camo::rung(0), Camo(1));
        assert_eq!(Camo::rung(LADDER - 1).0 + 1, Camo::GOLD.0);
        assert_eq!(Camo::from_code(Camo::LAST.0 + 1), None);
    }
}
