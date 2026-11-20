//! How far along each animation a body is: the stride, a reload, and a
//! mantle's climb. Snapshots carry these ([`crate::net::protocol::PlayerView`]),
//! so every client draws a body at the same moment of each animation, and
//! the hit zones ([`super::hitboxes`]) pose that moment too.
//!
//! The dedicated server keeps one [`BodyClock`] per player and ticks it with
//! the player's simulated state; a character with no server (offline targets)
//! keeps its own.

use crate::Vec3;
use crate::movement::Stance;
use crate::weapon::{Reload, WeaponId};

use super::gait;

/// Each animation's progress, 0..1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Progress {
    /// Stride phase (wraps; see [`gait`]).
    pub stride: f32,
    /// Through a reload (0 when not reloading).
    pub reload: f32,
    /// Through a mantle's climb (0 when not mantling).
    pub mantle: f32,
    /// How far the aim has turned from the feet, standing still (radians,
    /// positive to the left; see [`gait::turn_feet`]). The legs and hips stay
    /// on the feet; the spine turns the rest.
    pub turn: f32,
    /// How far the legs are stepping in place as the feet swing round, 0..1
    /// ([`gait::Turning`]): the walk loop blends in, at `stride`.
    pub step: f32,
}

/// What a body is doing this tick, as the clock needs it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyState {
    pub velocity: Vec3,
    pub yaw: f32,
    pub stance: Stance,
    pub sprinting: bool,
    pub on_floor: bool,
    pub alive: bool,
    pub reloading: bool,
    /// How fast the reload runs against real time (Sleight of Hand: 2;
    /// `perk::Handling::reload`), so the body's reload ends with the
    /// player's.
    pub reload_rate: f32,
    pub weapon: WeaponId,
    /// Seconds left of a mantle (`MoveState::mantle_time`), 0 when not mantling.
    pub mantle_left: f32,
    /// On a ladder: the yaw that faces its rungs. The feet stay on it.
    pub ladder: Option<f32>,
}

/// One body's animation clocks.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BodyClock {
    stride: f32,
    /// Seconds into the current reload.
    reload: Option<f32>,
    /// The current mantle's full length, seconds.
    mantle: Option<f32>,
    turning: gait::Turning,
    progress: Progress,
}

impl BodyClock {
    /// Advances the clocks by one step of `dt` seconds with the body in `s`.
    pub fn tick(&mut self, s: &BodyState, dt: f32) -> Progress {
        let (right, forward) = gait::local_velocity(s.velocity, s.yaw);
        let rate = if s.alive {
            gait::stride_rate(s.velocity, s.yaw, s.stance, s.sprinting, s.on_floor)
        } else {
            0.0
        };
        let speed = (right * right + forward * forward).sqrt();
        let mantling = s.mantle_left > 0.0 && s.alive;
        let still = s.alive
            && s.on_floor
            && !mantling
            && s.stance != Stance::Sliding
            && speed <= gait::MOVING_SPEED;
        let (turn, stepped, rate) = match s.ladder.filter(|_| s.alive) {
            // On a ladder the body faces the rungs, and the climb's hands
            // and feet run on the stride, a cycle every two rungs.
            Some(ladder) => (
                self.turning.hold(ladder, s.yaw),
                0.0,
                s.velocity.y.abs() / gait::LADDER_STRIDE,
            ),
            None => {
                let (turn, stepped) = self.turning.tick(s.yaw, still, dt);
                (turn, stepped, rate)
            }
        };
        // Moving, the loops' cadence; stepping round on the spot, the step's.
        self.stride = gait::advance_stride(self.stride, rate, dt) + stepped;
        self.stride = self.stride.rem_euclid(1.0);

        self.reload = match (s.reloading && s.alive, self.reload) {
            (true, Some(t)) => Some(t + dt * s.reload_rate),
            (true, None) => Some(0.0),
            (false, _) => None,
        };
        let reload = self
            .reload
            .map_or(0.0, |t| (t / reload_seconds(s.weapon)).clamp(0.0, 1.0));

        self.mantle = match (mantling, self.mantle) {
            (true, Some(total)) => Some(total.max(s.mantle_left)),
            // The first mantling tick has already run one step of it.
            (true, None) => Some(s.mantle_left + dt),
            (false, _) => None,
        };
        let mantle = self
            .mantle
            .map_or(0.0, |total| (1.0 - s.mantle_left / total).clamp(0.0, 1.0));

        self.progress = Progress {
            stride: self.stride,
            reload,
            mantle,
            turn,
            step: self.turning.step,
        };
        self.progress
    }

    /// Progress after the last [`tick`](Self::tick).
    pub fn progress(&self) -> Progress {
        self.progress
    }
}

