//! A player's career stats: counters the server keeps for each player
//! through a match ([`Recorder`]), added to their totals at the end of it
//! (`crate::progression::Earned::stats`). Challenges (`crate::challenge`)
//! and the combat record read these totals.
//!
//! Black Ops 2 works the same way: the game server counts each player's
//! stats (`mp/gametypes/_persistence.gsc` writes `playerstatslist` and the
//! per-weapon `weaponstats`), and a challenge is a target on one of those
//! counters, so it completes when the counter crosses the target, in
//! whatever match that happens.
//!
//! Keys are short lowercase strings, at most [`MAX_KEY_LEN`] bytes of
//! `a-z`, `0-9`, `_`, and `.`, so they are safe to store and check:
//!
//! - career totals: `kills`, `deaths`, `headshots`, `shots`, `hits`,
//!   `score`, `time` (seconds played), `matches`, `wins`, `losses`,
//!   `draws`, `longshots`, `revenge`, `doubles` (double kills), `triples`
//!   (triple kills or more), `bloodthirsty` (5 kills without dying),
//!   `merciless` (10 kills without dying), `collateral`, `point_blank`,
//!   `aircraft` (enemy aircraft shot down);
//! - kills by other means: `knife.kills`, `<equipment>.kills` (`frag`);
//! - per weapon, by its plain key ([`weapon_key`]): `w.<key>.hs`
//!   (headshots), `.long` (longshots), `.bare` (kills with no
//!   attachments), `.noperk` (kills with no perks), `.double`, `.bt`
//!   (bloodthirsty), `.revenge`, `.ohk` (one-hit kills), `.shots`,
//!   `.hits`, `.deaths` (deaths while holding it). Kills with each weapon
//!   are `Earned::weapon_kills`, which unlocks attachments;
//! - objective awards: `a.<award>` ([`award_key`]);
//! - scorestreaks: `s.<streak>.earned`, `s.<streak>.kills`,
//!   `s.<streak>.downed` (enemy ones shot down);
//! - modes: `m.<mode>.played`, `m.<mode>.wins`.

use std::collections::BTreeMap;

use crate::net::protocol::KillCause;
use crate::progression::Outcome;
use crate::score::{Award, Scored};
use crate::streak::Streak;
use crate::weapon::WeaponId;

/// Stat key to count.
pub type Stats = BTreeMap<String, u32>;

/// Longest stat key.
pub const MAX_KEY_LEN: usize = 32;
/// Most keys one match may report (well above what one match touches).
pub const MAX_MATCH_KEYS: usize = 192;

/// Damage one hit must deal to count as a one-hit kill: a full health bar
/// (`PlayerState::max_health`).
pub const ONE_HIT_DAMAGE: f32 = 100.0;

/// The plain weapon key stats are kept under ("m4" for any M4A1 variant).
pub fn weapon_key(weapon: WeaponId) -> &'static str {
    weapon.plain().def().key.as_str()
}

/// Whether `key` is a well-formed stat key.
pub fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= MAX_KEY_LEN
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.')
}

/// The stat an objective or medal award counts toward, if any. Kills,
/// headshots, longshots, multi-kills, and streak medals are counted from the
/// kill itself (they need its weapon), so they have none here.
pub fn award_key(award: Award) -> Option<&'static str> {
    Some(match award {
        Award::FlagTaken => "flag_taken",
        Award::FlagCapture => "flag_capture",
        Award::FlagReturn => "flag_return",
        Award::CarrierKill => "carrier_kill",
        Award::HardpointCapture => "hp_capture",
        Award::HardpointSecure => "hp_secure",
        Award::HardpointDefend => "hp_defend",
        Award::HardpointAssault => "hp_assault",
        Award::ChargePlanted => "plant",
        Award::ChargeDefused => "defuse",
        Award::WireCutter => "wire_cutter",
        Award::Clutch => "clutch",
        Award::Ace => "ace",
        Award::DominationCapture => "dom_capture",
        Award::DominationDefend => "dom_defend",
        Award::DominationAssault => "dom_assault",
        Award::Kill
        | Award::Headshot
        | Award::Longshot
        | Award::PointBlank
        | Award::Collateral
        | Award::MultiKill(_)
        | Award::Streak(_)
        | Award::ShotDown(_) => return None,
    })
}

