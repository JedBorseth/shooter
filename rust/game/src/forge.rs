//! The Zombies forge (`shooter_sim::zombies::forge`) as every client (and
//! the server) shows it: a furnace-press that lights up with the power, takes
//! a weapon in, works it with a glow and sparks of light, and pushes the
//! upgraded weapon out with the forge's sheen on it.
//!
//! The dedicated server decides (`net/machines.rs`); `HordeView` passes each
//! `ZombiesEvent::Forge` on with [`Forge::show`]. The scene
//! (`scenes/zombies/forge.tscn`) has `Body`, `Lit` (the glowing parts, shown
//! while powered), `Display` (where the weapon rests in the mouth), and
//! `Light`.

use godot::classes::{IStaticBody3D, Node3D, OmniLight3D, StaticBody3D};
use godot::prelude::*;
use shooter_sim::net::protocol::ForgePhase;
use shooter_sim::weapon::WeaponId;
use shooter_sim::zombies::forge;

use crate::pickup::INTERACT_LAYER;
use crate::player::FpsPlayer;
use crate::viewmodel::instantiate_world_weapon;

/// How far the weapon slides into the mouth while worked (metres, -z).
const SLIDE: f32 = 0.55;
const LIGHT_ENERGY: f32 = 1.2;

#[derive(GodotClass)]
#[class(init, base = StaticBody3D)]
pub struct Forge {
    base: Base<StaticBody3D>,
    #[init(val = ForgePhase::Idle)]
    phase: ForgePhase,
    powered: bool,
    /// The local player is the one it works for.
    mine: bool,
    weapon: Option<WeaponId>,
    elapsed: f32,
    shown: Option<(WeaponId, Gd<Node3D>)>,
}

#[godot_api]
impl IStaticBody3D for Forge {
    fn ready(&mut self) {
        let layer = self.base().get_collision_layer();
        self.base_mut().set_collision_layer(layer | INTERACT_LAYER);
        self.apply_power();
    }

    fn process(&mut self, delta: f64) {
        self.elapsed += delta as f32;
        self.sync_model();
        let t = self.elapsed;
        let (slide, glow) = match self.phase {
            ForgePhase::Idle => (1.0, 0.0),
            // In over the first second, worked, then out.
            ForgePhase::Working => {
                let into = (t / 0.8).min(1.0);
                let out = ((t - (forge::WORK_SECONDS - 0.6)) / 0.6).clamp(0.0, 1.0);
                (into - out, 0.6 + 0.4 * (t * 9.0).sin().abs())
            }
            ForgePhase::Offering => (0.0, 0.3),
        };
        if let Some((_, view)) = &mut self.shown {
            view.set_position(Vector3::new(0.0, 0.0, -SLIDE * smooth(slide)));
        }
        if let Some(mut light) = self.base().try_get_node_as::<OmniLight3D>("Light") {
            light.set_param(
                godot::classes::light_3d::Param::ENERGY,
                LIGHT_ENERGY * (1.0 + 2.5 * glow),
            );
        }
    }
}

#[godot_api]
impl Forge {
    #[func]
    fn interact_prompt(&self) -> GString {
        if self.phase != ForgePhase::Idle {
            return GString::new();
        }
        if !self.powered {
            return GString::from("Forge: needs the power on");
        }
        let Some(player) = self
            .base()
            .get_tree()
            .get_first_node_in_group("players")
            .and_then(|p| p.try_cast::<FpsPlayer>().ok())
        else {
            return GString::new();
        };
        let points = self
            .base()
            .get_tree()
            .get_first_node_in_group("zombies_wallet")
            .map_or(0, |mut w| {
                w.call("points", &[]).try_to::<i64>().unwrap_or(0)
            });
        let player = player.bind();
        let combat = &player.sim_state().combat;
        let price = i64::from(forge::PRICE);
        match forge::can_forge(combat, u32::try_from(points).unwrap_or(0), true, true) {
            Ok(up) => GString::from(&format!(
                "[{{key}}] Forge {} into {}: {price}",
                combat.def().name,
                up.def().name
            )),
            Err(forge::Refusal::Points) => GString::from(&format!("Forge: {price} points")),
            Err(forge::Refusal::OnlyWeapon) => {
                GString::from("Forge: carry another weapon to hand this one over")
            }
            Err(_) => GString::from("Forge: hold a weapon it can upgrade"),
        }
    }

