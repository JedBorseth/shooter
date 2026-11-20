//! Drinks: the Zombies machines that sell a lasting edge for points.
//!
//! Black Ops' perk machines, under our own names and looks, with Black Ops
//! III's numbers (`_zm_perks.gsc`, the `_zm_perk_*.gsc` scripts, and
//! `_zm_laststand.gsc` in the decompiled scripts):
//!
//! | Drink | Effect | Price | Black Ops III specialty |
//! | --- | --- | --- | --- |
//! | [`Drink::Bulwark`] | Down on the fifth hit instead of the second | 2500 | `specialty_armorvest` |
//! | [`Drink::SecondWind`] | Revive teammates in half the time; alone, get back up by yourself (three times a game) | 1500, alone 500 | `specialty_quickrevive` |
//! | [`Drink::Quickload`] | Reloads take half the time | 3000 | `specialty_fastreload` |
//! | [`Drink::HairTrigger`] | Fire a third faster, and every round hits twice | 2000 | `specialty_doubletap2` |
//! | [`Drink::LongStride`] | Move 7% faster | 2000 | `specialty_staminup` |
//!
//! A player holds at most [`LIMIT`] drinks (`level.perk_purchase_limit`) and
//! loses every one on going down. The machines need the power on, except
//! Second Wind in a game for one, which also works without it.
//!
//! Drinks that change handling (Quickload, Hair Trigger, Long Stride) live in
//! [`crate::player::PlayerState::drinks`], so client prediction follows them.

/// One drink.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Drink {
    Bulwark,
    SecondWind,
    Quickload,
    HairTrigger,
    LongStride,
}

/// Most drinks one player holds at once (`level.perk_purchase_limit`, 4).
pub const LIMIT: u8 = 4;

/// Hits that down a player without Bulwark: the game's two-hit rule.
pub const HITS_TO_DOWN: u32 = 2;
/// Hits that down a player with Bulwark: Black Ops III's Jugg player
/// survives four zombie hits and goes down on the fifth (health 100 + 100).
pub const BULWARK_HITS_TO_DOWN: u32 = 5;

/// Reload speed with Quickload: `perk_weapReloadMultiplier` 0.5, so twice
/// as fast.
pub const QUICKLOAD_RELOAD_RATE: f32 = 2.0;
/// Time between shots with Hair Trigger (`perk_weapRateMultiplier`, 0.75):
/// a third more rounds a minute.
pub const HAIR_TRIGGER_FIRE_INTERVAL: f32 = 0.75;
/// Damage each round does to zombies with Hair Trigger: Black Ops III's
/// version fires every bullet twice.
pub const HAIR_TRIGGER_DAMAGE: f32 = 2.0;
/// Movement speed with Long Stride (Black Ops' 7% faster move speed).
pub const LONG_STRIDE_SPEED: f32 = 1.07;
/// Share of the revive time with Second Wind (`revive_get_revive_time`
/// halves the 3 s).
pub const SECOND_WIND_REVIVE_SCALE: f32 = 0.5;
/// Seconds a lone player with Second Wind lies down before getting back up
/// (`wait_and_revive`'s `solo_revive_time`).
pub const SELF_REVIVE_SECONDS: f32 = 10.0;
/// Times a lone player can buy Second Wind in one game before its machine
/// leaves (`level.solo_lives_given >= 3`).
pub const SOLO_SECOND_WINDS: u8 = 3;
/// Second Wind's price in a game for one (`revive_cost_override`).
pub const SOLO_SECOND_WIND_PRICE: u32 = 500;

impl Drink {
    pub const ALL: [Drink; 5] = [
        Drink::Bulwark,
        Drink::SecondWind,
        Drink::Quickload,
        Drink::HairTrigger,
        Drink::LongStride,
    ];

    /// Its bit in [`Drinks`] and its number on the wire.
    pub fn code(self) -> u8 {
        Drink::ALL.iter().position(|d| *d == self).unwrap_or(0) as u8
    }

    pub fn from_code(code: u8) -> Option<Drink> {
        Drink::ALL.get(usize::from(code)).copied()
    }

