//! Perks: passive abilities a class carries, one from each of three tiers.
//!
//! Black Ops 2's perk system (kept in Black Ops 3): Create-a-Class has a
//! Perk 1, Perk 2, and Perk 3 slot, each taking one perk of its tier for a
//! point ([`crate::class`]). A Perk Greed wildcard opens a second perk of
//! one tier ((`crate::class::Wildcard::Perk1Greed` and its siblings)).
//!
//! The perks this game has, and what each changes (Black Ops 2's own
//! numbers where its game files give them; see `docs/create-a-class.md`):
//!
//! | Tier | Perk | Effect |
//! | --- | --- | --- |
//! | 1 | Lightweight | moves 7% faster, no fall damage |
//! | 1 | Flak Jacket | takes 35% of explosive damage |
//! | 2 | Toughness | aim flinches a quarter as much when shot |
//! | 2 | Fast Hands | switches weapons in half the time, throws faster |
//! | 2 | Scavenger | refills ammo and grenades from packs enemies drop |
//! | 2 | Sleight of Hand | reloads in half the time (Black Ops 1's Perk 2) |
//! | 3 | Dexterity | mantles 40% faster, climbs ladders 75% faster, weapon up sooner after a sprint |
//! | 3 | Dead Silence | near-silent footsteps |
//! | 3 | Awareness | hears enemy footsteps louder |
//! | 3 | Tactical Mask | flashbangs blind for half as long |
//!
//! Movement and handling perks change what the client predicts, so they
//! live in the player's simulated state ([`crate::player::PlayerState::perks`]),
//! which the server sends with every own-state snapshot: prediction and the
//! server always step with the same perks. The rest act where their target
//! is decided: explosive damage on the server, flash and flinch on the
//! client that is flashed or flinches, footsteps where they are heard.
//!
//! Black Ops 2 perks this game leaves out, because their target does not
//! exist yet: Hardline (scorestreaks), Blind Eye and Ghost (UAV and air
//! support), Cold-Blooded (AI streaks and thermal), Hard Wired (counter-UAV
//! and EMP), Engineer (enemy equipment and care packages), and Extreme
//! Conditioning (sprint is unlimited here, a recorded project decision).

/// A perk's tier: the Create-a-Class slot it goes in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tier {
    One,
    Two,
    Three,
}

impl Tier {
    pub const ALL: [Tier; 3] = [Tier::One, Tier::Two, Tier::Three];

    /// 1, 2, or 3.
    pub fn number(self) -> u8 {
        match self {
            Tier::One => 1,
            Tier::Two => 2,
            Tier::Three => 3,
        }
    }
}

/// A perk.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Perk {
    Lightweight,
    FlakJacket,
    Toughness,
    FastHands,
    Scavenger,
    SleightOfHand,
    Dexterity,
    DeadSilence,
    Awareness,
    TacticalMask,
}

/// Lightweight's movement speed (Black Ops 2 `perk_speedMultiplier` 1.07).
pub const LIGHTWEIGHT_SPEED: f32 = 1.07;
/// The share of explosive damage Flak Jacket lets through (Black Ops 2
/// `perk_flakJacket` 35).
pub const FLAK_JACKET_DAMAGE: f32 = 0.35;
/// The share of the hit flinch Toughness leaves (Black Ops 2: flinch to 25%,
/// `perk_damageKickReduction` 0.25).
pub const TOUGHNESS_FLINCH: f32 = 0.25;
/// Fast Hands: weapon switches take this share of their time (Black Ops 2
/// `perk_weapSwitchMultiplier` 0.5), and a throw's wind-up and recovery this
/// share (`perk_weapEquipmentUseMultiplier` 0.6). The fuse still burns at
/// its own pace.
pub const FAST_HANDS_SWITCH: f32 = 0.5;
pub const FAST_HANDS_THROW: f32 = 0.6;
/// Sleight of Hand: reloads take this share of their time (`perk_weapReloadMultiplier` 0.5).
pub const SLEIGHT_OF_HAND_RELOAD: f32 = 0.5;
/// Dexterity: a mantle or vault takes this share of its time (Black Ops 2
/// `perk_mantleReduction` 0.4 off), ladders climb this much faster
/// (`perk_fastLadderClimbMultiplier` 1.75), and the sprint-out takes this
/// share of its time (an estimate: Black Ops 2 only says "faster").
pub const DEXTERITY_CLIMB: f32 = 0.6;
pub const DEXTERITY_LADDER: f32 = 1.75;
pub const DEXTERITY_SPRINT_OUT: f32 = 0.5;
/// Tactical Mask: a flashbang blinds for this share of its time (Black Ops 2
/// `perk_shellShockReduction` 0.5).
pub const TACTICAL_MASK_FLASH: f32 = 0.5;

