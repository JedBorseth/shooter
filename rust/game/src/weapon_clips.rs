//! Keyframed first-person clips for reloads, draws, holsters, and inspects.
//!
//! No CC0 first-person animation exists for these weapons, so every clip is
//! authored here as keys in seconds. Reload clips are built per weapon class
//! from the weapon's own timings, so the beats line up with the rules: the
//! fresh magazine seats exactly at the simulation's `mag_in` (when its rounds
//! count), and the empty reload's bolt release or charge lands before the
//! weapon can fire. Each beat names a foley event (`WeaponView::foley_event`)
//! and gives the weapon a small jolt.
//!
//! Poses are in the weapon view's space: the origin on the sight line at the
//! rear sight, +X right, +Y up, -Z down the barrel. Rotations are degrees
//! (pitch up, yaw left, roll counter-clockwise as the player sees it).

use godot::prelude::Vector3;
use shooter_sim::weapon::{LoadStage, Reload, Slot, WeaponDef};

pub fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Piecewise keyframes `(time, value)`, eased between keys.
pub fn keyed(keys: &[(f32, f32)], t: f32) -> f32 {
    let Some(first) = keys.first() else {
        return 0.0;
    };
    if t <= first.0 {
        return first.1;
    }
    for pair in keys.windows(2) {
        let ((t0, v0), (t1, v1)) = (pair[0], pair[1]);
        if t <= t1 {
            let u = smoothstep((t - t0) / (t1 - t0).max(1e-4));
            return v0 + (v1 - v0) * u;
        }
    }
    keys.last().map_or(0.0, |k| k.1)
}

/// A whole-weapon pose: rotation (degrees) and offset (metres).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    pub rot: Vector3,
    pub pos: Vector3,
}

impl Pose {
    pub const ZERO: Pose = Pose {
        rot: Vector3::ZERO,
        pos: Vector3::ZERO,
    };

    pub const fn new(pitch: f32, yaw: f32, roll: f32, x: f32, y: f32, z: f32) -> Pose {
        Pose {
            rot: Vector3::new(pitch, yaw, roll),
            pos: Vector3::new(x, y, z),
        }
    }

    pub fn scaled(self, k: f32) -> Pose {
        Pose {
            rot: self.rot * k,
            pos: self.pos * k,
        }
    }

    fn lerp(self, other: Pose, u: f32) -> Pose {
        Pose {
            rot: self.rot.lerp(other.rot, u),
            pos: self.pos.lerp(other.pos, u),
        }
    }
}

/// Pose keys `(time, pose)`, eased between keys.
fn posed(keys: &[(f32, Pose)], t: f32) -> Pose {
    let Some(first) = keys.first() else {
        return Pose::ZERO;
    };
    if t <= first.0 {
        return first.1;
    }
    for pair in keys.windows(2) {
        let ((t0, p0), (t1, p1)) = (pair[0], pair[1]);
        if t <= t1 {
            return p0.lerp(p1, smoothstep((t - t0) / (t1 - t0).max(1e-4)));
        }
    }
    keys.last().map_or(Pose::ZERO, |k| k.1)
}

/// How a weapon's clips are staged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipKind {
    /// A box magazine under the receiver. `charge`: the empty reload ends by
    /// pulling the charging handle; otherwise by slapping the bolt release.
    Rifle { charge: bool, heavy: bool },
    /// A magazine in the grip; the empty reload ends by dropping the slide.
    Pistol,
    /// A bolt action fed from a magazine; the empty reload ends by working
    /// the bolt.
    BoltAction,
    /// Shells loaded one at a time through the loading port.
    Shotgun,
}

impl ClipKind {
    pub fn of(def: &WeaponDef) -> ClipKind {
        match def.reload {
            Reload::PerRound { .. } => ClipKind::Shotgun,
            _ if def.cycle_time > 0.0 => ClipKind::BoltAction,
            _ if def.slot == Slot::Secondary => ClipKind::Pistol,
            _ => ClipKind::Rifle {
                // AR-pattern rifles lock open on empty and close with the
                // bolt release; the rest are charged.
                charge: def.model_key() != "m4",
                heavy: def.magazine > 50,
            },
        }
    }
}

/// One moment in a clip: a foley event and a jolt to the weapon (metres and
/// degrees, added to the kick springs as peaks).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beat {
    pub at: f32,
    pub name: &'static str,
    pub jolt_pos: Vector3,
    pub jolt_rot: Vector3,
}

impl Beat {
    const fn quiet(at: f32, name: &'static str) -> Beat {
        Beat {
            at,
            name,
            jolt_pos: Vector3::ZERO,
            jolt_rot: Vector3::ZERO,
        }
    }
}

/// Points the hands reach for, in the view's space, measured from the
/// weapon at rest.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Anchors {
    /// Where the support hand holds the weapon.
    pub hand_rest: Vector3,
    /// The magazine's base (where the hand grips it) at rest.
    pub mag_base: Vector3,
    /// The bolt release on the left of the receiver.
    pub release: Vector3,
    /// The charging handle.
    pub charge: Vector3,
    /// The shotgun's loading port.
    pub port: Vector3,
    /// Where fresh magazines and shells come from: low and out of view.
    pub pouch: Vector3,
}

impl Anchors {
    /// Anchors for a weapon whose support hand rests at `hand_rest` and whose
    /// magazine (if any) sits at `mag_rest`. `charge` overrides the charging
    /// handle's guessed place (a `ChargingHandle` marker on the model).
    pub fn new(
        kind: ClipKind,
        hand_rest: Vector3,
        mag_rest: Option<Vector3>,
        charge: Option<Vector3>,
    ) -> Anchors {
        // Without a magazine part, the magazine's base is under the grip area
        // (a pistol's grip, a rifle's well).
        let mag_rest = mag_rest.unwrap_or(match kind {
            ClipKind::Pistol => Vector3::new(0.0, -0.1, 0.06),
            _ => Vector3::new(0.0, -0.08, -0.06),
        });
        let mag_base = mag_rest + Vector3::new(0.0, -MAG_BASE_DROP, 0.0);
        let default_charge = match kind {
            // Behind the rear sight, on top.
            ClipKind::Rifle { charge: false, .. } => Vector3::new(0.0, -0.015, 0.07),
            // A side handle on the right of the receiver.
            _ => Vector3::new(0.035, -0.035, -0.04),
        };
        Anchors {
            hand_rest,
            mag_base,
            release: Vector3::new(-0.035, -0.035, mag_rest.z + 0.03),
            charge: charge.unwrap_or(default_charge),
            port: hand_rest + Vector3::new(0.0, -0.015, 0.13),
            pouch: mag_base + Vector3::new(-0.09, -0.3, 0.14),
        }
    }
}

impl Anchors {
    /// The magazine well: where the magazine meets the weapon.
    pub fn well(&self) -> Vector3 {
        self.mag_base + Vector3::new(0.0, MAG_BASE_DROP, 0.0)
    }
}

/// How far an AK-pattern magazine rocks about its front lip, and a drum.
const MAG_ROCK: f32 = 16.0;
const DRUM_ROCK: f32 = 10.0;
/// How much harder a drum's beats jolt the weapon than a box magazine's.
const DRUM_HEFT: f32 = 1.6;

/// How much further from the eye an AK-pattern rifle is held for its
/// magazine change than an AR (metres, view space).
const AK_CHANGE_OUT: Vector3 = Vector3::new(-0.01, 0.0, -0.12);

