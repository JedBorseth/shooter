//! Weapon pickups: a rack or dropped gun the player takes with the interact key.
//!
//! Racks never run out: taking a weapon you already carry refills its ammo.
//! In Zombies they are wall weapons with a price: the weapon costs `price`
//! points and its ammo half that, paid from the `zombies_wallet` group's
//! node (`ZombieDirector::spend`).

use godot::classes::{IStaticBody3D, Label3D, MeshInstance3D, Node3D, PackedScene, StaticBody3D};
use godot::prelude::*;
use shooter_sim::weapon::WeaponId;

use crate::viewmodel::instantiate_world_weapon;

/// Collision layer 4 ("interactables"), as a bit mask.
pub const INTERACT_LAYER: u32 = 1 << 3;

#[derive(GodotClass)]
#[class(init, base = StaticBody3D)]
pub struct WeaponPickup {
    base: Base<StaticBody3D>,

    /// Weapon file key, such as `m870` for `data/weapons/m870.ron`.
    #[export]
    weapon: GString,
    /// Model shown lying on the rack, for a weapon without a view scene.
    /// Weapons with one show their own third-person model (the same one
    /// other players carry), so these are only a fallback.
    #[export]
    display_model: Option<Gd<PackedScene>>,
    #[export]
    #[init(val = 0.16)]
    display_scale: f32,
    /// Points the weapon costs (Zombies); 0 is free. Ammo costs half.
    #[export]
    price: i64,
}

#[godot_api]
impl IStaticBody3D for WeaponPickup {
    fn ready(&mut self) {
        let layer = self.base().get_collision_layer();
        self.base_mut().set_collision_layer(layer | INTERACT_LAYER);
        let Some(id) = self.weapon_id() else {
            godot_error!("WeaponPickup: unknown weapon `{}`", self.weapon);
            return;
        };
        if let Some(mut label) = self.base().try_get_node_as::<Label3D>("Label") {
            let name = &id.def().name;
            if self.price > 0 {
                label.set_text(&format!("{name}\n{}", self.price));
            } else {
                label.set_text(name);
            }
        }
        let Some(mut display) = self.base().try_get_node_as::<Node3D>("Display") else {
            return;
        };
        if let Some(view) = instantiate_world_weapon(id) {
            display.add_child(&view);
            lay_on_side(&view.upcast());
            return;
        }
        let model = self
            .display_model
            .as_ref()
            .and_then(|scene| scene.instantiate())
            .and_then(|node| node.try_cast::<Node3D>().ok());
        if let Some(mut model) = model {
            // Lay the gun on its side, barrel to the right, roughly centred.
            let scale = self.display_scale;
            model.set_scale(Vector3::splat(scale));
            model.set_rotation(Vector3::new(std::f32::consts::FRAC_PI_2, 0.0, 0.0));
            model.set_position(Vector3::new(-scale, 0.02, 0.0));
            display.add_child(&model);
        }
    }
}

/// Lays a weapon view (barrel along -Z, +Y up) on its left side, right side
/// up, barrel to +X, resting on its parent's origin and centred on it.
fn lay_on_side(view: &Gd<Node3D>) {
    let mut view = view.clone();
    // Local +X (the right side) up, local +Y (the top) away from the front,
    // the barrel (-Z) along +X.
    let basis = Basis::from_cols(Vector3::UP, Vector3::FORWARD, Vector3::LEFT);
    view.set_transform(Transform3D::new(basis, Vector3::ZERO));
    let Some((min, max)) = bounds(&view, Transform3D::IDENTITY) else {
        return;
    };
    let centre = (min + max) * 0.5;
    // In the parent: x = -local z, y = local x, z = -local y.
    view.set_position(Vector3::new(centre.z, -min.x + 0.003, centre.y));
}

/// The bounds of every mesh under `node`, in the space `to_space` maps `node` into.
fn bounds(node: &Gd<Node3D>, to_space: Transform3D) -> Option<(Vector3, Vector3)> {
    let mut result: Option<(Vector3, Vector3)> = None;
    for child in node.get_children().iter_shared() {
        let Ok(child) = child.try_cast::<Node3D>() else {
            continue;
        };
        if !child.is_visible() {
            continue;
        }
        let xf = to_space * child.get_transform();
        if let Ok(visual) = child.clone().try_cast::<MeshInstance3D>() {
            let aabb = visual.get_aabb();
            for i in 0..8 {
                let p = xf * aabb.get_corner(i);
                result = Some(match result {
                    Some((lo, hi)) => (lo.coord_min(p), hi.coord_max(p)),
                    None => (p, p),
                });
            }
        }
        if let Some((lo, hi)) = bounds(&child, xf) {
            result = Some(match result {
                Some((a, b)) => (a.coord_min(lo), b.coord_max(hi)),
                None => (lo, hi),
            });
        }
    }
    result
}

#[godot_api]
impl WeaponPickup {
    #[func]
    fn weapon_name(&self) -> GString {
        self.weapon_id()
            .map(|id| id.def().name.as_str().into())
            .unwrap_or_default()
    }
}

impl WeaponPickup {
    pub fn weapon_id(&self) -> Option<WeaponId> {
        WeaponId::by_key(&self.weapon.to_string())
    }

    pub fn price(&self) -> i64 {
        self.price.max(0)
    }
}