impl Perk {
    pub const ALL: [Perk; 10] = [
        Perk::Lightweight,
        Perk::FlakJacket,
        Perk::Toughness,
        Perk::FastHands,
        Perk::Scavenger,
        Perk::SleightOfHand,
        Perk::Dexterity,
        Perk::DeadSilence,
        Perk::Awareness,
        Perk::TacticalMask,
    ];

    pub fn tier(self) -> Tier {
        match self {
            Perk::Lightweight | Perk::FlakJacket => Tier::One,
            Perk::Toughness | Perk::FastHands | Perk::Scavenger | Perk::SleightOfHand => Tier::Two,
            Perk::Dexterity | Perk::DeadSilence | Perk::Awareness | Perk::TacticalMask => {
                Tier::Three
            }
        }
    }

    /// The perks of a tier, in menu order.
    pub fn of_tier(tier: Tier) -> impl Iterator<Item = Perk> {
        Self::ALL.into_iter().filter(move |p| p.tier() == tier)
    }

    /// The key settings, scripts, and icons use.
    pub fn key(self) -> &'static str {
        match self {
            Perk::Lightweight => "lightweight",
            Perk::FlakJacket => "flak_jacket",
            Perk::Toughness => "toughness",
            Perk::FastHands => "fast_hands",
            Perk::Scavenger => "scavenger",
            Perk::SleightOfHand => "sleight_of_hand",
            Perk::Dexterity => "dexterity",
            Perk::DeadSilence => "dead_silence",
            Perk::Awareness => "awareness",
            Perk::TacticalMask => "tactical_mask",
        }
    }

    pub fn by_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.key() == key)
    }

    /// The name players see: plain descriptions, as the wildcards have, not
    /// Call of Duty's names (which the code and docs keep for reference).
    pub fn label(self) -> &'static str {
        match self {
            Perk::Lightweight => "Light Load",
            Perk::FlakJacket => "Blast Vest",
            Perk::Toughness => "Unshaken",
            Perk::FastHands => "Quick Hands",
            Perk::Scavenger => "Scrounger",
            Perk::SleightOfHand => "Fast Reload",
            Perk::Dexterity => "Climber",
            Perk::DeadSilence => "Soft Step",
            Perk::Awareness => "Keen Ears",
            Perk::TacticalMask => "Flash Guard",
        }
    }

    /// One line on what it does.
    pub fn summary(self) -> &'static str {
        match self {
            Perk::Lightweight => "Move 7% faster. No fall damage.",
            Perk::FlakJacket => "Take 65% less damage from explosives.",
            Perk::Toughness => "Your aim flinches far less when you are shot.",
            Perk::FastHands => "Switch weapons in half the time. Throw grenades faster.",
            Perk::Scavenger => "Take ammo and grenades from packs enemies drop.",
            Perk::SleightOfHand => "Reload in half the time.",
            Perk::Dexterity => {
                "Mantle, vault, and climb ladders faster. Your weapon comes up sooner after a sprint."
            }
            Perk::DeadSilence => "Your footsteps are near silent.",
            Perk::Awareness => "Hear enemy footsteps louder.",
            Perk::TacticalMask => "Flashbangs blind you for half as long.",
        }
    }
}

/// A set of perks, one bit per [`Perk::ALL`] entry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Perks(u16);

impl Perks {
    pub const NONE: Perks = Perks(0);
    /// Bits the set uses on the wire (room for more perks).
    pub const BITS: u32 = 12;

    pub fn from_bits(bits: u16) -> Option<Self> {
        let known = (1u16 << Perk::ALL.len()) - 1;
        (bits & !known == 0).then_some(Self(bits))
    }

    pub fn bits(self) -> u16 {
        self.0
    }

    pub fn of(list: &[Perk]) -> Self {
        list.iter().fold(Self::NONE, |set, p| set.with(*p))
    }

    pub fn with(self, perk: Perk) -> Self {
        Self(self.0 | 1 << perk as u16)
    }

    pub fn without(self, perk: Perk) -> Self {
        Self(self.0 & !(1 << perk as u16))
    }

    pub fn has(self, perk: Perk) -> bool {
        self.0 & 1 << perk as u16 != 0
    }

