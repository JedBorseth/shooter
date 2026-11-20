//! Create-a-Class: the rules a custom class must follow.
//!
//! Modelled on Black Ops 2's Pick 10 (kept in Black Ops 3 and Black Ops 4):
//! a class has [`PICKS`] points, and every item in it costs one, whether a
//! weapon, an attachment, a grenade, or a wildcard. Leaving a slot empty
//! frees its point for something else. The slots have their own limits,
//! which wildcards raise:
//!
//! | Slot | Limit | Raised by |
//! | --- | --- | --- |
//! | Primary weapon | one, any primary | |
//! | Primary attachments | two | [`Wildcard::PrimaryGunfighter`]: three |
//! | Secondary weapon | one, any secondary | [`Wildcard::Overkill`]: a primary instead |
//! | Lethal | one frag | [`Wildcard::DangerClose`]: two |
//! | Tactical | two of one kind (flashbang or smoke) | |
//!
//! A weapon takes only the attachments its file lists, and at most one optic
//! (a red dot or an ACOG), as in Call of Duty.
//!
//! Differences from Black Ops 2, on purpose:
//!
//! - A class must carry at least one gun: there is no knife-only weapon to
//!   fall back on.
//! - No perks yet, so no perk slots and no Perk Greed wildcards; their points
//!   go to the rest. Secondary Gunfighter and secondary attachments wait for
//!   a pistol that takes attachments, and Tactician (a tactical in the lethal
//!   slot) for a lethal button that can throw one.
//!
//! The server checks every class it is sent with [`Class::validate`] and
//! never trusts the client's arithmetic. Level unlocks are checked by the
//! client only until accounts exist (Phase 5): the server cannot know a
//! player's level yet.

use crate::equipment;
use crate::weapon::{Attachment, CombatState, Slot, WeaponId};

/// Points a class can spend (Black Ops 2's Pick 10).
pub const PICKS: u8 = 10;
/// Attachments a primary takes, and with Primary Gunfighter.
pub const PRIMARY_ATTACHMENTS: u8 = 2;
pub const GUNFIGHTER_ATTACHMENTS: u8 = 3;
/// Frags a class carries, and with Danger Close.
pub const LETHALS: u8 = 1;
pub const DANGER_CLOSE_LETHALS: u8 = 2;
/// Tacticals a class carries (two of the same kind, a point each).
pub const TACTICALS: u8 = 2;
/// Custom class slots a player has (Black Ops 2's five, before prestige).
pub const CUSTOM_CLASSES: usize = 5;

/// A set of attachments, one bit per [`Attachment::ALL`] entry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Attachments(u8);

impl Attachments {
    pub const NONE: Attachments = Attachments(0);
    /// Bits the set uses on the wire.
    pub const BITS: u32 = Attachment::ALL.len() as u32;

    pub fn from_bits(bits: u8) -> Option<Self> {
        (u32::from(bits) < 1 << Self::BITS).then_some(Self(bits))
    }

    pub fn bits(self) -> u8 {
        self.0
    }

    pub fn of(list: &[Attachment]) -> Self {
        list.iter().fold(Self::NONE, |set, a| set.with(*a))
    }

    pub fn with(self, attachment: Attachment) -> Self {
        Self(self.0 | 1 << index(attachment))
    }

    pub fn without(self, attachment: Attachment) -> Self {
        Self(self.0 & !(1 << index(attachment)))
    }

    pub fn has(self, attachment: Attachment) -> bool {
        self.0 & 1 << index(attachment) != 0
    }

    pub fn len(self) -> u8 {
        self.0.count_ones() as u8
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The attachments in [`Attachment::ALL`] order.
    pub fn iter(self) -> impl Iterator<Item = Attachment> {
        Attachment::ALL.into_iter().filter(move |a| self.has(*a))
    }
}

fn index(attachment: Attachment) -> u32 {
    Attachment::ALL
        .iter()
        .position(|a| *a == attachment)
        .unwrap_or_default() as u32
}

/// A wildcard: a point spent to bend one slot's rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Wildcard {
    /// A third attachment on the primary.
    PrimaryGunfighter,
    /// A primary weapon in the secondary slot.
    Overkill,
    /// A second frag.
    DangerClose,
}

