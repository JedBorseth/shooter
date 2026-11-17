//! Another player as this client sees them: interpolated from snapshots and
//! drawn as the third-person soldier (`SoldierRig`), with their gunfire at the
//! muzzle (`gunfire`), their footsteps, landings, slides, and reloads at their
//! feet (`footsteps`), and hit zones posed from the same snapshot (drawn with
//! F4).
//!
//! The local player's shots hit these hit zones for instant feedback (hit
//! markers, blood, a flinch), but deal no damage here: the server rewinds the
//! same poses (`shooter_sim::net::lag`) and decides every hit and kill.

use godot::classes::{INode3D, Label3D};
use godot::prelude::*;
use shooter_sim::look;
use shooter_sim::movement::{MoveParams, Stance};
pub use shooter_sim::net::lag::RemoteSample;
use shooter_sim::net::protocol;
use shooter_sim::weapon::{Reload, WeaponId};

use crate::acoustics::zone_at;
use crate::bullets::ShotTracer;
use crate::character::rig::{SoldierFrame, SoldierRig};
use crate::convert::{to_godot, to_sim};
use crate::footsteps::{Foley, Motion, RemoteReload};
use crate::gunfire::Gunfire;
use crate::hitbox_rig::HitboxRig;

#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct RemotePlayer {
    base: Base<Node3D>,

    #[init(node = "Soldier")]
    rig: OnReady<Gd<SoldierRig>>,
    #[init(node = "Hitboxes")]
    hitboxes: OnReady<Gd<HitboxRig>>,
    #[init(node = "Name")]
    name_label: OnReady<Gd<Label3D>>,

    shots_fired: Option<u32>,
    hits: Option<u8>,
    /// Flinches played from snapshots (tests).
    hits_seen: u32,
    /// Last drawn feet position and the velocity estimated from it.
    last_position: Option<Vector3>,
    velocity: Vector3,
    /// Seconds since the current reload started.
    reload_time: Option<f32>,
    gunfire: Option<Gunfire>,
    foley: Option<Foley>,
    friendly: bool,
    /// View angles and life from the last drawn sample, for spectating.
    yaw: f32,
    pitch: f32,
    #[init(val = true)]
    alive: bool,
    stance_eye: f32,
}

#[godot_api]
impl INode3D for RemotePlayer {
    fn ready(&mut self) {
        let mut owner = self.to_gd().upcast::<Node3D>();
        self.gunfire = Some(Gunfire::new(&mut owner));
        self.foley = Some(Foley::new(&mut owner));
    }
}

#[godot_api]
impl RemotePlayer {
    /// Last rendered feet position, for tests and debugging.
    #[func]
    pub fn feet(&self) -> Vector3 {
        self.base().get_global_position()
    }

    /// The character, for tests.
    /// The view yaw and pitch last drawn, in radians (spectator cameras).
    #[func]
    fn view_angles(&self) -> Vector2 {
        Vector2::new(self.yaw, self.pitch)
    }

    /// Eye height above the feet for the stance last drawn.
    #[func]
    fn eye_height(&self) -> f32 {
        self.stance_eye
    }

    #[func]
    fn is_alive(&self) -> bool {
        self.alive
    }

    #[func]
    fn soldier(&self) -> Gd<SoldierRig> {
        self.rig.clone()
    }

    /// The hit zones, for tests.
    #[func]
    fn hitboxes(&self) -> Gd<HitboxRig> {
        self.hitboxes.clone()
    }

    /// Called by the local player's hitscan. The server owns health, so this
    /// never kills here; the kill arrives from the server.
    #[func]
    fn apply_damage(&mut self, _amount: f32, _headshot: bool) -> bool {
        false
    }

    /// A small predicted flinch away from the local player's hit.
    /// Hits this player was seen to take, from the snapshots (tests).
    #[func]
    fn hits_seen(&self) -> u32 {
        self.hits_seen
    }

    #[func]
    pub fn react_to_hit(&mut self, direction: Vector3, killed: bool) {
        self.rig.bind_mut().hit(direction, killed);
    }

    /// Footsteps this player has made audible to us (tests).
    #[func]
    fn steps_heard(&self) -> u32 {
        self.foley.as_ref().map_or(0, |f| f.steps_played)
    }

    /// Gunshots played from this player (tests).
    #[func]
    fn shots_heard(&self) -> u32 {
        self.gunfire.as_ref().map_or(0, |g| g.shots_played)
    }

    /// Applies a made-up snapshot, as if the server sent it (tests): standing
    /// at `feet` with an M4, `stance` 0 standing, 1 crouching, 2 sliding.
    #[func]
    #[allow(clippy::too_many_arguments)]
    fn apply_test_sample(
        &mut self,
        feet: Vector3,
        stance: i64,
        sprinting: bool,
        on_floor: bool,
        shots_fired: i64,
        reloading: bool,
        delta: f32,
    ) {
        let Some(weapon) = WeaponId::by_key("m4") else {
            return;
        };
        let sample = RemoteSample {
            position: to_sim(feet),
            yaw: 0.0,
            pitch: 0.0,
            stance: match stance {
                1 => Stance::Crouching,
                2 => Stance::Sliding,
                _ => Stance::Standing,
            },
            sprinting,
            on_floor,
            ads: 0.0,
            weapon,
            reloading,
            alive: true,
            shots_fired: u32::try_from(shots_fired).unwrap_or(0),
            mantling: false,
            hits: 0,
            hit_heading: 0,
        };
        self.apply(&sample, delta);
    }
}