/// How much of the magazine-change tilt a rifle gives up for the bolt
/// release, and how far it moves away from the eye (metres, view space).
const RELEASE_EASE: f32 = 0.45;
const RELEASE_AWAY: Vector3 = Vector3::new(0.01, -0.01, -0.035);

/// How far the magazine's base (where the hand grips it) sits below the well.
const MAG_BASE_DROP: f32 = 0.04;

/// How far a charging handle is drawn back to charge the weapon, in metres.
pub const HANDLE_PULL: f32 = 0.06;
/// How long a released charging handle or bolt takes to slam home, in
/// seconds.
const SLAM: f32 = 0.035;

/// What a clip does to the weapon at one moment.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClipPose {
    pub weapon: Pose,
    /// The magazine in the weapon (or falling away), as an offset from its
    /// rest position; `None` hides it.
    pub old_mag: Option<Vector3>,
    /// The fresh magazine on its way in, as an offset from the magazine's rest
    /// position; `None` hides it.
    pub fresh_mag: Option<Vector3>,
    /// Where the support hand is, or `None` for its rest grip.
    pub hand: Option<Vector3>,
    /// A bolt being worked: (lift 0..1, draw back 0..1).
    pub bolt: Option<(f32, f32)>,
    /// 0..1: how far the firing hand has gone to the bolt handle.
    pub bolt_hand: f32,
    /// A pistol slide held back (1) or let go (0).
    pub slide: Option<f32>,
    /// A pump drawn back, 0..1.
    pub pump: Option<f32>,
    /// The point on the weapon (view space) the clip turns it about, such as
    /// the magazine well, so the work stays where the eye can see it
    /// whatever the weapon's hip pivot; `None` turns about the pivot.
    pub focus: Option<Vector3>,
    /// A shotgun shell in the support hand on its way to the loading port
    /// (view space); `None` hides it.
    pub shell: Option<Vector3>,
    /// The charging handle drawn back, in metres (0 home).
    pub handle: f32,
    /// The bolt carrier held back by the clip, 0..1 of its travel: an AR's
    /// bolt locked open until the release. `None` leaves it to the shots.
    pub carrier: Option<f32>,
    /// 0..1: the magazine release pressed.
    pub mag_release: f32,
    /// Radians the old and the fresh magazine are rocked about their front
    /// lip, base forward: an AK-pattern magazine rocks out, and hooks its
    /// front lip in the well and rocks back to lock.
    pub old_mag_rock: f32,
    pub fresh_mag_rock: f32,
}

impl ClipPose {
    /// No clip: everything at rest, the magazine in the weapon.
    pub fn rest() -> ClipPose {
        ClipPose {
            old_mag: Some(Vector3::ZERO),
            ..ClipPose::default()
        }
    }
}

/// Direction a magazine leaves the well: down and a little forward.
pub fn mag_dir() -> Vector3 {
    Vector3::new(0.0, -1.0, -0.25).normalized()
}

/// Where the support hand is: one of the anchors, or a magazine it holds.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Hand {
    Rest,
    OldMag,
    /// Where the hand lets the old magazine go.
    LetGo,
    /// Slid back along the handguard (a re-grip).
    Regrip,
    FreshMag,
    Pouch,
    Release,
    /// On the charging handle, pulled back by this far (metres).
    Charge(f32),
    Port(f32),
}

/// A magazine reload, built from one weapon's timings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MagazineReload {
    pub kind: ClipKind,
    pub empty: bool,
    /// Seconds: the whole reload, and when the fresh magazine seats.
    pub total: f32,
    pub mag_in: f32,
    /// When the weapon is back on the shoulder and may fire
    /// (`WeaponDef::reload_ready_at`).
    pub ready: f32,
}

/// The moments of a magazine reload, in seconds.
#[derive(Clone, Copy, Debug)]
struct Times {
    tilt: f32,
    out: f32,
    free: f32,
    grab: f32,
    entry: f32,
    seat: f32,
    /// The finishing action on an empty reload (bolt release, charge, slide
    /// drop, or the start of the bolt cycle), and its end.
    finish: f32,
    finish_end: f32,
    /// The weapon back on the shoulder.
    done: f32,
}

impl MagazineReload {
    pub fn new(def: &WeaponDef, empty: bool) -> Option<MagazineReload> {
        let total = def.reload.magazine_time(empty)?;
        let mag_in = def.reload.mag_in()?;
        let ready = def.reload_ready_at(empty)?;
        Some(MagazineReload {
            kind: ClipKind::of(def),
            empty,
            total,
            mag_in,
            ready,
        })
    }

    fn times(&self) -> Times {
        // Hand travel takes real time whatever the weapon: a heavy box
        // magazine is slower to fetch and spends longer lining up.
        let seat = self.mag_in;
        let heavy = matches!(self.kind, ClipKind::Rifle { heavy: true, .. });
        let out = (seat * 0.3).clamp(0.18, 0.42);
        let tilt = (out * 0.75).min(0.3);
        let free = out + 0.12;
        let grab = (free + if heavy { 0.5 } else { 0.35 }).min(seat - 0.35);
        let entry = seat - if heavy { 0.3 } else { 0.18 };
        let (finish, finish_end) = match (self.empty, self.kind) {
            (true, ClipKind::BoltAction) => (seat + 0.3, self.total - 0.25),
            (true, ClipKind::Rifle { charge: true, .. }) => {
                let on = seat + 0.3;
                (on, on + 0.16)
            }
            (true, _) => {
                // The hand comes off the seated magazine to the release.
                let at = seat + 0.32;
                (at, at + 0.06)
            }
            (false, _) => (seat + 0.1, seat + 0.1),
        };
        Times {
            tilt,
            out,
            free,
            grab,
            entry,
            seat,
            finish,
            finish_end,
            done: self.total,
        }
    }

    /// The beats, in order, with their foley events.
    pub fn beats(&self) -> Vec<Beat> {
        let t = self.times();
        // A drum is heavy: it seats with a heave the whole weapon feels.
        let heft = match self.kind {
            ClipKind::Rifle { heavy: true, .. } => DRUM_HEFT,
            _ => 1.0,
        };
        let mut beats = vec![
            Beat {
                at: t.out,
                name: "mag_out",
                jolt_pos: Vector3::new(0.0, -0.004, 0.0) * heft,
                jolt_rot: Vector3::new(-0.8, 0.0, 2.5),
            },
            Beat {
                at: t.seat,
                name: "mag_in",
                jolt_pos: Vector3::new(0.0, 0.007, 0.0) * heft,
                jolt_rot: Vector3::new(-2.2, 0.0, -1.0) * heft,
            },
        ];
        if self.empty {
            match self.kind {
                ClipKind::Rifle { charge: true, .. } => beats.push(Beat {
                    at: t.finish_end,
                    name: "charge",
                    jolt_pos: Vector3::new(0.0, 0.0, 0.006),
                    jolt_rot: Vector3::new(1.2, -0.8, 1.5),
                }),
                ClipKind::BoltAction => {
                    let span = t.finish_end - t.finish;
                    for (p, name) in [
                        (0.24, "bolt_up"),
                        (0.42, "bolt_back"),
                        (0.66, "bolt_forward"),
                        (0.84, "bolt_down"),
                    ] {
                        beats.push(Beat::quiet(t.finish + span * p, name));
                    }
                    if let Some(last) = beats.last_mut() {
                        last.jolt_rot = Vector3::new(-0.8, 0.0, -1.2);
                    }
                }
                _ => beats.push(Beat {
                    at: t.finish,
                    name: "bolt_release",
                    jolt_pos: Vector3::new(0.0, 0.0, 0.005),
                    jolt_rot: Vector3::new(1.0, 0.0, -1.5),
                }),
            }
        }
        beats
    }

