//! Weapon pickups: a rack or dropped gun the player takes with the interact key.
//!
//! Racks never run out: taking a weapon you already carry refills its ammo.
//! In Zombies they are wall weapons with a price: the weapon costs `price`
//! points and its ammo half that, paid from the `zombies_wallet` group's
//! node (`ZombieDirector::spend`).

use godot::classes::{IStaticBody3D, Label3D, Node3D, PackedScene, StaticBody3D};
use godot::prelude::*;
use shooter_sim::weapon::WeaponId;

/// Collision layer 4 ("interactables"), as a bit mask.
pub const INTERACT_LAYER: u32 = 1 << 3;

#[derive(GodotClass)]
#[class(init, base = StaticBody3D)]
pub struct WeaponPickup {
    base: Base<StaticBody3D>,

    /// Weapon file key, such as `m870` for `data/weapons/m870.ron`.
    #[export]
    weapon: GString,
    /// Model shown lying on the rack (optional).
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
        let model = self
            .display_model
            .as_ref()
            .and_then(|scene| scene.instantiate())
            .and_then(|node| node.try_cast::<Node3D>().ok());
        if let (Some(mut model), Some(mut display)) =
            (model, self.base().try_get_node_as::<Node3D>("Display"))
        {
            // Lay the gun on its side, barrel to the right, roughly centred.
            let scale = self.display_scale;
            model.set_scale(Vector3::splat(scale));
            model.set_rotation(Vector3::new(std::f32::consts::FRAC_PI_2, 0.0, 0.0));
            model.set_position(Vector3::new(-scale, 0.02, 0.0));
            display.add_child(&model);
        }
    }
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
