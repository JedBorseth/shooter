//! Equipment: the frag grenade (lethal), and the flashbang and smoke
//! grenade (tacticals).
//!
//! Modern Call of Duty rules (Modern Warfare 2019 onward): the lethal sits on
//! its own button (G, right bumper). Pressing it pulls the pin and lowers the
//! weapon; holding it cooks the grenade, whose fuse is already burning;
//! letting go throws it overhand after a short wind-up. A grenade cooked to
//! the end of its fuse goes off in the hand. A player who dies with the pin
//! out drops the live grenade at their feet.
//!
//! The throw is part of the player's predicted state ([`crate::weapon::Action::Throwing`]):
//! the client predicts the pin, the cook, and the release exactly as the
//! server runs them. The grenade's flight after that is the server's (online)
//! or the client's (offline): [`Grenade::step`] moves it at the tick rate
//! with gravity, bounces off whatever the engine reports in the way
//! ([`Collide`], like `MoveEnv` for movement), rolls, and settles. The same
//! contacts always give the same flight, so a client can draw its own throw
//! from the release and hand over to the server's grenade when it arrives.
//!
//! When the fuse runs out, everyone within [`FRAG_RADIUS`] whose body the
//! blast can see takes damage that falls off with distance ([`frag_damage`]).
//! No friendly fire, but the thrower's own grenade hurts them. Counts follow
//! each mode ([`spawn_lethals`], [`spawn_tacticals`]).
//!
//! Tacticals go on a second button (Q, left bumper) and do not cook: their
//! fuse starts when they leave the hand. A flashbang blinds and deafens
//! whoever it can see, more the closer they are and the more they face it
//! ([`flash_amount`]); it hurts no one. A smoke grenade pops into a cloud
//! that grows to [`SMOKE_RADIUS`], hangs for [`SMOKE_SECONDS`], and blocks
//! sight through it ([`smoke_blocks`]).

use glam::Vec3;

use crate::look;
use crate::tdm::Mode;

/// A kind of equipment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Kind {
    #[default]
    Frag,
    Flash,
    Smoke,
    /// A launcher's rocket (`WeaponDef::rocket`): it flies straight from the
    /// tube and goes off on whatever it meets, or at the end of its flight.
    /// Not a class item; it rides the grenades' flight, snapshots, and blast.
    Rocket,
}

impl Kind {
    /// The tacticals a loadout can pick from.
    pub const TACTICALS: [Kind; 2] = [Kind::Flash, Kind::Smoke];

    /// The wire and save code.
    pub fn code(self) -> u8 {
        match self {
            Kind::Frag => 0,
            Kind::Flash => 1,
            Kind::Smoke => 2,
            Kind::Rocket => 3,
        }
    }

    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Kind::Frag),
            1 => Some(Kind::Flash),
            2 => Some(Kind::Smoke),
            3 => Some(Kind::Rocket),
            _ => None,
        }
    }

    /// The key settings and loadouts use.
    pub fn key(self) -> &'static str {
        match self {
            Kind::Frag => "frag",
            Kind::Flash => "flash",
            Kind::Smoke => "smoke",
            Kind::Rocket => "rocket",
        }
    }

    pub fn by_key(key: &str) -> Option<Self> {
        [Kind::Frag, Kind::Flash, Kind::Smoke]
            .into_iter()
            .find(|kind| kind.key() == key)
    }

    /// The name the kill feed and menus show.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Frag => "Frag",
            Kind::Flash => "Flashbang",
            Kind::Smoke => "Smoke",
            Kind::Rocket => "Rocket",
        }
    }

    /// Lethal (the lethal button) rather than tactical.
    pub fn lethal(self) -> bool {
        self == Kind::Frag
    }

    /// Whether its fuse burns in the hand once the pin is out. Tacticals'
    /// fuses start when they leave it.
    pub fn cooks(self) -> bool {
        self == Kind::Frag
    }

    /// Seconds from the pin (cooking) or the release (tacticals) to going off.
    pub fn fuse(self) -> f32 {
        match self {
            Kind::Frag => FRAG_FUSE,
            Kind::Flash => FLASH_FUSE,
            Kind::Smoke => SMOKE_FUSE,
            Kind::Rocket => ROCKET_FLIGHT,
        }
    }
}