/// Adds `amount` to `key`, saturating.
pub fn bump(stats: &mut Stats, key: &str, amount: u32) {
    if amount == 0 {
        return;
    }
    let entry = stats.entry(key.to_owned()).or_default();
    *entry = entry.saturating_add(amount);
}

/// Adds every counter of `more` into `stats`.
pub fn merge(stats: &mut Stats, more: &Stats) {
    for (key, amount) in more {
        bump(stats, key, *amount);
    }
}

/// A kill a player made, as the server saw it.
#[derive(Clone, Copy, Debug)]
pub struct KillFacts {
    pub victim: u8,
    /// The weapon in the killer's hands (or the launcher that fired).
    pub weapon: WeaponId,
    pub cause: KillCause,
    pub headshot: bool,
    /// The killer's class carried no perks.
    pub perkless: bool,
}

/// One player's stats through one match.
#[derive(Clone, Debug, Default)]
pub struct Recorder {
    stats: Stats,
    /// Changed since [`Recorder::take_changed`] last said so.
    changed: bool,
    /// The weapon of the player's last kill, for the medals that follow it
    /// (longshot, double kill, bloodthirsty) in the same score event.
    last_weapon: Option<WeaponId>,
    /// Who killed the player last, until they get their revenge.
    avenge: Option<u8>,
    /// A victim the player's last hit took from full health to dead.
    one_hit: Option<u8>,
}

impl Recorder {
    pub fn stats(&self) -> &Stats {
        &self.stats
    }

    pub fn is_empty(&self) -> bool {
        self.stats.is_empty()
    }

