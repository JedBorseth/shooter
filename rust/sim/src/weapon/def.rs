//! Weapon definitions, loaded from RON files, and the table that holds them.
//!
//! Each weapon is one file in `godot/data/weapons/<key>.ron`. The game loads the
//! directory at startup (and reloads it in debug builds when a file changes); the
//! same files are compiled in as [`builtin`] so tests and tools work without the
//! engine. Client and server must load identical tables: a [`WeaponId`] is an
//! index into the table, which is sorted by key.

use std::fmt;
use std::sync::{LazyLock, RwLock};

use glam::Vec2;
use serde::Deserialize;

/// Index of a weapon in the installed table.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WeaponId(pub u8);

impl WeaponId {
    pub fn def(self) -> &'static WeaponDef {
        &table()[usize::from(self.0)]
    }

    /// Looks a weapon up by its file key, such as `"m4"`.
    pub fn by_key(key: &str) -> Option<WeaponId> {
        table()
            .iter()
            .position(|def| def.key == key)
            .and_then(|i| u8::try_from(i).ok())
            .map(WeaponId)
    }

    /// Like [`WeaponId::by_key`], for keys that must exist (defaults, tests).
    pub fn expect(key: &str) -> WeaponId {
        Self::by_key(key).unwrap_or_else(|| panic!("weapon `{key}` is not in the table"))
    }

    pub fn all() -> impl Iterator<Item = WeaponId> {
        (0..table().len()).map(|i| WeaponId(i as u8))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub enum FireMode {
    Auto,
    Semi,
    /// A press fires `rounds` shots at the weapon's fire rate, even if the
    /// trigger is let go, then waits `delay` seconds before the next burst.
    Burst {
        rounds: u8,
        delay: f32,
    },
}

/// Which loadout slot a weapon goes in. Picking one up replaces that slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Slot {
    Primary,
    Secondary,
}

impl Slot {
    pub fn index(self) -> u8 {
        match self {
            Slot::Primary => 0,
            Slot::Secondary => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub enum Reload {
    /// Swap the whole magazine. The empty reload also chambers a round.
    Magazine { time: f32, empty_time: f32 },
    /// Load one round at a time (shotguns). Firing interrupts it.
    PerRound {
        start: f32,
        per_round: f32,
        end: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct Damage {
    pub near: f32,
    pub far: f32,
    /// Full damage up to this distance...
    pub falloff_start: f32,
    /// ...then linear falloff to `far` at this distance.
    pub falloff_end: f32,
    pub headshot: f32,
}

/// Cone half-angles in degrees.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct Spread {
    pub hip: f32,
    pub hip_moving: f32,
    pub air: f32,
    pub ads: f32,
}

/// Recoil kicks are in degrees: `x` is pitch up, `y` is yaw right.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct RecoilDef {
    /// Kick per consecutive shot; the last entry repeats.
    pub pattern: Vec<Vec2>,
    /// Random variation added to each kick (± degrees).
    pub jitter: Vec2,
    /// Kick multiplier while fully aimed down sights.
    pub ads_scale: f32,
    /// Seconds without firing before the view starts to settle.
    pub recovery_delay: f32,
    /// How quickly the view returns to the original aim, in 1/seconds.
    pub recovery_rate: f32,
}

/// A magnified optic: the HUD draws a scope overlay and the aim sways.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct ScopeDef {
    /// Sway amplitude in degrees while scoped and breathing normally.
    pub sway: f32,
    /// Seconds the player can hold their breath (steadies the scope).
    pub hold_breath: f32,
    /// Seconds of extra sway after running out of breath.
    pub breath_recovery: f32,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct WeaponDef {
    /// File name without extension. Set by the loader, not read from the file.
    #[serde(skip)]
    pub key: String,
    pub name: String,
    pub slot: Slot,
    pub fire_mode: FireMode,
    pub rounds_per_minute: f32,
    /// Seconds to work a bolt or pump after each shot. 0 for self-loading weapons.
    #[serde(default)]
    pub cycle_time: f32,
    /// Projectiles per shot. Shotguns fire several, each dealing full `damage`.
    #[serde(default = "one")]
    pub pellets: u8,
    /// Each pellet's cone half-angle in degrees, around the shot direction.
    #[serde(default)]
    pub pellet_spread: f32,
    pub magazine: u16,
    pub reserve: u16,
    pub damage: Damage,
    pub range: f32,
    pub reload: Reload,
    pub ads_time: f32,
    pub sprint_out_time: f32,
    pub raise_time: f32,
    pub lower_time: f32,
    pub spread: Spread,
    /// World FOV multiplier when fully aimed (lower zooms in).
    pub ads_fov_scale: f32,
    pub ads_move_scale: f32,
    pub move_scale: f32,
    pub recoil: RecoilDef,
    #[serde(default)]
    pub scope: Option<ScopeDef>,
    /// Draw a tracer for every Nth round of the player's own fire. 0 = never.
    #[serde(default)]
    pub tracer_every: u8,
    /// An energy weapon: every shot draws a solid beam to the hit point instead
    /// of tracers, and no shell casings come out.
    #[serde(default)]
    pub beam: bool,
}

fn one() -> u8 {
    1
}

impl WeaponDef {
    pub fn fire_interval(&self) -> f32 {
        60.0 / self.rounds_per_minute
    }

    /// Body-shot damage per pellet at a distance.
    pub fn damage_at(&self, distance: f32) -> f32 {
        let d = &self.damage;
        if distance <= d.falloff_start {
            return d.near;
        }
        if distance >= d.falloff_end {
            return d.far;
        }
        let t = (distance - d.falloff_start) / (d.falloff_end - d.falloff_start);
        d.near + (d.far - d.near) * t
    }

    /// Rounds below which the HUD warns and the weapon clicks.
    pub fn low_ammo(&self) -> u16 {
        (self.magazine / 4).max(1)
    }

    fn validate(&self) -> Result<(), String> {
        let mut problems = Vec::new();
        if self.rounds_per_minute <= 0.0 {
            problems.push("rounds_per_minute must be positive");
        }
        if self.magazine == 0 {
            problems.push("magazine must be at least 1");
        }
        if self.pellets == 0 {
            problems.push("pellets must be at least 1");
        }
        if self.recoil.pattern.is_empty() {
            problems.push("recoil.pattern needs at least one kick");
        }
        if self.ads_time <= 0.0 {
            problems.push("ads_time must be positive");
        }
        if self.damage.falloff_end < self.damage.falloff_start {
            problems.push("damage.falloff_end must not be before falloff_start");
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems.join("; "))
        }
    }
}

#[derive(Debug)]
pub struct LoadError {
    pub key: String,
    pub message: String,
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "weapon `{}`: {}", self.key, self.message)
    }
}

