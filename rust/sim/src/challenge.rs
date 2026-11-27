//! Challenges: targets on a player's career stats (`crate::stats`) that pay
//! XP once and may award a camo or a calling card. A challenge completes in
//! the match its counter crosses the target; completion is never stored,
//! only worked out from the totals.
//!
//! Black Ops 2's challenges, with our numbers where its own are unknown:
//!
//! - **Weapon challenges** (the camo ladder). Each weapon has fourteen,
//!   each earning a camo, in Black Ops 2's order for its class: a headshot
//!   ladder (5, 10, 15, 20, 30, 40, 50, 75, 100), then 10 longshots, 150
//!   kills with no attachments, 150 kills with no perks, 20 double kills,
//!   and 10 times five kills without dying. Shotguns and sniper rifles climb
//!   one-hit kills instead of headshots, and SMGs, LMGs, and pistols take
//!   30 revenge kills in place of longshots, as in Black Ops 2 (camo guides
//!   at games.gg and gamerant.com, "Black Ops 2 camo challenges"). Doing all
//!   fourteen earns gold; gold on every weapon of a class earns the
//!   class's mastery camo (Black Ops 2's Diamond); mastery in every class
//!   earns the final camo (Black Ops 3's Dark Matter).
//! - **Career, mode, and scorestreak challenges** in tiers, the last tier
//!   of most awarding a calling card, as Black Ops 2's do.
//!
//! XP: Black Ops 2's challenges pay 250 to 5,000 XP (Call of Duty wiki,
//! "Challenges/Call of Duty: Black Ops II"), and a weapon's whole ladder
//! comes to about 1.4% of the XP to its top level. Ours keep that share of
//! our shorter curve: a weapon's ladder pays 9,000 XP of the 604,800 to
//! level 55.

use std::sync::LazyLock;

use crate::camo::{self, Camo};
use crate::card::Card;
use crate::progression::Earned;
use crate::stats::Stats;
use crate::streak::Streak;
use crate::weapon::{WeaponClass, WeaponId};

/// Where a challenge is listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    /// A weapon's camo ladder and gold (by its plain weapon).
    Weapon(WeaponId),
    /// A weapon class's mastery, and the final camo.
    Mastery,
    Career,
    /// A mode, by its key ("tdm").
    Mode(&'static str),
    Scorestreak(Streak),
}

/// What a challenge counts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Goal {
    /// A stat key.
    Stat(String),
    /// Kills with a weapon (its plain key), kept apart from the other stats
    /// because attachments unlock by it.
    WeaponKills(String),
    /// The weapon's fourteen ladder challenges done.
    Gold(WeaponId),
    /// Gold on every weapon of the class.
    Mastery(WeaponClass),
    /// Mastery in every class.
    Final,
}

/// What finishing a challenge awards besides its XP.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reward {
    None,
    /// A camo for the challenge's weapon, or every weapon of its class.
    Camo(Camo),
    Card(Card),
}

#[derive(Clone, Debug)]
pub struct Challenge {
    /// Stable id ("w.m4.0", "career.kills.2").
    pub id: String,
    pub group: Group,
    pub name: String,
    /// One line on what to do.
    pub detail: String,
    pub goal: Goal,
    pub target: u32,
    pub xp: u32,
    pub reward: Reward,
}

/// Every challenge, weapons first (in Create-a-Class order), then mastery,
/// career, modes, and scorestreaks.
pub fn all() -> &'static [Challenge] {
    &ALL
}

pub fn by_id(id: &str) -> Option<&'static Challenge> {
    ALL.iter().find(|c| c.id == id)
}

/// The plain weapons that have a camo ladder, in Create-a-Class order.
pub fn ladder_weapons() -> Vec<WeaponId> {
    let mut weapons: Vec<WeaponId> = WeaponId::all().filter(|w| w.def().plain()).collect();
    weapons.sort_by_key(|w| (w.def().class.order(), w.def().unlock_level, w.0));
    weapons
}