/// A rocket's speed out of the tube (m/s), how fast it falls (m/s², a
/// slow sag), and the most seconds it flies before it goes off in the air
/// (Black Ops 2's RPG self-destructs at the end of its flight).
pub const ROCKET_SPEED: f32 = 65.0;
pub const ROCKET_DROP: f32 = 1.5;
pub const ROCKET_FLIGHT: f32 = 4.0;
/// How near a body's axis (metres) a rocket passing it strikes it, and the
/// body's height from the feet.
pub const ROCKET_BODY_RADIUS: f32 = 0.4;
pub const ROCKET_BODY_HEIGHT: f32 = 1.8;
/// A rocket's blast: full damage within the inner radius (a direct hit kills
/// through Flak Jacket's share as well), falling to the edge's, none beyond.
/// Black Ops 2's RPG kills outright near the blast and wounds further out;
/// a full-health player dies within about 4.1 m.
pub const ROCKET_INNER_RADIUS: f32 = 1.5;
pub const ROCKET_RADIUS: f32 = 6.0;
pub const ROCKET_MAX_DAMAGE: f32 = 200.0;
pub const ROCKET_MIN_DAMAGE: f32 = 30.0;
/// How near a scorestreak aircraft's centre (metres) a rocket passing it
/// brings it down: one hit downs any of them, as in Black Ops 2.
pub const ROCKET_AIRCRAFT_RADIUS: f32 = 6.0;

/// Seconds from leaving the hand to a flashbang's bang.
pub const FLASH_FUSE: f32 = 1.5;
/// Seconds from leaving the hand to a smoke grenade popping.
pub const SMOKE_FUSE: f32 = 1.0;
/// A flashbang reaches this far (metres).
pub const FLASH_RANGE: f32 = 20.0;
/// Most seconds a flashbang blinds for (at its strongest), and least.
pub const FLASH_MAX_SECONDS: f32 = 4.0;
pub const FLASH_MIN_SECONDS: f32 = 0.6;
/// Share of the flash that still lands with the flash straight behind.
pub const FLASH_BEHIND: f32 = 0.3;
/// A smoke cloud's full radius (metres), how long it takes to grow to it,
/// how long the cloud hangs, and how long it takes to thin away at the end.
pub const SMOKE_RADIUS: f32 = 5.0;
pub const SMOKE_BLOOM: f32 = 2.0;
pub const SMOKE_SECONDS: f32 = 16.0;
pub const SMOKE_THIN: f32 = 3.0;
/// The cloud's centre sits this far above where the grenade lies.
pub const SMOKE_HEIGHT: f32 = 1.6;

/// How hard a flashbang `distance` metres away hits someone whose view is
/// `facing` (the cosine between their view and the way to the flash), from
/// 0 to 1. Nothing past [`FLASH_RANGE`]; the caller checks the flash is in
/// plain view. Facing it lands it all, turned fully away about a third.
pub fn flash_amount(distance: f32, facing: f32) -> f32 {
    if !distance.is_finite() || distance >= FLASH_RANGE {
        return 0.0;
    }
    // Full strength out to about a third of the range, then falling away.
    let far = (distance / FLASH_RANGE).clamp(0.0, 1.0);
    let near = 1.0 - far * far;
    // Anywhere in the middle of the view lands it all.
    let facing = ((facing.clamp(-1.0, 1.0) + 0.2) / 0.7).clamp(0.0, 1.0);
    (near * (FLASH_BEHIND + (1.0 - FLASH_BEHIND) * facing)).clamp(0.0, 1.0)
}

/// Seconds a flash of `amount` (0 to 1) blinds for.
pub fn flash_seconds(amount: f32) -> f32 {
    if amount <= 0.0 {
        return 0.0;
    }
    FLASH_MIN_SECONDS + (FLASH_MAX_SECONDS - FLASH_MIN_SECONDS) * amount.clamp(0.0, 1.0)
}

/// A smoke cloud's radius `age` seconds after it popped: growing over
/// [`SMOKE_BLOOM`], full, then thinning away over the last [`SMOKE_THIN`]
/// seconds. Zero once it has gone.
pub fn smoke_radius(age: f32) -> f32 {
    if !(0.0..SMOKE_SECONDS).contains(&age) {
        return 0.0;
    }
    let grow = (age / SMOKE_BLOOM).clamp(0.0, 1.0);
    let grow = 1.0 - (1.0 - grow) * (1.0 - grow);
    let thin = ((SMOKE_SECONDS - age) / SMOKE_THIN).clamp(0.0, 1.0);
    SMOKE_RADIUS * grow * (0.4 + 0.6 * thin)
}

/// Whether a smoke cloud at `center`, `age` seconds old, blocks the line
/// from `from` to `to`: the line passes through its thick middle, or either
/// end is inside it.
pub fn smoke_blocks(center: Vec3, age: f32, from: Vec3, to: Vec3) -> bool {
    let radius = smoke_radius(age);
    if radius <= 0.5 {
        return false;
    }
    let thick = radius * 0.8;
    if from.distance(center) < thick || to.distance(center) < thick {
        return true;
    }
    let line = to - from;
    let length = line.length();
    if length < 1e-4 {
        return false;
    }
    let along = ((center - from).dot(line) / (length * length)).clamp(0.0, 1.0);
    (from + line * along).distance(center) < thick
}

