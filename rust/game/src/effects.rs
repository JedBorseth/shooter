//! Combat effects as this client draws them: bullet impacts, blood, and the
//! marks they leave, from fixed pools sized by the graphics preset.
//!
//! **Pools.** Nothing is instanced per shot. Impact effects (`impact.tscn`:
//! sparks, dust, debris, blood) come from a ring of [`Budget::impacts`]
//! nodes, each hidden again once its particles are done so an idle pool
//! draws nothing. Bullet holes and blood spatter are separate decal pools
//! that keep their marks on the walls and recycle the oldest first, so a
//! firefight leaves its history on the map the way Call of Duty's does,
//! within a per-preset bound ([`BUDGETS`]).
//!
//! **Characters stay clean.** Decals project onto everything inside their
//! box, so characters and first-person models draw on their own visual layer
//! ([`NO_DECALS_LAYER`], set by [`keep_decals_off`]) that no decal touches.
//!
//! **Blood** is dark and lit, never additive: droplets that stretch along
//! their flight, a darker mist that hangs for a moment, and spatter on a wall
//! within [`SPLAT_REACH`] behind the target (one ray per hit).
//!
//! The local player adds one [`CombatEffects`] to itself, so the dedicated
//! server, which has no local player, pays nothing for any of it.

use std::collections::{HashMap, VecDeque};

use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::{
    Decal, INode3D, Mesh, MeshInstance3D, Node3D, PackedScene, PhysicsRayQueryParameters3D,
    Texture2D, VisualInstance3D, decal::DecalTexture,
};
use godot::prelude::*;
use shooter_sim::rng;
use shooter_sim::weapon::WeaponId;

use crate::bullets::ShotTracer;
use crate::surfaces::Surface;
use crate::target::WORLD_LAYER;

/// Group the effects node joins, so anything can find it.
pub const GROUP: &str = "combat_effects";
/// The visual layer characters and first-person models draw on. Decals
/// project onto every other layer.
pub const NO_DECALS_LAYER: u32 = 1 << 1;
/// What decals project onto: everything but [`NO_DECALS_LAYER`].
const DECAL_CULL_MASK: u32 = 0xF_FFFF & !NO_DECALS_LAYER;

const IMPACT_SCENE: &str = "res://scenes/effects/impact.tscn";
const EXPLOSION_SCENE: &str = "res://scenes/effects/explosion.tscn";
const FIRE_SCENE: &str = "res://scenes/effects/fire.tscn";
/// Explosions playing at once; a fourth reuses the oldest.
const EXPLOSIONS: usize = 3;
/// Seconds an explosion's particles last (its smoke column the longest).
const EXPLOSION_LIFETIME: f32 = 5.5;
/// A frag's scorch mark across (metres), and how deep it projects.
const SCORCH_SIZE: f32 = 2.6;
/// Within this distance (metres) an explosion shakes the camera, hardest
/// close up.
pub const SHAKE_RADIUS: f32 = 22.0;
/// Seconds an impact's particles can last; the node is hidden after this.
const IMPACT_LIFETIME: f32 = 1.4;
/// Blood reaches a wall this far behind the target (metres).
pub const SPLAT_REACH: f32 = 2.0;
/// A spatter closer than this (metres) to an earlier one replaces it.
const SPLAT_SPACING: f32 = 0.35;
/// Decals fade out past this distance (metres), over [`DECAL_FADE_LENGTH`].
const DECAL_FADE_BEGIN: f32 = 40.0;
const DECAL_FADE_LENGTH: f32 = 10.0;

/// How much each preset keeps at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    /// Impact effects playing at once.
    pub impacts: usize,
    /// Bullet holes kept on the walls.
    pub holes: usize,
    /// Blood spatters kept on the walls.
    pub splats: usize,
    /// Tracers in flight at once.
    pub tracers: usize,
    /// Scorch marks kept on the ground.
    pub scorches: usize,
    /// Fires burning at once.
    pub fires: usize,
}

/// Low, Medium, High. Holes follow the audit's starting point (16, 48, 96);
/// decals cost per pixel they cover, so the small ones are cheap to keep.
pub const BUDGETS: [Budget; 3] = [
    Budget {
        impacts: 12,
        holes: 16,
        splats: 4,
        tracers: 8,
        scorches: 4,
        fires: 2,
    },
    Budget {
        impacts: 20,
        holes: 48,
        splats: 12,
        tracers: 16,
        scorches: 8,
        fires: 4,
    },
    Budget {
        impacts: 32,
        holes: 96,
        splats: 24,
        tracers: 24,
        scorches: 12,
        fires: 6,
    },
];