impl Wildcard {
    pub const ALL: [Wildcard; 3] = [
        Wildcard::PrimaryGunfighter,
        Wildcard::Overkill,
        Wildcard::DangerClose,
    ];

    /// The key settings and scripts use.
    pub fn key(self) -> &'static str {
        match self {
            Wildcard::PrimaryGunfighter => "gunfighter",
            Wildcard::Overkill => "overkill",
            Wildcard::DangerClose => "danger_close",
        }
    }

    pub fn by_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|w| w.key() == key)
    }

    /// The name players see. Plain descriptions of what each does, not
    /// Call of Duty's names.
    pub fn label(self) -> &'static str {
        match self {
            Wildcard::PrimaryGunfighter => "Third Attachment",
            Wildcard::Overkill => "Two Primaries",
            Wildcard::DangerClose => "Extra Lethal",
        }
    }

    /// One line on what it allows.
    pub fn summary(self) -> &'static str {
        match self {
            Wildcard::PrimaryGunfighter => "A third attachment on your primary weapon.",
            Wildcard::Overkill => "A second primary weapon in place of your sidearm.",
            Wildcard::DangerClose => "Carry a second frag grenade.",
        }
    }
}

/// A set of wildcards, one bit per [`Wildcard::ALL`] entry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Wildcards(u8);

impl Wildcards {
    pub const NONE: Wildcards = Wildcards(0);
    /// Bits the set uses on the wire (room for more wildcards).
    pub const BITS: u32 = 4;

    pub fn from_bits(bits: u8) -> Option<Self> {
        let known = (1u8 << Wildcard::ALL.len()) - 1;
        (bits & !known == 0).then_some(Self(bits))
    }

    pub fn bits(self) -> u8 {
        self.0
    }

    pub fn of(list: &[Wildcard]) -> Self {
        list.iter().fold(Self::NONE, |set, w| set.with(*w))
    }

    pub fn with(self, wildcard: Wildcard) -> Self {
        Self(self.0 | 1 << wildcard as u8)
    }

    pub fn has(self, wildcard: Wildcard) -> bool {
        self.0 & 1 << wildcard as u8 != 0
    }

    pub fn len(self) -> u8 {
        self.0.count_ones() as u8
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn iter(self) -> impl Iterator<Item = Wildcard> {
        Wildcard::ALL.into_iter().filter(move |w| self.has(*w))
    }
}

/// A weapon in a class: the plain weapon and what is fitted to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Gun {
    /// The plain weapon (never an attachment variant).
    pub base: WeaponId,
    pub attachments: Attachments,
}

impl Gun {
    pub fn plain(base: WeaponId) -> Self {
        Self {
            base,
            attachments: Attachments::NONE,
        }
    }

    pub fn with(base: WeaponId, attachments: &[Attachment]) -> Self {
        Self {
            base,
            attachments: Attachments::of(attachments),
        }
    }

    /// The weapon table entry for this gun: the plain weapon or the variant
    /// with exactly these attachments, if the table has one.
    pub fn weapon(self) -> Option<WeaponId> {
        let list: Vec<Attachment> = self.attachments.iter().collect();
        self.base.with_attachments(&list)
    }
}

/// Why a class was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassError {
    /// It costs more than [`PICKS`].
    OverBudget,
    /// It carries no gun at all.
    NoGun,
    /// The primary slot holds something that is not a plain primary weapon
    /// players may pick (the laser is secret, variants come from attachments).
    NotAPrimary,
    /// The secondary slot holds something that is not a plain secondary, or
    /// a primary without Two Primaries.
    NotASecondary,
    /// More attachments than the slot takes.
    TooManyAttachments,
    /// An attachment the weapon does not take.
    AttachmentNotOffered,
    /// Two optics on one weapon.
    TwoOptics,
    /// The weapon table has no variant with this set of attachments.
    NoVariant,
    TooManyLethals,
    TooManyTacticals,
    /// The tactical slot holds a lethal.
    NotATactical,
}

/// A class: what a player spawns with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Class {
    pub primary: Option<Gun>,
    pub secondary: Option<Gun>,
    /// Frags carried.
    pub lethals: u8,
    /// The tactical kind, and how many.
    pub tactical: equipment::Kind,
    pub tacticals: u8,
    pub wildcards: Wildcards,
}

