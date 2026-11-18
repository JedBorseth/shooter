//! Bots on the dedicated server, for Team Deathmatch and Capture the Flag.
//!
//! A bot plays through the same `PlayerInput` a client sends, so it moves and
//! shoots by the players' rules. Each tick its [`BotBrain`] decides that input:
//!
//! - **Perception**, a few times a second: enemies inside its field of view (or
//!   very close, or the one that just shot it) with a clear line from its eye.
//! - **Fighting**: it turns toward the target no faster than its skill allows,
//!   with an aim error that shrinks the longer it watches, waits out a reaction
//!   time, then fires in bursts (single taps for semi-automatics), aims down
//!   sights at range, and strafes.
//! - **Moving**: it walks a path on the navigation mesh toward a goal: where an
//!   enemy was last seen, a spot on the enemy's side, or anywhere on the map.
//!   It sprints when nothing is happening, and jumps or picks a new goal when
//!   stuck.
//! - **Objectives** (Capture the Flag): the server hands each bot a
//!   [`BotObjective`] every tick: run a flag somewhere, hunt toward a spot, or
//!   hold one. A bot running a flag keeps moving while it fights; the others
//!   stop to fight as usual.
//!
//! The aim model and skill levels are engine-free (`shooter_sim::bot`). The
//! navigation mesh is baked when the server loads its map, from the map's
//! collision ([`BotNavigation`]).

use godot::classes::{ClassDb, Engine, Node, Node3D, Object, PhysicsDirectSpaceState3D, Resource};
use godot::prelude::*;
use shooter_sim::bot::{self, Engagement, Skill, SkillParams};
use shooter_sim::look;
use shooter_sim::movement::Stance;
use shooter_sim::net::Tick;
use shooter_sim::net::protocol::Predicted;
use shooter_sim::player::{PlayerInput, PlayerParams};
use shooter_sim::tdm::Team;
use shooter_sim::weapon::FireMode;
use shooter_sim::{TICK_DT, TICK_RATE, Vec2, Vec3, rng};

use crate::convert::{to_godot, to_sim};
use crate::target::WORLD_LAYER;

/// Bots look for enemies every this many ticks (spread over the bots).
const PERCEPTION_INTERVAL: Tick = 6;
/// Farthest a bot notices an enemy, in metres.
const SIGHT_RANGE: f32 = 90.0;
/// Enemies this close are noticed whichever way the bot faces (footsteps).
const HEARING_RANGE: f32 = 7.0;
/// A waypoint counts as reached within this many metres (horizontally).
const WAYPOINT_RADIUS: f32 = 0.8;
/// Seconds between path updates.
const REPATH_SECONDS: f32 = 2.0;
/// Seconds a bot keeps hunting the spot where it last saw an enemy.
const MEMORY_SECONDS: f32 = 4.0;
/// Seconds of not getting anywhere before a bot jumps, and picks a new goal.
const STUCK_SECONDS: f32 = 1.2;
/// Aim down sights beyond this range (not with a shotgun).
const ADS_RANGE: f32 = 12.0;
/// Within this many metres of an objective, a bot walks straight at it
/// instead of following the path (flags are touched, not just reached).
const OBJECTIVE_APPROACH: f32 = 2.5;
/// A held spot counts as reached within this many metres.
const HOLD_RADIUS: f32 = 1.5;
/// An attacker this close to its objective rushes it like a runner, shooting
/// on the move instead of stopping to fight.
const RUSH_RADIUS: f32 = 12.0;
/// A defender chases an enemy it lost sight of only this close to its spot.
const DEFEND_CHASE: f32 = 18.0;
/// An attacker chases an enemy it lost sight of only this close to itself.
const ATTACK_CHASE: f32 = 12.0;
/// Objectives farther than this get a detour through the middle of the map,
/// so attackers spread over the lanes instead of all taking the shortest.
const DETOUR_BEYOND: f32 = 35.0;
/// A detour point counts as reached within this many metres.
const DETOUR_RADIUS: f32 = 4.0;
/// Health below which a bot under fire breaks off to find cover.
const COVER_HEALTH: f32 = 45.0;
/// Longest a bot stays in cover, in seconds (it leaves once healed).
const COVER_SECONDS: f32 = 4.0;
/// How far a bot looks for cover, in metres.
const COVER_RANGE: (f32, f32) = (3.5, 9.0);
/// Chance a bot with no objective follows one of the map's lanes rather
/// than roaming.
const LANE_CHANCE: f32 = 0.75;
/// A teammate this close to a lane's points counts as on that lane, metres.
const LANE_RADIUS: f32 = 9.0;

/// What the server wants a bot to go for (Capture the Flag).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BotObjective {
    pub goal: Vector3,
    pub kind: ObjectiveKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectiveKind {
    /// Get there whatever happens: keep moving while fighting (a carrier
    /// running home, a bot going for a dropped flag).
    Run,
    /// Head there, but stop to fight and chase what it sees on the way.
    Hunt,
    /// Stand guard there.
    Hold,
}

impl BotObjective {
    pub fn run(goal: Vector3) -> Self {
        Self {
            goal,
            kind: ObjectiveKind::Run,
        }
    }

    pub fn hunt(goal: Vector3) -> Self {
        Self {
            goal,
            kind: ObjectiveKind::Hunt,
        }
    }

