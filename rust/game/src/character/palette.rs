//! Team colours. The soldier model's materials are named after palette roles
//! (`uniform`, `vest`, `accent`, ...); each team maps every role to a colour.
//!
//! The two teams differ in value first (light tan against dark charcoal), so
//! they read apart in silhouette and for colour-blind players, and in accent
//! hue second (blue against red-orange), on the helmet band and armbands.
//! See `docs/art-direction.md`.

use std::cell::RefCell;
use std::collections::HashMap;

use godot::classes::StandardMaterial3D;
use godot::classes::base_material_3d::{Feature, ShadingMode};
use godot::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Team {
    /// The local player's side: tan and olive, blue accents.
    Allies,
    /// Everyone else until teams exist: charcoal, red-orange accents.
    Enemies,
}

impl Team {
    pub fn from_index(index: i32) -> Self {
        if index == 0 {
            Team::Allies
        } else {
            Team::Enemies
        }
    }
}

/// Every role the model uses.
pub const ROLES: [&str; 11] = [
    "skin", "uniform", "gloves", "pants", "boots", "mask", "helmet", "visor", "vest", "webbing",
    "accent",
];

fn colour(team: Team, role: &str) -> Color {
    let rgb = |r, g, b| Color::from_rgb(r, g, b);
    match (team, role) {
        (_, "skin") => rgb(0.6, 0.43, 0.33),
        (_, "visor") => rgb(0.04, 0.05, 0.06),
        (Team::Allies, "uniform") => rgb(0.56, 0.51, 0.39),
        (Team::Allies, "pants") => rgb(0.5, 0.46, 0.35),
        (Team::Allies, "gloves") => rgb(0.2, 0.18, 0.15),
        (Team::Allies, "boots") => rgb(0.34, 0.27, 0.19),
        (Team::Allies, "mask") => rgb(0.45, 0.42, 0.33),
        (Team::Allies, "helmet") => rgb(0.47, 0.45, 0.33),
        (Team::Allies, "vest") => rgb(0.4, 0.39, 0.28),
        (Team::Allies, "webbing") => rgb(0.33, 0.32, 0.23),
        (Team::Allies, "accent") => rgb(0.16, 0.56, 1.0),
        (Team::Enemies, "uniform") => rgb(0.2, 0.21, 0.22),
        (Team::Enemies, "pants") => rgb(0.17, 0.18, 0.19),
        (Team::Enemies, "gloves") => rgb(0.07, 0.07, 0.07),
        (Team::Enemies, "boots") => rgb(0.07, 0.065, 0.06),
        (Team::Enemies, "mask") => rgb(0.09, 0.09, 0.1),
        (Team::Enemies, "helmet") => rgb(0.13, 0.135, 0.14),
        (Team::Enemies, "vest") => rgb(0.11, 0.115, 0.12),
        (Team::Enemies, "webbing") => rgb(0.085, 0.09, 0.095),
        (Team::Enemies, "accent") => rgb(1.0, 0.24, 0.1),
        _ => rgb(1.0, 0.0, 1.0),
    }
}

thread_local! {
    static MATERIALS: RefCell<HashMap<(Team, &'static str), Gd<StandardMaterial3D>>> =
        RefCell::new(HashMap::new());
}

/// The shared material for a team's role, or None for an unknown role.
pub fn material(team: Team, role: &str) -> Option<Gd<StandardMaterial3D>> {
    let role = *ROLES.iter().find(|r| **r == role)?;
    Some(MATERIALS.with(|cache| {
        cache
            .borrow_mut()
            .entry((team, role))
            .or_insert_with(|| build(team, role))
            .clone()
    }))
}

fn build(team: Team, role: &str) -> Gd<StandardMaterial3D> {
    let mut m = StandardMaterial3D::new_gd();
    m.set_name(&format!("{team:?}_{role}"));
    let c = colour(team, role);
    m.set_albedo(c);
    m.set_roughness(match role {
        "visor" => 0.35,
        "helmet" | "vest" | "webbing" => 0.75,
        _ => 0.88,
    });
    m.set_specular(if role == "visor" { 0.55 } else { 0.35 });
    if role == "accent" {
        // A little self-light keeps the team colour readable in shade.
        m.set_feature(Feature::EMISSION, true);
        m.set_emission(c);
        m.set_emission_energy_multiplier(0.35);
    }
    m
}

/// An unshaded additive overlay that tints the whole body when hit.
pub fn hit_flash_material() -> Gd<StandardMaterial3D> {
    let mut m = StandardMaterial3D::new_gd();
    m.set_shading_mode(ShadingMode::UNSHADED);
    m.set_transparency(godot::classes::base_material_3d::Transparency::ALPHA);
    m.set_blend_mode(godot::classes::base_material_3d::BlendMode::ADD);
    m.set_albedo(Color::from_rgba(1.0, 0.25, 0.1, 0.0));
    m
}