/// The bullet hole set per impact look (`Surface::impact_key`): each has
/// four variants (`tools/build_combat_fx.py`).
const HOLE_CLASSES: [&str; 5] = ["concrete", "metal", "wood", "glass", "dirt"];
/// A bullet hole decal's width (metres): a hole about 1 cm across in a
/// crater about 7 cm across and a soot ring round it. Exit holes blow out
/// wider.
const HOLE_SIZE: f32 = 0.16;
const EXIT_HOLE_SCALE: f32 = 1.5;
const TRACER_SCENE: &str = "res://scenes/effects/tracer.tscn";
/// How fast a tracer streak travels (m/s). Real rounds fly at 700 to 900 m/s;
/// this is slow enough to follow, as in Call of Duty.
pub const TRACER_SPEED: f32 = 320.0;
/// The streak's length and true width (metres). The shader keeps it at
/// least a pixel and a half wide at any range.
pub const TRACER_LENGTH: f32 = 7.0;
const TRACER_WIDTH: f32 = 0.018;

/// How far below an impact its chips look for a floor to land on (metres).
const FLOOR_SEARCH: f32 = 4.0;

/// A basis whose +Y is `up` (unit), turned `angle` radians about it.
fn basis_along(up: Vector3, angle: f32) -> Basis {
    let reference = if up.dot(Vector3::UP).abs() < 0.99 {
        Vector3::UP
    } else {
        Vector3::FORWARD
    };
    let x = reference.cross(up).normalized();
    let z = x.cross(up);
    Basis::from_cols(x, up, z) * Basis::from_axis_angle(Vector3::UP, angle)
}

/// A hole's turn about its normal: any angle, except wood, whose splinters
/// run along the grain, here taken as level on walls (plank cladding) with a
/// little wander.
fn hole_angle(look: &str, up: Vector3, seed: u32) -> f32 {
    if look != "wood" || up.y.abs() > 0.7 {
        return rng::unit(seed, 11) * std::f32::consts::TAU;
    }
    // basis_along's X is level on a wall, so a turn of 0 or half a turn
    // lays the splinters along it.
    let flip = if rng::unit(seed, 19) < 0.5 {
        0.0
    } else {
        std::f32::consts::PI
    };
    flip + rng::signed(seed, 23) * 0.2
}

/// The quad every tracer draws, with the tracer shader: `tracer.tscn`'s, so
/// pooled tracers and the scene draw the same streak.
fn tracer_mesh() -> Option<Gd<Mesh>> {
    let node = try_load::<PackedScene>(TRACER_SCENE)
        .ok()?
        .instantiate()?
        .try_cast::<MeshInstance3D>()
        .ok()?;
    let mesh = node.get_mesh();
    node.free();
    mesh
}

/// A decal's albedo, normal, and ORM maps, `textures/effects/<stem>_*.png`.
fn load_maps(stem: &str) -> Option<[Gd<Texture2D>; 3]> {
    let base = format!("res://assets/textures/effects/{stem}");
    let [a, n, o] = ["albedo", "normal", "orm"]
        .map(|kind| try_load::<Texture2D>(&format!("{base}_{kind}.png")).ok());
    Some([a?, n?, o?])
}

/// How big a spatter is when the wall is `distance` metres behind the hit:
/// close walls take a tight, dense mark, further ones a wider spray.
pub fn splat_size(distance: f32) -> f32 {
    0.7 + 0.4 * (distance / SPLAT_REACH).clamp(0.0, 1.0)
}

/// Puts every visual under `root` on [`NO_DECALS_LAYER`], so bullet holes and
/// blood never project onto it. For characters and first-person models.
pub fn keep_decals_off(root: &Gd<Node>) {
    if let Ok(mut visual) = root.clone().try_cast::<VisualInstance3D>() {
        visual.set_layer_mask(NO_DECALS_LAYER);
    }
    for child in root.get_children().iter_shared() {
        keep_decals_off(&child);
    }
}

/// A ring of decals that recycles the oldest first.
#[derive(Default)]
struct DecalRing {
    nodes: VecDeque<Gd<Decal>>,
}