    /// The tilted pose held while the magazine is changed, and the pose for
    /// the finishing action.
    fn poses(&self) -> (Pose, Pose, Pose) {
        match self.kind {
            // Turned about the magazine well (`ClipPose::focus`): lifted
            // into the lower middle of the frame, muzzle up and right, and
            // canted clockwise, so the well faces the eye while the support
            // hand works.
            ClipKind::Rifle { heavy, charge } => {
                let k = if heavy { 1.1 } else { 1.0 };
                // An AK-pattern rifle's deep receiver and long magazine fill
                // the frame from an AR's distance, with the well below its
                // corner: held further out, the receiver shrinks toward the
                // centre and the magazine work rises into view.
                let out = if charge { AK_CHANGE_OUT } else { Vector3::ZERO };
                let held = Pose::new(8.0, -10.0, -34.0, -0.05, 0.12, 0.03).scaled(k);
                let watch = Pose::new(10.0, -13.0, -40.0, -0.055, 0.135, 0.025).scaled(k);
                (
                    Pose {
                        pos: held.pos + out,
                        ..held
                    },
                    Pose {
                        pos: watch.pos + out,
                        ..watch
                    },
                    Pose::new(5.0, 8.0, 14.0, -0.02, 0.06, 0.02).scaled(k),
                )
            }
            ClipKind::Pistol => (
                Pose::new(16.0, 0.0, 16.0, -0.04, 0.06, 0.04),
                Pose::new(12.0, -4.0, 22.0, -0.045, 0.065, 0.04),
                Pose::new(6.0, 6.0, -6.0, -0.02, 0.05, 0.035),
            ),
            // A bolt action's magazine change rolls it clockwise like a
            // rifle's, the magazine toward the eye, and turns the muzzle
            // right and away: rolled the other way, its scope's eyepiece
            // swung into the eye and hid the magazine.
            ClipKind::BoltAction => (
                Pose::new(8.0, -12.0, -30.0, -0.04, 0.1, -0.02),
                Pose::new(10.0, -14.0, -34.0, -0.045, 0.11, -0.025),
                Pose::new(4.0, 6.0, 12.0, -0.02, 0.045, 0.025),
            ),
            ClipKind::Shotgun => (
                Pose::new(6.0, -4.0, 20.0, -0.045, 0.05, 0.025),
                Pose::new(8.0, -6.0, 24.0, -0.05, 0.055, 0.025),
                Pose::new(4.0, 6.0, 12.0, -0.02, 0.045, 0.025),
            ),
        }
    }

    pub fn pose(&self, t: f32, anchors: &Anchors) -> ClipPose {
        let times = self.times();
        let Times {
            tilt,
            out,
            free,
            grab,
            entry,
            seat,
            finish,
            finish_end,
            done,
        } = times;
        let (held, watch, action) = self.poses();

        // The whole weapon.
        let reshoulder = if self.empty {
            finish_end + 0.04
        } else {
            seat + 0.12
        };
        // Back on the shoulder fast (most of the way within about 0.3 s),
        // then a slow settle into the grip until the weapon can fire.
        // It is back exactly when the rules let it fire (`reload_ready_at`).
        let back = self.ready.clamp(reshoulder + 0.05, done);
        let settle = (done - 0.1).max(back + 0.04);
        let overshoot = held.scaled(-0.05);
        // While the magazine is pulled the weapon rolls toward the hand, and it
        // drifts while the fresh one is fetched.
        let pull = Pose {
            rot: held.rot + Vector3::new(1.0, 0.0, -3.5),
            pos: held.pos + Vector3::new(0.0, -0.004, 0.0),
        };
        let drift = Pose {
            rot: held.lerp(watch, 0.5).rot + Vector3::new(0.8, 1.2, 1.5),
            pos: held.lerp(watch, 0.5).pos,
        };
        let mut keys = vec![
            (0.0, Pose::ZERO),
            (tilt, held),
            ((out - 0.03).max(tilt + 0.01), held),
            (out + 0.08, pull),
            (grab, drift),
            (entry, watch),
            (seat, watch),
        ];
        let works_action = matches!(
            self.kind,
            ClipKind::Rifle { charge: true, .. } | ClipKind::BoltAction
        );
        if self.empty && works_action {
            // Turned over to reach the charging handle or the bolt.
            let reach = (finish - 0.02).max(seat + 0.05);
            keys.push((reach, action.lerp(watch, 0.4)));
            keys.push((finish_end, action));
        } else if self.empty && matches!(self.kind, ClipKind::Rifle { .. }) {
            // A bolt release: once the magazine seats, the rifle eases down
            // out of the magazine-change tilt and away from the eye while the
            // hand comes up to slap the catch, so the forearm stays low in the
            // frame instead of passing in front of the eye.
            let slap = Pose {
                rot: watch.rot * (1.0 - RELEASE_EASE),
                pos: watch.pos * (1.0 - RELEASE_EASE) + RELEASE_AWAY,
            };
            keys.push(((seat + 0.1).min(finish - 0.1), watch));
            keys.push((finish, slap));
        } else if self.empty {
            // A slide drop: held while the hand slaps it.
            keys.push((finish, watch));
        }
        let last = keys.last().map_or(watch, |k| k.1);
        keys.push((reshoulder, last));
        if back + 0.08 <= done {
            // Back on the shoulder, then a slow settle into the grip that
            // firing may cut short.
            keys.push((back, last.scaled(0.12)));
            keys.push((settle, overshoot));
        }
        keys.push((done, Pose::ZERO));
        let weapon = posed(&keys, t);

        // The magazine falling away, and the fresh one.
        let dir = mag_dir();
        let old_mag = if t < out - 0.08 {
            Some(Vector3::ZERO)
        } else if t < out {
            Some(dir * 0.012 * smoothstep((t - out + 0.08) / 0.08))
        } else if t < free {
            Some(dir * (0.012 + 0.09 * smoothstep((t - out) / (free - out).max(1e-3))))
        } else if t < free + 0.45 {
            // Let go: it drops out of view.
            let fall = t - free;
            Some(dir * 0.102 + Vector3::new(-0.15 * fall, -4.9 * fall * fall, 0.05 * fall))
        } else if t >= seat {
            Some(Vector3::ZERO)
        } else {
            None
        };
        // An AK-pattern magazine rocks: base forward about its front lip to
        // come out, and in on its front lip, then back to lock. A heavy drum
        // rocks less and seats with a heave.
        let rocks = matches!(self.kind, ClipKind::Rifle { charge: true, .. });
        let rock = match self.kind {
            ClipKind::Rifle { heavy: true, .. } => DRUM_ROCK,
            _ => MAG_ROCK,
        }
        .to_radians();
        let old_mag_rock = if rocks {
            rock * keyed(&[(out - 0.14, 0.0), (out, 1.0)], t)
        } else {
            0.0
        };
        let fresh_mag_rock = if rocks {
            let hooked = entry + (seat - entry) * 0.45;
            rock * keyed(&[(hooked, 1.0), (seat, 0.0)], t)
        } else {
            0.0
        };
        let pouch = anchors.pouch - anchors.mag_base;
        let fresh_mag = if t >= grab && t < seat {
            // Up from the pouch in about 0.35 s, lined up under the well
            // (wobbling a little while it is aligned), then pushed home
            // accelerating into the seat.
            let hover = dir * 0.1;
            let rise_end = (grab + 0.35).min(entry);
            Some(if t < rise_end {
                pouch.lerp(hover, smoothstep((t - grab) / (rise_end - grab).max(1e-3)))
            } else if t < entry {
                let w = (t - rise_end) * 9.0;
                hover + Vector3::new(0.004 * w.sin(), 0.003 * (w * 1.3).cos() - 0.003, 0.0)
            } else {
                let push = ((t - entry) / (seat - entry).max(1e-3)).clamp(0.0, 1.0);
                hover * (1.0 - push * push)
            })
        } else {
            None
        };

        // The support hand.
        let mut hand_keys = vec![
            (0.0, Hand::Rest),
            ((out - 0.16).max(0.05), Hand::OldMag),
            (free - 0.02, Hand::OldMag),
            (free, Hand::LetGo),
            (grab, Hand::Pouch),
            (grab + 0.01, Hand::FreshMag),
            (seat + 0.06, Hand::FreshMag),
        ];
        let back_at = if self.empty {
            match self.kind {
                ClipKind::Rifle { charge: true, .. } => {
                    hand_keys.push((finish, Hand::Charge(0.0)));
                    hand_keys.push((finish_end, Hand::Charge(HANDLE_PULL)));
                    finish_end + 0.22
                }
                ClipKind::Rifle { charge: false, .. } => {
                    hand_keys.push((finish - 0.02, Hand::Release));
                    hand_keys.push((finish + 0.06, Hand::Release));
                    finish + 0.3
                }
                // The firing hand works the slide or the bolt; the support
                // hand goes back to the weapon.
                _ => (seat + 0.3).min(finish + 0.1),
            }
        } else {
            (seat + 0.3).min(reshoulder + 0.15)
        };
        hand_keys.push((back_at.min(done - 0.02), Hand::Rest));
        let hand = self.hand_at(&hand_keys, t, anchors, old_mag, fresh_mag);

        let working = self.kind == ClipKind::BoltAction && self.empty;
        let p = (t - finish) / (finish_end - finish).max(1e-3);
        let bolt = (working && (0.0..=1.0).contains(&p))
            .then(|| (keyed(&BOLT_LIFT, p), keyed(&BOLT_BACK, p)));
        // The firing hand reaches the handle just before the bolt lifts.
        let bolt_hand = if working {
            keyed(&[(-0.15, 0.0), (0.04, 1.0), (0.88, 1.0), (1.05, 0.0)], p)
        } else {
            0.0
        };
        let slide = (self.kind == ClipKind::Pistol && self.empty).then(|| {
            if t < finish {
                1.0
            } else {
                (1.0 - (t - finish) / 0.04).max(0.0)
            }
        });
        // An empty reload's charge: the handle drawn back with the hand, then
        // let go to slam home.
        let handle = match self.kind {
            ClipKind::Rifle { charge: true, .. } if self.empty => keyed(
                &[
                    (finish, 0.0),
                    (finish_end, HANDLE_PULL),
                    (finish_end + SLAM, 0.0),
                ],
                t,
            ),
            _ => 0.0,
        };
        // An AR's bolt stays locked open until the release is slapped.
        let carrier = (matches!(self.kind, ClipKind::Rifle { charge: false, .. }) && self.empty)
            .then(|| 1.0 - ((t - finish) / SLAM).clamp(0.0, 1.0));
        // The release pressed as the magazine is pulled.
        let mag_release = match self.kind {
            ClipKind::Rifle { .. } | ClipKind::Pistol => keyed(
                &[
                    (out - 0.14, 0.0),
                    (out - 0.08, 1.0),
                    (out + 0.02, 1.0),
                    (out + 0.08, 0.0),
                ],
                t,
            ),
            _ => 0.0,
        };
        ClipPose {
            weapon,
            old_mag,
            fresh_mag,
            hand,
            bolt,
            bolt_hand,
            slide,
            pump: None,
            focus: Some(anchors.well()),
            shell: None,
            handle,
            carrier,
            mag_release,
            old_mag_rock,
            fresh_mag_rock,
        }
    }

