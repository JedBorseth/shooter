//! Scorestreaks: rewards earned with score in one life.
//!
//! Black Ops 2's scorestreaks (kept in Black Ops 3 and Black Ops 4):
//!
//! - A player picks three streaks ([`Loadout`]), shared by every class, as
//!   Black Ops 2's Scorestreaks menu is.
//! - Score earned since the last spawn counts toward them: kills, their
//!   bonuses, and objective play alike ([`StreakState::on_points`]). Kills
//!   made with a streak score points but do not count toward streaks
//!   (Black Ops 2's rule).
//! - Each streak is earned once the life's score reaches its cost. After the
//!   dearest one is earned the count starts again from nothing, so a long
//!   life earns them all again.
//! - Earned streaks are kept until used, death included, and are used one at
//!   a time from the selected one ([`StreakState::take`]).
//! - Dying resets the count.
//! - Hardline (Perk 1): score counts 20% more toward streaks, about 16.7%
//!   off every cost (Black Ops 2's Hardline, as the Call of Duty wiki gives it).
//!
//! The streaks this game has, Call of Duty's names kept in the code for
//! reference (in-game names are the generic military terms):
//!
//! | Streak | Cost | Black Ops 2 | What it does |
//! | --- | --- | --- | --- |
//! | UAV | 425 | UAV, 425 after patch 1.04 (350 at launch) | sweeps enemies onto the team's minimaps every 3 s for 30 s |
//! | Counter-UAV | 475 | Counter-UAV (sources give 400 to 600) | jams enemy minimaps for 30 s |
//! | Care Package | 550 | Care Package, 550 | a crate from a flyover with a random streak inside ([`crate::care_package`]) |
//! | Airstrike | 750 | Lightning Strike, 750 | three strikes on points marked on the map ([`Strikes`]) |
//! | Sentry Gun | 800 | Sentry Gun, 800 | an automated gun set down in front of the caller ([`crate::turret`]) |
//!
//! Every number here is an estimate where Black Ops 2's own files give none;
//! the docs list which (`docs/scorestreaks.md`).

use crate::TICK_RATE;
use crate::net::Tick;

/// A scorestreak.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Streak {
    Uav,
    CounterUav,
    CarePackage,
    Airstrike,
    SentryGun,
}

/// UAV: seconds it stays up, and seconds between its sweeps (Black Ops 2's
/// UAV lasts 30 s; the sweep interval is an estimate of its refresh).
pub const UAV_SECONDS: f32 = 30.0;
pub const UAV_SWEEP_SECONDS: f32 = 3.0;
/// Counter-UAV: seconds enemy minimaps stay jammed.
pub const COUNTER_UAV_SECONDS: f32 = 30.0;
/// Hardline: score counts this many tenths toward streaks (Black Ops 2:
/// +20%).
pub const HARDLINE_TENTHS: u32 = 12;
/// Most streaks a player holds at once; one earned beyond it is lost.
pub const MAX_HELD: usize = 8;

impl Streak {
    pub const ALL: [Streak; 5] = [
        Streak::Uav,
        Streak::CounterUav,
        Streak::CarePackage,
        Streak::Airstrike,
        Streak::SentryGun,
    ];

    /// Score to earn it.
    pub fn cost(self) -> u32 {
        match self {
            Streak::Uav => 425,
            Streak::CounterUav => 475,
            Streak::CarePackage => 550,
            Streak::Airstrike => 750,
            Streak::SentryGun => 800,
        }
    }

    /// The level it unlocks at: the UAV and Counter-UAV from the start, as
    /// Black Ops 2 hands every new player a set of streaks.
    pub fn unlock_level(self) -> u8 {
        match self {
            Streak::Uav | Streak::CounterUav => 1,
            Streak::CarePackage => 7,
            Streak::Airstrike => 13,
            Streak::SentryGun => 16,
        }
    }

