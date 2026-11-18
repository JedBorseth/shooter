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
    /// `mag_in` is the second (from the start of either reload) the fresh
    /// magazine seats and its rounds count; a reload cut short after that
    /// keeps them. Without it, 60% of the tactical reload.
    Magazine {
        time: f32,
        empty_time: f32,
        #[serde(default)]
        mag_in: Option<f32>,
    },
    /// Load one round at a time (shotguns). Firing interrupts it.
    PerRound {
        start: f32,
        per_round: f32,
        end: f32,
    },
}

impl Reload {
    fn mag_in_of(time: f32, mag_in: Option<f32>) -> f32 {
        mag_in.unwrap_or(time * 0.6)
    }

    /// Seconds into a magazine reload when the fresh magazine seats.
    pub fn mag_in(&self) -> Option<f32> {
        match *self {
            Reload::Magazine { time, mag_in, .. } => Some(Reload::mag_in_of(time, mag_in)),
            Reload::PerRound { .. } => None,
        }
    }

    /// Total seconds of a magazine reload (`empty` or tactical).
    pub fn magazine_time(&self, empty: bool) -> Option<f32> {
        match *self {
            Reload::Magazine {
                time, empty_time, ..
            } => Some(if empty { empty_time } else { time }),
            Reload::PerRound { .. } => None,
        }
    }
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
    /// Hip-fire bloom: degrees added to the hip cone per shot, up to
    /// `bloom_max`, shrinking at `bloom_recovery` degrees a second.
    #[serde(default)]
    pub bloom: f32,
    #[serde(default)]
    pub bloom_max: f32,
    #[serde(default = "default_bloom_recovery")]
    pub bloom_recovery: f32,
    /// Idle sway while aimed, in degrees (scopes use [`ScopeDef::sway`]).
    #[serde(default = "default_ads_sway")]
    pub ads_sway: f32,
}

fn default_bloom_recovery() -> f32 {
    6.0
}

fn default_ads_sway() -> f32 {
    0.05
}

/// Damage multipliers for hits that are not on the head (which uses
/// [`Damage::headshot`]). The chest always counts 1.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct Zones {
    pub stomach: f32,
    pub arm: f32,
    pub leg: f32,
}

impl Default for Zones {
    fn default() -> Self {
        Self {
            stomach: 1.0,
            arm: 0.9,
            leg: 0.85,
        }
    }
}

/// Recoil kicks are in degrees: `x` is pitch up, `y` is yaw right.
///
/// Each kick splits in two. Most of it climbs: the view turns and stays
/// turned, as if the player had moved the mouse, so pulling down against a
/// spray holds the aim where the player put it. The rest (`kick_return`) is a
/// snap that settles back at `recovery_rate` once `recovery_delay` has passed
/// since the shot. Both are part of the aim, so the centre of the screen is
/// always where the next round goes.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct RecoilDef {
    /// Kick per consecutive shot; the last entry repeats.
    pub pattern: Vec<Vec2>,
    /// Random variation added to each kick (± degrees).
    pub jitter: Vec2,
    /// Kick multiplier while fully aimed down sights.
    pub ads_scale: f32,
    /// Fraction of each kick that settles back after the shot.
    #[serde(default = "default_kick_return")]
    pub kick_return: f32,
    /// Seconds after a shot before its snap starts to settle.
    pub recovery_delay: f32,
    /// How quickly the snap settles, in 1/seconds.
    pub recovery_rate: f32,
    /// Kick multiplier while crouched.
    #[serde(default = "default_crouch_scale")]
    pub crouch_scale: f32,
}

fn default_kick_return() -> f32 {
    0.35
}