impl std::error::Error for LoadError {}

/// Parses one weapon file.
pub fn parse(key: &str, source: &str) -> Result<WeaponDef, LoadError> {
    let error = |message: String| LoadError {
        key: key.to_owned(),
        message,
    };
    let mut def: WeaponDef = ron::from_str(source).map_err(|e| error(e.to_string()))?;
    def.key = key.to_owned();
    def.validate().map_err(error)?;
    Ok(def)
}

/// Parses a set of `(key, source)` files into a table sorted by key.
pub fn parse_all<'a>(
    files: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<Vec<WeaponDef>, LoadError> {
    let mut defs = files
        .into_iter()
        .map(|(key, source)| parse(key, source))
        .collect::<Result<Vec<_>, _>>()?;
    defs.sort_by(|a, b| a.key.cmp(&b.key));
    if defs.len() > usize::from(u8::MAX) {
        return Err(LoadError {
            key: String::new(),
            message: "too many weapons".into(),
        });
    }
    Ok(defs)
}

macro_rules! builtin_files {
    ($($key:literal),* $(,)?) => {
        [$(($key, include_str!(concat!("../../../../godot/data/weapons/", $key, ".ron")))),*]
    };
}

/// The weapon files in `godot/data/weapons`, compiled in.
pub fn builtin() -> Vec<WeaponDef> {
    parse_all(builtin_files![
        "ak47", "br3", "laser", "lsw6", "m1911", "m40", "m4", "m870", "pdw9"
    ])
    .expect("built-in weapon files are valid")
}

static TABLE: LazyLock<RwLock<&'static [WeaponDef]>> =
    LazyLock::new(|| RwLock::new(builtin().leak()));

/// All weapons, indexed by [`WeaponId`].
pub fn table() -> &'static [WeaponDef] {
    *TABLE.read().unwrap_or_else(|e| e.into_inner())
}

/// Replaces the weapon table.
///
/// While the game is running only the numbers may change: the keys must match
/// the current table, so existing [`WeaponId`]s stay valid. Pass
/// `allow_new_keys` only before any player state exists (at startup). The old
/// table is leaked, since `&'static` borrows of it may still be alive; this
/// happens once at startup and on debug hot reloads only.
pub fn install(defs: Vec<WeaponDef>, allow_new_keys: bool) -> Result<(), LoadError> {
    let mut table = TABLE.write().unwrap_or_else(|e| e.into_inner());
    if !allow_new_keys {
        let same =
            table.len() == defs.len() && table.iter().zip(&defs).all(|(a, b)| a.key == b.key);
        if !same {
            return Err(LoadError {
                key: String::new(),
                message: "weapons were added or removed; restart to load them".into(),
            });
        }
    }
    *table = defs.leak();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_table_is_sorted_and_complete() {
        let keys: Vec<_> = table().iter().map(|d| d.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "ak47", "br3", "laser", "lsw6", "m1911", "m4", "m40", "m870", "pdw9"
            ]
        );
        assert_eq!(WeaponId::expect("m4").def().name, "M4A1");
    }

    #[test]
    fn damage_falls_off_linearly() {
        let m4 = WeaponId::expect("m4").def();
        assert_eq!(m4.damage_at(10.0), 30.0);
        assert_eq!(m4.damage_at(37.5), 25.5);
        assert_eq!(m4.damage_at(100.0), 21.0);
    }

    #[test]
    fn optional_fields_default() {
        let m4 = WeaponId::expect("m4").def();
        assert_eq!(m4.pellets, 1);
        assert_eq!(m4.cycle_time, 0.0);
        assert!(m4.scope.is_none());
        assert!(!m4.beam);
        let shotgun = WeaponId::expect("m870").def();
        assert_eq!(shotgun.pellets, 8);
        assert!(matches!(shotgun.reload, Reload::PerRound { .. }));
    }

    #[test]
    fn laser_kills_in_one_hit_anywhere() {
        let laser = WeaponId::expect("laser").def();
        assert!(laser.beam);
        assert_eq!(laser.fire_mode, FireMode::Auto);
        assert!(laser.damage_at(0.0) >= 1000.0);
        assert!(laser.damage_at(laser.range) >= 1000.0);
        assert_eq!(laser.spread.hip, 0.0);
        assert!(laser.recoil.pattern.iter().all(|kick| *kick == Vec2::ZERO));
    }

    #[test]
    fn invalid_files_are_rejected_with_the_key() {
        let error = parse("broken", "(name: \"x\")").unwrap_err();
        assert_eq!(error.key, "broken");
        let source = include_str!("../../../../godot/data/weapons/m4.ron")
            .replace("magazine: 30", "magazine: 0");
        let error = parse("m4", &source).unwrap_err();
        assert!(error.message.contains("magazine"), "{error}");
    }
}
