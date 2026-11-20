//! One zombie's mind: who it chases, where it walks, when it swings, and
//! when it tears at its window. The dedicated server runs it for every
//! zombie and moves the bodies with the engine's physics; it is engine-free
//! so its rules can be tested here.
//!
//! There is no pathfinding on purpose: each tick the zombie heads straight
//! for the nearest standing player, and the physics slides it along whatever
//! is in the way. Two exceptions keep the straight lines from pinning zombies
//! to walls once the map has more than one yard: a zombie spawned in a side
//! yard first walks straight to that yard's gate (its waypoint), and a zombie
//! that stops getting anywhere walks straight to the open gate that best
//! leads to its target.

use glam::Vec3;

use super::{Attack, AttackEvent, ZombieRules, chase_direction, flat_distance};
use crate::hitbox::{Activity, HitboxPose};
use crate::movement::Stance;
use crate::rng;

/// Radians per second the body turns toward where it walks (the walk itself
/// always follows the straight line).
pub const TURN_RATE: f32 = 10.0;
/// A waypoint counts as reached within this many metres.
pub const WAYPOINT_REACH: f32 = 1.5;
/// Seconds of moving under a quarter of its speed before a zombie heads for
/// a gate.
pub const STUCK_SECONDS: f32 = 1.0;
/// A zombie this close to its window's barrier tears at it.
pub const BARRIER_REACH: f32 = 1.7;
/// Seconds per board torn off.
pub const TEAR_SECONDS: f32 = 1.1;
/// A hit this hard (force level, about a tenth of the damage) staggers a
/// zombie for [`STAGGER_SECONDS`]: it stops, rocks back, then comes on again.
pub const STAGGER_FORCE: u8 = 8;
pub const STAGGER_SECONDS: f32 = 0.5;
/// Each zombie walks up to this share faster or slower than its round's
/// speed (evenly spread, so the horde's average is the round's), and its
/// shamble's cadence follows: a crowd spreads out and falls out of step
/// instead of marching in a block.
pub const SPEED_JITTER: f32 = 0.08;
/// Ground speed above which a zombie counts as walking, for its hit zones.
pub const WALKING_SPEED: f32 = 0.3;

/// A zombie standing still is drawn lower on bent knees and less far forward
/// than one walking: its hit zones take this much of the walking hunch and
/// this much of a crouch.
const IDLE_HUNCH: f32 = 0.65;
const IDLE_CROUCH: f32 = 0.36;

/// This zombie's share of its round's speed, from its seed: 1 ± [`SPEED_JITTER`].
pub fn speed_factor(seed: u32) -> f32 {
    1.0 + SPEED_JITTER * (rng::unit(seed, 2) * 2.0 - 1.0)
}

/// The yaw (radians, 0 facing -Z) that faces along `direction`.
pub fn yaw_of(direction: Vec3) -> f32 {
    (-direction.x).atan2(-direction.z)
}

fn wrap_angle(angle: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    (angle + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI
}

/// A zombie's hit zones, hunched as its shamble is drawn so a headshot aims
/// at the head: walking, bent forward with the head out; standing, lower on
/// bent knees. The server's rewind and every client pose them alike.
pub fn hitbox_pose(feet: Vec3, yaw: f32, walking: bool) -> HitboxPose {
    let (hunch, crouch) = if walking {
        (1.0, 0.0)
    } else {
        (IDLE_HUNCH, IDLE_CROUCH)
    };
    HitboxPose {
        activity: Activity {
            hunch,
            crouch: Some(crouch),
            ..Default::default()
        },
        ..HitboxPose::new(feet, yaw, 0.0, Stance::Standing)
    }
}

/// A player a zombie may go for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prey {
    pub id: u8,
    /// Feet position.
    pub position: Vec3,
}

/// A boarded window, as zombies see it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Window {
    pub position: Vec3,
    /// Boards still up.
    pub boards: u8,
}