    fn hand_at(
        &self,
        keys: &[(f32, Hand)],
        t: f32,
        anchors: &Anchors,
        old_mag: Option<Vector3>,
        fresh_mag: Option<Vector3>,
    ) -> Option<Vector3> {
        let place = |hand: Hand| -> Vector3 {
            match hand {
                Hand::Rest => anchors.hand_rest,
                Hand::OldMag => anchors.mag_base + old_mag.unwrap_or(mag_dir() * 0.102),
                Hand::LetGo => anchors.mag_base + mag_dir() * 0.102,
                Hand::Regrip => anchors.hand_rest + Vector3::new(0.0, -0.015, 0.035),
                Hand::FreshMag => anchors.mag_base + fresh_mag.unwrap_or(Vector3::ZERO),
                Hand::Pouch => anchors.pouch,
                Hand::Release => anchors.release,
                Hand::Charge(pull) => anchors.charge + Vector3::new(0.0, 0.0, pull),
                Hand::Port(push) => anchors.port + Vector3::new(0.0, 0.0, -push),
            }
        };
        hand_position(keys, t, place)
    }
}

/// The hand's position along `keys` at `t`, or `None` at its rest grip.
fn hand_position(keys: &[(f32, Hand)], t: f32, place: impl Fn(Hand) -> Vector3) -> Option<Vector3> {
    let first = keys.first()?;
    if t <= first.0 {
        return (first.1 != Hand::Rest).then(|| place(first.1));
    }
    for pair in keys.windows(2) {
        let ((t0, h0), (t1, h1)) = (pair[0], pair[1]);
        if t <= t1 {
            if h0 == Hand::Rest && h1 == Hand::Rest {
                return None;
            }
            let u = smoothstep((t - t0) / (t1 - t0).max(1e-4));
            return Some(place(h0).lerp(place(h1), u));
        }
    }
    let last = keys.last()?;
    (last.1 != Hand::Rest).then(|| place(last.1))
}

/// A bolt: lift, draw back, push forward, and turn down, over a cycle.
pub const BOLT_LIFT: [(f32, f32); 4] = [(0.08, 0.0), (0.24, 1.0), (0.7, 1.0), (0.84, 0.0)];
pub const BOLT_BACK: [(f32, f32); 4] = [(0.24, 0.0), (0.42, 1.0), (0.5, 1.0), (0.66, 0.0)];
/// A pump: back, held, and forward, over a cycle.
pub const PUMP_BACK: [(f32, f32); 4] = [(0.12, 0.0), (0.42, 1.0), (0.52, 1.0), (0.82, 0.0)];

/// The beats of a bolt or pump cycle, as fractions of the cycle, on the
/// contacts of `PUMP_BACK` and `BOLT_LIFT`/`BOLT_BACK`: the pump hits its
/// back stop and slams forward; the bolt lifts, draws, and closes.
pub fn cycle_beats(kind: ClipKind) -> &'static [Beat] {
    const PUMP: [Beat; 2] = [
        Beat {
            at: 0.42,
            name: "pump_back",
            jolt_pos: Vector3::new(0.0, 0.0, 0.004),
            jolt_rot: Vector3::new(0.8, 0.0, 0.4),
        },
        Beat {
            at: 0.82,
            name: "pump_forward",
            jolt_pos: Vector3::new(0.0, 0.0, -0.006),
            jolt_rot: Vector3::new(-1.0, 0.0, -0.5),
        },
    ];
    const BOLT: [Beat; 4] = [
        Beat::quiet(0.24, "bolt_up"),
        Beat {
            at: 0.42,
            name: "bolt_back",
            jolt_pos: Vector3::new(0.0, 0.0, 0.003),
            jolt_rot: Vector3::new(0.4, 0.0, 0.6),
        },
        Beat {
            at: 0.66,
            name: "bolt_forward",
            jolt_pos: Vector3::new(0.0, 0.0, -0.004),
            jolt_rot: Vector3::new(-0.5, 0.0, -0.4),
        },
        Beat {
            at: 0.84,
            name: "bolt_down",
            jolt_pos: Vector3::ZERO,
            jolt_rot: Vector3::new(-0.6, 0.0, -0.8),
        },
    ];
    // A self-loading weapon's charge after a cut-off empty reload.
    const CHARGE: [Beat; 1] = [Beat {
        at: 0.55,
        name: "charge",
        jolt_pos: Vector3::new(0.0, 0.0, 0.005),
        jolt_rot: Vector3::new(1.0, -0.6, 1.2),
    }];
    match kind {
        ClipKind::Shotgun => &PUMP,
        ClipKind::BoltAction => &BOLT,
        _ => &CHARGE,
    }
}

