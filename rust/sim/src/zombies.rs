//! Zombies rules: rounds, horde size and toughness, how zombies move and
//! attack, the two-hit rule for players, and points.
//!
//! Engine-free and deterministic, like the rest of the crate: the Godot side
//! (`Zombie`, `ZombieDirector` in `rust/game`) moves bodies and runs hit tests,
//! and asks this module what should happen.

use glam::Vec3;

use crate::rng;
use crate::weapon::{CombatState, WeaponId};

pub mod chase;
pub mod drinks;
pub mod laststand;
pub mod mystery;

/// Tuning for a Zombies match.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZombieRules {
    /// Most zombies alive at once; the rest of the round waits its turn.
    pub max_alive: u32,
    /// Seconds between rounds.
    pub round_break: f32,
    /// Seconds between spawns in round 1. Later rounds spawn faster.
    pub spawn_interval: f32,
    /// The fastest spawn interval, reached in later rounds.
    pub min_spawn_interval: f32,
    /// Ground speeds (m/s). Every zombie walks straight at its target at one of them.
    pub walk_speed: f32,
    pub jog_speed: f32,
    pub run_speed: f32,
    /// A zombie stops and swings once its target is this close (metres, horizontal).
    pub attack_reach: f32,
    /// A swing still lands if the target is within this distance when it connects.
    pub attack_hit_reach: f32,
    /// Seconds from the start of a swing to the moment it connects.
    pub attack_windup: f32,
    /// Seconds from the start of one swing to the next.
    pub attack_cycle: f32,
}

impl Default for ZombieRules {
    fn default() -> Self {
        Self {
            max_alive: 24,
            round_break: 8.0,
            spawn_interval: 2.0,
            min_spawn_interval: 0.4,
            // Walk and jog stay under the player's 5 m/s walk, and runners
            // under their 7.4 m/s sprint, so a player can always get away.
            walk_speed: 1.4,
            jog_speed: 2.8,
            run_speed: 4.6,
            attack_reach: 1.3,
            attack_hit_reach: 1.9,
            attack_windup: 0.45,
            attack_cycle: 1.2,
        }
    }
}

/// Most players in a game of Zombies (Black Ops: four).
pub const MAX_PLAYERS: usize = 4;

/// The round from which a player coming back in has at least
/// [`RESPAWN_POINTS`]: Black Ops tops a returning player up to 1500 points
/// after round 6 (`spectator_respawn_player`).
pub const RESPAWN_POINTS_AFTER_ROUND: u32 = 6;
pub const RESPAWN_POINTS: u32 = 1500;

/// Points a player has when they come back in at the start of `round`.
pub fn respawn_points(round: u32, points: u32) -> u32 {
    if round > RESPAWN_POINTS_AFTER_ROUND {
        points.max(RESPAWN_POINTS)
    } else {
        points
    }
}

/// What taking a wall weapon costs: its price, or half for ammo of one
/// already carried (Black Ops).
pub fn pickup_cost(price: i64, carried: bool) -> u32 {
    let price = price.max(0) as u32;
    if carried { price / 2 } else { price }
}

/// Points each player starts a game with (Black Ops: `zombie_score_start`, 500).
pub const START_POINTS: u32 = 500;

/// The weapon every player starts with, and falls back on when downed.
pub const START_WEAPON: &str = "m1911";

/// What a player carries at the start of a Zombies game: the sidearm alone
/// and two frags (Black Ops).
pub fn start_combat() -> CombatState {
    CombatState::only(WeaponId::expect(START_WEAPON)).with_lethals(START_FRAGS)
}

/// Frag grenades at the start of a Zombies game (Black Ops rules).
pub const START_FRAGS: u8 = 2;
/// Frags given at the start of every later round, up to [`MAX_FRAGS`].
pub const FRAGS_PER_ROUND: u8 = 2;
/// Most frags a player carries in Zombies.
pub const MAX_FRAGS: u8 = 4;

/// Frags carried after a new round's restock, from `carried`.
pub fn restock_frags(carried: u8) -> u8 {
    carried
        .saturating_add(FRAGS_PER_ROUND)
        .min(MAX_FRAGS)
        .max(carried)
}

