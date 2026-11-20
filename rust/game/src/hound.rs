//! `Hound`: a Zombies hound as clients draw it (`net::horde_view`), for the
//! server's hound (`shooter_sim::zombies::hounds`, `net::horde`).
//!
//! The scene (`scenes/zombies/hound.tscn`) holds the Blender-built hound
//! (`tools/build_hound.py`) with its `run`, `bite`, and `idle` animations,
//! the hit zones the local player's shots and knife find (`HitboxRig`, posed
//! as the server rewinds hounds), and an aim-assist body. It runs at the
//! snapshots' pace, bites when the server's swing counter moves, flinches
//! when hit, and on death flares and crumbles to ash where it fell, as Black
//! Ops' hounds burst (no ragdoll).

use godot::classes::{AnimationPlayer, INode3D, Node3D, OmniLight3D, animation::LoopMode};
use godot::prelude::*;
use shooter_sim::zombies::hounds;

use crate::aim_assist::AimAssistTarget;
use crate::convert::to_sim;
use crate::hitbox_rig::HitboxRig;

/// Ground speed at which the gallop plays at its own pace (the hounds'
/// run speed).
const RUN_SPEED: f32 = 6.2;
/// Below this ground speed (m/s) it stands and pants.
const STILL_SPEED: f32 = 0.6;
/// Seconds a dead hound takes to crumble away.
const DEATH_SECONDS: f32 = 0.6;
/// How far a hit knocks the drawn body back, and how fast it recovers.
const FLINCH_DISTANCE: f32 = 0.12;
const FLINCH_RECOVERY: f32 = 8.0;
/// The flare's colour, the hound's cold glow.
const GLOW: Color = Color::from_rgb(0.55, 0.85, 1.0);

#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct Hound {
    base: Base<Node3D>,
    #[init(node = "Hitboxes")]
    hitboxes: OnReady<Gd<HitboxRig>>,
    anim: Option<Gd<AnimationPlayer>>,
    model: Option<Gd<Node3D>>,
    yaw: f32,
    /// Ground velocity, from the snapshots.
    moved: Vector3,
    #[init(val = true)]
    alive: bool,
    dead_for: f32,
    /// Seconds left of the bite playing.
    biting: f32,
    /// The drawn body's knock-back from a hit, easing out.
    flinch: Vector3,
    flare: Option<Gd<OmniLight3D>>,
    /// When the local player's hit last flinched it (engine seconds), so the
    /// server's report of that hit does not flinch it again.
    flinched_at: Option<f64>,
}

#[godot_api]
impl INode3D for Hound {
    fn ready(&mut self) {
        self.model = self.base().try_get_node_as::<Node3D>("Model");
        self.anim = self
            .model
            .as_ref()
            .and_then(|m| m.find_child_ex("AnimationPlayer").owned(false).done())
            .and_then(|n| n.try_cast::<AnimationPlayer>().ok());
        if let Some(anim) = &mut self.anim {
            for name in ["run", "idle"] {
                if let Some(mut animation) = anim.get_animation(name) {
                    animation.set_loop_mode(LoopMode::LINEAR);
                }
            }
            anim.play_ex().name("idle").done();
        }
    }

    fn process(&mut self, delta: f64) {
        let _profile = crate::profile::scope("hounds (frame)");
        let dt = delta as f32;
        if !self.alive {
            self.crumble(dt);
            return;
        }
        self.flinch *= 1.0 - (FLINCH_RECOVERY * dt).min(1.0);
        if let Some(model) = &mut self.model {
            model.set_position(self.flinch);
        }
        let yaw = self.yaw;
        self.base_mut().set_rotation(Vector3::new(0.0, yaw, 0.0));
        self.biting = (self.biting - dt).max(0.0);
        if self.biting > 0.0 {
            return;
        }
        let speed = Vector3::new(self.moved.x, 0.0, self.moved.z).length();
        let Some(anim) = &mut self.anim else {
            return;
        };
        if speed > STILL_SPEED {
            if anim.get_current_animation() != "run" {
                anim.play_ex().name("run").custom_blend(0.15).done();
            }
            anim.set_speed_scale((speed / RUN_SPEED).clamp(0.5, 1.4));
        } else if anim.get_current_animation() != "idle" {
            anim.set_speed_scale(1.0);
            anim.play_ex().name("idle").custom_blend(0.25).done();
        }
    }
}