/// A shell-by-shell reload at one stage: `elapsed` seconds into a stage of
/// `length` seconds. `began_empty` adds a pump at the end.
pub fn shotgun_pose(
    stage: LoadStage,
    elapsed: f32,
    length: f32,
    began_empty: bool,
    anchors: &Anchors,
) -> ClipPose {
    let p = (elapsed / length.max(1e-3)).clamp(0.0, 1.0);
    // Rolled to show the loading port, muzzle up a little.
    let load = Pose::new(10.0, -6.0, 32.0, -0.05, 0.055, 0.025);
    let mut pose = ClipPose {
        focus: Some(anchors.port),
        ..ClipPose::rest()
    };
    match stage {
        LoadStage::Opening => {
            pose.weapon = Pose::ZERO.lerp(load, smoothstep(p / 0.8));
            pose.hand = hand_position(
                &[(0.0, Hand::Rest), (0.75, Hand::Pouch), (1.0, Hand::Pouch)],
                p,
                |h| shotgun_place(h, anchors),
            );
        }
        LoadStage::Round => {
            // Each shell bobs the weapon a touch as it is thumbed in.
            let thumb = (std::f32::consts::PI * p).sin();
            pose.weapon = load.lerp(
                Pose::new(11.5, -6.0, 33.5, -0.05, 0.052, 0.025),
                thumb * 0.5,
            );
            pose.hand = hand_position(
                &[
                    (0.0, Hand::Pouch),
                    (0.5, Hand::Port(0.0)),
                    (0.66, Hand::Port(0.035)),
                    (0.74, Hand::Port(0.035)),
                    (1.0, Hand::Pouch),
                ],
                p,
                |h| shotgun_place(h, anchors),
            );
            // The shell rides in the fingertips until the thumb pushes it in.
            if p < SHELL_IN {
                pose.shell = pose.hand.map(|h| h + SHELL_IN_FINGERS);
            }
        }
        LoadStage::Closing => {
            let pump = if began_empty {
                keyed(&PUMP_BACK, p)
            } else {
                0.0
            };
            pose.pump = began_empty.then_some(pump);
            let to = if began_empty { 0.85 } else { 1.0 };
            pose.weapon = load.lerp(Pose::ZERO, smoothstep(p / to));
            pose.hand = hand_position(&[(0.0, Hand::Pouch), (0.4, Hand::Rest)], p, |h| {
                shotgun_place(h, anchors)
            })
            .map(|h| h + Vector3::new(0.0, 0.0, pump * 0.075));
        }
    }
    pose
}

/// Fraction of a shell stage when the shell is pushed into the port.
const SHELL_IN: f32 = 0.66;
/// Where a held shell sits from the support hand's grip point: in the
/// fingertips, just ahead of the palm.
const SHELL_IN_FINGERS: Vector3 = Vector3::new(0.0, 0.01, -0.02);

fn shotgun_place(hand: Hand, anchors: &Anchors) -> Vector3 {
    match hand {
        Hand::Port(push) => anchors.port + Vector3::new(0.0, 0.0, -push),
        Hand::Pouch => anchors.port + Vector3::new(-0.05, -0.22, 0.12),
        _ => anchors.hand_rest,
    }
}

/// The beats of a shotgun stage, as fractions of the stage.
pub fn shotgun_beats(stage: LoadStage, began_empty: bool) -> &'static [(f32, &'static str)] {
    match (stage, began_empty) {
        (LoadStage::Round, _) => &[(SHELL_IN, "shell_in")],
        (LoadStage::Closing, true) => &[(0.27, "pump_back"), (0.67, "pump_forward")],
        _ => &[],
    }
}

/// A draw over the raise (fractions of the raise time): up from low, rolled
/// in, a slight overshoot as it settles on the shoulder.
pub fn draw_keys(kind: ClipKind) -> &'static [(f32, Pose)] {
    const RIFLE: [(f32, Pose); 4] = [
        (0.0, LONG_GUN_DOWN),
        (0.6, Pose::new(5.0, 2.0, -5.0, 0.0, 0.012, 0.0)),
        (0.82, Pose::new(-1.5, -0.5, 1.5, 0.0, -0.003, 0.0)),
        (1.0, Pose::ZERO),
    ];
    // A sidearm comes up from low centre, quick and straight.
    const PISTOL: [(f32, Pose); 4] = [
        (0.0, PISTOL_DOWN),
        (0.55, Pose::new(4.0, 0.0, -2.0, 0.0, 0.01, 0.0)),
        (0.8, Pose::new(-1.0, 0.0, 0.5, 0.0, -0.002, 0.0)),
        (1.0, Pose::ZERO),
    ];
    // Long guns swing up heavier, with more overshoot.
    const HEAVY: [(f32, Pose); 4] = [
        (0.0, LONG_GUN_DOWN),
        (0.62, Pose::new(6.0, 2.5, -6.0, 0.0, 0.016, 0.0)),
        (0.84, Pose::new(-2.0, -0.6, 2.0, 0.0, -0.004, 0.0)),
        (1.0, Pose::ZERO),
    ];
    match kind {
        ClipKind::Pistol => &PISTOL,
        ClipKind::Rifle { heavy: false, .. } => &RIFLE,
        _ => &HEAVY,
    }
}

/// Where a weapon is when fully put away: below the frame. A holster ends
/// and a draw starts here, so a switch never pops between the two models.
const LONG_GUN_DOWN: Pose = Pose::new(-45.0, 6.0, -10.0, 0.02, -0.3, 0.04);
const PISTOL_DOWN: Pose = Pose::new(-45.0, 0.0, 6.0, -0.02, -0.3, 0.04);

/// A holster over the lower (fractions of the lower time).
pub fn holster_keys(kind: ClipKind) -> &'static [(f32, Pose)] {
    const RIFLE: [(f32, Pose); 3] = [
        (0.0, Pose::ZERO),
        (0.35, Pose::new(-6.0, 4.0, -8.0, -0.01, -0.03, 0.0)),
        (1.0, LONG_GUN_DOWN),
    ];
    const PISTOL: [(f32, Pose); 3] = [
        (0.0, Pose::ZERO),
        (0.35, Pose::new(-8.0, 0.0, 3.0, 0.0, -0.03, 0.01)),
        (1.0, PISTOL_DOWN),
    ];
    match kind {
        ClipKind::Pistol => &PISTOL,
        _ => &RIFLE,
    }
}

