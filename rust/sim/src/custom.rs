//! Custom game settings: the rules a host sets before a private or local
//! match, as in Black Ops 2's Custom Games (its `getgametypesetting` keys,
//! named beside each field).
//!
//! The server owns them: it takes them on the command line (`--custom`, in
//! the text form [`Settings::parse`] reads), applies them, and sends them to
//! every player in the welcome, so prediction (health and its regeneration)
//! and the HUD (hardcore, the minimap) follow the same rules. Ranked servers
//! ignore them: ranked play is always the default rules.
//!
//! Hardcore is a set of rules, as Treyarch put it ("not a game mode, a variant
//! on the game rules"): 30% health, friendly fire on, and no HUD. Turning it
//! on in the text form sets the first two; later keys can still change them.

use crate::net::bits::{BitSink, BitSource, DecodeError, DecodeResult};
use crate::player::HealthParams;
use crate::tdm::{MatchRules, Mode};

/// Health a hardcore match starts players with, in percent of normal
/// (Black Ops 2's hardcore `playerMaxHealth` 30).
pub const HARDCORE_HEALTH: u8 = 30;
/// The health choices, in percent of normal.
pub const HEALTH_CHOICES: [u8; 5] = [30, 50, 100, 150, 200];
/// Respawn delay choices, in seconds; 0 is instant.
pub const RESPAWN_CHOICES: [u8; 5] = [0, 3, 5, 7, 10];
/// The share of a friendly hit that reflects onto the attacker, or that each
/// takes when it is shared (Black Ops 2 `friendlyfire` 2 and 3: half,
/// at least 1).
pub const FRIENDLY_SHARE: f32 = 0.5;
/// Longest text form [`Settings::parse`] takes, in bytes.
pub const MAX_TEXT_BYTES: usize = 256;

/// How fast health comes back (Black Ops 2 `playerHealthRegenTime`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Regen {
    #[default]
    Normal,
    /// Twice the wait, at half the rate.
    Slow,
    /// Half the wait, at twice the rate.
    Fast,
    /// None: damage stays until the next life.
    Off,
}

/// What a hit on a teammate does (Black Ops 2 `friendlyfiretype` 0 to 3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FriendlyFire {
    /// Teammates take no damage.
    #[default]
    Off,
    /// Teammates take full damage.
    On,
    /// The attacker takes half the damage instead.
    Reflect,
    /// The teammate and the attacker each take half.
    Shared,
}

impl FriendlyFire {
    /// A hit of `damage` on a teammate: what the teammate takes, and what
    /// reflects onto the attacker. Black Ops 2 halves reflected and shared
    /// damage, `int(damage * 0.5)`, at least 1.
    pub fn split(self, damage: f32) -> (f32, f32) {
        let half = (damage * FRIENDLY_SHARE).floor().max(1.0);
        match self {
            Self::Off => (0.0, 0.0),
            Self::On => (damage, 0.0),
            Self::Reflect => (0.0, half),
            Self::Shared => (half, half),
        }
    }
}

/// What the minimap shows (Black Ops 2 `forceRadar`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Radar {
    /// Shots and UAVs put enemies on it.
    #[default]
    Normal,
    /// A UAV that never goes down: every enemy, every sweep.
    Constant,
    /// No minimap.
    Off,
}

/// Which classes players may use (Black Ops 2 `disableCAC`, and the
/// restrictions party games use).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Loadouts {
    /// Any class the player builds.
    #[default]
    Any,
    /// Only the ready-made classes.
    DefaultClasses,
    /// Pistols only: no primary, the class's pistol (or the default one).
    PistolsOnly,
    /// Sniper rifles only, with a pistol as the secondary.
    SnipersOnly,
}