#[godot_api]
impl Hound {
    /// The server owns its health; a hit here only predicts the marker.
    #[func]
    pub fn apply_damage(&mut self, _amount: f32, _headshot: bool) -> bool {
        false
    }

    #[func]
    pub fn apply_melee(&mut self, _amount: f32) -> bool {
        false
    }

    /// The local player's hit: a flinch now, before the server's word.
    #[func]
    pub fn react_to_hit(
        &mut self,
        direction: Vector3,
        _killed: bool,
        _point: Vector3,
        _part: i64,
        _force: f32,
    ) {
        self.flinched_at = Some(now_seconds());
        self.flinch_along(direction);
    }

    #[func]
    pub fn is_alive(&self) -> bool {
        self.alive
    }

    /// Places the drawn hound where the server's was, facing `yaw` and
    /// moving at `velocity`, with its hit zones posed to match.
    #[func]
    pub fn net_apply(&mut self, feet: Vector3, yaw: f32, velocity: Vector3) {
        self.base_mut().set_global_position(feet);
        self.yaw = yaw;
        self.moved = velocity;
        if self.alive {
            let pose = hounds::hitbox_pose(to_sim(feet), yaw);
            self.hitboxes.bind_mut().set_pose(Some(pose));
        }
    }

    /// The server's hound bit (or snapped at nothing).
    #[func]
    pub fn net_swing(&mut self) {
        if let Some(anim) = &mut self.anim {
            anim.set_speed_scale(1.0);
            anim.play_ex().name("bite").custom_blend(0.08).done();
            self.biting = anim
                .get_animation("bite")
                .map_or(0.35, |a| a.get_length() as f32);
        }
    }

    /// The server's hound died: it flares cold and crumbles where it fell.
    #[func]
    pub fn net_die(&mut self) {
        if !self.alive {
            return;
        }
        self.alive = false;
        self.hitboxes.bind_mut().set_pose(None);
        if let Some(mut assist) = self.base().try_get_node_as::<AimAssistTarget>("AimAssist") {
            assist.bind_mut().set_active(false);
        }
        if let Some(anim) = &mut self.anim {
            anim.pause();
        }
        let mut flare = OmniLight3D::new_alloc();
        flare.set_color(GLOW);
        flare.set_param(godot::classes::light_3d::Param::ENERGY, 4.0);
        flare.set_param(godot::classes::light_3d::Param::RANGE, 5.0);
        flare.set_position(Vector3::UP * 0.6);
        self.base_mut().add_child(&flare);
        self.flare = Some(flare);
    }
}

impl Hound {
    /// A hit the server reported, travelling along `direction`.
    pub fn flinch_along(&mut self, direction: Vector3) {
        let flat = Vector3::new(direction.x, 0.0, direction.z).normalized_or_zero();
        self.flinch = flat * FLINCH_DISTANCE;
    }

    /// Whether the local player's own hit flinched it in the last `seconds`;
    /// answers yes once per hit.
    pub fn predicted_flinch(&mut self, seconds: f32) -> bool {
        let recent = self
            .flinched_at
            .is_some_and(|at| now_seconds() - at < f64::from(seconds));
        self.flinched_at = None;
        recent
    }

    /// Shrinks and sinks the dead body and fades the flare, then frees it.
    fn crumble(&mut self, dt: f32) {
        self.dead_for += dt;
        let t = (self.dead_for / DEATH_SECONDS).min(1.0);
        if let Some(model) = &mut self.model {
            let scale = 1.0 - t * t;
            model.set_scale(Vector3::new(1.0 + t * 0.3, scale.max(0.02), 1.0 + t * 0.3));
            model.set_position(self.flinch + Vector3::DOWN * (t * 0.2));
        }
        if let Some(flare) = &mut self.flare {
            flare.set_param(godot::classes::light_3d::Param::ENERGY, 4.0 * (1.0 - t));
        }
        if t >= 1.0 {
            self.base_mut().queue_free();
        }
    }
}

fn now_seconds() -> f64 {
    godot::classes::Time::singleton().get_ticks_usec() as f64 / 1e6
}