/// The pose at `t` from fraction keys.
/// The weapon during a knife strike, `t` seconds in with the strike landing
/// at `strike`: the firing hand swings it down and out to the right, out of
/// the knife's way, and brings it back up over the recovery.
pub fn melee_pose(t: f32, strike: f32) -> Pose {
    // Dropped down and right, rolled out of the knife's way. A steep pitch
    // here swung the stock and the firing forearm up toward the lens.
    let away = Pose::new(-6.0, -10.0, 24.0, 0.05, -0.085, 0.02);
    let keys = [
        (0.0, Pose::ZERO),
        (strike * 0.8, away),
        (strike + 0.12, away.scaled(0.95)),
        (strike + shooter_sim::melee::RECOVERY, Pose::ZERO),
    ];
    posed(&keys, t)
}

pub fn pose_at(keys: &[(f32, Pose)], t: f32) -> Pose {
    posed(keys, t)
}

/// A weapon inspect, in seconds: turn the left side up to the eye and check
/// the magazine with a tap, roll over to look at the top and ease the
/// charging handle back a touch to check the chamber, and settle back.
pub const INSPECT_LENGTH: f32 = 3.2;

pub fn inspect_pose(t: f32, kind: ClipKind, anchors: &Anchors) -> ClipPose {
    let keys = [
        (0.0, Pose::ZERO),
        (0.4, Pose::new(8.0, 42.0, -30.0, -0.05, 0.03, 0.0)),
        (1.3, Pose::new(10.0, 46.0, -32.0, -0.05, 0.035, 0.0)),
        (1.75, Pose::new(12.0, -10.0, 20.0, 0.015, 0.01, 0.0)),
        (2.6, Pose::new(11.0, -11.0, 21.0, 0.015, 0.01, 0.0)),
        (3.0, Pose::new(-4.0, 2.0, -3.0, 0.0, -0.005, 0.0)),
        (INSPECT_LENGTH, Pose::ZERO),
    ];
    // Held poses drift a little (no frozen holds), weighted in between the
    // turns.
    let hold = smoothstep((t - 0.3) / 0.3) * (1.0 - smoothstep((t - 2.7) / 0.3));
    let mut weapon = posed(&keys, t);
    weapon.rot += Vector3::new(
        0.6 * (t * 1.7).sin(),
        0.8 * (t * 1.3 + 0.5).sin(),
        0.5 * (t * 2.1).sin(),
    ) * hold;
    // The magazine check: pulled a finger's width and slapped home.
    let tug = keyed(&[(0.55, 0.0), (0.75, 0.022), (0.9, 0.022), (1.0, 0.0)], t);
    let has_mag = matches!(kind, ClipKind::Rifle { .. } | ClipKind::Pistol);
    let old_mag = Some(if has_mag {
        mag_dir() * tug
    } else {
        Vector3::ZERO
    });
    // The chamber check: the handle eased back and let forward.
    let pull = keyed(&[(1.95, 0.0), (2.15, 0.025), (2.35, 0.025), (2.45, 0.0)], t);
    let place = |hand: Hand| match hand {
        Hand::Rest => anchors.hand_rest,
        Hand::OldMag => anchors.mag_base + mag_dir() * tug,
        Hand::Charge(p) => anchors.charge + Vector3::new(0.0, 0.0, p + pull),
        Hand::Regrip => anchors.hand_rest + Vector3::new(0.0, -0.015, 0.035),
        _ => anchors.hand_rest,
    };
    let mut hand_keys = vec![(0.0, Hand::Rest)];
    if has_mag {
        hand_keys.extend([
            (0.5, Hand::OldMag),
            (1.05, Hand::OldMag),
            (1.35, Hand::Rest),
        ]);
    }
    // A re-grip as the weapon rolls over: the hand slides back and forward.
    hand_keys.extend([(1.45, Hand::Regrip), (1.62, Hand::Rest)]);
    if kind != ClipKind::Shotgun {
        hand_keys.extend([
            (1.8, Hand::Rest),
            (2.0, Hand::Charge(0.0)),
            (2.45, Hand::Charge(0.0)),
            (2.7, Hand::Rest),
        ]);
    }
    // The release held while the magazine is tugged.
    let mag_release = if has_mag {
        keyed(&[(0.5, 0.0), (0.56, 1.0), (0.9, 1.0), (0.96, 0.0)], t)
    } else {
        0.0
    };
    ClipPose {
        weapon,
        old_mag,
        hand: hand_position(&hand_keys, t, place),
        focus: Some(anchors.well()),
        handle: if kind == ClipKind::Shotgun { 0.0 } else { pull },
        mag_release,
        ..ClipPose::rest()
    }
}

/// A self-loading weapon charged outside a reload (after an empty reload
/// was cut off by a switch), `p` 0..1 through the cycle: the support hand
/// goes to the handle, draws it back, and lets it slam home on the
/// `charge` beat of `cycle_beats`.
pub fn charge_pose(p: f32, anchors: &Anchors) -> ClipPose {
    let handle = keyed(&[(0.25, 0.0), (0.5, HANDLE_PULL), (0.5 + SLAM, 0.0)], p);
    let hand = hand_position(
        &[
            (0.05, Hand::Rest),
            (0.25, Hand::Charge(0.0)),
            (0.5, Hand::Charge(HANDLE_PULL)),
            (0.8, Hand::Rest),
        ],
        p,
        |h| match h {
            Hand::Charge(pull) => anchors.charge + Vector3::new(0.0, 0.0, pull),
            _ => anchors.hand_rest,
        },
    );
    ClipPose {
        hand,
        handle,
        ..ClipPose::rest()
    }
}

/// A pistol's slide racked by the support hand (a first raise), `p` 0..1
/// through the work: the hand comes over the top, pinches the slide's rear,
/// draws it fully back, and lets it slam home on the `charge` beat of
/// `cycle_beats`.
pub fn slide_rack_pose(p: f32, anchors: &Anchors) -> ClipPose {
    let slide = keyed(&[(0.25, 0.0), (0.5, 1.0), (0.5 + SLAM, 0.0)], p);
    // The slide's rear serrations, just behind the rear sight (the origin).
    let rear = Vector3::new(-0.006, -0.008, 0.014);
    let hand = hand_position(
        &[
            (0.05, Hand::Rest),
            (0.25, Hand::Charge(0.0)),
            (0.5, Hand::Charge(PISTOL_SLIDE_TRAVEL)),
            (0.62, Hand::Charge(PISTOL_SLIDE_TRAVEL + 0.03)),
            (0.85, Hand::Rest),
        ],
        p,
        |h| match h {
            Hand::Charge(pull) => rear + Vector3::new(-0.01, 0.015, pull),
            _ => anchors.hand_rest,
        },
    );
    ClipPose {
        hand,
        slide: Some(slide),
        ..ClipPose::rest()
    }
}

/// How far a pistol's slide runs back, in metres (as the view draws it).
const PISTOL_SLIDE_TRAVEL: f32 = 0.028;

