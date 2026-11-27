//! Lag compensation: the server checks each shot against other players as the
//! shooter saw them.
//!
//! A client draws other players a little in the past (snapshot interpolation,
//! about 35 to 50 ms on a clean link), and its input reaches the server half
//! a round trip later.
//! Checking a shot against where targets are on the server "now" would make
//! every player lead their targets by their ping. Instead each firing input
//! carries the server tick the client was drawing others at
//! ([`PlayerInput::view_tick`](crate::player::PlayerInput::view_tick); for a
//! click between ticks, the tick drawn on the frame it was clicked on, see
//! [`super::subtick`]), and the
//! server rebuilds everyone's hitboxes at that tick from a history of the
//! snapshots it sent, interpolated exactly as clients interpolate them
//! ([`RemoteSample`]). Favor the shooter: what they hit on their screen is a
//! hit, up to [`MAX_REWIND`] in the past. (Valve, *Latency Compensating
//! Methods*; Overwatch GDC 2017.)
//!
//! Hitboxes come from [`crate::hitbox`], a pure function of a snapshot's
//! position, view angles, stance, and activity (sprinting, airborne, mantling,
//! reloading, aim, weapon) and of the velocity between the two snapshots a
//! sample is blended from, so rebuilding them from history needs no
//! animation, and the server's rewind poses exactly what the client drew.

use std::collections::VecDeque;

use glam::Vec3;

use super::interpolation::{Interpolate, lerp_angle};
use super::protocol::PlayerView;
use super::{TICK_RATE, Tick};
use crate::hitbox::clock::Progress;
use crate::hitbox::gait::lerp_phase;
use crate::hitbox::{self, Activity, HitboxPose};
use crate::movement::Stance;
use crate::penetration::MAX_WALLS;
use crate::weapon::{WeaponDef, WeaponId};

/// The furthest back a shot is checked, in seconds. Covers the interpolation
/// delay (about 50 ms on a clean link, up to about 150 ms while snapshots go
/// missing) plus a round trip of 150 to 250 ms. Shooters with a worse
/// connection have to lead their targets by the excess, so a high ping
/// cannot shoot players who are long gone behind cover on their own screens.
pub const MAX_REWIND: f32 = 0.3;
/// [`MAX_REWIND`] in ticks.
pub const MAX_REWIND_TICKS: u32 = TICK_RATE * 3 / 10;

/// Seconds, beyond the round trip, that a client may draw remote players in
/// the past: the interpolation delay (about 0.05 s on a clean link, up to
/// about 0.15 s while snapshots go missing) and time queued before the server
/// runs the input.
pub const VIEW_SLACK: f32 = 0.25;
/// Ticks of history kept: comfortably more than the rewind cap.
const HISTORY_TICKS: Tick = TICK_RATE;

/// One remote player at one snapshot, as clients draw them. Client rendering
/// and the server's rewind both interpolate these, so both pose hitboxes
/// identically.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RemoteSample {
    /// The server tick this shows (fractional between snapshots).
    pub tick: f64,
    pub position: Vec3,
    /// Between two snapshots, the velocity from one to the other (m/s);
    /// zero on a snapshot itself. Snapshots carry no velocity.
    pub velocity: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub stance: Stance,
    /// The crouch blend, 0 standing to 1 crouched (`MoveState::crouch`).
    pub crouch: f32,
    pub sprinting: bool,
    pub on_floor: bool,
    pub ads: f32,
    pub weapon: WeaponId,
    pub reloading: bool,
    /// The reload is from an empty magazine (`PlayerView::reload_empty`).
    pub reload_empty: bool,
    pub alive: bool,
    pub shots_fired: u32,
    pub mantling: bool,
    /// The climb is a vault (`PlayerView::vaulting`).
    pub vaulting: bool,
    pub meleeing: bool,
    /// How far along a grenade throw, if throwing (`PlayerView::throw`).
    pub throw: Option<f32>,
    pub throw_kind: crate::equipment::Kind,
    pub hits: u8,
    pub hit_heading: u8,
    /// Where the latest hit landed and how hard (`protocol::HitPart`).
    pub hit_part: u8,
    /// Stride phase and reload and mantle progress (`hitbox::clock`).
    pub progress: Progress,
    /// The way the ladder the body is on is climbed (`x`, `z`), if it is on one.
    pub ladder: Option<glam::Vec2>,
    /// How far the weapon is mounted on cover, 0 to 1 (`PlayerView::mount`).
    pub mount: f32,
    /// Planting or defusing the charge, and how far along
    /// (`PlayerView::charge_progress`).
    pub charge_use: Option<(crate::snd::UseKind, f32)>,
    /// Lying prone along the ground (radians, `PlayerView::prone_slope`).
    pub prone_slope: f32,
}