/// The weapon classes with a mastery camo: those with ladder weapons.
pub fn mastery_classes() -> Vec<WeaponClass> {
    let weapons = ladder_weapons();
    WeaponClass::ALL
        .into_iter()
        .filter(|class| weapons.iter().any(|w| w.def().class == *class))
        .collect()
}

/// How far `earned` is toward `challenge` (at most its target).
pub fn progress(earned: &Earned, challenge: &Challenge) -> u32 {
    count(earned, &challenge.goal).min(challenge.target)
}

pub fn done(earned: &Earned, challenge: &Challenge) -> bool {
    count(earned, &challenge.goal) >= challenge.target
}

fn stat(stats: &Stats, key: &str) -> u32 {
    stats.get(key).copied().unwrap_or(0)
}

fn count(earned: &Earned, goal: &Goal) -> u32 {
    match goal {
        Goal::Stat(key) => stat(&earned.stats, key),
        Goal::WeaponKills(key) => earned.kills(key),
        Goal::Gold(weapon) => ladder_of(*weapon)
            .iter()
            .filter(|c| done(earned, c))
            .count() as u32,
        Goal::Mastery(class) => ladder_weapons()
            .into_iter()
            .filter(|w| w.def().class == *class && gold(earned, *w))
            .count() as u32,
        Goal::Final => mastery_classes()
            .into_iter()
            .filter(|class| mastered(earned, *class))
            .count() as u32,
    }
}

/// A weapon's fourteen ladder challenges, in order.
pub fn ladder_of(weapon: WeaponId) -> Vec<&'static Challenge> {
    let weapon = weapon.plain();
    ALL.iter()
        .filter(|c| c.group == Group::Weapon(weapon) && !matches!(c.goal, Goal::Gold(_)))
        .collect()
}

fn gold(earned: &Earned, weapon: WeaponId) -> bool {
    ALL.iter()
        .find(|c| c.goal == Goal::Gold(weapon))
        .is_some_and(|c| done(earned, c))
}

fn mastered(earned: &Earned, class: WeaponClass) -> bool {
    ALL.iter()
        .find(|c| c.goal == Goal::Mastery(class))
        .is_some_and(|c| done(earned, c))
}

/// Whether `earned` has unlocked `camo` on `weapon` (any variant).
pub fn camo_unlocked(earned: &Earned, weapon: WeaponId, camo: Camo) -> bool {
    let weapon = weapon.plain();
    if camo == Camo::NONE {
        return true;
    }
    if camo == Camo::GOLD {
        return gold(earned, weapon);
    }
    if camo == Camo::MASTERY {
        return mastered(earned, weapon.def().class);
    }
    if camo == Camo::FINAL {
        return ALL
            .iter()
            .find(|c| c.goal == Goal::Final)
            .is_some_and(|c| done(earned, c));
    }
    ladder_of(weapon)
        .get(usize::from(camo.0) - 1)
        .is_some_and(|c| done(earned, c))
}

/// Whether `earned` has unlocked calling card `card`.
pub fn card_unlocked(earned: &Earned, card: Card) -> bool {
    if card == Card::DEFAULT {
        return true;
    }
    if let Some(prestige) = card
        .key()
        .strip_prefix("prestige_")
        .and_then(|p| p.parse::<u8>().ok())
    {
        return earned.prestige >= prestige;
    }
    ALL.iter()
        .any(|c| c.reward == Reward::Card(card) && done(earned, c))
}

/// The challenges `after` has done that `before` had not, in list order.
pub fn completed_between(before: &Earned, after: &Earned) -> Vec<&'static Challenge> {
    ALL.iter()
        .filter(|c| done(after, c) && !done(before, c))
        .collect()
}

/// The XP of [`completed_between`].
pub fn xp_between(before: &Earned, after: &Earned) -> u32 {
    completed_between(before, after)
        .iter()
        .map(|c| c.xp)
        .fold(0, u32::saturating_add)
}

// The weapon ladder.