    pub fn hold(goal: Vector3) -> Self {
        Self {
            goal,
            kind: ObjectiveKind::Hold,
        }
    }

    /// Whether a bot at `feet` keeps moving to this objective through a
    /// fight: always when running, and when hunting once it is close.
    fn rushes(&self, feet: Vector3) -> bool {
        match self.kind {
            ObjectiveKind::Run => true,
            ObjectiveKind::Hunt => horizontal_distance(self.goal, feet) < RUSH_RADIUS,
            ObjectiveKind::Hold => false,
        }
    }
}

/// The navigation mesh bots walk on, baked from the map's collision.
pub struct BotNavigation {
    server: Gd<Object>,
    map: Rid,
}

impl BotNavigation {
    /// Bakes a navigation mesh from `map`'s static colliders on the world
    /// layer and adds its region under `parent`. Both must be in the tree.
    pub fn bake(parent: &mut Gd<Node>, map: &Gd<Node>) -> Option<Self> {
        let mut server = Engine::singleton().get_singleton("NavigationServer3D")?;
        let db = ClassDb::singleton();
        let mut mesh = db
            .instantiate("NavigationMesh")
            .try_to::<Gd<Resource>>()
            .ok()?;
        let settings: [(&str, Variant); 6] = [
            ("agent_radius", 0.4.to_variant()),
            ("agent_height", 1.8.to_variant()),
            ("agent_max_climb", 0.4.to_variant()),
            ("agent_max_slope", 40.0.to_variant()),
            // Static colliders only: hidden blockers count, visual trim does not.
            ("geometry_parsed_geometry_type", 1.to_variant()),
            (
                "geometry_collision_mask",
                i64::from(WORLD_LAYER).to_variant(),
            ),
        ];
        for (key, value) in settings {
            mesh.set(key, &value);
        }
        let source = db
            .instantiate("NavigationMeshSourceGeometryData3D")
            .try_to::<Gd<Resource>>()
            .ok()?;
        server.call(
            "parse_source_geometry_data",
            &[mesh.to_variant(), source.to_variant(), map.to_variant()],
        );
        server.call(
            "bake_from_source_geometry_data",
            &[mesh.to_variant(), source.to_variant()],
        );
        let polygons = mesh
            .call("get_polygon_count", &[])
            .try_to::<i64>()
            .unwrap_or(0);
        if polygons == 0 {
            godot_warn!("server: the bots' navigation mesh is empty");
            return None;
        }
        let mut region = db
            .instantiate("NavigationRegion3D")
            .try_to::<Gd<Node3D>>()
            .ok()?;
        region.set_name("BotNavigation");
        region.set("navigation_mesh", &mesh.to_variant());
        parent.add_child(&region);
        let map_rid = region.get_world_3d()?.get_navigation_map();
        godot_print!("server: baked the bots' navigation mesh ({polygons} polygons)");
        Some(Self {
            server,
            map: map_rid,
        })
    }

    /// The path from `from` to `to`, or empty if there is none.
    pub fn path(&mut self, from: Vector3, to: Vector3) -> Vec<Vector3> {
        let args = [
            self.map.to_variant(),
            from.to_variant(),
            to.to_variant(),
            true.to_variant(),
        ];
        self.server
            .call("map_get_path", &args)
            .try_to::<PackedVector3Array>()
            .map(|p| p.to_vec())
            .unwrap_or_default()
    }

    /// The point on the mesh nearest `to`.
    pub fn closest_point(&mut self, to: Vector3) -> Vector3 {
        let args = [self.map.to_variant(), to.to_variant()];
        self.server
            .call("map_get_closest_point", &args)
            .try_to::<Vector3>()
            .unwrap_or(to)
    }

    /// A random point on the mesh.
    pub fn random_point(&mut self) -> Vector3 {
        let args = [self.map.to_variant(), 1.to_variant(), false.to_variant()];
        self.server
            .call("map_get_random_point", &args)
            .try_to::<Vector3>()
            .unwrap_or(Vector3::ZERO)
    }
}

/// What a bot knows about every player this tick.
#[derive(Clone, Copy, Debug)]
pub struct BotPlayer {
    pub id: u8,
    pub team: Team,
    pub alive: bool,
    pub feet: Vector3,
    pub stance: Stance,
}

impl BotPlayer {
    fn eye(&self, params: &PlayerParams) -> Vector3 {
        self.feet + Vector3::UP * params.movement.eye_height(self.stance)
    }

    /// Where a bot aims on this player: the head, or the middle of the chest.
    fn aim_point(&self, params: &PlayerParams, head: bool) -> Vector3 {
        let eye = self.eye(params);
        if head {
            eye + Vector3::UP * 0.05
        } else {
            eye - Vector3::UP * 0.4
        }
    }
}

/// The world a bot thinks in.
pub struct BotWorld<'a> {
    pub tick: Tick,
    pub players: &'a [BotPlayer],
    pub params: &'a PlayerParams,
    pub navigation: &'a mut BotNavigation,
    pub space: &'a mut Gd<PhysicsDirectSpaceState3D>,
    /// Spawn points on each team's own side, by team, for goals on the
    /// enemy's side. Teams can change sides (Search and Destroy's halftime).
    pub team_spawns: &'a [Vec<Vector3>; 2],
    /// The map's lanes (`BotLanes`), each a line of points from the west to
    /// the east.
    pub lanes: &'a [Vec<Vector3>],
}