    /// The key settings, scripts, and the wire use.
    pub fn key(self) -> &'static str {
        match self {
            Streak::Uav => "uav",
            Streak::CounterUav => "counter_uav",
            Streak::CarePackage => "care_package",
            Streak::Airstrike => "airstrike",
            Streak::SentryGun => "sentry_gun",
        }
    }

    pub fn by_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.key() == key)
    }

    /// The name players see.
    pub fn label(self) -> &'static str {
        match self {
            Streak::Uav => "UAV",
            Streak::CounterUav => "Counter-UAV",
            Streak::CarePackage => "Care Package",
            Streak::Airstrike => "Airstrike",
            Streak::SentryGun => "Sentry Gun",
        }
    }

    /// One line on what it does.
    pub fn summary(self) -> &'static str {
        match self {
            Streak::Uav => "Shows enemies on your team's minimap for 30 seconds.",
            Streak::CounterUav => "Jams enemy minimaps for 30 seconds.",
            Streak::CarePackage => "Drops a crate where you aim, with a random scorestreak inside.",
            Streak::Airstrike => "Calls three air strikes on points you mark on the map.",
            Streak::SentryGun => {
                "Sets down an automated gun that fires at enemies in front of it for four minutes."
            }
        }
    }

    /// Whether it comes down from the sky onto a point on the map: a crate
    /// from a flyover or strikes from a jet. A map roofed over (Warehouse)
    /// has nowhere for these to land, so they are left out there
    /// ([`Loadout::usable`]); the radar streaks still work.
    pub fn needs_open_sky(self) -> bool {
        matches!(self, Streak::CarePackage | Streak::Airstrike)
    }

    /// Whether it can be earned and called in on a map, `roofed` or not.
    pub fn usable(self, roofed: bool) -> bool {
        !(roofed && self.needs_open_sky())
    }

    /// Its index in [`Streak::ALL`], for the wire.
    pub fn code(self) -> u8 {
        self as u8
    }

    pub fn from_code(code: u8) -> Option<Self> {
        Self::ALL.get(usize::from(code)).copied()
    }

    /// Ticks it stays up once used (0 for streaks that act once).
    pub fn duration(self) -> Tick {
        let seconds = match self {
            Streak::Uav => UAV_SECONDS,
            Streak::CounterUav => COUNTER_UAV_SECONDS,
            Streak::CarePackage | Streak::Airstrike | Streak::SentryGun => 0.0,
        };
        seconds_to_ticks(seconds)
    }
}

pub fn seconds_to_ticks(seconds: f32) -> Tick {
    (seconds * TICK_RATE as f32).round() as Tick
}

/// The three streaks a player brings, in order of cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Loadout(pub [Option<Streak>; 3]);

impl Default for Loadout {
    /// A new player's: the UAV and the Counter-UAV.
    fn default() -> Self {
        Self([Some(Streak::Uav), Some(Streak::CounterUav), None])
    }
}

impl Loadout {
    pub const NONE: Loadout = Loadout([None; 3]);

    /// The loadout with the same streak listed once, sorted by cost; empty
    /// slots last.
    pub fn sorted(self) -> Self {
        let mut list: Vec<Streak> = Vec::new();
        for streak in self.0.into_iter().flatten() {
            if !list.contains(&streak) {
                list.push(streak);
            }
        }
        list.sort_by_key(|s| s.cost());
        let mut slots = [None; 3];
        for (slot, streak) in slots.iter_mut().zip(list) {
            *slot = Some(streak);
        }
        Self(slots)
    }

    /// The streaks, cheapest first.
    pub fn streaks(self) -> impl Iterator<Item = Streak> {
        self.sorted().0.into_iter().flatten()
    }

    /// The loadout without the streaks a map, `roofed` or not, cannot
    /// have: the count runs on the rest, so the dearest usable one closes
    /// the loop.
    pub fn usable(self, roofed: bool) -> Self {
        let mut slots = self.sorted().0;
        for slot in &mut slots {
            if slot.is_some_and(|s| !s.usable(roofed)) {
                *slot = None;
            }
        }
        Self(slots).sorted()
    }
}

/// One player's streak count and earned streaks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StreakState {
    /// Score toward the next streak this life (after Hardline's boost).
    pub progress: u32,
    /// Streaks earned and not yet used, oldest first.
    pub held: Vec<Streak>,
    /// Which of `held` the use button calls in.
    pub selected: usize,
    /// Hardline's boost not yet paid out as whole points, in tenths.
    remainder: u32,
}

