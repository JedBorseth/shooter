//! Lag compensation: the server checks each shot against other players as the
//! shooter saw them.
//!
//! A client draws other players about 100 ms in the past (snapshot
//! interpolation), and its input reaches the server half a round trip later.
//! Checking a shot against where targets are on the server "now" would make
//! every player lead their targets by their ping. Instead each firing input
//! carries the server tick the client was drawing others at
//! ([`PlayerInput::view_tick`](crate::player::PlayerInput::view_tick)), and the
//! server rebuilds everyone's hitboxes at that tick from a history of the
//! snapshots it sent, interpolated exactly as clients interpolate them
//! ([`RemoteSample`]). Favor the shooter: what they hit on their screen is a
//! hit, up to [`MAX_REWIND`] in the past. (Valve, *Latency Compensating
//! Methods*; Overwatch GDC 2017.)
//!
//! Hitboxes come from [`crate::hitbox`], a pure function of position, view
//! angles, and stance, so rebuilding them from history needs no animation.

use std::collections::VecDeque;

use glam::Vec3;

use super::interpolation::{Interpolate, lerp_angle};
use super::protocol::PlayerView;
use super::{TICK_RATE, Tick};
use crate::hitbox::{self, HitboxPose};
use crate::movement::Stance;
use crate::penetration::MAX_WALLS;
use crate::weapon::{WeaponDef, WeaponId};

/// The furthest back a shot is checked, in seconds. Covers the 100 ms
/// interpolation delay plus a 200 ms round trip. Shooters with a worse
/// connection have to lead their targets by the excess, so a high ping
/// cannot shoot players who are long gone behind cover on their own screens.
pub const MAX_REWIND: f32 = 0.3;
/// [`MAX_REWIND`] in ticks.
pub const MAX_REWIND_TICKS: u32 = TICK_RATE * 3 / 10;
/// Ticks of history kept: comfortably more than the rewind cap.
const HISTORY_TICKS: Tick = TICK_RATE;

/// One remote player at one snapshot, as clients draw them. Client rendering
/// and the server's rewind both interpolate these, so both pose hitboxes
/// identically.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RemoteSample {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub stance: Stance,
    pub sprinting: bool,
    pub on_floor: bool,
    pub ads: f32,
    pub weapon: WeaponId,
    pub reloading: bool,
    pub alive: bool,
    pub shots_fired: u32,
    pub mantling: bool,
    pub hits: u8,
    pub hit_heading: u8,
}

impl From<&PlayerView> for RemoteSample {
    fn from(v: &PlayerView) -> Self {
        Self {
            position: v.position(),
            yaw: v.yaw(),
            pitch: v.pitch(),
            stance: v.stance,
            sprinting: v.sprinting,
            on_floor: v.on_floor,
            ads: v.ads(),
            weapon: v.weapon,
            reloading: v.reloading,
            alive: v.alive,
            shots_fired: v.shots_fired,
            mantling: v.mantling,
            hits: v.hits,
            hit_heading: v.hit_heading,
        }
    }
}

impl Interpolate for RemoteSample {
    fn interpolate(&self, to: &Self, t: f32) -> Self {
        // Respawns and other teleports jump instead of sliding across the map.
        if self.alive != to.alive || self.position.distance(to.position) > 2.0 {
            return if t < 0.5 { *self } else { *to };
        }
        let discrete = if t < 0.5 { self } else { to };
        Self {
            position: self.position.lerp(to.position, t),
            yaw: lerp_angle(self.yaw, to.yaw, t),
            pitch: self.pitch + (to.pitch - self.pitch) * t,
            ads: self.ads + (to.ads - self.ads) * t,
            // Shots show once the snapshot that contains them is reached.
            shots_fired: self.shots_fired,
            ..*discrete
        }
    }
}

impl RemoteSample {
    /// The hitboxes' pose, or `None` while dead (the dead cannot be hit).
    pub fn hitbox_pose(&self) -> Option<HitboxPose> {
        self.alive.then_some(HitboxPose {
            feet: self.position,
            yaw: self.yaw,
            pitch: self.pitch,
            stance: self.stance,
        })
    }
}

/// The server's record of every snapshot it sent in the last second.
#[derive(Default)]
pub struct LagHistory {
    /// Snapshot tick and every player's view, oldest first.
    frames: VecDeque<(Tick, Vec<PlayerView>)>,
}

