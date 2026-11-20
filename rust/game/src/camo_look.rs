//! How each weapon camo draws (`shooter_sim::camo`, docs/progression.md):
//! its pattern texture (`tools/build_camos.py`), how often the pattern
//! repeats, the coat's metal and roughness, and its style. The shader side
//! is `godot/shaders/weapon_camo.gdshaderinc`.
//!
//! A camo never adds a draw call: first person, it is a few parameters on
//! the viewmodel material each surface already has; third person and in
//! killcams, the weapon's shared material is swapped for one with the camo
//! painted in (cached per material and camo, so every player's M4A1 in Gold
//! shares one).

use std::cell::RefCell;
use std::collections::HashMap;

use godot::classes::base_material_3d::TextureParam;
use godot::classes::{BaseMaterial3D, Material, MeshInstance3D, Shader, ShaderMaterial, Texture2D};
use godot::prelude::*;
use shooter_sim::camo::Camo;

/// The shader third-person weapons wear a camo with.
const WORLD_SHADER: &str = "res://shaders/weapon_camo.gdshader";
const TEXTURE_DIR: &str = "res://assets/camos";

/// Painted pattern, metal, view-turning facets, drifting clouds.
const PATTERN: i32 = 0;
const METAL: i32 = 1;
const FACETS: i32 = 2;
const CLOUDS: i32 = 3;

/// How a camo draws.
#[derive(Clone, Copy, Debug)]
pub struct CamoLook {
    /// Pattern repeats a metre along the gun.
    pub scale: f32,
    pub metallic: f32,
    pub roughness: f32,
    pub style: i32,
}

/// The look of `camo`, or none for the factory finish.
pub fn look(camo: Camo) -> Option<CamoLook> {
    let (scale, metallic, roughness, style) = match camo.key() {
        "gravel" => (2.5, 0.05, 0.7, PATTERN),
        "dune" | "timber" => (3.0, 0.05, 0.75, PATTERN),
        "tundra" => (2.5, 0.05, 0.7, PATTERN),
        "cocoa" => (2.5, 0.05, 0.75, PATTERN),
        "riptide" => (2.5, 0.1, 0.6, PATTERN),
        "ember" => (2.5, 0.1, 0.55, PATTERN),
        "static" | "scales" => (3.0, 0.15, 0.5, PATTERN),
        "weave" => (5.0, 0.3, 0.35, PATTERN),
        "bloom" | "ink" => (2.5, 0.05, 0.62, PATTERN),
        "hazard" => (2.0, 0.1, 0.55, PATTERN),
        "circuit" => (3.0, 0.2, 0.45, PATTERN),
        "gold" => (2.0, 1.0, 0.28, METAL),
        "prism" => (5.0, 0.85, 0.16, FACETS),
        "nebula" => (1.5, 0.2, 0.3, CLOUDS),
        _ => return None,
    };
    Some(CamoLook {
        scale,
        metallic,
        roughness,
        style,
    })
}

fn texture(camo: Camo) -> Option<Gd<Texture2D>> {
    try_load::<Texture2D>(&format!("{TEXTURE_DIR}/{}.png", camo.key())).ok()
}

/// Sets `camo`'s parameters on a material using `weapon_camo.gdshaderinc`
/// (off for the factory finish).
fn set_parameters(material: &mut Gd<ShaderMaterial>, camo: Camo) {
    let (Some(look), Some(texture)) = (look(camo), texture(camo)) else {
        material.set_shader_parameter("camo_on", &false.to_variant());
        return;
    };
    material.set_shader_parameter("camo_on", &true.to_variant());
    material.set_shader_parameter("camo_texture", &texture.to_variant());
    material.set_shader_parameter("camo_scale", &look.scale.to_variant());
    material.set_shader_parameter("camo_metallic", &look.metallic.to_variant());
    material.set_shader_parameter("camo_roughness", &look.roughness.to_variant());
    material.set_shader_parameter("camo_style", &look.style.to_variant());
}

