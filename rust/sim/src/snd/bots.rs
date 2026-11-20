//! How Search and Destroy bots plan a round, engine free. The server
//! (`rust/game/src/net/server.rs`, `charge_objectives`) turns the plan into
//! each bot's objective.
//!
//! Black Ops 2's bots (`maps/mp/bots/_bot_sd.gsc`) are the reference:
//!
//! - The carrier does not run straight to a site. Every 2.5 to 5 seconds it
//!   decides to go and plant with a chance equal to the share of the round
//!   gone, and always once 85% of it is gone (`bot_sd_attacker`). Ours never
//!   has less than [`PLANT_FLOOR`] at each look.
//! - About three in ten of the other attackers protect the carrier: once it
//!   heads to plant, they take spots within 6.5 m of the plant spot.
//! - Defenders guard the sites and frag enemies within about 10 m of the
//!   nearest one (`bot_sd_grenade`).
//!
//! On top of that, as players do (and as the owner asked of this game's
//! bots): attackers switch sites when they see the target held, one attacker
//! lurks on the other site, and defenders rotate to a site under attack,
//! leaving an anchor on their own.

use glam::Vec3;

use super::Site;
use crate::rng;

/// Metres around a site where a sighting of an enemy or a teammate's death
/// counts as the site being held.
pub const WATCH_MARGIN: f32 = 8.0;
/// Seconds a sighting counts toward a site's threat.
pub const SIGHTING_SECONDS: f32 = 4.0;
/// Seconds a teammate's death counts toward a site's threat.
pub const LOSS_SECONDS: f32 = 20.0;
/// A teammate killed at a site counts as this many sightings.
pub const LOSS_WEIGHT: u32 = 2;
/// Threat at which attackers give up on their target site: several
/// defenders seen there, or a teammate lost there and more seen. Attackers
/// pushing a site always see someone; a site they have not looked at counts
/// as quiet, so the bar is high.
pub const SWITCH_THREAT: u32 = 5;
/// The other site must be at least this much quieter to switch to it.
pub const SWITCH_GAP: u32 = 3;
/// Most site switches in one round, and seconds between them.
pub const MAX_SWITCHES: u32 = 1;
pub const SWITCH_COOLDOWN: f32 = 15.0;
/// Share of the round after which the attackers commit to their site: a
/// late switch only runs the clock down.
pub const SWITCH_BEFORE: f32 = 0.6;
/// Threat at which defenders rotate to a site.
pub const ROTATE_THREAT: u32 = 2;
/// Seconds between the carrier's looks at whether to go and plant (Black
/// Ops 2: 2.5 to 5 s).
pub const PLANT_CHECK: (f32, f32) = (2.5, 5.0);
/// The least chance the carrier goes to plant at each look. Black Ops 2 has
/// none (the chance is the share of the round gone), which on the bench left
/// the team waiting short of the site until defenders found it: Warehouse's
/// attackers planted in 6 rounds of 28.
pub const PLANT_FLOOR: f32 = 0.3;
/// Share of the round after which the carrier always goes to plant.
pub const PLANT_FORCED: f32 = 0.85;
/// Chance an attacker other than the carrier protects it (Black Ops 2: 30%).
pub const ESCORT_CHANCE: f32 = 0.3;
/// How far from the plant spot escorts stand (Black Ops 2: 256 units).
pub const ESCORT_RADIUS: f32 = 6.5;
/// Attacker bots it takes for one of them to lurk.
pub const LURK_FROM: usize = 3;
/// How far short of the target site the carrier waits with the team before
/// it decides to plant, in metres.
pub const STAGING: f32 = 14.0;
/// How far around the carrier's waiting spot the pushers gather, in metres.
pub const GATHER_RADIUS: f32 = 5.0;

/// Whether `p` is within `margin` metres of `site` (horizontally, and
/// within a storey of its floor or ceiling).
pub fn near_site(site: &Site, p: Vec3, margin: f32) -> bool {
    let d = (p - site.center).abs();
    d.x <= site.half_extents.x + margin
        && d.z <= site.half_extents.z + margin
        && d.y <= site.half_extents.y + 4.0
}

/// How held `site` looks: enemies recently seen near it, plus
/// [`LOSS_WEIGHT`] for each teammate recently killed near it.
pub fn site_threat(site: &Site, sightings: &[Vec3], losses: &[Vec3]) -> u32 {
    let seen = sightings
        .iter()
        .filter(|p| near_site(site, **p, WATCH_MARGIN))
        .count() as u32;
    let lost = losses
        .iter()
        .filter(|p| near_site(site, **p, WATCH_MARGIN))
        .count() as u32;
    seen + lost * LOSS_WEIGHT
}