/// How many zombies a round sends: 6, 8, 13, 18, 24, then about five more each round.
pub fn zombies_in_round(round: u32) -> u32 {
    match round {
        0 => 0,
        1 => 6,
        2 => 8,
        3 => 13,
        4 => 18,
        5 => 24,
        _ => 24 + (round - 5) * 5,
    }
}

/// Zombie health in a round: 150 in round 1, 100 more each round through
/// round 9, then 10% more each round.
pub fn zombie_health(round: u32) -> f32 {
    let round = round.max(1);
    if round < 10 {
        return 150.0 + 100.0 * (round - 1) as f32;
    }
    let mut health = 950.0;
    for _ in 10..=round {
        health *= 1.1;
    }
    health
}

/// Seconds between spawns in a round.
pub fn spawn_interval(rules: &ZombieRules, round: u32) -> f32 {
    // Repeated multiplication rather than `powi`, which may differ by platform.
    let mut interval = rules.spawn_interval;
    for _ in 1..round.min(60) {
        interval *= 0.95;
    }
    interval.max(rules.min_spawn_interval)
}

/// How fast a zombie moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gait {
    Walk,
    Jog,
    Run,
}

impl Gait {
    /// The gait of the `index`th zombie of a round. Round 1 to 3 zombies all
    /// walk; joggers appear from round 4 and runners from round 7, and more of
    /// the horde speeds up each round. Hashed, so the same round always plays
    /// out the same way.
    pub fn for_zombie(round: u32, index: u32) -> Self {
        let roll = rng::unit(round.wrapping_mul(7919), index);
        let run_share = (round.saturating_sub(6) as f32 * 0.12).min(0.7);
        let jog_share = (round.saturating_sub(3) as f32 * 0.2).min(1.0 - run_share);
        if roll < run_share {
            Gait::Run
        } else if roll < run_share + jog_share {
            Gait::Jog
        } else {
            Gait::Walk
        }
    }

    pub fn speed(self, rules: &ZombieRules) -> f32 {
        match self {
            Gait::Walk => rules.walk_speed,
            Gait::Jog => rules.jog_speed,
            Gait::Run => rules.run_speed,
        }
    }
}

/// The horizontal direction from `from` straight to `to`, or zero when they
/// stand on the same spot. Zombies never path around anything: they walk this
/// line, and the physics slides them along whatever is in the way.
pub fn chase_direction(from: Vec3, to: Vec3) -> Vec3 {
    let flat = Vec3::new(to.x - from.x, 0.0, to.z - from.z);
    flat.try_normalize().unwrap_or(Vec3::ZERO)
}

/// Horizontal distance, ignoring height.
pub fn flat_distance(a: Vec3, b: Vec3) -> f32 {
    Vec3::new(b.x - a.x, 0.0, b.z - a.z).length()
}

/// Damage a zombie's swing deals to a player.
///
/// Players go down on the second hit: a hit at full health takes them to half,
/// and any hit before health has fully regenerated is lethal.
pub fn hit_damage(health: f32, max_health: f32) -> f32 {
    if health >= max_health {
        max_health * 0.5
    } else {
        health.max(0.0)
    }
}

/// A zombie's swing, from the moment it is in reach.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Attack {
    /// Seconds into the current swing, or None between swings.
    swing: Option<f32>,
    /// Seconds until another swing may start.
    cooldown: f32,
}

/// What a zombie's attack did this tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttackEvent {
    /// A swing began (play the attack animation).
    Swung,
    /// The swing connected with a target still in reach.
    Hit,
    /// The swing connected with nothing: the target got away.
    Missed,
}

impl Attack {
    /// True while a swing is under way; the zombie stands still for it.
    pub fn swinging(&self) -> bool {
        self.swing.is_some()
    }

    /// Advances the attack. `distance` is the horizontal distance to the
    /// target, or None when there is no living target.
    pub fn step(
        &mut self,
        rules: &ZombieRules,
        distance: Option<f32>,
        dt: f32,
    ) -> Option<AttackEvent> {
        self.cooldown = (self.cooldown - dt).max(0.0);
        if let Some(t) = self.swing {
            let t = t + dt;
            if t >= rules.attack_windup {
                self.swing = None;
                return Some(match distance {
                    Some(d) if d <= rules.attack_hit_reach => AttackEvent::Hit,
                    _ => AttackEvent::Missed,
                });
            }
            self.swing = Some(t);
            return None;
        }
        match distance {
            Some(d) if d <= rules.attack_reach && self.cooldown <= 0.0 => {
                self.swing = Some(0.0);
                self.cooldown = rules.attack_cycle;
                Some(AttackEvent::Swung)
            }
            _ => None,
        }
    }
}

