//! Team colours and character materials.
//!
//! Every character kit (the soldier, the first-person arms, the zombies) is
//! one mesh with one baked texture atlas (`tools/build_character_assets.py`).
//! Each vertex carries a palette role (`uniform`, `vest`, `accent`, ...). Cloth
//! and gear roles are *tinted*: the atlas holds only their shading detail and
//! each team maps the role to a colour. Skin, eyes, lenses, and hardware keep
//! the atlas colour.
//!
//! The two teams differ in value first (light tan and coyote against dark
//! charcoal and black), so they read apart in silhouette and for colour-blind
//! players, and in accent hue second (blue against red-orange), on the helmet
//! band and armbands. See `docs/art-direction.md`.

use std::cell::RefCell;
use std::collections::HashMap;

use godot::classes::base_material_3d::{Feature, ShadingMode};
use godot::classes::{Shader, ShaderMaterial, StandardMaterial3D, Texture2D};
use godot::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Team {
    /// The local player's side: tan and coyote, blue accents.
    Allies,
    /// The other side: charcoal and black, red-orange accents.
    Enemies,
    /// The undead in Zombies: sickly skin, filthy civilian clothes, blood.
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

/// Every role, in the order the build tool numbers them (at most 16).
pub const ROLES: [&str; 13] = [
    "skin", "uniform", "gloves", "pants", "boots", "mask", "helmet", "visor", "vest", "webbing",
    "accent", "eyes", "metal",
];

/// Roles whose albedo is shading detail, coloured per team.
const TINTED: [&str; 9] = [
    "uniform", "gloves", "pants", "boots", "mask", "helmet", "vest", "webbing", "accent",
];

/// A character mesh and its texture atlas. The model's material is named
/// `character_<kit>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kit {
    Soldier,
    Arms,
    /// One of the zombie bodies (0 to 3: `zombie`, `zombie_b`, ...). Zombie
    /// kits keep their clothes' own colours: no role is tinted.
    Zombie(u8),
}

/// The zombie kits' directories, in `Kit::Zombie` order.
const ZOMBIE_KITS: [&str; 4] = ["zombie", "zombie_b", "zombie_c", "zombie_d"];

impl Kit {
    pub fn from_material_name(name: &str) -> Option<Self> {
        match name {
            // Models built before the atlas used a single `character` material.
            "character" | "character_soldier" => Some(Kit::Soldier),
            "character_arms" => Some(Kit::Arms),
            _ => {
                let dir = name.strip_prefix("character_")?;
                let i = ZOMBIE_KITS.iter().position(|k| *k == dir)?;
                Some(Kit::Zombie(i as u8))
            }
        }
    }

    fn dir(self) -> &'static str {
        match self {
            Kit::Soldier => "soldier",
            Kit::Arms => "arms",
            Kit::Zombie(i) => ZOMBIE_KITS[usize::from(i).min(ZOMBIE_KITS.len() - 1)],
        }
    }
}

#[cfg(test)]
fn role_index(role: &str) -> Option<usize> {
    ROLES.iter().position(|r| *r == role)
}

/// A team's colour for a role (linear values are what the shader multiplies
/// the detail by; these are sRGB).
pub fn colour(team: Team, role: &str) -> Color {
    let rgb = |r, g, b| Color::from_rgb(r, g, b);
    match (team, role) {
        // Civilian clothes, filthy and faded: the zombie atlas carries their
        // own colours in its detail, so these stay near neutral.
        (Team::Zombies, "uniform") => rgb(0.5, 0.48, 0.44),
        (Team::Zombies, "pants") => rgb(0.42, 0.42, 0.42),
        (Team::Zombies, "boots") => rgb(0.36, 0.33, 0.3),
        (Team::Zombies, "gloves" | "mask") => rgb(0.45, 0.43, 0.4),
        (Team::Zombies, "vest" | "webbing" | "helmet") => rgb(0.3, 0.29, 0.25),
        (Team::Zombies, "accent") => rgb(0.36, 0.03, 0.02),
        // Allies: tan combat shirt and trousers, coyote and ranger-green kit.
        (Team::Allies, "uniform") => rgb(0.6, 0.53, 0.4),
        (Team::Allies, "pants") => rgb(0.5, 0.45, 0.34),
        (Team::Allies, "gloves") => rgb(0.24, 0.21, 0.17),
        (Team::Allies, "boots") => rgb(0.46, 0.38, 0.28),
        (Team::Allies, "mask") => rgb(0.5, 0.46, 0.36),
        (Team::Allies, "helmet") => rgb(0.52, 0.46, 0.34),
        (Team::Allies, "vest") => rgb(0.45, 0.4, 0.29),
        (Team::Allies, "webbing") => rgb(0.4, 0.4, 0.3),
        (Team::Allies, "accent") => rgb(0.16, 0.56, 1.0),
        // Enemies: charcoal shirt and trousers, near-black kit, and a lighter
        // helmet so the head reads apart from the body.
        (Team::Enemies, "uniform") => rgb(0.22, 0.23, 0.24),
        (Team::Enemies, "pants") => rgb(0.2, 0.21, 0.22),
        (Team::Enemies, "gloves") => rgb(0.15, 0.15, 0.15),
        (Team::Enemies, "boots") => rgb(0.14, 0.13, 0.12),
        (Team::Enemies, "mask") => rgb(0.17, 0.17, 0.18),
        (Team::Enemies, "helmet") => rgb(0.225, 0.235, 0.225),
        (Team::Enemies, "vest") => rgb(0.15, 0.155, 0.16),
        (Team::Enemies, "webbing") => rgb(0.16, 0.165, 0.17),
        (Team::Enemies, "accent") => rgb(1.0, 0.24, 0.1),
        // Untinted roles keep the atlas colour.
        _ => rgb(1.0, 1.0, 1.0),
    }
}