impl DecalRing {
    /// The next decal to place: a new one while under `budget`, else the
    /// oldest. Extra decals over a lowered budget are freed.
    fn take(&mut self, parent: &mut Gd<Node3D>, budget: usize) -> Option<Gd<Decal>> {
        if budget == 0 {
            self.trim(0);
            return None;
        }
        self.trim(budget);
        let decal = if self.nodes.len() < budget {
            let mut decal = Decal::new_alloc();
            decal.set_cull_mask(DECAL_CULL_MASK);
            decal.set_enable_distance_fade(true);
            decal.set_distance_fade_begin(DECAL_FADE_BEGIN);
            decal.set_distance_fade_length(DECAL_FADE_LENGTH);
            parent.add_child(&decal);
            decal
        } else {
            self.nodes.pop_front()?
        };
        self.nodes.push_back(decal.clone());
        Some(decal)
    }

    /// A decal already within `radius` of `at`, taken out of its place in
    /// the ring and put at the back as the newest, to be placed again.
    fn take_near(&mut self, at: Vector3, radius: f32) -> Option<Gd<Decal>> {
        let index = self
            .nodes
            .iter()
            .position(|d| d.get_global_position().distance_to(at) < radius)?;
        let decal = self.nodes.remove(index)?;
        self.nodes.push_back(decal.clone());
        Some(decal)
    }

    fn trim(&mut self, budget: usize) {
        while self.nodes.len() > budget {
            if let Some(mut old) = self.nodes.pop_front() {
                old.queue_free();
            }
        }
    }
}

/// How hard (0 to 1) an explosion `size` times a frag's shakes a camera
/// `distance` metres away: full within 3 m, falling off to nothing at
/// [`SHAKE_RADIUS`] (scaled by size).
pub fn blast_shake(distance: f32, size: f32) -> f32 {
    let radius = SHAKE_RADIUS * size.max(0.1).sqrt();
    let t = ((distance - 3.0) / (radius - 3.0)).clamp(0.0, 1.0);
    (1.0 - t) * (1.0 - t)
}

/// Where a tracer's streak is after it has flown `travelled` metres of a
/// path `distance` long: (tail, head) distances from the start. The head
/// stops at the end of the path and the tail follows it in.
pub fn tracer_span(travelled: f32, distance: f32) -> (f32, f32) {
    let head = travelled.min(distance);
    let tail = (travelled - TRACER_LENGTH).clamp(0.0, distance);
    (tail, head)
}

/// One pooled tracer in flight.
struct Tracer {
    node: Gd<MeshInstance3D>,
    from: Vector3,
    direction: Vector3,
    distance: f32,
    travelled: f32,
}

impl Tracer {
    /// Places the streak; false once it has finished.
    fn place(&mut self) -> bool {
        let (tail, head) = tracer_span(self.travelled, self.distance);
        if tail >= self.distance {
            self.node.set_visible(false);
            return false;
        }
        let span = (head - tail).max(0.001);
        // The quad's Y runs along the streak, X across it; the shader turns
        // it to face the camera.
        let side = basis_along(self.direction, 0.0).col_a();
        let basis = Basis::from_cols(
            side * TRACER_WIDTH,
            self.direction * span,
            side.cross(self.direction),
        );
        let centre = self.from + self.direction * (tail + span * 0.5);
        self.node
            .set_global_transform(Transform3D::new(basis, centre));
        true
    }
}

/// One pooled impact effect and how long it has been playing.
struct Impact {
    node: Gd<Node3D>,
    age: f32,
}

