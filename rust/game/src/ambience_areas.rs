//! Area-aware soundscape configs and sparse off-map industrial scheduling.
//!
//! Call of Duty multiplayer maps with contrasting outdoor halves (such as
//! CoD4's Vacant and modern industrial maps like Hackney Yard and Atlas
//! Superstore) assign each outdoor zone its own ambience bed and area-specific
//! one-shots, cross-fade them smoothly as the player moves between halves, and
//! keep distant off-map industrial work sparse enough that it reads as
//! atmosphere rather than a tactical cue.
//!
//! Pure geometry and scheduling rules live here so they are covered by fast
//! Rust unit tests without a running Godot tree.

/// Axis-aligned horizontal rectangle in world metres (X east/west, Z north/south).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect2D {
    pub min_x: f32,
    pub max_x: f32,
    pub min_z: f32,
    pub max_z: f32,
}

impl Rect2D {
    pub const fn new(min_x: f32, max_x: f32, min_z: f32, max_z: f32) -> Self {
        Self {
            min_x,
            max_x,
            min_z,
            max_z,
        }
    }

    pub fn contains(self, x: f32, z: f32) -> bool {
        x >= self.min_x && x <= self.max_x && z >= self.min_z && z <= self.max_z
    }

    /// Distance from `(x, z)` to the nearest point inside or on the rectangle
    /// (0 when inside).
    pub fn distance_to(self, x: f32, z: f32) -> f32 {
        let dx = if x < self.min_x {
            self.min_x - x
        } else if x > self.max_x {
            x - self.max_x
        } else {
            0.0
        };
        let dz = if z < self.min_z {
            self.min_z - z
        } else if z > self.max_z {
            z - self.max_z
        } else {
            0.0
        };
        (dx * dx + dz * dz).sqrt()
    }
}

/// Positional outdoor one-shot belonging to a specific map area.
#[derive(Clone, Copy, Debug)]
pub struct AreaOneShot {
    /// File stem under `res://assets/audio/ambience/<stem>_N.ogg`.
    pub stem: &'static str,
    /// Playback gain in dB (distance-baked recordings, `AttenuationModel::DISABLED`).
    pub volume_db: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    pub rise_y: f32,
    /// When set, this area one-shot shares cooldown tracking with the off-map
    /// industrial category (e.g. reversing alarms never exceed once per ~2 min).
    pub shared_category: Option<IndustrialCategory>,
}

/// One outdoor zone on a multi-area map (its bed and area-specific one-shots).
#[derive(Clone, Copy, Debug)]
pub struct OutdoorAreaConfig {
    pub id: &'static str,
    pub bed: &'static str,
    pub bed_db: f32,
    pub rects: &'static [Rect2D],
    /// Optional non-positional wind gust played on the outdoor bus.
    pub gust_stem: Option<&'static str>,
    /// Probability `[0, 1]` that an outdoor one-shot tick plays a gust rather
    /// than a positional area sound.
    pub gust_probability: f32,
    /// Gust level offset relative to `bed_db`.
    pub gust_offset_db: f32,
    pub positional_one_shots: &'static [AreaOneShot],
}

impl OutdoorAreaConfig {
    pub fn contains(&self, x: f32, z: f32) -> bool {
        self.rects.iter().any(|r| r.contains(x, z))
    }

    pub fn distance_to(&self, x: f32, z: f32) -> f32 {
        self.rects
            .iter()
            .map(|r| r.distance_to(x, z))
            .fold(f32::INFINITY, f32::min)
    }
}

/// A roof opening inside an enclosed hall that lets faint sky wind bleed in.
#[derive(Clone, Copy, Debug)]
pub struct RoofBreachConfig {
    pub rect: Rect2D,
    /// Metres outside the opening over which the wind bleed tapers to zero.
    pub falloff_m: f32,
    pub bed: &'static str,
    pub bed_db: f32,
    /// Low-pass cutoff lifted toward this frequency (Hz) under the opening so
    /// the muffled exterior bed also opens up subtly overhead.
    pub cutoff_lift_hz: f32,
}