impl StreakState {
    /// `points` of score earned this life (none from streak kills): counts
    /// toward `loadout`'s streaks. Returns the streaks earned, in order.
    pub fn on_points(&mut self, loadout: Loadout, points: u32, hardline: bool) -> Vec<Streak> {
        let mut earned = Vec::new();
        let streaks: Vec<Streak> = loadout.streaks().collect();
        let Some(top) = streaks.last().map(|s| s.cost()) else {
            return earned;
        };
        let tenths = points * if hardline { HARDLINE_TENTHS } else { 10 } + self.remainder;
        self.remainder = tenths % 10;
        let mut left = tenths / 10;
        // Reach each cost in turn; past the dearest, start again.
        while left > 0 {
            let before = self.progress;
            let step = (top - before).min(left);
            self.progress += step;
            left -= step;
            for streak in &streaks {
                if before < streak.cost() && self.progress >= streak.cost() {
                    earned.push(*streak);
                }
            }
            if self.progress >= top {
                self.progress = 0;
            }
        }
        for streak in &earned {
            self.hold(*streak);
        }
        earned
    }

    /// Adds an earned streak (or one from a care package) and selects it,
    /// as Black Ops 2 moves the selection to the newest.
    pub fn hold(&mut self, streak: Streak) {
        if self.held.len() >= MAX_HELD {
            return;
        }
        self.held.push(streak);
        self.selected = self.held.len() - 1;
    }

    /// The streak the use button would call in.
    pub fn selected(&self) -> Option<Streak> {
        self.held.get(self.selected).copied()
    }

    /// Moves the selection to the next held streak.
    pub fn cycle(&mut self) {
        if !self.held.is_empty() {
            self.selected = (self.selected + 1) % self.held.len();
        }
    }

    /// Selects held streak `index`, if there is one.
    pub fn select(&mut self, index: usize) {
        if index < self.held.len() {
            self.selected = index;
        }
    }

    /// Takes the selected streak out to call it in.
    pub fn take(&mut self) -> Option<Streak> {
        let streak = self.selected()?;
        self.held.remove(self.selected);
        self.selected = self.held.len().saturating_sub(1);
        Some(streak)
    }

    /// Death: the count starts again; earned streaks stay.
    pub fn on_death(&mut self) {
        self.progress = 0;
        self.remainder = 0;
    }

    /// The next streak the count is heading for, and its cost.
    pub fn next(&self, loadout: Loadout) -> Option<Streak> {
        loadout.streaks().find(|s| s.cost() > self.progress)
    }
}

/// Airstrike: how many points the caller marks, seconds from the call to
/// the first strike and between strikes, and each blast's reach and damage
/// (Black Ops 2's Lightning Strike drops three strikes in turn; the
/// timings and blast are estimates sized to kill anyone in the open near a
/// mark).
pub const AIRSTRIKE_TARGETS: usize = 3;
pub const STRIKE_FIRST_SECONDS: f32 = 3.0;
pub const STRIKE_GAP_SECONDS: f32 = 1.0;
pub const STRIKE_RADIUS: f32 = 9.0;
pub const STRIKE_DAMAGE: f32 = 250.0;
pub const STRIKE_OUTER_DAMAGE: f32 = 40.0;

/// One strike coming.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strike {
    pub owner: u8,
    pub at: crate::Vec3,
    pub tick: Tick,
}

/// The strikes called in and not yet landed.
#[derive(Clone, Debug, Default)]
pub struct Strikes {
    pending: Vec<Strike>,
}

impl Strikes {
    /// `owner` calls strikes onto `targets` (the first
    /// [`AIRSTRIKE_TARGETS`]) at `now`. Returns them, in the order they land.
    pub fn call(&mut self, owner: u8, targets: &[crate::Vec3], now: Tick) -> Vec<Strike> {
        let first = now + seconds_to_ticks(STRIKE_FIRST_SECONDS);
        let gap = seconds_to_ticks(STRIKE_GAP_SECONDS);
        let called: Vec<Strike> = targets
            .iter()
            .take(AIRSTRIKE_TARGETS)
            .enumerate()
            .map(|(i, &at)| Strike {
                owner,
                at,
                tick: first + gap * i as Tick,
            })
            .collect();
        self.pending.extend(&called);
        called
    }

