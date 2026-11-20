//! What a player shows others: their prestige, calling card, and the camo on
//! each of their weapons (docs/progression.md). The game sends it with its
//! class (`ClientMessage::Cosmetics`), and the server passes it to everyone
//! (`GameEvent::Cosmetics`) for the scoreboard, kill feed, killcam card, and
//! the weapons in their hands.
//!
//! A server that checks accounts takes the prestige from the account and
//! drops a card or camo the player has not unlocked, as Call of Duty's
//! servers show only what the player's stats earned. Other servers pass on
//! what the game sends (a modified game could show a camo it never earned,
//! on its own server only).

use crate::camo::Camo;
use crate::card::Card;
use crate::challenge;
use crate::progression::{Earned, MAX_PRESTIGE};
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

    /// The cosmetics a server lets the player show: with `earned` (the
    /// account's), the account's prestige and only what it has unlocked;
    /// without, what was sent, within range.
    pub fn checked(&self, earned: Option<&Earned>) -> Cosmetics {
        let mut camos: Vec<(WeaponId, Camo)> = Vec::new();
        for (weapon, camo) in &self.camos {
            let weapon = weapon.plain();
            let allowed = *camo != Camo::NONE
                && Camo::from_code(camo.0).is_some()
                && earned.is_none_or(|earned| challenge::camo_unlocked(earned, weapon, *camo));
            if allowed && camos.len() < MAX_CAMOS && !camos.iter().any(|(w, _)| *w == weapon) {
                camos.push((weapon, *camo));
            }
        }
        let card = match earned {
            Some(earned) if !challenge::card_unlocked(earned, self.card) => Card::DEFAULT,
            _ if Card::from_code(self.card.0).is_none() => Card::DEFAULT,
            _ => self.card,
        };
        Cosmetics {
            prestige: earned
                .map_or(self.prestige, |e| e.prestige)
                .min(MAX_PRESTIGE),
            card,
            camos,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_account_shows_only_what_it_earned() {
        let m4 = WeaponId::expect("m4");
        let ak = WeaponId::expect("ak47");
        let sent = Cosmetics {
            prestige: 9,
            card: Card::by_key("veteran").expect("a card"),
            camos: vec![(m4, Camo::rung(0)), (ak, Camo::GOLD)],
        };
        let mut earned = Earned::default();
        earned.stats.insert("w.m4.hs".to_owned(), 5);
        earned.prestige = 2;
        let shown = sent.checked(Some(&earned));
        assert_eq!(shown.prestige, 2);
        assert_eq!(shown.card, Card::DEFAULT);
        assert_eq!(shown.camos, vec![(m4, Camo::rung(0))]);
        // Any variant of the M4A1 wears it.
        let variant = WeaponId::all()
            .find(|w| w.plain() == m4 && *w != m4)
            .expect("a variant");
        assert_eq!(shown.camo_for(variant), Camo::rung(0));
        assert_eq!(shown.camo_for(ak), Camo::NONE);
        // Without an account, what was sent stands.
        let trusted = sent.checked(None);
        assert_eq!(trusted.prestige, 9);
        assert_eq!(trusted.camos.len(), 2);
    }
}