/// Pools and places this client's combat effects. The local player adds one.
#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct CombatEffects {
    scene: Option<Gd<PackedScene>>,
    impacts: Vec<Impact>,
    next_impact: usize,
    holes: DecalRing,
    splats: DecalRing,
    explosion_scene: Option<Gd<PackedScene>>,
    explosions: Vec<Impact>,
    next_explosion: usize,
    scorches: DecalRing,
    scorch_textures: Option<[Gd<Texture2D>; 2]>,
    /// Explosions played so far (tests).
    explosions_played: u32,
    fire_scene: Option<Gd<PackedScene>>,
    fires: Vec<Gd<Node3D>>,
    next_fire: usize,
    tracers: Vec<Tracer>,
    next_tracer: usize,
    tracer_mesh: Option<Gd<Mesh>>,
    /// Rounds traced per weapon (model key), for every Nth tracer round.
    remote_rounds: HashMap<String, u32>,
    /// Tracers started so far (tests).
    tracers_started: u32,
    /// Per `HOLE_CLASSES` entry, its variants' albedo, normal, and ORM maps.
    hole_textures: Vec<Vec<[Gd<Texture2D>; 3]>>,
    splat_textures: Vec<[Gd<Texture2D>; 3]>,
    /// Effects placed, for varied but repeatable choices.
    placed: u32,
    /// Spatters placed so far (tests).
    splats_placed: u32,
    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for CombatEffects {
    fn ready(&mut self) {
        self.base_mut().add_to_group(GROUP);
        // Children stay where they are put in the world.
        self.base_mut().set_as_top_level(true);
        self.base_mut().set_global_transform(Transform3D::IDENTITY);
        // Other players' rounds, as the shot tracer rebuilds them.
        if let Some(mut tracer) = ShotTracer::find(&self.base().get_tree()) {
            let callable = self.base().callable("on_shot_traced");
            tracer.connect("shot_traced", &callable);
        }
        self.scene = try_load::<PackedScene>(IMPACT_SCENE).ok();
        self.explosion_scene = try_load::<PackedScene>(EXPLOSION_SCENE).ok();
        self.fire_scene = try_load::<PackedScene>(FIRE_SCENE).ok();
        let scorch = ["albedo", "orm"].map(|kind| {
            try_load::<Texture2D>(&format!("res://assets/textures/effects/scorch_{kind}.png")).ok()
        });
        if let [Some(albedo), Some(orm)] = scorch {
            self.scorch_textures = Some([albedo, orm]);
        }
        self.hole_textures = HOLE_CLASSES
            .iter()
            .map(|class| {
                (0..4)
                    .filter_map(|i| load_maps(&format!("hole_{class}_{i}")))
                    .collect()
            })
            .collect();
        self.splat_textures = (0..4)
            .filter_map(|i| load_maps(&format!("blood_splat_{i}")))
            .collect();
    }

    fn process(&mut self, delta: f64) {
        let delta = delta as f32;
        for tracer in &mut self.tracers {
            if tracer.node.is_visible() {
                tracer.travelled += TRACER_SPEED * delta;
                tracer.place();
            }
        }
        for explosion in &mut self.explosions {
            if explosion.age < EXPLOSION_LIFETIME {
                explosion.age += delta;
                if explosion.age >= EXPLOSION_LIFETIME {
                    explosion.node.set_visible(false);
                }
            }
        }
        for impact in &mut self.impacts {
            if impact.age < IMPACT_LIFETIME {
                impact.age += delta;
                if impact.age >= IMPACT_LIFETIME {
                    impact.node.set_visible(false);
                }
            }
        }
    }
}

#[godot_api]
impl CombatEffects {
    /// An impact on the world at `position`, facing `normal`, in the look of
    /// `look` (`Surface::impact_key`), with a bullet hole if `hole`.
    #[func]
    pub fn impact(&mut self, position: Vector3, normal: Vector3, look: GString, hole: bool) {
        self.impact_at(position, normal, &look.to_string(), hole, false);
    }

    /// Where a round leaves the far side of cover at `position` (`normal`
    /// points out along its travel): a wider hole and a bigger spray of
    /// chips and dust along the round.
    #[func]
    pub fn exit_impact(&mut self, position: Vector3, normal: Vector3, look: GString, hole: bool) {
        self.impact_at(position, normal, &look.to_string(), hole, true);
    }

    /// An explosion at `position` on a surface facing `normal` (up for the
    /// ground), `size` times a frag grenade's: fireball, smoke column, dust,
    /// dirt, sparks, and a flash of light, a scorch mark on the surface, and
    /// a camera shake for the local player by distance. The grenades call
    /// this; anything else that blows up can too. The caller plays the bang.
    #[func]
    pub fn explosion(&mut self, position: Vector3, normal: Vector3, size: f32) {
        let normal = if normal.length_squared() > 0.0 {
            normal.normalized()
        } else {
            Vector3::UP
        };
        let floor = self.floor_below(position + normal * 0.3);
        let Some(scene) = self.explosion_scene.clone() else {
            return;
        };
        let index = if self.explosions.len() < EXPLOSIONS {
            let Some(node) = scene
                .instantiate()
                .and_then(|n| n.try_cast::<Node3D>().ok())
            else {
                return;
            };
            self.base_mut().add_child(&node);
            self.explosions.push(Impact { node, age: 0.0 });
            self.explosions.len() - 1
        } else {
            let index = self.next_explosion % EXPLOSIONS;
            self.next_explosion = index + 1;
            index
        };
        let explosion = &mut self.explosions[index];
        explosion.age = 0.0;
        explosion.node.set_visible(true);
        explosion.node.set_global_position(position);
        explosion.node.call(
            "setup",
            &[
                normal.to_variant(),
                floor.unwrap_or(-1000.0).to_variant(),
                size.to_variant(),
            ],
        );
        self.explosions_played = self.explosions_played.wrapping_add(1);
        self.place_scorch(position, normal, size);
        self.shake_camera(position, size);
    }