/// A custom game's rules. The defaults are the normal game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// The score to win (`scoreLimit`); `None` keeps the mode's.
    pub score_limit: Option<u16>,
    /// Minutes of play (`timeLimit`); `Some(0)` is no clock, `None` keeps
    /// the server's.
    pub time_limit: Option<u8>,
    /// Search and Destroy's rounds to win (`roundWinLimit`); `None` keeps
    /// the server's.
    pub rounds_to_win: Option<u8>,
    /// Seconds a dead player waits (`playerRespawnDelay`); `None` keeps the
    /// mode's.
    pub respawn_delay: Option<u8>,
    /// Health, in percent of normal (`playerMaxHealth`).
    pub health: u8,
    pub regen: Regen,
    pub friendly_fire: FriendlyFire,
    /// Killcams on death and the final killcam (`allowKillcam`).
    pub killcam: bool,
    pub radar: Radar,
    /// Scorestreaks are earned and called in (`custom_killstreak_mode`).
    pub streaks: bool,
    pub loadouts: Loadouts,
    /// No HUD (`hardcoreMode`); its health and friendly fire are their own
    /// fields.
    pub hardcore: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            score_limit: None,
            time_limit: None,
            rounds_to_win: None,
            respawn_delay: None,
            health: 100,
            regen: Regen::Normal,
            friendly_fire: FriendlyFire::Off,
            killcam: true,
            radar: Radar::Normal,
            streaks: true,
            loadouts: Loadouts::Any,
            hardcore: false,
        }
    }
}

impl Settings {
    /// Black Ops 2's hardcore rules: 30% health, friendly fire on, no HUD.
    pub fn hardcore() -> Self {
        Self {
            hardcore: true,
            health: HARDCORE_HEALTH,
            friendly_fire: FriendlyFire::On,
            ..Self::default()
        }
    }

    /// Whether these are the normal rules.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Health and its regeneration under these rules, from the normal
    /// `base`.
    pub fn health_params(&self, base: HealthParams) -> HealthParams {
        let percent = f32::from(self.health.clamp(1, 250));
        let scale = percent / 100.0;
        let (delay, rate) = match self.regen {
            Regen::Normal => (base.regen_delay, base.regen_rate),
            Regen::Slow => (base.regen_delay * 2.0, base.regen_rate * 0.5),
            Regen::Fast => (base.regen_delay * 0.5, base.regen_rate * 2.0),
            Regen::Off => (f32::INFINITY, 0.0),
        };
        HealthParams {
            max_health: base.max_health * percent / 100.0,
            regen_delay: delay,
            // The rate scales with health, so a full refill takes as long.
            regen_rate: rate * scale,
            ..base
        }
    }

    /// The match rules for `mode` under these settings, from the server's
    /// own `rules`. Zombies keeps its own (the horde ends it).
    pub fn match_rules(&self, mode: &Mode, rules: MatchRules) -> MatchRules {
        if *mode == Mode::Zombies {
            return rules;
        }
        let mut rules = rules;
        if let Some(limit) = self.score_limit.filter(|l| *l > 0)
            && *mode != Mode::SearchAndDestroy
        {
            rules.score_limit = u32::from(limit);
        }
        if let Some(minutes) = self.time_limit
            && rules.time_limit.is_some()
        {
            rules.time_limit = (minutes > 0).then_some(f32::from(minutes) * 60.0);
        }
        rules
    }