/// What one weapon challenge counts, by stat suffix ("hs") or kills.
#[derive(Clone, Copy)]
enum Rung {
    Stat(&'static str),
    Kills,
}

/// XP for the nine rungs of the first ladder, then for each later
/// challenge, and for gold: 9,000 for a weapon's whole ladder.
const FIRST_XP: [u32; 9] = [250, 250, 500, 500, 500, 750, 750, 1000, 1000];
const LATER_XP: u32 = 500;
const GOLD_XP: u32 = 1000;

const HEADSHOTS: [u32; 9] = [5, 10, 15, 20, 30, 40, 50, 75, 100];
/// Black Ops 2's sniper rifles climb further.
const SNIPER_ONE_HITS: [u32; 9] = [5, 10, 25, 50, 75, 100, 150, 200, 250];
/// Black Ops 2's launchers count aircraft destroyed; ours count kills
/// until there are aircraft to shoot down.
const LAUNCHER_KILLS: [u32; 9] = [5, 10, 20, 30, 40, 50, 60, 75, 100];

/// A class's fourteen rungs: what each counts and its target.
fn ladder(class: WeaponClass) -> Vec<(Rung, u32)> {
    use Rung::{Kills, Stat};
    let first: Vec<(Rung, u32)> = match class {
        WeaponClass::Shotgun => HEADSHOTS.iter().map(|n| (Stat("ohk"), *n)).collect(),
        WeaponClass::Sniper => SNIPER_ONE_HITS.iter().map(|n| (Stat("ohk"), *n)).collect(),
        WeaponClass::Launcher => LAUNCHER_KILLS.iter().map(|n| (Kills, *n)).collect(),
        _ => HEADSHOTS.iter().map(|n| (Stat("hs"), *n)).collect(),
    };
    let later: [(Rung, u32); 5] = match class {
        WeaponClass::Smg | WeaponClass::Lmg | WeaponClass::Pistol => [
            (Stat("revenge"), 30),
            (Stat("bare"), 150),
            (Stat("noperk"), 150),
            (Stat("double"), 20),
            (Stat("bt"), 10),
        ],
        WeaponClass::Shotgun => [
            (Stat("revenge"), 30),
            (Stat("bare"), 50),
            (Stat("noperk"), 50),
            (Stat("double"), 5),
            (Stat("bt"), 10),
        ],
        WeaponClass::Sniper => [
            (Stat("long"), 10),
            (Stat("bare"), 50),
            (Stat("noperk"), 50),
            (Stat("double"), 5),
            (Stat("bt"), 10),
        ],
        // Launchers carry no attachments, so every kill is a bare one: a
        // longer kill count stands in.
        WeaponClass::Launcher => [
            (Stat("long"), 10),
            (Kills, 150),
            (Stat("noperk"), 50),
            (Stat("double"), 5),
            (Stat("bt"), 5),
        ],
        _ => [
            (Stat("long"), 10),
            (Stat("bare"), 150),
            (Stat("noperk"), 150),
            (Stat("double"), 20),
            (Stat("bt"), 10),
        ],
    };
    first.into_iter().chain(later).collect()
}

fn rung_text(rung: Rung, target: u32) -> (&'static str, String) {
    match rung {
        Rung::Kills => ("Kills", format!("Get {target} kills")),
        Rung::Stat("hs") => ("Headshots", format!("Get {target} headshots")),
        Rung::Stat("ohk") => ("One Hit", format!("Get {target} one-hit kills")),
        Rung::Stat("long") => ("Longshots", format!("Get {target} longshot kills")),
        Rung::Stat("revenge") => ("Revenge", format!("Get {target} revenge kills")),
        Rung::Stat("bare") => (
            "Iron Only",
            format!("Get {target} kills with no attachments on"),
        ),
        Rung::Stat("noperk") => (
            "No Perks",
            format!("Get {target} kills with no perks in your class"),
        ),
        Rung::Stat("double") => ("Doubles", format!("Get {target} double kills")),
        Rung::Stat("bt") => (
            "On a Roll",
            format!("Get 5 kills without dying {target} times"),
        ),
        Rung::Stat(other) => ("Challenge", format!("Get {target} {other}")),
    }
}

fn weapon_challenges(out: &mut Vec<Challenge>) {
    for weapon in ladder_weapons() {
        let def = weapon.def();
        let key = def.key.as_str();
        for (i, (rung, target)) in ladder(def.class).into_iter().enumerate() {
            let goal = match rung {
                Rung::Kills => Goal::WeaponKills(key.to_owned()),
                Rung::Stat(suffix) => Goal::Stat(format!("w.{key}.{suffix}")),
            };
            let (name, detail) = rung_text(rung, target);
            let camo = Camo::rung(i);
            out.push(Challenge {
                id: format!("w.{key}.{i}"),
                group: Group::Weapon(weapon),
                name: format!("{name} {}", roman(i + 1)),
                detail: format!("{detail} with the {}.", def.name),
                goal,
                target,
                xp: FIRST_XP.get(i).copied().unwrap_or(LATER_XP),
                reward: Reward::Camo(camo),
            });
        }
        out.push(Challenge {
            id: format!("w.{key}.gold"),
            group: Group::Weapon(weapon),
            name: "Gold".to_owned(),
            detail: format!("Complete every {} challenge.", def.name),
            goal: Goal::Gold(weapon),
            target: camo::LADDER as u32,
            xp: GOLD_XP,
            reward: Reward::Camo(Camo::GOLD),
        });
    }
}

fn mastery_challenges(out: &mut Vec<Challenge>) {
    let weapons = ladder_weapons();
    let classes = mastery_classes();
    for class in &classes {
        let count = weapons.iter().filter(|w| w.def().class == *class).count() as u32;
        out.push(Challenge {
            id: format!("mastery.{}", class_key(*class)),
            group: Group::Mastery,
            name: format!("{} Mastery", class.label()),
            detail: format!(
                "Earn Gold on every weapon in {}.",
                class.label().to_lowercase()
            ),
            goal: Goal::Mastery(*class),
            target: count,
            // Black Ops 2's Diamond paid no XP of its own.
            xp: 0,
            reward: Reward::Camo(Camo::MASTERY),
        });
    }
    out.push(Challenge {
        id: "mastery.final".to_owned(),
        group: Group::Mastery,
        name: Camo::FINAL.name().to_owned(),
        detail: "Earn mastery in every weapon class.".to_owned(),
        goal: Goal::Final,
        target: classes.len() as u32,
        xp: 0,
        reward: Reward::Camo(Camo::FINAL),
    });
    // The final camo's card comes with it.
    if let Some(card) = Card::by_key("nebula") {
        out.push(Challenge {
            id: "mastery.final.card".to_owned(),
            group: Group::Mastery,
            name: "Nebula Card".to_owned(),
            detail: "Earn mastery in every weapon class.".to_owned(),
            goal: Goal::Final,
            target: classes.len() as u32,
            xp: 0,
            reward: Reward::Card(card),
        });
    }
}

pub fn class_key(class: WeaponClass) -> &'static str {
    match class {
        WeaponClass::AssaultRifle => "assault_rifle",
        WeaponClass::Smg => "smg",
        WeaponClass::Lmg => "lmg",
        WeaponClass::Shotgun => "shotgun",
        WeaponClass::Marksman => "marksman",
        WeaponClass::Sniper => "sniper",
        WeaponClass::Pistol => "pistol",
        WeaponClass::Launcher => "launcher",
    }
}