/// Seconds from the pin to the blast. Modern Call of Duty frags run about
/// three seconds; cooking spends them in the hand.
pub const FRAG_FUSE: f32 = 3.0;
/// Seconds the pin pull takes: a tap still waits this long before the arm
/// comes forward.
pub const PIN_TIME: f32 = 0.3;
/// Seconds from letting go of the button to the grenade leaving the hand
/// (the arm coming over). The fuse keeps burning through it.
pub const WIND_UP: f32 = 0.15;
/// Seconds from the release until the weapon is back up and can fire.
pub const RECOVERY: f32 = 0.45;

/// Share of a throw's progress ([`throw_progress`]) reached when the pin is
/// out and the grenade is held, cocked; it holds there while cooking.
pub const COCKED_PROGRESS: f32 = 0.45;

/// How far along a throw is, 0 to 1, from the seconds since the pin was
/// pulled and the seconds until the release (`Action::Throwing`): up to
/// [`COCKED_PROGRESS`] over the pin pull, held while cooking, then on to 1
/// through the wind-up, the release, and the follow-through. Everyone draws
/// the throw from it, the first-person arms and other players' bodies alike.
pub fn throw_progress(cooked: f32, release_in: Option<f32>) -> f32 {
    match release_in {
        None => COCKED_PROGRESS * (cooked / PIN_TIME).clamp(0.0, 1.0),
        Some(left) => {
            let since = (WIND_UP - left).max(0.0);
            COCKED_PROGRESS + (1.0 - COCKED_PROGRESS) * (since / (WIND_UP + RECOVERY)).min(1.0)
        }
    }
}

/// [`throw_progress`] at the moment the grenade leaves the hand.
pub fn release_progress() -> f32 {
    throw_progress(PIN_TIME, Some(0.0))
}

/// Launch speed of an overhand throw, in metres a second, before the
/// thrower's own velocity is added.
pub const THROW_SPEED: f32 = 21.0;
/// Radians the throw leaves above the view: looking level still lobs.
pub const THROW_LOFT: f32 = 0.16;
/// Downward acceleration on a grenade in flight, m/s². Lighter than the
/// player's (19, an arcade jump), so a full throw carries about 30 m.
pub const GRAVITY: f32 = 14.0;
/// Radius of the grenade's body: it rests this far off a surface.
pub const RADIUS: f32 = 0.045;
/// Share of the speed into a surface a bounce gives back.
pub const RESTITUTION: f32 = 0.32;
/// Share of the speed along a surface a bounce keeps.
pub const BOUNCE_GRIP: f32 = 0.7;
/// Slowing while rolling on the ground, m/s².
pub const ROLL_DECEL: f32 = 3.5;
/// A grenade rolling slower than this stops.
pub const SETTLE_SPEED: f32 = 0.2;
/// Speed into the floor below which a bounce becomes a roll instead.
pub const ROLL_CATCH: f32 = 1.4;
/// Surfaces whose normal points up at least this much count as ground for
/// rolling (about 45°).
pub const GROUND_NORMAL_Y: f32 = 0.7;
/// Most contacts handled in one tick, so a grenade wedged in a corner
/// cannot spin the loop.
const MAX_CONTACTS: usize = 4;

/// Distance within which a frag does its full damage.
pub const FRAG_INNER_RADIUS: f32 = 3.0;
/// Distance past which a frag does nothing.
pub const FRAG_RADIUS: f32 = 8.0;
/// Damage at the centre: more than a full-health player has.
pub const FRAG_MAX_DAMAGE: f32 = 150.0;
/// Damage at the edge of the blast.
pub const FRAG_MIN_DAMAGE: f32 = 20.0;
/// Heights above the feet the blast looks for: the body is hit if any of
/// them is in plain view of the blast (shins, chest, head).
pub const BLAST_TARGET_HEIGHTS: [f32; 3] = [0.3, 1.1, 1.6];
/// The blast looks from this far above where the grenade lies, so a
/// grenade on the floor still sees over a kerb.
pub const BLAST_LIFT: f32 = 0.15;

/// Damage a frag does to a body `distance` metres from the blast: full
/// within [`FRAG_INNER_RADIUS`], falling straight to [`FRAG_MIN_DAMAGE`] at
/// [`FRAG_RADIUS`], none beyond. A full-health player dies within about
/// 5 m.
pub fn frag_damage(distance: f32) -> f32 {
    if !distance.is_finite() || distance > FRAG_RADIUS {
        return 0.0;
    }
    if distance <= FRAG_INNER_RADIUS {
        return FRAG_MAX_DAMAGE;
    }
    let t = (distance - FRAG_INNER_RADIUS) / (FRAG_RADIUS - FRAG_INNER_RADIUS);
    FRAG_MAX_DAMAGE + (FRAG_MIN_DAMAGE - FRAG_MAX_DAMAGE) * t
}

