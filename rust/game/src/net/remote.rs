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
use shooter_sim::perk::{self, Perk, Perks};
use shooter_sim::snd::UseKind;
use shooter_sim::weapon::WeaponId;

use crate::acoustics::zone_at;
use crate::aim_assist::AimAssistTarget;
use crate::bullets::ShotTracer;
use crate::character::corpses::{self, CorpseYard};
use crate::character::rig::{ActionPose, SoldierFrame, SoldierRig, push_of};
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

    /// The name shown over a teammate.
    display_name: String,
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
    /// This player moves quietly (Dead Silence), and the perks of whoever
    /// is listening (Awareness).
    quiet: bool,
    listener_perks: Perks,
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
    /// A death our own knife predicted (knife hits always kill): seconds
    /// since, and the body it left, until a snapshot confirms it. If none
    /// does in [`PREDICTED_DEATH_SECONDS`], the server disagreed: the body
    /// goes and the player is drawn alive again.
    predicted_death: Option<(f32, Gd<SoldierRig>)>,
    /// The team colours the body wears (`SoldierRig::set_team_index`).
    #[init(val = 1)]
    team_index: i32,
    /// Makes no sound and traces no shots (a live player while a killcam
    /// plays over them).
    muted: bool,
    /// A killcam's copy of a player: never aim assisted, and the bodies it
    /// leaves are kept to take away when the killcam ends.
    replay: bool,
    /// Bodies this killcam copy has left in the corpse yard.
    left_bodies: Vec<Gd<SoldierRig>>,
    /// Seen from its own eyes (the killer in a killcam): the body is
    /// hidden and its gunfire is heard from the first-person weapon instead.
    first_person: bool,
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

    /// Called by the local player's knife strike. The server owns health, but
    /// a knife always kills, so the death is drawn now, on the strike,
    /// instead of an interpolation delay and a round trip later (it is
    /// undone if the server disagrees).
    #[func]
    fn apply_melee(&mut self, damage: f32) -> bool {
        self.alive && damage >= shooter_sim::player::HealthParams::default().max_health
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
        if killed && self.alive {
            self.predict_death();
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
    /// at `feet` with an M4, `stance` 0 standing, 1 crouching, 2 sliding, 3 prone.
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
            3 => Stance::Prone,
            _ => Stance::Standing,
        };
        let sample = RemoteSample {
            tick: 0.0,
            position: to_sim(feet),
            velocity: self.test_velocity(feet, delta),
            yaw: 0.0,
            pitch: 0.0,
            stance,
            crouch: MoveParams::full_crouch(stance),
            sprinting,
            on_floor,
            ads: 0.0,
            weapon,
            reloading,
            reload_empty: false,
            alive: true,
            shots_fired: u32::try_from(shots_fired).unwrap_or(0),
            mantling: false,
            vaulting: false,
            meleeing: false,
            throw: None,
            throw_kind: Default::default(),
            charge_use: None,
            hits: 0,
            hit_heading: 0,
            hit_part: 0,
            progress: Progress::default(),
            ladder: None,
            mount: 0.0,
        };
        let sample = self.with_test_progress(sample, None, delta);
        self.apply(&sample, delta);
    }

    /// Applies a made-up snapshot from a dictionary (tests): `feet`
    /// (Vector3), `yaw`, `pitch`, `stance` ("standing", "crouching", "prone",
    /// "sliding"), `sprinting`, `on_floor`, `ads`, `weapon` (file key),
    /// `reloading`, `reload_empty`, `mantling`, `vaulting`, `alive`, `hits`, `hit_heading`, `hit_part`
    /// (the snapshot's hit fields), `crouch` (0..1, the crouch blend;
    /// missing is fully in the stance), and the animation progress
    /// `stride`, `reload_progress`, and `mantle_progress` (0..1), and
    /// `action` ("plant" or "defuse") with `action_progress`. Missing keys
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
            Some("prone") => Stance::Prone,
            _ => Stance::Standing,
        };
        let sample = RemoteSample {
            tick: 0.0,
            position: to_sim(feet),
            velocity: self.test_velocity(feet, delta),
            yaw: f("yaw").unwrap_or(0.0),
            pitch: f("pitch").unwrap_or(0.0),
            stance,
            crouch: f("crouch").unwrap_or(MoveParams::full_crouch(stance)),
            sprinting: b("sprinting", false),
            on_floor: b("on_floor", true),
            ads: f("ads").unwrap_or(0.0),
            weapon,
            reloading: b("reloading", false),
            reload_empty: b("reload_empty", false),
            alive: b("alive", true),
            shots_fired: 0,
            mantling: b("mantling", false),
            vaulting: b("vaulting", false),
            meleeing: b("meleeing", false),
            throw: f("throw"),
            throw_kind: Default::default(),
            // `action` "plant" or "defuse" at `action_progress`: kneeling at
            // the charge.
            charge_use: match get("action").map(|v| v.to_string()).as_deref() {
                Some("plant") => Some((UseKind::Plant, f("action_progress").unwrap_or(0.5))),
                Some("defuse") => Some((UseKind::Defuse, f("action_progress").unwrap_or(0.5))),
                _ => None,
            },
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
            // `ladder_yaw`: on a ladder climbed facing that yaw.
            ladder: f("ladder_yaw").map(|yaw| {
                let (forward, _) = shooter_sim::movement::basis_from_yaw(yaw);
                shooter_sim::Vec2::new(forward.x, forward.z)
            }),
            mount: f("mount").unwrap_or(0.0),
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
/// A death our knife predicted waits this long (seconds) for the snapshot
/// that confirms it.
const PREDICTED_DEATH_SECONDS: f32 = 1.0;

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
            reload_rate: 1.0,
            weapon: s.weapon,
            mantle_left: self.test_mantle_left.unwrap_or(0.0),
            ladder: s.ladder.map(shooter_sim::hitbox::gait::yaw_of),
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
        if self.replay {
            // A replay's body, and the gun it drops, go with the replay.
            self.rig.add_to_group(corpses::REPLAY_GROUP);
            self.left_bodies.push(self.rig.clone());
        }
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
        self.display_name = name.to_owned();
        self.name_label.set_text(name);
    }

    /// Zombies: marks a teammate down in last stand over their head, seen
    /// through walls like their name, until they are back up.
    pub fn set_downed_marker(&mut self, downed: bool) {
        let (text, colour) = if downed {
            (
                format!("REVIVE\n{}", self.display_name),
                Color::from_rgb(1.0, 0.45, 0.3),
            )
        } else {
            (self.display_name.clone(), Color::from_rgb(0.55, 0.78, 1.0))
        };
        self.name_label.set_text(&text);
        self.name_label.set_modulate(colour);
    }

    /// Teammates wear the allies' colours, show their name, and take no hits
    /// from us; everyone else wears the enemies' colours.
    /// Whether this player moves quietly (Dead Silence, from its snapshot),
    /// and the listening player's perks (Awareness).
    pub fn set_hearing(&mut self, quiet: bool, listener: Perks) {
        self.quiet = quiet;
        self.listener_perks = listener;
    }

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
        self.name_label.set_visible(friendly && !self.first_person);
    }

    /// Silences this player's gunfire, footsteps, and traced shots.
    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }

    /// Makes this a killcam's copy of the player (see `replay`).
    pub fn set_replay(&mut self) {
        self.replay = true;
        self.update_aim_assist();
    }

    /// Draws this player from their own eyes: the body hides, and the
    /// first-person weapon makes their gunfire (the killcam's killer).
    pub fn set_first_person(&mut self, first_person: bool) {
        self.first_person = first_person;
        if first_person {
            self.rig.set_visible(false);
        }
        self.name_label.set_visible(self.friendly && !first_person);
    }

    /// Takes away the bodies this killcam copy left (the killcam ended).
    pub fn release_bodies(&mut self) {
        let tree = self.base().get_tree();
        let mut yard = CorpseYard::of(&tree);
        for body in self.left_bodies.drain(..) {
            if body.is_instance_valid() {
                yard.bind_mut().release(body);
            }
        }
    }

    /// Aim assist is on for living enemies only.
    fn update_aim_assist(&mut self) {
        let on = self.sided && !self.friendly && self.alive && !self.replay;
        self.aim_assist.bind_mut().set_active(on);
    }

    /// Poses only the hitboxes, at an earlier sample than the body: where
    /// they were on the frame a pending shot was clicked on.
    pub fn hold_hitboxes(&mut self, s: &RemoteSample) {
        self.hitboxes.bind_mut().set_pose(s.hitbox_pose());
    }

    /// Draws this player dying now, ahead of the server (our knife struck):
    /// the body falls from the pose it is in and a fresh one waits hidden.
    fn predict_death(&mut self) {
        if self.drawn_alive != Some(true) || !self.base().is_inside_tree() {
            return;
        }
        let dying = SoldierFrame {
            velocity: self.velocity,
            alive: false,
            ..SoldierFrame::default()
        };
        let body = self.rig.clone();
        self.rig.bind_mut().update(&dying, 0.0);
        self.leave_body();
        self.alive = false;
        self.drawn_alive = Some(false);
        self.update_aim_assist();
        self.predicted_death = Some((0.0, body));
    }

    /// The sample as it should be drawn: dead while a predicted death waits
    /// for the server, until a snapshot confirms it or the wait runs out.
    fn with_predicted_death(&mut self, s: &RemoteSample, delta: f32) -> RemoteSample {
        let Some((age, body)) = &mut self.predicted_death else {
            return *s;
        };
        if !s.alive {
            // Confirmed: the body we dropped is the one that died.
            self.predicted_death = None;
            return *s;
        }
        *age += delta;
        if *age < PREDICTED_DEATH_SECONDS {
            let mut dead = *s;
            dead.alive = false;
            return dead;
        }
        // The server says the strike did not kill: take the body away and
        // draw the player alive again.
        let body = body.clone();
        self.predicted_death = None;
        if body.is_instance_valid() {
            let tree = self.base().get_tree();
            CorpseYard::of(&tree).bind_mut().release(body);
        }
        *s
    }

    pub fn apply(&mut self, s: &RemoteSample, delta: f32) {
        let shown = self.with_predicted_death(s, delta);
        let s = &shown;
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
            reload_empty: s.reload_empty,
            alive: s.alive,
            mantling: s.mantling,
            vaulting: s.vaulting,
            meleeing: s.meleeing,
            stride: Some(s.progress.stride),
            mantle_progress: s.mantling.then_some(s.progress.mantle),
            crouch: Some(s.crouch),
            turn: Some((s.progress.turn, s.progress.step)),
            // Kneeling over the charge, or a throw.
            action: s
                .charge_use
                .map(|(kind, t)| {
                    let pose = match kind {
                        UseKind::Plant => ActionPose::Plant,
                        UseKind::Defuse => ActionPose::Defuse,
                    };
                    (pose, t)
                })
                .or_else(|| s.throw.map(|t| (ActionPose::Throw, t)))
                .filter(|_| s.alive),
            throw_kind: s.throw_kind,
            ladder: s.ladder,
            mount: s.mount,
        };
        let rig = crate::profile::scope("remotes/rig update");
        match (self.drawn_alive, s.alive) {
            // Dead: the body that died lies where it fell, in the scene's
            // corpse yard, and a fresh one waits hidden for the respawn.
            (Some(true), false) => {
                self.rig.bind_mut().update(&frame, delta);
                self.leave_body();
            }
            (_, false) => {}
            (_, true) => {
                if !self.rig.is_visible() && !self.first_person {
                    self.rig.set_visible(true);
                }
                self.rig.bind_mut().update(&frame, delta);
            }
        }
        if self.drawn_alive.is_none() && !s.alive {
            self.rig.set_visible(false);
        }
        self.drawn_alive = Some(s.alive);
        drop(rig);
        // The server rewinds exactly this pose when it checks our shots.
        let hitboxes = crate::profile::scope("remotes/hitboxes");
        self.hitboxes.bind_mut().set_pose(s.hitbox_pose());
        drop(hitboxes);

        if let Some(gunfire) = &mut self.gunfire {
            gunfire.update(delta);
        }
        let fired = self
            .shots_fired
            .map_or(0, |last| s.shots_fired.wrapping_sub(last));
        self.shots_fired = Some(s.shots_fired);
        if fired > 0 && fired < 64 && s.alive && !self.muted {
            self.rig.bind_mut().fire();
            let muzzle = self.rig.bind().muzzle_position();
            let listener = self
                .base()
                .get_viewport()
                .and_then(|v| v.get_camera_3d())
                .map(|c| c.get_global_position());
            let indoors = zone_at(&self.base().get_tree(), muzzle).is_some();
            if let Some(gunfire) = &mut self.gunfire
                && !self.first_person
            {
                gunfire.fire(s.weapon, muzzle, listener, indoors);
            }
            // Each round, traced on this client for its near miss and impact
            // (and for effects, through `shot_traced`). A snapshot rarely
            // carries more than one; at most three are traced.
            let eye = position + Vector3::UP * self.stance_eye;
            let direction = to_godot(look::view_direction(s.yaw, s.pitch));
            // A rocket is no round: it flies in the snapshots' grenades.
            let rounds = !s.weapon.def().rocket;
            if rounds && let Some(mut tracer) = ShotTracer::find(&self.base().get_tree()) {
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
            ladder: s.ladder.is_some() && s.alive,
        };
        let friendly = self.friendly;
        let mover = if self.quiet {
            Perks::of(&[Perk::DeadSilence])
        } else {
            Perks::NONE
        };
        let hearing = perk::heard(mover, self.listener_perks, !friendly);
        if let Some(foley) = &mut self.foley
            && !self.muted
        {
            foley.update(&motion, delta, listener, friendly, hearing, reload);
        }
    }
}