fn default_crouch_scale() -> f32 {
    0.85
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

/// Something fitted to a primary weapon in a loadout. Each weapon file lists
/// the attachments it takes; the table then holds a variant of the weapon for
/// each (`m4+grip`), with its numbers changed by [`Attachment::apply`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
pub enum Attachment {
    /// A reflex sight in place of the iron sights.
    RedDot,
    /// A 2.5x combat scope.
    Acog,
    /// Quiet, no muzzle flash or tracers, shorter range.
    Suppressor,
    /// A vertical foregrip: a quarter less recoil.
    Grip,
    /// Half again as many rounds, a slightly slower reload.
    ExtendedMag,
}

impl Attachment {
    pub const ALL: [Attachment; 5] = [
        Attachment::RedDot,
        Attachment::Acog,
        Attachment::Suppressor,
        Attachment::Grip,
        Attachment::ExtendedMag,
    ];

    /// Lower-case key, used in variant keys (`m4+grip`) and scripts.
    pub fn key(self) -> &'static str {
        match self {
            Attachment::RedDot => "reddot",
            Attachment::Acog => "acog",
            Attachment::Suppressor => "suppressor",
            Attachment::Grip => "grip",
            Attachment::ExtendedMag => "extmag",
        }
    }

    pub fn from_key(key: &str) -> Option<Attachment> {
        Attachment::ALL.into_iter().find(|a| a.key() == key)
    }

    /// The name players see.
    pub fn label(self) -> &'static str {
        match self {
            Attachment::RedDot => "Red Dot",
            Attachment::Acog => "ACOG",
            Attachment::Suppressor => "Suppressor",
            Attachment::Grip => "Grip",
            Attachment::ExtendedMag => "Ext. Mag",
        }
    }

    /// The weapon with this attachment fitted.
    pub fn apply(self, base: &WeaponDef) -> WeaponDef {
        let mut def = base.clone();
        def.key = format!("{}+{}", base.key, self.key());
        def.name = format!("{} {}", base.name, self.label());
        def.attachment = Some(self);
        def.base_key = base.key.clone();
        def.attachments = Vec::new();
        match self {
            Attachment::RedDot => {
                // A clear sight picture aims a touch faster than irons.
                def.ads_time *= 0.95;
            }
            Attachment::Acog => {
                // A magnified prism sight: the whole view zooms, and the
                // weapon and the optic's housing stay on screen around the
                // eyepiece (no scope overlay), with the weapon's own aimed
                // sway, which the zoom magnifies.
                def.ads_fov_scale = (def.ads_fov_scale * 0.55).min(0.45);
                def.ads_time += 0.06;
                def.ads_move_scale *= 0.9;
            }
            Attachment::Suppressor => {
                def.damage.falloff_start *= 0.7;
                def.damage.falloff_end *= 0.75;
                def.tracer_every = 0;
            }
            Attachment::Grip => {
                for kick in &mut def.recoil.pattern {
                    *kick *= 0.75;
                }
                def.recoil.jitter *= 0.75;
            }
            Attachment::ExtendedMag => {
                def.magazine = (f32::from(def.magazine) * 1.5).round() as u16;
                def.reload = match def.reload {
                    Reload::Magazine {
                        time,
                        empty_time,
                        mag_in,
                    } => Reload::Magazine {
                        time: time + 0.2,
                        empty_time: empty_time + 0.2,
                        // The longer magazine takes a moment more to line up.
                        mag_in: Some(Reload::mag_in_of(time, mag_in) + 0.15),
                    },
                    other => other,
                };
            }
        }
        def
    }
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
    /// Body-zone multipliers; defaults to chest and stomach 1, arms 0.9, legs 0.85.
    #[serde(default)]
    pub zones: Zones,
    pub range: f32,
    /// Metres of wood a round can go through (see `crate::penetration`).
    /// 0 stops at the first wall.
    #[serde(default)]
    pub penetration: f32,
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
    /// Attachments this weapon takes in a loadout.
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    /// Where a third-person soldier's hands hold the weapon: its scene's
    /// `GripRight` and `GripLeft` markers and grip rake. The arm hit zones
    /// follow them (`crate::hitbox`).
    #[serde(default)]
    pub grips: Option<GripsDef>,
    /// On an attachment variant: the attachment fitted.
    #[serde(skip)]
    pub attachment: Option<Attachment>,
    /// On an attachment variant: the plain weapon's key (its model and sounds).
    #[serde(skip)]
    pub base_key: String,
}

/// Hand positions on a weapon model, in its space (origin at the rear sight,
/// -Z toward the muzzle, metres).
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct GripsDef {
    /// Centre of the firing hand's grip.
    pub right: [f32; 3],
    /// Where the support hand holds.
    pub left: [f32; 3],
    /// Degrees the pistol grip leans back from vertical.
    pub rake: f32,
}

fn one() -> u8 {
    1
}

/// Seconds after the seat until a tactical reload can be fired out of.
pub const RELOAD_READY_TACTICAL: f32 = 0.45;
/// Seconds after the seat until an empty reload (release or charge, then
/// the re-shoulder) can be fired out of.
pub const RELOAD_READY_EMPTY: f32 = 0.8;

impl WeaponDef {
    /// The key of the weapon whose model and sounds this one uses: itself, or
    /// the plain weapon for an attachment variant.
    pub fn model_key(&self) -> &str {
        if self.base_key.is_empty() {
            &self.key
        } else {
            &self.base_key
        }
    }

    pub fn fire_interval(&self) -> f32 {
        60.0 / self.rounds_per_minute
    }

