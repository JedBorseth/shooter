//! Account checks on a dedicated server: a server run with accounts on
//! (`NetServer::accounts`, the `--accounts URL` option) admits only players
//! the account server vouches for, refuses classes and streaks they have not
//! unlocked, and reports each player's match XP and weapon kills to their
//! account itself (docs/online.md).
//!
//! The server never talks HTTP itself. `NetServer` emits `account_check`
//! with a joining player's session token; `godot/scripts/net/account_link.gd`
//! asks the account server and answers with `accept_account` or
//! `deny_account`. At the end of a match (or when a player leaves one)
//! `NetServer` emits `account_award`, which the link reports.
//!
//! Call of Duty's dedicated servers work this way: a player signs in to
//! play, the server reads their rank and unlocks from the platform's
//! servers, and the match's XP is written by the game server, never by the
//! player's game.

use std::collections::BTreeMap;

use godot::prelude::*;
use shooter_sim::net::{TICK_RATE, Tick};
use shooter_sim::progression::{self, Earned, Outcome};
use shooter_sim::weapon::WeaponId;

/// How long a joining player may wait on the account server.
pub const ACCOUNT_TIMEOUT_TICKS: Tick = 10 * TICK_RATE;

pub const NOT_SIGNED_IN: &str = "Sign in to play on this server (title screen, top right).";
pub const NO_ANSWER: &str = "The account server did not answer. Try again in a moment.";
pub const ALREADY_PLAYING: &str = "That account is already playing on this server.";

/// Where a connection stands with the account server.
#[derive(Clone, Debug, Default)]
pub enum AccountState {
    /// No check: the server runs without accounts, or nothing was asked yet.
    #[default]
    None,
    /// Waiting for the account server to vouch for the hello's token.
    Pending {
        serial: i64,
        name: String,
        since: Tick,
    },
    /// The account server answered: who this is and what they have earned.
    Verified { user_id: String, earned: Earned },
}

impl AccountState {
    pub fn user_id(&self) -> Option<&str> {
        match self {
            AccountState::Verified { user_id, .. } => Some(user_id),
            _ => None,
        }
    }

    pub fn earned(&self) -> Option<&Earned> {
        match self {
            AccountState::Verified { earned, .. } => Some(earned),
            _ => None,
        }
    }
}

/// One player's part of the current match, for their account: when they
/// started playing it and their kills with each weapon (by plain key).
#[derive(Clone, Debug, Default)]
pub struct MatchShare {
    pub playing_since: Option<Tick>,
    pub weapon_kills: BTreeMap<String, u32>,
}

impl MatchShare {
    pub fn start(&mut self, tick: Tick) {
        self.playing_since = Some(tick);
        self.weapon_kills.clear();
    }

    /// A kill with `weapon` (any variant counts for its plain weapon, as the
    /// game counts attachment unlocks).
    pub fn add_kill(&mut self, weapon: WeaponId) {
        let key = weapon.plain().def().key.clone();
        *self.weapon_kills.entry(key).or_default() += 1;
    }

    /// The XP the match pays: the score, plus the time bonus when the match
    /// ended with them in it (`outcome`), as the game's own
    /// `match_xp.gd` adds it. Leaving early keeps the score, without the
    /// bonus.
    pub fn xp(&self, score: u32, now: Tick, outcome: Option<Outcome>) -> u32 {
        let bonus = match (outcome, self.playing_since) {
            (Some(outcome), Some(since)) => {
                let seconds = now.saturating_sub(since) as f32 / TICK_RATE as f32;
                progression::match_bonus(seconds, outcome)
            }
            _ => 0,
        };
        score.saturating_add(bonus)
    }

    /// Whether there is anything to report.
    pub fn earned_anything(&self, score: u32) -> bool {
        score > 0 || !self.weapon_kills.is_empty()
    }
}

/// What the account server says a player has earned, from the link's
/// answer: total XP and a dictionary of plain weapon keys to kills.
pub fn earned_from(xp: i64, weapon_kills: &VarDictionary) -> Earned {
    let mut earned = Earned {
        xp: xp.clamp(0, i64::from(u32::MAX)) as u32,
        ..Earned::default()
    };
    for (key, count) in weapon_kills.iter_shared() {
        let (Ok(key), Ok(count)) = (key.try_to::<GString>(), count.try_to::<i64>()) else {
            continue;
        };
        earned
            .weapon_kills
            .insert(key.to_string(), count.clamp(0, i64::from(u32::MAX)) as u32);
    }
    earned
}

pub fn kills_dictionary(kills: &BTreeMap<String, u32>) -> VarDictionary {
    let mut d = VarDictionary::new();
    for (key, count) in kills {
        d.set(key.as_str(), i64::from(*count));
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finished_match_pays_the_time_bonus_and_leaving_does_not() {
        let mut share = MatchShare::default();
        share.start(0);
        let ten_minutes = 600 * TICK_RATE;
        assert_eq!(share.xp(2000, ten_minutes, Some(Outcome::Loss)), 3000);
        assert_eq!(share.xp(2000, ten_minutes, Some(Outcome::Win)), 3500);
        assert_eq!(share.xp(2000, ten_minutes, None), 2000);
        // Never started playing (joined during the results): score only.
        assert_eq!(
            MatchShare::default().xp(100, ten_minutes, Some(Outcome::Win)),
            100
        );
    }

    #[test]
    fn kills_count_for_the_plain_weapon() {
        let mut share = MatchShare::default();
        let plain = WeaponId::by_key("m4").expect("the M4A1");
        let variant = WeaponId::all()
            .find(|id| *id != plain && id.plain() == plain)
            .expect("an M4A1 variant");
        share.add_kill(plain);
        share.add_kill(variant);
        assert_eq!(share.weapon_kills.get("m4"), Some(&2));
        assert!(share.earned_anything(0));
    }
}
