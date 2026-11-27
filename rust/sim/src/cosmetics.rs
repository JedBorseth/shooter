//! What a player shows others: their prestige, calling card, and the camo on
//! each of their weapons (docs/progression.md). The game sends it with its
//! class (`ClientMessage::Cosmetics`), and the server passes it to everyone
//! (`GameEvent::Cosmetics`) for the scoreboard, kill feed, killcam card, and
//! the weapons in their hands.
//!
//! Servers pass on what the game sends (a modified game could show a camo it
//! never earned).

use crate::camo::Camo;
use crate::card::Card;
use crate::progression::MAX_PRESTIGE;
use crate::weapon::WeaponId;

/// Most weapons one player's camos cover: a class's two, with room for a
/// spare.
pub const MAX_CAMOS: usize = 4;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cosmetics {
    /// 0 before the first prestige, [`MAX_PRESTIGE`] for the master.
    pub prestige: u8,
    pub card: Card,
    /// A camo for each plain weapon that wears one.
    pub camos: Vec<(WeaponId, Camo)>,
}

impl Cosmetics {
    /// The camo `weapon` (any variant) wears.
    pub fn camo_for(&self, weapon: WeaponId) -> Camo {
        let plain = weapon.plain();
        self.camos
            .iter()
            .find(|(w, _)| *w == plain)
            .map_or(Camo::NONE, |(_, camo)| *camo)
    }

    /// The cosmetics a server lets the player show: what was sent, within
    /// range.
    pub fn checked(&self) -> Cosmetics {
        let mut camos: Vec<(WeaponId, Camo)> = Vec::new();
        for (weapon, camo) in &self.camos {
            let weapon = weapon.plain();
            let allowed = *camo != Camo::NONE && Camo::from_code(camo.0).is_some();
            if allowed && camos.len() < MAX_CAMOS && !camos.iter().any(|(w, _)| *w == weapon) {
                camos.push((weapon, *camo));
            }
        }
        let card = if Card::from_code(self.card.0).is_some() {
            self.card
        } else {
            Card::DEFAULT
        };
        Cosmetics {
            prestige: self.prestige.min(MAX_PRESTIGE),
            card,
            camos,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_was_sent_stands_within_range() {
        let m4 = WeaponId::expect("m4");
        let variant = WeaponId::all()
            .find(|w| w.plain() == m4 && *w != m4)
            .expect("a variant");
        let sent = Cosmetics {
            prestige: MAX_PRESTIGE + 5,
            card: Card::by_key("veteran").expect("a card"),
            camos: vec![(variant, Camo::GOLD), (m4, Camo::rung(0))],
        };
        let shown = sent.checked();
        assert_eq!(shown.prestige, MAX_PRESTIGE);
        assert_eq!(shown.card, sent.card);
        // Kept once, for the plain weapon, which any variant wears.
        assert_eq!(shown.camos, vec![(m4, Camo::GOLD)]);
        assert_eq!(shown.camo_for(variant), Camo::GOLD);
    }
}
