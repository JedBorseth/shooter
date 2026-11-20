//! Traps: an electric fence across a gate, bought with points, that kills
//! every zombie that walks into it for a while and then has to cool down.
//! Black Ops' electric trap under our own look.
//!
//! Numbers from Black Ops III's `_zm_traps.gsc` and `_zm_trap_electric.gsc`:
//! 1000 points (`zombie_cost`), on for 40 s (`_trap_duration`), then 60 s
//! to cool down (`_trap_cooldown_time`), and only with the power on. A
//! zombie in it dies within 1.25 s (`zombie_trap_death`'s
//! `wait(randomfloat(1.25))`), paying nobody. A player who walks into it
//! goes down, unless they have the health drink, which turns it into 50 of
//! their 250 health (`player_elec_damage`) every 2.5 s they stay (the
//! shock).

use glam::Vec3;

use crate::rng;

/// The live part of a fence, in its own frame: 8 m along x, 2.4 m up from
/// its foot, 1 m deep (`scripts/zombies_trap.gd`).
pub const LIVE_HALF_EXTENTS: Vec3 = Vec3::new(4.0, 1.2, 0.5);

/// Whether `local` (a point in the fence's frame, from its foot) is in the
/// live part.
pub fn inside(local: Vec3) -> bool {
    local.x.abs() <= LIVE_HALF_EXTENTS.x
        && (0.0..=2.0 * LIVE_HALF_EXTENTS.y).contains(&local.y)
        && local.z.abs() <= LIVE_HALF_EXTENTS.z
}

/// Points the fence costs to switch on.
pub const PRICE: u32 = 1000;
/// Seconds it stays on.
pub const ACTIVE_SECONDS: f32 = 40.0;
/// Seconds it cools down before it can be bought again.
pub const COOLDOWN_SECONDS: f32 = 60.0;
/// Longest a zombie in it takes to die (seconds).
pub const ZOMBIE_DEATH_MAX: f32 = 1.25;
/// Share of full health a shock takes from a player with Bulwark (50 of
/// 250), and seconds between shocks while they stay in it.
pub const BULWARK_SHOCK_SHARE: f32 = 0.2;
pub const SHOCK_SECONDS: f32 = 2.5;

/// What a trap is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrapState {
    /// Ready to buy.
    Idle,
    /// On, for `left` seconds more, bought by `by`.
    Active { by: u8, left: f32 },
    /// Cooling down for `left` seconds more.
    Cooling { left: f32 },
}

/// Why a player cannot switch a trap on now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    NoPower,
    /// On or cooling down.
    Busy,
    Points,
}

impl TrapState {
    /// Whether player `by` with `points` may switch it on now.
    pub fn can_buy(&self, points: u32, powered: bool) -> Result<(), Refusal> {
        if !powered {
            return Err(Refusal::NoPower);
        }
        if *self != TrapState::Idle {
            return Err(Refusal::Busy);
        }
        if points < PRICE {
            return Err(Refusal::Points);
        }
        Ok(())
    }

    /// Advances it `dt`; true when it changed phase.
    pub fn step(&mut self, dt: f32) -> bool {
        match self {
            TrapState::Idle => false,
            TrapState::Active { left, .. } => {
                *left -= dt;
                if *left > 0.0 {
                    return false;
                }
                *self = TrapState::Cooling {
                    left: COOLDOWN_SECONDS,
                };
                true
            }
            TrapState::Cooling { left } => {
                *left -= dt;
                if *left > 0.0 {
                    return false;
                }
                *self = TrapState::Idle;
                true
            }
        }
    }

    pub fn active(&self) -> bool {
        matches!(self, TrapState::Active { .. })
    }

    /// Seconds left in this phase, if it has an end.
    pub fn left(&self) -> Option<f32> {
        match self {
            TrapState::Idle => None,
            TrapState::Active { left, .. } | TrapState::Cooling { left } => Some(*left),
        }
    }
}

/// Seconds a zombie caught in the fence takes to die, from `seed` (its id
/// and when it walked in).
pub fn zombie_death_delay(seed: u32) -> f32 {
    rng::unit(seed, 0x7472_6170) * ZOMBIE_DEATH_MAX
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TICK_DT;

    #[test]
    fn it_needs_power_and_1000_and_runs_40_then_cools_60() {
        let mut trap = TrapState::Idle;
        assert_eq!(trap.can_buy(5000, false), Err(Refusal::NoPower));
        assert_eq!(trap.can_buy(999, true), Err(Refusal::Points));
        assert_eq!(trap.can_buy(1000, true), Ok(()));
        trap = TrapState::Active {
            by: 1,
            left: ACTIVE_SECONDS,
        };
        assert_eq!(trap.can_buy(5000, true), Err(Refusal::Busy));
        let mut t = 0.0;
        while trap.active() {
            trap.step(TICK_DT);
            t += TICK_DT;
        }
        assert!((t - 40.0).abs() < 0.02);
        assert!(matches!(trap, TrapState::Cooling { .. }));
        while trap != TrapState::Idle {
            trap.step(TICK_DT);
            t += TICK_DT;
        }
        assert!((t - 100.0).abs() < 0.05);
    }

    #[test]
    fn zombies_die_within_a_second_and_a_quarter() {
        for seed in 0..100 {
            let d = zombie_death_delay(seed);
            assert!((0.0..=ZOMBIE_DEATH_MAX).contains(&d));
        }
    }
}
