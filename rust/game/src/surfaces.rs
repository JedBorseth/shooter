//! What the world is made of where a ray hits it, for footsteps and impacts.
//!
//! A collider (or any ancestor) tagged with `surface` metadata says so
//! directly. Untagged geometry is judged from the material drawn on the hit
//! triangle: imported maps such as Nuketown carry descriptive material names
//! (`nt_2020_astroturf_01`, `nt_2020_carpet_yellow`, `plastic_floor_tiles_*`),
//! so their grass, carpet, and tile sound right without hand tagging. The hit
//! triangle comes from the ray's `face_index` (enabled in the project's Jolt
//! settings) and is mapped back to the mesh surface it was built from.
//!
//! Penetration still reads only the tags (`player::surface_of`), so this never
//! changes what a bullet goes through.

use std::cell::RefCell;
use std::collections::HashMap;

use godot::classes::{ArrayMesh, Material, Mesh, MeshInstance3D, Node, mesh::PrimitiveType};
use godot::prelude::*;

/// A kind of surface, as footsteps and impacts tell them apart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Surface {
    #[default]
    Concrete,
    Wood,
    Metal,
    Dirt,
    Grass,
    Gravel,
    Tile,
    Carpet,
    Glass,
    Flesh,
}

/// Material-name keywords for each surface, in priority order: earlier rules
/// win, so `concrete_sidewalk_tile` is concrete.
const RULES: &[(Surface, &[&str])] = &[
    (Surface::Glass, &["glass", "window"]),
    (
        Surface::Grass,
        &["grass", "turf", "lawn", "hedge", "foliage", "leaves"],
    ),
    (Surface::Gravel, &["gravel", "pebble", "rubble"]),
    (
        Surface::Dirt,
        &["dirt", "mud", "sand", "soil", "_ter_", "ground"],
    ),
    (
        Surface::Carpet,
        &[
            "carpet", "rug", "fabric", "cloth", "mattress", "cushion", "canvas", "tarp",
        ],
    ),
    (
        Surface::Concrete,
        &[
            "concrete", "asphalt", "sidewalk", "curb", "plaster", "brick", "cement", "road",
        ],
    ),
    (
        Surface::Metal,
        &[
            "metal",
            "steel",
            "iron",
            "alumin",
            "chrome",
            "tread",
            "grate",
            "corrugated",
            "tin_",
            "copper",
            "vehicle",
            "truck",
            "car_",
            "bus_",
            "dumpster",
            "container",
            "barrel",
        ],
    ),
    (
        Surface::Wood,
        &[
            "wood", "plank", "parquet", "deck", "laminate", "crate", "bench", "door",
        ],
    ),
    (
        Surface::Tile,
        &[
            "tile",
            "marble",
            "ceramic",
            "stone",
            "granite",
            "floor_kitchen",
            "porcelain",
        ],
    ),
];

impl Surface {
    /// The surface for a `surface` tag, or None for an unknown tag.
    pub fn from_tag(tag: &str) -> Option<Self> {
        Some(match tag {
            "concrete" => Self::Concrete,
            "wood" => Self::Wood,
            "metal" => Self::Metal,
            "dirt" | "mud" | "sand" => Self::Dirt,
            "grass" => Self::Grass,
            "gravel" => Self::Gravel,
            "tile" | "stone" | "marble" => Self::Tile,
            "carpet" | "fabric" => Self::Carpet,
            "glass" => Self::Glass,
            "flesh" => Self::Flesh,
            _ => return None,
        })
    }

    /// The surface a material name describes, or None when the name says
    /// nothing useful. Layered names (`base__decal`) are judged by their base
    /// layer.
    pub fn from_material_name(name: &str) -> Option<Self> {
        let name = name.to_ascii_lowercase();
        let base = name.split("__").next().unwrap_or(&name);
        // Invisible helper geometry: no opinion.
        if ["caulk", "shadowcaster", "clip", "trigger"]
            .iter()
            .any(|k| base.contains(k))
        {
            return None;
        }
        RULES
            .iter()
            .find(|(_, keys)| keys.iter().any(|k| base.contains(k)))
            .map(|(surface, _)| *surface)
    }

