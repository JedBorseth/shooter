//! A spot for the Zombies mystery crate (`shooter_sim::zombies::mystery`),
//! as every client (and the server) shows it: closed and waiting, open with
//! weapons cycling up out of it, holding the result out, the duck, or gone.
//!
//! The dedicated server decides everything (`net/machines.rs`); the map's
//! `MysteryCrates` children are these nodes, and `HordeView` passes each
//! `ZombiesEvent::Crate` on with [`MysteryCrate::show`]. The spot the crate
//! is not at shows nothing and does not collide.
//!
//! The scene (`scenes/zombies/mystery_crate.tscn`) has `Body` (the crate),
//! `Lid` (hinged at its back edge, opening about x), `Display` (where the
//! weapons rise, inside the crate), `Duck`, and `Light`.

use std::collections::HashMap;

use godot::classes::{IStaticBody3D, Node3D, OmniLight3D, StaticBody3D};
use godot::prelude::*;
use shooter_sim::net::protocol::CratePhase;
use shooter_sim::rng;
use shooter_sim::weapon::WeaponId;
use shooter_sim::zombies::mystery;

use crate::pickup::INTERACT_LAYER;
use crate::viewmodel::instantiate_world_weapon;

/// The lid's angle open (radians about x; negative lifts the front).
const LID_OPEN: f32 = -1.75;
/// Seconds the lid takes to open or close.
const LID_SECONDS: f32 = 0.35;
/// How high the weapon rises over the crate's floor while cycling.
const RISE: f32 = 0.75;
const SOLID_LAYERS: u32 = 1 | INTERACT_LAYER;

#[derive(GodotClass)]
#[class(init, base = StaticBody3D)]
pub struct MysteryCrate {
    base: Base<StaticBody3D>,
    #[init(val = CratePhase::Away)]
    phase: CratePhase,
    /// The crate is at this spot.
    present: bool,
    /// The local player bought this spin.
    mine: bool,
    weapon: Option<WeaponId>,
    /// Seconds into the current phase, and how long it lasts.
    elapsed: f32,
    length: f32,
    lid: f32,
    views: HashMap<WeaponId, Gd<Node3D>>,
    shown: Option<WeaponId>,
}

#[godot_api]
impl IStaticBody3D for MysteryCrate {
    fn ready(&mut self) {
        self.base_mut().set_collision_mask(0);
        self.apply_presence();
        if let Some(mut duck) = self.base().try_get_node_as::<Node3D>("Duck") {
            duck.hide();
        }
    }

    fn process(&mut self, delta: f64) {
        let _profile = crate::profile::scope("mystery crate");
        let dt = delta as f32;
        self.elapsed += dt;
        let open = matches!(
            self.phase,
            CratePhase::Cycling | CratePhase::Offering | CratePhase::Duck
        );
        let target = if open { 1.0 } else { 0.0 };
        let step = dt / LID_SECONDS;
        self.lid = if self.lid < target {
            (self.lid + step).min(target)
        } else {
            (self.lid - step).max(target)
        };
        if let Some(mut lid) = self.base().try_get_node_as::<Node3D>("Lid") {
            lid.set_rotation(Vector3::new(LID_OPEN * smooth(self.lid), 0.0, 0.0));
        }
        self.animate_display();
    }
}

#[godot_api]
impl MysteryCrate {
    /// The use prompt: open it when closed and here; nothing otherwise (the
    /// weapon it holds out is taken like a rack's, see `offered_weapon`).
    #[func]
    fn interact_prompt(&self) -> GString {
        if !self.present || self.phase != CratePhase::Idle {
            return GString::new();
        }
        let price = i64::from(mystery::PRICE);
        let points = self
            .base()
            .get_tree()
            .get_first_node_in_group("zombies_wallet")
            .map_or(0, |mut w| {
                w.call("points", &[]).try_to::<i64>().unwrap_or(0)
            });
        GString::from(&if points >= price {
            format!("[{{key}}] Open the mystery crate: {price}")
        } else {
            format!("Mystery crate: {price} points")
        })
    }

    /// The server opens the crate; nothing happens here on a press.
    #[func]
    fn interact(&mut self, _player: Gd<Node>) {}

