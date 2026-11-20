//! Scavenger: ammunition packs dead players drop for Scavenger players.
//!
//! Black Ops 2's Scavenger (Perk 2): every player who dies leaves a pack at
//! their feet. Only players with Scavenger see the packs and can take them,
//! and only their enemies' (anyone else's in free-for-all). Walking over one
//! takes it: a magazine's worth of reserve for every gun carried, and a
//! lethal and a tactical back, each up to what the class started with
//! (Black Ops 2's `perk_scavenger_clip_multiplier` 1,
//! `perk_scavenger_lethal_proc` 1, `perk_scavenger_tactical_proc` 1). The
//! first Scavenger player to reach a pack takes it. A pack nobody takes is
//! gone after [`PACK_SECONDS`].
//!
//! The server holds the packs and decides every pickup ([`Packs`]); clients
//! draw them from events (`protocol::ScavengerEvent`).

use glam::Vec3;

use crate::TICK_RATE;
use crate::net::Tick;
use crate::perk::{Perk, Perks};
use crate::tdm::Team;
use crate::weapon::CombatState;

/// How close (metres, horizontally, from the feet) a pack is taken.
pub const PICKUP_RADIUS: f32 = 1.0;
/// How far above or below the feet a pack is still in reach.
pub const PICKUP_HEIGHT: f32 = 1.2;
/// Seconds a pack lies before it is gone. Black Ops 2 gives no number; long
/// enough to fight over the body and walk on (an estimate).
pub const PACK_SECONDS: f32 = 30.0;
/// Packs on the ground at once; the oldest goes first.
pub const MAX_PACKS: usize = 32;

/// A pack on the ground.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pack {
    pub id: u16,
    /// Where it lies (the dead player's feet).
    pub at: Vec3,
    /// Who dropped it, and their side (`None` without teams).
    pub dropped_by: u8,
    pub side: Option<Team>,
    pub dropped: Tick,
}

/// A player who might take a pack this tick.
#[derive(Clone, Copy, Debug)]
pub struct Taker {
    pub id: u8,
    pub side: Option<Team>,
    pub feet: Vec3,
    pub perks: Perks,
    pub alive: bool,
}

impl Taker {
    /// Whether this player may take `pack`: alive, with Scavenger, and an
    /// enemy of whoever dropped it.
    pub fn may_take(&self, pack: &Pack) -> bool {
        let enemy = match (self.side, pack.side) {
            (Some(a), Some(b)) => a != b,
            _ => self.id != pack.dropped_by,
        };
        self.alive && self.perks.has(Perk::Scavenger) && enemy
    }

    fn reaches(&self, pack: &Pack) -> bool {
        let to = pack.at - self.feet;
        to.x * to.x + to.z * to.z <= PICKUP_RADIUS * PICKUP_RADIUS && to.y.abs() <= PICKUP_HEIGHT
    }
}

/// The packs on the ground.
#[derive(Clone, Debug, Default)]
pub struct Packs {
    packs: Vec<Pack>,
    next_id: u16,
}

impl Packs {
    pub fn all(&self) -> &[Pack] {
        &self.packs
    }

    /// Drops a pack. Returns it, and the oldest pack if this pushed it out.
    pub fn drop_pack(
        &mut self,
        at: Vec3,
        dropped_by: u8,
        side: Option<Team>,
        tick: Tick,
    ) -> (Pack, Option<Pack>) {
        let pack = Pack {
            id: self.next_id,
            at,
            dropped_by,
            side,
            dropped: tick,
        };
        self.next_id = self.next_id.wrapping_add(1);
        let pushed_out = (self.packs.len() >= MAX_PACKS).then(|| self.packs.remove(0));
        self.packs.push(pack);
        (pack, pushed_out)
    }

    /// Takes away the packs that have lain [`PACK_SECONDS`]; returns them.
    pub fn expire(&mut self, tick: Tick) -> Vec<Pack> {
        let life = (PACK_SECONDS * TICK_RATE as f32) as Tick;
        let (gone, kept) = self
            .packs
            .iter()
            .partition(|p| tick.saturating_sub(p.dropped) >= life);
        self.packs = kept;
        gone
    }

    /// The packs taken this tick, each by the first taker in the list who
    /// reaches it and may take it: (pack, taker id). They leave the ground.
    pub fn take(&mut self, takers: &[Taker]) -> Vec<(Pack, u8)> {
        let mut taken = Vec::new();
        self.packs.retain(|pack| {
            match takers.iter().find(|t| t.may_take(pack) && t.reaches(pack)) {
                Some(taker) => {
                    taken.push((*pack, taker.id));
                    false
                }
                None => true,
            }
        });
        taken
    }

    /// Clears the ground (a new round or match).
    pub fn clear(&mut self) -> Vec<Pack> {
        std::mem::take(&mut self.packs)
    }
}