/// Points for damaging zombies.
pub mod points {
    /// A hit that does not kill.
    pub const HIT: u32 = 10;
    /// A kill.
    pub const KILL: u32 = 60;
    /// A kill with a headshot.
    pub const HEADSHOT_KILL: u32 = 100;
    /// A kill with the knife: the most a zombie pays.
    pub const MELEE_KILL: u32 = 130;

    pub fn for_hit(killed: bool, headshot: bool, melee: bool) -> u32 {
        match (killed, headshot, melee) {
            (false, ..) => HIT,
            (true, _, true) => MELEE_KILL,
            (true, false, false) => KILL,
            (true, true, false) => HEADSHOT_KILL,
        }
    }
}

/// Drops from killed zombies, as in the genre: each one helps for a moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PowerUp {
    /// Fills every weapon's ammo.
    MaxAmmo,
    /// Any hit kills, for [`POWER_UP_SECONDS`].
    InstaKill,
    /// Points are doubled, for [`POWER_UP_SECONDS`].
    DoublePoints,
    /// Kills every zombie in play and pays [`NUKE_POINTS`].
    Nuke,
}

/// Seconds Insta-Kill and Double Points last.
pub const POWER_UP_SECONDS: f32 = 30.0;
/// Seconds a dropped power-up waits to be picked up.
pub const POWER_UP_LIFETIME: f32 = 30.0;
/// A player picks up a dropped power-up within this many metres (flat) of it.
pub const PICKUP_RADIUS: f32 = 1.6;
/// Points each board put back on a window pays (Black Ops: 10).
pub const REBUILD_POINTS: u32 = 10;
/// Most points rebuilding pays one player in a round.
pub const MAX_REBUILD_POINTS: u32 = 300;
/// Seconds per board while the use button is held at a window.
pub const REBUILD_SECONDS: f32 = 0.6;
/// Points a Nuke pays (the zombies it kills pay nothing themselves).
pub const NUKE_POINTS: u32 = 400;
/// Most power-ups that drop in one round.
pub const MAX_DROPS_PER_ROUND: u32 = 4;
/// Chance that a kill drops a power-up, while the round has drops left.
pub const DROP_CHANCE: f32 = 0.035;

impl PowerUp {
    pub const ALL: [PowerUp; 4] = [
        PowerUp::MaxAmmo,
        PowerUp::InstaKill,
        PowerUp::DoublePoints,
        PowerUp::Nuke,
    ];

    /// Its number on the wire.
    pub fn code(self) -> u32 {
        PowerUp::ALL.iter().position(|p| *p == self).unwrap_or(0) as u32
    }

    pub fn from_code(code: u32) -> Option<PowerUp> {
        PowerUp::ALL.get(code as usize).copied()
    }

    /// The name the HUD shows.
    pub fn label(self) -> &'static str {
        match self {
            PowerUp::MaxAmmo => "MAX AMMO",
            PowerUp::InstaKill => "INSTA-KILL",
            PowerUp::DoublePoints => "DOUBLE POINTS",
            PowerUp::Nuke => "KABOOM",
        }
    }

    /// Lower-case key for scripts.
    pub fn key(self) -> &'static str {
        match self {
            PowerUp::MaxAmmo => "max_ammo",
            PowerUp::InstaKill => "insta_kill",
            PowerUp::DoublePoints => "double_points",
            PowerUp::Nuke => "nuke",
        }
    }

    pub fn from_key(key: &str) -> Option<PowerUp> {
        PowerUp::ALL.into_iter().find(|p| p.key() == key)
    }

    /// Whether it lasts a while instead of acting once.
    pub fn timed(self) -> bool {
        matches!(self, PowerUp::InstaKill | PowerUp::DoublePoints)
    }
}