/// One tiered challenge: a stat, its tiers (target, XP), and the card the
/// last tier awards.
struct Tiered {
    id: &'static str,
    name: &'static str,
    /// The detail, with `{}` for the target.
    detail: &'static str,
    stat: &'static str,
    tiers: &'static [(u32, u32)],
    card: Option<&'static str>,
}

fn tiered(out: &mut Vec<Challenge>, group: Group, list: &[Tiered]) {
    for t in list {
        for (i, (target, xp)) in t.tiers.iter().enumerate() {
            let last = i + 1 == t.tiers.len();
            let reward = match t.card.and_then(Card::by_key) {
                Some(card) if last => Reward::Card(card),
                _ => Reward::None,
            };
            out.push(Challenge {
                id: format!("{}.{i}", t.id),
                group,
                name: if t.tiers.len() > 1 {
                    format!("{} {}", t.name, roman(i + 1))
                } else {
                    t.name.to_owned()
                },
                detail: t.detail.replace("{}", &target.to_string()),
                goal: Goal::Stat(t.stat.to_owned()),
                target: *target,
                xp: *xp,
                reward,
            });
        }
    }
}

const CAREER: &[Tiered] = &[
    Tiered {
        id: "career.kills",
        name: "Killer",
        detail: "Get {} kills.",
        stat: "kills",
        tiers: &[
            (100, 500),
            (500, 1000),
            (1000, 2000),
            (2500, 3000),
            (5000, 5000),
        ],
        card: Some("veteran"),
    },
    Tiered {
        id: "career.headshots",
        name: "Headhunter",
        detail: "Get {} headshot kills.",
        stat: "headshots",
        tiers: &[(50, 500), (250, 1500), (500, 2500), (1000, 5000)],
        card: Some("sharpshooter"),
    },
    Tiered {
        id: "career.wins",
        name: "Winner",
        detail: "Win {} matches.",
        stat: "wins",
        tiers: &[(10, 500), (50, 1500), (100, 2500), (250, 5000)],
        card: Some("victor"),
    },
    Tiered {
        id: "career.matches",
        name: "Regular",
        detail: "Finish {} matches.",
        stat: "matches",
        tiers: &[(25, 500), (100, 1500), (500, 5000)],
        card: Some("regular"),
    },
    Tiered {
        id: "career.longshots",
        name: "Long Range",
        detail: "Get {} longshot kills.",
        stat: "longshots",
        tiers: &[(25, 1000), (100, 2500), (250, 5000)],
        card: Some("far_sight"),
    },
    Tiered {
        id: "career.revenge",
        name: "Payback",
        detail: "Kill {} enemies who killed you last.",
        stat: "revenge",
        tiers: &[(25, 1000), (100, 2500)],
        card: Some("payback"),
    },
    Tiered {
        id: "career.doubles",
        name: "Double Up",
        detail: "Get {} double kills.",
        stat: "doubles",
        tiers: &[(25, 1000), (100, 2500)],
        card: Some("double_tap"),
    },
    Tiered {
        id: "career.triples",
        name: "Chain",
        detail: "Get {} triple kills or better.",
        stat: "triples",
        tiers: &[(10, 1500), (50, 3000)],
        card: Some("chain"),
    },
    Tiered {
        id: "career.bloodthirsty",
        name: "On a Roll",
        detail: "Get 5 kills without dying {} times.",
        stat: "bloodthirsty",
        tiers: &[(10, 1000), (50, 2500)],
        card: Some("hot_streak"),
    },
    Tiered {
        id: "career.merciless",
        name: "Untouchable",
        detail: "Get 10 kills without dying {} times.",
        stat: "merciless",
        tiers: &[(5, 1500), (25, 3000)],
        card: Some("unstoppable"),
    },
    Tiered {
        id: "career.knife",
        name: "Close Quarters",
        detail: "Get {} knife kills.",
        stat: "knife.kills",
        tiers: &[(25, 1000), (100, 2500)],
        card: Some("blade"),
    },
    Tiered {
        id: "career.frag",
        name: "Grenadier",
        detail: "Get {} frag grenade kills.",
        stat: "frag.kills",
        tiers: &[(25, 1000), (100, 2500)],
        card: Some("shrapnel"),
    },
    Tiered {
        id: "career.aircraft",
        name: "Air Defense",
        detail: "Shoot down {} enemy aircraft.",
        stat: "aircraft",
        tiers: &[(5, 1000), (25, 2500)],
        card: None,
    },
    Tiered {
        id: "career.collateral",
        name: "Two for One",
        detail: "Kill two enemies with one bullet {} times.",
        stat: "collateral",
        tiers: &[(5, 1000), (25, 2500)],
        card: Some("two_for_one"),
    },
];