impl LagHistory {
    /// Records the players of the snapshot sent at `tick` (the same views,
    /// quantized, that clients receive).
    pub fn record(&mut self, tick: Tick, players: Vec<PlayerView>) {
        if self.frames.back().is_some_and(|(t, _)| *t >= tick) {
            return;
        }
        self.frames.push_back((tick, players));
        while self
            .frames
            .front()
            .is_some_and(|(t, _)| t + HISTORY_TICKS < tick)
        {
            self.frames.pop_front();
        }
    }

    pub fn newest_tick(&self) -> Option<Tick> {
        self.frames.back().map(|(t, _)| *t)
    }

    /// The tick to check a shot at: the shooter's view tick, clamped to the
    /// rewind cap and to the newest snapshot. Without a view tick (bots, or a
    /// client that sent none) shots are checked at the newest snapshot.
    pub fn rewind_target(&self, now: Tick, view_tick: Option<f64>) -> f64 {
        let newest = f64::from(self.newest_tick().unwrap_or(now).min(now));
        let oldest = f64::from(now.saturating_sub(MAX_REWIND_TICKS));
        view_tick
            .filter(|t| t.is_finite())
            .map_or(newest, |t| t.clamp(oldest.min(newest), newest))
    }

    /// Player `id` as clients drew them at `tick`, blended between the two
    /// snapshots around it. Matches
    /// [`InterpolationBuffer::sample`](super::interpolation::InterpolationBuffer::sample):
    /// before the oldest snapshot it holds the oldest, past the newest it
    /// holds the newest.
    pub fn sample(&self, id: u8, tick: f64) -> Option<RemoteSample> {
        let mut before: Option<(Tick, &PlayerView)> = None;
        let mut after: Option<(Tick, &PlayerView)> = None;
        for (t, views) in &self.frames {
            let Some(view) = views.iter().find(|v| v.id == id) else {
                continue;
            };
            if f64::from(*t) <= tick {
                before = Some((*t, view));
            } else {
                after = Some((*t, view));
                break;
            }
        }
        match (before, after) {
            (Some((from_tick, from)), Some((to_tick, to))) => {
                let t = (tick - f64::from(from_tick)) / f64::from(to_tick - from_tick);
                Some(RemoteSample::from(from).interpolate(&RemoteSample::from(to), t as f32))
            }
            (Some((_, only)), None) | (None, Some((_, only))) => Some(RemoteSample::from(only)),
            (None, None) => None,
        }
    }
}

/// One pellet's ray. `max_distance` stops it at the first wall (the world is
/// static, so the server tests walls in the present).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pellet {
    pub origin: Vec3,
    /// Unit direction.
    pub direction: Vec3,
    pub max_distance: f32,
    /// Walls the pellet went through on the way: how far along each one ends,
    /// and the damage multiplier from there on (see `crate::penetration`).
    /// Unused entries are infinitely far.
    pub walls: [(f32, f32); MAX_WALLS],
}

impl Pellet {
    /// A pellet stopped by the first wall at `max_distance`.
    pub fn new(origin: Vec3, direction: Vec3, max_distance: f32) -> Self {
        Self {
            origin,
            direction,
            max_distance,
            walls: [(f32::INFINITY, 1.0); MAX_WALLS],
        }
    }

    /// The damage multiplier for a hit `distance` metres along the pellet.
    pub fn damage_scale_at(&self, distance: f32) -> f32 {
        self.walls
            .iter()
            .rev()
            .find(|(end, _)| *end <= distance)
            .map_or(1.0, |(_, scale)| *scale)
    }
}

/// A player a shot may hit, posed at the rewound tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub id: u8,
    pub pose: HitboxPose,
}

/// What one shot did to one target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TargetHit {
    pub id: u8,
    /// Every pellet's damage, summed.
    pub damage: f32,
    /// Any pellet hit the head.
    pub headshot: bool,
    /// Distance to the first pellet that hit, for scoring.
    pub distance: f32,
    /// Direction of the first pellet that hit, for hit reactions.
    pub direction: Vec3,
}