impl RoofBreachConfig {
    /// Smooth 0..1 overhead wind bleed factor, gated by how far indoors the
    /// listener is (outdoors the full outdoor bed is already open).
    pub fn weight(&self, x: f32, z: f32, indoors: f32) -> f32 {
        let indoors = indoors.clamp(0.0, 1.0);
        if indoors <= 0.001 || self.falloff_m <= 0.0 {
            return 0.0;
        }
        let d = self.rect.distance_to(x, z);
        if d >= self.falloff_m {
            return 0.0;
        }
        let u = 1.0 - (d / self.falloff_m);
        // Smoothstep avoids a hard edge when walking past the breach rim.
        let smooth = u * u * (3.0 - 2.0 * u);
        smooth * indoors
    }
}

/// Complete area-aware preset for a map.
#[derive(Clone, Copy, Debug)]
pub struct MapSoundscapePreset {
    pub indoor_bed: &'static str,
    pub indoor_db: f32,
    pub areas: &'static [OutdoorAreaConfig],
    pub roof_breach: Option<RoofBreachConfig>,
    pub industrial_off_map: bool,
}

impl MapSoundscapePreset {
    /// Selects the active outdoor area index for world position `(x, z)`.
    /// Inside an area's rectangles wins; outside all explicit rectangles (for
    /// example inside the hall) chooses the nearest outdoor area so the
    /// muffled exterior bleed smoothly shifts from the north Field to the south
    /// Docks as the player crosses the hall.
    pub fn select_area(&self, x: f32, z: f32) -> usize {
        if let Some((idx, _)) = self
            .areas
            .iter()
            .enumerate()
            .find(|(_, area)| area.contains(x, z))
        {
            return idx;
        }
        self.areas
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.distance_to(x, z).total_cmp(&b.distance_to(x, z)))
            .map_or(0, |(idx, _)| idx)
    }
}

/// Direction arc for off-map industrial one-shots relative to the listener.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectionBias {
    /// Any direction around the plant perimeter.
    Any,
    /// Biased toward the south truck apron / adjacent yard (+Z, roughly SE to SW).
    SouthDocks,
    /// Biased away from the open north meadow (east, south, or west plant sides).
    PlantSides,
}

/// Distinct off-map working-industrial one-shot categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum IndustrialCategory {
    Hammer = 0,
    Shout = 1,
    ReverseAlarm = 2,
    ForkliftHorn = 3,
    MetalDrop = 4,
    ImpactWrench = 5,
    RollerDoor = 6,
    PaChime = 7,
}

impl IndustrialCategory {
    pub const ALL: [IndustrialCategory; 8] = [
        Self::Hammer,
        Self::Shout,
        Self::ReverseAlarm,
        Self::ForkliftHorn,
        Self::MetalDrop,
        Self::ImpactWrench,
        Self::RollerDoor,
        Self::PaChime,
    ];
    pub const COUNT: usize = Self::ALL.len();

    pub const fn stem(self) -> &'static str {
        match self {
            Self::Hammer => "ind_hammer",
            Self::Shout => "ind_shout",
            Self::ReverseAlarm => "ind_reverse_alarm",
            Self::ForkliftHorn => "ind_forklift_horn",
            Self::MetalDrop => "ind_metal_drop",
            Self::ImpactWrench => "ind_impact_wrench",
            Self::RollerDoor => "ind_roller_door",
            Self::PaChime => "ind_pa_chime",
        }
    }

    /// Playback level (dB) outdoors before indoor wall muffling. Kept low
    /// enough that footsteps and gunfire always dominate (`docs/audio.md`).
    pub const fn volume_db(self) -> f32 {
        match self {
            Self::Hammer => -21.0,
            Self::Shout => -23.0,
            Self::ReverseAlarm => -22.0,
            Self::ForkliftHorn => -22.0,
            Self::MetalDrop => -20.5,
            Self::ImpactWrench => -22.0,
            Self::RollerDoor => -21.0,
            Self::PaChime => -23.0,
        }
    }

    /// Minimum seconds before this same category may trigger again.
    pub const fn cooldown_s(self) -> f32 {
        match self {
            Self::Shout => INDUSTRIAL_SHOUT_COOLDOWN_S,
            Self::ReverseAlarm => INDUSTRIAL_REVERSE_ALARM_COOLDOWN_S,
            _ => INDUSTRIAL_CATEGORY_COOLDOWN_S,
        }
    }

    pub const fn direction_bias(self) -> DirectionBias {
        match self {
            Self::ReverseAlarm => DirectionBias::SouthDocks,
            Self::ForkliftHorn | Self::RollerDoor => DirectionBias::PlantSides,
            _ => DirectionBias::Any,
        }
    }
}