/// What a zombie knows about the world this tick.
#[derive(Clone, Copy, Debug)]
pub struct World<'a> {
    /// Standing players: downed and dead ones are ignored.
    pub prey: &'a [Prey],
    /// The open gates between the map's yards.
    pub gates: &'a [Vec3],
    pub windows: &'a [Window],
}

/// What a zombie did this tick, beyond walking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChaseEvent {
    /// A swing began (play the attack).
    Swung,
    /// The swing connected with player `id`, still in reach.
    Struck(u8),
    /// It tore a board off window `index`.
    Tore(usize),
}

/// The result of [`Chaser::plan`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Plan {
    /// Horizontal velocity to walk at (m/s).
    pub velocity: Vec3,
    pub event: Option<ChaseEvent>,
    /// Who it is after, and how far away they are (horizontally).
    pub target: Option<(u8, f32)>,
}

/// One zombie's state of mind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chaser {
    /// Ground speed (m/s), the round's gait times its own [`speed_factor`].
    pub speed: f32,
    /// Facing (radians, 0 faces -Z).
    pub yaw: f32,
    pub attack: Attack,
    /// Walk here before chasing (a gate, or the window it came through).
    pub waypoint: Option<Vec3>,
    /// The window it must get through first, until it is through.
    pub window: Option<usize>,
    stuck_for: f32,
    tear_for: f32,
    stagger_for: f32,
}

impl Chaser {
    pub fn new(speed: f32, yaw: f32) -> Self {
        Self {
            speed,
            yaw,
            attack: Attack::default(),
            waypoint: None,
            window: None,
            stuck_for: 0.0,
            tear_for: 0.0,
            stagger_for: 0.0,
        }
    }

    /// Stops it in its tracks for a moment after a heavy hit; a stagger
    /// already under way runs its course.
    pub fn stagger(&mut self) -> bool {
        if self.stagger_for > 0.0 {
            return false;
        }
        self.stagger_for = STAGGER_SECONDS;
        true
    }

    pub fn staggered(&self) -> bool {
        self.stagger_for > 0.0
    }

    /// Whether it is still outside, at or on its way to its window.
    pub fn outside(&self) -> bool {
        self.window.is_some()
    }

    /// Decides this tick: where to walk, whether to swing, tear, or strike.
    pub fn plan(&mut self, rules: &ZombieRules, at: Vec3, world: &World, dt: f32) -> Plan {
        if self.stagger_for > 0.0 {
            self.stagger_for -= dt;
            return Plan::default();
        }
        if let Some(event) = self.tear(at, world, dt) {
            return Plan {
                event: event.map(ChaseEvent::Tore),
                ..Plan::default()
            };
        }
        let target = nearest(at, world.prey);
        let distance = target.map(|p| flat_distance(at, p.position));
        let event = match self.attack.step(rules, distance, dt) {
            Some(AttackEvent::Swung) => Some(ChaseEvent::Swung),
            Some(AttackEvent::Hit) => target.map(|p| ChaseEvent::Struck(p.id)),
            Some(AttackEvent::Missed) | None => None,
        };
        if self
            .waypoint
            .is_some_and(|w| flat_distance(at, w) < WAYPOINT_REACH)
        {
            self.waypoint = None;
        }
        let mut velocity = Vec3::ZERO;
        if let Some(target) = target {
            let goal = self.waypoint.unwrap_or(target.position);
            let direction = chase_direction(at, goal);
            if direction != Vec3::ZERO {
                let turn = TURN_RATE * dt;
                self.yaw += wrap_angle(yaw_of(direction) - self.yaw).clamp(-turn, turn);
            }
            // Stand still to swing, and stop just inside reach instead of
            // pushing into the player.
            let close = distance.is_some_and(|d| d <= rules.attack_reach * 0.8);
            if !self.attack.swinging() && !close {
                velocity = direction * self.speed;
            }
        }
        Plan {
            velocity,
            event,
            target: target.zip(distance).map(|(p, d)| (p.id, d)),
        }
    }

