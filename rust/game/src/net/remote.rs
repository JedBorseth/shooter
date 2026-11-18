//! Another player as this client sees them: interpolated from snapshots and
//! drawn as the third-person soldier (`SoldierRig`), with their gunfire at the
//! muzzle (`gunfire`), their footsteps, landings, slides, and reloads at their
//! feet (`footsteps`), and hit zones posed from the same snapshot (drawn with
//! F4).
//!
//! Enemies carry an `AimAssistTarget` sized to their stance, on while they are
//! alive. Teammates, and anyone before the roster says which side they are on,
//! get no aim assist.
//!
//! The local player's shots hit these hit zones for instant feedback (hit
//! markers, blood, a flinch), but deal no damage here: the server rewinds the
//! same poses (`shooter_sim::net::lag`) and decides every hit and kill.

use godot::classes::{INode3D, Label3D};
use godot::prelude::*;
use shooter_sim::hitbox::clock::{BodyClock, BodyState, Progress};
use shooter_sim::look;
use shooter_sim::movement::{MoveParams, Stance};
pub use shooter_sim::net::lag::RemoteSample;
use shooter_sim::net::protocol;
use shooter_sim::weapon::WeaponId;

use crate::acoustics::zone_at;
use crate::aim_assist::AimAssistTarget;
use crate::bullets::ShotTracer;
use crate::character::corpses::{self, CorpseYard};
use crate::character::rig::{SoldierFrame, SoldierRig, push_of};
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
    #[init(node = "AimAssist")]
    aim_assist: OnReady<Gd<AimAssistTarget>>,

    shots_fired: Option<u32>,
    hits: Option<u8>,
    /// Flinches played from snapshots (tests).
    hits_seen: u32,
    /// Last drawn feet position and the velocity estimated from it.
    last_position: Option<Vector3>,
    velocity: Vector3,
    /// Made-up samples (tests) have no server clock: this stands in for it.
    test_clock: BodyClock,
    /// Seconds left of a made-up mantle (tests).
    test_mantle_left: Option<f32>,
    gunfire: Option<Gunfire>,
    foley: Option<Foley>,
    friendly: bool,
    /// The roster has said which side this player is on.
    sided: bool,
    /// View angles and life from the last drawn sample, for spectating.
    yaw: f32,
    pitch: f32,
    #[init(val = true)]
    alive: bool,
    stance_eye: f32,
    /// Whether the body was last drawn alive (`None` before the first sample).
    drawn_alive: Option<bool>,
    /// Seconds since each flinch our own hits predicted, oldest first: the
    /// snapshot that confirms a hit does not flinch the body a second time.
    predicted: std::collections::VecDeque<f32>,
    /// The team colours the body wears (`SoldierRig::set_team_index`).
    #[init(val = 1)]
    team_index: i32,
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

    /// Eye height above the feet as last drawn (it follows the crouch blend).
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
    pub fn react_to_hit(
        &mut self,
        direction: Vector3,
        killed: bool,
        point: Vector3,
        part: i64,
        force: f32,
    ) {
        let push = push_of(direction, point, part, force);
        self.rig.bind_mut().hit_at(push, killed);
        if !killed && self.alive {
            self.predicted.push_back(0.0);
        }
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
        let stance = match stance {
            1 => Stance::Crouching,
            2 => Stance::Sliding,
            _ => Stance::Standing,
        };
        let sample = RemoteSample {
            tick: 0.0,
            position: to_sim(feet),
            velocity: self.test_velocity(feet, delta),
            yaw: 0.0,
            pitch: 0.0,
            stance,
            crouch: if stance == Stance::Standing { 0.0 } else { 1.0 },
            sprinting,
            on_floor,
            ads: 0.0,
            weapon,
            reloading,
            alive: true,
            shots_fired: u32::try_from(shots_fired).unwrap_or(0),
            mantling: false,
            meleeing: false,
            hits: 0,
            hit_heading: 0,
            hit_part: 0,
            progress: Progress::default(),
        };
        let sample = self.with_test_progress(sample, None, delta);
        self.apply(&sample, delta);
    }

    /// Applies a made-up snapshot from a dictionary (tests): `feet`
    /// (Vector3), `yaw`, `pitch`, `stance` ("standing", "crouching",
    /// "sliding"), `sprinting`, `on_floor`, `ads`, `weapon` (file key),
    /// `reloading`, `mantling`, `alive`, `hits`, `hit_heading`, `hit_part`
    /// (the snapshot's hit fields), `crouch` (0..1, the crouch blend;
    /// missing is fully in the stance), and the animation progress
    /// `stride`, `reload_progress`, and `mantle_progress` (0..1). Missing keys
    /// take a standing, living M4 holder's values; missing progress runs as a
    /// server would run it. Velocity comes from how `feet` moves.
    #[func]
    fn apply_test_state(&mut self, state: VarDictionary, delta: f32) {
        let get = |key: &str| state.get(key);
        let f = |key: &str| get(key).and_then(|v| v.try_to::<f32>().ok());
        let b = |key: &str, default: bool| {
            get(key)
                .and_then(|v| v.try_to::<bool>().ok())
                .unwrap_or(default)
        };
        let Some(weapon) = get("weapon")
            .and_then(|v| WeaponId::by_key(&v.to_string()))
            .or_else(|| WeaponId::by_key("m4"))
        else {
            return;
        };
        let feet = get("feet")
            .and_then(|v| v.try_to::<Vector3>().ok())
            .unwrap_or(Vector3::ZERO);
        let stance = match get("stance").map(|v| v.to_string()).as_deref() {
            Some("crouching") => Stance::Crouching,
            Some("sliding") => Stance::Sliding,
            _ => Stance::Standing,
        };
        let sample = RemoteSample {
            tick: 0.0,
            position: to_sim(feet),
            velocity: self.test_velocity(feet, delta),
            yaw: f("yaw").unwrap_or(0.0),
            pitch: f("pitch").unwrap_or(0.0),
            stance,
            crouch: f("crouch").unwrap_or(if stance == Stance::Standing { 0.0 } else { 1.0 }),
            sprinting: b("sprinting", false),
            on_floor: b("on_floor", true),
            ads: f("ads").unwrap_or(0.0),
            weapon,
            reloading: b("reloading", false),
            alive: b("alive", true),
            shots_fired: 0,
            mantling: b("mantling", false),
            meleeing: b("meleeing", false),
            hits: get("hits")
                .and_then(|v| v.try_to::<i64>().ok())
                .unwrap_or(0) as u8,
            hit_heading: get("hit_heading")
                .and_then(|v| v.try_to::<i64>().ok())
                .unwrap_or(0) as u8,
            hit_part: get("hit_part")
                .and_then(|v| v.try_to::<i64>().ok())
                .unwrap_or(0) as u8,
            progress: Progress::default(),
        };
        let set = TestProgress {
            stride: f("stride"),
            reload: f("reload_progress"),
            mantle: f("mantle_progress"),
        };
        let sample = self.with_test_progress(sample, Some(set), delta);
        self.apply(&sample, delta);
    }
}