pub struct BotBrain {
    skill: Skill,
    seed: u32,
    /// View yaw and pitch.
    look: Vec2,
    engagement: Option<Engagement>,
    /// Whether the engaged target was in sight at the last check.
    in_sight: bool,
    /// An enemy's position when last seen, and the tick.
    last_seen: Option<(Vector3, Tick)>,
    /// Where the last shot that hurt this bot came from, and the tick.
    hurt_from: Option<(u8, Vector3, Tick)>,
    goal: Option<Vector3>,
    /// Lane points still to visit, in order, before the enemy's side.
    route: Vec<Vector3>,
    /// A spot out of the enemy's sight, and the tick to leave it by.
    cover: Option<(Vector3, Tick)>,
    /// After a goal turned out unreachable: no new goal before this tick.
    replan_at: Tick,
    path: Vec<Vector3>,
    next_waypoint: usize,
    repath_at: Tick,
    stuck_since: (Vector3, Tick),
    stuck_strikes: u32,
    strafe: f32,
    strafe_until: Tick,
    burst_until: Tick,
    pause_until: Tick,
    /// Toggles and taps need a release between presses.
    held: Held,
    counter: u32,
    /// The detour on the way to a far objective, picked once a life, and
    /// whether this life already picked (or finished) one.
    detour: Option<Vector3>,
    detour_picked: bool,
    /// Lanes taken and times in cover, over the bot's life on the server
    /// (tests).
    pub lanes_taken: u32,
    pub covers_taken: u32,
}

#[derive(Default)]
struct Held {
    sprint: bool,
    crouch: bool,
    jump: bool,
    fire: bool,
    reload: bool,
}

impl BotBrain {
    pub fn new(skill: Skill, seed: u32) -> Self {
        Self {
            skill,
            seed,
            look: Vec2::ZERO,
            engagement: None,
            in_sight: false,
            last_seen: None,
            hurt_from: None,
            goal: None,
            route: Vec::new(),
            cover: None,
            replan_at: 0,
            path: Vec::new(),
            next_waypoint: 0,
            repath_at: 0,
            stuck_since: (Vector3::ZERO, 0),
            stuck_strikes: 0,
            strafe: 1.0,
            strafe_until: 0,
            burst_until: 0,
            pause_until: 0,
            held: Held::default(),
            counter: 0,
            detour: None,
            detour_picked: false,
            lanes_taken: 0,
            covers_taken: 0,
        }
    }

    /// The bot respawned facing `yaw`: forget the last life.
    pub fn on_spawn(&mut self, yaw: f32, tick: Tick) {
        self.look = Vec2::new(yaw, 0.0);
        self.engagement = None;
        self.in_sight = false;
        self.last_seen = None;
        self.hurt_from = None;
        self.goal = None;
        self.route.clear();
        self.cover = None;
        self.path.clear();
        self.stuck_since = (Vector3::ZERO, tick);
        self.held = Held::default();
        self.detour = None;
        self.detour_picked = false;
    }

    /// Someone shot this bot from `from`.
    pub fn on_hurt(&mut self, attacker: u8, from: Vector3, tick: Tick) {
        self.hurt_from = Some((attacker, from, tick));
    }

    fn random(&mut self) -> f32 {
        self.counter = self.counter.wrapping_add(1);
        rng::unit(self.seed, self.counter)
    }

    /// Decides this tick's input for the bot `me`, whose state is `state`,
    /// going for `objective` if the server gave one.
    pub fn think(
        &mut self,
        me: &BotPlayer,
        state: &Predicted,
        world: &mut BotWorld,
        objective: Option<BotObjective>,
    ) -> PlayerInput {
        let skill = self.skill.params();
        let tick = world.tick;
        if !me.alive {
            self.engagement = None;
            return PlayerInput {
                yaw: self.look.x,
                pitch: self.look.y,
                ..Default::default()
            }
            .quantized();
        }
        if (tick + u32::from(me.id)).is_multiple_of(PERCEPTION_INTERVAL) {
            self.perceive(me, world, &skill);
        }
        let eye = me.eye(world.params);
        let target = self.engagement.and_then(|e| {
            world
                .players
                .iter()
                .find(|p| p.id == e.target && p.alive)
                .copied()
        });
        let mut want = Want::default();
        // Hurt and under fire: break off and get out of sight to heal. A
        // runner (a flag carrier) keeps running.
        let running = objective.is_some_and(|o| o.kind == ObjectiveKind::Run);
        if self.cover.is_none() && self.skill != Skill::Easy && !running {
            self.consider_cover(me, state, target.as_ref(), world);
        }
        if let Some((spot, until)) = self.cover {
            if tick >= until || state.state.health >= 90.0 || running {
                self.cover = None;
                self.goal = None;
            } else {
                self.hide(me, spot, target.as_ref(), state, world, &skill, &mut want);
                self.unstick(me, tick, &mut want);
                return self.press(want, state);
            }
        }
        match (self.engagement, target) {
            (Some(mut engagement), Some(target)) if self.in_sight => {
                engagement.update(&skill, TICK_DT);
                self.engagement = Some(engagement);
                self.fight(&engagement, &target, eye, state, world, &skill, &mut want);
                // A runner shoots on the move instead of strafing in place.
                if let Some(objective) = objective
                    && objective.rushes(me.feet)
                    && let Some(direction) = self.toward(me, world, objective.goal)
                {
                    want.wish = bot::wish_toward(self.look.x, direction);
                    want.crouch = false;
                    want.aim = false;
                }
            }
            _ => {
                if target.is_none() {
                    self.engagement = None;
                }
                self.roam(me, state, world, &skill, objective, &mut want);
            }
        }
        self.unstick(me, tick, &mut want);
        self.press(want, state)
    }