/// The site the attackers should switch to from `target`, if the target
/// looks held ([`SWITCH_THREAT`]) and another site is clearly quieter
/// ([`SWITCH_GAP`]).
pub fn switch_site(target: usize, threats: &[u32]) -> Option<usize> {
    let here = *threats.get(target)?;
    if here < SWITCH_THREAT {
        return None;
    }
    let (best, quiet) = threats
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != target)
        .min_by_key(|(_, t)| **t)?;
    (*quiet + SWITCH_GAP <= here).then_some(best)
}

/// The site the defenders should rotate to: the one most clearly under
/// attack, if any is ([`ROTATE_THREAT`]) and none ties it.
pub fn hot_site(threats: &[u32]) -> Option<usize> {
    let (hot, most) = threats.iter().enumerate().max_by_key(|(_, t)| **t)?;
    let tied = threats
        .iter()
        .enumerate()
        .any(|(i, t)| i != hot && t == most);
    (*most >= ROTATE_THREAT && !tied).then_some(hot)
}

/// Whether the carrier goes to plant now, `elapsed` (0 to 1) of the round
/// gone, with `roll` a uniform random number in `[0, 1)`.
pub fn plant_now(elapsed: f32, roll: f32) -> bool {
    elapsed > PLANT_FORCED || roll < elapsed.max(PLANT_FLOOR)
}

/// Seconds until the carrier next looks at whether to plant, for a uniform
/// random `roll`.
pub fn plant_check_after(roll: f32) -> f32 {
    PLANT_CHECK.0 + roll * (PLANT_CHECK.1 - PLANT_CHECK.0)
}

/// What an attacker bot does this round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttackRole {
    /// Carries the charge.
    Carrier,
    /// Protects the carrier, then its plant spot.
    Escort,
    /// Gathers with the carrier, then pushes into the target site to clear
    /// it as the carrier goes to plant.
    Push,
    /// Goes to the other site, to catch defenders rotating and to flank.
    Lurk,
}

/// Each attacker bot's role for the round. `attackers` are the attacking
/// team's bots; `carrier` holds the charge, if a bot does. With
/// [`LURK_FROM`] bots or more, one that is not the carrier lurks. Of the
/// rest, about [`ESCORT_CHANCE`] escort and the others push. `round_seed`
/// varies the picks from round to round.
pub fn attack_roles(
    attackers: &[u8],
    carrier: Option<u8>,
    round_seed: u32,
) -> Vec<(u8, AttackRole)> {
    let mut ids: Vec<u8> = attackers.to_vec();
    ids.sort_unstable();
    let others: Vec<u8> = ids
        .iter()
        .copied()
        .filter(|id| Some(*id) != carrier)
        .collect();
    let lurker = (ids.len() >= LURK_FROM && !others.is_empty())
        .then(|| others[rng::hash(round_seed, 41) as usize % others.len()]);
    ids.into_iter()
        .map(|id| {
            let role = if Some(id) == carrier {
                AttackRole::Carrier
            } else if Some(id) == lurker {
                AttackRole::Lurk
            } else if rng::unit(rng::hash(round_seed, u32::from(id)), 5) < ESCORT_CHANCE {
                AttackRole::Escort
            } else {
                AttackRole::Push
            };
            (id, role)
        })
        .collect()
}

/// Which defenders rotate to `hot`: `defenders` are each bot and the site
/// it guards, in a fixed order. Every defender guarding another site goes,
/// except the first on each site, who anchors it.
pub fn rotating(defenders: &[(u8, usize)], hot: usize) -> Vec<u8> {
    let mut anchored: Vec<usize> = Vec::new();
    let mut out = Vec::new();
    for (id, site) in defenders {
        if *site == hot {
            continue;
        }
        if anchored.contains(site) {
            out.push(*id);
        } else {
            anchored.push(*site);
        }
    }
    out
}

/// Enemies this close to a site (or the planted charge), in metres, are
/// worth a grenade (Black Ops 2: 384 units, `bot_sd_grenade`).
pub const GRENADE_REACH: f32 = 9.75;