    /// Starts a new match.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Whether anything changed since the last call.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    fn add(&mut self, key: &str, amount: u32) {
        if self.stats.len() >= MAX_MATCH_KEYS && !self.stats.contains_key(key) {
            return;
        }
        bump(&mut self.stats, key, amount);
        self.changed = true;
    }

    fn add_weapon(&mut self, weapon: WeaponId, stat: &str) {
        let key = format!("w.{}.{stat}", weapon_key(weapon));
        self.add(&key, 1);
    }

    /// The player fired a shot (a shotgun's blast is one).
    pub fn shot(&mut self, weapon: WeaponId) {
        self.add("shots", 1);
        self.add_weapon(weapon, "shots");
    }

    /// One of the player's shots hit an enemy for `damage` (once per victim
    /// per shot), with `weapon`; `killed` when it killed them.
    pub fn hit(&mut self, weapon: WeaponId, victim: u8, damage: f32, killed: bool) {
        self.add("hits", 1);
        self.add_weapon(weapon, "hits");
        if killed && damage >= ONE_HIT_DAMAGE {
            self.one_hit = Some(victim);
        }
    }

    /// The player killed someone.
    pub fn kill(&mut self, kill: KillFacts) {
        self.add("kills", 1);
        let revenge = self.avenge == Some(kill.victim);
        if revenge {
            self.avenge = None;
            self.add("revenge", 1);
        }
        let one_hit = self.one_hit.take() == Some(kill.victim);
        match kill.cause {
            KillCause::Weapon => {
                let weapon = kill.weapon;
                self.last_weapon = Some(weapon);
                if kill.headshot {
                    self.add("headshots", 1);
                    self.add_weapon(weapon, "hs");
                }
                if weapon.def().fitted.is_empty() {
                    self.add_weapon(weapon, "bare");
                }
                if kill.perkless {
                    self.add_weapon(weapon, "noperk");
                }
                if revenge {
                    self.add_weapon(weapon, "revenge");
                }
                if one_hit {
                    self.add_weapon(weapon, "ohk");
                }
            }
            KillCause::Knife => {
                self.last_weapon = None;
                self.add("knife.kills", 1);
            }
            KillCause::Equipment(kind) => {
                self.last_weapon = None;
                self.add(&format!("{}.kills", kind.key()), 1);
            }
            KillCause::Charge => {
                self.last_weapon = None;
                self.add("charge.kills", 1);
            }
            KillCause::Streak(streak) => {
                self.last_weapon = None;
                self.add(&format!("s.{}.kills", streak.key()), 1);
            }
        }
    }

    /// The player died to `killer` (themselves for a suicide) while holding
    /// `held`.
    pub fn died(&mut self, killer: u8, me: u8, held: WeaponId) {
        self.add("deaths", 1);
        self.add_weapon(held, "deaths");
        if killer != me {
            self.avenge = Some(killer);
        }
    }

    /// The awards one score event paid the player. Medals that follow a kill
    /// count for the weapon of that kill.
    pub fn awards(&mut self, awards: &[Scored]) {
        for scored in awards {
            self.add("score", scored.points);
            let weapon = self.last_weapon;
            match scored.award {
                Award::Longshot => {
                    self.add("longshots", 1);
                    if let Some(weapon) = weapon {
                        self.add_weapon(weapon, "long");
                    }
                }
                Award::MultiKill(2) => {
                    self.add("doubles", 1);
                    if let Some(weapon) = weapon {
                        self.add_weapon(weapon, "double");
                    }
                }
                Award::MultiKill(_) => self.add("triples", 1),
                Award::Streak(5) => {
                    self.add("bloodthirsty", 1);
                    if let Some(weapon) = weapon {
                        self.add_weapon(weapon, "bt");
                    }
                }
                Award::Streak(10) => self.add("merciless", 1),
                Award::Collateral => self.add("collateral", 1),
                Award::ShotDown(streak) => {
                    if streak.is_aircraft() {
                        self.add("aircraft", 1);
                    }
                    self.add(&format!("s.{}.downed", streak.key()), 1);
                }
                Award::PointBlank => self.add("point_blank", 1),
                award => {
                    if let Some(key) = award_key(award) {
                        self.add(&format!("a.{key}"), 1);
                    }
                }
            }
        }
    }

    /// The player earned a scorestreak (by score or from a crate).
    pub fn streak_earned(&mut self, streak: Streak) {
        self.add(&format!("s.{}.earned", streak.key()), 1);
    }

    /// The player played `seconds` of a match of `mode` (its key, "tdm"),
    /// which ended with them in it (`outcome`) or which they left.
    pub fn finish(&mut self, mode: &str, seconds: u32, outcome: Option<Outcome>) {
        self.add("time", seconds);
        let Some(outcome) = outcome else {
            return;
        };
        self.add("matches", 1);
        self.add(&format!("m.{mode}.played"), 1);
        match outcome {
            Outcome::Win => {
                self.add("wins", 1);
                self.add(&format!("m.{mode}.wins"), 1);
            }
            Outcome::Loss => self.add("losses", 1),
            Outcome::Draw => self.add("draws", 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::equipment;

    fn m4() -> WeaponId {
        WeaponId::expect("m4")
    }

    fn kill(victim: u8, weapon: WeaponId, headshot: bool) -> KillFacts {
        KillFacts {
            victim,
            weapon,
            cause: KillCause::Weapon,
            headshot,
            perkless: false,
        }
    }

    #[test]
    fn a_kill_counts_its_headshot_and_bare_weapon() {
        let mut r = Recorder::default();
        r.kill(kill(3, m4(), true));
        let variant = WeaponId::all()
            .find(|id| id.plain() == m4() && *id != m4())
            .expect("an M4A1 variant");
        r.kill(kill(4, variant, false));
        let s = r.stats();
        assert_eq!(s.get("kills"), Some(&2));
        assert_eq!(s.get("headshots"), Some(&1));
        assert_eq!(s.get("w.m4.hs"), Some(&1));
        // Only the plain M4A1 had no attachments.
        assert_eq!(s.get("w.m4.bare"), Some(&1));
        assert!(r.take_changed());
        assert!(!r.take_changed());
    }

    #[test]
    fn medals_count_for_the_weapon_of_the_kill_before_them() {
        let mut r = Recorder::default();
        r.kill(kill(3, m4(), false));
        r.awards(&[
            Scored {
                award: Award::Kill,
                points: 100,
            },
            Scored {
                award: Award::Longshot,
                points: 50,
            },
            Scored {
                award: Award::MultiKill(2),
                points: 50,
            },
            Scored {
                award: Award::Streak(5),
                points: 50,
            },
            Scored {
                award: Award::ChargePlanted,
                points: 200,
            },
        ]);
        let s = r.stats();
        assert_eq!(s.get("score"), Some(&450));
        assert_eq!(s.get("w.m4.long"), Some(&1));
        assert_eq!(s.get("w.m4.double"), Some(&1));
        assert_eq!(s.get("w.m4.bt"), Some(&1));
        assert_eq!(s.get("a.plant"), Some(&1));
        assert_eq!(s.get("bloodthirsty"), Some(&1));
    }

    #[test]
    fn revenge_is_killing_whoever_killed_you_last() {
        let mut r = Recorder::default();
        r.died(7, 1, m4());
        r.kill(kill(5, m4(), false));
        assert_eq!(r.stats().get("revenge"), None);
        r.kill(kill(7, m4(), false));
        assert_eq!(r.stats().get("revenge"), Some(&1));
        assert_eq!(r.stats().get("w.m4.revenge"), Some(&1));
        // Once only.
        r.kill(kill(7, m4(), false));
        assert_eq!(r.stats().get("revenge"), Some(&1));
        // A suicide sets no one to avenge.
        r.died(1, 1, m4());
        assert_eq!(r.stats().get("deaths"), Some(&2));
        assert_eq!(r.stats().get("w.m4.deaths"), Some(&2));
    }

    #[test]
    fn a_one_hit_kill_takes_a_full_bar_in_one_hit() {
        let mut r = Recorder::default();
        let m870 = WeaponId::expect("m870");
        r.shot(m870);
        r.hit(m870, 4, 120.0, true);
        r.kill(kill(4, m870, false));
        r.hit(m870, 5, 40.0, true);
        r.kill(kill(5, m870, false));
        let s = r.stats();
        assert_eq!(s.get("w.m870.ohk"), Some(&1));
        assert_eq!(s.get("w.m870.shots"), Some(&1));
        assert_eq!(s.get("w.m870.hits"), Some(&2));
    }

    #[test]
    fn other_kills_and_the_match_end() {
        let mut r = Recorder::default();
        r.kill(KillFacts {
            cause: KillCause::Equipment(equipment::Kind::Frag),
            ..kill(2, m4(), false)
        });
        r.kill(KillFacts {
            cause: KillCause::Streak(Streak::Airstrike),
            ..kill(3, m4(), false)
        });
        r.streak_earned(Streak::Uav);
        r.finish("tdm", 600, Some(Outcome::Win));
        r.finish("tdm", 30, None);
        let s = r.stats();
        assert_eq!(s.get("frag.kills"), Some(&1));
        assert_eq!(s.get("s.airstrike.kills"), Some(&1));
        assert_eq!(s.get("s.uav.earned"), Some(&1));
        assert_eq!(s.get("wins"), Some(&1));
        assert_eq!(s.get("matches"), Some(&1));
        assert_eq!(s.get("m.tdm.wins"), Some(&1));
        assert_eq!(s.get("time"), Some(&630));
        assert!(s.keys().all(|k| valid_key(k)));
    }

    #[test]
    fn keys_are_checked() {
        assert!(valid_key("w.m4.hs"));
        assert!(!valid_key("W.m4"));
        assert!(!valid_key(""));
        assert!(!valid_key(&"a".repeat(MAX_KEY_LEN + 1)));
        assert!(!valid_key("a b"));
    }
}