impl Default for Class {
    /// The class a player starts with before choosing one: the first default.
    fn default() -> Self {
        DefaultClass::ALL[0].class()
    }
}

impl Class {
    /// Points it spends: one per weapon, attachment, grenade, and wildcard.
    pub fn cost(&self) -> u8 {
        let gun = |g: Option<Gun>| g.map_or(0, |g| 1 + g.attachments.len());
        gun(self.primary)
            + gun(self.secondary)
            + self.lethals
            + self.tacticals
            + self.wildcards.len()
    }

    /// Checks every rule. The server refuses a class that fails.
    pub fn validate(&self) -> Result<(), ClassError> {
        if self.primary.is_none() && self.secondary.is_none() {
            return Err(ClassError::NoGun);
        }
        if let Some(primary) = self.primary {
            if !is_class_weapon(primary.base, Slot::Primary) {
                return Err(ClassError::NotAPrimary);
            }
            let limit = if self.wildcards.has(Wildcard::PrimaryGunfighter) {
                GUNFIGHTER_ATTACHMENTS
            } else {
                PRIMARY_ATTACHMENTS
            };
            check_attachments(primary, limit)?;
        }
        if let Some(secondary) = self.secondary {
            let overkill = self.wildcards.has(Wildcard::Overkill)
                && is_class_weapon(secondary.base, Slot::Primary);
            if !overkill && !is_class_weapon(secondary.base, Slot::Secondary) {
                return Err(ClassError::NotASecondary);
            }
            // A primary carried with Two Primaries takes the primary's limit;
            // a sidearm takes none until pistols offer attachments.
            let limit = if overkill { PRIMARY_ATTACHMENTS } else { 0 };
            check_attachments(secondary, limit)?;
        }
        let lethals = if self.wildcards.has(Wildcard::DangerClose) {
            DANGER_CLOSE_LETHALS
        } else {
            LETHALS
        };
        if self.lethals > lethals {
            return Err(ClassError::TooManyLethals);
        }
        if self.tactical.lethal() {
            return Err(ClassError::NotATactical);
        }
        if self.tacticals > TACTICALS {
            return Err(ClassError::TooManyTacticals);
        }
        if self.cost() > PICKS {
            return Err(ClassError::OverBudget);
        }
        Ok(())
    }

    /// The weapons to spawn with, primary slot first. Only meaningful for a
    /// valid class.
    pub fn weapons(&self) -> [Option<WeaponId>; 2] {
        [
            self.primary.and_then(Gun::weapon),
            self.secondary.and_then(Gun::weapon),
        ]
    }

    /// What a player spawns carrying: the weapons with the first one in
    /// hand, and the grenades. A class without a gun (never valid) spawns
    /// with the default class's.
    pub fn combat(&self) -> CombatState {
        let state = match self.weapons() {
            [Some(primary), Some(secondary)] => CombatState::new([primary, secondary]),
            [Some(only), None] | [None, Some(only)] => CombatState::only(only),
            [None, None] => return Class::default().combat(),
        };
        state
            .with_lethals(self.lethals)
            .with_tacticals(self.tactical, self.tacticals)
    }
}

fn check_attachments(gun: Gun, limit: u8) -> Result<(), ClassError> {
    if gun.attachments.len() > limit {
        return Err(ClassError::TooManyAttachments);
    }
    let offered = &gun.base.def().attachments;
    if gun.attachments.iter().any(|a| !offered.contains(&a)) {
        return Err(ClassError::AttachmentNotOffered);
    }
    if gun.attachments.iter().filter(|a| a.optic()).count() > 1 {
        return Err(ClassError::TwoOptics);
    }
    if gun.weapon().is_none() {
        return Err(ClassError::NoVariant);
    }
    Ok(())
}

/// Whether a class may put this weapon in `slot`: a plain weapon of that
/// slot, and not the secret laser.
pub fn is_class_weapon(id: WeaponId, slot: Slot) -> bool {
    let def = id.def();
    def.slot == slot && def.attachment.is_none() && !def.beam
}

/// The plain weapons a class may carry in `slot`, in table order.
pub fn class_weapons(slot: Slot) -> Vec<WeaponId> {
    WeaponId::all()
        .filter(|id| is_class_weapon(*id, slot))
        .collect()
}