    pub fn len(self) -> u8 {
        self.0.count_ones() as u8
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn iter(self) -> impl Iterator<Item = Perk> {
        Perk::ALL.into_iter().filter(move |p| self.has(*p))
    }

    /// One perk of each tier, each chosen by `pick(n)` (an index below `n`
    /// after wrapping), as a bot's class takes.
    pub fn one_per_tier(mut pick: impl FnMut(usize) -> usize) -> Self {
        Tier::ALL.into_iter().fold(Self::NONE, |set, tier| {
            let list: Vec<Perk> = Perk::of_tier(tier).collect();
            set.with(list[pick(list.len()) % list.len()])
        })
    }

    /// How many of the set are of `tier`.
    pub fn in_tier(self, tier: Tier) -> u8 {
        self.iter().filter(|p| p.tier() == tier).count() as u8
    }

    /// Movement speed scale (Lightweight).
    pub fn speed_scale(self) -> f32 {
        if self.has(Perk::Lightweight) {
            LIGHTWEIGHT_SPEED
        } else {
            1.0
        }
    }

    /// Whether landings hurt (not with Lightweight).
    pub fn fall_damage(self) -> bool {
        !self.has(Perk::Lightweight)
    }

    /// Scale on explosive damage taken (Flak Jacket).
    pub fn explosive_scale(self) -> f32 {
        if self.has(Perk::FlakJacket) {
            FLAK_JACKET_DAMAGE
        } else {
            1.0
        }
    }

    /// Scale on the aim flinch from a hit (Toughness).
    pub fn flinch_scale(self) -> f32 {
        if self.has(Perk::Toughness) {
            TOUGHNESS_FLINCH
        } else {
            1.0
        }
    }

    /// Scale on how long a flashbang blinds (Tactical Mask).
    pub fn flash_scale(self) -> f32 {
        if self.has(Perk::TacticalMask) {
            TACTICAL_MASK_FLASH
        } else {
            1.0
        }
    }

    /// How fast weapon timers run, as multiples of real time: a perk that
    /// halves a reload doubles its rate. Timers advance by `dt * rate`, so
    /// every stage of the action (and its animation, which reads the same
    /// timer) keeps its proportions.
    pub fn handling(self) -> Handling {
        let rate = |perk: Perk, share: f32| if self.has(perk) { 1.0 / share } else { 1.0 };
        Handling {
            reload: rate(Perk::SleightOfHand, SLEIGHT_OF_HAND_RELOAD),
            switch: rate(Perk::FastHands, FAST_HANDS_SWITCH),
            throw: rate(Perk::FastHands, FAST_HANDS_THROW),
            sprint_out: rate(Perk::Dexterity, DEXTERITY_SPRINT_OUT),
        }
    }

    /// Scale on mantle and vault times (Dexterity).
    pub fn climb_time_scale(self) -> f32 {
        if self.has(Perk::Dexterity) {
            DEXTERITY_CLIMB
        } else {
            1.0
        }
    }

    /// Scale on ladder climbing speed (Dexterity).
    pub fn ladder_speed_scale(self) -> f32 {
        if self.has(Perk::Dexterity) {
            DEXTERITY_LADDER
        } else {
            1.0
        }
    }
}

/// Rates weapon timers run at (see [`Perks::handling`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Handling {
    pub reload: f32,
    pub switch: f32,
    pub throw: f32,
    pub sprint_out: f32,
}

impl Default for Handling {
    fn default() -> Self {
        Perks::NONE.handling()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tier_has_perks_and_keys_round_trip() {
        for tier in Tier::ALL {
            assert!(Perk::of_tier(tier).count() >= 2, "{tier:?}");
        }
        for perk in Perk::ALL {
            assert_eq!(Perk::by_key(perk.key()), Some(perk));
            assert!(!perk.label().is_empty() && !perk.summary().is_empty());
        }
    }

    #[test]
    fn sets_round_trip_their_bits() {
        let set = Perks::of(&[Perk::Lightweight, Perk::TacticalMask]);
        assert_eq!(Perks::from_bits(set.bits()), Some(set));
        assert_eq!(Perks::from_bits(1 << 11), None);
        assert_eq!(set.len(), 2);
        assert_eq!(set.in_tier(Tier::One), 1);
        assert_eq!(set.in_tier(Tier::Two), 0);
        assert_eq!(
            set.without(Perk::Lightweight),
            Perks::of(&[Perk::TacticalMask])
        );
        assert!(Perk::ALL.len() as u32 <= Perks::BITS);
    }

    #[test]
    fn a_random_set_has_one_perk_of_each_tier() {
        for seed in 0..20u32 {
            let mut n = 0;
            let set = Perks::one_per_tier(|len| {
                n += 1;
                crate::rng::hash(seed, n) as usize % len
            });
            for tier in Tier::ALL {
                assert_eq!(set.in_tier(tier), 1);
            }
        }
    }

    #[test]
    fn effects_follow_black_ops_2() {
        let none = Perks::NONE;
        assert_eq!(none.handling(), Handling::default());
        assert_eq!(none.speed_scale(), 1.0);
        let all = Perks::of(&Perk::ALL);
        assert_eq!(all.speed_scale(), 1.07);
        assert!(!all.fall_damage());
        assert_eq!(all.explosive_scale(), 0.35);
        let h = all.handling();
        assert_eq!(h.reload, 2.0);
        assert_eq!(h.switch, 2.0);
        assert!((h.throw - 1.0 / 0.6).abs() < 1e-6);
        assert_eq!(h.sprint_out, 2.0);
        assert_eq!(all.climb_time_scale(), 0.6);
        assert_eq!(all.ladder_speed_scale(), 1.75);
    }
}