    /// Lower-case key for maps and scripts (a machine's `drink`).
    pub fn key(self) -> &'static str {
        match self {
            Drink::Bulwark => "bulwark",
            Drink::SecondWind => "second_wind",
            Drink::Quickload => "quickload",
            Drink::HairTrigger => "hair_trigger",
            Drink::LongStride => "long_stride",
        }
    }

    pub fn from_key(key: &str) -> Option<Drink> {
        Drink::ALL.into_iter().find(|d| d.key() == key)
    }

    /// The name players see.
    pub fn label(self) -> &'static str {
        match self {
            Drink::Bulwark => "Bulwark",
            Drink::SecondWind => "Second Wind",
            Drink::Quickload => "Quickload",
            Drink::HairTrigger => "Hair Trigger",
            Drink::LongStride => "Long Stride",
        }
    }

    /// Its price in points; `solo` for a game with one player.
    pub fn price(self, solo: bool) -> u32 {
        match self {
            Drink::Bulwark => 2500,
            Drink::SecondWind if solo => SOLO_SECOND_WIND_PRICE,
            Drink::SecondWind => 1500,
            Drink::Quickload => 3000,
            Drink::HairTrigger | Drink::LongStride => 2000,
        }
    }

    /// Whether its machine works with the power off: only Second Wind, and
    /// only in a game for one (Black Ops' solo revive machine).
    pub fn works_without_power(self, solo: bool) -> bool {
        solo && self == Drink::SecondWind
    }
}

/// The drinks one player holds, one bit per [`Drink::ALL`] entry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Drinks(u8);

impl Drinks {
    pub const NONE: Drinks = Drinks(0);
    /// Bits the set uses on the wire.
    pub const BITS: u32 = Drink::ALL.len() as u32;

    pub fn from_bits(bits: u8) -> Option<Self> {
        (u32::from(bits) < 1 << Self::BITS).then_some(Self(bits))
    }

    pub fn bits(self) -> u8 {
        self.0
    }

    pub fn has(self, drink: Drink) -> bool {
        self.0 & 1 << drink.code() != 0
    }

    pub fn with(self, drink: Drink) -> Self {
        Self(self.0 | 1 << drink.code())
    }

    pub fn without(self, drink: Drink) -> Self {
        Self(self.0 & !(1 << drink.code()))
    }

    pub fn len(self) -> u8 {
        self.0.count_ones() as u8
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The drinks held, in [`Drink::ALL`] order.
    pub fn iter(self) -> impl Iterator<Item = Drink> {
        Drink::ALL.into_iter().filter(move |d| self.has(*d))
    }

    /// Hits that down a player holding these.
    pub fn hits_to_down(self) -> u32 {
        if self.has(Drink::Bulwark) {
            BULWARK_HITS_TO_DOWN
        } else {
            HITS_TO_DOWN
        }
    }

    /// Multiplier on the speed reloads advance at.
    pub fn reload_rate(self) -> f32 {
        if self.has(Drink::Quickload) {
            QUICKLOAD_RELOAD_RATE
        } else {
            1.0
        }
    }

    /// Multiplier on the time between shots.
    pub fn fire_interval_scale(self) -> f32 {
        if self.has(Drink::HairTrigger) {
            HAIR_TRIGGER_FIRE_INTERVAL
        } else {
            1.0
        }
    }

    /// Multiplier on the damage each round does to zombies.
    pub fn zombie_damage_scale(self) -> f32 {
        if self.has(Drink::HairTrigger) {
            HAIR_TRIGGER_DAMAGE
        } else {
            1.0
        }
    }

    /// Multiplier on movement speed.
    pub fn speed_scale(self) -> f32 {
        if self.has(Drink::LongStride) {
            LONG_STRIDE_SPEED
        } else {
            1.0
        }
    }

    /// Seconds this player takes to revive a teammate.
    pub fn revive_seconds(self) -> f32 {
        let base = super::laststand::REVIVE_SECONDS;
        if self.has(Drink::SecondWind) {
            base * SECOND_WIND_REVIVE_SCALE
        } else {
            base
        }
    }
}

/// Why a player cannot buy a drink now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The machine needs the power on.
    NoPower,
    /// They already hold it.
    Owned,
    /// They hold [`LIMIT`] drinks.
    Full,
    /// Not enough points.
    Points,
    /// Second Wind's machine has left (a lone player used it up).
    Gone,
}

/// Whether a player holding `held` with `points` may buy `drink`, with the
/// power `powered`, in a game for one when `solo`, from a machine that has
/// not left (`available`). Ok holds the price.
pub fn can_buy(
    drink: Drink,
    held: Drinks,
    points: u32,
    powered: bool,
    solo: bool,
    available: bool,
) -> Result<u32, Refusal> {
    if !available {
        return Err(Refusal::Gone);
    }
    if !powered && !drink.works_without_power(solo) {
        return Err(Refusal::NoPower);
    }
    if held.has(drink) {
        return Err(Refusal::Owned);
    }
    if held.len() >= LIMIT {
        return Err(Refusal::Full);
    }
    let price = drink.price(solo);
    if points < price {
        return Err(Refusal::Points);
    }
    Ok(price)
}

