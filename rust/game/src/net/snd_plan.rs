//! The Search and Destroy bots' plan for a round, kept by the server: each
//! attacker bot's role, whether the carrier has gone to plant, site switches,
//! and which site the defenders rotate to. The rules are engine free in
//! `shooter_sim::snd::bots`; this keeps their state from tick to tick and
//! feeds them what the bots have seen.

use shooter_sim::net::Tick;
use shooter_sim::snd::bots::{self, AttackRole};
use shooter_sim::snd::{RoundState, Site, SndRules};
use shooter_sim::tdm::Team;
use shooter_sim::{TICK_RATE, Vec3, rng};

/// What the bots of each side have seen: the enemies each team's bots saw
/// recently, one position per enemy (the latest).
pub struct Intel<'a> {
    pub seen_by_attackers: &'a [Vec3],
    pub seen_by_defenders: &'a [Vec3],
}

#[derive(Default)]
pub struct SndPlan {
    round: u32,
    seed: u32,
    carrier: Option<u8>,
    roles: Vec<(u8, AttackRole)>,
    /// The carrier has decided to go and plant.
    pub planting: bool,
    next_check: Tick,
    switches: u32,
    last_switch: Option<Tick>,
    /// Where players died this round, by team, and when.
    losses: Vec<(Team, Vec3, Tick)>,
    /// The site the defenders rotate to, if one is under attack.
    pub hot: Option<usize>,
}

impl SndPlan {
    /// A new round: forget the last one.
    pub fn start_round(&mut self, number: u32, tick: Tick) {
        *self = Self {
            round: number,
            seed: rng::hash(number, tick),
            next_check: tick,
            ..Self::default()
        };
    }

    /// A player of `team` died at `at`.
    pub fn on_death(&mut self, team: Team, at: Vec3, tick: Tick) {
        self.losses.push((team, at, tick));
    }

    /// The role of attacker bot `id` this round.
    pub fn role(&self, id: u8) -> AttackRole {
        self.roles
            .iter()
            .find(|(b, _)| *b == id)
            .map_or(AttackRole::Push, |(_, r)| *r)
    }

