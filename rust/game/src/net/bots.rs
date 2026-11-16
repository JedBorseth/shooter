//! Team Deathmatch bots on the dedicated server.
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
    /// Spawn points on each team's side, for goals on the enemy's side.
    pub team_spawns: &'a [Vec<Vector3>; 2],
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
        self.path.clear();
        self.stuck_since = (Vector3::ZERO, tick);
        self.held = Held::default();
    }

    /// Someone shot this bot from `from`.
    pub fn on_hurt(&mut self, attacker: u8, from: Vector3, tick: Tick) {
        self.hurt_from = Some((attacker, from, tick));
    }

    fn random(&mut self) -> f32 {
        self.counter = self.counter.wrapping_add(1);
        rng::unit(self.seed, self.counter)
    }

    /// Decides this tick's input for the bot `me`, whose state is `state`.
    pub fn think(
        &mut self,
        me: &BotPlayer,
        state: &Predicted,
        world: &mut BotWorld,
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
        match (self.engagement, target) {
            (Some(mut engagement), Some(target)) if self.in_sight => {
                engagement.update(&skill, TICK_DT);
                self.engagement = Some(engagement);
                self.fight(&engagement, &target, eye, state, world, &skill, &mut want);
            }
            _ => {
                if target.is_none() {
                    self.engagement = None;
                }
                self.roam(me, state, world, &skill, &mut want);
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
            FireMode::Semi => {
                // Taps, no faster than a person pulls a trigger.
                let gap = (60.0 / def.rounds_per_minute).max(def.cycle_time).max(0.15)
                    * if self.skill == Skill::Easy { 1.6 } else { 1.15 };
                if tick >= self.pause_until {
                    want.fire = true;
                    self.pause_until = tick + (gap * TICK_RATE as f32) as Tick;
                }
            }
        }
    }

    /// No enemy in sight: hunt the last one seen, face whoever shot us, or
    /// walk toward a goal.
    fn roam(
        &mut self,
        me: &BotPlayer,
        state: &Predicted,
        world: &mut BotWorld,
        skill: &SkillParams,
        want: &mut Want,
    ) {
        let tick = world.tick;
        let memory = (MEMORY_SECONDS * TICK_RATE as f32) as Tick;
        if let Some((spot, seen)) = self.last_seen
            && tick.saturating_sub(seen) < memory
            && self.goal.is_none_or(|g| g.distance_to(spot) > 3.0)
        {
            self.set_goal(spot, tick);
        }
        if self
            .goal
            .is_none_or(|g| horizontal_distance(g, me.feet) < 2.0)
        {
            let goal = self.pick_goal(me, world);
            self.set_goal(goal, tick);
        }
        if tick >= self.repath_at || self.path.is_empty() {
            let goal = self.goal.unwrap_or(me.feet);
            self.path = world.navigation.path(me.feet, goal);
            self.next_waypoint = 1.min(self.path.len());
            self.repath_at = tick + (REPATH_SECONDS * TICK_RATE as f32) as Tick;
            if self.path.len() < 2 {
                // Unreachable: try somewhere else next tick.
                self.goal = None;
            }
        }
        while self.next_waypoint < self.path.len()
            && horizontal_distance(self.path[self.next_waypoint], me.feet) < WAYPOINT_RADIUS
        {
            self.next_waypoint += 1;
        }
        let Some(waypoint) = self.path.get(self.next_waypoint).copied() else {
            return;
        };
        let direction = to_sim(waypoint - me.feet);
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

    /// Somewhere to go: the enemy's side half the time, anywhere otherwise.
    fn pick_goal(&mut self, me: &BotPlayer, world: &mut BotWorld) -> Vector3 {
        let enemy_side = &world.team_spawns[usize::from(1 - me.team.min(1))];
        if !enemy_side.is_empty() && self.random() < 0.45 {
            let i = (self.random() * enemy_side.len() as f32) as usize;
            return enemy_side[i.min(enemy_side.len() - 1)];
        }
        world.navigation.random_point()
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
    fn press(&mut self, want: Want, state: &Predicted) -> PlayerInput {
        let movement = &state.state.movement;
        let crouched = movement.stance == Stance::Crouching;
        let crouch = want.crouch != crouched && !self.held.crouch;
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

fn is_semi(state: &Predicted) -> bool {
    state.state.combat.weapon().id.def().fire_mode == FireMode::Semi
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