/// Kit zombies have lost: the helmet, eye protection, face covering, plate
/// carrier, belt, and pads, so they read as bodies, not soldiers (and a horde
/// draws far fewer triangles). Their eyes glow instead (`SoldierRig` adds
/// them).
fn hidden(team: Team, role: &str) -> bool {
    team == Team::Zombies
        && matches!(
            role,
            "helmet" | "visor" | "mask" | "vest" | "webbing" | "metal" | "accent"
        )
}

/// Part groups the soldier kit swaps per character (`tools/blender_gear.py`):
/// two face coverings, and the skin each one replaces.
const BALACLAVA: i32 = 1 << 1;
const GAITER: i32 = 1 << 2;
const LOWER_FACE: i32 = 1 << 3;
const UPPER_HEAD: i32 = 1 << 4;

/// Which part groups a character does not wear, from a per-character seed: a
/// balaclava, a neck gaiter over the nose, or a bare face, so a squad is not
/// a row of clones. A covering hides the skin it covers.
pub fn hidden_parts(seed: u64, team: Team) -> i32 {
    if team == Team::Zombies {
        // Bare heads: no coverings, every bit of skin shown.
        return BALACLAVA | GAITER;
    }
    match seed % 3 {
        0 => BALACLAVA | LOWER_FACE,
        1 => GAITER | LOWER_FACE | UPPER_HEAD,
        _ => BALACLAVA | GAITER,
    }
}

thread_local! {
    static CHARACTER: RefCell<HashMap<(Team, Kit, bool), Gd<ShaderMaterial>>> =
        RefCell::new(HashMap::new());
}

/// Release engine objects before GDExtension shutdown, rather than at thread exit.
pub fn clear_cache() {
    CHARACTER.with_borrow_mut(|cache| cache.clear());
}

/// The one material a team's characters of a kit share:
/// `shaders/character.gdshader` with the kit's atlas and the team's colours.
/// `viewmodel` draws it with the first-person projection.
pub fn character_material(team: Team, kit: Kit, viewmodel: bool) -> Gd<ShaderMaterial> {
    CHARACTER.with(|cache| {
        cache
            .borrow_mut()
            .entry((team, kit, viewmodel))
            .or_insert_with(|| build_character(team, kit, viewmodel))
            .clone()
    })
}

/// The material for an imported surface material's name, or None if it is
/// not a character material.
pub fn material_for(name: &str, team: Team, viewmodel: bool) -> Option<Gd<ShaderMaterial>> {
    Kit::from_material_name(name).map(|kit| character_material(team, kit, viewmodel))
}

/// Bit masks over [`ROLES`]: (tinted, hidden, emissive).
pub fn role_masks(team: Team) -> (i32, i32, i32) {
    let mut tinted = 0;
    let mut hidden_mask = 0;
    let mut emissive = 0;
    for (i, role) in ROLES.iter().enumerate() {
        if TINTED.contains(role) {
            tinted |= 1 << i;
        }
        if hidden(team, role) {
            hidden_mask |= 1 << i;
        }
        if *role == "accent" && team != Team::Zombies {
            emissive |= 1 << i;
        }
    }
    (tinted, hidden_mask, emissive)
}