    /// The strikes landing by `now`, taken off the list.
    pub fn due(&mut self, now: Tick) -> Vec<Strike> {
        let (due, later): (Vec<Strike>, Vec<Strike>) =
            self.pending.iter().partition(|s| s.tick <= now);
        self.pending = later;
        due
    }

    /// Every strike still coming is called off (a new match).
    pub fn clear(&mut self) {
        self.pending.clear();
    }
}

/// The UAV's and Counter-UAV's health: Black Ops 2's `level.u2_maxhealth`
/// (`_spyplane.gsc`). Bullets take off their damage; any rocket downs one
/// outright (its `rocketdamageoneshot` is the health plus one).
pub const AIRCRAFT_HEALTH: f32 = 700.0;
/// Radius of the sphere around an aircraft that shots and rockets meet, in
/// metres: about the drones' 7 m span, so a hit on the model counts.
pub const AIRCRAFT_HIT_RADIUS: f32 = 3.2;
/// Seconds an orbit takes: Black Ops 2 turns the UAV's rig clockwise once
/// a minute and the Counter-UAV's the other way every 40 s.
pub const UAV_LAP_SECONDS: f32 = 60.0;
pub const COUNTER_UAV_LAP_SECONDS: f32 = 40.0;
/// Score for shooting one down (Black Ops 2 publishes no figure; earlier
/// games paid 100, and one Black Ops 2 source gives 75).
pub const SHOT_DOWN_POINTS: u32 = 100;

/// The ring the UAVs and Counter-UAVs fly over a map: its centre (at the
/// UAVs' height) and the UAVs' radius. Black Ops 2 flies them round a rig
/// over the minimap's centre, the UAV about 4,000 to 5,000 units out and
/// the Counter-UAV a fifth closer in and higher; on our smaller maps the
/// ring is scaled to the map so a launcher reaches it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Orbit {
    pub centre: crate::Vec3,
    pub radius: f32,
}

impl Orbit {
    /// Metres the UAVs fly over the highest floor, and how much higher the
    /// Counter-UAVs fly (Black Ops 2's Counter-UAV rig is 400 units, 10 m,
    /// above the UAV's).
    pub const HEIGHT: f32 = 45.0;
    pub const COUNTER_RISE: f32 = 10.0;

    /// The ring over a map whose floor spans `min` to `max` (its spawn
    /// points, say): centred over it, the UAVs four fifths of the way to its
    /// far edge.
    pub fn over(min: crate::Vec3, max: crate::Vec3) -> Self {
        let half = (max.x - min.x).max(max.z - min.z) * 0.5;
        Self {
            centre: crate::Vec3::new(
                (min.x + max.x) * 0.5,
                max.y + Self::HEIGHT,
                (min.z + max.z) * 0.5,
            ),
            radius: (half * 0.8).clamp(20.0, 60.0),
        }
    }

    /// Where `streak`'s aircraft is at `tick` (fractional for drawing),
    /// called in at `from` and starting `phase` (a 16-bit angle) round the
    /// ring, and the yaw it faces (its nose toward Godot's -z at yaw 0).
    /// The UAV circles clockwise seen from above, the Counter-UAV the other
    /// way, closer in and higher.
    pub fn aircraft_at(
        &self,
        streak: Streak,
        from: Tick,
        phase: u16,
        tick: f64,
    ) -> (crate::Vec3, f32) {
        let (radius, rise, lap, turn) = match streak {
            Streak::CounterUav => (
                self.radius * 0.8,
                Self::COUNTER_RISE,
                COUNTER_UAV_LAP_SECONDS,
                1.0,
            ),
            _ => (self.radius, 0.0, UAV_LAP_SECONDS, -1.0),
        };
        let seconds = ((tick - f64::from(from)) / f64::from(crate::TICK_RATE)) as f32;
        let angle = f32::from(phase) / 65536.0 * std::f32::consts::TAU
            + turn * std::f32::consts::TAU * seconds / lap;
        let (sin, cos) = angle.sin_cos();
        let at = self.centre + crate::Vec3::new(cos * radius, rise, sin * radius);
        // Moving along the ring: turn * (-sin, cos).
        let (fx, fz) = (-turn * sin, turn * cos);
        (at, (-fx).atan2(-fz))
    }
}