// --- Off-map industrial scheduling constants ---------------------------------
// Call of Duty industrial maps (Modern Warfare's Hackney Yard and Atlas
// Superstore) keep off-map work noise sparse and far beyond the perimeter so
// it sells a live industrial complex without ever reading as a combat cue or
// becoming fatiguing over a 10-minute match.

/// Grace period at match / scene start before any off-map industrial one-shot
/// can play, keeping countdowns, match-start stings, and opening sprints clean.
pub const INDUSTRIAL_START_GRACE_MIN_S: f32 = 12.0;
pub const INDUSTRIAL_START_GRACE_MAX_S: f32 = 24.0;

/// Random gap between consecutive off-map industrial one-shots (seconds).
pub const INDUSTRIAL_INTERVAL_MIN_S: f32 = 25.0;
pub const INDUSTRIAL_INTERVAL_MAX_S: f32 = 60.0;

/// Minimum time before the same industrial category can play again (seconds).
pub const INDUSTRIAL_CATEGORY_COOLDOWN_S: f32 = 90.0;

/// Distant muffled worker shouting is kept rare: no more than once every
/// 2.5 minutes (120–180 s target) so it never competes with combat awareness.
pub const INDUSTRIAL_SHOUT_COOLDOWN_S: f32 = 150.0;

/// Reversing alarms are kept to at most once every 2 minutes so they never
/// grate on players spawning on the Docks.
pub const INDUSTRIAL_REVERSE_ALARM_COOLDOWN_S: f32 = 120.0;

/// Off-map emitter distance range (metres) beyond the perimeter walls.
pub const INDUSTRIAL_DISTANCE_MIN_M: f32 = 30.0;
pub const INDUSTRIAL_DISTANCE_MAX_M: f32 = 80.0;

/// Extra attenuation (dB) applied to off-map industrial one-shots when indoors,
/// on top of the outdoor bus's 650 Hz low-pass filter.
pub const INDUSTRIAL_INDOOR_ATTEN_DB: f32 = 5.5;

/// Output of one scheduled off-map industrial cue.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScheduledIndustrialCue {
    pub category: IndustrialCategory,
    /// Horizontal offset from the listener (metres), 30–80 m out.
    pub offset_x: f32,
    pub offset_y: f32,
    pub offset_z: f32,
}

/// Deterministic scheduler for Warehouse's sparse off-map industrial layer.
#[derive(Clone, Debug)]
pub struct IndustrialScheduler {
    elapsed_s: f32,
    next_trigger_s: f32,
    last_category: Option<IndustrialCategory>,
    last_played_at: [Option<f32>; IndustrialCategory::COUNT],
}

impl IndustrialScheduler {
    /// Creates a scheduler whose first cue waits at least
    /// `INDUSTRIAL_START_GRACE_MIN_S` seconds. `initial_roll` is in `[0, 1]`.
    pub fn new(initial_roll: f32) -> Self {
        let r = initial_roll.clamp(0.0, 1.0);
        Self {
            elapsed_s: 0.0,
            next_trigger_s: lerp(
                INDUSTRIAL_START_GRACE_MIN_S,
                INDUSTRIAL_START_GRACE_MAX_S,
                r,
            ),
            last_category: None,
            last_played_at: [None; IndustrialCategory::COUNT],
        }
    }

    #[cfg(test)]
    pub fn elapsed_s(&self) -> f32 {
        self.elapsed_s
    }

    /// True if `category` is allowed to play right now: not an immediate repeat
    /// and past its per-category cooldown.
    pub fn can_play(&self, category: IndustrialCategory) -> bool {
        if self.last_category == Some(category) {
            return false;
        }
        match self.last_played_at[category as usize] {
            Some(at) => (self.elapsed_s - at) >= category.cooldown_s(),
            None => true,
        }
    }

    /// Records that `category` played now (also called when a Docks outdoor
    /// one-shot plays a shared category such as `ReverseAlarm`).
    pub fn record_played(&mut self, category: IndustrialCategory) {
        self.last_category = Some(category);
        self.last_played_at[category as usize] = Some(self.elapsed_s);
    }

    /// All categories currently eligible to play.
    pub fn eligible(&self) -> Vec<IndustrialCategory> {
        IndustrialCategory::ALL
            .into_iter()
            .filter(|&c| self.can_play(c))
            .collect()
    }