    /// A fire at `position`, `size` times a metre-wide patch, burning for
    /// `seconds` and then dying down: flames, smoke, embers, and a
    /// flickering light. Returns the fire node, whose `stop()` puts it out
    /// early. When every fire in the preset's pool is burning, the oldest
    /// is moved here.
    #[func]
    pub fn fire(&mut self, position: Vector3, size: f32, seconds: f32) -> Option<Gd<Node3D>> {
        let budget = self.current_budget().fires;
        while self.fires.len() > budget {
            if let Some(mut old) = self.fires.pop() {
                old.queue_free();
            }
        }
        if budget == 0 {
            return None;
        }
        // A fire that has burnt out first, else a new one, else the oldest.
        let idle = self
            .fires
            .iter()
            .position(|f| !f.clone().call("burning", &[]).booleanize());
        let index = match idle {
            Some(index) => index,
            None if self.fires.len() < budget => {
                let node = self
                    .fire_scene
                    .as_ref()
                    .and_then(|s| s.instantiate())
                    .and_then(|n| n.try_cast::<Node3D>().ok())?;
                self.base_mut().add_child(&node);
                self.fires.push(node);
                self.fires.len() - 1
            }
            None => {
                let index = self.next_fire % self.fires.len();
                self.next_fire = index + 1;
                index
            }
        };
        let mut fire = self.fires[index].clone();
        fire.set_visible(true);
        fire.set_global_position(position);
        fire.call("start", &[size.to_variant(), seconds.to_variant()]);
        Some(fire)
    }

    /// Fires burning or dying down now (tests).
    #[func]
    fn fires_burning(&self) -> i64 {
        self.fires
            .iter()
            .filter(|f| (*f).clone().call("burning", &[]).booleanize())
            .count() as i64
    }

    /// Explosions played so far (tests).
    #[func]
    fn explosions_played(&self) -> i64 {
        i64::from(self.explosions_played)
    }

    /// Scorch marks on the ground now (tests).
    #[func]
    fn scorches_kept(&self) -> i64 {
        self.scorches.nodes.len() as i64
    }

    /// A tracer round's streak flying from `from` to `to`.
    #[func]
    pub fn tracer(&mut self, from: Vector3, to: Vector3) {
        let distance = from.distance_to(to);
        if distance < 0.5 {
            return;
        }
        let budget = self.current_budget().tracers;
        while self.tracers.len() > budget {
            if let Some(mut old) = self.tracers.pop().map(|t| t.node) {
                old.queue_free();
            }
        }
        if budget == 0 {
            return;
        }
        let index = if self.tracers.len() < budget {
            if self.tracer_mesh.is_none() {
                self.tracer_mesh = tracer_mesh();
            }
            let Some(mesh) = self.tracer_mesh.clone() else {
                return;
            };
            let mut node = MeshInstance3D::new_alloc();
            node.set_mesh(&mesh);
            node.set_cast_shadows_setting(ShadowCastingSetting::OFF);
            // The shader moves the corners off the thin quad to face the camera.
            node.set_extra_cull_margin(2.0);
            self.base_mut().add_child(&node);
            self.tracers.push(Tracer {
                node,
                from,
                direction: Vector3::FORWARD,
                distance: 0.0,
                travelled: 0.0,
            });
            self.tracers.len() - 1
        } else {
            // The oldest in flight gives way.
            let index = self.next_tracer % self.tracers.len();
            self.next_tracer = index + 1;
            index
        };
        let tracer = &mut self.tracers[index];
        tracer.from = from;
        tracer.direction = (to - from) / distance;
        tracer.distance = distance;
        tracer.travelled = 0.0;
        tracer.node.set_visible(true);
        tracer.place();
        self.tracers_started = self.tracers_started.wrapping_add(1);
    }