/// The power-up the `kill`th kill of `round` drops, if any. `dropped` is how
/// many this round has dropped already. Deterministic: the same match drops
/// the same way.
pub fn power_up_drop(round: u32, kill: u32, dropped: u32) -> Option<PowerUp> {
    if dropped >= MAX_DROPS_PER_ROUND || rng::unit(round.wrapping_mul(7919), kill) >= DROP_CHANCE {
        return None;
    }
    let pick = rng::hash(round, kill.wrapping_add(31)) as usize % PowerUp::ALL.len();
    Some(PowerUp::ALL[pick])
}

/// Where a round is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Counting down to the round in `Rounds::round`.
    Break,
    /// Zombies are coming.
    Fighting,
    /// Every player is down; nothing more happens.
    Over,
}

/// What the round system wants done this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RoundEvent {
    /// Round `round` begins.
    RoundStarted { round: u32 },
    /// Spawn the `index`th zombie of the round (from 0), with this health and gait.
    Spawn { index: u32, health: f32, gait: Gait },
    /// Every zombie of round `round` is dead; a break follows.
    RoundCleared { round: u32 },
}

/// The round loop: a break, then a round's zombies spawn over time up to the
/// alive cap, and the round ends when the last one dies.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rounds {
    pub round: u32,
    pub phase: Phase,
    /// Zombies of this round spawned so far.
    pub spawned: u32,
    /// Zombies of this round killed so far.
    pub killed: u32,
    /// Seconds until the break ends or the next spawn.
    timer: f32,
}

impl Rounds {
    /// A new match, a short countdown before round 1.
    pub fn new(first_break: f32) -> Self {
        Self {
            round: 1,
            phase: Phase::Break,
            spawned: 0,
            killed: 0,
            timer: first_break,
        }
    }

    /// Zombies of this round still to kill, spawned or not.
    pub fn remaining(&self) -> u32 {
        zombies_in_round(self.round).saturating_sub(self.killed)
    }

    /// Zombies alive right now.
    pub fn alive(&self) -> u32 {
        self.spawned.saturating_sub(self.killed)
    }

    /// Seconds left in the break, or 0 outside one.
    pub fn break_left(&self) -> f32 {
        if self.phase == Phase::Break {
            self.timer.max(0.0)
        } else {
            0.0
        }
    }

    /// Records a zombie's death.
    pub fn on_killed(&mut self) {
        if self.killed < self.spawned {
            self.killed += 1;
        }
    }

    /// Ends the match: every player is down.
    pub fn end(&mut self) {
        self.phase = Phase::Over;
    }

    pub fn step(&mut self, rules: &ZombieRules, dt: f32, events: &mut Vec<RoundEvent>) {
        match self.phase {
            Phase::Over => {}
            Phase::Break => {
                self.timer -= dt;
                if self.timer <= 0.0 {
                    self.phase = Phase::Fighting;
                    self.spawned = 0;
                    self.killed = 0;
                    self.timer = 0.0;
                    events.push(RoundEvent::RoundStarted { round: self.round });
                }
            }
            Phase::Fighting => {
                let total = zombies_in_round(self.round);
                if self.killed >= total {
                    events.push(RoundEvent::RoundCleared { round: self.round });
                    self.round += 1;
                    self.phase = Phase::Break;
                    self.timer = rules.round_break;
                    return;
                }
                self.timer -= dt;
                if self.timer <= 0.0 && self.spawned < total && self.alive() < rules.max_alive {
                    events.push(RoundEvent::Spawn {
                        index: self.spawned,
                        health: zombie_health(self.round),
                        gait: Gait::for_zombie(self.round, self.spawned),
                    });
                    self.spawned += 1;
                    self.timer = spawn_interval(rules, self.round);
                }
            }
        }
    }
}

#[cfg(test)]
mod power_up_tests {
    use super::*;

    #[test]
    fn drops_are_rare_capped_and_varied() {
        let mut total = 0;
        let mut kinds = std::collections::HashSet::new();
        for round in 1..=50 {
            let mut dropped = 0;
            for kill in 0..zombies_in_round(round) {
                if let Some(power_up) = power_up_drop(round, kill, dropped) {
                    dropped += 1;
                    kinds.insert(power_up);
                }
            }
            assert!(dropped <= MAX_DROPS_PER_ROUND);
            total += dropped;
        }
        // About 3.5% of some 6,000 kills, capped per round.
        assert!((100..=220).contains(&total), "{total} drops");
        assert_eq!(kinds.len(), 4);
        assert_eq!(power_up_drop(3, 5, 1), power_up_drop(3, 5, 1));
    }