impl Interpolate for Progress {
    /// The stride blends the short way round; a reload or mantle that starts
    /// over between the two snapshots jumps rather than running backward.
    fn interpolate(&self, to: &Self, t: f32) -> Self {
        let track = |a: f32, b: f32| {
            if b >= a {
                a + (b - a) * t
            } else if t < 0.5 {
                a
            } else {
                b
            }
        };
        Self {
            stride: lerp_phase(self.stride, to.stride, t),
            reload: track(self.reload, to.reload),
            mantle: track(self.mantle, to.mantle),
            turn: self.turn + crate::hitbox::gait::wrap_angle(to.turn - self.turn) * t,
            step: self.step + (to.step - self.step) * t,
        }
    }
}

impl RemoteSample {
    /// Player `v` in the snapshot sent at `tick`.
    pub fn at(tick: Tick, v: &PlayerView) -> Self {
        Self {
            tick: f64::from(tick),
            position: v.position(),
            velocity: Vec3::ZERO,
            yaw: v.yaw(),
            pitch: v.pitch(),
            stance: v.stance,
            crouch: v.crouch(),
            sprinting: v.sprinting,
            on_floor: v.on_floor,
            ads: v.ads(),
            weapon: v.weapon,
            reloading: v.reloading,
            reload_empty: v.reload_empty,
            alive: v.alive,
            shots_fired: v.shots_fired,
            mantling: v.mantling,
            vaulting: v.vaulting,
            meleeing: v.meleeing,
            throw: v.throw_progress(),
            throw_kind: v.throw_kind,
            hits: v.hits,
            hit_heading: v.hit_heading,
            hit_part: v.hit_part,
            progress: v.progress(),
            ladder: v.ladder_facing(),
            mount: f32::from(v.mount) / 255.0,
            charge_use: v.charge_progress(),
            prone_slope: v.prone_slope(),
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
        let seconds = (to.tick - self.tick) / f64::from(TICK_RATE);
        Self {
            tick: self.tick + (to.tick - self.tick) * f64::from(t),
            position: self.position.lerp(to.position, t),
            velocity: if seconds > 0.0 {
                (to.position - self.position) / seconds as f32
            } else {
                Vec3::ZERO
            },
            yaw: lerp_angle(self.yaw, to.yaw, t),
            pitch: self.pitch + (to.pitch - self.pitch) * t,
            ads: self.ads + (to.ads - self.ads) * t,
            crouch: self.crouch + (to.crouch - self.crouch) * t,
            mount: self.mount + (to.mount - self.mount) * t,
            prone_slope: self.prone_slope + (to.prone_slope - self.prone_slope) * t,
            progress: self.progress.interpolate(&to.progress, t),
            throw: match (self.throw, to.throw) {
                (Some(a), Some(b)) if b >= a => Some(a + (b - a) * t),
                _ => discrete.throw,
            },
            charge_use: match (self.charge_use, to.charge_use) {
                (Some((k, a)), Some((l, b))) if k == l && b >= a => Some((k, a + (b - a) * t)),
                _ => discrete.charge_use,
            },
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
            activity: Activity {
                sprinting: self.sprinting,
                // On a ladder the zones hang standing, facing the rungs.
                airborne: !self.on_floor && self.ladder.is_none(),
                mantling: self.mantling,
                vaulting: self.vaulting,
                reloading: self.reloading,
                ads: self.ads,
                velocity: self.velocity,
                weapon: Some(self.weapon),
                progress: self.progress,
                crouch: Some(self.crouch),
                throw: self.throw,
                kneel: self
                    .charge_use
                    .map(|(kind, t)| hitbox::action::kneel(kind, t)),
                slope: self.prone_slope,
            },
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
        self.rewind_target_for(now, view_tick, None)
    }

    /// Like [`rewind_target`](Self::rewind_target), but also no further back
    /// than the shooter's round trip (`rtt`, seconds) and the most a client's
    /// interpolation delay and input queue add ([`VIEW_SLACK`]) explain. An
    /// honest client's view tick always fits; a tampered one cannot pick an
    /// older, more favourable moment inside the rewind cap.
    pub fn rewind_target_for(&self, now: Tick, view_tick: Option<f64>, rtt: Option<f32>) -> f64 {
        let newest = f64::from(self.newest_tick().unwrap_or(now).min(now));
        let mut oldest = f64::from(now.saturating_sub(MAX_REWIND_TICKS));
        if let Some(rtt) = rtt.filter(|r| r.is_finite() && *r >= 0.0) {
            let explained = f64::from((rtt + VIEW_SLACK) * TICK_RATE as f32);
            oldest = oldest.max(f64::from(now) - explained);
        }
        view_tick
            .filter(|t| t.is_finite())
            .map_or(newest, |t| t.clamp(oldest.min(newest), newest))
    }

    /// Whether a view tick lies outside what the shooter's connection
    /// explains: further back than its round trip plus [`VIEW_SLACK`], or
    /// past the newest snapshot. The rewind cap is not counted: an honest
    /// player on a very slow connection runs into it every shot. For the
    /// integrity flags (`net::integrity`); the shot is clamped either way.
    pub fn view_tick_unexplained(
        &self,
        now: Tick,
        view_tick: Option<f64>,
        rtt: Option<f32>,
    ) -> bool {
        let Some(tick) = view_tick.filter(|t| t.is_finite()) else {
            return false;
        };
        let newest = f64::from(self.newest_tick().unwrap_or(now).min(now));
        let slack = f64::from(super::integrity::VIEW_TICK_SLACK);
        if tick > newest + slack {
            return true;
        }
        rtt.filter(|r| r.is_finite() && *r >= 0.0)
            .is_some_and(|rtt| {
                let explained = f64::from((rtt + VIEW_SLACK) * TICK_RATE as f32);
                tick < f64::from(now) - explained - slack
            })
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
                let from = RemoteSample::at(from_tick, from);
                Some(from.interpolate(&RemoteSample::at(to_tick, to), t as f32))
            }
            (Some((tick, only)), None) | (None, Some((tick, only))) => {
                Some(RemoteSample::at(tick, only))
            }
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
    /// The shape the first pellet hit (an index into `hitbox::hitboxes`),
    /// for hit reactions and the fall.
    pub part: u8,
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
            * def.zone_scale(hit.zone)
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
                part: hit.index as u8,
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
            buffer.push(tick, RemoteSample::at(tick, &v));
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
        assert_eq!(oldest.position, history.sample(7, 480.0).unwrap().position);
        assert_eq!(oldest.velocity, Vec3::ZERO, "held, not moving");
    }

    #[test]
    fn progress_blends_between_snapshots() {
        let a = Progress {
            stride: 0.9,
            reload: 0.2,
            mantle: 0.8,
            turn: 0.0,
            step: 0.0,
        };
        let b = Progress {
            stride: 0.1,
            reload: 0.4,
            mantle: 0.1,
            turn: 0.0,
            step: 0.0,
        };
        let mid = a.interpolate(&b, 0.5);
        assert!(
            mid.stride.abs() < 1e-5 || (mid.stride - 1.0).abs() < 1e-5,
            "{mid:?}"
        );
        assert!((mid.reload - 0.3).abs() < 1e-5);
        // A new mantle started: no running backward through the climb.
        assert_eq!(a.interpolate(&b, 0.25).mantle, 0.8);
        assert_eq!(a.interpolate(&b, 0.75).mantle, 0.1);
    }

    #[test]
    fn the_rewound_pose_carries_what_the_body_was_doing() {
        let mut v = view(5, 1.0, 0.3, true);
        v.sprinting = true;
        v.on_floor = false;
        v.reloading = true;
        v.mantling = true;
        v.ads = 255;
        v.weapon = WeaponId::expect("m870");
        v.stride = 64;
        v.reload_progress = 51;
        v.mantle_progress = 102;
        let pose = RemoteSample::at(10, &v).hitbox_pose().unwrap();
        assert_eq!(
            pose.activity,
            Activity {
                sprinting: true,
                airborne: true,
                mantling: true,
                vaulting: false,
                reloading: true,
                ads: 1.0,
                velocity: Vec3::ZERO,
                weapon: Some(WeaponId::expect("m870")),
                crouch: Some(0.0),
                throw: None,
                kneel: None,
                slope: 0.0,
                progress: Progress {
                    stride: 0.25,
                    reload: 0.2,
                    mantle: 0.4,
                    turn: 0.0,
                    step: 0.0,
                },
            }
        );
        // The client's hit zones and the server's rewind pose from the same
        // sample, so they are the same shapes.
        let mut history = LagHistory::default();
        history.record(10, vec![v]);
        let mut buffer = InterpolationBuffer::<RemoteSample>::default();
        buffer.push(10, RemoteSample::at(10, &v));
        let rewound = history
            .sample(5, 10.0)
            .and_then(|s| s.hitbox_pose())
            .unwrap();
        let drawn = buffer.sample(10.0).and_then(|s| s.hitbox_pose()).unwrap();
        assert_eq!(hitbox::hitboxes(&rewound), hitbox::hitboxes(&drawn));
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
        let pose = HitboxPose::new(Vec3::new(0.0, 0.0, -5.0), 0.0, 0.0, Stance::Standing);
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
    fn view_ticks_older_than_the_round_trip_explains_are_clamped() {
        let mut history = LagHistory::default();
        for tick in 0..=200 {
            history.record(tick, vec![view(1, 0.0, 0.0, true)]);
        }
        let now = 200;
        // 30 ms of ping and 0.25 s of slack explain 33.6 ticks of view delay.
        let floor = f64::from(now) - f64::from((0.03 + VIEW_SLACK) * TICK_RATE as f32);
        let honest = f64::from(now) - 20.0;
        assert_eq!(
            history.rewind_target_for(now, Some(honest), Some(0.03)),
            honest
        );
        let tampered = f64::from(now) - 34.0;
        assert!((history.rewind_target_for(now, Some(tampered), Some(0.03)) - floor).abs() < 1e-6);
        // A slow connection is still held to the rewind cap.
        let capped = history.rewind_target_for(now, Some(0.0), Some(1.0));
        assert_eq!(capped, f64::from(now - MAX_REWIND_TICKS));
        // Without a round trip, only the cap applies.
        assert_eq!(history.rewind_target(now, Some(tampered)), tampered);
    }

    #[test]
    fn a_slow_connection_is_explained_and_a_tampered_view_tick_is_not() {
        let mut history = LagHistory::default();
        for tick in 0..=200 {
            history.record(tick, vec![view(1, 0.0, 0.0, true)]);
        }
        let now = 200;
        // 400 ms of ping: the view tick lies past the rewind cap, which
        // clamps the shot but says nothing about the player.
        let slow = f64::from(now) - (0.4 + 0.04) * f64::from(TICK_RATE);
        assert!(!history.view_tick_unexplained(now, Some(slow), Some(0.4)));
        assert!(!history.view_tick_unexplained(now, Some(f64::from(now) - 5.0), Some(0.03)));
        assert!(!history.view_tick_unexplained(now, None, Some(0.03)));
        // 30 ms of ping cannot explain a view half a second old, nor one not
        // yet sent.
        assert!(history.view_tick_unexplained(now, Some(f64::from(now) - 60.0), Some(0.03)));
        assert!(history.view_tick_unexplained(now, Some(f64::from(now) + 10.0), Some(0.03)));
    }

    #[test]
    fn the_nearest_target_takes_the_bullet() {
        let def = WeaponId::expect("m4").def();
        let pose = |z| HitboxPose::new(Vec3::new(0.0, 0.0, z), 0.0, 0.0, Stance::Standing);
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