    /// Advances the clock by `dt` seconds. When due and the single industrial
    /// voice is idle, picks an eligible category (with dock-side weighting for
    /// truck/forklift sounds when `on_dock_side` is true) and schedules the
    /// next trigger 25–60 s later.
    ///
    /// All `*_roll` parameters are unit random values in `[0, 1)`.
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        dt: f32,
        voice_busy: bool,
        on_dock_side: bool,
        pick_roll: f32,
        interval_roll: f32,
        angle_roll: f32,
        dist_roll: f32,
    ) -> Option<ScheduledIndustrialCue> {
        self.elapsed_s += dt.max(0.0);
        if self.elapsed_s < self.next_trigger_s || voice_busy {
            return None;
        }
        let candidates = self.eligible();
        if candidates.is_empty() {
            // All categories on cooldown (should not happen with 8 categories
            // and 25–60 s gaps); check again shortly.
            self.next_trigger_s = self.elapsed_s + 5.0;
            return None;
        }
        let category = weighted_pick(&candidates, on_dock_side, pick_roll.clamp(0.0, 0.999999));
        self.record_played(category);
        self.next_trigger_s = self.elapsed_s
            + lerp(
                INDUSTRIAL_INTERVAL_MIN_S,
                INDUSTRIAL_INTERVAL_MAX_S,
                interval_roll.clamp(0.0, 1.0),
            );
        let (offset_x, offset_y, offset_z) = off_map_offset(
            category.direction_bias(),
            angle_roll.clamp(0.0, 1.0),
            dist_roll.clamp(0.0, 1.0),
        );
        Some(ScheduledIndustrialCue {
            category,
            offset_x,
            offset_y,
            offset_z,
        })
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn category_weight(category: IndustrialCategory, on_dock_side: bool) -> f32 {
    match category {
        IndustrialCategory::ReverseAlarm => {
            if on_dock_side {
                1.8
            } else {
                0.55
            }
        }
        IndustrialCategory::ForkliftHorn => {
            if on_dock_side {
                1.4
            } else {
                0.85
            }
        }
        IndustrialCategory::Shout => 0.7,
        _ => 1.0,
    }
}

fn weighted_pick(
    candidates: &[IndustrialCategory],
    on_dock_side: bool,
    roll: f32,
) -> IndustrialCategory {
    let total: f32 = candidates
        .iter()
        .map(|&c| category_weight(c, on_dock_side))
        .sum();
    let mut target = roll * total;
    for &c in candidates {
        let w = category_weight(c, on_dock_side);
        if target < w {
            return c;
        }
        target -= w;
    }
    *candidates.last().expect("candidates non-empty")
}

/// Computes a 3D offset 30–80 m out in the direction arc appropriate for the
/// cue (angle 0 = +Z south/docks, PI/2 = +X east, PI = -Z north/field).
pub fn off_map_offset(bias: DirectionBias, angle_roll: f32, dist_roll: f32) -> (f32, f32, f32) {
    use std::f32::consts::{FRAC_PI_2, PI, TAU};
    let angle = match bias {
        DirectionBias::Any => angle_roll * TAU,
        // South arc: -65 deg to +65 deg around +Z (the dock yard side).
        DirectionBias::SouthDocks => lerp(-1.13, 1.13, angle_roll),
        // East, south, or west plant sides (-120 deg to +120 deg around +Z).
        DirectionBias::PlantSides => lerp(-PI * 0.67, PI * 0.67, angle_roll),
    };
    let distance = lerp(
        INDUSTRIAL_DISTANCE_MIN_M,
        INDUSTRIAL_DISTANCE_MAX_M,
        dist_roll,
    );
    // Slight elevation keeps off-map sources above ground horizon.
    let _ = FRAC_PI_2;
    (angle.sin() * distance, 3.0, angle.cos() * distance)
}

/// Computes the outdoor low-pass cutoff (Hz) given `indoors` (0..1) and
/// `breach_weight` (0..1). Deep inside the hall the filter closes to
/// `indoor_hz`; standing under the open roof breach lifts it smoothly toward
/// `breach_lift_hz`.
pub fn outdoor_cutoff_with_breach(
    open_hz: f32,
    indoor_hz: f32,
    indoors: f32,
    breach_weight: f32,
    breach_lift_hz: f32,
) -> f32 {
    let indoors = indoors.clamp(0.0, 1.0);
    let base = open_hz * (indoor_hz / open_hz).powf(indoors);
    let breach = breach_weight.clamp(0.0, 1.0);
    if breach <= 0.001 || breach_lift_hz <= base {
        return base;
    }
    base * (breach_lift_hz / base).powf(breach)
}

// --- Warehouse map soundscape preset ----------------------------------------
// World coordinates from tools/map_blockout/warehouse_2026_11b.py and
// warehouse_photo_layout.py (origin offset CX = 13.5, CZ = 3.5):
// - THE FIELD (north spawn): x -35.5..17.5, z -37.5..-26.8, plus demolished
//   west wing x -35.5..-17.7, z -26.8..-8.5.
// - THE DOCKS (south spawn): concrete apron x -17..44, z 33..42 at y = -1.25.
// - THE HALL: x -17..44, z -26..33 under a 9.3 m roof with a 12x12 m roof
//   breach at x 7.5..19.5, z -2.5..9.5.

const WAREHOUSE_FIELD_RECTS: &[Rect2D] = &[
    // North grass lot (with slight margin to yard boundary)
    Rect2D::new(-36.5, 18.0, -38.5, -26.0),
    // Demolished west wing grass lot
    Rect2D::new(-36.5, -17.0, -26.8, -8.0),
];

const WAREHOUSE_DOCKS_RECTS: &[Rect2D] = &[
    // Lowered concrete truck apron and dock bay thresholds
    Rect2D::new(-18.0, 45.0, 32.5, 43.5),
];

const WAREHOUSE_FIELD_ONE_SHOTS: &[AreaOneShot] = &[
    AreaOneShot {
        stem: "field_fence_rattle",
        volume_db: -21.0,
        min_distance: 12.0,
        max_distance: 26.0,
        rise_y: 1.5,
        shared_category: None,
    },
    AreaOneShot {
        stem: "field_roof_bang",
        volume_db: -21.5,
        min_distance: 14.0,
        max_distance: 28.0,
        rise_y: 4.5,
        shared_category: None,
    },
    AreaOneShot {
        stem: "field_crow",
        volume_db: -22.0,
        min_distance: 18.0,
        max_distance: 36.0,
        rise_y: 8.0,
        shared_category: None,
    },
];

const WAREHOUSE_DOCKS_ONE_SHOTS: &[AreaOneShot] = &[
    AreaOneShot {
        stem: "docks_trailer_creak",
        volume_db: -21.0,
        min_distance: 10.0,
        max_distance: 24.0,
        rise_y: 2.0,
        shared_category: None,
    },
    AreaOneShot {
        stem: "docks_pallet_drop",
        volume_db: -21.5,
        min_distance: 14.0,
        max_distance: 28.0,
        rise_y: 0.5,
        shared_category: None,
    },
    AreaOneShot {
        stem: "docks_air_brakes",
        volume_db: -21.0,
        min_distance: 14.0,
        max_distance: 30.0,
        rise_y: 1.5,
        shared_category: None,
    },
    AreaOneShot {
        stem: "ind_reverse_alarm",
        volume_db: -22.5,
        min_distance: 22.0,
        max_distance: 40.0,
        rise_y: 2.0,
        shared_category: Some(IndustrialCategory::ReverseAlarm),
    },
];

const WAREHOUSE_AREAS: &[OutdoorAreaConfig] = &[
    OutdoorAreaConfig {
        id: "field",
        bed: "warehouse_field",
        bed_db: -15.5,
        rects: WAREHOUSE_FIELD_RECTS,
        gust_stem: Some("field_gust"),
        gust_probability: 0.45,
        gust_offset_db: 2.0,
        positional_one_shots: WAREHOUSE_FIELD_ONE_SHOTS,
    },
    OutdoorAreaConfig {
        id: "docks",
        bed: "warehouse_docks",
        bed_db: -15.0,
        rects: WAREHOUSE_DOCKS_RECTS,
        gust_stem: None,
        gust_probability: 0.0,
        gust_offset_db: 0.0,
        positional_one_shots: WAREHOUSE_DOCKS_ONE_SHOTS,
    },
];

pub const WAREHOUSE_PRESET: MapSoundscapePreset = MapSoundscapePreset {
    indoor_bed: "warehouse_hall",
    indoor_db: -14.0,
    areas: WAREHOUSE_AREAS,
    roof_breach: Some(RoofBreachConfig {
        rect: Rect2D::new(7.5, 19.5, -2.5, 9.5),
        falloff_m: 5.0,
        bed: "warehouse_breach_wind",
        bed_db: -20.5,
        cutoff_lift_hz: 1_500.0,
    }),
    industrial_off_map: true,
};

pub fn preset_by_name(name: &str) -> Option<&'static MapSoundscapePreset> {
    match name {
        "warehouse" => Some(&WAREHOUSE_PRESET),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warehouse_area_selection_matches_field_and_docks_coordinates() {
        let p = preset_by_name("warehouse").expect("warehouse preset exists");
        // North Field spawns (e.g. local (-7, -36) => world (6.5, -32.5))
        assert_eq!(p.areas[p.select_area(6.5, -32.5)].id, "field");
        assert_eq!(p.areas[p.select_area(-30.0, -34.0)].id, "field");
        // Demolished west wing Field
        assert_eq!(p.areas[p.select_area(-26.0, -18.0)].id, "field");
        // South Docks apron (e.g. local (13, 37.5) => world (26.5, 41.0))
        assert_eq!(p.areas[p.select_area(26.5, 41.0)].id, "docks");
        assert_eq!(p.areas[p.select_area(-5.0, 36.0)].id, "docks");
        // Inside hall: nearest outdoor area wins so bleed shifts smoothly
        assert_eq!(p.areas[p.select_area(5.0, -20.0)].id, "field");
        assert_eq!(p.areas[p.select_area(10.0, 28.0)].id, "docks");
    }

    #[test]
    fn roof_breach_weight_peaks_under_opening_and_fades_cleanly() {
        let breach = WAREHOUSE_PRESET
            .roof_breach
            .expect("warehouse has roof breach");
        // Directly under the breach, indoors
        assert!((breach.weight(13.5, 3.5, 1.0) - 1.0).abs() < 1e-5);
        // Outdoors: no extra breach bleed needed
        assert_eq!(breach.weight(13.5, 3.5, 0.0), 0.0);
        // Halfway through the 5 m falloff outside the east edge (x = 19.5 + 2.5 = 22.0)
        let mid = breach.weight(22.0, 3.5, 1.0);
        assert!(mid > 0.45 && mid < 0.55, "mid falloff {mid}");
        // Beyond falloff (x = 25.0)
        assert_eq!(breach.weight(25.0, 3.5, 1.0), 0.0);
    }

    #[test]
    fn roof_breach_lifts_indoor_cutoff_only_under_opening() {
        let closed = outdoor_cutoff_with_breach(20_000.0, 650.0, 1.0, 0.0, 1_500.0);
        assert!((closed - 650.0).abs() < 1.0);
        let under_hole = outdoor_cutoff_with_breach(20_000.0, 650.0, 1.0, 1.0, 1_500.0);
        assert!((under_hole - 1_500.0).abs() < 1.0);
        let outdoors = outdoor_cutoff_with_breach(20_000.0, 650.0, 0.0, 0.0, 1_500.0);
        assert!((outdoors - 20_000.0).abs() < 1.0);
    }

    #[test]
    fn industrial_scheduler_respects_start_grace_and_single_voice() {
        let mut sched = IndustrialScheduler::new(0.0);
        // Nothing plays during the first 10+ seconds of a match.
        for _ in 0..11 {
            assert!(sched.tick(1.0, false, false, 0.5, 0.5, 0.5, 0.5).is_none());
        }
        // Even once due, a busy voice defers playback (single voice budget).
        assert!(sched.tick(2.0, true, false, 0.5, 0.5, 0.5, 0.5).is_none());
        // As soon as the voice is free, it fires.
        let cue = sched.tick(0.1, false, false, 0.5, 0.5, 0.5, 0.5);
        assert!(cue.is_some());
    }

    #[test]
    fn industrial_scheduler_enforces_intervals_no_repeats_and_cooldowns() {
        let mut sched = IndustrialScheduler::new(0.0);
        let mut played: Vec<(f32, IndustrialCategory)> = Vec::new();
        // Simulate a full 20-minute match (1200 s) in 1-second steps with varied rolls.
        for step in 0..1200 {
            let f = step as f32;
            let pick = ((step * 37) % 100) as f32 / 100.0;
            let interval = ((step * 53) % 100) as f32 / 100.0;
            let angle = ((step * 19) % 100) as f32 / 100.0;
            let dist = ((step * 71) % 100) as f32 / 100.0;
            if let Some(cue) = sched.tick(1.0, false, step % 2 == 0, pick, interval, angle, dist) {
                let d = (cue.offset_x * cue.offset_x + cue.offset_z * cue.offset_z).sqrt();
                assert!(
                    (INDUSTRIAL_DISTANCE_MIN_M - 0.01..=INDUSTRIAL_DISTANCE_MAX_M + 0.01)
                        .contains(&d),
                    "off-map distance out of 30..80 m range: {d} (step {f})"
                );
                played.push((sched.elapsed_s(), cue.category));
            }
        }
        // Expect sparse cadence: roughly 1 every 25..60 s over ~1188 s => 20..48 events.
        assert!(
            played.len() >= 18 && played.len() <= 50,
            "unexpected event count over 20 min: {}",
            played.len()
        );
        // Check first event waited past start grace.
        assert!(played[0].0 >= INDUSTRIAL_START_GRACE_MIN_S);

        // Check consecutive gaps, no back-to-back repeats, and per-category cooldowns.
        for pair in played.windows(2) {
            let (t0, c0) = pair[0];
            let (t1, c1) = pair[1];
            let gap = t1 - t0;
            assert!(
                (INDUSTRIAL_INTERVAL_MIN_S - 0.01..=INDUSTRIAL_INTERVAL_MAX_S + 1.01)
                    .contains(&gap),
                "gap {gap} outside [25, 60] s"
            );
            assert_ne!(
                c0, c1,
                "same industrial sound played twice in a row: {c0:?}"
            );
        }

        let mut last_seen: [Option<f32>; IndustrialCategory::COUNT] =
            [None; IndustrialCategory::COUNT];
        for (t, cat) in played {
            if let Some(prev) = last_seen[cat as usize] {
                let delta = t - prev;
                assert!(
                    delta >= cat.cooldown_s() - 0.01,
                    "{cat:?} repeated after {delta} s (cooldown {} s)",
                    cat.cooldown_s()
                );
            }
            last_seen[cat as usize] = Some(t);
        }
    }

    #[test]
    fn reverse_alarm_is_dock_biased_and_shares_two_minute_cooldown() {
        let mut sched = IndustrialScheduler::new(0.0);
        // Advance to t = 50 s and simulate a Docks outdoor reversing alarm playing.
        let _ = sched.tick(50.0, true, true, 0.0, 0.0, 0.0, 0.0);
        sched.record_played(IndustrialCategory::ReverseAlarm);
        assert!(!sched.can_play(IndustrialCategory::ReverseAlarm));
        // Still blocked 100 s later (< 120 s).
        let _ = sched.tick(100.0, true, true, 0.0, 0.0, 0.0, 0.0);
        assert!(!sched.can_play(IndustrialCategory::ReverseAlarm));
        // Allowed once 120 s have passed AND another category has played (no immediate repeat).
        let _ = sched.tick(25.0, true, true, 0.0, 0.0, 0.0, 0.0);
        sched.record_played(IndustrialCategory::Hammer);
        assert!(sched.can_play(IndustrialCategory::ReverseAlarm));

        // SouthDocks bias always places the sound in the +Z (dock) half-plane.
        for i in 0..10 {
            let (_, _, oz) = off_map_offset(DirectionBias::SouthDocks, i as f32 / 9.0, 0.5);
            assert!(oz > 0.0, "dock-biased sound had non-south oz={oz}");
        }
    }

    #[test]
    fn shout_cooldown_is_two_to_three_minutes() {
        assert!(
            (120.0..=180.0).contains(&IndustrialCategory::Shout.cooldown_s()),
            "shout cooldown should be 2–3 minutes"
        );
        let mut sched = IndustrialScheduler::new(0.0);
        let _ = sched.tick(20.0, true, false, 0.0, 0.0, 0.0, 0.0);
        sched.record_played(IndustrialCategory::Shout);
        sched.record_played(IndustrialCategory::Hammer);
        let _ = sched.tick(119.0, true, false, 0.0, 0.0, 0.0, 0.0);
        assert!(!sched.can_play(IndustrialCategory::Shout));
        let _ = sched.tick(35.0, true, false, 0.0, 0.0, 0.0, 0.0);
        assert!(sched.can_play(IndustrialCategory::Shout));
    }
}