/// Each mode's challenges.
fn mode_challenges(out: &mut Vec<Challenge>) {
    let modes: &[(&'static str, &[Tiered])] = &[
        (
            "tdm",
            &[Tiered {
                id: "mode.tdm.wins",
                name: "Team Player",
                detail: "Win {} Team Deathmatch matches.",
                stat: "m.tdm.wins",
                tiers: &[(5, 500), (25, 1500), (100, 3000)],
                card: Some("squad"),
            }],
        ),
        (
            "ffa",
            &[Tiered {
                id: "mode.ffa.wins",
                name: "Lone Wolf",
                detail: "Win {} Free-for-All matches.",
                stat: "m.ffa.wins",
                tiers: &[(5, 1000), (25, 2500)],
                card: Some("lone_wolf"),
            }],
        ),
        (
            "dom",
            &[
                Tiered {
                    id: "mode.dom.wins",
                    name: "Dominant",
                    detail: "Win {} Domination matches.",
                    stat: "m.dom.wins",
                    tiers: &[(5, 500), (25, 1500)],
                    card: None,
                },
                Tiered {
                    id: "mode.dom.captures",
                    name: "Flag Taker",
                    detail: "Capture {} flags in Domination.",
                    stat: "a.dom_capture",
                    tiers: &[(25, 1000), (100, 2500)],
                    card: Some("flagbearer"),
                },
                Tiered {
                    id: "mode.dom.defends",
                    name: "Flag Guard",
                    detail: "Get {} defend kills in Domination.",
                    stat: "a.dom_defend",
                    tiers: &[(50, 1500)],
                    card: None,
                },
            ],
        ),
        (
            "hardpoint",
            &[
                Tiered {
                    id: "mode.hardpoint.wins",
                    name: "Holdout",
                    detail: "Win {} Hardpoint matches.",
                    stat: "m.hardpoint.wins",
                    tiers: &[(5, 500), (25, 1500)],
                    card: None,
                },
                Tiered {
                    id: "mode.hardpoint.captures",
                    name: "Zone Taker",
                    detail: "Capture {} hardpoints.",
                    stat: "a.hp_capture",
                    tiers: &[(25, 1000), (100, 2500)],
                    card: Some("zone_control"),
                },
                Tiered {
                    id: "mode.hardpoint.defends",
                    name: "Zone Guard",
                    detail: "Get {} defend kills in Hardpoint.",
                    stat: "a.hp_defend",
                    tiers: &[(50, 1500)],
                    card: None,
                },
            ],
        ),
        (
            "ctf",
            &[
                Tiered {
                    id: "mode.ctf.wins",
                    name: "Flag Runner",
                    detail: "Win {} Capture the Flag matches.",
                    stat: "m.ctf.wins",
                    tiers: &[(5, 500), (25, 1500)],
                    card: None,
                },
                Tiered {
                    id: "mode.ctf.captures",
                    name: "Homeward",
                    detail: "Capture the enemy flag {} times.",
                    stat: "a.flag_capture",
                    tiers: &[(10, 1000), (50, 2500)],
                    card: Some("runner"),
                },
                Tiered {
                    id: "mode.ctf.returns",
                    name: "Recovery",
                    detail: "Return your flag {} times.",
                    stat: "a.flag_return",
                    tiers: &[(25, 1500)],
                    card: None,
                },
                Tiered {
                    id: "mode.ctf.carrier_kills",
                    name: "Interceptor",
                    detail: "Kill {} enemy flag carriers.",
                    stat: "a.carrier_kill",
                    tiers: &[(25, 1500)],
                    card: None,
                },
            ],
        ),
        (
            "snd",
            &[
                Tiered {
                    id: "mode.snd.wins",
                    name: "Mission Done",
                    detail: "Win {} Search and Destroy matches.",
                    stat: "m.snd.wins",
                    tiers: &[(5, 500), (25, 1500)],
                    card: None,
                },
                Tiered {
                    id: "mode.snd.plants",
                    name: "Charge Setter",
                    detail: "Plant the charge {} times.",
                    stat: "a.plant",
                    tiers: &[(10, 1000), (50, 2500)],
                    card: Some("demolition"),
                },
                Tiered {
                    id: "mode.snd.defuses",
                    name: "Steady Hands",
                    detail: "Defuse the charge {} times.",
                    stat: "a.defuse",
                    tiers: &[(10, 1000), (50, 2500)],
                    card: Some("cool_head"),
                },
                Tiered {
                    id: "mode.snd.wire_cutter",
                    name: "Wire Cutter",
                    detail: "Defuse with an attacker still alive {} times.",
                    stat: "a.wire_cutter",
                    tiers: &[(5, 2000)],
                    card: None,
                },
                Tiered {
                    id: "mode.snd.clutch",
                    name: "Clutch",
                    detail: "Win a round as your team's last one standing {} times.",
                    stat: "a.clutch",
                    tiers: &[(5, 2500)],
                    card: Some("last_standing"),
                },
                Tiered {
                    id: "mode.snd.ace",
                    name: "Clean Sweep",
                    detail: "Kill the whole enemy team in one round {} time(s).",
                    stat: "a.ace",
                    tiers: &[(1, 3000)],
                    card: Some("ace"),
                },
            ],
        ),
        (
            "gungame",
            &[Tiered {
                id: "mode.gungame.wins",
                name: "Top of the Ladder",
                detail: "Win {} Gun Game matches.",
                stat: "m.gungame.wins",
                tiers: &[(5, 1000), (25, 2500)],
                card: Some("climber"),
            }],
        ),
    ];
    for (mode, list) in modes {
        tiered(out, Group::Mode(mode), list);
    }
}

fn streak_challenges(out: &mut Vec<Challenge>) {
    let list: &[(Streak, &[Tiered])] = &[
        (
            Streak::Uav,
            &[Tiered {
                id: "streak.uav.earned",
                name: "Eyes Up",
                detail: "Earn the UAV {} times.",
                stat: "s.uav.earned",
                tiers: &[(10, 500), (50, 1500)],
                card: Some("watchtower"),
            }],
        ),
        (
            Streak::CounterUav,
            &[Tiered {
                id: "streak.counter_uav.earned",
                name: "Jammer",
                detail: "Earn the Counter-UAV {} times.",
                stat: "s.counter_uav.earned",
                tiers: &[(10, 500), (50, 1500)],
                card: Some("blackout"),
            }],
        ),
        (
            Streak::CarePackage,
            &[Tiered {
                id: "streak.care_package.earned",
                name: "Special Delivery",
                detail: "Earn the Care Package {} times.",
                stat: "s.care_package.earned",
                tiers: &[(10, 750), (25, 1500)],
                card: Some("airdrop"),
            }],
        ),
    ];
    for (streak, list) in list {
        tiered(out, Group::Scorestreak(*streak), list);
    }
}

fn roman(n: usize) -> &'static str {
    const NUMERALS: [&str; 15] = [
        "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII", "XIII", "XIV",
        "XV",
    ];
    NUMERALS.get(n.max(1) - 1).copied().unwrap_or("")
}