    /// Updates the plan for this tick. `attackers` are the attacking team's
    /// living bots; `carrier` holds the charge. Switches `target` (an index
    /// into `sites`) when the attackers see it held, and returns whether it
    /// did.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        tick: Tick,
        round: &RoundState,
        rules: &SndRules,
        sites: &[Site],
        attackers: &[u8],
        carrier: Option<u8>,
        intel: &Intel,
        target: &mut usize,
    ) -> bool {
        if round.number != self.round {
            self.start_round(round.number, tick);
        }
        let mut ids: Vec<u8> = attackers.to_vec();
        ids.sort_unstable();
        if carrier != self.carrier || self.roles.iter().map(|(id, _)| *id).ne(ids.iter().copied()) {
            self.carrier = carrier;
            self.roles = bots::attack_roles(&ids, carrier, self.seed);
        }
        if !round.live() || round.planted || sites.is_empty() {
            return false;
        }
        let window = |seconds: f32| (seconds * TICK_RATE as f32) as Tick;
        self.losses
            .retain(|(_, _, t)| tick.saturating_sub(*t) < window(bots::LOSS_SECONDS));
        let lost = |team_attacks: bool| -> Vec<Vec3> {
            self.losses
                .iter()
                .filter(|(team, _, _)| (*team == round.attackers) == team_attacks)
                .map(|(_, p, _)| *p)
                .collect()
        };
        let attacker_losses = lost(true);
        let defender_losses = lost(false);
        let attack_threats: Vec<u32> = sites
            .iter()
            .map(|s| bots::site_threat(s, intel.seen_by_attackers, &attacker_losses))
            .collect();
        let defence_threats: Vec<u32> = sites
            .iter()
            .map(|s| bots::site_threat(s, intel.seen_by_defenders, &defender_losses))
            .collect();
        self.hot = bots::hot_site(&defence_threats);

        let elapsed = 1.0 - (round.time_left / rules.round_time.max(1.0)).clamp(0.0, 1.0);
        // The carrier decides when to go and plant.
        if !self.planting && carrier.is_some() && tick >= self.next_check {
            let roll = rng::unit(self.seed, tick);
            self.planting = bots::plant_now(elapsed, roll);
            let after = bots::plant_check_after(rng::unit(self.seed, tick.wrapping_add(1)));
            self.next_check = tick + window(after);
        }

        // Switch sites when the target looks held.
        let cooled = self
            .last_switch
            .is_none_or(|t| tick.saturating_sub(t) >= window(bots::SWITCH_COOLDOWN));
        if self.switches < bots::MAX_SWITCHES
            && cooled
            && elapsed < bots::SWITCH_BEFORE
            && let Some(other) = bots::switch_site(*target % sites.len(), &attack_threats)
        {
            *target = other;
            self.switches += 1;
            self.last_switch = Some(tick);
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shooter_sim::snd::RoundPhase;

    fn sites() -> Vec<Site> {
        [-30.0, 30.0]
            .into_iter()
            .map(|x| Site {
                center: Vec3::new(x, 1.5, 0.0),
                half_extents: Vec3::new(4.0, 1.5, 4.0),
            })
            .collect()
    }

    fn live(time_left: f32) -> RoundState {
        RoundState {
            number: 1,
            phase: RoundPhase::Live,
            time_left,
            attackers: 0,
            planted: false,
        }
    }

    #[test]
    fn attackers_switch_away_from_a_held_site_once_per_cooldown() {
        let rules = SndRules::default();
        let sites = sites();
        let mut plan = SndPlan::default();
        let mut target = 0;
        let held = [Vec3::new(-28.0, 0.0, 0.0); 5];
        let intel = Intel {
            seen_by_attackers: &held,
            seen_by_defenders: &[],
        };
        let switched = plan.update(
            10,
            &live(90.0),
            &rules,
            &sites,
            &[200, 201, 202],
            Some(200),
            &intel,
            &mut target,
        );
        assert!(switched);
        assert_eq!(target, 1);
        // Held at B as well, straight after: no switching back yet.
        let held_b = [Vec3::new(28.0, 0.0, 0.0); 5];
        let intel = Intel {
            seen_by_attackers: &held_b,
            seen_by_defenders: &[],
        };
        let again = plan.update(
            11,
            &live(90.0),
            &rules,
            &sites,
            &[200, 201, 202],
            Some(200),
            &intel,
            &mut target,
        );
        assert!(!again);
        assert_eq!(target, 1);
    }

    #[test]
    fn the_carrier_always_goes_to_plant_late_in_the_round() {
        let rules = SndRules::default();
        let sites = sites();
        let mut plan = SndPlan::default();
        let mut target = 0;
        let intel = Intel {
            seen_by_attackers: &[],
            seen_by_defenders: &[],
        };
        let late = live(rules.round_time * 0.1);
        plan.update(
            5,
            &late,
            &rules,
            &sites,
            &[200],
            Some(200),
            &intel,
            &mut target,
        );
        assert!(plan.planting);
        assert_eq!(plan.role(200), AttackRole::Carrier);
    }

    #[test]
    fn defenders_learn_which_site_is_under_attack() {
        let rules = SndRules::default();
        let sites = sites();
        let mut plan = SndPlan::default();
        let mut target = 0;
        let pushing = [Vec3::new(30.0, 0.0, 6.0), Vec3::new(33.0, 0.0, -5.0)];
        let intel = Intel {
            seen_by_attackers: &[],
            seen_by_defenders: &pushing,
        };
        plan.update(
            5,
            &live(80.0),
            &rules,
            &sites,
            &[200],
            None,
            &intel,
            &mut target,
        );
        assert_eq!(plan.hot, Some(1));
    }
}
