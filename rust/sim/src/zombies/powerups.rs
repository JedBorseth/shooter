//! When kills drop power-ups, and which: Black Ops III's drop cycle.
//!
//! A drop is earned with points, not left to luck alone
//! (`_zm_powerups.gsc`, `watch_for_drop` and `powerup_drop`):
//!
//! - Once the players together have earned [`FIRST_DROP_POINTS`] more
//!   points, the next kill drops a power-up. Each time that happens the
//!   step grows by [`DROP_STEP_GROWTH`] (`zombie_powerup_drop_increment`,
//!   2000, times 1.14 each drop) and counts from the points earned so far.
//! - Any kill also has a [`RANDOM_DROP_PERCENT`]% chance to drop one anyway
//!   (`randomint(100) <= 2`).
//! - No more than [`MAX_DROPS_PER_ROUND`] drop in a round
//!   (`zombie_powerup_drop_max_per_round`).
//!
//! Which one comes from a shuffled deck of every kind, dealt in turn and
//! shuffled again once it runs out (`get_next_powerup`). A kind that cannot
//! drop now is passed over for the next card (`get_valid_powerup`): the
//! Carpenter while fewer than [`CARPENTER_MIN_WINDOWS`] windows are bare.

use super::PowerUp;
use crate::rng;

/// Points the players must earn, together, before the first earned drop
/// (`zombie_powerup_drop_increment`).
pub const FIRST_DROP_POINTS: f32 = 2000.0;
/// The step to the next earned drop grows by this each time
/// (`zombie_powerup_drop_increment * 1.14`).
pub const DROP_STEP_GROWTH: f32 = 1.14;
/// Chance in a hundred that any kill drops a power-up regardless.
pub const RANDOM_DROP_PERCENT: u32 = 3;
/// Most power-ups that drop in one round.
pub const MAX_DROPS_PER_ROUND: u32 = 4;
/// The Carpenter drops only once this many windows have lost every board
/// (`func_should_drop_carpenter`).
pub const CARPENTER_MIN_WINDOWS: usize = 5;
/// Points the Carpenter pays every player once every window is boarded
/// (`start_carpenter`).
pub const CARPENTER_POINTS: u32 = 200;
/// Seconds between boards the Carpenter puts back, nearest window first
/// (`wait(0.05)` and a network frame per board).
pub const CARPENTER_BOARD_SECONDS: f32 = 0.1;

/// Seconds a dropped power-up lies still before it starts to blink
/// (`powerup_timeout`'s `wait_time`).
pub const STEADY_SECONDS: f32 = 15.0;
/// Seconds it blinks before it is gone: 15 blinks of 0.5 s, 10 of 0.25 s,
/// and 15 of 0.1 s.
pub const BLINK_SECONDS: f32 = 11.5;
/// Seconds a dropped power-up waits to be picked up, all told.
pub const LIFETIME: f32 = STEADY_SECONDS + BLINK_SECONDS;

/// What the game looks like when a kill might drop a power-up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DropContext {
    /// Windows with no boards left.
    pub bare_windows: usize,
}

impl PowerUp {
    /// Whether this kind may come out of the deck now.
    pub fn can_drop(self, context: &DropContext) -> bool {
        match self {
            PowerUp::Carpenter => context.bare_windows >= CARPENTER_MIN_WINDOWS,
            _ => true,
        }
    }
}

/// The game's drop cycle: the points still to earn for the next drop, the
/// deck, and this round's count.
#[derive(Clone, Debug)]
pub struct DropCycle {
    seed: u32,
    deck: [PowerUp; PowerUp::ALL.len()],
    next: usize,
    shuffles: u32,
    step: f32,
    /// Points the players have earned together at which the next drop is
    /// earned.
    earn_at: f32,
    /// The next kill drops one (`zombie_drop_item`).
    earned: bool,
    /// Drops this round.
    round_drops: u32,
    /// Kills seen, for the random roll.
    kills: u32,
}

impl DropCycle {
    /// A new game's cycle; `seed` varies the deck and the random drops.
    pub fn new(seed: u32) -> Self {
        let mut cycle = Self {
            seed,
            deck: PowerUp::ALL,
            next: 0,
            shuffles: 0,
            step: FIRST_DROP_POINTS,
            earn_at: FIRST_DROP_POINTS,
            earned: false,
            round_drops: 0,
            kills: 0,
        };
        cycle.shuffle();
        cycle
    }

    /// A new round: up to [`MAX_DROPS_PER_ROUND`] again.
    pub fn round_started(&mut self) {
        self.round_drops = 0;
    }