    /// Looks for enemies and picks the one to fight.
    fn perceive(&mut self, me: &BotPlayer, world: &mut BotWorld, skill: &SkillParams) {
        let eye = me.eye(world.params);
        let facing = to_godot(look::view_direction(self.look.x, 0.0));
        let tick = world.tick;
        let attacker = self
            .hurt_from
            .filter(|(_, _, t)| tick.saturating_sub(*t) < TICK_RATE)
            .map(|(id, _, _)| id);
        let current = self.engagement.map(|e| e.target);
        let mut best: Option<(u8, f32, Vector3)> = None;
        let mut current_visible = false;
        for enemy in world
            .players
            .iter()
            .filter(|p| p.alive && p.team != me.team)
        {
            let point = enemy.aim_point(world.params, false);
            let to = point - eye;
            let distance = to.length();
            if distance > SIGHT_RANGE {
                continue;
            }
            let flat = Vector3::new(to.x, 0.0, to.z).normalized_or_zero();
            let in_view = flat.dot(facing) >= (skill.fov * 0.5).cos();
            let noticed = in_view
                || distance < HEARING_RANGE
                || Some(enemy.id) == attacker
                || Some(enemy.id) == current;
            if !noticed || !clear_line(world.space, eye, point) {
                continue;
            }
            if Some(enemy.id) == current {
                current_visible = true;
            }
            // The current target wins ties, so bots do not flick between targets.
            let score = distance - if Some(enemy.id) == current { 8.0 } else { 0.0 };
            if best.is_none_or(|(_, b, _)| score < b) {
                best = Some((enemy.id, score, enemy.feet));
            }
        }
        self.in_sight = best.is_some();
        match best {
            Some((id, _, feet)) => {
                self.last_seen = Some((feet, tick));
                if current != Some(id) || !current_visible {
                    let seed = rng::hash(self.seed, tick);
                    self.engagement = Some(Engagement::new(id, skill, seed));
                    // A fresh target: pick a new strafe (and maybe crouch).
                    self.strafe_until = 0;
                }
            }
            None => {
                self.engagement = None;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn fight(
        &mut self,
        engagement: &Engagement,
        target: &BotPlayer,
        eye: Vector3,
        state: &Predicted,
        world: &BotWorld,
        skill: &SkillParams,
        want: &mut Want,
    ) {
        let point = target.aim_point(world.params, engagement.aim_head);
        let bearing = look::bearing(to_sim(eye), to_sim(point));
        let goal = bearing + engagement.error;
        self.look = bot::turn_toward(self.look, goal, skill.turn_rate, TICK_DT);
        let distance = eye.distance_to(point);
        let tick = world.tick;
        let weapon = state.state.combat.weapon();
        let def = weapon.id.def();

        // Strafe, changing direction now and then.
        if tick >= self.strafe_until {
            self.strafe = if self.random() < 0.5 { -1.0 } else { 1.0 };
            let seconds = 0.6 + self.random() * 0.9;
            self.strafe_until = tick + (seconds * TICK_RATE as f32) as Tick;
            want.crouch = distance > 18.0 && self.random() < 0.25;
        } else {
            want.crouch = state.state.movement.stance == Stance::Crouching;
        }
        // Close in on far targets, back off from very near ones.
        let forward = if distance > 35.0 {
            0.6
        } else if distance < 4.0 && def.pellets == 1 {
            -0.4
        } else {
            0.0
        };
        want.wish = Vec2::new(self.strafe * 0.8, forward);
        want.aim = distance > ADS_RANGE && def.pellets == 1;
        if weapon.magazine == 0 {
            want.reload = true;
            return;
        }

        let on_target =
            bot::view_gap(self.look, bearing) <= skill.fire_cone + 0.6 / distance.max(1.0);
        if !engagement.reacted(skill) || !on_target {
            return;
        }
        match def.fire_mode {
            FireMode::Auto => {
                if tick >= self.pause_until && tick >= self.burst_until {
                    let burst = skill.burst * (0.7 + self.random() * 0.6);
                    self.burst_until = tick + (burst * TICK_RATE as f32) as Tick;
                    self.pause_until =
                        self.burst_until + (skill.burst_pause * TICK_RATE as f32) as Tick;
                }
                want.fire = tick < self.burst_until;
            }
            FireMode::Semi | FireMode::Burst { .. } => {
                // Taps (one per burst), no faster than a person pulls a trigger.
                let between = match def.fire_mode {
                    FireMode::Burst { rounds, delay } => {
                        60.0 / def.rounds_per_minute * f32::from(rounds) + delay
                    }
                    _ => 60.0 / def.rounds_per_minute,
                };
                let gap = between.max(def.cycle_time).max(0.15)
                    * if self.skill == Skill::Easy { 1.6 } else { 1.15 };
                if tick >= self.pause_until {
                    want.fire = true;
                    self.pause_until = tick + (gap * TICK_RATE as f32) as Tick;
                }
            }
        }
    }

    /// Starts taking cover when hurt badly and recently shot, picking a spot
    /// nearby that the threat (the target, or whoever shot us) cannot see.
    fn consider_cover(
        &mut self,
        me: &BotPlayer,
        state: &Predicted,
        target: Option<&BotPlayer>,
        world: &mut BotWorld,
    ) {
        let tick = world.tick;
        let shot_at = self
            .hurt_from
            .filter(|(_, _, t)| tick.saturating_sub(*t) < TICK_RATE * 3 / 2);
        if state.state.health >= COVER_HEALTH || shot_at.is_none() {
            return;
        }
        let threat = match (target, shot_at) {
            (Some(target), _) => target.eye(world.params),
            (None, Some((_, from, _))) => from,
            (None, None) => return,
        };
        if let Some(spot) = self.find_cover(me, threat, world) {
            let until = tick + (COVER_SECONDS * TICK_RATE as f32) as Tick;
            self.cover = Some((spot, until));
            self.covers_taken += 1;
            self.set_goal(spot, tick);
        }
    }

    /// The nearest reachable spot within [`COVER_RANGE`] where a standing
    /// player's chest is hidden from `threat`, not toward it.
    fn find_cover(
        &mut self,
        me: &BotPlayer,
        threat: Vector3,
        world: &mut BotWorld,
    ) -> Option<Vector3> {
        let start = self.random() * std::f32::consts::TAU;
        let mut best: Option<(Vector3, f32)> = None;
        for k in 0..12 {
            let angle = start + k as f32 * std::f32::consts::TAU / 12.0;
            let reach = COVER_RANGE.0 + self.random() * (COVER_RANGE.1 - COVER_RANGE.0);
            let probe = me.feet + Vector3::new(angle.cos(), 0.0, angle.sin()) * reach;
            let spot = world.navigation.closest_point(probe);
            let distance = horizontal_distance(spot, me.feet);
            if distance < 2.0 || (spot.y - me.feet.y).abs() > 1.5 {
                continue;
            }
            // Not closer to the threat than we are now.
            if horizontal_distance(spot, threat) < horizontal_distance(me.feet, threat) - 1.0 {
                continue;
            }
            if best.is_some_and(|(_, d)| d <= distance) {
                continue;
            }
            if clear_line(world.space, threat, spot + Vector3::UP * 1.2) {
                continue;
            }
            best = Some((spot, distance));
        }
        best.map(|(spot, _)| spot)
    }

    /// Runs to the cover spot, then crouches there, reloads, and watches for
    /// the threat.
    #[allow(clippy::too_many_arguments)]
    fn hide(
        &mut self,
        me: &BotPlayer,
        spot: Vector3,
        target: Option<&BotPlayer>,
        state: &Predicted,
        world: &mut BotWorld,
        skill: &SkillParams,
        want: &mut Want,
    ) {
        let weapon = state.state.combat.weapon();
        let def = weapon.id.def();
        let direction = (horizontal_distance(spot, me.feet) > 1.2)
            .then(|| self.toward(me, world, spot))
            .flatten();
        if let Some(direction) = direction {
            let mut view = look::bearing(Vec3::ZERO, Vec3::new(direction.x, 0.0, direction.z));
            view.y = 0.0;
            self.look = bot::turn_toward(self.look, view, skill.turn_rate * 0.7, TICK_DT);
            want.wish = bot::wish_toward(self.look.x, direction);
            want.sprint = want.wish.y > 0.8;
        } else {
            want.wish = Vec2::ZERO;
            want.crouch = true;
            // Face where the threat would come from.
            let from = target.map(|t| t.feet).or(self.hurt_from.map(|(_, f, _)| f));
            if let Some(from) = from {
                let mut view = look::bearing(to_sim(me.eye(world.params)), to_sim(from));
                view.y = 0.0;
                self.look = bot::turn_toward(self.look, view, skill.turn_rate * 0.6, TICK_DT);
            }
        }
        want.reload = weapon.reserve > 0 && weapon.magazine < def.magazine;
        want.fire = false;
    }

    /// No enemy in sight: go for the objective, hunt the last enemy seen,
    /// face whoever shot us, or walk toward a goal.
    fn roam(
        &mut self,
        me: &BotPlayer,
        state: &Predicted,
        world: &mut BotWorld,
        skill: &SkillParams,
        objective: Option<BotObjective>,
        want: &mut Want,
    ) {
        let tick = world.tick;
        let memory = (MEMORY_SECONDS * TICK_RATE as f32) as Tick;
        // Searched where the enemy was last seen, and they are gone: stop
        // going back there.
        if self
            .last_seen
            .is_some_and(|(spot, _)| horizontal_distance(spot, me.feet) < 2.0)
        {
            self.last_seen = None;
        }
        // Without an objective, chase the last enemy seen. Defenders chase
        // only near their spot, attackers only near themselves, and runners
        // never: the objective comes first.
        let chase = |spot: Vector3| match objective {
            None => true,
            Some(o) => match o.kind {
                ObjectiveKind::Run => false,
                ObjectiveKind::Hold => o.goal.distance_to(spot) < DEFEND_CHASE,
                ObjectiveKind::Hunt => me.feet.distance_to(spot) < ATTACK_CHASE,
            },
        };
        let hunting = if let Some((spot, seen)) = self.last_seen
            && tick.saturating_sub(seen) < memory
            && chase(spot)
        {
            if self.goal.is_none_or(|g| g.distance_to(spot) > 3.0) {
                self.set_goal(spot, tick);
            }
            true
        } else {
            false
        };
        let direction = match objective {
            Some(objective) if !hunting => {
                if objective.kind == ObjectiveKind::Hold
                    && horizontal_distance(objective.goal, me.feet) < HOLD_RADIUS
                {
                    self.guard(me, world, skill, objective.goal, want);
                    return;
                }
                let goal = match objective.kind {
                    ObjectiveKind::Hunt => self.detour_to(me, world, objective.goal),
                    _ => objective.goal,
                };
                self.toward(me, world, goal)
            }
            _ => {
                if tick >= self.replan_at
                    && self
                        .goal
                        .is_none_or(|g| horizontal_distance(g, me.feet) < 2.0)
                {
                    let goal = self.pick_goal(me, world);
                    self.set_goal(goal, tick);
                }
                self.follow_path(me, world)
            }
        };
        let Some(direction) = direction else {
            return;
        };
        // Look where we are going; turn toward a shot from behind first.
        let mut goal_view = look::bearing(Vec3::ZERO, Vec3::new(direction.x, 0.0, direction.z));
        goal_view.y = 0.0;
        let recently_hurt = self
            .hurt_from
            .filter(|(_, _, t)| tick.saturating_sub(*t) < TICK_RATE * 2);
        if let Some((_, from, _)) = recently_hurt {
            goal_view = look::bearing(to_sim(me.eye(world.params)), to_sim(from));
        }
        self.look = bot::turn_toward(self.look, goal_view, skill.turn_rate * 0.7, TICK_DT);
        want.wish = bot::wish_toward(self.look.x, direction);
        let calm = self
            .last_seen
            .is_none_or(|(_, t)| tick.saturating_sub(t) > TICK_RATE * 3)
            && recently_hurt.is_none();
        want.sprint = calm && want.wish.y > 0.8;
        // Stand up to move.
        want.crouch = false;
        let weapon = state.state.combat.weapon();
        let def = weapon.id.def();
        want.reload =
            calm && weapon.reserve > 0 && u32::from(weapon.magazine) * 2 < u32::from(def.magazine);
    }

    fn set_goal(&mut self, goal: Vector3, tick: Tick) {
        self.goal = Some(goal);
        self.repath_at = tick;
    }

    /// The direction to walk toward `goal`: along the navigation mesh, and
    /// straight at it for the last couple of metres. `None` when there is
    /// nowhere to go (no path yet, or already there).
    fn toward(&mut self, me: &BotPlayer, world: &mut BotWorld, goal: Vector3) -> Option<Vec3> {
        if horizontal_distance(goal, me.feet) < OBJECTIVE_APPROACH
            && (goal.y - me.feet.y).abs() < 1.5
        {
            let direction = to_sim(goal - me.feet);
            return (Vec3::new(direction.x, 0.0, direction.z).length() > 0.2).then_some(direction);
        }
        // A goal that moved (a carrier on the run) needs a fresh path.
        if self.goal.is_none_or(|g| g.distance_to(goal) > 2.0) {
            self.set_goal(goal, world.tick);
        }
        self.follow_path(me, world)
    }

    /// Where to head for `goal`: the first time this life that a far goal
    /// comes up, a random point in the middle third of the way there (on the
    /// navigation mesh) to pass through first; then the goal itself.
    fn detour_to(&mut self, me: &BotPlayer, world: &mut BotWorld, goal: Vector3) -> Vector3 {
        if !self.detour_picked {
            self.detour_picked = true;
            let span = goal - me.feet;
            let length = horizontal_distance(goal, me.feet);
            if length > DETOUR_BEYOND {
                let axis = Vector3::new(span.x, 0.0, span.z) / length;
                self.detour = (0..12).map(|_| world.navigation.random_point()).find(|p| {
                    let along = (*p - me.feet).dot(axis) / length;
                    (0.3..0.7).contains(&along)
                });
            }
        }
        match self.detour {
            Some(detour) if horizontal_distance(detour, me.feet) > DETOUR_RADIUS => detour,
            _ => {
                self.detour = None;
                goal
            }
        }
    }

    /// The direction to the next waypoint on the path to `self.goal`,
    /// repathing every couple of seconds.
    fn follow_path(&mut self, me: &BotPlayer, world: &mut BotWorld) -> Option<Vec3> {
        let tick = world.tick;
        if tick >= self.repath_at || self.path.is_empty() {
            let goal = self.goal?;
            self.path = world.navigation.path(me.feet, goal);
            self.next_waypoint = 1.min(self.path.len());
            self.repath_at = tick + (REPATH_SECONDS * TICK_RATE as f32) as Tick;
            if self.path.len() < 2 {
                // Unreachable: drop the route and plan again in a moment.
                self.goal = None;
                self.route.clear();
                self.replan_at = tick + TICK_RATE / 2;
            }
        }
        while self.next_waypoint < self.path.len()
            && horizontal_distance(self.path[self.next_waypoint], me.feet) < WAYPOINT_RADIUS
        {
            self.next_waypoint += 1;
        }
        let waypoint = self.path.get(self.next_waypoint).copied()?;
        Some(to_sim(waypoint - me.feet))
    }

    /// Stands guard at a held spot: crouched now and then, looking around
    /// slowly, and turning toward a shot.
    fn guard(
        &mut self,
        me: &BotPlayer,
        world: &BotWorld,
        skill: &SkillParams,
        spot: Vector3,
        want: &mut Want,
    ) {
        let tick = world.tick;
        if tick >= self.strafe_until {
            // A new direction to watch every couple of seconds.
            let seconds = 1.5 + self.random() * 2.0;
            self.strafe_until = tick + (seconds * TICK_RATE as f32) as Tick;
            self.strafe = (self.random() - 0.5) * std::f32::consts::TAU;
        }
        let mut goal_view = Vec2::new(self.strafe, 0.0);
        if let Some((_, from, t)) = self.hurt_from
            && tick.saturating_sub(t) < TICK_RATE * 2
        {
            goal_view = look::bearing(to_sim(me.eye(world.params)), to_sim(from));
        }
        self.look = bot::turn_toward(self.look, goal_view, skill.turn_rate * 0.4, TICK_DT);
        let _ = spot;
        want.crouch = rng::unit(self.seed, tick / (TICK_RATE * 6)) < 0.4;
    }

    /// Somewhere to go: the enemy's side half the time, anywhere otherwise.
    fn pick_goal(&mut self, me: &BotPlayer, world: &mut BotWorld) -> Vector3 {
        // The next point of the lane being followed, or a new lane: the one
        // with the fewest teammates on it.
        if !self.route.is_empty() {
            return self.route.remove(0);
        }
        if !world.lanes.is_empty() && self.random() < LANE_CHANCE {
            self.route = self.lane_route(me, world);
            if !self.route.is_empty() {
                self.lanes_taken += 1;
                return self.route.remove(0);
            }
        }
        let enemy_side = &world.team_spawns[usize::from(1 - me.team.min(1))];
        if !enemy_side.is_empty() && self.random() < 0.45 {
            let i = (self.random() * enemy_side.len() as f32) as usize;
            return enemy_side[i.min(enemy_side.len() - 1)];
        }
        world.navigation.random_point()
    }

    /// The rest of the least crowded lane, from where the bot is toward the
    /// enemy's side, then a spawn there. Empty at the far end already.
    fn lane_route(&mut self, me: &BotPlayer, world: &mut BotWorld) -> Vec<Vector3> {
        let mut best: Option<(usize, f32)> = None;
        for (i, lane) in world.lanes.iter().enumerate() {
            let crowd = world
                .players
                .iter()
                .filter(|p| p.alive && p.id != me.id && p.team == me.team)
                .filter(|p| {
                    lane.iter()
                        .any(|q| horizontal_distance(*q, p.feet) < LANE_RADIUS)
                })
                .count() as f32;
            let score = crowd + self.random() * 0.9;
            if best.is_none_or(|(_, b)| score < b) {
                best = Some((i, score));
            }
        }
        let Some((lane, _)) = best else {
            return Vec::new();
        };
        // Lanes run west to east. A team heads away from its own spawns,
        // wherever they are this round; without sides (free-for-all) team 0
        // heads east.
        let team = usize::from(me.team.min(1));
        let mean_x = |points: &[Vector3]| {
            (!points.is_empty())
                .then(|| points.iter().map(|p| p.x).sum::<f32>() / points.len() as f32)
        };
        let eastward = match (
            mean_x(&world.team_spawns[team]),
            mean_x(&world.team_spawns[1 - team]),
        ) {
            (Some(own), Some(enemy)) => own < enemy,
            _ => match me.team {
                0 => true,
                1 => false,
                _ => me.feet.x < 0.0,
            },
        };
        let mut points = world.lanes[lane].clone();
        if !eastward {
            points.reverse();
        }
        let ahead = |p: &Vector3| {
            if eastward {
                p.x > me.feet.x + 3.0
            } else {
                p.x < me.feet.x - 3.0
            }
        };
        let mut route: Vec<Vector3> = points.into_iter().filter(ahead).collect();
        if route.is_empty() {
            return route;
        }
        let enemy_side = &world.team_spawns[1 - team];
        if !enemy_side.is_empty() {
            let i = (self.random() * enemy_side.len() as f32) as usize;
            route.push(enemy_side[i.min(enemy_side.len() - 1)]);
        }
        route
    }

    /// Jumps when the bot stops getting anywhere, and gives up on the goal if
    /// that does not help.
    fn unstick(&mut self, me: &BotPlayer, tick: Tick, want: &mut Want) {
        let moving = want.wish.length() > 0.5 && self.engagement.is_none();
        let (since_pos, since_tick) = self.stuck_since;
        if !moving || horizontal_distance(since_pos, me.feet) > 0.6 {
            self.stuck_since = (me.feet, tick);
            self.stuck_strikes = 0;
            return;
        }
        if tick.saturating_sub(since_tick) > (STUCK_SECONDS * TICK_RATE as f32) as Tick {
            want.jump = true;
            self.stuck_strikes += 1;
            self.stuck_since = (me.feet, tick);
            if self.stuck_strikes >= 2 {
                self.goal = None;
                self.path.clear();
                self.stuck_strikes = 0;
            }
        }
    }

    /// Turns what the bot wants into an input, releasing toggles and taps
    /// between presses.
    fn press(&mut self, mut want: Want, state: &Predicted) -> PlayerInput {
        let movement = &state.state.movement;
        // A bot never means to slide. A slide counts as down, so a bot that
        // wants to crouch doesn't press again and cancel it.
        let crouched = movement.stance != Stance::Standing;
        // Crouching while sprinting would slide. Let go of the stick for a
        // tick instead, which ends the sprint, and crouch on the next one.
        let ends_sprint = want.crouch && !crouched && movement.sprinting;
        if ends_sprint {
            want.wish = Vec2::ZERO;
            want.sprint = false;
        }
        let crouch = want.crouch != crouched && !self.held.crouch && !ends_sprint;
        let sprint = want.sprint && !movement.sprinting && !self.held.sprint;
        let jump = want.jump && !self.held.jump;
        let fire = want.fire && !(self.held.fire && is_semi(state));
        let reload = want.reload && !self.held.reload;
        self.held = Held {
            sprint,
            crouch,
            jump,
            fire,
            reload,
        };
        PlayerInput {
            wish: want.wish,
            yaw: self.look.x,
            pitch: self.look.y,
            jump,
            sprint,
            crouch,
            fire,
            aim: want.aim,
            reload,
            ..Default::default()
        }
        .quantized()
    }
}

#[derive(Default)]
struct Want {
    wish: Vec2,
    sprint: bool,
    crouch: bool,
    jump: bool,
    fire: bool,
    aim: bool,
    reload: bool,
}

/// Whether the trigger must be let go between shots (semi-automatics and bursts).
fn is_semi(state: &Predicted) -> bool {
    !matches!(
        state.state.combat.weapon().id.def().fire_mode,
        FireMode::Auto
    )
}

fn horizontal_distance(a: Vector3, b: Vector3) -> f32 {
    Vector2::new(a.x - b.x, a.z - b.z).length()
}

/// Whether nothing on the map lies between `from` and `to`.
fn clear_line(space: &mut Gd<PhysicsDirectSpaceState3D>, from: Vector3, to: Vector3) -> bool {
    let Some(query) = godot::classes::PhysicsRayQueryParameters3D::create_ex(from, to)
        .collision_mask(WORLD_LAYER)
        .done()
    else {
        return false;
    };
    space.intersect_ray(&query).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shooter_sim::movement::MoveEnv;
    use shooter_sim::player::{self, PlayerState};

    /// Steps a bot that holds `want` through the players' rules on open, flat
    /// ground, and returns every stance it passed through.
    fn run(
        brain: &mut BotBrain,
        p: &mut Predicted,
        want: impl Fn() -> Want,
        ticks: u32,
    ) -> Vec<Stance> {
        let params = PlayerParams::default();
        let env = MoveEnv {
            on_floor: true,
            can_stand: true,
            ledge: None,
        };
        let mut stances = Vec::new();
        for _ in 0..ticks {
            let input = brain.press(want(), p);
            player::step(&mut p.state, &input, env, &params, TICK_DT, &mut Vec::new());
            stances.push(p.state.movement.stance);
        }
        stances
    }

    fn sprinting_bot() -> (BotBrain, Predicted) {
        let mut brain = BotBrain::new(Skill::Normal, 7);
        let mut p = Predicted {
            state: PlayerState::spawned(&PlayerParams::default()),
            position: Vec3::ZERO,
            on_floor: true,
        };
        let run_in = || Want {
            wish: Vec2::new(0.0, 1.0),
            sprint: true,
            ..Default::default()
        };
        run(&mut brain, &mut p, run_in, 120);
        assert!(p.state.movement.sprinting, "the bot sprints first");
        (brain, p)
    }

    #[test]
    fn a_bot_reaching_cover_at_a_sprint_crouches_without_sliding() {
        let (mut brain, mut p) = sprinting_bot();
        let take_cover = || Want {
            crouch: true,
            ..Default::default()
        };
        let stances = run(&mut brain, &mut p, take_cover, 120);
        assert!(!stances.contains(&Stance::Sliding), "{stances:?}");
        assert_eq!(p.state.movement.stance, Stance::Crouching);
    }

    #[test]
    fn a_bot_crouching_mid_sprint_crouches_without_sliding() {
        let (mut brain, mut p) = sprinting_bot();
        let crouch_running = || Want {
            wish: Vec2::new(0.0, 1.0),
            crouch: true,
            ..Default::default()
        };
        let stances = run(&mut brain, &mut p, crouch_running, 120);
        assert!(!stances.contains(&Stance::Sliding), "{stances:?}");
        assert_eq!(p.state.movement.stance, Stance::Crouching);
    }
}