    /// Tracers started so far, and in flight now (tests).
    #[func]
    fn tracers_started(&self) -> i64 {
        i64::from(self.tracers_started)
    }

    #[func]
    fn tracers_flying(&self) -> i64 {
        self.tracers.iter().filter(|t| t.node.is_visible()).count() as i64
    }

    /// A round into someone at `position`, travelling along `direction`:
    /// droplets and mist, and spatter on a wall close behind them.
    #[func]
    pub fn blood(&mut self, position: Vector3, direction: Vector3) {
        let direction = direction.normalized_or_zero();
        let normal = if direction == Vector3::ZERO {
            Vector3::UP
        } else {
            -direction
        };
        self.play_impact(position, normal, "flesh", direction, false, false);
        if direction != Vector3::ZERO {
            self.place_splat(position, direction);
        }
    }

    /// Draws what another player's traced round hit (`ShotTracer`'s
    /// `shot_traced`): an impact and a hole, or blood.
    #[func]
    fn on_shot_traced(
        &mut self,
        muzzle: Vector3,
        end: Vector3,
        normal: Vector3,
        surface: StringName,
        hit: bool,
        weapon: StringName,
    ) {
        // Every Nth round of each weapon is a tracer, as for the local player.
        let weapon = weapon.to_string();
        let every = WeaponId::by_key(&weapon).map_or(0, |w| w.def().tracer_every);
        let rounds = self.remote_rounds.entry(weapon).or_insert(0);
        *rounds = rounds.wrapping_add(1);
        if every > 0 && rounds.is_multiple_of(u32::from(every)) {
            self.tracer(muzzle, end);
        }
        if !hit {
            return;
        }
        let surface = Surface::from_tag(&surface.to_string()).unwrap_or_default();
        if surface == Surface::Flesh {
            self.blood(end, (end - muzzle).normalized_or_zero());
        } else {
            self.impact(end, normal, GString::from(surface.impact_key()), true);
        }
    }

    /// Bullet holes on the walls now (tests).
    #[func]
    fn holes_kept(&self) -> i64 {
        self.holes.nodes.len() as i64
    }

    /// Blood spatters on the walls now (tests).
    #[func]
    fn splats_kept(&self) -> i64 {
        self.splats.nodes.len() as i64
    }

    /// Blood spatters placed so far (tests).
    #[func]
    fn splats_placed(&self) -> i64 {
        i64::from(self.splats_placed)
    }

    /// Impact effects drawing now (tests).
    #[func]
    fn impacts_visible(&self) -> i64 {
        self.impacts.iter().filter(|i| i.node.is_visible()).count() as i64
    }

    /// The pools' current bounds, from the graphics preset (tests).
    #[func]
    fn budget(&self) -> VarDictionary {
        let b = self.current_budget();
        vdict! {
            "impacts" => b.impacts as i64,
            "holes" => b.holes as i64,
            "splats" => b.splats as i64,
            "tracers" => b.tracers as i64,
            "scorches" => b.scorches as i64,
            "fires" => b.fires as i64,
        }
    }
}

impl CombatEffects {
    /// The scene's effects node, if the local player made one.
    pub fn find(tree: &Gd<SceneTree>) -> Option<Gd<CombatEffects>> {
        tree.get_first_node_in_group(GROUP)
            .and_then(|n| n.try_cast::<CombatEffects>().ok())
    }

    /// The soot an explosion leaves where it went off.
    fn place_scorch(&mut self, position: Vector3, normal: Vector3, size: f32) {
        let Some([albedo, orm]) = self.scorch_textures.clone() else {
            return;
        };
        let budget = self.current_budget().scorches;
        let seed = self.next_seed();
        let mut parent = self.base().clone();
        let Some(mut decal) = self
            .scorches
            .take_near(position, SCORCH_SIZE * 0.4)
            .or_else(|| self.scorches.take(&mut parent, budget))
        else {
            return;
        };
        decal.set_texture(DecalTexture::ALBEDO, &albedo);
        decal.set_texture(DecalTexture::ORM, &orm);
        let across = SCORCH_SIZE * size.max(0.2);
        decal.set_size(Vector3::new(across, 1.2, across));
        decal.set_normal_fade(0.3);
        let angle = rng::unit(seed, 29) * std::f32::consts::TAU;
        decal.set_global_transform(Transform3D::new(basis_along(normal, angle), position));
    }