    /// The players together have now earned `total` points this game.
    pub fn earned(&mut self, total: u32) {
        let total = total as f32;
        if total > self.earn_at {
            self.step *= DROP_STEP_GROWTH;
            self.earn_at = total + self.step;
            self.earned = true;
        }
    }

    /// Whether the next kill drops a power-up because it was earned.
    pub fn drop_earned(&self) -> bool {
        self.earned
    }

    /// A zombie that may drop one died: the power-up it drops, if any.
    pub fn on_kill(&mut self, context: &DropContext) -> Option<PowerUp> {
        let kill = self.kills;
        self.kills = self.kills.wrapping_add(1);
        if self.round_drops >= MAX_DROPS_PER_ROUND {
            return None;
        }
        let lucky = rng::hash(self.seed ^ 0x6472_6f70, kill) % 100 < RANDOM_DROP_PERCENT;
        if !lucky && !self.earned {
            return None;
        }
        let kind = self.deal(context)?;
        self.round_drops += 1;
        self.earned = false;
        Some(kind)
    }

    /// The next card that can drop now, passing over those that cannot.
    fn deal(&mut self, context: &DropContext) -> Option<PowerUp> {
        for _ in 0..2 * self.deck.len() {
            let kind = self.deck[self.next];
            self.next += 1;
            if self.next >= self.deck.len() {
                self.shuffle();
            }
            if kind.can_drop(context) {
                return Some(kind);
            }
        }
        None
    }

    fn shuffle(&mut self) {
        self.next = 0;
        let round = self.shuffles;
        self.shuffles = self.shuffles.wrapping_add(1);
        // Fisher-Yates from a hash of the seed and the shuffle count.
        for i in (1..self.deck.len()).rev() {
            let j = rng::hash(self.seed ^ round.wrapping_mul(0x9e37), i as u32) as usize % (i + 1);
            self.deck.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_can_drop() -> DropContext {
        DropContext { bare_windows: 8 }
    }

    #[test]
    fn earning_2000_points_drops_on_the_next_kill() {
        let mut cycle = DropCycle::new(1);
        let context = all_can_drop();
        cycle.earned(1999);
        assert!(!cycle.drop_earned());
        cycle.earned(2001);
        assert!(cycle.drop_earned());
        assert!(cycle.on_kill(&context).is_some());
        assert!(!cycle.drop_earned(), "one drop per threshold");
        // The next step is 14% longer, counted from the points earned then.
        cycle.earned(2001 + 2279);
        assert!(!cycle.drop_earned());
        cycle.earned(2001 + 2281);
        assert!(cycle.drop_earned());
    }

    #[test]
    fn about_three_kills_in_a_hundred_drop_regardless() {
        let mut cycle = DropCycle::new(7);
        let context = all_can_drop();
        let mut drops = 0;
        for kill in 0..10_000 {
            if kill % 100 == 0 {
                cycle.round_started();
            }
            if cycle.on_kill(&context).is_some() {
                drops += 1;
            }
        }
        assert!((220..=380).contains(&drops), "{drops} random drops");
    }

    #[test]
    fn four_a_round_at_most() {
        let mut cycle = DropCycle::new(3);
        let context = all_can_drop();
        let mut drops = 0;
        for i in 0..50 {
            cycle.earned(10_000 * (i + 1));
            if cycle.on_kill(&context).is_some() {
                drops += 1;
            }
        }
        assert_eq!(drops, MAX_DROPS_PER_ROUND);
        cycle.round_started();
        cycle.earned(10_000_000);
        assert!(cycle.on_kill(&context).is_some());
    }

    #[test]
    fn the_deck_deals_every_kind_before_repeating() {
        let mut cycle = DropCycle::new(11);
        let context = all_can_drop();
        let mut dealt = Vec::new();
        for _ in 0..PowerUp::ALL.len() {
            dealt.push(cycle.deal(&context).unwrap());
        }
        for kind in PowerUp::ALL {
            assert!(dealt.contains(&kind), "{kind:?} missing from {dealt:?}");
        }
    }

    #[test]
    fn the_carpenter_waits_for_five_bare_windows() {
        let mut cycle = DropCycle::new(5);
        let context = DropContext { bare_windows: 4 };
        for _ in 0..100 {
            assert_ne!(cycle.deal(&context), Some(PowerUp::Carpenter));
        }
        assert!(PowerUp::Carpenter.can_drop(&all_can_drop()));
    }

    #[test]
    fn a_drop_lies_for_26_and_a_half_seconds() {
        assert!((LIFETIME - 26.5).abs() < 1e-6);
    }
}
