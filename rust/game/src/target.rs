//! Shooting-range target: an armed soldier that takes damage per hit zone,
//! flinches away from hits, falls away from the killing shot, and respawns.

use godot::classes::{IStaticBody3D, Label3D, StaticBody3D};
use godot::prelude::*;
use shooter_sim::hitbox::HitboxPose;
use shooter_sim::movement::Stance;
use shooter_sim::weapon::WeaponId;

use crate::aim_assist::AimAssistTarget;
use crate::character::rig::{SoldierFrame, SoldierRig};
use crate::convert::to_sim;
use crate::hitbox_rig::HitboxRig;

pub use crate::hitbox_rig::HITBOX_LAYER;
/// Collision layer 1 ("world"), as a bit mask.
pub const WORLD_LAYER: u32 = 1;
/// Collision layer 5 ("bodies"): blocks movement but not bullets.
pub const BODY_LAYER: u32 = 1 << 4;

#[derive(GodotClass)]
#[class(init, base = StaticBody3D)]
pub struct TargetDummy {
    base: Base<StaticBody3D>,

    #[export]
    #[init(val = 100.0)]
    max_health: f32,
    #[export]
    #[init(val = 3.0)]
    respawn_delay: f32,
    /// Weapon file key the soldier holds.
    #[export]
    #[init(val = GString::from("m4"))]
    weapon: GString,
    #[export]
    crouching: bool,
    /// Aim pitch in degrees (up positive).
    #[export]
    aim_pitch: f32,

    #[init(node = "HealthLabel")]
    label: OnReady<Gd<Label3D>>,
    #[init(node = "Soldier")]
    rig: OnReady<Gd<SoldierRig>>,
    #[init(node = "Hitboxes")]
    hitboxes: OnReady<Gd<HitboxRig>>,

    health: f32,
    respawn_in: f32,
    /// False on a dedicated server, which never draws the soldier.
    visuals: bool,
}

#[godot_api]
impl IStaticBody3D for TargetDummy {
    fn ready(&mut self) {
        self.visuals = crate::character::rig::visuals_enabled();
        self.health = self.max_health;
        self.update_label();
        self.base_mut().set_collision_layer(BODY_LAYER);
        self.base_mut().set_collision_mask(0);
    }

    fn process(&mut self, delta: f64) {
        let dt = delta as f32;
        let alive = self.health > 0.0;
        let frame = self.frame(alive);
        if self.visuals {
            self.rig.bind_mut().update(&frame, dt);
        }
        let pose = alive.then(|| HitboxPose {
            activity: frame.activity(self.rig.bind().progress()),
            ..HitboxPose::new(
                to_sim(self.base().get_global_position()),
                frame.yaw,
                frame.pitch,
                frame.stance,
            )
        });
        self.hitboxes.bind_mut().set_pose(pose);

        if !alive {
            self.respawn_in -= dt;
            if self.respawn_in <= 0.0 {
                self.respawn();
            }
        }
    }
}

#[godot_api]
impl TargetDummy {
    /// Applies damage and returns true if this hit killed the dummy.
    #[func]
    pub fn apply_damage(&mut self, amount: f32, _headshot: bool) -> bool {
        if self.health <= 0.0 {
            return false;
        }
        self.health -= amount;
        let killed = self.health <= 0.0;
        if killed {
            self.health = 0.0;
            self.respawn_in = self.respawn_delay;
            self.set_hittable(false);
        }
        self.update_label();
        killed
    }

    /// Visual reaction to a hit from a shot travelling along `direction`
    /// (world space) that landed at `point` on hit shape `part` (-1 unknown)
    /// with force level `force`: a flinch, or on a kill, a fall with the
    /// shot's push.
    #[func]
    pub fn react_to_hit(
        &mut self,
        direction: Vector3,
        killed: bool,
        point: Vector3,
        part: i64,
        force: f32,
    ) {
        let push = crate::character::rig::push_of(direction, point, part, force);
        self.rig.bind_mut().hit_at(push, killed);
    }

    #[func]
    pub fn health(&self) -> f32 {
        self.health
    }

    /// Horizontal direction the body falls toward, or zero while standing.
    #[func]
    pub fn fall_direction(&self) -> Vector3 {
        self.rig.bind().fall_direction()
    }
}

impl TargetDummy {
    fn frame(&self, alive: bool) -> SoldierFrame {
        SoldierFrame {
            stance: if self.crouching {
                Stance::Crouching
            } else {
                Stance::Standing
            },
            yaw: self.base().get_global_rotation().y,
            pitch: self.aim_pitch.to_radians(),
            weapon: WeaponId::by_key(&self.weapon.to_string()),
            alive,
            ..SoldierFrame::default()
        }
    }

    fn respawn(&mut self) {
        self.health = self.max_health;
        self.set_hittable(true);
        self.update_label();
    }

    fn set_hittable(&mut self, hittable: bool) {
        self.label.set_visible(hittable);
        if let Some(mut assist) = self
            .base()
            .get_node_or_null("AimAssist")
            .and_then(|n| n.try_cast::<AimAssistTarget>().ok())
        {
            assist.bind_mut().set_active(hittable);
        }
    }

    fn update_label(&mut self) {
        let text = format!("{:.0}", self.health.max(0.0));
        self.label.set_text(&text);
    }
}