/// First person: turns `camo` on (or off) in every viewmodel material under
/// `root` that draws the weapon's maps (lenses, glows, and plain parts have
/// none, and keep their look).
pub fn apply_viewmodel(root: &Gd<Node>, camo: Camo) {
    for child in root.get_children().iter_shared() {
        if let Ok(mesh) = child.clone().try_cast::<MeshInstance3D>() {
            for surface in 0..mesh.get_surface_override_material_count() {
                let Some(material) = mesh.get_surface_override_material(surface) else {
                    continue;
                };
                let Ok(mut shader) = material.try_cast::<ShaderMaterial>() else {
                    continue;
                };
                let maps = shader.get_shader_parameter("use_maps");
                if maps.try_to::<bool>().unwrap_or(false) {
                    set_parameters(&mut shader, camo);
                }
            }
        }
        apply_viewmodel(&child, camo);
    }
}

/// A surface's material before a camo replaced it.
pub type Original = (Gd<MeshInstance3D>, i32, Option<Gd<Material>>);

/// Third person: draws every weapon surface under `root` that has the
/// weapon's maps with `camo` (its shared camo material), or, for the
/// factory finish, puts back the materials `originals` kept.
pub fn apply_world(root: &Gd<Node>, camo: Camo, originals: &mut Vec<Original>) {
    for (mut mesh, surface, material) in originals.drain(..) {
        if mesh.is_instance_valid() {
            mesh.set_surface_override_material(surface, material.as_ref());
        }
    }
    if look(camo).is_none() {
        return;
    }
    swap_world(root, camo, originals);
}

fn swap_world(root: &Gd<Node>, camo: Camo, originals: &mut Vec<Original>) {
    for child in root.get_children().iter_shared() {
        if let Ok(mut mesh) = child.clone().try_cast::<MeshInstance3D>() {
            for surface in 0..mesh.get_surface_override_material_count() {
                let Some(source) = mesh.get_active_material(surface) else {
                    continue;
                };
                let Some(painted) = world_material(&source, camo) else {
                    continue;
                };
                originals.push((
                    mesh.clone(),
                    surface,
                    mesh.get_surface_override_material(surface),
                ));
                mesh.set_surface_override_material(surface, &painted);
            }
        }
        swap_world(&child, camo, originals);
    }
}

thread_local! {
    /// Camo materials by the material they stand in for and the camo.
    static WORLD: RefCell<HashMap<(i64, u8), Gd<ShaderMaterial>>> = RefCell::new(HashMap::new());
}

/// `source` (a weapon's standard material with ORM and normal maps) with
/// `camo` painted in, shared by every weapon wearing it; none for a
/// material without the maps (lenses, glows).
fn world_material(source: &Gd<Material>, camo: Camo) -> Option<Gd<ShaderMaterial>> {
    let key = (source.instance_id().to_i64(), camo.0);
    if let Some(found) = WORLD.with_borrow(|cache| cache.get(&key).cloned()) {
        return Some(found);
    }
    let standard = source.clone().try_cast::<BaseMaterial3D>().ok()?;
    let normal = standard.get_texture(TextureParam::NORMAL)?;
    let orm = standard.get_texture(TextureParam::ROUGHNESS)?;
    let mut material = ShaderMaterial::new_gd();
    material.set_shader(&load::<Shader>(WORLD_SHADER));
    material.set_shader_parameter("albedo", &standard.get_albedo().to_variant());
    if let Some(albedo) = standard.get_texture(TextureParam::ALBEDO) {
        material.set_shader_parameter("albedo_texture", &albedo.to_variant());
    }
    material.set_shader_parameter("normal_texture", &normal.to_variant());
    material.set_shader_parameter("orm_texture", &orm.to_variant());
    material.set_shader_parameter(
        "ao_light_affect",
        &standard.get_ao_light_affect().to_variant(),
    );
    set_parameters(&mut material, camo);
    WORLD.with_borrow_mut(|cache| cache.insert(key, material.clone()));
    Some(material)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_camo_but_the_factory_finish_has_a_look() {
        for camo in Camo::all() {
            assert_eq!(look(camo).is_some(), camo != Camo::NONE, "{}", camo.key());
        }
    }
}