    /// After the body moved: a zombie that wanted to walk but barely moved
    /// for [`STUCK_SECONDS`] heads for the open gate on the shortest way to
    /// its target (`goal`), then chases again.
    pub fn moved(
        &mut self,
        at: Vec3,
        wanted: Vec3,
        moved: Vec3,
        goal: Option<Vec3>,
        gates: &[Vec3],
        dt: f32,
    ) {
        let want = Vec3::new(wanted.x, 0.0, wanted.z).length();
        let got = Vec3::new(moved.x, 0.0, moved.z).length();
        if want < 0.1 || got > want * 0.25 || self.waypoint.is_some() {
            self.stuck_for = 0.0;
            return;
        }
        self.stuck_for += dt;
        if self.stuck_for < STUCK_SECONDS {
            return;
        }
        self.stuck_for = 0.0;
        let Some(goal) = goal else {
            return;
        };
        self.waypoint = gates
            .iter()
            .copied()
            .filter(|g| g.distance(at) > WAYPOINT_REACH * 2.0)
            .min_by(|a, b| {
                let cost = |g: &Vec3| g.distance(at) + g.distance(goal);
                cost(a).total_cmp(&cost(b))
            });
    }

    /// Tears at its window while next to it and boards remain. `None` when
    /// not busy with it; `Some(event)` while it is (and stands still), with
    /// the window's index on the tick a board comes off.
    fn tear(&mut self, at: Vec3, world: &World, dt: f32) -> Option<Option<usize>> {
        let index = self.window?;
        let Some(window) = world.windows.get(index) else {
            self.window = None;
            return None;
        };
        if window.boards == 0 {
            // Through the window once past it: chase from here on.
            if self.waypoint.is_none() {
                self.window = None;
            }
            return None;
        }
        if flat_distance(at, window.position) >= BARRIER_REACH {
            return None;
        }
        let to = window.position - at;
        self.yaw = yaw_of(Vec3::new(to.x, 0.0, to.z));
        self.tear_for += dt;
        if self.tear_for < TEAR_SECONDS {
            return Some(None);
        }
        self.tear_for = 0.0;
        Some(Some(index))
    }
}