/// Damage a rocket's blast does `distance` metres away: full within
/// [`ROCKET_INNER_RADIUS`], falling straight to [`ROCKET_MIN_DAMAGE`] at
/// [`ROCKET_RADIUS`], none beyond.
pub fn rocket_damage(distance: f32) -> f32 {
    if !distance.is_finite() || distance > ROCKET_RADIUS {
        return 0.0;
    }
    if distance <= ROCKET_INNER_RADIUS {
        return ROCKET_MAX_DAMAGE;
    }
    let t = (distance - ROCKET_INNER_RADIUS) / (ROCKET_RADIUS - ROCKET_INNER_RADIUS);
    ROCKET_MAX_DAMAGE + (ROCKET_MIN_DAMAGE - ROCKET_MAX_DAMAGE) * t
}

/// Damage `kind` does at `distance`.
pub fn blast_damage(kind: Kind, distance: f32) -> f32 {
    match kind {
        Kind::Frag => frag_damage(distance),
        Kind::Rocket => rocket_damage(distance),
        Kind::Flash | Kind::Smoke => 0.0,
    }
}

/// The share of the way along `from`..`to` where a moving point first comes
/// within `radius` of the upright segment `base`..`base + height` (a body),
/// if it does.
pub fn segment_hits_body(
    from: Vec3,
    to: Vec3,
    base: Vec3,
    height: f32,
    radius: f32,
) -> Option<f32> {
    // Sample the move finely: rockets cover at most a metre a tick.
    let steps = ((from.distance(to) / 0.05).ceil() as usize).clamp(1, 64);
    (0..=steps).find_map(|i| {
        let t = i as f32 / steps as f32;
        let p = from.lerp(to, t);
        let y = (p.y - base.y).clamp(0.0, height);
        let axis = Vec3::new(base.x, base.y + y, base.z);
        (p.distance(axis) <= radius).then_some(t)
    })
}

/// How far `kind`'s blast reaches.
pub fn blast_radius(kind: Kind) -> f32 {
    match kind {
        Kind::Frag => FRAG_RADIUS,
        Kind::Rocket => ROCKET_RADIUS,
        Kind::Flash => FLASH_RANGE,
        Kind::Smoke => SMOKE_RADIUS,
    }
}

/// Frags a player spawns with in each mode, refilled every life. One lethal
/// a life is the modern default; Gun Game hands out guns only.
pub fn spawn_lethals(mode: Mode) -> u8 {
    match mode {
        Mode::GunGame(_) => 0,
        _ => 1,
    }
}

/// Tacticals a player spawns with in each mode: one a life, none in Gun Game.
pub fn spawn_tacticals(mode: Mode) -> u8 {
    spawn_lethals(mode)
}

/// A surface the grenade ran into during a move.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    /// Share of the move travelled before touching, 0 to 1.
    pub fraction: f32,
    /// The surface's outward normal (unit length).
    pub normal: Vec3,
}

/// The world as a grenade sees it: the engine sweeps the grenade's body
/// along a move and reports the first surface in the way.
pub trait Collide {
    fn sweep(&mut self, from: Vec3, to: Vec3) -> Option<Contact>;
}

/// No world at all, for tests and empty space.
pub struct OpenAir;

impl Collide for OpenAir {
    fn sweep(&mut self, _from: Vec3, _to: Vec3) -> Option<Contact> {
        None
    }
}

/// A flat floor at height `y` (tests).
pub struct Floor(pub f32);

impl Collide for Floor {
    fn sweep(&mut self, from: Vec3, to: Vec3) -> Option<Contact> {
        let floor = self.0 + RADIUS;
        if to.y >= floor || from.y < floor - 1e-4 {
            return None;
        }
        let drop = from.y - to.y;
        let fraction = if drop > 1e-6 {
            ((from.y - floor) / drop).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Some(Contact {
            fraction,
            normal: Vec3::Y,
        })
    }
}

/// A live grenade.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grenade {
    pub kind: Kind,
    /// Centre of the grenade's body.
    pub position: Vec3,
    pub velocity: Vec3,
    /// Seconds until it goes off.
    pub fuse: f32,
    /// Lying still: it no longer moves.
    pub resting: bool,
    /// Rolling along the ground rather than flying.
    pub rolling: bool,
}

/// What one tick of flight did, for sounds and effects.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Flight {
    /// The hardest impact this tick, as the speed into the surface (m/s),
    /// if it bounced.
    pub bounce: Option<f32>,
    /// The fuse ran out this tick.
    pub exploded: bool,
}

