//! The forge: the Zombies upgrade machine, Black Ops' Pack-a-Punch under our
//! own name and look.
//!
//! With the power on, a player holding a plain weapon that has an upgrade
//! pays [`PRICE`] and hands it over; the forge works it for
//! [`WORK_SECONDS`], then holds the upgraded weapon out for
//! [`TAKE_SECONDS`]. Left untaken, it is lost, as in Black Ops. The upgraded
//! weapon (the table's `<key>+forged` variant, `weapon::def::forged`) does
//! twice the damage, carries half again the magazine and twice the reserve,
//! and comes back full. Its ammo from a wall costs [`UPGRADED_AMMO_PRICE`].
//!
//! Numbers from Black Ops III's `_zm_pack_a_punch.gsc` and
//! `_zm_pack_a_punch_util.gsc` (cost 5000, `wait(0.35)` and `wait(3)` while
//! it works, a 15 s `timeout`) and `_zm_weapons.gsc`
//! (`get_upgraded_ammo_cost`, 4500).

use crate::weapon::{CombatState, WeaponId};

/// Points an upgrade costs (`vending_weapon_upgrade_cost`, 5000).
pub const PRICE: u32 = 5000;
/// Seconds the forge works the weapon (0.35 + 3).
pub const WORK_SECONDS: f32 = 3.35;
/// Seconds the upgraded weapon waits to be taken (`pack_a_punch.timeout`).
pub const TAKE_SECONDS: f32 = 15.0;
/// Points a wall weapon's ammo costs for its upgraded version
/// (`get_upgraded_ammo_cost`).
pub const UPGRADED_AMMO_PRICE: u32 = 4500;

/// Damage, magazine, and reserve of an upgraded weapon over the plain one.
/// Black Ops sets each upgraded weapon's numbers by hand; these are the
/// usual changes (about double the damage, a bigger magazine, more spare).
pub const DAMAGE_SCALE: f32 = 2.0;
pub const MAGAZINE_SCALE: f32 = 1.5;
pub const RESERVE_SCALE: f32 = 2.0;

/// Why a player cannot use the forge now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    NoPower,
    /// The forge is working or holding someone's weapon.
    Busy,
    /// The weapon in hand has no upgrade, or is upgraded already.
    NoUpgrade,
    /// It is the only weapon they carry: they would be left with nothing.
    OnlyWeapon,
    Points,
}

/// The upgrade of the weapon in `combat`'s hands, if a player with
/// `points` may have it forged now (with the power `powered` and the forge
/// `free`).
pub fn can_forge(
    combat: &CombatState,
    points: u32,
    powered: bool,
    free: bool,
) -> Result<WeaponId, Refusal> {
    if !powered {
        return Err(Refusal::NoPower);
    }
    if !free {
        return Err(Refusal::Busy);
    }
    let weapon = combat.weapon();
    let upgrade = weapon.id.upgraded().filter(|_| weapon.carried);
    let Some(upgrade) = upgrade else {
        return Err(Refusal::NoUpgrade);
    };
    if combat.slots.iter().filter(|s| s.carried).count() < 2 {
        return Err(Refusal::OnlyWeapon);
    }
    if points < PRICE {
        return Err(Refusal::Points);
    }
    Ok(upgrade)
}

/// Takes the weapon in hand away for the forge and brings the other one up.
pub fn hand_over(combat: &mut CombatState) {
    let active = usize::from(combat.active);
    let shots = combat.shots_fired;
    combat.slots[active].carried = false;
    combat.active = 1 - combat.active;
    *combat = combat.raised();
    combat.shots_fired = shots;
}

/// What a wall weapon's ammo costs a player carrying `combat`: 4500 for a
/// weapon they carry upgraded, otherwise `zombies::pickup_cost`.
pub fn wall_cost(price: i64, combat: &CombatState, weapon: WeaponId) -> u32 {
    if combat.carries_upgraded(weapon) {
        UPGRADED_AMMO_PRICE
    } else {
        super::pickup_cost(price, combat.carries(weapon))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapon::{CombatContext, CombatInput, step};

    #[test]
    fn upgrades_double_damage_and_grow_the_magazine() {
        let m4 = WeaponId::expect("m4");
        let up = m4.upgraded().expect("the M4A1 has an upgrade");
        let (plain, forged) = (m4.def(), up.def());
        assert!(forged.upgraded && !forged.plain());
        assert_eq!(forged.base_key, "m4");
        assert_eq!(forged.model_key(), "m4");
        assert_eq!(up.plain(), m4);
        assert_eq!(forged.damage.near, plain.damage.near * 2.0);
        assert_eq!(forged.magazine, 45);
        assert_eq!(forged.reserve, 240);
        assert_eq!(forged.name, "M4A1 MK II");
        assert_eq!(up.upgraded(), None, "no upgrade of an upgrade");
        assert_eq!(WeaponId::expect("laser").upgraded(), None);
        assert_eq!(WeaponId::expect("m4+grip").upgraded(), None);
        // A two-round shotgun still gains a round.
        for id in WeaponId::all().filter(|id| id.def().upgraded) {
            assert!(id.def().magazine > id.plain().def().magazine);
        }
    }

    #[test]
    fn the_forge_needs_power_a_second_weapon_and_points() {
        let mut combat = CombatState::new([WeaponId::expect("ak47"), WeaponId::expect("m1911")]);
        assert_eq!(can_forge(&combat, 9000, false, true), Err(Refusal::NoPower));
        assert_eq!(can_forge(&combat, 9000, true, false), Err(Refusal::Busy));
        assert_eq!(can_forge(&combat, 4999, true, true), Err(Refusal::Points));
        let up = can_forge(&combat, 5000, true, true).unwrap();
        assert_eq!(up.def().key, "ak47+forged");
        hand_over(&mut combat);
        assert!(!combat.slots[0].carried);
        assert_eq!(combat.weapon().id, WeaponId::expect("m1911"));
        assert_eq!(
            can_forge(&combat, 9000, true, true),
            Err(Refusal::OnlyWeapon)
        );
        let alone = crate::zombies::start_combat();
        assert_eq!(
            can_forge(&alone, 9000, true, true),
            Err(Refusal::OnlyWeapon)
        );
    }

    #[test]
    fn wall_ammo_for_an_upgraded_weapon_costs_4500_and_refills_it() {
        let m4 = WeaponId::expect("m4");
        let up = m4.upgraded().unwrap();
        let mut combat = CombatState::new([up, WeaponId::expect("m1911")]);
        assert_eq!(wall_cost(1200, &combat, m4), 4500);
        assert_eq!(wall_cost(1200, &CombatState::new([m4, m4]), m4), 600);
        assert_eq!(wall_cost(1200, &crate::zombies::start_combat(), m4), 1200);
        combat.slots[0].reserve = 3;
        let input = CombatInput {
            pickup: Some(m4),
            ..Default::default()
        };
        step(
            &mut combat,
            &input,
            &CombatContext::default(),
            1.0 / 120.0,
            &mut Vec::new(),
        );
        assert_eq!(combat.slots[0].id, up, "the upgraded weapon stays");
        assert_eq!(combat.slots[0].reserve, up.def().reserve);
    }
}