/// The ready-made classes every player has, as in Call of Duty before
/// Create-a-Class unlocks (and bots use them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultClass {
    Assault,
    Rifleman,
    CloseQuarters,
    Support,
    Breacher,
    Marksman,
}

impl DefaultClass {
    pub const ALL: [DefaultClass; 6] = [
        DefaultClass::Assault,
        DefaultClass::Rifleman,
        DefaultClass::CloseQuarters,
        DefaultClass::Support,
        DefaultClass::Breacher,
        DefaultClass::Marksman,
    ];

    pub fn name(self) -> &'static str {
        match self {
            DefaultClass::Assault => "Assault",
            DefaultClass::Rifleman => "Rifleman",
            DefaultClass::CloseQuarters => "Close Quarters",
            DefaultClass::Support => "Support",
            DefaultClass::Breacher => "Breacher",
            DefaultClass::Marksman => "Marksman",
        }
    }

    pub fn class(self) -> Class {
        use Attachment::*;
        use equipment::Kind::{Flash, Smoke};
        let pistol = Some(Gun::plain(WeaponId::expect("m1911")));
        let (primary, attachments, tactical, tacticals): (&str, &[Attachment], _, u8) = match self {
            DefaultClass::Assault => ("m4", &[RedDot], Flash, 1),
            DefaultClass::Rifleman => ("ak47", &[Grip], Flash, 1),
            DefaultClass::CloseQuarters => ("pdw9", &[Suppressor], Flash, 1),
            DefaultClass::Support => ("lsw6", &[ExtendedMag], Smoke, 1),
            DefaultClass::Breacher => ("m870", &[Grip], Flash, 1),
            DefaultClass::Marksman => ("m40", &[ExtendedMag], Smoke, 1),
        };
        Class {
            primary: Some(Gun::with(WeaponId::expect(primary), attachments)),
            secondary: pistol,
            lethals: 1,
            tactical,
            tacticals,
            wildcards: Wildcards::NONE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use equipment::Kind;

    fn m4() -> WeaponId {
        WeaponId::expect("m4")
    }

    fn class(primary: Option<Gun>) -> Class {
        Class {
            primary,
            secondary: Some(Gun::plain(WeaponId::expect("m1911"))),
            lethals: 1,
            tactical: Kind::Flash,
            tacticals: 1,
            wildcards: Wildcards::NONE,
        }
    }

    #[test]
    fn default_classes_are_valid_and_spend_their_points() {
        for default in DefaultClass::ALL {
            let class = default.class();
            assert_eq!(class.validate(), Ok(()), "{}", default.name());
            assert!(class.cost() <= PICKS, "{}", default.name());
            assert!(class.weapons()[0].is_some(), "{}", default.name());
        }
    }

    #[test]
    fn every_item_costs_one_point() {
        let mut c = class(Some(Gun::with(m4(), &[Attachment::Grip])));
        // Primary + grip + sidearm + frag + flashbang.
        assert_eq!(c.cost(), 5);
        c.tacticals = 2;
        c.wildcards = Wildcards::of(&[Wildcard::DangerClose]);
        c.lethals = 2;
        assert_eq!(c.cost(), 8);
        assert_eq!(c.validate(), Ok(()));
    }

    #[test]
    fn more_than_ten_points_is_refused() {
        let mut c = class(Some(Gun::with(m4(), &[Attachment::Grip])));
        c.secondary = Some(Gun::plain(WeaponId::expect("ak47")));
        c.tacticals = 2;
        c.lethals = 2;
        c.wildcards = Wildcards::of(&[
            Wildcard::DangerClose,
            Wildcard::Overkill,
            Wildcard::PrimaryGunfighter,
        ]);
        // 2 + 1 + 2 + 2 + 3 = 10: allowed.
        assert_eq!(c.cost(), 10);
        assert_eq!(c.validate(), Ok(()));
        c.secondary = Some(Gun::with(WeaponId::expect("ak47"), &[Attachment::Grip]));
        assert_eq!(c.cost(), 11);
        assert_eq!(c.validate(), Err(ClassError::OverBudget));
    }

    #[test]
    fn slot_limits_and_their_wildcards() {
        let mut c = class(Some(Gun::plain(m4())));
        c.lethals = 2;
        assert_eq!(c.validate(), Err(ClassError::TooManyLethals));
        c.wildcards = Wildcards::of(&[Wildcard::DangerClose]);
        assert_eq!(c.validate(), Ok(()));

        let mut c = class(Some(Gun::plain(m4())));
        c.tacticals = 3;
        assert_eq!(c.validate(), Err(ClassError::TooManyTacticals));
        c.tacticals = 1;
        c.tactical = Kind::Frag;
        assert_eq!(c.validate(), Err(ClassError::NotATactical));

        let mut c = class(Some(Gun::plain(m4())));
        c.secondary = Some(Gun::plain(WeaponId::expect("ak47")));
        assert_eq!(c.validate(), Err(ClassError::NotASecondary));
        c.wildcards = Wildcards::of(&[Wildcard::Overkill]);
        assert_eq!(c.validate(), Ok(()));
    }

    #[test]
    fn attachments_follow_the_weapon_file() {
        use Attachment::*;
        let c = class(Some(Gun::with(m4(), &[Grip, Suppressor, ExtendedMag])));
        assert_eq!(c.validate(), Err(ClassError::TooManyAttachments));
        let c = class(Some(Gun::with(m4(), &[RedDot, Acog])));
        assert_eq!(c.validate(), Err(ClassError::TwoOptics));
        // The sniper takes no grip.
        let c = class(Some(Gun::with(WeaponId::expect("m40"), &[Grip])));
        assert_eq!(c.validate(), Err(ClassError::AttachmentNotOffered));
        // A sidearm takes none yet.
        let mut c = class(Some(Gun::plain(m4())));
        c.secondary = Some(Gun::with(WeaponId::expect("m1911"), &[Suppressor]));
        assert_eq!(c.validate(), Err(ClassError::TooManyAttachments));
    }

    #[test]
    fn a_class_needs_a_gun_and_no_variants_or_secrets() {
        let mut c = class(None);
        assert_eq!(c.validate(), Ok(()), "a sidearm alone is a class");
        c.secondary = None;
        assert_eq!(c.validate(), Err(ClassError::NoGun));
        let c = class(Some(Gun::plain(WeaponId::expect("m4+grip"))));
        assert_eq!(c.validate(), Err(ClassError::NotAPrimary));
        let c = class(Some(Gun::plain(WeaponId::expect("laser"))));
        assert_eq!(c.validate(), Err(ClassError::NotAPrimary));
        let c = class(Some(Gun::plain(WeaponId::expect("m1911"))));
        assert_eq!(c.validate(), Err(ClassError::NotAPrimary));
    }

    #[test]
    fn guns_resolve_to_their_variant() {
        assert_eq!(Gun::plain(m4()).weapon(), Some(m4()));
        assert_eq!(
            Gun::with(m4(), &[Attachment::Grip]).weapon(),
            Some(WeaponId::expect("m4+grip"))
        );
    }

    #[test]
    fn a_class_spawns_with_what_it_carries() {
        let mut c = class(Some(Gun::with(m4(), &[Attachment::Grip])));
        c.lethals = 0;
        c.tactical = Kind::Smoke;
        c.tacticals = 2;
        let combat = c.combat();
        assert_eq!(combat.weapon().id, WeaponId::expect("m4+grip"));
        assert!(combat.carries(WeaponId::expect("m1911")));
        assert_eq!(combat.lethals, 0);
        assert_eq!((combat.tactical, combat.tacticals), (Kind::Smoke, 2));
        // A sidearm alone comes up in hand.
        let combat = class(None).combat();
        assert_eq!(combat.weapon().id, WeaponId::expect("m1911"));
        assert!(!combat.carries(m4()));
        assert_eq!(combat.lethals, 1);
    }

    #[test]
    fn attachment_sets_round_trip_their_bits() {
        let set = Attachments::of(&[Attachment::Acog, Attachment::ExtendedMag]);
        assert_eq!(Attachments::from_bits(set.bits()), Some(set));
        assert_eq!(Attachments::from_bits(1 << Attachments::BITS), None);
        assert_eq!(
            set.iter().collect::<Vec<_>>(),
            vec![Attachment::Acog, Attachment::ExtendedMag]
        );
        let wild = Wildcards::of(&[Wildcard::Overkill]);
        assert_eq!(Wildcards::from_bits(wild.bits()), Some(wild));
        assert_eq!(Wildcards::from_bits(0b1000), None);
    }
}
