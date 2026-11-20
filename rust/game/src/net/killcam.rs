//! Killcam playback on the client: the killer's eyes and weapon, and every
//! other player as the killer saw them, replayed from the client's own
//! recording of the snapshots (`shooter_sim::killcam::Tape`).
//!
//! The replay draws its own copies of the players (`RemotePlayer`s in a
//! `Killcam` node), so the live ones keep following the match underneath,
//! hidden and silent, and nothing about them jumps when the killcam ends.
//! The killer's copy is drawn from their eyes: its body hides and the local
//! player's first-person weapon rig plays their weapon (`ViewDriver`), aim,
//! reloads, switches, shots, and knife. Bodies that fall in the replay are
//! taken away when it ends, and bodies already lying in the world hide
//! while it plays (they may not have fallen yet at the replayed moment).

use godot::prelude::*;
use shooter_sim::killcam::{Killcam, Playback, Tape};
use shooter_sim::movement::{MoveParams, Stance};
use shooter_sim::net::lag::RemoteSample;
use shooter_sim::weapon::{LoadStage, Reload, WeaponId};

use super::remote::RemotePlayer;
use crate::character::corpses::CorpseYard;
use crate::character::rig::SoldierRig;
use crate::convert::to_godot;
use crate::viewmodel::{ViewAction, ViewFrame};

/// One frame of a killcam as the local player draws it.
#[derive(Clone, Copy, Debug)]
pub struct KillcamView {
    /// The killer's eye: position and view angles.
    pub camera: Transform3D,
    /// Field of view against the player's own (aiming zooms in).
    pub zoom: f32,
    /// What the first-person weapon shows.
    pub frame: ViewFrame,
}

/// Something the killer's weapon did this frame, beyond its pose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponCue {
    Fired(WeaponId),
    Melee,
}

/// Turns the killer's replayed states into the first-person weapon's
/// frames: snapshots say what the weapon is doing, and this keeps the
/// timers (since a switch, a shot, a knife swing, a sprint's end) that the
/// rig animates from.
#[derive(Default)]
pub struct ViewDriver {
    last: Option<RemoteSample>,
    /// Replay seconds since the weapon came out, the last shot, the knife
    /// swing started, and the sprint ended.
    since_switch: f32,
    since_shot: Option<f32>,
    since_melee: Option<f32>,
    since_sprint: Option<f32>,
    look_velocity: Vector2,
}

/// Seconds a switch shows the weapon coming up, when the raise time is
/// unknown.
const MIN_RAISE: f32 = 0.2;

impl ViewDriver {
    /// The weapon's frame for sample `s`, `dt` replay seconds after the
    /// last, and what it did since.
    pub fn frame(&mut self, s: &RemoteSample, dt: f32, cues: &mut Vec<WeaponCue>) -> ViewFrame {
        let def = s.weapon.def();
        match self.last {
            Some(last) => {
                if last.weapon != s.weapon {
                    self.since_switch = 0.0;
                    self.since_shot = None;
                } else {
                    self.since_switch += dt;
                }
                let fired = s.shots_fired.wrapping_sub(last.shots_fired);
                if fired > 0 && fired < 64 && s.alive {
                    for _ in 0..fired.min(3) {
                        cues.push(WeaponCue::Fired(s.weapon));
                    }
                    self.since_shot = Some(0.0);
                } else if let Some(t) = &mut self.since_shot {
                    *t += dt;
                }
                if s.meleeing && !last.meleeing {
                    cues.push(WeaponCue::Melee);
                    self.since_melee = Some(0.0);
                } else if let Some(t) = &mut self.since_melee {
                    *t += dt;
                }
                if last.sprinting && !s.sprinting {
                    self.since_sprint = Some(0.0);
                } else if let Some(t) = &mut self.since_sprint {
                    *t += dt;
                }
                if dt > 0.0 {
                    let turned = Vector2::new(
                        shooter_sim::hitbox::gait::wrap_angle(s.yaw - last.yaw),
                        s.pitch - last.pitch,
                    ) / dt;
                    let measured = turned.clamp(Vector2::splat(-30.0), Vector2::splat(30.0));
                    let blend = 1.0 - (-20.0 * dt).exp();
                    self.look_velocity += (measured - self.look_velocity) * blend;
                }
            }
            // Mid-replay start: the weapon is already up.
            None => self.since_switch = def.raise_time.max(MIN_RAISE),
        }
        self.last = Some(*s);
        let action = self.action(s);
        let (sin, cos) = s.yaw.sin_cos();
        let v = s.velocity;
        let velocity = Vector3::new(v.x * cos - v.z * sin, v.y, -(v.x * sin + v.z * cos));
        let sprint_out = match self.since_sprint {
            Some(t) if def.sprint_out_time > 0.0 => (1.0 - t / def.sprint_out_time).clamp(0.0, 1.0),
            _ => 0.0,
        };
        ViewFrame {
            weapon: s.weapon,
            ads: s.ads,
            scope: if def.scope.is_some() { s.ads } else { 0.0 },
            visible: s.alive,
            sprinting: s.sprinting,
            on_floor: s.on_floor,
            action,
            empty: false,
            look_velocity: self.look_velocity,
            velocity,
            // The body's stride is two footsteps; the weapon bobs per step.
            stride_phase: (s.progress.stride * 2.0).fract(),
            sprint_out,
            crouched: s.stance == Stance::Crouching,
            sliding: s.stance == Stance::Sliding,
            mantle_left: if s.mantling {
                (1.0 - s.progress.mantle) * 0.4
            } else {
                0.0
            },
            vaulting: s.vaulting,
            on_ladder: s.ladder.is_some(),
            feet: to_godot(s.position),
            mount: s.mount,
        }
    }