    #[test]
    fn power_ups_round_trip_their_keys() {
        for p in PowerUp::ALL {
            assert_eq!(PowerUp::from_key(p.key()), Some(p));
        }
        assert!(PowerUp::InstaKill.timed() && !PowerUp::Nuke.timed());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returning_players_get_at_least_1500_after_round_six() {
        assert_eq!(respawn_points(6, 200), 200);
        assert_eq!(respawn_points(7, 200), 1500);
        assert_eq!(respawn_points(9, 4000), 4000);
        assert_eq!(pickup_cost(1200, false), 1200);
        assert_eq!(pickup_cost(1200, true), 600);
    }

    #[test]
    fn frags_restock_two_a_round_up_to_four() {
        assert_eq!(restock_frags(0), 2);
        assert_eq!(restock_frags(1), 3);
        assert_eq!(restock_frags(START_FRAGS), 4);
        assert_eq!(restock_frags(4), 4);
        assert_eq!(restock_frags(9), 9, "never takes any away");
    }
    use crate::TICK_DT;

    #[test]
    fn two_hits_down_a_player_from_full_health() {
        let max = 100.0;
        let mut health = max;
        health -= hit_damage(health, max);
        assert!(health > 0.0, "the first hit must not kill");
        health -= hit_damage(health, max);
        assert!(health <= 0.0, "the second hit must kill");
    }

    #[test]
    fn a_hit_during_regeneration_is_still_lethal() {
        // Partly regenerated: still the second hit.
        assert_eq!(hit_damage(95.0, 100.0), 95.0);
        // Fully regenerated: back to two hits.
        assert_eq!(hit_damage(100.0, 100.0), 50.0);
        // Any max health.
        assert_eq!(hit_damage(250.0, 250.0), 125.0);
    }

    #[test]
    fn chase_direction_points_straight_at_the_target_on_the_ground() {
        let d = chase_direction(Vec3::new(0.0, 0.0, 0.0), Vec3::new(3.0, 5.0, -4.0));
        assert!((d - Vec3::new(0.6, 0.0, -0.8)).length() < 1e-6);
        assert_eq!(
            chase_direction(Vec3::ONE, Vec3::new(1.0, 9.0, 1.0)),
            Vec3::ZERO
        );
    }

    #[test]
    fn walking_straight_closes_the_distance_every_tick() {
        let rules = ZombieRules::default();
        let target = Vec3::new(10.0, 0.0, -7.0);
        let mut zombie = Vec3::new(-12.0, 0.0, 9.0);
        let mut last = flat_distance(zombie, target);
        while flat_distance(zombie, target) > rules.attack_reach {
            zombie += chase_direction(zombie, target) * rules.walk_speed * TICK_DT;
            let now = flat_distance(zombie, target);
            assert!(now < last);
            last = now;
        }
    }

    #[test]
    fn a_swing_hits_after_the_windup_and_misses_if_the_target_left() {
        let rules = ZombieRules::default();
        let mut attack = Attack::default();
        assert_eq!(
            attack.step(&rules, Some(3.0), TICK_DT),
            None,
            "out of reach"
        );
        assert_eq!(
            attack.step(&rules, Some(1.0), TICK_DT),
            Some(AttackEvent::Swung)
        );
        assert!(attack.swinging());
        let mut t = 0.0;
        let event = loop {
            t += TICK_DT;
            if let Some(e) = attack.step(&rules, Some(1.0), TICK_DT) {
                break e;
            }
        };
        assert_eq!(event, AttackEvent::Hit);
        assert!((t - rules.attack_windup).abs() < 2.0 * TICK_DT);

        // The next swing waits for the cycle, and misses if the player backs off.
        let mut swung = false;
        let mut result = None;
        for _ in 0..(3.0 / TICK_DT) as usize {
            let distance = if swung { 4.0 } else { 1.0 };
            match attack.step(&rules, Some(distance), TICK_DT) {
                Some(AttackEvent::Swung) => swung = true,
                Some(e) => {
                    result = Some(e);
                    break;
                }
                None => {}
            }
        }
        assert_eq!(result, Some(AttackEvent::Missed));
    }

    #[test]
    fn horde_grows_and_toughens() {
        assert_eq!(zombies_in_round(1), 6);
        assert!(zombies_in_round(10) > zombies_in_round(5));
        assert_eq!(zombie_health(1), 150.0);
        assert_eq!(zombie_health(9), 950.0);
        assert!((zombie_health(10) - 1045.0).abs() < 0.01);
        let rules = ZombieRules::default();
        assert!(spawn_interval(&rules, 20) < spawn_interval(&rules, 1));
        assert!(spawn_interval(&rules, 200) >= rules.min_spawn_interval);
    }

    #[test]
    fn early_rounds_only_walk_and_later_rounds_speed_up() {
        for i in 0..30 {
            assert_eq!(Gait::for_zombie(1, i), Gait::Walk);
            assert_eq!(Gait::for_zombie(3, i), Gait::Walk);
        }
        let fast = |round| {
            (0..200)
                .filter(|i| Gait::for_zombie(round, *i) != Gait::Walk)
                .count()
        };
        assert!(fast(5) > 0);
        assert!(fast(12) > fast(5));
        assert!((0..200).any(|i| Gait::for_zombie(12, i) == Gait::Run));
        assert_eq!(
            Gait::for_zombie(9, 4),
            Gait::for_zombie(9, 4),
            "deterministic"
        );
    }

    #[test]
    fn points_reward_kills_and_headshots() {
        assert_eq!(points::for_hit(false, true, false), points::HIT);
        assert_eq!(points::for_hit(true, false, false), points::KILL);
        assert_eq!(points::for_hit(true, true, false), points::HEADSHOT_KILL);
        assert_eq!(points::for_hit(false, false, true), points::HIT);
        assert_eq!(points::for_hit(true, false, true), points::MELEE_KILL);
    }

    #[test]
    fn a_round_spawns_its_horde_respects_the_cap_and_ends_when_cleared() {
        let rules = ZombieRules {
            max_alive: 4,
            ..ZombieRules::default()
        };
        let mut rounds = Rounds::new(1.0);
        let mut events = Vec::new();
        let mut spawned = 0;
        let mut started = false;
        for _ in 0..(60.0 / TICK_DT) as usize {
            events.clear();
            rounds.step(&rules, TICK_DT, &mut events);
            for e in &events {
                match e {
                    RoundEvent::RoundStarted { round: 1 } => started = true,
                    RoundEvent::Spawn { health, gait, .. } => {
                        assert_eq!(*health, 150.0);
                        assert_eq!(*gait, Gait::Walk);
                        spawned += 1;
                    }
                    _ => {}
                }
            }
            assert!(rounds.alive() <= rules.max_alive);
        }
        assert!(started);
        assert_eq!(spawned, 4, "capped at max_alive until some die");
        assert_eq!(rounds.remaining(), 6);

        // Kill everything as it spawns: the round clears and round 2 follows the break.
        let mut cleared = false;
        let mut round_two = false;
        for _ in 0..(120.0 / TICK_DT) as usize {
            while rounds.alive() > 0 {
                rounds.on_killed();
            }
            events.clear();
            rounds.step(&rules, TICK_DT, &mut events);
            for e in &events {
                match e {
                    RoundEvent::RoundCleared { round: 1 } => cleared = true,
                    RoundEvent::RoundStarted { round: 2 } => round_two = true,
                    _ => {}
                }
            }
            if round_two {
                break;
            }
        }
        assert!(cleared && round_two);
        assert_eq!(rounds.round, 2);
        assert_eq!(rounds.remaining(), 8);
    }

    #[test]
    fn nothing_happens_once_the_match_is_over() {
        let rules = ZombieRules::default();
        let mut rounds = Rounds::new(0.0);
        rounds.end();
        let mut events = Vec::new();
        for _ in 0..1000 {
            rounds.step(&rules, TICK_DT, &mut events);
        }
        assert!(events.is_empty());
    }
}