/// How a first raise (Call of Duty's `firstRaiseAnim`) turns the weapon
/// while the action is worked, `p` 0..1 through the work: tipped toward the
/// working hand and rolled so the action faces the eye, held, and back onto
/// the shoulder. A bolt or pump gun's own cycle motion does the rest.
pub fn first_raise_present(kind: ClipKind, p: f32) -> Pose {
    let shown = keyed(&[(0.0, 0.0), (0.2, 1.0), (0.75, 1.0), (1.0, 0.0)], p);
    let pose = match kind {
        // Rolled left, toward the support hand reaching over the top.
        ClipKind::Rifle { .. } => Pose::new(4.0, 8.0, -6.0, -0.015, 0.015, -0.03),
        // Turned in and up, the slide toward the support hand.
        ClipKind::Pistol => Pose::new(8.0, 10.0, 16.0, -0.03, 0.025, 0.02),
        // Rolled right, the bolt or the ejection port up toward the eye.
        ClipKind::BoltAction => Pose::new(4.0, 3.0, -10.0, -0.01, 0.015, 0.01),
        ClipKind::Shotgun => Pose::new(4.0, 4.0, -8.0, -0.015, 0.015, 0.01),
    };
    pose.scaled(shown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shooter_sim::weapon::WeaponId;

    #[test]
    fn a_racked_slide_runs_back_and_slams_home() {
        let a = anchors(ClipKind::Pistol);
        assert_eq!(slide_rack_pose(0.0, &a).slide, Some(0.0));
        assert_eq!(slide_rack_pose(0.5, &a).slide, Some(1.0));
        assert_eq!(slide_rack_pose(0.6, &a).slide, Some(0.0));
        assert!(slide_rack_pose(0.4, &a).hand.is_some());
        assert!(slide_rack_pose(1.0, &a).hand.is_none(), "back on the grip");
    }

    #[test]
    fn a_first_raise_shows_the_action_and_settles() {
        for kind in [
            ClipKind::Rifle {
                charge: true,
                heavy: false,
            },
            ClipKind::Pistol,
            ClipKind::BoltAction,
            ClipKind::Shotgun,
        ] {
            assert_eq!(first_raise_present(kind, 0.0), Pose::ZERO);
            assert!(first_raise_present(kind, 0.5).rot.z.abs() > 5.0);
            assert_eq!(first_raise_present(kind, 1.0), Pose::ZERO);
        }
    }

    fn reload(key: &str, empty: bool) -> MagazineReload {
        MagazineReload::new(WeaponId::expect(key).def(), empty).expect("a magazine weapon")
    }

    fn anchors(kind: ClipKind) -> Anchors {
        Anchors::new(
            kind,
            Vector3::new(0.0, -0.06, -0.2),
            Some(Vector3::new(0.0, -0.08, -0.06)),
            None,
        )
    }

    #[test]
    fn ak_magazines_rock_out_and_hook_in_on_their_front_lip() {
        for key in ["ak47", "lsw6"] {
            let r = reload(key, false);
            let a = anchors(r.kind);
            let t = r.times();
            assert_eq!(r.pose(0.0, &a).old_mag_rock, 0.0, "{key}");
            assert!(r.pose(t.out, &a).old_mag_rock > 0.1, "{key}: rocked out");
            // Fetched rocked, hooked on its lip, then rocked back to lock.
            assert!(r.pose(t.entry, &a).fresh_mag_rock > 0.1, "{key}");
            assert_eq!(r.pose(t.seat, &a).fresh_mag_rock, 0.0, "{key}: locked");
        }
        let lsw = reload("lsw6", false);
        let ak = reload("ak47", false);
        let a = anchors(ak.kind);
        assert!(
            lsw.pose(lsw.times().out, &a).old_mag_rock < ak.pose(ak.times().out, &a).old_mag_rock,
            "a drum rocks less"
        );
        // An AR's magazine goes straight in.
        let m4 = reload("m4", true);
        let a = anchors(m4.kind);
        for i in 0..40 {
            let p = m4.pose(m4.total * i as f32 / 40.0, &a);
            assert_eq!((p.old_mag_rock, p.fresh_mag_rock), (0.0, 0.0));
        }
    }

    #[test]
    fn a_drum_seats_with_a_heavier_jolt() {
        let seat = |key: &str| {
            reload(key, false)
                .beats()
                .into_iter()
                .find(|b| b.name == "mag_in")
                .expect("a seat")
        };
        assert!(seat("lsw6").jolt_rot.length() > seat("ak47").jolt_rot.length() * 1.4);
    }

    #[test]
    fn an_ar_eases_down_and_away_for_the_bolt_release() {
        let r = reload("m4", true);
        let a = anchors(r.kind);
        let t = r.times();
        let held = r.pose(t.seat, &a).weapon;
        let slap = r.pose(t.finish, &a).weapon;
        assert!(slap.rot.length() < held.rot.length() * 0.7, "less tilt");
        assert!(slap.pos.z < held.pos.z - 0.02, "further from the eye");
        assert!(slap.pos.y < held.pos.y, "lower");
    }

    #[test]
    fn a_bolt_action_rolls_its_magazine_toward_the_eye_like_a_rifle() {
        let r = reload("m40", true);
        let a = anchors(r.kind);
        let held = r.pose(r.times().entry, &a).weapon;
        let rifle = reload("m4", false);
        let rifle_held = rifle.pose(rifle.times().entry, &anchors(rifle.kind)).weapon;
        // Clockwise (negative roll), as a rifle: the scope swings away from
        // the eye and the magazine turns toward it.
        assert!(held.rot.z < -20.0, "{held:?}");
        assert_eq!(held.rot.z.signum(), rifle_held.rot.z.signum());
        // And no nearer the eye than at rest.
        assert!(held.pos.z <= 0.0);
    }

    #[test]
    fn classes_follow_the_weapon() {
        let kind = |k: &str| ClipKind::of(WeaponId::expect(k).def());
        assert_eq!(
            kind("m4"),
            ClipKind::Rifle {
                charge: false,
                heavy: false
            }
        );
        assert_eq!(
            kind("ak47"),
            ClipKind::Rifle {
                charge: true,
                heavy: false
            }
        );
        assert_eq!(
            kind("lsw6"),
            ClipKind::Rifle {
                charge: true,
                heavy: true
            }
        );
        assert_eq!(kind("m1911"), ClipKind::Pistol);
        assert_eq!(kind("m40"), ClipKind::BoltAction);
        assert_eq!(kind("m870"), ClipKind::Shotgun);
        assert_eq!(
            kind("m4+extmag"),
            ClipKind::Rifle {
                charge: false,
                heavy: false
            }
        );
    }

    #[test]
    fn the_magazine_seats_exactly_when_its_rounds_count() {
        for key in [
            "m4",
            "ak47",
            "br3",
            "lsw6",
            "pdw9",
            "m1911",
            "m40",
            "m4+extmag",
        ] {
            for empty in [false, true] {
                let clip = reload(key, empty);
                let a = anchors(clip.kind);
                let seat = clip
                    .beats()
                    .iter()
                    .find(|b| b.name == "mag_in")
                    .expect("a seat")
                    .at;
                assert!(
                    (seat - clip.mag_in).abs() < 1e-5,
                    "{key}: seat {seat} vs {}",
                    clip.mag_in
                );
                // Fully home at the seat, and on its way just before.
                let at = clip.pose(clip.mag_in, &a);
                assert_eq!(at.old_mag, Some(Vector3::ZERO), "{key}");
                let before = clip.pose(clip.mag_in - 0.05, &a);
                assert!(
                    before.fresh_mag.is_some_and(|m| m.length() > 0.001),
                    "{key}"
                );
                // The old magazine is gone while the fresh one is fetched.
                let t = clip.times();
                assert!(
                    clip.pose((t.free + t.grab) * 0.5 + 0.25, &a)
                        .old_mag
                        .is_none()
                        || t.grab - t.free < 0.5
                );
            }
        }
    }

    #[test]
    fn beats_come_in_order_and_finish_before_the_weapon_can_fire() {
        for key in ["m4", "ak47", "br3", "lsw6", "pdw9", "m1911", "m40"] {
            for empty in [false, true] {
                let clip = reload(key, empty);
                let beats = clip.beats();
                let names: Vec<_> = beats.iter().map(|b| b.name).collect();
                assert_eq!(names[0], "mag_out", "{key}");
                assert_eq!(names[1], "mag_in", "{key}");
                assert!(
                    beats.windows(2).all(|w| w[0].at < w[1].at),
                    "{key} {names:?}"
                );
                assert!(beats.last().unwrap().at < clip.total - 0.1, "{key} {empty}");
                if empty {
                    let finish = match clip.kind {
                        ClipKind::Rifle { charge: true, .. } => "charge",
                        ClipKind::BoltAction => "bolt_down",
                        _ => "bolt_release",
                    };
                    assert!(names.contains(&finish), "{key}: {names:?}");
                } else {
                    assert_eq!(names.len(), 2, "{key}: {names:?}");
                }
            }
        }
    }

    #[test]
    fn a_reload_starts_and_ends_on_the_shoulder_without_a_jump() {
        for key in ["m4", "lsw6", "m1911", "m40"] {
            for empty in [false, true] {
                let clip = reload(key, empty);
                let a = anchors(clip.kind);
                assert_eq!(clip.pose(0.0, &a).weapon, Pose::ZERO);
                let end = clip.pose(clip.total, &a);
                assert!(
                    end.weapon.rot.length() < 1e-3 && end.hand.is_none(),
                    "{key}"
                );
                // No frame-to-frame jump anywhere (60 FPS).
                let mut last = clip.pose(0.0, &a);
                let mut t = 0.0;
                while t < clip.total {
                    t += 1.0 / 60.0;
                    let now = clip.pose(t, &a);
                    let step = (now.weapon.rot - last.weapon.rot).length();
                    assert!(
                        step < 4.0,
                        "{key} {empty}: {step} degrees in a frame at {t}"
                    );
                    if let (Some(h0), Some(h1)) = (last.hand, now.hand) {
                        assert!(
                            (h1 - h0).length() < 0.06,
                            "{key} {empty}: hand jumped at {t}"
                        );
                    }
                    last = now;
                }
            }
        }
    }

    #[test]
    fn the_action_parts_follow_the_reload_beats() {
        let beat = |clip: &MagazineReload, name: &str| {
            clip.beats().iter().find(|b| b.name == name).unwrap().at
        };
        // An AR's bolt stays locked open until the release is slapped, then
        // slams home.
        let m4 = reload("m4", true);
        let a = anchors(m4.kind);
        let release = beat(&m4, "bolt_release");
        assert_eq!(m4.pose(0.1, &a).carrier, Some(1.0));
        assert_eq!(m4.pose(release - 0.01, &a).carrier, Some(1.0));
        assert_eq!(m4.pose(release + 0.05, &a).carrier, Some(0.0));
        assert_eq!(reload("m4", false).pose(0.5, &a).carrier, None);
        // A charged rifle's handle is fully back on the charge beat's eve and
        // home just after it, and still the rest of the time.
        let ak = reload("ak47", true);
        let a = anchors(ak.kind);
        let charge = beat(&ak, "charge");
        assert!((ak.pose(charge, &a).handle - HANDLE_PULL).abs() < 1e-4);
        assert_eq!(ak.pose(charge + 0.05, &a).handle, 0.0);
        assert_eq!(ak.pose(0.5, &a).handle, 0.0);
        assert_eq!(reload("ak47", false).pose(charge, &a).handle, 0.0);
        // The magazine release is pressed as the magazine comes out, and let go.
        for key in ["m4", "ak47", "m1911"] {
            let clip = reload(key, false);
            let a = anchors(clip.kind);
            let out = beat(&clip, "mag_out");
            assert_eq!(clip.pose(out, &a).mag_release, 1.0, "{key}");
            assert_eq!(clip.pose(out + 0.3, &a).mag_release, 0.0, "{key}");
            assert_eq!(clip.pose(0.0, &a).mag_release, 0.0, "{key}");
        }
    }

    #[test]
    fn a_charge_outside_a_reload_draws_the_handle_with_the_hand() {
        let kind = ClipKind::of(WeaponId::expect("ak47").def());
        let a = anchors(kind);
        let pulled = charge_pose(0.5, &a);
        assert!((pulled.handle - HANDLE_PULL).abs() < 1e-4);
        let hand = pulled.hand.expect("the hand is on the handle");
        assert!((hand - (a.charge + Vector3::new(0.0, 0.0, HANDLE_PULL))).length() < 1e-3);
        // Home on the `charge` beat of the cycle, hand back at rest by the end.
        let beat = cycle_beats(kind)[0].at;
        assert_eq!(charge_pose(beat, &a).handle, 0.0);
        assert_eq!(charge_pose(1.0, &a).hand, None);
        assert_eq!(charge_pose(0.0, &a).handle, 0.0);
    }

    #[test]
    fn an_empty_reload_finishes_with_its_action() {
        let m4 = reload("m4", true);
        let a = anchors(m4.kind);
        let release = m4
            .beats()
            .iter()
            .find(|b| b.name == "bolt_release")
            .unwrap()
            .at;
        let hand = m4.pose(release, &a).hand.unwrap();
        assert!(
            (hand - a.release).length() < 0.01,
            "the hand slaps the release"
        );
        let pistol = reload("m1911", true);
        let drop = pistol
            .beats()
            .iter()
            .find(|b| b.name == "bolt_release")
            .unwrap()
            .at;
        let a = anchors(pistol.kind);
        assert_eq!(
            pistol.pose(drop - 0.1, &a).slide,
            Some(1.0),
            "locked back until the drop"
        );
        assert_eq!(pistol.pose(drop + 0.1, &a).slide, Some(0.0));
        let sniper = reload("m40", true);
        let a = anchors(sniper.kind);
        let back = sniper
            .beats()
            .iter()
            .find(|b| b.name == "bolt_back")
            .unwrap()
            .at;
        let (_, drawn) = sniper.pose(back, &a).bolt.unwrap();
        assert!(drawn > 0.9, "the bolt is drawn at its beat: {drawn}");
    }

    #[test]
    fn shotgun_stages_load_a_shell_and_pump_after_an_empty_reload() {
        let a = anchors(ClipKind::Shotgun);
        let thumbed = shotgun_pose(LoadStage::Round, 0.33, 0.5, false, &a)
            .hand
            .unwrap();
        assert!((thumbed - (a.port + Vector3::new(0.0, 0.0, -0.035))).length() < 0.01);
        let closing = shotgun_pose(LoadStage::Closing, 0.2, 0.45, true, &a);
        assert!(closing.pump.unwrap() > 0.9);
        assert!(
            shotgun_pose(LoadStage::Closing, 0.2, 0.45, false, &a)
                .pump
                .is_none()
        );
        assert_eq!(
            shotgun_beats(LoadStage::Round, false),
            &[(0.66, "shell_in")]
        );
    }

    #[test]
    fn inspect_checks_the_magazine_and_the_chamber() {
        let a = anchors(ClipKind::Rifle {
            charge: false,
            heavy: false,
        });
        let kind = ClipKind::Rifle {
            charge: false,
            heavy: false,
        };
        let tug = inspect_pose(0.8, kind, &a);
        assert!(tug.old_mag.unwrap().length() > 0.02);
        assert!(tug.hand.is_some());
        let check = inspect_pose(2.25, kind, &a);
        assert!((check.hand.unwrap() - a.charge).z > 0.02);
        assert_eq!(inspect_pose(INSPECT_LENGTH, kind, &a).weapon, Pose::ZERO);
    }
}