    fn action(&self, s: &RemoteSample) -> ViewAction {
        let def = s.weapon.def();
        if let Some(t) = s.throw {
            return ViewAction::Throwing(t, s.throw_kind);
        }
        if s.meleeing {
            let since = self.since_melee.unwrap_or(0.0);
            return ViewAction::Meleeing {
                strike_in: shooter_sim::melee::STRIKE_DELAY - since,
            };
        }
        let raise = def.raise_time.max(MIN_RAISE);
        if self.since_switch < raise {
            return ViewAction::Raising(self.since_switch / raise);
        }
        if s.reloading {
            let elapsed = s.progress.reload * shooter_sim::hitbox::clock::reload_seconds(s.weapon);
            return match def.reload {
                Reload::Magazine { .. } => ViewAction::Reloading {
                    elapsed,
                    empty: s.reload_empty,
                },
                Reload::PerRound { start, .. } if elapsed < start => ViewAction::Loading {
                    stage: LoadStage::Opening,
                    elapsed,
                    length: start,
                },
                Reload::PerRound {
                    start, per_round, ..
                } => ViewAction::Loading {
                    stage: LoadStage::Round,
                    elapsed: (elapsed - start).rem_euclid(per_round.max(0.05)),
                    length: per_round,
                },
            };
        }
        if def.cycle_time > 0.0
            && let Some(t) = self.since_shot
            && t < def.cycle_time
        {
            return ViewAction::Cycling(t / def.cycle_time);
        }
        ViewAction::Ready
    }
}

/// The camera for the killer's eyes in sample `s`, and its zoom.
pub fn eye(s: &RemoteSample) -> (Transform3D, f32) {
    let height = MoveParams::default().eye_height_at(s.crouch);
    let origin = to_godot(s.position) + Vector3::UP * height;
    let basis = Basis::from_euler(EulerOrder::YXZ, Vector3::new(s.pitch, s.yaw, 0.0));
    let ads = crate::viewmodel::ads_ease(s.ads);
    let zoom = 1.0 + (s.weapon.def().ads_fov_scale - 1.0) * ads;
    (Transform3D::new(basis, origin), zoom)
}

/// A killcam being played.
pub struct Playing {
    pub playback: Playback,
    root: Gd<Node3D>,
    bodies: Vec<(u8, Gd<RemotePlayer>)>,
    driver: ViewDriver,
    /// Bodies already lying in the world, hidden while the replay plays.
    hidden_corpses: Vec<Gd<SoldierRig>>,
    /// Wall-clock seconds the last frame has been held after the end.
    held: f32,
}

impl Playing {
    /// Starts playing `killcam` from `tape`, with copies of its players made
    /// by `spawn` (already named and dressed) under `parent`.
    pub fn start(
        killcam: &Killcam,
        tape: &Tape,
        parent: &mut Gd<Node>,
        mut spawn: impl FnMut(u8) -> Option<Gd<RemotePlayer>>,
    ) -> Self {
        let mut root = Node3D::new_alloc();
        root.set_name("Killcam");
        parent.add_child(&root);
        let mut bodies = Vec::new();
        for id in tape.ids_between(killcam.start_tick(), killcam.end_tick()) {
            let Some(mut node) = spawn(id) else {
                continue;
            };
            node.reparent(&root);
            {
                let mut body = node.bind_mut();
                body.set_replay();
                body.set_first_person(id == killcam.killer);
            }
            bodies.push((id, node));
        }
        let tree = parent.get_tree();
        let yard = CorpseYard::of(&tree);
        let mut hidden_corpses: Vec<Gd<SoldierRig>> = yard.bind().corpses().iter_shared().collect();
        hidden_corpses.retain(|c| c.is_visible());
        for corpse in &mut hidden_corpses {
            corpse.set_visible(false);
        }
        Self {
            playback: Playback::new(killcam),
            root,
            bodies,
            driver: ViewDriver::default(),
            hidden_corpses,
            held: 0.0,
        }
    }

    /// Advances the replay by `dt` wall-clock seconds and draws it: every
    /// copy posed from the tape, and the killer's view for the local player
    /// with what their weapon did. `None` once the killer is off the tape.
    pub fn update(
        &mut self,
        tape: &Tape,
        dt: f32,
        cues: &mut Vec<WeaponCue>,
    ) -> Option<KillcamView> {
        if self.playback.finished() {
            self.held += dt;
        }
        let replay_dt = self.playback.advance(dt);
        let killer = self.playback.killcam().killer;
        for (id, body) in &mut self.bodies {
            let tick = if *id == killer {
                self.playback.tick()
            } else {
                self.playback.others_tick()
            };
            if let Some(sample) = tape.sample(*id, tick) {
                body.bind_mut().apply(&sample, replay_dt);
            }
        }
        let sample = tape.sample(killer, self.playback.tick())?;
        let frame = self.driver.frame(&sample, replay_dt, cues);
        let (camera, zoom) = eye(&sample);
        Some(KillcamView {
            camera,
            zoom,
            frame,
        })
    }

    /// Wall-clock seconds the replay has held its last frame.
    pub fn held(&self) -> f32 {
        self.held
    }

    /// Takes the replay away: its players and the bodies they left, and
    /// shows the bodies that were lying in the world again.
    pub fn stop(mut self) {
        for (_, body) in &mut self.bodies {
            body.bind_mut().release_bodies();
        }
        self.root.queue_free();
        for corpse in &mut self.hidden_corpses {
            if corpse.is_instance_valid() {
                corpse.set_visible(true);
            }
        }
    }
}