    /// The server takes the weapon; nothing happens here on a press.
    #[func]
    fn interact(&mut self, _player: Gd<Node>) {}

    /// The upgraded weapon held out to the local player ("" for none):
    /// `FpsPlayer` treats it as a free rack.
    #[func]
    fn offered_weapon(&self) -> GString {
        match (self.phase, self.weapon) {
            (ForgePhase::Offering, Some(weapon)) if self.mine => GString::from(&weapon.def().key),
            _ => GString::new(),
        }
    }

    /// The power went on (or off for a new game).
    #[func]
    pub fn set_powered(&mut self, powered: bool) {
        self.powered = powered;
        self.apply_power();
    }

    /// Shows the forge in `phase` ("idle", "working", "offering") with
    /// `weapon` (a key), `seconds` into it (captures and tests).
    #[func]
    fn debug_show(&mut self, phase: GString, weapon: GString, seconds: f32) {
        let phase = match phase.to_string().as_str() {
            "working" => ForgePhase::Working,
            "offering" => ForgePhase::Offering,
            _ => ForgePhase::Idle,
        };
        self.show(phase, true, 0, WeaponId::by_key(&weapon.to_string()), 0.0);
        self.elapsed = seconds;
    }
}

impl Forge {
    /// The forge is in `phase` for player `by` (`mine` when that is the
    /// local player), working or holding `weapon` with `seconds` left.
    pub fn show(
        &mut self,
        phase: ForgePhase,
        mine: bool,
        _by: u8,
        weapon: Option<WeaponId>,
        seconds: f32,
    ) {
        if phase != self.phase {
            let length = match phase {
                ForgePhase::Working => forge::WORK_SECONDS,
                ForgePhase::Offering => forge::TAKE_SECONDS,
                ForgePhase::Idle => 0.0,
            };
            self.elapsed = (length - seconds).clamp(0.0, length);
        }
        self.phase = phase;
        self.mine = mine;
        self.weapon = weapon;
        self.sync_model();
    }

    /// The weapon in the mouth: the plain one going in, the upgrade out.
    fn sync_model(&mut self) {
        let model = match self.phase {
            ForgePhase::Idle => None,
            ForgePhase::Working if self.elapsed < forge::WORK_SECONDS - 0.6 => {
                self.weapon.map(WeaponId::plain)
            }
            _ => self.weapon,
        };
        if self.shown.as_ref().map(|(id, _)| *id) == model {
            return;
        }
        if let Some((_, mut old)) = self.shown.take() {
            old.queue_free();
        }
        if let Some(id) = model
            && let Some(view) = instantiate_world_weapon(id)
        {
            let mut holder = Node3D::new_alloc();
            // Side on to the player, barrel to +x, on the tray.
            let mut view = view;
            view.set_rotation(Vector3::new(0.0, -std::f32::consts::FRAC_PI_2, 0.0));
            view.set_position(Vector3::new(-0.3, 0.0, 0.0));
            holder.add_child(&view);
            if let Some(mut display) = self.base().try_get_node_as::<Node3D>("Display") {
                display.add_child(&holder);
            }
            self.shown = Some((id, holder));
        }
    }

    fn apply_power(&mut self) {
        let powered = self.powered;
        if let Some(mut lit) = self.base().try_get_node_as::<Node3D>("Lit") {
            lit.set_visible(powered);
        }
        if let Some(mut light) = self.base().try_get_node_as::<Node3D>("Light") {
            light.set_visible(powered);
        }
    }
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