/// Animation progress a test names outright instead of letting it run.
struct TestProgress {
    stride: Option<f32>,
    reload: Option<f32>,
    mantle: Option<f32>,
}

/// A predicted flinch waits this long (seconds) for the snapshot that
/// confirms its hit: a round trip and the interpolation delay, with room.
const PREDICTED_FLINCH_SECONDS: f32 = 1.0;

/// The body every player wears.
const SOLDIER_SCENE: &str = "res://scenes/characters/soldier.tscn";

/// A made-up mantle (tests) lasts this long, as a mantle up a metre does.
const TEST_MANTLE_SECONDS: f32 = 0.42;

impl RemotePlayer {
    /// A made-up sample with the animation progress a server would have sent
    /// for it (a [`BodyClock`] run on the made-up states), or what `set` names.
    fn with_test_progress(
        &mut self,
        mut s: RemoteSample,
        set: Option<TestProgress>,
        delta: f32,
    ) -> RemoteSample {
        self.test_mantle_left = match (s.mantling, self.test_mantle_left) {
            (true, Some(left)) => Some((left - delta).max(0.0)),
            (true, None) => Some(TEST_MANTLE_SECONDS - delta),
            (false, _) => None,
        };
        let state = BodyState {
            velocity: s.velocity,
            yaw: s.yaw,
            stance: s.stance,
            sprinting: s.sprinting,
            on_floor: s.on_floor,
            alive: s.alive,
            reloading: s.reloading,
            weapon: s.weapon,
            mantle_left: self.test_mantle_left.unwrap_or(0.0),
        };
        let mut p = self.test_clock.tick(&state, delta);
        if let Some(set) = set {
            p.stride = set.stride.unwrap_or(p.stride);
            p.reload = set.reload.unwrap_or(p.reload);
            p.mantle = set.mantle.unwrap_or(p.mantle);
        }
        s.progress = p;
        s
    }

    /// A made-up sample's velocity: how far the feet moved since the last one.
    fn test_velocity(&self, feet: Vector3, delta: f32) -> shooter_sim::Vec3 {
        match self.last_position {
            Some(last) if delta > 0.0 && last.distance_to(feet) < 2.0 => {
                to_sim((feet - last) / delta)
            }
            _ => shooter_sim::Vec3::ZERO,
        }
    }