/// Checks a shot's pellets against the targets. Each pellet hits the nearest
/// shape of any target within its range; damage falls off with distance and
/// scales by zone, as on the client (`FpsPlayer::fire_shot`). Hits are summed
/// per target, in the order targets were first hit.
pub fn resolve_shot(def: &WeaponDef, pellets: &[Pellet], targets: &[Target]) -> Vec<TargetHit> {
    let shapes: Vec<(u8, [hitbox::Hitbox; hitbox::COUNT])> = targets
        .iter()
        .map(|t| (t.id, hitbox::hitboxes(&t.pose)))
        .collect();
    let mut hits: Vec<TargetHit> = Vec::new();
    for pellet in pellets {
        let nearest = shapes
            .iter()
            .filter_map(|(id, boxes)| {
                hitbox::raycast(boxes, pellet.origin, pellet.direction, pellet.max_distance)
                    .map(|hit| (*id, hit))
            })
            .min_by(|a, b| a.1.distance.total_cmp(&b.1.distance));
        let Some((id, hit)) = nearest else {
            continue;
        };
        let damage = def.damage_at(hit.distance)
            * hit.zone.damage_scale(def.damage.headshot)
            * pellet.damage_scale_at(hit.distance);
        let headshot = hit.zone == hitbox::HitZone::Head;
        match hits.iter_mut().find(|h| h.id == id) {
            Some(entry) => {
                entry.damage += damage;
                entry.headshot |= headshot;
            }
            None => hits.push(TargetHit {
                id,
                damage,
                headshot,
                distance: hit.distance,
                direction: pellet.direction,
            }),
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::interpolation::InterpolationBuffer;
    use crate::net::protocol::Predicted;
    use crate::player::PlayerState;

    fn view(id: u8, x: f32, yaw: f32, alive: bool) -> PlayerView {
        let mut state = PlayerState::default();
        if !alive {
            state.health = 0.0;
        }
        let predicted = Predicted {
            state,
            position: Vec3::new(x, 0.0, -10.0),
            on_floor: true,
        };
        PlayerView::from_state(id, &predicted, yaw, 0.0)
    }

    /// A target strafing along +X at 5 m/s, 10 m in front of a shooter at the
    /// origin, recorded at 60 Hz snapshots.
    fn strafing_history(ticks: Tick) -> LagHistory {
        let mut history = LagHistory::default();
        for tick in (0..=ticks).step_by(2) {
            let x = -3.0 + 5.0 * tick as f32 / TICK_RATE as f32;
            history.record(tick, vec![view(7, x, 0.0, true)]);
        }
        history
    }

    fn chest_shot_at(x: f32) -> Pellet {
        let origin = Vec3::new(0.0, 1.3, 0.0);
        let target = Vec3::new(x, 1.3, -10.0);
        Pellet::new(origin, (target - origin).normalize(), 100.0)
    }

    fn targets_at(history: &LagHistory, tick: f64) -> Vec<Target> {
        history
            .sample(7, tick)
            .and_then(|s| s.hitbox_pose())
            .map(|pose| Target { id: 7, pose })
            .into_iter()
            .collect()
    }

    #[test]
    fn samples_like_the_client_interpolation_buffer() {
        let mut history = LagHistory::default();
        let mut buffer = InterpolationBuffer::<RemoteSample>::default();
        for tick in (100..200).step_by(2) {
            let x = (tick as f32 * 0.37).sin() * 3.0;
            let v = view(3, x, tick as f32 * 0.05, tick % 50 != 0);
            history.record(tick, vec![v]);
            buffer.push(tick, RemoteSample::from(&v));
        }
        // The client samples forward in time (its buffer prunes behind it).
        let mut tick = 99.0;
        while tick < 205.0 {
            assert_eq!(history.sample(3, tick), buffer.sample(tick), "tick {tick}");
            tick += 0.37;
        }
    }

    #[test]
    fn a_moving_target_is_hit_where_the_shooter_saw_it() {
        let def = WeaponId::expect("m4").def();
        let history = strafing_history(240);
        // The shooter drew the target at tick 200 (x = 5.33) and aimed there;
        // by the time the shot arrives the target has moved on.
        let view_tick = 200.0;
        let seen = history.sample(7, view_tick).unwrap().position.x;
        let shot = chest_shot_at(seen);
        let rewound = history.rewind_target(232, Some(view_tick));
        assert_eq!(rewound, view_tick);
        let hits = resolve_shot(def, &[shot], &targets_at(&history, rewound));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, 7);
        assert!(!hits[0].headshot);
        let range = (shot.origin - Vec3::new(seen, 1.3, -10.0)).length();
        assert!((range - 0.3..range).contains(&hits[0].distance));
        assert!((hits[0].damage - def.damage_at(hits[0].distance)).abs() < 1e-4);

        // Without rewinding, the same shot misses: the target has moved 1.3 m.
        let now = targets_at(&history, 232.0);
        assert!(resolve_shot(def, &[shot], &now).is_empty());
    }

    #[test]
    fn rewind_is_capped() {
        let history = strafing_history(240);
        let now = 240;
        assert_eq!(
            history.rewind_target(now, Some(0.0)),
            f64::from(now - MAX_REWIND_TICKS)
        );
        assert_eq!(history.rewind_target(now, Some(1e9)), 240.0, "no future");
        assert_eq!(history.rewind_target(now, None), 240.0);
        assert_eq!(history.rewind_target(now, Some(f64::NAN)), 240.0);
        // Between snapshots the client's fraction is kept.
        assert_eq!(history.rewind_target(now, Some(225.5)), 225.5);

        // A shooter aiming where the target was 0.5 s ago misses.
        let def = WeaponId::expect("m4").def();
        let stale = chest_shot_at(history.sample(7, 180.0).unwrap().position.x);
        let rewound = history.rewind_target(now, Some(180.0));
        assert!(resolve_shot(def, &[stale], &targets_at(&history, rewound)).is_empty());
    }

    #[test]
    fn history_keeps_about_a_second() {
        let history = strafing_history(600);
        assert!(history.frames.len() <= (HISTORY_TICKS as usize / 2) + 1);
        assert_eq!(history.newest_tick(), Some(600));
        // Older than the history: holds the oldest snapshot kept.
        let oldest = history.sample(7, 0.0).unwrap();
        assert_eq!(oldest, history.sample(7, 480.0).unwrap());
    }

    #[test]
    fn the_dead_cannot_be_hit() {
        let mut history = LagHistory::default();
        history.record(10, vec![view(2, 0.0, 0.0, false)]);
        assert!(history.sample(2, 10.0).unwrap().hitbox_pose().is_none());
        assert!(history.sample(9, 10.0).is_none(), "unknown player");
    }

    #[test]
    fn pellets_sum_per_target_and_walls_stop_them() {
        let def = WeaponId::expect("m870").def();
        let pose = HitboxPose {
            feet: Vec3::new(0.0, 0.0, -5.0),
            yaw: 0.0,
            pitch: 0.0,
            stance: Stance::Standing,
        };
        let targets = [Target { id: 1, pose }];
        let origin = Vec3::new(0.0, 1.5, 0.0);
        let pellets: Vec<Pellet> = [1.7, 1.3, 1.0, 3.0]
            .iter()
            .map(|y| {
                Pellet::new(
                    origin,
                    (Vec3::new(0.0, *y, -5.0) - origin).normalize(),
                    50.0,
                )
            })
            .collect();
        let hits = resolve_shot(def, &pellets, &targets);
        assert_eq!(hits.len(), 1, "one entry per target");
        assert!(hits[0].headshot, "the first pellet hit the head");
        let near = def.damage_at(4.8);
        assert!(
            hits[0].damage > near * 2.5,
            "three pellets: {}",
            hits[0].damage
        );

        // A wall 2 m out stops every pellet.
        let blocked: Vec<Pellet> = pellets
            .iter()
            .map(|p| Pellet {
                max_distance: 2.0,
                ..*p
            })
            .collect();
        assert!(resolve_shot(def, &blocked, &targets).is_empty());

        // Through a plank 2 m out, the same pellets land weaker.
        let through: Vec<Pellet> = pellets
            .iter()
            .map(|p| Pellet {
                walls: [(2.05, 0.7), (f32::INFINITY, 1.0)],
                ..*p
            })
            .collect();
        let weaker = resolve_shot(def, &through, &targets);
        assert!((weaker[0].damage - hits[0].damage * 0.7).abs() < 1e-3);
    }

    #[test]
    fn the_nearest_target_takes_the_bullet() {
        let def = WeaponId::expect("m4").def();
        let pose = |z| HitboxPose {
            feet: Vec3::new(0.0, 0.0, z),
            yaw: 0.0,
            pitch: 0.0,
            stance: Stance::Standing,
        };
        let targets = [
            Target {
                id: 1,
                pose: pose(-20.0),
            },
            Target {
                id: 2,
                pose: pose(-8.0),
            },
        ];
        let hits = resolve_shot(def, &[chest_shot_at(0.0)], &targets);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, 2);
    }
}