static ALL: LazyLock<Vec<Challenge>> = LazyLock::new(|| {
    let mut out = Vec::new();
    weapon_challenges(&mut out);
    mastery_challenges(&mut out);
    tiered(&mut out, Group::Career, CAREER);
    mode_challenges(&mut out);
    streak_challenges(&mut out);
    out
});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats;

    fn earned_with(stats: &[(&str, u32)]) -> Earned {
        let mut earned = Earned::default();
        for (key, n) in stats {
            earned.stats.insert((*key).to_owned(), *n);
        }
        earned
    }

    #[test]
    fn ids_are_unique_and_stats_are_valid_keys() {
        let mut ids: Vec<&str> = all().iter().map(|c| c.id.as_str()).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before, "duplicate challenge ids");
        for c in all() {
            if let Goal::Stat(key) = &c.goal {
                assert!(stats::valid_key(key), "{key}");
            }
            assert!(c.target > 0, "{}", c.id);
        }
    }

    #[test]
    fn every_ladder_weapon_has_fourteen_rungs_and_gold() {
        for weapon in ladder_weapons() {
            assert_eq!(
                ladder_of(weapon).len(),
                camo::LADDER,
                "{}",
                weapon.def().key
            );
            let total: u32 = all()
                .iter()
                .filter(|c| c.group == Group::Weapon(weapon))
                .map(|c| c.xp)
                .sum();
            assert_eq!(total, 9000, "{}", weapon.def().key);
        }
        // Every card a challenge awards exists, and every non-prestige card
        // but the default is awarded by something.
        for card in Card::all() {
            if card == Card::DEFAULT || card.key().starts_with("prestige_") {
                continue;
            }
            assert!(
                all().iter().any(|c| c.reward == Reward::Card(card)),
                "nothing awards {}",
                card.key()
            );
        }
    }

    #[test]
    fn the_headshot_ladder_unlocks_camos_in_order() {
        let m4 = WeaponId::expect("m4");
        let earned = earned_with(&[("w.m4.hs", 15)]);
        assert!(camo_unlocked(&earned, m4, Camo::NONE));
        assert!(camo_unlocked(&earned, m4, Camo::rung(0)));
        assert!(camo_unlocked(&earned, m4, Camo::rung(2)));
        assert!(!camo_unlocked(&earned, m4, Camo::rung(3)));
        assert!(!camo_unlocked(&earned, m4, Camo::GOLD));
        // Any variant shares the plain weapon's camos.
        let variant = WeaponId::all()
            .find(|w| w.plain() == m4 && *w != m4)
            .expect("a variant");
        assert!(camo_unlocked(&earned, variant, Camo::rung(1)));
    }

    #[test]
    fn crossing_targets_pays_once() {
        let before = earned_with(&[("w.m4.hs", 4), ("kills", 99)]);
        let after = earned_with(&[("w.m4.hs", 11), ("kills", 100)]);
        let done: Vec<&str> = completed_between(&before, &after)
            .iter()
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(done, ["w.m4.0", "w.m4.1", "career.kills.0"]);
        assert_eq!(xp_between(&before, &after), 250 + 250 + 500);
        assert_eq!(xp_between(&after, &after), 0);
    }

    #[test]
    fn gold_mastery_and_final_follow_the_ladders() {
        let mut earned = Earned::default();
        let weapons = ladder_weapons();
        let max = |earned: &mut Earned, weapon: WeaponId| {
            for c in ladder_of(weapon) {
                match &c.goal {
                    Goal::Stat(key) => {
                        earned.stats.insert(key.clone(), c.target.max(1000));
                    }
                    Goal::WeaponKills(key) => {
                        earned.weapon_kills.insert(key.clone(), c.target.max(1000));
                    }
                    _ => {}
                }
            }
        };
        let smgs: Vec<WeaponId> = weapons
            .iter()
            .copied()
            .filter(|w| w.def().class == WeaponClass::Smg)
            .collect();
        max(&mut earned, smgs[0]);
        assert!(camo_unlocked(&earned, smgs[0], Camo::GOLD));
        if smgs.len() > 1 {
            assert!(!camo_unlocked(&earned, smgs[0], Camo::MASTERY));
        }
        for w in &smgs {
            max(&mut earned, *w);
        }
        assert!(camo_unlocked(&earned, smgs[0], Camo::MASTERY));
        assert!(!camo_unlocked(&earned, smgs[0], Camo::FINAL));
        for w in &weapons {
            max(&mut earned, *w);
        }
        assert!(camo_unlocked(&earned, weapons[0], Camo::FINAL));
        let nebula = Card::by_key("nebula").expect("the card");
        assert!(card_unlocked(&earned, nebula));
        assert!(!card_unlocked(&Earned::default(), nebula));
    }
}