    /// Hands the dying body to the corpse yard and puts a fresh one in its
    /// place, hidden until the player is alive again.
    fn leave_body(&mut self) {
        if !self.base().is_inside_tree() {
            return;
        }
        let Some(mut fresh) = try_load::<PackedScene>(SOLDIER_SCENE)
            .ok()
            .and_then(|scene| scene.try_instantiate_as::<SoldierRig>())
        else {
            godot_error!("RemotePlayer: cannot make a new body from {SOLDIER_SCENE}");
            return;
        };
        let tree = self.base().get_tree();
        CorpseYard::of(&tree)
            .bind_mut()
            .adopt(self.rig.clone(), corpses::SOLDIER_SECONDS);
        fresh.set_name("Soldier");
        fresh.set_visible(false);
        self.base_mut().add_child(&fresh);
        let team = self.team_index;
        fresh.bind_mut().set_team_index(team);
        *self.rig = fresh;
    }

    pub fn set_display_name(&mut self, name: &str) {
        self.name_label.set_text(name);
    }

    /// Teammates wear the allies' colours, show their name, and take no hits
    /// from us; everyone else wears the enemies' colours.
    pub fn set_friendly(&mut self, friendly: bool) {
        self.team_index = if friendly { 0 } else { 1 };
        let team = self.team_index;
        self.rig.bind_mut().set_team_index(team);
        self.base_mut().set_meta("friendly", &friendly.to_variant());
        self.friendly = friendly;
        self.sided = true;
        self.update_aim_assist();
        // Names show through walls, so only teammates get one. The label starts
        // hidden (remote_player.tscn) so nobody's name shows before the roster
        // says which side they are on.
        self.name_label
            .set_modulate(Color::from_rgb(0.55, 0.78, 1.0));
        self.name_label.set_visible(friendly);
    }

    /// Aim assist is on for living enemies only.
    fn update_aim_assist(&mut self) {
        let on = self.sided && !self.friendly && self.alive;
        self.aim_assist.bind_mut().set_active(on);
    }

    /// Poses only the hitboxes, at an earlier sample than the body: where
    /// they were on the frame a pending shot was clicked on.
    pub fn hold_hitboxes(&mut self, s: &RemoteSample) {
        self.hitboxes.bind_mut().set_pose(s.hitbox_pose());
    }

    pub fn apply(&mut self, s: &RemoteSample, delta: f32) {
        let position = to_godot(s.position);
        self.base_mut().set_global_position(position);
        self.yaw = s.yaw;
        self.pitch = s.pitch;
        self.alive = s.alive;
        self.stance_eye = MoveParams::default().eye_height_at(s.crouch);
        let height = MoveParams::default().height_at(s.crouch);
        self.aim_assist.bind_mut().set_body_height(height);
        self.update_aim_assist();

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

        // A hit from anyone: flinch away from it, or, if it killed, fall with
        // its push (before the body is drawn dead below).
        let hit = self.hits.is_some_and(|last| s.hits != last);
        self.hits = Some(s.hits);
        if hit {
            self.hits_seen += 1;
            let (x, z) = protocol::hit_direction(s.hit_heading);
            let part = protocol::HitPart::from_u8(s.hit_part);
            let push = push_of(
                Vector3::new(x, 0.0, z),
                Vector3::ZERO,
                part.part.map_or(-1, i64::from),
                f32::from(part.force),
            );
            // Our own hit already flinched the body when we fired: the server
            // confirming it only updates the push a death would take.
            let confirms_ours = s.alive && self.predicted.pop_front().is_some();
            if confirms_ours {
                self.rig.bind_mut().note_hit(push);
            } else {
                self.rig.bind_mut().hit_at(push, !s.alive);
            }
        }
        for age in &mut self.predicted {
            *age += delta;
        }
        while self
            .predicted
            .front()
            .is_some_and(|age| *age > PREDICTED_FLINCH_SECONDS)
        {
            self.predicted.pop_front();
        }

        // Drawn at the moment of each animation the snapshot names, which is
        // the moment its hit zones are posed at.
        let reload = (s.reloading && s.alive).then_some(s.progress.reload);
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
            meleeing: s.meleeing,
            stride: Some(s.progress.stride),
            mantle_progress: s.mantling.then_some(s.progress.mantle),
            crouch: Some(s.crouch),
        };
        match (self.drawn_alive, s.alive) {
            // Dead: the body that died lies where it fell, in the scene's
            // corpse yard, and a fresh one waits hidden for the respawn.
            (Some(true), false) => {
                self.rig.bind_mut().update(&frame, delta);
                self.leave_body();
            }
            (_, false) => {}
            (_, true) => {
                if !self.rig.is_visible() {
                    self.rig.set_visible(true);
                }
                self.rig.bind_mut().update(&frame, delta);
            }
        }
        if self.drawn_alive.is_none() && !s.alive {
            self.rig.set_visible(false);
        }
        self.drawn_alive = Some(s.alive);
        // The server rewinds exactly this pose when it checks our shots.
        self.hitboxes.bind_mut().set_pose(s.hitbox_pose());

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