    /// The weapon the crate holds out to the local player ("" for none):
    /// `FpsPlayer` treats it as a free rack.
    #[func]
    fn offered_weapon(&self) -> GString {
        match (self.phase, self.weapon) {
            (CratePhase::Offering, Some(weapon)) if self.mine && self.present => {
                GString::from(&weapon.def().key)
            }
            _ => GString::new(),
        }
    }

    /// Shows the crate here in `phase` ("idle", "cycling", "offering",
    /// "duck", "away") with `weapon` (a key, or "" for none), `seconds`
    /// into it, as the server would (captures and tests).
    #[func]
    fn debug_show(&mut self, phase: GString, weapon: GString, seconds: f32) {
        let phase = match phase.to_string().as_str() {
            "cycling" => CratePhase::Cycling,
            "offering" => CratePhase::Offering,
            "duck" => CratePhase::Duck,
            "away" => CratePhase::Away,
            _ => CratePhase::Idle,
        };
        let weapon = WeaponId::by_key(&weapon.to_string());
        self.show(true, phase, true, weapon, 0.0);
        self.elapsed = seconds;
        self.lid = if phase == CratePhase::Idle { 0.0 } else { 1.0 };
    }

    /// Whether the crate stands at this spot now.
    #[func]
    fn is_present(&self) -> bool {
        self.present
    }

    /// The crate's phase here ("idle", "cycling", "offering", "duck",
    /// "away", or "" when it is elsewhere).
    #[func]
    fn phase_name(&self) -> GString {
        if !self.present {
            return GString::new();
        }
        GString::from(match self.phase {
            CratePhase::Idle => "idle",
            CratePhase::Cycling => "cycling",
            CratePhase::Offering => "offering",
            CratePhase::Duck => "duck",
            CratePhase::Away => "away",
        })
    }
}

impl MysteryCrate {
    /// The crate is at this spot in `phase` (for the local player when
    /// `mine`), landing on or holding `weapon`, for `seconds` more; or (with
    /// `present` false) it is somewhere else.
    pub fn show(
        &mut self,
        present: bool,
        phase: CratePhase,
        mine: bool,
        weapon: Option<WeaponId>,
        seconds: f32,
    ) {
        let changed = phase != self.phase || present != self.present;
        self.present = present;
        self.mine = mine;
        self.weapon = weapon;
        if changed {
            self.phase = phase;
            self.elapsed = 0.0;
            self.length = match phase {
                CratePhase::Cycling => mystery::CYCLE_SECONDS,
                CratePhase::Offering => mystery::TAKE_SECONDS,
                CratePhase::Duck => mystery::DUCK_SECONDS,
                CratePhase::Away => mystery::AWAY_SECONDS,
                CratePhase::Idle => 0.0,
            };
            // A late joiner sees the phase partway through.
            self.elapsed = (self.length - seconds).clamp(0.0, self.length);
            if phase == CratePhase::Idle && present {
                self.lid = 0.0;
            }
        }
        self.apply_presence();
    }

    fn apply_presence(&mut self) {
        let here = self.present && self.phase != CratePhase::Away;
        let layers = if here { SOLID_LAYERS } else { 0 };
        self.base_mut().set_collision_layer(layers);
        for name in ["Body", "Lid", "Light"] {
            if let Some(mut node) = self.base().try_get_node_as::<Node3D>(name) {
                node.set_visible(self.present);
            }
        }
        if let Some(mut light) = self.base().try_get_node_as::<OmniLight3D>("Light") {
            light.set_visible(here);
        }
    }