    /// Reads the text form: `key=value` pairs split by commas, as servers
    /// take them on the command line. Keys: `score`, `time` (minutes, 0 for
    /// none), `rounds`, `respawn` (seconds), `health` (percent), `regen`
    /// (normal, slow, fast, off), `ff` (off, on, reflect, shared),
    /// `killcam`, `streaks`, `hardcore` (on, off), `radar` (normal,
    /// constant, off), and `loadouts` (any, default, pistols, snipers).
    /// `hardcore=on` sets hardcore's health and friendly fire, so it comes
    /// first. Unknown keys and values are errors.
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > MAX_TEXT_BYTES {
            return Err("custom settings too long".into());
        }
        let mut s = Self::default();
        for pair in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let (key, value) = pair
                .split_once('=')
                .ok_or_else(|| format!("\"{pair}\" is not key=value"))?;
            let (key, value) = (key.trim(), value.trim().to_ascii_lowercase());
            let bad = || format!("bad value for {key}: \"{value}\"");
            let number = |max: u32| -> Result<u32, String> {
                value
                    .parse::<u32>()
                    .ok()
                    .filter(|n| *n <= max)
                    .ok_or_else(bad)
            };
            let switch = || match value.as_str() {
                "1" | "on" | "true" | "yes" => Ok(true),
                "0" | "off" | "false" | "no" => Ok(false),
                _ => Err(bad()),
            };
            match key {
                "score" => s.score_limit = Some(number(u32::from(u16::MAX))? as u16),
                "time" => s.time_limit = Some(number(120)? as u8),
                "rounds" => s.rounds_to_win = Some(number(20)?.max(1) as u8),
                "respawn" => s.respawn_delay = Some(number(30)? as u8),
                "health" => s.health = number(250)?.max(1) as u8,
                "regen" => {
                    s.regen = match value.as_str() {
                        "normal" => Regen::Normal,
                        "slow" => Regen::Slow,
                        "fast" => Regen::Fast,
                        "off" | "none" => Regen::Off,
                        _ => return Err(bad()),
                    }
                }
                "ff" => {
                    s.friendly_fire = match value.as_str() {
                        "off" | "0" => FriendlyFire::Off,
                        "on" | "1" => FriendlyFire::On,
                        "reflect" => FriendlyFire::Reflect,
                        "shared" => FriendlyFire::Shared,
                        _ => return Err(bad()),
                    }
                }
                "killcam" => s.killcam = switch()?,
                "streaks" => s.streaks = switch()?,
                "radar" => {
                    s.radar = match value.as_str() {
                        "normal" => Radar::Normal,
                        "constant" | "always" => Radar::Constant,
                        "off" => Radar::Off,
                        _ => return Err(bad()),
                    }
                }
                "loadouts" => {
                    s.loadouts = match value.as_str() {
                        "any" => Loadouts::Any,
                        "default" | "defaults" => Loadouts::DefaultClasses,
                        "pistols" => Loadouts::PistolsOnly,
                        "snipers" => Loadouts::SnipersOnly,
                        _ => return Err(bad()),
                    }
                }
                "hardcore" => {
                    if switch()? {
                        s.hardcore = true;
                        s.health = HARDCORE_HEALTH;
                        s.friendly_fire = FriendlyFire::On;
                    } else {
                        s.hardcore = false;
                    }
                }
                _ => return Err(format!("unknown custom setting \"{key}\"")),
            }
        }
        Ok(s)
    }

    /// The text form [`Settings::parse`] reads back, listing only what
    /// differs from the defaults (empty for the normal rules).
    pub fn to_text(&self) -> String {
        let d = Self::default();
        let mut parts: Vec<String> = Vec::new();
        // First, since it sets health and friendly fire.
        if self.hardcore {
            parts.push("hardcore=on".into());
        }
        let base = if self.hardcore { Self::hardcore() } else { d };
        if let Some(v) = self.score_limit {
            parts.push(format!("score={v}"));
        }
        if let Some(v) = self.time_limit {
            parts.push(format!("time={v}"));
        }
        if let Some(v) = self.rounds_to_win {
            parts.push(format!("rounds={v}"));
        }
        if let Some(v) = self.respawn_delay {
            parts.push(format!("respawn={v}"));
        }
        if self.health != base.health {
            parts.push(format!("health={}", self.health));
        }
        if self.regen != base.regen {
            let v = match self.regen {
                Regen::Normal => "normal",
                Regen::Slow => "slow",
                Regen::Fast => "fast",
                Regen::Off => "off",
            };
            parts.push(format!("regen={v}"));
        }
        if self.friendly_fire != base.friendly_fire {
            let v = match self.friendly_fire {
                FriendlyFire::Off => "off",
                FriendlyFire::On => "on",
                FriendlyFire::Reflect => "reflect",
                FriendlyFire::Shared => "shared",
            };
            parts.push(format!("ff={v}"));
        }
        if self.killcam != base.killcam {
            parts.push(format!("killcam={}", on_off(self.killcam)));
        }
        if self.streaks != base.streaks {
            parts.push(format!("streaks={}", on_off(self.streaks)));
        }
        if self.radar != base.radar {
            let v = match self.radar {
                Radar::Normal => "normal",
                Radar::Constant => "constant",
                Radar::Off => "off",
            };
            parts.push(format!("radar={v}"));
        }
        if self.loadouts != base.loadouts {
            let v = match self.loadouts {
                Loadouts::Any => "any",
                Loadouts::DefaultClasses => "default",
                Loadouts::PistolsOnly => "pistols",
                Loadouts::SnipersOnly => "snipers",
            };
            parts.push(format!("loadouts={v}"));
        }
        parts.join(",")
    }

    /// Writes the settings for the welcome: a flag, then the fields only
    /// when they differ from the defaults.
    pub fn write(&self, w: &mut impl BitSink) {
        let custom = !self.is_default();
        w.bool(custom);
        if !custom {
            return;
        }
        write_u16(w, self.score_limit);
        write_u8(w, self.time_limit);
        write_u8(w, self.rounds_to_win);
        write_u8(w, self.respawn_delay);
        w.u8(self.health);
        w.bits(self.regen as u32, 2);
        w.bits(self.friendly_fire as u32, 2);
        w.bool(self.killcam);
        w.bits(self.radar as u32, 2);
        w.bool(self.streaks);
        w.bits(self.loadouts as u32, 2);
        w.bool(self.hardcore);
    }

    /// Reads what [`Settings::write`] wrote.
    pub fn read(r: &mut impl BitSource) -> DecodeResult<Self> {
        if !r.bool()? {
            return Ok(Self::default());
        }
        let score_limit = read_u16(r)?;
        let time_limit = read_u8(r)?;
        let rounds_to_win = read_u8(r)?;
        let respawn_delay = read_u8(r)?;
        let health = r.u8()?.max(1);
        let regen = match r.bits(2)? {
            0 => Regen::Normal,
            1 => Regen::Slow,
            2 => Regen::Fast,
            _ => Regen::Off,
        };
        let friendly_fire = match r.bits(2)? {
            0 => FriendlyFire::Off,
            1 => FriendlyFire::On,
            2 => FriendlyFire::Reflect,
            _ => FriendlyFire::Shared,
        };
        let killcam = r.bool()?;
        let radar = match r.bits(2)? {
            0 => Radar::Normal,
            1 => Radar::Constant,
            2 => Radar::Off,
            _ => return Err(DecodeError::Invalid("radar rule")),
        };
        let streaks = r.bool()?;
        let loadouts = match r.bits(2)? {
            0 => Loadouts::Any,
            1 => Loadouts::DefaultClasses,
            2 => Loadouts::PistolsOnly,
            _ => Loadouts::SnipersOnly,
        };
        let hardcore = r.bool()?;
        Ok(Self {
            score_limit,
            time_limit,
            rounds_to_win,
            respawn_delay,
            health,
            regen,
            friendly_fire,
            killcam,
            radar,
            streaks,
            loadouts,
            hardcore,
        })
    }
}