/// Where along `from` to `to` (0 to 1) a segment first enters the sphere
/// at `centre` of `radius`, if it does.
pub fn segment_meets_sphere(
    from: crate::Vec3,
    to: crate::Vec3,
    centre: crate::Vec3,
    radius: f32,
) -> Option<f32> {
    let d = to - from;
    let f = from - centre;
    let a = d.length_squared();
    let c = f.length_squared() - radius * radius;
    if c <= 0.0 {
        return Some(0.0);
    }
    if a <= f32::EPSILON {
        return None;
    }
    let b = f.dot(d);
    let disc = b * b - a * c;
    if disc < 0.0 {
        return None;
    }
    let t = (-b - disc.sqrt()) / a;
    (0.0..=1.0).contains(&t).then_some(t)
}

/// A streak up in the sky: a UAV or a Counter-UAV.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Airborne {
    /// Which one, for shots and the wire.
    pub id: u16,
    pub streak: Streak,
    /// Who called it in, and their side (their team, or their own id
    /// without teams).
    pub owner: u8,
    pub side: u8,
    /// The tick it was called in, where round the ring it started (a
    /// 16-bit angle), and the tick it leaves.
    pub from: Tick,
    pub phase: u16,
    pub until: Tick,
    /// Damage taken ([`AIRCRAFT_HEALTH`] downs it).
    pub damage: f32,
    /// The tick a UAV sweeps next.
    next_sweep: Tick,
}

/// One downed by a hit ([`Sky::hurt`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Downed {
    pub air: Airborne,
    /// Where it was when it went down.
    pub at: crate::Vec3,
}

/// Every UAV and Counter-UAV up. Several of one kind on a side overlap,
/// each with its own clock: the side's minimap shows the effect while any
/// is up, as in Black Ops 2. Each flies the map's [`Orbit`] and can be shot
/// down.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sky {
    pub up: Vec<Airborne>,
    pub orbit: Orbit,
    next_id: u16,
}