fn build_character(team: Team, kit: Kit, viewmodel: bool) -> Gd<ShaderMaterial> {
    let mut m = ShaderMaterial::new_gd();
    m.set_name(&format!("{team:?}_{kit:?}"));
    m.set_shader(&load::<Shader>("res://shaders/character.gdshader"));
    let mut colours: PackedColorArray = ROLES.iter().map(|role| colour(team, role)).collect();
    colours.resize(16);
    let (mut tinted, hidden_mask, emissive) = role_masks(team);
    if matches!(kit, Kit::Zombie(_)) {
        tinted = 0;
    }
    m.set_shader_parameter("palette", &colours.to_variant());
    m.set_shader_parameter("tinted_roles", &tinted.to_variant());
    m.set_shader_parameter("hidden_roles", &hidden_mask.to_variant());
    m.set_shader_parameter("emissive_roles", &emissive.to_variant());
    m.set_shader_parameter("viewmodel", &viewmodel.to_variant());
    let zombie = if team == Team::Zombies { 1.0 } else { 0.0 };
    m.set_shader_parameter("zombie", &zombie.to_variant());
    // A faint rim in the team's accent, so a dark silhouette still separates
    // from a dark background at night and indoors.
    let (rim, energy) = match team {
        Team::Allies => (Color::from_rgb(0.55, 0.7, 0.9), 0.05),
        Team::Enemies => (Color::from_rgb(0.9, 0.45, 0.3), 0.07),
        Team::Zombies => (Color::from_rgb(0.5, 0.55, 0.4), 0.03),
    };
    if !viewmodel {
        m.set_shader_parameter("rim_colour", &rim.to_variant());
        m.set_shader_parameter("rim_energy", &energy.to_variant());
    }
    let dir = kit.dir();
    for (param, map) in [
        ("albedo_texture", "albedo"),
        ("normal_texture", "normal"),
        ("orm_texture", "orm"),
    ] {
        let path = format!("res://assets/characters/{dir}/{dir}_{map}.png");
        if let Ok(texture) = try_load::<Texture2D>(&path) {
            m.set_shader_parameter(param, &texture.to_variant());
        }
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
pub fn hit_flash_material() -> Gd<ShaderMaterial> {
    let mut m = ShaderMaterial::new_gd();
    m.set_shader(&load::<Shader>("res://shaders/character_flash.gdshader"));
    set_flash(&mut m, 0.0);
    m
}

/// The hit flash's strength, 0 (off) to 1.
pub fn set_flash(material: &mut Gd<ShaderMaterial>, alpha: f32) {
    material.set_shader_parameter(
        "colour",
        &Color::from_rgba(1.0, 0.25, 0.1, alpha).to_variant(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_fit_the_vertex_encoding() {
        assert!(ROLES.len() <= 16);
        for role in TINTED {
            assert!(role_index(role).is_some(), "{role}");
        }
    }

    #[test]
    fn teams_differ_in_value_first() {
        // Relative luminance (linear) of the large areas: allies light, enemies dark.
        let lin = |v: f32| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        let lum = |c: Color| 0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b);
        for role in ["uniform", "pants", "vest", "helmet"] {
            let a = lum(colour(Team::Allies, role));
            let e = lum(colour(Team::Enemies, role));
            assert!(a > e * 4.0, "{role}: allies {a} enemies {e}");
        }
    }

    #[test]
    fn every_character_wears_at_most_one_face_covering() {
        for seed in 0..9 {
            let hidden = hidden_parts(seed, Team::Enemies);
            let worn = !hidden & (BALACLAVA | GAITER);
            assert!(worn.count_ones() <= 1, "seed {seed}");
            // Skin under a covering is hidden; bare skin is shown.
            assert_eq!(worn != 0, hidden & LOWER_FACE != 0, "seed {seed}");
        }
    }

    #[test]
    fn every_zombie_kit_is_found_by_its_material() {
        for (i, dir) in ZOMBIE_KITS.iter().enumerate() {
            let kit = Kit::from_material_name(&format!("character_{dir}"));
            assert_eq!(kit, Some(Kit::Zombie(i as u8)));
            assert_eq!(kit.unwrap().dir(), *dir);
        }
        assert_eq!(Kit::from_material_name("character_zombie_z"), None);
    }

    #[test]
    fn zombies_lose_their_helmets() {
        let (_, hidden, emissive) = role_masks(Team::Zombies);
        assert_ne!(hidden & (1 << role_index("helmet").unwrap()), 0);
        assert_eq!(emissive, 0);
        let (_, hidden, _) = role_masks(Team::Allies);
        assert_eq!(hidden, 0);
    }
}