impl Grenade {
    /// A grenade thrown overhand from `eye` along the view (`yaw`, `pitch`)
    /// by a body moving at `carrier`, with `fuse` seconds left.
    pub fn thrown(kind: Kind, eye: Vec3, yaw: f32, pitch: f32, carrier: Vec3, fuse: f32) -> Self {
        let loft = (pitch + THROW_LOFT).min(look::PITCH_LIMIT);
        let direction = look::view_direction(yaw, loft);
        Self {
            kind,
            position: eye,
            velocity: direction * THROW_SPEED + carrier,
            fuse: fuse.max(0.0),
            resting: false,
            rolling: false,
        }
    }

    /// A rocket leaving a launcher's tube at `muzzle` along the view (`yaw`,
    /// `pitch`): it flies straight at [`ROCKET_SPEED`], sagging slowly, and
    /// does not carry the firer's own movement.
    pub fn launched(muzzle: Vec3, yaw: f32, pitch: f32) -> Self {
        Self {
            kind: Kind::Rocket,
            position: muzzle,
            velocity: look::view_direction(yaw, pitch) * ROCKET_SPEED,
            fuse: ROCKET_FLIGHT,
            resting: false,
            rolling: false,
        }
    }

    /// A grenade let go of where it is, with no throw: dropped by a player
    /// who died cooking it, or cooked off in the hand.
    pub fn dropped(kind: Kind, at: Vec3, carrier: Vec3, fuse: f32) -> Self {
        Self {
            kind,
            position: at,
            velocity: carrier,
            fuse: fuse.max(0.0),
            resting: false,
            rolling: false,
        }
    }