    /// The tag for this surface.
    pub fn key(self) -> &'static str {
        match self {
            Self::Concrete => "concrete",
            Self::Wood => "wood",
            Self::Metal => "metal",
            Self::Dirt => "dirt",
            Self::Grass => "grass",
            Self::Gravel => "gravel",
            Self::Tile => "tile",
            Self::Carpet => "carpet",
            Self::Glass => "glass",
            Self::Flesh => "flesh",
        }
    }

    /// The footstep set to play (`assets/audio/footsteps/<key>_N`). Nobody
    /// walks on flesh or glass for long; they borrow the nearest set.
    pub fn footstep_key(self) -> &'static str {
        match self {
            Self::Glass => "tile",
            Self::Flesh => "dirt",
            other => other.key(),
        }
    }

    /// The impact sound set (`assets/audio/impacts/<key>_N`): every
    /// surface has its own.
    pub fn impact_sound_key(self) -> &'static str {
        self.key()
    }

    /// The impact look (`scripts/impact.gd`).
    pub fn impact_key(self) -> &'static str {
        match self {
            Self::Grass | Self::Gravel | Self::Carpet => "dirt",
            Self::Tile => "concrete",
            other => other.key(),
        }
    }
}

/// A mesh's triangle surfaces: each one's first face index and what its
/// material says it is.
type MeshSurfaces = Vec<(i64, Option<Surface>)>;

thread_local! {
    static MESH_SURFACES: RefCell<HashMap<InstanceId, MeshSurfaces>> =
        RefCell::new(HashMap::new());
}

/// What a ray hit is made of: the `surface` tag on the collider or its
/// ancestors, else the hit triangle's material, else concrete.
pub fn surface_at(collider: Option<&Gd<Node>>, face_index: i32) -> Surface {
    let Some(collider) = collider else {
        return Surface::Concrete;
    };
    if let Some(tag) = tag_of(collider) {
        return Surface::from_tag(&tag).unwrap_or(Surface::Concrete);
    }
    material_surface(collider, face_index).unwrap_or(Surface::Concrete)
}

fn tag_of(collider: &Gd<Node>) -> Option<String> {
    let mut node = Some(collider.clone());
    while let Some(current) = node {
        if current.has_meta("surface") {
            return Some(current.get_meta("surface").to_string());
        }
        node = current.get_parent();
    }
    None
}

/// The material surface of the mesh a collider was built from: the mesh it is
/// a child of, as imported and baked maps (and Godot's "create trimesh static
/// body") lay them out. Any other collider has no material to go by.
fn material_surface(collider: &Gd<Node>, face_index: i32) -> Option<Surface> {
    let mesh_node = collider.get_parent()?.try_cast::<MeshInstance3D>().ok()?;
    let mesh = mesh_node.get_mesh()?;
    let id = mesh.instance_id();
    let cached = MESH_SURFACES.with(|cache| cache.borrow().get(&id).cloned());
    let surfaces = match cached {
        Some(s) => s,
        None => {
            let s = classify_mesh(&mesh_node, &mesh);
            MESH_SURFACES.with(|cache| cache.borrow_mut().insert(id, s.clone()));
            s
        }
    };
    let face = i64::from(face_index.max(0));
    // The last surface starting at or before the face.
    surfaces
        .iter()
        .rev()
        .find(|(first, _)| *first <= face)
        .or(surfaces.first())
        .and_then(|(_, surface)| *surface)
}