/// Damage a zombie's swing deals to a player with `health` of `max_health`
/// who holds `drinks`.
///
/// Without Bulwark this is the two-hit rule ([`super::hit_damage`]): a hit at
/// full health takes half, and any hit before health has fully regenerated
/// downs. With Bulwark each hit takes a fifth of full health, so the fifth
/// hit in a row downs; health regenerates between hits as usual.
pub fn hit_damage(health: f32, max_health: f32, drinks: Drinks) -> f32 {
    match drinks.hits_to_down() {
        HITS_TO_DOWN => super::hit_damage(health, max_health),
        hits => (max_health / hits as f32).min(health.max(0.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices_follow_black_ops_iii() {
        assert_eq!(Drink::Bulwark.price(false), 2500);
        assert_eq!(Drink::SecondWind.price(false), 1500);
        assert_eq!(Drink::SecondWind.price(true), 500);
        assert_eq!(Drink::Quickload.price(false), 3000);
        assert_eq!(Drink::HairTrigger.price(false), 2000);
        assert_eq!(Drink::LongStride.price(true), 2000);
    }

    #[test]
    fn keys_and_codes_round_trip() {
        for drink in Drink::ALL {
            assert_eq!(Drink::from_key(drink.key()), Some(drink));
            assert_eq!(Drink::from_code(drink.code()), Some(drink));
        }
        assert_eq!(Drinks::from_bits(0b1_1111), Some(Drinks(0b1_1111)));
        assert_eq!(Drinks::from_bits(0b10_0000), None);
    }

    #[test]
    fn four_drinks_at_most_and_each_once() {
        let mut held = Drinks::NONE;
        for drink in [
            Drink::Bulwark,
            Drink::Quickload,
            Drink::HairTrigger,
            Drink::LongStride,
        ] {
            assert!(can_buy(drink, held, 10_000, true, false, true).is_ok());
            held = held.with(drink);
        }
        assert_eq!(held.len(), 4);
        assert_eq!(
            can_buy(Drink::SecondWind, held, 10_000, true, false, true),
            Err(Refusal::Full)
        );
        assert_eq!(
            can_buy(
                Drink::Bulwark,
                Drinks::NONE.with(Drink::Bulwark),
                10_000,
                true,
                false,
                true
            ),
            Err(Refusal::Owned)
        );
    }

    #[test]
    fn machines_need_power_except_second_wind_alone() {
        assert_eq!(
            can_buy(Drink::Bulwark, Drinks::NONE, 5000, false, true, true),
            Err(Refusal::NoPower)
        );
        assert_eq!(
            can_buy(Drink::SecondWind, Drinks::NONE, 5000, false, false, true),
            Err(Refusal::NoPower)
        );
        assert_eq!(
            can_buy(Drink::SecondWind, Drinks::NONE, 5000, false, true, true),
            Ok(500)
        );
        assert_eq!(
            can_buy(Drink::SecondWind, Drinks::NONE, 5000, true, true, false),
            Err(Refusal::Gone)
        );
        assert_eq!(
            can_buy(Drink::Quickload, Drinks::NONE, 2999, true, false, true),
            Err(Refusal::Points)
        );
    }

    #[test]
    fn bulwark_downs_on_the_fifth_hit() {
        let max = 100.0;
        let bulwark = Drinks::NONE.with(Drink::Bulwark);
        let mut health = max;
        let mut hits = 0;
        while health > 0.0 {
            health -= hit_damage(health, max, bulwark);
            hits += 1;
        }
        assert_eq!(hits, 5);
        // Without it, the two-hit rule.
        let mut health = max;
        let mut hits = 0;
        while health > 0.0 {
            health -= hit_damage(health, max, Drinks::NONE);
            hits += 1;
        }
        assert_eq!(hits, 2);
    }

    #[test]
    fn effects_scale_from_one() {
        assert_eq!(Drinks::NONE.reload_rate(), 1.0);
        assert_eq!(Drinks::NONE.fire_interval_scale(), 1.0);
        assert_eq!(Drinks::NONE.speed_scale(), 1.0);
        assert_eq!(Drinks::NONE.revive_seconds(), 3.0);
        let all = Drink::ALL.into_iter().fold(Drinks::NONE, Drinks::with);
        assert_eq!(all.reload_rate(), 2.0);
        assert_eq!(all.fire_interval_scale(), 0.75);
        assert_eq!(all.zombie_damage_scale(), 2.0);
        assert!((all.speed_scale() - 1.07).abs() < 1e-6);
        assert_eq!(all.revive_seconds(), 1.5);
    }
}