    /// Advances the grenade `dt` seconds through `world`.
    pub fn step(&mut self, world: &mut impl Collide, dt: f32) -> Flight {
        let mut flight = Flight::default();
        self.fuse -= dt;
        if self.fuse <= 0.0 {
            self.fuse = 0.0;
            flight.exploded = true;
            return flight;
        }
        if self.resting {
            return flight;
        }
        if self.kind == Kind::Rocket {
            // Straight on until it meets something, and goes off there.
            self.velocity.y -= ROCKET_DROP * dt;
            let target = self.position + self.velocity * dt;
            match world.sweep(self.position, target) {
                Some(contact) => {
                    let fraction = contact.fraction.clamp(0.0, 1.0);
                    self.position += (target - self.position) * fraction + contact.normal * 0.05;
                    self.fuse = 0.0;
                    flight.exploded = true;
                }
                None => self.position = target,
            }
            return flight;
        }
        if self.rolling {
            // Still on the ground? Feel just below; a roll off a ledge flies.
            let probe = self.position - Vec3::Y * (RADIUS + 0.05);
            match world.sweep(self.position, probe) {
                Some(contact) if contact.normal.y >= GROUND_NORMAL_Y => {
                    let flat = Vec3::new(self.velocity.x, 0.0, self.velocity.z);
                    let speed = flat.length();
                    let slowed = (speed - ROLL_DECEL * dt).max(0.0);
                    // Rolls downhill a little on a slope.
                    let downhill = Vec3::new(contact.normal.x, 0.0, contact.normal.z) * GRAVITY;
                    self.velocity = if speed > 1e-6 {
                        flat * (slowed / speed)
                    } else {
                        Vec3::ZERO
                    } + downhill * dt;
                    // Keep it on the surface.
                    self.velocity -= contact.normal * self.velocity.dot(contact.normal);
                    if self.velocity.length() < SETTLE_SPEED && contact.normal.y > 0.95 {
                        self.velocity = Vec3::ZERO;
                        self.resting = true;
                        return flight;
                    }
                }
                _ => self.rolling = false,
            }
        }
        if !self.rolling {
            self.velocity.y -= GRAVITY * dt;
        }
        let mut remaining = dt;
        for _ in 0..MAX_CONTACTS {
            if remaining <= 0.0 {
                break;
            }
            let target = self.position + self.velocity * remaining;
            let Some(contact) = world.sweep(self.position, target) else {
                self.position = target;
                break;
            };
            let fraction = contact.fraction.clamp(0.0, 1.0);
            self.position += self.velocity * remaining * fraction;
            remaining *= 1.0 - fraction;
            let n = contact.normal;
            let into = self.velocity.dot(n);
            if into >= 0.0 {
                // Moving away already (touching on the way out): carry on.
                self.position += n * 1e-3;
                continue;
            }
            let normal_part = n * into;
            let tangent = self.velocity - normal_part;
            let impact = -into;
            if n.y >= GROUND_NORMAL_Y && impact < ROLL_CATCH {
                // Too slow to bounce off the ground: roll along it.
                self.velocity = tangent;
                self.rolling = true;
            } else {
                self.velocity = tangent * BOUNCE_GRIP - normal_part * RESTITUTION;
                flight.bounce = Some(flight.bounce.map_or(impact, |b: f32| b.max(impact)));
            }
            // Off the surface a hair, so the next sweep starts clear.
            self.position += n * 1e-3;
        }
        flight
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TICK_DT;

    fn fly(grenade: &mut Grenade, world: &mut impl Collide, seconds: f32) -> Vec<Flight> {
        let ticks = (seconds / TICK_DT).round() as usize;
        (0..ticks).map(|_| grenade.step(world, TICK_DT)).collect()
    }

    #[test]
    fn a_rocket_flies_straight_and_goes_off_on_what_it_meets() {
        let mut r = Grenade::launched(Vec3::new(0.0, 1.6, 0.0), 0.0, 0.0);
        assert_eq!(r.kind, Kind::Rocket);
        // In open air it flies its whole flight, sagging under 2 m by
        // 100 m, then goes off where it is.
        let flights = fly(&mut r, &mut OpenAir, 1.54);
        assert!(flights.iter().all(|f| !f.exploded));
        assert!((-r.position.z - 100.0).abs() < 1.5, "{}", r.position.z);
        assert!(1.6 - r.position.y < 2.0, "{}", r.position.y);
        let flights = fly(&mut r, &mut OpenAir, ROCKET_FLIGHT);
        assert!(flights.iter().any(|f| f.exploded));
        // Aimed down at a floor, it goes off on it rather than bouncing.
        let mut r = Grenade::launched(Vec3::new(0.0, 1.6, 0.0), 0.0, -0.5);
        let flights = fly(&mut r, &mut Floor(0.0), 1.0);
        assert!(flights.iter().any(|f| f.exploded && f.bounce.is_none()));
        assert!(r.position.y < 0.2 && r.fuse == 0.0);
    }

    #[test]
    fn a_rocket_kills_near_its_blast_and_wounds_further_out() {
        assert_eq!(rocket_damage(0.0), ROCKET_MAX_DAMAGE);
        assert!(rocket_damage(4.0) >= 100.0);
        assert!(rocket_damage(4.3) < 100.0);
        assert_eq!(rocket_damage(ROCKET_RADIUS + 0.1), 0.0);
        assert_eq!(blast_damage(Kind::Rocket, 1.0), ROCKET_MAX_DAMAGE);
    }

    #[test]
    fn a_rocket_strikes_a_body_it_passes_close_to() {
        let base = Vec3::new(0.0, 0.0, -10.0);
        let hit = segment_hits_body(
            Vec3::new(0.0, 1.5, -9.0),
            Vec3::new(0.0, 1.5, -11.0),
            base,
            1.8,
            0.4,
        );
        assert!(hit.is_some_and(|t| (0.2..0.4).contains(&t)), "{hit:?}");
        let over = segment_hits_body(
            Vec3::new(0.0, 2.6, -9.0),
            Vec3::new(0.0, 2.6, -11.0),
            base,
            1.8,
            0.4,
        );
        assert!(over.is_none());
        let wide = segment_hits_body(
            Vec3::new(0.6, 1.0, -9.0),
            Vec3::new(0.6, 1.0, -11.0),
            base,
            1.8,
            0.4,
        );
        assert!(wide.is_none());
    }

    #[test]
    fn damage_is_full_inside_and_falls_to_the_edge() {
        assert_eq!(frag_damage(0.0), FRAG_MAX_DAMAGE);
        assert_eq!(frag_damage(FRAG_INNER_RADIUS), FRAG_MAX_DAMAGE);
        assert!((frag_damage(FRAG_RADIUS) - FRAG_MIN_DAMAGE).abs() < 1e-4);
        assert_eq!(frag_damage(FRAG_RADIUS + 0.01), 0.0);
        assert_eq!(frag_damage(f32::NAN), 0.0);
        // Kills a full-health player out to about 5 m.
        assert!(frag_damage(4.8) >= 100.0);
        assert!(frag_damage(5.2) < 100.0);
        let mut last = f32::MAX;
        for i in 0..=80 {
            let d = frag_damage(i as f32 * 0.1);
            assert!(d <= last, "falls off monotonically");
            last = d;
        }
    }

    #[test]
    fn a_level_throw_carries_like_a_modern_frag() {
        // Thrown from 1.6 m looking level onto open ground.
        let mut g = Grenade::thrown(
            Kind::Frag,
            Vec3::new(0.0, 1.6, 0.0),
            0.0,
            0.0,
            Vec3::ZERO,
            10.0,
        );
        let mut floor = Floor(0.0);
        let mut first_bounce = None;
        for tick in 0..2000 {
            let flight = g.step(&mut floor, TICK_DT);
            if flight.bounce.is_some() && first_bounce.is_none() {
                first_bounce = Some((tick, g.position));
            }
            if g.resting {
                break;
            }
        }
        let (tick, at) = first_bounce.expect("lands");
        let air = tick as f32 * TICK_DT;
        assert!(air > 0.4 && air < 0.9, "first landing after {air} s");
        assert!(-at.z > 8.0 && -at.z < 16.0, "lands {} m out", -at.z);
        assert!(g.resting, "comes to rest");
        assert!(-g.position.z > -at.z, "bounces and rolls on");
        assert!((g.position.y - RADIUS).abs() < 0.01, "lies on the floor");
    }

    #[test]
    fn a_high_throw_carries_about_thirty_metres() {
        let mut g = Grenade::thrown(
            Kind::Frag,
            Vec3::new(0.0, 1.6, 0.0),
            0.0,
            0.6,
            Vec3::ZERO,
            10.0,
        );
        let mut floor = Floor(0.0);
        let mut landed = None;
        for _ in 0..2000 {
            if g.step(&mut floor, TICK_DT).bounce.is_some() {
                landed = Some(g.position);
                break;
            }
        }
        let at = landed.expect("lands");
        assert!(-at.z > 25.0 && -at.z < 36.0, "lands {} m out", -at.z);
    }

    #[test]
    fn the_fuse_runs_out_on_time_and_only_once() {
        let mut g = Grenade::dropped(Kind::Frag, Vec3::Y, Vec3::ZERO, 1.0);
        let flights = fly(&mut g, &mut Floor(0.0), 1.5);
        let blasts: Vec<usize> = flights
            .iter()
            .enumerate()
            .filter(|(_, f)| f.exploded)
            .map(|(i, _)| i)
            .collect();
        let expected = (1.0 / TICK_DT).round() as usize - 1;
        assert!(
            blasts.first().is_some_and(|&t| t.abs_diff(expected) <= 1),
            "{blasts:?}"
        );
        // The caller removes it on the first; it stays at zero.
        assert_eq!(g.fuse, 0.0);
    }

    #[test]
    fn a_dropped_grenade_bounces_lower_each_time_and_settles() {
        let mut g = Grenade::dropped(Kind::Frag, Vec3::new(0.0, 3.0, 0.0), Vec3::ZERO, 10.0);
        let mut floor = Floor(0.0);
        let mut impacts = Vec::new();
        for _ in 0..1200 {
            if let Some(speed) = g.step(&mut floor, TICK_DT).bounce {
                impacts.push(speed);
            }
            if g.resting {
                break;
            }
        }
        assert!(!impacts.is_empty());
        assert!(impacts.windows(2).all(|w| w[1] < w[0]), "{impacts:?}");
        assert!(g.resting);
        assert!(g.position.y >= RADIUS - 1e-3, "never sinks through");
    }

    /// A wall across the way at `z`, facing the thrower (+Z).
    struct Wall(f32);

    impl Collide for Wall {
        fn sweep(&mut self, from: Vec3, to: Vec3) -> Option<Contact> {
            let face = self.0 + RADIUS;
            if to.z >= face || from.z < face {
                return None;
            }
            let fraction = ((from.z - face) / (from.z - to.z)).clamp(0.0, 1.0);
            Some(Contact {
                fraction,
                normal: Vec3::Z,
            })
        }
    }

    #[test]
    fn a_wall_bounces_the_grenade_back_slower() {
        let mut g = Grenade::thrown(
            Kind::Frag,
            Vec3::new(0.0, 1.6, 0.0),
            0.0,
            0.0,
            Vec3::ZERO,
            10.0,
        );
        let mut wall = Wall(-4.0);
        let before = g.velocity.z;
        let flights = fly(&mut g, &mut wall, 0.5);
        assert!(flights.iter().any(|f| f.bounce.is_some()));
        assert!(g.position.z > -4.0, "stays on the thrower's side");
        assert!(g.velocity.z > 0.0, "comes back");
        assert!(g.velocity.z < -before * 0.5, "with much less speed");
    }

    #[test]
    fn the_throw_carries_the_throwers_speed() {
        let still = Grenade::thrown(Kind::Frag, Vec3::ZERO, 0.0, 0.0, Vec3::ZERO, 3.0);
        let running = Grenade::thrown(
            Kind::Frag,
            Vec3::ZERO,
            0.0,
            0.0,
            Vec3::new(0.0, 0.0, -6.0),
            3.0,
        );
        assert!((running.velocity.z - still.velocity.z + 6.0).abs() < 1e-5);
        assert!(still.velocity.y > 0.0, "looking level still lobs");
        assert!((still.velocity.length() - THROW_SPEED).abs() < 1e-4);
    }

    #[test]
    fn the_same_contacts_give_the_same_flight() {
        let throw = || {
            Grenade::thrown(
                Kind::Frag,
                Vec3::new(0.3, 1.5, -0.2),
                0.7,
                0.2,
                Vec3::new(1.0, 0.0, 0.5),
                FRAG_FUSE,
            )
        };
        let (mut a, mut b) = (throw(), throw());
        fly(&mut a, &mut Floor(0.0), 2.0);
        fly(&mut b, &mut Floor(0.0), 2.0);
        assert_eq!(a, b);
    }

    #[test]
    fn a_throw_progresses_to_the_cock_holds_and_finishes() {
        assert_eq!(throw_progress(0.0, None), 0.0);
        assert_eq!(throw_progress(PIN_TIME, None), COCKED_PROGRESS);
        assert_eq!(
            throw_progress(2.0, None),
            COCKED_PROGRESS,
            "holds while cooking"
        );
        assert_eq!(throw_progress(2.0, Some(WIND_UP)), COCKED_PROGRESS);
        let release = release_progress();
        assert!(release > COCKED_PROGRESS && release < 0.7);
        assert_eq!(throw_progress(2.0, Some(-RECOVERY)), 1.0);
    }

    #[test]
    fn a_flash_hits_hardest_close_and_facing_it() {
        assert_eq!(flash_amount(FLASH_RANGE, 1.0), 0.0);
        assert!(flash_amount(3.0, 1.0) > 0.9);
        let facing = flash_amount(8.0, 1.0);
        let side = flash_amount(8.0, 0.0);
        let behind = flash_amount(8.0, -1.0);
        assert!(facing > side && side > behind && behind > 0.0);
        assert!((behind / facing - FLASH_BEHIND).abs() < 0.05);
        assert_eq!(flash_seconds(0.0), 0.0);
        assert_eq!(flash_seconds(1.0), FLASH_MAX_SECONDS);
    }

    #[test]
    fn smoke_grows_hangs_and_thins() {
        assert_eq!(smoke_radius(-0.1), 0.0);
        assert!(smoke_radius(0.5) < smoke_radius(1.5));
        assert_eq!(smoke_radius(SMOKE_BLOOM), SMOKE_RADIUS);
        assert_eq!(smoke_radius(SMOKE_SECONDS * 0.5), SMOKE_RADIUS);
        assert!(smoke_radius(SMOKE_SECONDS - 0.5) < SMOKE_RADIUS * 0.6);
        assert_eq!(smoke_radius(SMOKE_SECONDS), 0.0);
    }

    #[test]
    fn smoke_blocks_lines_through_its_middle_only() {
        let c = Vec3::new(0.0, 1.6, 0.0);
        let age = 5.0;
        let through = smoke_blocks(
            c,
            age,
            Vec3::new(-10.0, 1.6, 0.0),
            Vec3::new(10.0, 1.6, 0.0),
        );
        assert!(through);
        let past = smoke_blocks(
            c,
            age,
            Vec3::new(-10.0, 1.6, 6.0),
            Vec3::new(10.0, 1.6, 6.0),
        );
        assert!(!past, "a line clear of the cloud");
        assert!(
            smoke_blocks(c, age, c, Vec3::new(30.0, 0.0, 0.0)),
            "from inside"
        );
        assert!(
            !smoke_blocks(
                c,
                0.05,
                Vec3::new(-10.0, 1.6, 0.0),
                Vec3::new(10.0, 1.6, 0.0)
            ),
            "not yet"
        );
        let short = smoke_blocks(
            c,
            age,
            Vec3::new(-10.0, 1.6, 0.0),
            Vec3::new(-6.0, 1.6, 0.0),
        );
        assert!(!short, "a line that stops short of it");
    }

    #[test]
    fn kinds_round_trip_and_only_the_frag_cooks() {
        for kind in [Kind::Frag, Kind::Flash, Kind::Smoke] {
            assert_eq!(Kind::from_code(kind.code()), Some(kind));
            assert_eq!(Kind::by_key(kind.key()), Some(kind));
            assert_eq!(kind.cooks(), kind.lethal());
        }
        assert_eq!(blast_damage(Kind::Flash, 0.0), 0.0);
    }

    #[test]
    fn gun_game_carries_no_grenades() {
        assert_eq!(spawn_lethals(Mode::TeamDeathmatch), 1);
        assert_eq!(spawn_lethals(Mode::SearchAndDestroy), 1);
        assert_eq!(
            spawn_lethals(Mode::GunGame(crate::gun_game::Rules::default())),
            0
        );
    }
}