/// Each triangle surface's first face index (in the order `create_trimesh_shape`
/// lays faces out) and the surface its material describes.
fn classify_mesh(node: &Gd<MeshInstance3D>, mesh: &Gd<Mesh>) -> MeshSurfaces {
    let array_mesh = mesh.clone().try_cast::<ArrayMesh>().ok();
    let mut first = 0i64;
    let mut out = Vec::new();
    for i in 0..mesh.get_surface_count() {
        let faces = match &array_mesh {
            Some(m) => {
                if m.surface_get_primitive_type(i) != PrimitiveType::TRIANGLES {
                    continue;
                }
                let indices = m.surface_get_array_index_len(i);
                let count = if indices > 0 {
                    indices
                } else {
                    m.surface_get_array_len(i)
                };
                i64::from(count / 3)
            }
            // Primitive meshes have one surface.
            None => i64::MAX / 2,
        };
        let material = node.get_active_material(i);
        out.push((first, material.as_ref().and_then(material_name_surface)));
        first += faces;
    }
    out
}

fn material_name_surface(material: &Gd<Material>) -> Option<Surface> {
    let name = material.get_name().to_string();
    if let Some(surface) = Surface::from_material_name(&name) {
        return Some(surface);
    }
    // Saved materials are often unnamed; their file name says it instead.
    let path = material.get_path().to_string();
    let file = path.rsplit('/').next().unwrap_or("");
    let stem = file.split('.').next().unwrap_or("");
    Surface::from_material_name(stem)
}

#[cfg(test)]
mod tests {
    use super::Surface::{self, *};

    fn named(name: &str) -> Option<Surface> {
        Surface::from_material_name(name)
    }

    #[test]
    fn imported_map_materials_are_recognised() {
        assert_eq!(named("nt_2020_astroturf_01"), Some(Grass));
        assert_eq!(
            named("nt_2020_astroturf_01__nt_2020_astroturf_01_worn"),
            Some(Grass)
        );
        assert_eq!(named("us_ter_grass_fairway01"), Some(Grass));
        assert_eq!(named("dub_dirt_sand_01"), Some(Dirt));
        assert_eq!(named("dub_gravel_pebbles"), Some(Gravel));
        assert_eq!(named("nt_2020_carpet_yellow_plain"), Some(Carpet));
        assert_eq!(named("nt_2020_modern_rug01_dec"), Some(Carpet));
        assert_eq!(named("plastic_floor_tiles_white_bright"), Some(Tile));
        assert_eq!(named("nt_2020_stone_cut"), Some(Tile));
        assert_eq!(named("nt_2020_floor_kitchen_02"), Some(Tile));
        assert_eq!(named("wood_panel_off_white"), Some(Wood));
        assert_eq!(named("metal_floor_treadplate_used"), Some(Metal));
        assert_eq!(named("glass_clear_mp"), Some(Glass));
    }

    #[test]
    fn decal_layers_do_not_change_the_base() {
        assert_eq!(
            named("asphalt_smooth01_dark__asphalt_road_dark_dec"),
            Some(Concrete)
        );
        assert_eq!(
            named("concrete_plates__decal_grunge_lightstain_04"),
            Some(Concrete)
        );
    }

    #[test]
    fn earlier_rules_win() {
        // A concrete sidewalk laid in tiles is still concrete underfoot.
        assert_eq!(named("concrete_sidewalk_tile"), Some(Concrete));
        assert_eq!(named("yard_dirt"), Some(Dirt));
        assert_eq!(named("steel_painted"), Some(Metal));
        assert_eq!(named("barrel_rust"), Some(Metal));
    }

    #[test]
    fn helper_and_unknown_materials_have_no_opinion() {
        assert_eq!(named("caulk_shadow_primary"), None);
        assert_eq!(named("shadowcaster"), None);
        assert_eq!(named("lambert1"), None);
        assert_eq!(named(""), None);
    }

    #[test]
    fn every_surface_has_footsteps_and_impacts() {
        for s in [
            Concrete, Wood, Metal, Dirt, Grass, Gravel, Tile, Carpet, Glass, Flesh,
        ] {
            assert_eq!(Surface::from_tag(s.key()), Some(s));
            let step = Surface::from_tag(s.footstep_key()).unwrap();
            assert_eq!(step.footstep_key(), s.footstep_key());
            let impact = Surface::from_tag(s.impact_key()).unwrap();
            assert!(matches!(
                impact,
                Concrete | Wood | Metal | Dirt | Glass | Flesh
            ));
        }
    }
}