    /// Shakes the local player's camera for an explosion `size` times a
    /// frag's at `position`: hardest close up, nothing past [`SHAKE_RADIUS`].
    fn shake_camera(&mut self, position: Vector3, size: f32) {
        let Some(mut player) = self.base().get_parent() else {
            return;
        };
        if !player.has_method("blast_shake") {
            return;
        }
        let Some(camera) = self.base().get_viewport().and_then(|v| v.get_camera_3d()) else {
            return;
        };
        let strength = blast_shake(camera.get_global_position().distance_to(position), size);
        if strength > 0.0 {
            // Deferred: a blast can go off inside the player's own update.
            player.call_deferred("blast_shake", &[strength.to_variant()]);
        }
    }

    fn current_budget(&self) -> Budget {
        let preset = self
            .base()
            .get_node_or_null("/root/Settings")
            .map(|s| s.get("graphics_quality"))
            .and_then(|v| v.try_to::<i64>().ok())
            .unwrap_or(1);
        BUDGETS[preset.clamp(0, 2) as usize]
    }

    pub fn impact_at(
        &mut self,
        position: Vector3,
        normal: Vector3,
        look: &str,
        hole: bool,
        exit: bool,
    ) {
        self.play_impact(position, normal, look, Vector3::ZERO, hole || exit, exit);
        if hole {
            self.place_hole(position, normal, look, exit);
        }
    }

    /// The height of the floor under `position`, for chips to land on.
    fn floor_below(&self, position: Vector3) -> Option<f32> {
        let mut space = self.base().get_world_3d()?.get_direct_space_state()?;
        let query = PhysicsRayQueryParameters3D::create_ex(
            position,
            position + Vector3::DOWN * FLOOR_SEARCH,
        )
        .collision_mask(WORLD_LAYER)
        .done()?;
        let hit = space.intersect_ray(&query);
        hit.get_or_nil("position")
            .try_to::<Vector3>()
            .ok()
            .map(|p| p.y)
    }

    fn play_impact(
        &mut self,
        position: Vector3,
        normal: Vector3,
        look: &str,
        direction: Vector3,
        puff: bool,
        exit: bool,
    ) {
        let floor = if look == "flesh" || look == "metal" {
            None
        } else {
            self.floor_below(position + normal * 0.05)
        };
        let budget = self.current_budget().impacts;
        while self.impacts.len() > budget {
            if let Some(mut old) = self.impacts.pop().map(|i| i.node) {
                old.queue_free();
            }
        }
        let index = if self.impacts.len() < budget {
            let Some(node) = self
                .scene
                .as_ref()
                .and_then(|s| s.instantiate())
                .and_then(|n| n.try_cast::<Node3D>().ok())
            else {
                return;
            };
            self.base_mut().add_child(&node);
            self.impacts.push(Impact { node, age: 0.0 });
            self.impacts.len() - 1
        } else {
            let index = self.next_impact % self.impacts.len().max(1);
            self.next_impact = index + 1;
            index
        };
        let impact = &mut self.impacts[index];
        impact.age = 0.0;
        impact.node.set_visible(true);
        impact.node.set_global_position(position);
        impact.node.call(
            "setup",
            &[
                normal.to_variant(),
                GString::from(look).to_variant(),
                direction.to_variant(),
                puff.to_variant(),
                floor.unwrap_or(-1000.0).to_variant(),
                exit.to_variant(),
            ],
        );
    }

    fn next_seed(&mut self) -> u32 {
        self.placed = self.placed.wrapping_add(1);
        self.placed
    }

    fn place_hole(&mut self, position: Vector3, normal: Vector3, look: &str, exit: bool) {
        let class = HOLE_CLASSES.iter().position(|c| *c == look).unwrap_or(0);
        let seed = self.next_seed();
        let Some(maps) = self
            .hole_textures
            .get(class)
            .filter(|v| !v.is_empty())
            .map(|v| v[(rng::hash(seed, 13) as usize) % v.len()].clone())
        else {
            return;
        };
        let budget = self.current_budget().holes;
        let mut parent = self.base().clone();
        let Some(mut decal) = self.holes.take(&mut parent, budget) else {
            return;
        };
        decal.set_texture(DecalTexture::ALBEDO, &maps[0]);
        decal.set_texture(DecalTexture::NORMAL, &maps[1]);
        decal.set_texture(DecalTexture::ORM, &maps[2]);
        let size = HOLE_SIZE
            * if exit { EXIT_HOLE_SCALE } else { 1.0 }
            * (0.9 + 0.2 * rng::unit(seed, 17));
        decal.set_size(Vector3::new(size, 0.08, size));
        decal.set_normal_fade(0.5);
        let up = normal.normalized_or_zero();
        let up = if up == Vector3::ZERO { Vector3::UP } else { up };
        decal.set_global_transform(Transform3D::new(
            basis_along(up, hole_angle(look, up, seed)),
            position,
        ));
    }