fn write_u8(w: &mut impl BitSink, value: Option<u8>) {
    w.bool(value.is_some());
    if let Some(v) = value {
        w.u8(v);
    }
}

fn write_u16(w: &mut impl BitSink, value: Option<u16>) {
    w.bool(value.is_some());
    if let Some(v) = value {
        w.u16(v);
    }
}

fn read_u8(r: &mut impl BitSource) -> DecodeResult<Option<u8>> {
    Ok(if r.bool()? { Some(r.u8()?) } else { None })
}

fn read_u16(r: &mut impl BitSource) -> DecodeResult<Option<u16>> {
    Ok(if r.bool()? { Some(r.u16()?) } else { None })
}

fn on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::bits::{BitReader, BitWriter};

    fn round_trip(s: &Settings) -> Settings {
        let mut w = BitWriter::new();
        s.write(&mut w);
        let bytes = w.finish();
        Settings::read(&mut BitReader::new(&bytes)).unwrap()
    }

    #[test]
    fn defaults_are_the_normal_game_and_cost_one_bit() {
        let s = Settings::default();
        assert!(s.is_default());
        assert_eq!(s.to_text(), "");
        assert_eq!(Settings::parse("").unwrap(), s);
        let mut w = BitWriter::new();
        s.write(&mut w);
        assert_eq!(w.finish().len(), 1);
        assert_eq!(
            s.health_params(HealthParams::default()),
            HealthParams::default()
        );
    }

    #[test]
    fn hardcore_is_thirty_percent_health_and_friendly_fire() {
        let s = Settings::parse("hardcore=on").unwrap();
        assert_eq!(s, Settings::hardcore());
        assert!(s.hardcore);
        assert_eq!(s.health, 30);
        assert_eq!(s.friendly_fire, FriendlyFire::On);
        let health = s.health_params(HealthParams::default());
        assert_eq!(health.max_health, 30.0);
        // A later key still changes what hardcore set.
        let s = Settings::parse("hardcore=on,ff=reflect").unwrap();
        assert_eq!(s.friendly_fire, FriendlyFire::Reflect);
        assert_eq!(s.to_text(), "hardcore=on,ff=reflect");
    }

    #[test]
    fn text_and_wire_round_trip() {
        let s = Settings {
            score_limit: Some(150),
            time_limit: Some(0),
            rounds_to_win: Some(4),
            respawn_delay: Some(0),
            health: 150,
            regen: Regen::Off,
            friendly_fire: FriendlyFire::Shared,
            killcam: false,
            radar: Radar::Constant,
            streaks: false,
            loadouts: Loadouts::SnipersOnly,
            hardcore: true,
        };
        assert_eq!(Settings::parse(&s.to_text()).unwrap(), s);
        assert_eq!(round_trip(&s), s);
        for radar in [Radar::Normal, Radar::Constant, Radar::Off] {
            for loadouts in [
                Loadouts::Any,
                Loadouts::DefaultClasses,
                Loadouts::PistolsOnly,
                Loadouts::SnipersOnly,
            ] {
                let s = Settings {
                    radar,
                    loadouts,
                    ..Settings::default()
                };
                assert_eq!(round_trip(&s), s);
                assert_eq!(Settings::parse(&s.to_text()).unwrap(), s);
            }
        }
    }

    #[test]
    fn bad_text_is_refused() {
        assert!(Settings::parse("ff=sometimes").is_err());
        assert!(Settings::parse("gravity=low").is_err());
        assert!(Settings::parse("health").is_err());
        assert!(Settings::parse("time=500").is_err());
        assert!(Settings::parse(&"a".repeat(MAX_TEXT_BYTES + 1)).is_err());
        // Spaces and case are forgiven.
        let s = Settings::parse(" killcam = OFF , radar=Constant ").unwrap();
        assert!(!s.killcam);
        assert_eq!(s.radar, Radar::Constant);
    }

    #[test]
    fn friendly_fire_splits_damage_as_black_ops_2() {
        assert_eq!(FriendlyFire::Off.split(40.0), (0.0, 0.0));
        assert_eq!(FriendlyFire::On.split(40.0), (40.0, 0.0));
        assert_eq!(FriendlyFire::Reflect.split(41.0), (0.0, 20.0));
        assert_eq!(FriendlyFire::Shared.split(41.0), (20.0, 20.0));
        // At least 1.
        assert_eq!(FriendlyFire::Shared.split(1.0), (1.0, 1.0));
    }

    #[test]
    fn regeneration_choices() {
        let base = HealthParams::default();
        let slow = Settings {
            regen: Regen::Slow,
            ..Settings::default()
        }
        .health_params(base);
        assert_eq!(slow.regen_delay, base.regen_delay * 2.0);
        let off = Settings {
            regen: Regen::Off,
            ..Settings::default()
        }
        .health_params(base);
        assert_eq!(off.regen_rate, 0.0);
        // Half health refills at half the rate: the same time to full.
        let half = Settings {
            health: 50,
            ..Settings::default()
        }
        .health_params(base);
        assert_eq!(
            half.max_health / half.regen_rate,
            base.max_health / base.regen_rate
        );
    }

    #[test]
    fn limits_override_the_servers() {
        let rules = MatchRules::default();
        let s = Settings {
            score_limit: Some(30),
            time_limit: Some(0),
            ..Settings::default()
        };
        let tdm = s.match_rules(&Mode::TeamDeathmatch, rules);
        assert_eq!(tdm.score_limit, 30);
        assert_eq!(tdm.time_limit, None);
        // Zombies keeps its own; Search and Destroy counts rounds instead.
        assert_eq!(s.match_rules(&Mode::Zombies, rules), rules);
        let snd = s.match_rules(&Mode::SearchAndDestroy, rules);
        assert_eq!(snd.score_limit, rules.score_limit);
    }
}