/// What a pack gives: a magazine's worth of reserve for every gun carried,
/// up to its starting reserve, and a lethal and a tactical, up to
/// `max_lethals` and `max_tacticals` (what the class started with). Returns
/// whether anything was added.
pub fn resupply(combat: &mut CombatState, max_lethals: u8, max_tacticals: u8) -> bool {
    let mut gained = false;
    for slot in combat.slots.iter_mut().filter(|s| s.carried) {
        let def = slot.id.def();
        let reserve = slot.reserve.saturating_add(def.magazine).min(def.reserve);
        gained |= reserve > slot.reserve;
        slot.reserve = reserve.max(slot.reserve);
    }
    if combat.lethals < max_lethals {
        combat.lethals += 1;
        gained = true;
    }
    if combat.tacticals < max_tacticals {
        combat.tacticals += 1;
        gained = true;
    }
    gained
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapon::WeaponId;

    fn taker(id: u8, side: Option<Team>, feet: Vec3, perks: &[Perk]) -> Taker {
        Taker {
            id,
            side,
            feet,
            perks: Perks::of(perks),
            alive: true,
        }
    }

    #[test]
    fn only_an_enemy_with_scavenger_takes_a_pack_in_reach() {
        let mut packs = Packs::default();
        let at = Vec3::new(5.0, 0.0, 5.0);
        packs.drop_pack(at, 1, Some(0), 100);
        let near = at + Vec3::new(0.6, 0.0, 0.3);
        // A teammate, a player without the perk, and one too far: no.
        let nobody = [
            taker(2, Some(0), near, &[Perk::Scavenger]),
            taker(3, Some(1), near, &[Perk::Lightweight]),
            taker(
                4,
                Some(1),
                at + Vec3::new(1.5, 0.0, 0.0),
                &[Perk::Scavenger],
            ),
            taker(
                5,
                Some(1),
                at + Vec3::new(0.0, 2.0, 0.0),
                &[Perk::Scavenger],
            ),
        ];
        assert!(packs.take(&nobody).is_empty());
        let mut dead = taker(6, Some(1), near, &[Perk::Scavenger]);
        dead.alive = false;
        assert!(packs.take(&[dead]).is_empty());
        // An enemy with Scavenger, in reach: the first such takes it.
        let both = [
            taker(7, Some(1), near, &[Perk::Scavenger]),
            taker(8, Some(1), near, &[Perk::Scavenger]),
        ];
        let taken = packs.take(&both);
        assert_eq!(taken.len(), 1);
        assert_eq!(taken[0].1, 7);
        assert!(packs.all().is_empty());
    }

    #[test]
    fn without_teams_anyone_else_takes_it() {
        let mut packs = Packs::default();
        packs.drop_pack(Vec3::ZERO, 1, None, 0);
        assert!(
            packs
                .take(&[taker(1, None, Vec3::ZERO, &[Perk::Scavenger])])
                .is_empty()
        );
        assert_eq!(
            packs
                .take(&[taker(2, None, Vec3::ZERO, &[Perk::Scavenger])])
                .len(),
            1
        );
    }

    #[test]
    fn packs_expire_and_the_oldest_makes_room() {
        let mut packs = Packs::default();
        let (first, _) = packs.drop_pack(Vec3::ZERO, 1, None, 0);
        let life = (PACK_SECONDS * TICK_RATE as f32) as Tick;
        assert!(packs.expire(life - 1).is_empty());
        assert_eq!(packs.expire(life), vec![first]);
        for i in 0..MAX_PACKS {
            let (_, out) = packs.drop_pack(Vec3::ZERO, 1, None, i as Tick);
            assert!(out.is_none());
        }
        let (_, out) = packs.drop_pack(Vec3::ZERO, 1, None, 99);
        assert_eq!(out.map(|p| p.dropped), Some(0));
        assert_eq!(packs.all().len(), MAX_PACKS);
    }

    #[test]
    fn a_pack_gives_a_magazine_a_gun_and_a_grenade_of_each_up_to_the_start() {
        let m4 = WeaponId::expect("m4");
        let pistol = WeaponId::expect("m1911");
        let mut combat = CombatState::new([m4, pistol])
            .with_lethals(0)
            .with_tacticals(crate::equipment::Kind::Flash, 0);
        combat.slots[0].reserve = 0;
        combat.slots[1].reserve = 0;
        assert!(resupply(&mut combat, 1, 2));
        assert_eq!(
            combat.slots[0].reserve,
            m4.def().magazine.min(m4.def().reserve)
        );
        assert_eq!(
            combat.slots[1].reserve,
            pistol.def().magazine.min(pistol.def().reserve)
        );
        assert_eq!((combat.lethals, combat.tacticals), (1, 1));
        // Capped at what the class started with.
        assert!(resupply(&mut combat, 1, 2));
        assert_eq!((combat.lethals, combat.tacticals), (1, 2));
        for _ in 0..20 {
            resupply(&mut combat, 1, 2);
        }
        assert_eq!(combat.slots[0].reserve, m4.def().reserve);
        assert!(!resupply(&mut combat, 1, 2), "full: nothing to add");
    }
}
