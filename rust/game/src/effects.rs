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

use std::collections::VecDeque;

use godot::classes::{
    Decal, INode3D, Node3D, PackedScene, PhysicsRayQueryParameters3D, Texture2D, VisualInstance3D,
    decal::DecalTexture,
};
use godot::prelude::*;
use shooter_sim::rng;

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
}

/// Low, Medium, High. Holes follow the audit's starting point (16, 48, 96);
/// decals cost per pixel they cover, so the small ones are cheap to keep.
pub const BUDGETS: [Budget; 3] = [
    Budget {
        impacts: 12,
        holes: 16,
        splats: 4,
    },
    Budget {
        impacts: 20,
        holes: 48,
        splats: 12,
    },
    Budget {
        impacts: 32,
        holes: 96,
        splats: 24,
    },
];

/// The bullet hole's tint per impact look (`Surface::impact_key`): pale
/// chipped concrete, bare grey metal, light splintered wood.
fn hole_tint(look: &str) -> Color {
    match look {
        "metal" => Color::from_rgb(0.55, 0.55, 0.58),
        "wood" => Color::from_rgb(0.85, 0.66, 0.45),
        "dirt" => Color::from_rgb(0.5, 0.42, 0.34),
        "glass" => Color::from_rgb(0.75, 0.8, 0.85),
        _ => Color::from_rgb(0.85, 0.83, 0.8),
    }
}

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
    hole_textures: Option<(Gd<Texture2D>, Gd<Texture2D>)>,
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
        self.hole_textures =
            try_load::<Texture2D>("res://assets/textures/decals/bullet_hole_albedo.png")
                .ok()
                .zip(
                    try_load::<Texture2D>("res://assets/textures/decals/bullet_hole_normal.png")
                        .ok(),
                );
        for i in 0..4 {
            let base = format!("res://assets/textures/effects/blood_splat_{i}");
            let maps = ["albedo", "normal", "orm"]
                .map(|kind| try_load::<Texture2D>(&format!("{base}_{kind}.png")).ok());
            if let [Some(a), Some(n), Some(o)] = maps {
                self.splat_textures.push([a, n, o]);
            }
        }
    }

    fn process(&mut self, delta: f64) {
        let delta = delta as f32;
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
        let look = look.to_string();
        self.play_impact(position, normal, &look, Vector3::ZERO, hole);
        if hole {
            self.place_hole(position, normal, &look);
        }
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
        self.play_impact(position, normal, "flesh", direction, false);
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
        _weapon: StringName,
    ) {
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
        }
    }
}

impl CombatEffects {
    fn current_budget(&self) -> Budget {
        let preset = self
            .base()
            .get_node_or_null("/root/Settings")
            .map(|s| s.get("graphics_quality"))
            .and_then(|v| v.try_to::<i64>().ok())
            .unwrap_or(1);
        BUDGETS[preset.clamp(0, 2) as usize]
    }

    fn play_impact(
        &mut self,
        position: Vector3,
        normal: Vector3,
        look: &str,
        direction: Vector3,
        puff: bool,
    ) {
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
            ],
        );
    }

    fn next_seed(&mut self) -> u32 {
        self.placed = self.placed.wrapping_add(1);
        self.placed
    }

    fn place_hole(&mut self, position: Vector3, normal: Vector3, look: &str) {
        let Some((albedo, normal_map)) = self.hole_textures.clone() else {
            return;
        };
        let budget = self.current_budget().holes;
        let seed = self.next_seed();
        let mut parent = self.base().clone();
        let Some(mut decal) = self.holes.take(&mut parent, budget) else {
            return;
        };
        decal.set_texture(DecalTexture::ALBEDO, &albedo);
        decal.set_texture(DecalTexture::NORMAL, &normal_map);
        decal.set_size(Vector3::new(0.15, 0.2, 0.15));
        decal.set_modulate(hole_tint(look));
        let angle = rng::unit(seed, 11) * std::f32::consts::TAU;
        let up = normal.normalized_or_zero();
        let up = if up == Vector3::ZERO { Vector3::UP } else { up };
        decal.set_global_transform(Transform3D::new(basis_along(up, angle), position));
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
    fn spatter_widens_with_distance() {
        assert!(splat_size(0.2) < splat_size(1.5));
        assert_eq!(splat_size(10.0), splat_size(SPLAT_REACH));
    }
}