/// Where the carrier waits before it decides to plant: [`STAGING`] metres
/// short of `site`'s centre on the way from `from` (the attackers' side),
/// on the site's floor.
pub fn staging_point(site: &Site, from: Vec3) -> Vec3 {
    let floor = site.center.y - site.half_extents.y;
    let flat = Vec3::new(from.x - site.center.x, 0.0, from.z - site.center.z);
    let length = flat.length();
    let reach = STAGING + site.half_extents.x.max(site.half_extents.z);
    let at = if length > reach {
        site.center + flat / length * reach
    } else {
        Vec3::new(from.x, site.center.y, from.z)
    };
    Vec3::new(at.x, floor, at.z)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(x: f32) -> Site {
        Site {
            center: Vec3::new(x, 1.5, 0.0),
            half_extents: Vec3::new(4.0, 1.5, 4.0),
        }
    }

    #[test]
    fn threat_counts_sightings_and_losses_near_a_site() {
        let a = site(0.0);
        let near = Vec3::new(9.0, 0.0, 2.0);
        let far = Vec3::new(30.0, 0.0, 0.0);
        let upstairs = Vec3::new(0.0, 9.0, 0.0);
        assert_eq!(site_threat(&a, &[near, far, upstairs], &[]), 1);
        assert_eq!(site_threat(&a, &[near], &[Vec3::ZERO]), 1 + LOSS_WEIGHT);
    }

    #[test]
    fn attackers_switch_only_from_a_held_site_to_a_quieter_one() {
        assert_eq!(switch_site(0, &[4, 0]), None, "not held enough");
        assert_eq!(switch_site(0, &[5, 0]), Some(1));
        assert_eq!(switch_site(0, &[5, 3]), None, "the other is nearly as busy");
        assert_eq!(switch_site(1, &[0, 6]), Some(0));
        assert_eq!(switch_site(5, &[0, 4]), None);
    }

    #[test]
    fn defenders_rotate_to_the_one_site_clearly_under_attack() {
        assert_eq!(hot_site(&[0, 1]), None);
        assert_eq!(hot_site(&[2, 0]), Some(0));
        assert_eq!(hot_site(&[2, 2]), None, "a tie says nothing");
        let defenders = [(1, 0), (2, 1), (3, 0), (4, 1), (5, 1)];
        // Site 1's first defender anchors; site 0's are already there.
        assert_eq!(rotating(&defenders, 0), vec![4, 5]);
    }

    #[test]
    fn the_carrier_plants_more_often_as_the_round_runs_down() {
        assert!(!plant_now(0.1, 0.5));
        assert!(plant_now(0.0, PLANT_FLOOR - 0.01), "early, now and then");
        assert!(plant_now(0.6, 0.5));
        assert!(plant_now(0.9, 0.99), "forced late");
        assert!(plant_check_after(0.0) >= PLANT_CHECK.0);
        assert!(plant_check_after(0.999) <= PLANT_CHECK.1);
    }

    #[test]
    fn one_attacker_lurks_and_about_a_third_escort() {
        let attackers = [200, 201, 202, 203, 204, 205];
        let mut lurks = 0;
        let mut escorts = 0;
        for round in 0..200 {
            let roles = attack_roles(&attackers, Some(202), round);
            assert_eq!(roles.len(), 6);
            assert!(roles.contains(&(202, AttackRole::Carrier)));
            lurks += roles.iter().filter(|(_, r)| *r == AttackRole::Lurk).count();
            escorts += roles
                .iter()
                .filter(|(_, r)| *r == AttackRole::Escort)
                .count();
        }
        assert_eq!(lurks, 200, "exactly one a round");
        let share = escorts as f32 / (200.0 * 4.0);
        assert!((0.2..0.4).contains(&share), "{share}");
        // Two bots: nobody lurks.
        let roles = attack_roles(&[200, 201], Some(200), 3);
        assert!(roles.iter().all(|(_, r)| *r != AttackRole::Lurk));
    }

    #[test]
    fn the_carrier_stages_short_of_the_site_on_its_side() {
        let a = site(0.0);
        let at = staging_point(&a, Vec3::new(-60.0, 0.0, 0.0));
        assert!((at.x + STAGING + 4.0).abs() < 1e-4, "{at}");
        assert_eq!(at.y, 0.0);
        // Already closer than that: stay put.
        let close = staging_point(&a, Vec3::new(-10.0, 0.0, 1.0));
        assert_eq!(close, Vec3::new(-10.0, 0.0, 1.0));
    }
}
