//! Team colours. The soldier model's materials are named after palette roles
//! (`uniform`, `vest`, `accent`, ...); each team maps every role to a colour.
//!
//! The two teams differ in value first (light tan against dark charcoal), so
//! they read apart in silhouette and for colour-blind players, and in accent
//! hue second (blue against red-orange), on the helmet band and armbands.
//! See `docs/art-direction.md`.

use std::cell::RefCell;
use std::collections::HashMap;

use godot::classes::base_material_3d::{Feature, ShadingMode};
use godot::classes::{Shader, ShaderMaterial, StandardMaterial3D, Texture2D};
use godot::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Team {
    /// The local player's side: tan and olive, blue accents.
    Allies,
    /// Everyone else until teams exist: charcoal, red-orange accents.
    Enemies,
    /// The undead in Zombies: grey-green skin, faded and blood-stained
    /// uniforms, and glowing orange eyes behind the goggles.
    Zombies,
}

impl Team {
    pub fn from_index(index: i32) -> Self {
        match index {
            0 => Team::Allies,
            2 => Team::Zombies,
            _ => Team::Enemies,
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
        // Rotting skin, also where the face mask was, so the head is bare.
        (Team::Zombies, "skin" | "mask") => rgb(0.5, 0.56, 0.44),
        (Team::Zombies, "uniform") => rgb(0.36, 0.33, 0.26),
        (Team::Zombies, "pants") => rgb(0.27, 0.25, 0.2),
        (Team::Zombies, "gloves") => rgb(0.42, 0.47, 0.37),
        (Team::Zombies, "boots") => rgb(0.12, 0.1, 0.09),
        (Team::Zombies, "vest") => rgb(0.24, 0.23, 0.18),
        (Team::Zombies, "webbing") => rgb(0.19, 0.18, 0.14),
        // Blood-soaked armbands.
        (Team::Zombies, "accent") => rgb(0.36, 0.03, 0.02),
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

/// Release engine objects before GDExtension shutdown, rather than at thread exit.
pub fn clear_cache() {
    MATERIALS.with_borrow_mut(|cache| cache.clear());
    CHARACTER.with_borrow_mut(|cache| cache.clear());
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

/// Gear zombies have lost: the helmet and goggles. Their eyes glow instead
/// (`SoldierRig` adds them).
fn hidden(team: Team, role: &str) -> bool {
    team == Team::Zombies && matches!(role, "helmet" | "visor")
}

fn build(team: Team, role: &str) -> Gd<StandardMaterial3D> {
    let mut m = StandardMaterial3D::new_gd();
    m.set_name(&format!("{team:?}_{role}"));
    if hidden(team, role) {
        // Alpha scissor at zero alpha discards every pixel, shadows included.
        m.set_transparency(godot::classes::base_material_3d::Transparency::ALPHA_SCISSOR);
        m.set_albedo(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
        return m;
    }
    let c = colour(team, role);
    m.set_albedo(c);
    m.set_roughness(match role {
        "visor" => 0.35,
        "helmet" | "vest" | "webbing" => 0.75,
        _ => 0.88,
    });
    m.set_specular(if role == "visor" { 0.55 } else { 0.35 });
    if role == "accent" && team != Team::Zombies {
        // A little self-light keeps the team colour readable in shade.
        m.set_feature(Feature::EMISSION, true);
        m.set_emission(c);
        m.set_emission_energy_multiplier(0.35);
    }
    m
}

thread_local! {
    static CHARACTER: RefCell<HashMap<(Team, bool), Gd<ShaderMaterial>>> =
        RefCell::new(HashMap::new());
}

/// The one material a team's characters use for every merged mesh (body, gear,
/// arms): `shaders/character.gdshader` with the team's colours per role.
/// `viewmodel` draws it with the first-person projection.
pub fn character_material(team: Team, viewmodel: bool) -> Gd<ShaderMaterial> {
    CHARACTER.with(|cache| {
        cache
            .borrow_mut()
            .entry((team, viewmodel))
            .or_insert_with(|| build_character(team, viewmodel))
            .clone()
    })
}

fn build_character(team: Team, viewmodel: bool) -> Gd<ShaderMaterial> {
    let mut m = ShaderMaterial::new_gd();
    m.set_shader(&load::<Shader>("res://shaders/character.gdshader"));
    let colours: PackedColorArray = ROLES.iter().map(|role| colour(team, role)).collect();
    let roughness: PackedFloat32Array = ROLES
        .iter()
        .map(|role| match *role {
            "visor" => 0.35,
            "helmet" | "vest" | "webbing" => 0.75,
            _ => 0.88,
        })
        .collect();
    let mut hidden = 0;
    let mut emissive = 0;
    for (i, role) in ROLES.iter().enumerate() {
        if super::palette::hidden(team, role) {
            hidden |= 1 << i;
        }
        if *role == "accent" && team != Team::Zombies {
            emissive |= 1 << i;
        }
    }
    m.set_shader_parameter("palette", &colours.to_variant());
    m.set_shader_parameter("role_roughness", &roughness.to_variant());
    m.set_shader_parameter("hidden_roles", &hidden.to_variant());
    m.set_shader_parameter("emissive_roles", &emissive.to_variant());
    m.set_shader_parameter("viewmodel", &viewmodel.to_variant());
    let zombie = if team == Team::Zombies { 1.0 } else { 0.0 };
    m.set_shader_parameter("zombie", &zombie.to_variant());
    if let Ok(weave) = try_load::<Texture2D>("res://assets/textures/canvas/albedo.jpg") {
        m.set_shader_parameter("weave", &weave.to_variant());
    }
    m
}

/// A zombie's glowing eyes: unshaded and bright enough to bloom.
pub fn zombie_eye_material() -> Gd<StandardMaterial3D> {
    let mut m = StandardMaterial3D::new_gd();
    m.set_shading_mode(ShadingMode::UNSHADED);
    m.set_albedo(Color::from_rgb(1.0, 0.62, 0.15));
    m.set_feature(Feature::EMISSION, true);
    m.set_emission(Color::from_rgb(1.0, 0.5, 0.08));
    m.set_emission_energy_multiplier(6.0);
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
