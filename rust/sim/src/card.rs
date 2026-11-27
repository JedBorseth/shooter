//! Calling cards: the banner a player shows on the scoreboard and on the
//! card of a killcam, earned by challenges (`crate::challenge`) and
//! prestige (`crate::progression`). Black Ops 2 awards calling cards for
//! the last tier of most career, mode, and scorestreak challenges, and for
//! each prestige; players pick one in their profile. The art is our own,
//! drawn as vector shapes by `godot/scripts/ui/calling_card.gd` from each
//! card's key.

/// A calling card by its wire code. 0 is the card every player starts with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Card(pub u8);

impl Card {
    pub const DEFAULT: Card = Card(0);

    pub fn all() -> impl Iterator<Item = Card> {
        (0..CARDS.len()).map(|i| Card(i as u8))
    }

    pub fn from_code(code: u8) -> Option<Card> {
        (usize::from(code) < CARDS.len()).then_some(Card(code))
    }

    pub fn key(self) -> &'static str {
        CARDS[usize::from(self.0).min(CARDS.len() - 1)].0
    }

    pub fn by_key(key: &str) -> Option<Card> {
        Self::all().find(|c| c.key() == key)
    }

    /// The name players see.
    pub fn name(self) -> &'static str {
        CARDS[usize::from(self.0).min(CARDS.len() - 1)].1
    }

    /// The card for reaching prestige `prestige` (1 to
    /// `progression::MAX_PRESTIGE`), as Black Ops 2 hands out one per
    /// prestige.
    pub fn prestige(prestige: u8) -> Option<Card> {
        Card::by_key(&format!("prestige_{prestige}"))
    }
}

/// Each card's key and name, by code. New cards go at the end: the code is
/// what the wire and saved profiles hold.
const CARDS: &[(&str, &str)] = &[
    ("default", "Recruit"),
    // Career.
    ("veteran", "Veteran"),
    ("sharpshooter", "Sharpshooter"),
    ("victor", "Victor"),
    ("far_sight", "Far Sight"),
    ("payback", "Payback"),
    ("double_tap", "Double Tap"),
    ("chain", "Chain Reaction"),
    ("hot_streak", "Hot Streak"),
    ("unstoppable", "Unstoppable"),
    ("blade", "Cold Steel"),
    ("shrapnel", "Shrapnel"),
    ("two_for_one", "Two for One"),
    ("regular", "Regular"),
    // Modes.
    ("squad", "Squad Up"),
    ("lone_wolf", "Lone Wolf"),
    ("flagbearer", "Flagbearer"),
    ("zone_control", "Zone Control"),
    ("runner", "Runner"),
    ("demolition", "Demolition"),
    ("cool_head", "Cool Head"),
    ("last_standing", "Last Standing"),
    ("ace", "Clean Sweep"),
    ("climber", "Climber"),
    // Scorestreaks.
    ("watchtower", "Watchtower"),
    ("blackout", "Blackout"),
    ("airdrop", "Airdrop"),
    // Mastery.
    ("nebula", "Nebula"),
    // Prestige.
    ("prestige_1", "Prestige I"),
    ("prestige_2", "Prestige II"),
    ("prestige_3", "Prestige III"),
    ("prestige_4", "Prestige IV"),
    ("prestige_5", "Prestige V"),
    ("prestige_6", "Prestige VI"),
    ("prestige_7", "Prestige VII"),
    ("prestige_8", "Prestige VIII"),
    ("prestige_9", "Prestige IX"),
    ("prestige_10", "Prestige X"),
    ("prestige_11", "Master"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_and_round_trip() {
        for card in Card::all() {
            assert_eq!(Card::by_key(card.key()), Some(card), "{}", card.key());
        }
        assert!(Card::all().count() < usize::from(u8::MAX));
        for prestige in 1..=11 {
            assert!(Card::prestige(prestige).is_some());
        }
    }
}