/// The closest standing player, by straight-line distance.
pub fn nearest(at: Vec3, prey: &[Prey]) -> Option<Prey> {
    prey.iter().copied().min_by(|a, b| {
        a.position
            .distance_squared(at)
            .total_cmp(&b.position.distance_squared(at))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TICK_DT;

    fn world<'a>(prey: &'a [Prey], windows: &'a [Window]) -> World<'a> {
        World {
            prey,
            gates: &[],
            windows,
        }
    }

    #[test]
    fn speeds_spread_round_the_rounds_speed() {
        let factors: Vec<f32> = (0..400).map(speed_factor).collect();
        let mean = factors.iter().sum::<f32>() / factors.len() as f32;
        assert!((mean - 1.0).abs() < 0.01, "mean {mean}");
        assert!(factors.iter().all(|f| (f - 1.0).abs() <= SPEED_JITTER));
    }

    #[test]
    fn walks_straight_at_the_nearest_standing_player_and_swings_in_reach() {
        let rules = ZombieRules::default();
        let prey = [
            Prey {
                id: 1,
                position: Vec3::new(10.0, 0.0, 0.0),
            },
            Prey {
                id: 2,
                position: Vec3::new(0.0, 0.0, -4.0),
            },
        ];
        let mut zombie = Chaser::new(rules.walk_speed, 0.0);
        let mut at = Vec3::ZERO;
        let mut struck = None;
        for _ in 0..(10.0 / TICK_DT) as usize {
            let plan = zombie.plan(&rules, at, &world(&prey, &[]), TICK_DT);
            assert_eq!(plan.target.map(|t| t.0), Some(2));
            if plan.velocity != Vec3::ZERO {
                assert!(plan.velocity.normalize().dot(Vec3::NEG_Z) > 0.999);
            }
            at += plan.velocity * TICK_DT;
            if let Some(ChaseEvent::Struck(id)) = plan.event {
                struck = Some(id);
                break;
            }
        }
        assert_eq!(struck, Some(2));
        assert!(flat_distance(at, prey[1].position) <= rules.attack_reach);
        assert!(zombie.yaw.abs() < 0.01, "faces -Z, got {}", zombie.yaw);
    }

    #[test]
    fn nobody_standing_means_nothing_to_chase() {
        let rules = ZombieRules::default();
        let mut zombie = Chaser::new(rules.walk_speed, 0.0);
        let plan = zombie.plan(&rules, Vec3::ZERO, &world(&[], &[]), TICK_DT);
        assert_eq!(plan, Plan::default());
    }

    #[test]
    fn tears_its_window_down_board_by_board_then_climbs_through() {
        let rules = ZombieRules::default();
        let prey = [Prey {
            id: 1,
            position: Vec3::new(0.0, 0.0, -20.0),
        }];
        let mut windows = [Window {
            position: Vec3::new(0.0, 0.0, -1.0),
            boards: 2,
        }];
        let mut zombie = Chaser::new(rules.walk_speed, 0.0);
        zombie.window = Some(0);
        zombie.waypoint = Some(Vec3::new(0.0, 0.0, -2.0));
        let mut torn = 0;
        let mut seconds = 0.0;
        while windows[0].boards > 0 && seconds < 10.0 {
            let plan = zombie.plan(&rules, Vec3::ZERO, &world(&prey, &windows), TICK_DT);
            assert_eq!(plan.velocity, Vec3::ZERO, "stands still to tear");
            if let Some(ChaseEvent::Tore(0)) = plan.event {
                windows[0].boards -= 1;
                torn += 1;
            }
            seconds += TICK_DT;
        }
        assert_eq!(torn, 2);
        assert!((seconds - 2.0 * TEAR_SECONDS).abs() < 0.05, "{seconds}");
        // Bare: it walks on to the window's far side, then chases.
        let plan = zombie.plan(&rules, Vec3::ZERO, &world(&prey, &windows), TICK_DT);
        assert!(plan.velocity.z < 0.0);
        assert!(zombie.outside(), "outside until past the window");
        zombie.waypoint = None;
        zombie.plan(
            &rules,
            Vec3::new(0.0, 0.0, -2.0),
            &world(&prey, &windows),
            TICK_DT,
        );
        assert!(!zombie.outside());
    }

    #[test]
    fn a_stagger_stops_it_for_half_a_second() {
        let rules = ZombieRules::default();
        let prey = [Prey {
            id: 1,
            position: Vec3::new(0.0, 0.0, -20.0),
        }];
        let mut zombie = Chaser::new(rules.walk_speed, 0.0);
        assert!(zombie.stagger());
        assert!(!zombie.stagger(), "one at a time");
        let mut still = 0;
        for _ in 0..(1.0 / TICK_DT) as usize {
            if zombie
                .plan(&rules, Vec3::ZERO, &world(&prey, &[]), TICK_DT)
                .velocity
                == Vec3::ZERO
            {
                still += 1;
            }
        }
        assert!((still as f32 * TICK_DT - STAGGER_SECONDS).abs() < 0.02);
    }

    #[test]
    fn a_stuck_zombie_heads_for_the_gate_on_the_way() {
        let mut zombie = Chaser::new(1.4, 0.0);
        let gates = [Vec3::new(5.0, 0.0, 0.0), Vec3::new(-30.0, 0.0, 0.0)];
        let wanted = Vec3::new(0.0, 0.0, -1.4);
        for _ in 0..(STUCK_SECONDS / TICK_DT) as usize + 2 {
            zombie.moved(
                Vec3::ZERO,
                wanted,
                Vec3::ZERO,
                Some(Vec3::new(10.0, 0.0, -5.0)),
                &gates,
                TICK_DT,
            );
        }
        assert_eq!(zombie.waypoint, Some(gates[0]));
    }

    #[test]
    fn standing_zombies_pose_lower_than_walking_ones() {
        let walking = hitbox_pose(Vec3::ZERO, 0.0, true);
        let standing = hitbox_pose(Vec3::ZERO, 0.0, false);
        assert_eq!(walking.activity.hunch, 1.0);
        assert!(standing.activity.crouch.unwrap() > 0.0);
    }
}