    /// Continues the round past the target; a wall it meets within
    /// [`SPLAT_REACH`] takes a spatter, sized by how far behind it is.
    fn place_splat(&mut self, position: Vector3, direction: Vector3) {
        if self.splat_textures.is_empty() {
            return;
        }
        let budget = self.current_budget().splats;
        if budget == 0 {
            return;
        }
        let Some(mut space) = self
            .base()
            .get_world_3d()
            .and_then(|w| w.get_direct_space_state())
        else {
            return;
        };
        let Some(query) = PhysicsRayQueryParameters3D::create_ex(
            position + direction * 0.05,
            position + direction * SPLAT_REACH,
        )
        .collision_mask(WORLD_LAYER)
        .done() else {
            return;
        };
        let hit = space.intersect_ray(&query);
        let Ok(wall) = hit.get_or_nil("position").try_to::<Vector3>() else {
            return;
        };
        let normal = hit
            .get_or_nil("normal")
            .try_to::<Vector3>()
            .unwrap_or(-direction);
        // Only surfaces that face the spray: not one it grazes along.
        if normal.dot(direction) > -0.25 {
            return;
        }
        let seed = self.next_seed();
        let textures =
            self.splat_textures[(rng::hash(seed, 3) as usize) % self.splat_textures.len()].clone();
        // A new spatter replaces one close by rather than piling on it:
        // stacked decals cost per layer, and a spray on a spray looks the same.
        let mut parent = self.base().clone();
        let Some(mut decal) = self
            .splats
            .take_near(wall, SPLAT_SPACING)
            .or_else(|| self.splats.take(&mut parent, budget))
        else {
            return;
        };
        let size = splat_size(position.distance_to(wall)) * (0.85 + 0.3 * rng::unit(seed, 5));
        decal.set_texture(DecalTexture::ALBEDO, &textures[0]);
        decal.set_texture(DecalTexture::NORMAL, &textures[1]);
        decal.set_texture(DecalTexture::ORM, &textures[2]);
        decal.set_size(Vector3::new(size, 0.3, size));
        decal.set_normal_fade(0.4);
        let angle = rng::unit(seed, 7) * std::f32::consts::TAU;
        decal.set_global_transform(Transform3D::new(basis_along(normal, angle), wall));
        self.splats_placed = self.splats_placed.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budgets_grow_with_the_preset() {
        for pair in BUDGETS.windows(2) {
            assert!(pair[0].impacts <= pair[1].impacts);
            assert!(pair[0].holes <= pair[1].holes);
            assert!(pair[0].splats <= pair[1].splats);
        }
        assert_eq!(BUDGETS.map(|b| b.holes), [16, 48, 96]);
    }

    #[test]
    fn tracer_streaks_run_in_and_stop_at_the_hit() {
        // Leaving the muzzle: the tail stays at the start.
        assert_eq!(tracer_span(2.0, 100.0), (0.0, 2.0));
        // In flight: a full streak.
        assert_eq!(tracer_span(50.0, 100.0), (50.0 - TRACER_LENGTH, 50.0));
        // Arrived: the head waits at the hit while the tail runs in.
        assert_eq!(tracer_span(103.0, 100.0), (103.0 - TRACER_LENGTH, 100.0));
        assert_eq!(tracer_span(100.0 + TRACER_LENGTH, 100.0).0, 100.0);
    }

    #[test]
    fn blasts_shake_harder_close_up() {
        assert_eq!(blast_shake(1.0, 1.0), 1.0);
        assert!(blast_shake(8.0, 1.0) > blast_shake(15.0, 1.0));
        assert_eq!(blast_shake(SHAKE_RADIUS, 1.0), 0.0);
        assert!(blast_shake(15.0, 2.0) > blast_shake(15.0, 1.0));
    }

    #[test]
    fn spatter_widens_with_distance() {
        assert!(splat_size(0.2) < splat_size(1.5));
        assert_eq!(splat_size(10.0), splat_size(SPLAT_REACH));
    }
}
