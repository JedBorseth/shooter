//! Team colours and character materials.
//!
//! Every character kit (the soldier and the first-person arms) is
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

use godot::classes::{Shader, ShaderMaterial, Texture2D};
use godot::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Team {
    /// The local player's side: tan and coyote, blue accents.
    Allies,
    /// The other side: charcoal and black, red-orange accents.
    Enemies,
}

impl Team {
    pub fn from_index(index: i32) -> Self {
        match index {
            0 => Team::Allies,
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
}

impl Kit {
    pub fn from_material_name(name: &str) -> Option<Self> {
        match name {
            // Models built before the atlas used a single `character` material.
            "character" | "character_soldier" => Some(Kit::Soldier),
            "character_arms" => Some(Kit::Arms),
            _ => None,
        }
    }

    fn dir(self) -> &'static str {
        match self {
            Kit::Soldier => "soldier",
            Kit::Arms => "arms",
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

/// Part groups the soldier kit swaps per character (`tools/blender_gear.py`):
/// two face coverings, and the skin each one replaces.
const BALACLAVA: i32 = 1 << 1;
const GAITER: i32 = 1 << 2;
const LOWER_FACE: i32 = 1 << 3;
const UPPER_HEAD: i32 = 1 << 4;

/// Which part groups a character does not wear, from a per-character seed: a
/// balaclava, a neck gaiter over the nose, or a bare face, so a squad is not
/// a row of clones. A covering hides the skin it covers.
pub fn hidden_parts(seed: u64) -> i32 {
    match seed % 3 {
        0 => BALACLAVA | LOWER_FACE,
        1 => GAITER | LOWER_FACE | UPPER_HEAD,
        _ => BALACLAVA | GAITER,
    }
}

/// The soldier's faces: the base head and the head shapes the build tool
/// stores as blend shapes on its mesh (`heads` in the soldier kit of
/// `tools/build_character_assets.py`), each with the skin tone that goes with
/// it, multiplied into the skin's albedo (`skin_tone` in
/// `shaders/character.gdshader`; the atlas's skin is light).
pub const HEAD_TONES: [[f32; 3]; 5] = [
    // The base head: a young man, light-skinned.
    [1.0, 1.0, 1.0],
    // Dark-skinned.
    [0.42, 0.34, 0.29],
    // East Asian.
    [0.93, 0.85, 0.74],
    // Older, light-skinned.
    [0.98, 0.94, 0.9],
    // Mixed, olive-brown.
    [0.68, 0.56, 0.46],
];

/// One soldier's face: which head shape (0 is the base head, which has no
/// blend shape), how far toward it (0.7 to 1, so faces fall between the
/// shapes too), and the skin tone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Face {
    pub shape: usize,
    pub amount: f32,
    pub tone: [f32; 3],
}

impl Face {
    /// The weight of blend shape `index` (shape `index + 1`).
    pub fn blend(&self, index: usize) -> f32 {
        if self.shape == index + 1 {
            self.amount
        } else {
            0.0
        }
    }
}

/// A soldier's face from a per-character seed, independent of the face
/// covering.
pub fn face(seed: u64) -> Face {
    // A different mix of the seed's bits than `hidden_parts` uses.
    let mut x = seed ^ 0x5DEE_CE66_D1CE_4E5B;
    x = (x ^ (x >> 33)).wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    x = (x ^ (x >> 33)).wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    x ^= x >> 33;
    let shape = (x % HEAD_TONES.len() as u64) as usize;
    let amount = 0.7 + 0.3 * ((x >> 16) % 1000) as f32 / 999.0;
    Face {
        shape,
        amount,
        tone: HEAD_TONES[shape],
    }
}

thread_local! {
    static CHARACTER: RefCell<HashMap<(Team, Kit, bool), Gd<ShaderMaterial>>> =
        RefCell::new(HashMap::new());
    /// Whether first-person materials cut a magnified optic's lens out of the
    /// hands (`clip_first_person_lens`).
    static FIRST_PERSON_CLIP: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Release engine objects before GDExtension shutdown, rather than at thread exit.
pub fn clear_cache() {
    CHARACTER.with_borrow_mut(|cache| cache.clear());
}

const CHARACTER_SHADER: &str = "res://shaders/character.gdshader";

/// The first-person arms draw with a copy of `character.gdshader` that adds
/// the first-person key light (`crate::viewmodel::first_person_shader`).
/// While the local player's weapon carries a magnified optic, the copy also
/// cuts the optic's lens out of the hands, so the fingers never float in the
/// sight picture. Every character in the world keeps the plain shader: no
/// key and no `discard`, so no cost to the depth prepass.
pub fn clip_first_person_lens(on: bool) {
    FIRST_PERSON_CLIP.with(|clip| clip.set(on));
    let shader = first_person_shader(on);
    CHARACTER.with_borrow_mut(|cache| {
        for ((_, _, viewmodel), material) in cache.iter_mut() {
            if *viewmodel {
                material.set_shader(&shader);
            }
        }
    });
}

fn first_person_shader(clip: bool) -> Gd<Shader> {
    crate::viewmodel::first_person_shader(CHARACTER_SHADER, clip)
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

/// Bit masks over [`ROLES`]: (tinted, emissive).
fn role_masks() -> (i32, i32) {
    let mut tinted = 0;
    let mut emissive = 0;
    for (i, role) in ROLES.iter().enumerate() {
        if TINTED.contains(role) {
            tinted |= 1 << i;
        }
        if *role == "accent" {
            emissive |= 1 << i;
        }
    }
    (tinted, emissive)
}

fn build_character(team: Team, kit: Kit, viewmodel: bool) -> Gd<ShaderMaterial> {
    let mut m = ShaderMaterial::new_gd();
    m.set_name(&format!("{team:?}_{kit:?}"));
    m.set_shader(&if viewmodel {
        first_person_shader(FIRST_PERSON_CLIP.with(std::cell::Cell::get))
    } else {
        load::<Shader>(CHARACTER_SHADER)
    });
    let mut colours: PackedColorArray = ROLES.iter().map(|role| colour(team, role)).collect();
    colours.resize(16);
    let (tinted, emissive) = role_masks();
    m.set_shader_parameter("palette", &colours.to_variant());
    m.set_shader_parameter("tinted_roles", &tinted.to_variant());
    m.set_shader_parameter("emissive_roles", &emissive.to_variant());
    m.set_shader_parameter("viewmodel", &viewmodel.to_variant());
    // A faint rim in the team's accent, so a dark silhouette still separates
    // from a dark background at night and indoors.
    let (rim, energy) = match team {
        Team::Allies => (Color::from_rgb(0.55, 0.7, 0.9), 0.05),
        Team::Enemies => (Color::from_rgb(0.9, 0.45, 0.3), 0.07),
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
            let hidden = hidden_parts(seed);
            let worn = !hidden & (BALACLAVA | GAITER);
            assert!(worn.count_ones() <= 1, "seed {seed}");
            // Skin under a covering is hidden; bare skin is shown.
            assert_eq!(worn != 0, hidden & LOWER_FACE != 0, "seed {seed}");
        }
    }

    #[test]
    fn every_face_turns_up_and_blends_one_shape() {
        let mut seen = [false; HEAD_TONES.len()];
        for seed in 0..200 {
            let face = face(seed);
            seen[face.shape] = true;
            assert!((0.7..=1.0).contains(&face.amount), "seed {seed}");
            let blended = (0..HEAD_TONES.len() - 1)
                .filter(|&i| face.blend(i) > 0.0)
                .count();
            assert_eq!(blended, usize::from(face.shape > 0), "seed {seed}");
        }
        assert!(seen.iter().all(|&s| s), "{seen:?}");
    }
}