impl Sky {
    /// Calls in `streak` (a UAV or Counter-UAV) for `owner` on `side` at
    /// `now`, starting `phase` round the ring. Streaks that do not stay up
    /// are ignored. Returns its id.
    pub fn call(
        &mut self,
        streak: Streak,
        owner: u8,
        side: u8,
        now: Tick,
        phase: u16,
    ) -> Option<u16> {
        if streak.duration() == 0 {
            return None;
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.up.push(Airborne {
            id,
            streak,
            owner,
            side,
            from: now,
            phase,
            until: now + streak.duration(),
            damage: 0.0,
            next_sweep: now,
        });
        Some(id)
    }

    /// Where aircraft `air` is at `tick`.
    pub fn position(&self, air: &Airborne, tick: Tick) -> crate::Vec3 {
        self.orbit
            .aircraft_at(air.streak, air.from, air.phase, f64::from(tick))
            .0
    }

    /// The first aircraft `hostile` picks that the segment `from` to `to`
    /// passes within `radius` of at `tick` ([`AIRCRAFT_HIT_RADIUS`] for a
    /// bullet, `equipment::ROCKET_AIRCRAFT_RADIUS` for a rocket's proximity
    /// fuse): its id and how far along (0 to 1).
    pub fn first_on_segment(
        &self,
        from: crate::Vec3,
        to: crate::Vec3,
        radius: f32,
        tick: Tick,
        hostile: impl Fn(&Airborne) -> bool,
    ) -> Option<(u16, f32)> {
        self.up
            .iter()
            .filter(|a| hostile(a))
            .filter_map(|a| {
                let t = segment_meets_sphere(from, to, self.position(a, tick), radius)?;
                Some((a.id, t))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// Aircraft `id` takes `damage` at `tick` (a rocket's is its whole
    /// health). Returns it, taken out of the sky, if that downs it.
    pub fn hurt(&mut self, id: u16, damage: f32, tick: Tick) -> Option<Downed> {
        let index = self.up.iter().position(|a| a.id == id)?;
        self.up[index].damage += damage;
        if self.up[index].damage <= AIRCRAFT_HEALTH {
            return None;
        }
        let air = self.up.remove(index);
        Some(Downed {
            at: self.position(&air, tick),
            air,
        })
    }

    /// Drops what has left by `now`. Returns whether anything did.
    pub fn expire(&mut self, now: Tick) -> bool {
        let before = self.up.len();
        self.up.retain(|a| a.until > now);
        self.up.len() != before
    }

    /// The sides whose UAV sweeps at `now`, each once however many UAVs it
    /// has up. Advances the sweep clocks.
    pub fn sweeps(&mut self, now: Tick) -> Vec<u8> {
        let interval = seconds_to_ticks(UAV_SWEEP_SECONDS);
        let mut sides = Vec::new();
        for air in self.up.iter_mut().filter(|a| a.streak == Streak::Uav) {
            if now >= air.next_sweep {
                air.next_sweep = now + interval;
                if !sides.contains(&air.side) {
                    sides.push(air.side);
                }
            }
        }
        sides
    }

    /// Whether `side` has a UAV up.
    pub fn uav(&self, side: u8) -> bool {
        self.up
            .iter()
            .any(|a| a.streak == Streak::Uav && a.side == side)
    }

    /// Whether an enemy Counter-UAV jams `side`'s minimaps.
    pub fn jammed(&self, side: u8) -> bool {
        self.up
            .iter()
            .any(|a| a.streak == Streak::CounterUav && a.side != side)
    }

    /// Everything up leaves (a new match or round).
    pub fn clear(&mut self) {
        self.up.clear();
    }

    /// The aircraft up at `tick`: each one's id, side, and position.
    pub fn aircraft(&self, tick: Tick) -> impl Iterator<Item = (&Airborne, crate::Vec3)> {
        self.up.iter().map(move |a| (a, self.position(a, tick)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_in_one_life_earns_each_streak_at_its_cost() {
        let loadout = Loadout::default();
        let mut state = StreakState::default();
        assert!(state.on_points(loadout, 400, false).is_empty());
        assert_eq!(state.next(loadout), Some(Streak::Uav));
        assert_eq!(state.on_points(loadout, 30, false), vec![Streak::Uav]);
        assert_eq!(state.progress, 430);
        assert_eq!(state.next(loadout), Some(Streak::CounterUav));
        assert_eq!(
            state.on_points(loadout, 100, false),
            vec![Streak::CounterUav]
        );
        // Past the dearest, the count starts again: 530 is 55 into the next lap.
        assert_eq!(state.progress, 55);
        assert_eq!(state.held, vec![Streak::Uav, Streak::CounterUav]);
        assert_eq!(state.selected(), Some(Streak::CounterUav));
    }

    #[test]
    fn one_big_award_can_earn_several_and_carry_over() {
        let loadout = Loadout([Some(Streak::CounterUav), Some(Streak::Uav), None]);
        let mut state = StreakState::default();
        let earned = state.on_points(loadout, 1000, false);
        // Two laps of 475, and 50 into a third.
        assert_eq!(
            earned,
            vec![
                Streak::Uav,
                Streak::CounterUav,
                Streak::Uav,
                Streak::CounterUav
            ]
        );
        assert_eq!(state.progress, 50);
    }

    #[test]
    fn death_resets_the_count_but_keeps_what_was_earned() {
        let loadout = Loadout::default();
        let mut state = StreakState::default();
        state.on_points(loadout, 450, false);
        state.on_death();
        assert_eq!(state.progress, 0);
        assert_eq!(state.held, vec![Streak::Uav]);
        assert_eq!(state.on_points(loadout, 300, false), vec![]);
    }

    #[test]
    fn hardline_counts_a_fifth_more() {
        let loadout = Loadout::default();
        let mut state = StreakState::default();
        // 355 * 1.2 = 426: a UAV a kill sooner.
        assert_eq!(state.on_points(loadout, 355, true), vec![Streak::Uav]);
        let mut slow = StreakState::default();
        assert!(slow.on_points(loadout, 355, false).is_empty());
        // Fractions carry over rather than being lost.
        let mut bits = StreakState::default();
        for _ in 0..10 {
            bits.on_points(loadout, 1, true);
        }
        assert_eq!(bits.progress, 12);
    }

    #[test]
    fn using_takes_the_selected_streak() {
        let mut state = StreakState::default();
        state.hold(Streak::Uav);
        state.hold(Streak::CounterUav);
        assert_eq!(state.selected(), Some(Streak::CounterUav));
        state.cycle();
        assert_eq!(state.selected(), Some(Streak::Uav));
        assert_eq!(state.take(), Some(Streak::Uav));
        assert_eq!(state.selected(), Some(Streak::CounterUav));
        assert_eq!(state.take(), Some(Streak::CounterUav));
        assert_eq!(state.take(), None);
    }

    #[test]
    fn a_loadout_sorts_by_cost_and_lists_each_streak_once() {
        let loadout = Loadout([
            Some(Streak::CounterUav),
            Some(Streak::Uav),
            Some(Streak::Uav),
        ]);
        assert_eq!(
            loadout.sorted(),
            Loadout([Some(Streak::Uav), Some(Streak::CounterUav), None])
        );
        let mut state = StreakState::default();
        assert!(state.on_points(Loadout::NONE, 1000, false).is_empty());
    }

    #[test]
    fn a_uav_sweeps_every_few_seconds_until_it_leaves() {
        let mut sky = Sky::default();
        sky.call(Streak::Uav, 3, 1, 1000, 0);
        assert!(sky.uav(1) && !sky.uav(0));
        assert_eq!(sky.sweeps(1000), vec![1]);
        assert!(sky.sweeps(1001).is_empty());
        let interval = seconds_to_ticks(UAV_SWEEP_SECONDS);
        assert_eq!(sky.sweeps(1000 + interval), vec![1]);
        // A second UAV on the side does not sweep twice.
        sky.call(Streak::Uav, 4, 1, 1000 + interval, 0);
        assert_eq!(sky.sweeps(1000 + interval), vec![1]);
        assert!(!sky.expire(1000 + Streak::Uav.duration() - 1));
        assert!(sky.expire(1000 + Streak::Uav.duration()));
        assert!(sky.uav(1));
        assert!(sky.expire(1000 + interval + Streak::Uav.duration()));
        assert!(!sky.uav(1));
    }

    #[test]
    fn a_counter_uav_jams_only_the_other_sides() {
        let mut sky = Sky::default();
        sky.call(Streak::CounterUav, 3, 1, 0, 0);
        assert!(sky.jammed(0));
        assert!(!sky.jammed(1));
        sky.expire(Streak::CounterUav.duration());
        assert!(!sky.jammed(0));
    }

    #[test]
    fn aircraft_circle_the_map_each_its_own_way() {
        let orbit = Orbit::over(
            crate::Vec3::new(-40.0, 0.0, -30.0),
            crate::Vec3::new(40.0, 4.0, 30.0),
        );
        assert_eq!(orbit.centre, crate::Vec3::new(0.0, 49.0, 0.0));
        assert_eq!(orbit.radius, 32.0);
        let lap = f64::from(seconds_to_ticks(UAV_LAP_SECONDS));
        let (start, _) = orbit.aircraft_at(Streak::Uav, 0, 0, 0.0);
        assert!((start - crate::Vec3::new(32.0, 49.0, 0.0)).length() < 1e-3);
        // A quarter lap on, clockwise seen from above (y up): toward -z.
        let (quarter, yaw) = orbit.aircraft_at(Streak::Uav, 0, 0, lap / 4.0);
        assert!((quarter - crate::Vec3::new(0.0, 49.0, -32.0)).length() < 1e-2);
        // Heading toward -x there: nose (-sin yaw, -cos yaw).
        assert!((-yaw.sin() + 1.0).abs() < 1e-3 && yaw.cos().abs() < 1e-3);
        let (back, _) = orbit.aircraft_at(Streak::Uav, 0, 0, lap);
        assert!((back - start).length() < 1e-2);
        // The Counter-UAV flies closer in, higher, and the other way.
        let cuav_lap = f64::from(seconds_to_ticks(COUNTER_UAV_LAP_SECONDS));
        let (c, _) = orbit.aircraft_at(Streak::CounterUav, 0, 0, cuav_lap / 4.0);
        assert!((c - crate::Vec3::new(0.0, 59.0, 25.6)).length() < 1e-2);
        // A phase of half a turn starts on the far side.
        let (far, _) = orbit.aircraft_at(Streak::Uav, 0, 32768, 0.0);
        assert!((far - crate::Vec3::new(-32.0, 49.0, 0.0)).length() < 1e-3);
    }

    #[test]
    fn bullets_wear_an_aircraft_down_and_a_rocket_downs_it() {
        let mut sky = Sky {
            orbit: Orbit {
                centre: crate::Vec3::new(0.0, 50.0, 0.0),
                radius: 30.0,
            },
            ..Sky::default()
        };
        let uav = sky.call(Streak::Uav, 3, 1, 0, 0).unwrap();
        let cuav = sky.call(Streak::CounterUav, 4, 1, 0, 32768).unwrap();
        let at = sky.position(&sky.up[0], 0);
        // A shot from the ground straight up at it meets it; one wide does not.
        let eye = crate::Vec3::new(at.x, 0.0, at.z);
        let up = crate::Vec3::new(at.x, 100.0, at.z);
        assert_eq!(
            sky.first_on_segment(eye, up, AIRCRAFT_HIT_RADIUS, 0, |_| true)
                .map(|h| h.0),
            Some(uav)
        );
        let wide = crate::Vec3::new(at.x + 5.0, 100.0, at.z);
        assert!(
            sky.first_on_segment(
                eye + crate::Vec3::X * 5.0,
                wide,
                AIRCRAFT_HIT_RADIUS,
                0,
                |_| true
            )
            .is_none()
        );
        assert!(
            sky.first_on_segment(eye, up, AIRCRAFT_HIT_RADIUS, 0, |a| a.side != 1)
                .is_none()
        );
        // 700 health: 23 bullets of 30 leave it up, the 24th downs it.
        for _ in 0..23 {
            assert!(sky.hurt(uav, 30.0, 0).is_none());
        }
        let downed = sky.hurt(uav, 30.0, 10).unwrap();
        assert_eq!(downed.air.owner, 3);
        assert!(!sky.uav(1));
        // A rocket's damage is the whole health plus one.
        assert!(sky.hurt(cuav, AIRCRAFT_HEALTH + 1.0, 0).is_some());
        assert!(sky.up.is_empty());
    }

    #[test]
    fn strikes_land_in_turn_after_the_call() {
        let mut strikes = Strikes::default();
        let targets = [
            crate::Vec3::X,
            crate::Vec3::Y,
            crate::Vec3::Z,
            crate::Vec3::ONE,
        ];
        let called = strikes.call(4, &targets, 100);
        assert_eq!(called.len(), AIRSTRIKE_TARGETS);
        assert_eq!(called[0].tick, 100 + 360);
        assert_eq!(called[2].tick, 100 + 360 + 240);
        assert!(strikes.due(459).is_empty());
        assert_eq!(strikes.due(460), vec![called[0]]);
        assert_eq!(strikes.due(1000).len(), 2);
        assert!(strikes.due(2000).is_empty());
    }

    #[test]
    fn a_roofed_map_leaves_out_the_streaks_that_need_open_sky() {
        let loadout = Loadout([
            Some(Streak::Uav),
            Some(Streak::CarePackage),
            Some(Streak::Airstrike),
        ]);
        assert_eq!(loadout.usable(false), loadout);
        let roofed = loadout.usable(true);
        assert_eq!(roofed, Loadout([Some(Streak::Uav), None, None]));
        // The count loops on the UAV alone.
        let mut state = StreakState::default();
        assert_eq!(
            state.on_points(roofed, 900, false),
            vec![Streak::Uav, Streak::Uav]
        );
        assert_eq!(state.progress, 50);
        assert!(Streak::CounterUav.usable(true));
        assert!(!Streak::Airstrike.usable(true));
    }

    #[test]
    fn keys_and_codes_round_trip() {
        for streak in Streak::ALL {
            assert_eq!(Streak::by_key(streak.key()), Some(streak));
            assert_eq!(Streak::from_code(streak.code()), Some(streak));
        }
        assert_eq!(Streak::Uav.duration(), 3600);
    }
}