/// How long a weapon's reload animation runs, seconds: a magazine's full
/// reload, or a pump gun's start, four shells, and end.
pub fn reload_seconds(weapon: WeaponId) -> f32 {
    reload_seconds_of(weapon.def())
}

/// [`reload_seconds`] for a weapon's definition.
pub fn reload_seconds_of(def: &crate::weapon::WeaponDef) -> f32 {
    match def.reload {
        Reload::Magazine { time, .. } => time,
        Reload::PerRound {
            start,
            per_round,
            end,
        } => start + per_round * 4.0 + end,
    }
    .max(0.3)
}

/// A turn offset as a snapshot byte: 1/256 turns, signed.
pub fn turn_to_u8(turn: f32) -> u8 {
    ((gait::wrap_angle(turn) / std::f32::consts::TAU * 256.0).round() as i32).rem_euclid(256) as u8
}

pub fn turn_from_u8(value: u8) -> f32 {
    f32::from(value as i8) / 256.0 * std::f32::consts::TAU
}

/// A progress (0..1) as a snapshot byte.
pub fn to_u8(progress: f32) -> u8 {
    (progress.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// A stride phase as a snapshot byte (1/256 of a stride; wraps).
pub fn stride_to_u8(phase: f32) -> u8 {
    ((phase.rem_euclid(1.0) * 256.0).round() as u32 % 256) as u8
}

pub fn stride_from_u8(value: u8) -> f32 {
    f32::from(value) / 256.0
}

pub fn from_u8(value: u8) -> f32 {
    f32::from(value) / 255.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body() -> BodyState {
        BodyState {
            velocity: Vec3::ZERO,
            yaw: 0.0,
            stance: Stance::Standing,
            sprinting: false,
            on_floor: true,
            alive: true,
            reloading: false,
            reload_rate: 1.0,
            weapon: WeaponId::expect("m4"),
            mantle_left: 0.0,
            ladder: None,
        }
    }

    #[test]
    fn the_stride_runs_at_the_walk_cadence() {
        let mut clock = BodyClock::default();
        let walking = BodyState {
            velocity: Vec3::new(0.0, 0.0, -gait::WALK_SPEED),
            ..body()
        };
        let dt = 1.0 / 120.0;
        for _ in 0..80 {
            clock.tick(&walking, dt);
        }
        // 80 ticks of a 1.3333 s stride.
        let expected = (80.0 * dt / 1.3333) % 1.0;
        assert!((clock.progress().stride - expected).abs() < 1e-3);
        // Standing still holds the phase.
        let held = clock.progress().stride;
        clock.tick(&body(), dt);
        assert_eq!(clock.progress().stride, held);
    }

    #[test]
    fn reloads_and_mantles_run_from_zero_to_one() {
        let mut clock = BodyClock::default();
        let dt = 0.05;
        let reloading = BodyState {
            reloading: true,
            ..body()
        };
        let total = reload_seconds(WeaponId::expect("m4"));
        let mut last = -1.0;
        let mut t = 0.0;
        while t < total {
            let p = clock.tick(&reloading, dt).reload;
            assert!(p >= last);
            last = p;
            t += dt;
        }
        assert!(last > 0.95);
        assert_eq!(clock.tick(&body(), dt).reload, 0.0);
        // Sleight of Hand: the same reload in half the time.
        let quick = BodyState {
            reload_rate: 2.0,
            ..reloading
        };
        clock.tick(&quick, dt);
        let mut t = 0.0;
        let mut p = 0.0;
        while t < total * 0.5 - dt {
            p = clock.tick(&quick, dt).reload;
            t += dt;
        }
        assert!(p > 0.9, "{p}");
        assert_eq!(clock.tick(&body(), dt).reload, 0.0);
        // A new reload starts over.
        assert_eq!(clock.tick(&reloading, dt).reload, 0.0);

        // A 0.4 s mantle: the simulation has already run one step on the
        // first tick the clock sees.
        let mut left = 0.4 - dt;
        let mut seen = Vec::new();
        while left > 0.0 {
            seen.push(
                clock
                    .tick(
                        &BodyState {
                            mantle_left: left,
                            ..body()
                        },
                        dt,
                    )
                    .mantle,
            );
            left -= dt;
        }
        assert!((seen[0] - dt / 0.4).abs() < 1e-4, "{seen:?}");
        assert!(seen.windows(2).all(|w| w[1] > w[0]));
        assert!(*seen.last().unwrap() > 0.85);
    }

    #[test]
    fn bytes_round_trip() {
        assert_eq!(stride_to_u8(0.999), 0);
        assert_eq!(stride_from_u8(stride_to_u8(0.5)), 0.5);
        assert_eq!(to_u8(1.0), 255);
        assert_eq!(from_u8(255), 1.0);
    }
}