    /// Cycles weapons up out of the crate, slowing down as Black Ops does,
    /// then holds the result (or the duck), sinking it back while the buyer
    /// has to take it; and lifts the crate away when it leaves.
    fn animate_display(&mut self) {
        let t = self.elapsed;
        let (show, rise, duck) = match self.phase {
            CratePhase::Cycling => {
                let steps: Vec<f32> = mystery::cycle_steps().collect();
                let step = steps.iter().rposition(|s| *s <= t).unwrap_or(0);
                let landed = t >= mystery::CYCLE_SECONDS - 0.1;
                let shown = if landed {
                    self.weapon
                } else {
                    let pool = &mystery::POOL;
                    let (key, _) = pool[rng::hash(step as u32, 0x5759) as usize % pool.len()];
                    WeaponId::by_key(key)
                };
                // Up out of the crate in the first second, then a slow climb.
                let rise = 0.7 * (t / 1.0).min(1.0) + 0.3 * (t / mystery::CYCLE_SECONDS).min(1.0);
                (shown, rise, landed && self.weapon.is_none())
            }
            CratePhase::Offering => {
                // The weapon sinks back into the crate as time runs out.
                let left = 1.0 - (t / self.length.max(0.01)).min(1.0);
                (self.weapon, 0.35 + 0.65 * left, false)
            }
            CratePhase::Duck => (None, 1.0, true),
            _ => (None, 0.0, false),
        };
        let shown = show.filter(|_| self.present);
        if shown != self.shown {
            if let Some(old) = self.shown.and_then(|id| self.views.get(&id)) {
                let mut old = old.clone();
                old.hide();
            }
            if let Some(id) = shown {
                self.view_for(id).show();
            }
            self.shown = shown;
        }
        if let Some(mut display) = self.base().try_get_node_as::<Node3D>("Display") {
            let mut position = display.get_position();
            position.y = 0.45 + RISE * smooth(rise);
            display.set_position(position);
            display.set_rotation(Vector3::new(0.0, (t * 0.6).sin() * 0.15, 0.0));
        }
        if let Some(mut node) = self.base().try_get_node_as::<Node3D>("Duck") {
            node.set_visible(duck && self.present);
            let bob = (t * 3.0).sin() * 0.04;
            node.set_position(Vector3::new(
                0.0,
                0.9 + 0.5 * smooth((t / 1.5).min(1.0)) + bob,
                0.0,
            ));
            node.set_rotation(Vector3::new(0.0, t * 1.5, 0.0));
        }
        // Leaving: the crate lifts away, then is gone until it shows up at
        // its next spot.
        if let Some(mut body) = self.base().try_get_node_as::<Node3D>("Body") {
            let lift = if self.phase == CratePhase::Away {
                (t / 1.5).min(1.0)
            } else {
                0.0
            };
            body.set_position(Vector3::new(0.0, 6.0 * lift * lift, 0.0));
            body.set_visible(self.present && lift < 1.0);
            if let Some(mut lid) = self.base().try_get_node_as::<Node3D>("Lid") {
                let mut p = lid.get_position();
                p.y = crate_lid_height() + 6.0 * lift * lift;
                lid.set_position(p);
                lid.set_visible(self.present && lift < 1.0);
            }
        }
    }

    /// The display model for `id`, made on first use.
    fn view_for(&mut self, id: WeaponId) -> Gd<Node3D> {
        if let Some(view) = self.views.get(&id) {
            return view.clone();
        }
        let mut holder = Node3D::new_alloc();
        // Barrel along the crate (+x), lying level.
        holder.set_rotation(Vector3::new(0.0, -std::f32::consts::FRAC_PI_2, 0.0));
        if let Some(view) = instantiate_world_weapon(id) {
            holder.add_child(&view);
        }
        holder.hide();
        if let Some(mut display) = self.base().try_get_node_as::<Node3D>("Display") {
            display.add_child(&holder);
        }
        self.views.insert(id, holder.clone());
        holder
    }
}

/// Height of the lid's hinge over the crate's base (`CRATE_H` in
/// tools/gear_models.py).
fn crate_lid_height() -> f32 {
    0.74
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Shows a `ZombiesEvent::Crate` on the map's spots: the crate at `spot`
/// in `phase` (for the local player `me` when they bought it), every other
/// spot empty. `seconds` is how long the phase has left.
pub fn show_on_spots(
    spots: &[Gd<Node3D>],
    spot: u8,
    phase: CratePhase,
    by: u8,
    weapon: Option<WeaponId>,
    seconds: f32,
    me: Option<u8>,
) {
    for (index, node) in spots.iter().enumerate() {
        let Ok(mut crate_) = node.clone().try_cast::<MysteryCrate>() else {
            continue;
        };
        let here = index == usize::from(spot);
        crate_
            .bind_mut()
            .show(here, phase, here && me == Some(by), weapon, seconds);
    }
}