impl RemotePlayer {
    pub fn set_display_name(&mut self, name: &str) {
        self.name_label.set_text(name);
    }

    /// Teammates wear the allies' colours, show their name, and take no hits
    /// from us; everyone else wears the enemies' colours.
    pub fn set_friendly(&mut self, friendly: bool) {
        self.rig
            .bind_mut()
            .set_team_index(if friendly { 0 } else { 1 });
        self.base_mut().set_meta("friendly", &friendly.to_variant());
        self.friendly = friendly;
        // Names show through walls, so only teammates get one.
        self.name_label
            .set_modulate(Color::from_rgb(0.55, 0.78, 1.0));
        self.name_label.set_visible(friendly);
    }

    pub fn apply(&mut self, s: &RemoteSample, delta: f32) {
        let position = to_godot(s.position);
        self.base_mut().set_global_position(position);
        self.yaw = s.yaw;
        self.pitch = s.pitch;
        self.alive = s.alive;
        self.stance_eye = MoveParams::default().eye_height(s.stance);

        // Velocity from the drawn motion (snapshots carry no velocity). A jump
        // in position (respawn) restarts the estimate.
        if let Some(last) = self.last_position
            && delta > 0.0
        {
            let step = position - last;
            if step.length() < 2.0 {
                let measured = step / delta;
                self.velocity += (measured - self.velocity) * (1.0 - (-12.0 * delta).exp());
            } else {
                self.velocity = Vector3::ZERO;
            }
        }
        self.last_position = Some(position);

        self.reload_time = match (s.reloading && s.alive, self.reload_time) {
            (true, Some(t)) => Some(t + delta),
            (true, None) => Some(0.0),
            (false, _) => None,
        };
        let reload = self
            .reload_time
            .map(|t| (t / reload_duration(s.weapon)).clamp(0.0, 1.0));

        let frame = SoldierFrame {
            velocity: self.velocity,
            on_floor: s.on_floor,
            stance: s.stance,
            sprinting: s.sprinting,
            yaw: s.yaw,
            pitch: s.pitch,
            ads: s.ads,
            weapon: Some(s.weapon),
            reload,
            alive: s.alive,
            mantling: s.mantling,
        };
        self.rig.bind_mut().update(&frame, delta);
        // The server rewinds exactly this pose when it checks our shots.
        self.hitboxes.bind_mut().set_pose(s.hitbox_pose());

        // A hit from anyone: flinch away from it (or fall away, if it killed).
        let hit = self.hits.is_some_and(|last| s.hits != last);
        self.hits = Some(s.hits);
        if hit {
            self.hits_seen += 1;
            let (x, z) = protocol::hit_direction(s.hit_heading);
            self.rig.bind_mut().hit(Vector3::new(x, 0.0, z), !s.alive);
        }

        if let Some(gunfire) = &mut self.gunfire {
            gunfire.update(delta);
        }
        let fired = self
            .shots_fired
            .map_or(0, |last| s.shots_fired.wrapping_sub(last));
        self.shots_fired = Some(s.shots_fired);
        if fired > 0 && fired < 64 && s.alive {
            self.rig.bind_mut().fire();
            let muzzle = self.rig.bind().muzzle_position();
            let listener = self
                .base()
                .get_viewport()
                .and_then(|v| v.get_camera_3d())
                .map(|c| c.get_global_position());
            let indoors = zone_at(&self.base().get_tree(), muzzle).is_some();
            if let Some(gunfire) = &mut self.gunfire {
                gunfire.fire(s.weapon, muzzle, listener, indoors);
            }
            // Each round, traced on this client for its near miss and impact
            // (and for effects, through `shot_traced`). A snapshot rarely
            // carries more than one; at most three are traced.
            let eye = position + Vector3::UP * self.stance_eye;
            let direction = to_godot(look::view_direction(s.yaw, s.pitch));
            if let Some(mut tracer) = ShotTracer::find(&self.base().get_tree()) {
                for _ in 0..fired.min(3) {
                    tracer.bind_mut().trace(s.weapon, eye, direction, muzzle);
                }
            }
        }

        let listener = self
            .base()
            .get_viewport()
            .and_then(|v| v.get_camera_3d())
            .map(|c| c.get_global_position());
        let reload = reload.map(|progress| RemoteReload {
            weapon: s.weapon,
            progress,
        });
        let motion = Motion {
            feet: position,
            velocity: self.velocity,
            on_floor: s.on_floor,
            stance: s.stance,
            sprinting: s.sprinting,
            alive: s.alive,
            mantling: s.mantling,
            reloading: s.reloading,
            walk_speed: MoveParams::default().walk_speed,
        };
        let friendly = self.friendly;
        if let Some(foley) = &mut self.foley {
            foley.update(&motion, delta, listener, friendly, reload);
        }
    }
}

/// About how long a weapon's reload animation runs, in seconds. Snapshots only
/// say whether a player is reloading, so the third-person reload paces itself.
fn reload_duration(weapon: WeaponId) -> f32 {
    match weapon.def().reload {
        Reload::Magazine { time, .. } => time,
        Reload::PerRound {
            start,
            per_round,
            end,
        } => start + per_round * 4.0 + end,
    }
    .max(0.3)
}