    /// Seconds into a magazine reload (`empty` or tactical) from which the
    /// weapon is back on the shoulder with a round chambered: firing then
    /// ends the reload, cutting its settle short. A tactical reload is ready
    /// soon after the seat; an empty one after the bolt release or charge; a
    /// bolt action only at its end, after working the bolt.
    pub fn reload_ready_at(&self, empty: bool) -> Option<f32> {
        let total = self.reload.magazine_time(empty)?;
        let seat = self.reload.mag_in()?;
        let ready = match (empty, self.cycle_time > 0.0) {
            (false, _) => seat + RELOAD_READY_TACTICAL,
            (true, false) => seat + RELOAD_READY_EMPTY,
            (true, true) => total,
        };
        Some(ready.min(total))
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

    /// Damage multiplier for a hit on `zone`.
    pub fn zone_scale(&self, zone: crate::hitbox::HitZone) -> f32 {
        use crate::hitbox::HitZone;
        match zone {
            HitZone::Head => self.damage.headshot,
            HitZone::Chest => 1.0,
            HitZone::Stomach => self.zones.stomach,
            HitZone::Arm => self.zones.arm,
            HitZone::Leg => self.zones.leg,
        }
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
        if let Some(mag_in) = self.reload.mag_in()
            && let Reload::Magazine {
                time, empty_time, ..
            } = self.reload
            && !(0.0..time.min(empty_time)).contains(&mag_in)
        {
            problems.push("reload.mag_in must come before the end of both reloads");
        }
        if !(0.0..=1.0).contains(&self.recoil.kick_return) {
            problems.push("recoil.kick_return must be between 0 and 1");
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
    let variants: Vec<WeaponDef> = defs
        .iter()
        .flat_map(|def| def.attachments.iter().map(move |a| a.apply(def)))
        .collect();
    defs.extend(variants);
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
        let keys: Vec<_> = table()
            .iter()
            .filter(|d| d.attachment.is_none())
            .map(|d| d.key.as_str())
            .collect();
        assert_eq!(
            keys,
            [
                "ak47", "br3", "laser", "lsw6", "m1911", "m4", "m40", "m870", "pdw9"
            ]
        );
        let all: Vec<_> = table().iter().map(|d| d.key.as_str()).collect();
        let mut sorted = all.clone();
        sorted.sort_unstable();
        assert_eq!(all, sorted);
        assert_eq!(WeaponId::expect("m4").def().name, "M4A1");
    }

    #[test]
    fn attachments_make_variants_with_changed_numbers() {
        let m4 = WeaponId::expect("m4").def();
        let grip = WeaponId::expect("m4+grip").def();
        assert_eq!(grip.name, "M4A1 Grip");
        assert_eq!(grip.model_key(), "m4");
        assert_eq!(grip.attachment, Some(Attachment::Grip));
        assert!(grip.recoil.pattern[0].x < m4.recoil.pattern[0].x);
        let mag = WeaponId::expect("m4+extmag").def();
        assert_eq!(mag.magazine, 45);
        let acog = WeaponId::expect("ak47+acog").def();
        assert!(acog.scope.is_none() && acog.ads_fov_scale < 0.5);
        let quiet = WeaponId::expect("m4+suppressor").def();
        assert_eq!(quiet.tracer_every, 0);
        assert!(quiet.damage.falloff_end < m4.damage.falloff_end);
        // No red dot on weapons that already have one, nor optics on the sniper.
        assert!(WeaponId::by_key("pdw9+reddot").is_none());
        assert!(WeaponId::by_key("m40+acog").is_none());
        assert!(WeaponId::by_key("m1911+grip").is_none());
        assert!(table().len() < 64);
    }

    #[test]
    fn damage_falls_off_linearly() {
        let m4 = WeaponId::expect("m4").def();
        assert_eq!(m4.damage_at(10.0), 30.0);
        assert_eq!(m4.damage_at(37.5), 25.5);
        assert_eq!(m4.damage_at(100.0), 21.0);
    }

    #[test]
    fn zone_multipliers_default_and_set_identity() {
        use crate::hitbox::HitZone;
        let m4 = WeaponId::expect("m4").def();
        assert_eq!(m4.zone_scale(HitZone::Head), m4.damage.headshot);
        assert_eq!(m4.zone_scale(HitZone::Chest), 1.0);
        assert_eq!(m4.zone_scale(HitZone::Stomach), 1.0);
        assert!(m4.zone_scale(HitZone::Leg) < m4.zone_scale(HitZone::Arm));
        assert!(m4.zone_scale(HitZone::Arm) < 1.0);
        // The sniper kills in one shot only to the head or upper chest.
        let m40 = WeaponId::expect("m40").def();
        assert!(m40.damage_at(50.0) * m40.zone_scale(HitZone::Chest) >= 100.0);
        assert!(m40.damage_at(50.0) * m40.zone_scale(HitZone::Stomach) < 100.0);
        assert!(m40.damage_at(50.0) * m40.zone_scale(HitZone::Leg) < 100.0);
    }

    #[test]
    fn every_weapon_seats_its_magazine_before_the_reload_ends() {
        for def in table() {
            if let (Some(mag_in), Some(time)) =
                (def.reload.mag_in(), def.reload.magazine_time(false))
            {
                assert!(mag_in > 0.3 * time && mag_in < 0.8 * time, "{}", def.key);
            }
        }
    }

    #[test]
    fn optional_fields_default() {
        let m4 = WeaponId::expect("m4").def();
        assert_eq!(m4.pellets, 1);
        assert_eq!(m4.cycle_time, 0.0);
        assert!(m4.scope.is_none());
        assert!(!m4.beam);
        let shotgun = WeaponId::expect("m870").def();
        assert_eq!(shotgun.pellets, 10);
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
