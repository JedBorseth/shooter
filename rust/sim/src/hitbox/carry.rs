//! How a soldier carries their weapon: where it sits relative to the eyes at
//! the hip, aimed, sprinting, and reloading, how the aim pitch spreads up the
//! spine, and how the hands hold it.
//!
//! The third-person character (`rust/game/src/character`) poses its skeleton
//! from these numbers, and [`super::hitboxes`] poses the arms the same way, so
//! the arm zones sit where the arms are drawn. Change them here and both move.
//!
//! Spaces: the aim frame and body space have +X right, +Y up, -Z forward (along
//! the aim). Weapon space has its origin at the rear sight, -Z toward the
//! muzzle. A hand bone's space has +Y toward the knuckles and +Z out of the palm.

use glam::{EulerRot, Mat3, Quat, Vec2, Vec3};

use crate::weapon::{Attachment, Reload, Slot, WeaponDef};

/// Where the eyes are in the head bone's space (+Y up the head, +Z forward).
pub const EYE_IN_HEAD: Vec3 = Vec3::new(0.0, 0.15, 0.09);
/// Share of the aim pitch each bone takes, from the spine up: spine, chest,
/// upper chest, neck, head. Sums to 1, so the head looks along the aim.
pub const PITCH_SHARES: [f32; 5] = [0.12, 0.18, 0.25, 0.2, 0.25];
/// The body bends no further than this (radians) to aim up or down.
pub const MAX_PITCH: f32 = 1.3;
/// Sliding on the back, the body bends this much of the aim pitch.
pub const SLIDE_PITCH_SCALE: f32 = 0.4;

/// Weapon offsets from the eyes, in the aim frame.
pub const ADS_OFFSET: Vec3 = Vec3::new(0.034, -0.045, -0.12);
pub const HIP_OFFSET: Vec3 = Vec3::new(0.085, -0.13, -0.14);
pub const SPRINT_OFFSET: Vec3 = Vec3::new(0.03, -0.3, -0.16);
/// Degrees: pitch, yaw, roll.
pub const HIP_ROTATION: Vec3 = Vec3::new(0.0, 3.0, -6.0);
pub const SPRINT_ROTATION: Vec3 = Vec3::new(-42.0, 38.0, -28.0);
/// Where the support hand reaches during a reload, in weapon space (a pouch on
/// the vest, below and behind the magazine well).
pub const RELOAD_REACH: Vec3 = Vec3::new(-0.12, -0.32, 0.12);
/// Where a pistol's support hand fetches a magazine (a belt pouch), and a
/// pump gun's shells (a side pouch), in weapon space.
pub const PISTOL_POUCH: Vec3 = Vec3::new(-0.1, -0.4, 0.3);
pub const SHELL_POUCH: Vec3 = Vec3::new(-0.14, -0.22, 0.14);
/// Radians a soldier holding a long gun turns the upper chest about the spine,
/// support shoulder forward, as a rifle stance is bladed toward the target:
/// the support arm reaches further up the handguard and the stock comes out
/// of the chest onto the firing shoulder. The neck turns back as much, so the
/// head still faces the aim. Pistols are held square.
pub const BLADE: f32 = 0.26;
/// How far (m) the support hand sits behind the weapon's `GripLeft` marker on
/// a rifle handguard, so the support elbow bends and drops under the weapon.
pub const SUPPORT_BACK: f32 = 0.02;
/// Where a vertical foregrip moves the support hand from the weapon's
/// `GripLeft` marker: down onto the grip under the handguard (the drawn
/// weapon moves its marker the same way, `viewmodel.rs`).
pub const FOREGRIP_SHIFT: Vec3 = Vec3::new(0.0, -0.045, 0.004);
/// How far the support palm's centre sits off the surface it rests on: the
/// glove and the flesh of the palm.
pub const PALM_SKIN: f32 = 0.019;
/// Where the grip sits in each hand's bone space (from the wrist).
pub const RIGHT_PALM: Vec3 = Vec3::new(0.0, 0.085, 0.035);
pub const LEFT_PALM: Vec3 = Vec3::new(0.0, 0.07, 0.05);
/// Elbow hints in weapon space: the firing elbow down and out, the support
/// elbow down and in under the weapon.
pub const RIGHT_POLE: Vec3 = Vec3::new(0.45, -0.7, 0.35);
pub const LEFT_POLE: Vec3 = Vec3::new(-0.25, -0.8, 0.0);
/// Reload progress the hit zones assume: the support hand at the pouch (from
/// 0.25 to 0.6 of the reload) and the weapon dipped about halfway between the
/// least and most it does there. Snapshots only say a player is reloading,
/// not how far along.
pub const MID_RELOAD: f32 = 0.35;

/// How a class of weapon is carried and reloaded in third person.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CarryClass {
    /// Rifles and SMGs: a box magazine under the receiver.
    #[default]
    Rifle,
    /// Light machine guns: held lower and heavier, a big box to change.
    Heavy,
    /// Bolt-action rifles fed from a magazine.
    Bolt,
    /// Sidearms: held out in both hands, carried muzzle down in a sprint,
    /// the magazine in the grip.
    Pistol,
    /// Pump guns: shells one at a time through the loading port.
    Shotgun,
}

impl CarryClass {
    pub fn of(def: &WeaponDef) -> Self {
        match def.reload {
            Reload::PerRound { .. } => CarryClass::Shotgun,
            _ if def.slot == Slot::Secondary => CarryClass::Pistol,
            _ if def.cycle_time > 0.0 => CarryClass::Bolt,
            _ if def.magazine > 50 => CarryClass::Heavy,
            _ => CarryClass::Rifle,
        }
    }

    /// The weapon offsets from the eyes for this class (aim frame): at the
    /// hip, aimed, and sprinting; and the hip and sprint rotations (degrees:
    /// pitch, yaw, roll).
    fn offsets(self) -> (Vec3, Vec3, Vec3, Vec3, Vec3) {
        match self {
            // Out in front in both hands, sights at arm's length; in a sprint
            // the arm hangs and the muzzle points at the ground.
            CarryClass::Pistol => (
                Vec3::new(0.06, -0.16, -0.3),
                Vec3::new(0.02, -0.04, -0.34),
                Vec3::new(0.2, -0.55, -0.1),
                Vec3::new(-4.0, 2.0, -4.0),
                Vec3::new(-75.0, 5.0, 0.0),
            ),
            // Low on the hip, taking its weight.
            CarryClass::Heavy => (
                HIP_OFFSET + Vec3::new(0.01, -0.05, 0.02),
                ADS_OFFSET,
                SPRINT_OFFSET + Vec3::new(0.0, -0.03, 0.0),
                HIP_ROTATION + Vec3::new(-3.0, 1.0, -3.0),
                SPRINT_ROTATION,
            ),
            _ => (
                HIP_OFFSET,
                ADS_OFFSET,
                SPRINT_OFFSET,
                HIP_ROTATION,
                SPRINT_ROTATION,
            ),
        }
    }

    /// The tilt mid-reload, in degrees (pitch, roll), and a lift (m): rifles
    /// roll the magazine well toward the support hand, pistols tip up, pump
    /// guns roll the loading port to it.
    fn reload_tilt(self) -> (f32, f32, f32) {
        match self {
            CarryClass::Pistol => (18.0, 12.0, 0.04),
            CarryClass::Shotgun => (10.0, 40.0, 0.0),
            CarryClass::Heavy => (8.0, 22.0, -0.02),
            _ => (12.0, 30.0, 0.0),
        }
    }
}

/// What the support hand holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Hold {
    /// A rifle's handguard, from below.
    #[default]
    Handguard,
    /// A shotgun's pump.
    Pump,
    /// One-handed weapons: the support hand cups the firing hand.
    Pistol,
}

impl Hold {
    /// The hold for a weapon: sidearms are pistols, multi-pellet guns pumps.
    pub fn of(def: &WeaponDef) -> Self {
        if def.slot == Slot::Secondary {
            Hold::Pistol
        } else if def.pellets > 1 {
            Hold::Pump
        } else {
            Hold::Handguard
        }
    }
}

/// Where the hands hold a weapon, in weapon space: the weapon file's `grips`,
/// which match the `GripRight` and `GripLeft` markers of its scene.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grips {
    /// Centre of the firing hand's grip.
    pub right: Vec3,
    /// Where the support hand holds: the handguard, or the firing hand for pistols.
    pub left: Vec3,
    /// Pistol grip rake in degrees: how far the grip leans back from vertical.
    pub rake: f32,
    pub hold: Hold,
    /// The handguard's or pump's cross-section where the support hand holds
    /// it, across the barrel at `left`: the support palm rests on its
    /// surface ([`Grips::support_contact`]).
    pub section: Option<Section>,
}

/// A weapon's cross-section across the barrel (weapon space): its centre
/// (x, y) and half its width and height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Section {
    pub centre: Vec2,
    pub half: Vec2,
}

impl Default for Grips {
    fn default() -> Self {
        Self {
            right: Vec3::new(0.0, -0.15, 0.0),
            left: Vec3::new(0.0, -0.06, -0.28),
            rake: 22.0,
            hold: Hold::Handguard,
            section: None,
        }
    }
}

impl Grips {
    /// A weapon's grips as a third-person soldier holds them: the support hand
    /// moves back along a rifle's handguard by [`SUPPORT_BACK`], with its palm
    /// on the handguard's section, or down onto a vertical foregrip.
    pub fn held(def: &WeaponDef) -> Self {
        let mut grips = def.grips.map_or_else(Grips::default, |g| Grips {
            right: Vec3::from(g.right),
            left: Vec3::from(g.left),
            rake: g.rake,
            hold: Hold::Handguard,
            section: g.section.map(|s| Section {
                centre: Vec2::from(s.centre),
                half: Vec2::from(s.half),
            }),
        });
        grips.hold = Hold::of(def);
        match grips.hold {
            Hold::Handguard if def.attachment == Some(Attachment::Grip) => {
                // A fist round the foregrip, not a palm on the handguard.
                grips.left += FOREGRIP_SHIFT;
                grips.section = None;
            }
            Hold::Handguard => grips.left.z += SUPPORT_BACK,
            Hold::Pump => {}
            Hold::Pistol => grips.section = None,
        }
        grips
    }

    /// Where the support hand's grip centre goes (weapon space) and where
    /// that centre sits in the hand (hand space), for the hand's basis in
    /// weapon space. On a handguard or pump with a section, the centre is the
    /// section's and the palm rests on its surface: the section's reach
    /// along the palm's normal (an ellipse's support) plus [`PALM_SKIN`].
    /// Otherwise the marker, held [`LEFT_PALM`] from the wrist.
    pub fn support_contact(&self, hand: Mat3) -> (Vec3, Vec3) {
        match (self.hold, self.section) {
            (Hold::Pistol, _) | (_, None) => (self.left, LEFT_PALM),
            (_, Some(section)) => section_contact(section, self.left.z, hand),
        }
    }

    /// The hand bones' bases in weapon space: right, then left. Measured from
    /// the animation library's two-handed pistol pose, then raked for rifle grips.
    pub fn hand_bases(&self) -> (Mat3, Mat3) {
        let rake = Mat3::from_axis_angle(Vec3::X, -self.rake.to_radians());
        let right =
            rake * basis_from_y_z(Vec3::new(-0.02, 0.19, -0.98), Vec3::new(-1.0, 0.04, 0.03));
        let left = if self.hold == Hold::Pistol {
            basis_from_y_z(Vec3::new(0.31, 0.11, -0.95), Vec3::new(0.65, 0.7, 0.29))
        } else {
            // Under the handguard: palm up and in, knuckles forward and to the right.
            basis_from_y_z(Vec3::new(0.25, 0.05, -0.97), Vec3::new(0.35, 0.94, 0.0))
        };
        (right, left)
    }
}

/// The support palm on a section cut across the barrel at `z`, for a hand
/// with basis `hand` (weapon space): the section's centre, and that centre
/// in the hand's space (see [`Grips::support_contact`]).
pub fn section_contact(section: Section, z: f32, hand: Mat3) -> (Vec3, Vec3) {
    // The palm's normal (the hand's +Z) in weapon space, and how far the
    // section reaches along it.
    let n = hand.z_axis;
    let half = section.half;
    let reach = ((n.x * half.x).powi(2) + (n.y * half.y).powi(2)).sqrt();
    (
        section.centre.extend(z),
        Vec3::new(LEFT_PALM.x, LEFT_PALM.y, reach + PALM_SKIN),
    )
}

/// The support hand's basis in weapon space while it fetches a magazine: the
/// palm turned toward the body.
pub fn fetch_basis() -> Mat3 {
    basis_from_y_z(Vec3::new(0.2, 0.3, -0.9), Vec3::new(0.8, 0.4, 0.0))
}

/// The axis the upper chest blades about (body space): up, tilted with the
/// aim pitch as far as the spine has bent it below the neck.
pub fn blade_axis(pitch: f32) -> Vec3 {
    let share: f32 = PITCH_SHARES[..3].iter().sum();
    Mat3::from_axis_angle(Vec3::X, pitch * share) * Vec3::Y
}

/// A basis whose +Y is `y` and whose +Z is as close to `z` as possible.
pub fn basis_from_y_z(y: Vec3, z: Vec3) -> Mat3 {
    let y = y.try_normalize().unwrap_or(Vec3::Y);
    let mut z = z - y * z.dot(y);
    if z.length_squared() < 1e-8 {
        z = if y.x.abs() < 0.9 { Vec3::X } else { Vec3::Z }.cross(y);
    }
    let z = z.normalize();
    Mat3::from_cols(y.cross(z), y, z)
}

/// Where the support hand takes a box magazine and seats the fresh one, from
/// the firing hand's grip (weapon space).
const MAGAZINE_WELL: Vec3 = Vec3::new(-0.01, -0.03, -0.12);
/// Where it pushes a pistol's magazine home, at the grip's base.
const PISTOL_WELL: Vec3 = Vec3::new(0.0, -0.1, 0.01);
/// A pistol's magazine drops from this share of the reload.
const PISTOL_DROP_START: f32 = 0.04;
/// On an empty reload a box magazine drops at this share of the time to
/// the seat: as the support hand reaches the well.
const EMPTY_DROP: f32 = 0.3;

/// Where a weapon's magazine is ([`Carry::magazine_pose`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MagazinePose {
    /// In the weapon.
    Seated,
    /// Out of sight: dropped, or in a pouch.
    Out,
    /// Moved this far from where it sits (weapon space).
    At(Vec3),
}

/// The weapon's carry state, each part 0..1 unless noted.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Carry {
    /// Aim down sights (raw; eased here, and cancelled by sprinting).
    pub ads: f32,
    /// Sprint carry.
    pub sprint: f32,
    /// How far into a reload the body is, and the reload's progress.
    pub reload: f32,
    pub reload_progress: f32,
    /// Firing kick: x pitch (radians), z push back (metres).
    pub kick: Vec3,
    /// The weapon dropping out of view during a switch.
    pub switch_drop: f32,
    /// A knife swing: the gun drops most of the way to the sprint carry.
    /// Hit zones leave it at zero (snapshots say who is swinging, not how
    /// far along, and the swing is over in half a second).
    pub swing: f32,
    /// The weapon's class, and the share of its reload at which the fresh
    /// magazine seats (or, for a pump gun, the shells' timing).
    pub class: CarryClass,
    pub timing: ReloadTiming,
}

/// When a reload's beats fall, as shares of the reload's progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReloadTiming {
    /// The fresh magazine seats at this share.
    Magazine { seat: f32 },
    /// Shells go in from `start` for `count` shells of `each`.
    Shells { start: f32, each: f32, count: u32 },
}

impl Default for ReloadTiming {
    fn default() -> Self {
        ReloadTiming::Magazine { seat: 0.6 }
    }
}

impl ReloadTiming {
    /// A weapon's reload beats, as shares of the time the hit zones' clock
    /// gives its reload (`clock::reload_seconds`: a tactical reload, or a
    /// pump gun's start, four shells, and end).
    pub fn of(def: &WeaponDef) -> Self {
        let total = super::clock::reload_seconds_of(def);
        match def.reload {
            Reload::Magazine { .. } => ReloadTiming::Magazine {
                seat: (def.reload.mag_in().unwrap_or(total * 0.6) / total).clamp(0.3, 0.92),
            },
            Reload::PerRound {
                start, per_round, ..
            } => ReloadTiming::Shells {
                start: start / total,
                each: per_round / total,
                count: 4,
            },
        }
    }
}

impl Carry {
    /// The weapon's rear sight offset from the eyes and its rotation, both in
    /// the aim frame.
    pub fn weapon_offset(&self) -> (Vec3, Mat3) {
        let (hip_offset, ads_offset, sprint_offset, hip_rotation, sprint_rotation) =
            self.class.offsets();
        let (tilt_pitch, tilt_roll, tilt_lift) = self.class.reload_tilt();
        let ads = smoothstep(self.ads) * (1.0 - self.sprint);
        let sprint = smoothstep(self.sprint);
        let dip = self.dip();
        let mut offset = hip_offset.lerp(ads_offset, ads);
        offset += (sprint_offset - hip_offset) * (sprint + 0.6 * self.swing).min(1.0);
        offset += Vec3::new(
            -0.04 * dip,
            (tilt_lift - 0.04) * dip - 0.25 * self.switch_drop,
            0.03 * dip,
        );
        offset.z += self.kick.z;
        let deg = |v: f32| v.to_radians();
        let hip = 1.0 - ads;
        let carry = sprint + 0.5 * self.swing;
        let pitch = deg(hip_rotation.x) * hip
            + deg(sprint_rotation.x) * carry
            + self.kick.x
            + deg(tilt_pitch) * dip
            - deg(40.0) * self.switch_drop;
        let yaw = deg(hip_rotation.y) * hip + deg(sprint_rotation.y) * carry;
        let roll =
            deg(hip_rotation.z) * hip + deg(sprint_rotation.z) * carry + deg(tilt_roll) * dip;
        (offset, Mat3::from_euler(EulerRot::YXZ, yaw, pitch, roll))
    }

    /// How far the upper chest turns, support shoulder forward (radians,
    /// [`BLADE`]): squared up in a sprint, and never with a pistol.
    pub fn blade(&self) -> f32 {
        if self.class == CarryClass::Pistol {
            return 0.0;
        }
        BLADE * (1.0 - smoothstep(self.sprint))
    }

    /// The weapon dips and rolls toward the support hand mid-reload.
    fn dip(&self) -> f32 {
        crate::fmath::sin(std::f32::consts::PI * self.reload_progress).max(0.0) * self.reload
    }

    /// How far the support hand has let go of the weapon to fetch a magazine.
    pub fn support_off(&self) -> f32 {
        self.support_path(&Grips::default()).1
    }

    /// Where the support hand is during a reload (weapon space) and how far
    /// it has let go of its grip (0..1), for a weapon held by `grips`:
    ///
    /// - a box magazine: to the magazine, down with it to a vest pouch, back
    ///   up with a fresh one, seated as the reload counts its rounds, and back
    ///   to the handguard;
    /// - a pistol: down to the belt as the empty magazine drops, back up to
    ///   the grip's base, and back round the firing hand;
    /// - a pump gun: from the pouch to the loading port and back for each
    ///   shell.
    pub fn support_path(&self, grips: &Grips) -> (Vec3, f32) {
        let p = self.reload_progress;
        let ease = |a: Vec3, b: Vec3, from: f32, to: f32| {
            a.lerp(b, smoothstep((p - from) / (to - from).max(1e-3)))
        };
        let (target, off) = match (self.class, self.timing) {
            (CarryClass::Shotgun, ReloadTiming::Shells { start, each, count }) => {
                let port = grips.right + Vec3::new(-0.01, -0.06, -0.11);
                let end = start + each * count as f32;
                // Each shell: from the pouch to the port (the first 60% of
                // its time), pushed in, and back.
                let phase = ((p - start) / each.max(1e-3)).rem_euclid(1.0);
                let target = if phase < 0.6 {
                    ease(SHELL_POUCH, port, start, start).lerp(port, smoothstep(phase / 0.6))
                } else {
                    port.lerp(SHELL_POUCH, smoothstep((phase - 0.6) / 0.4))
                };
                let off =
                    smoothstep((p - (start - 0.05)) / 0.08) * (1.0 - smoothstep((p - end) / 0.08));
                (target, off)
            }
            (CarryClass::Pistol, ReloadTiming::Magazine { seat }) => {
                let well = grips.right + PISTOL_WELL;
                let target = ease(PISTOL_POUCH, well, seat * 0.55, seat * 0.92);
                let off =
                    smoothstep((p - 0.04) / (seat * 0.25)) * (1.0 - smoothstep((p - seat) / 0.12));
                (target, off)
            }
            (_, ReloadTiming::Magazine { seat }) => {
                let well = grips.right + MAGAZINE_WELL;
                let below = well + Vec3::new(0.0, -0.07, 0.02);
                let target = if p < seat * 0.42 {
                    well
                } else if p < seat * 0.7 {
                    ease(well, RELOAD_REACH, seat * 0.42, seat * 0.58)
                } else {
                    ease(RELOAD_REACH, below, seat * 0.7, seat * 0.88)
                        .lerp(well, smoothstep((p - seat * 0.88) / (seat * 0.12)))
                };
                let off = smoothstep((p - 0.04) / (seat * 0.22).max(0.02))
                    * (1.0 - smoothstep((p - seat) / 0.12));
                (target, off)
            }
            _ => (RELOAD_REACH, 0.0),
        };
        (target, off * self.reload)
    }

    /// Where the weapon's magazine is during a reload, for the drawn
    /// third-person weapon (cosmetic: the hit zones do not use it). It
    /// follows the support hand's path ([`Carry::support_path`]):
    ///
    /// - a box magazine stays seated until the hand has it, rides the hand
    ///   down to the vest pouch, where the empty goes in and a fresh one
    ///   comes out, and back up into the well as it seats (a tactical
    ///   reload);
    /// - on an `empty` reload the hand reaches the well as the old magazine
    ///   drops free ([`Carry::drop_point`]), and comes back from the pouch
    ///   with the fresh one;
    /// - a pistol's drops out of the grip as the release is pressed, and a
    ///   fresh one comes up from the belt in the hand and is pushed home;
    /// - a pump gun has none to move.
    ///
    /// From the drop point the old magazine is out of the gun: the drawn
    /// weapon lets a loose copy fall where it was.
    pub fn magazine_pose(&self, grips: &Grips, empty: bool) -> MagazinePose {
        let p = self.reload_progress;
        let (target, _) = self.support_path(grips);
        let dropped = self.drop_point(empty).is_some_and(|at| p >= at);
        match (self.class, self.timing) {
            (CarryClass::Shotgun, _) | (_, ReloadTiming::Shells { .. }) => MagazinePose::Seated,
            (CarryClass::Pistol, ReloadTiming::Magazine { seat }) => {
                let well = grips.right + PISTOL_WELL;
                if !dropped || p >= seat {
                    MagazinePose::Seated
                } else if p < seat * 0.55 {
                    MagazinePose::Out
                } else {
                    MagazinePose::At(target - well)
                }
            }
            (_, ReloadTiming::Magazine { seat }) => {
                let held = if dropped { seat * 0.6 } else { seat * 0.42 };
                if p >= seat || (!dropped && p < held) {
                    MagazinePose::Seated
                } else if p < held {
                    MagazinePose::Out
                } else {
                    MagazinePose::At(target - (grips.right + MAGAZINE_WELL))
                }
            }
        }
    }

    /// The share of the reload at which the old magazine drops free of the
    /// gun, if it does: a pistol's always, as the release is pressed; a box
    /// magazine's on an `empty` reload, as the support hand reaches the
    /// well (a tactical reload keeps it and stows it in the pouch).
    pub fn drop_point(&self, empty: bool) -> Option<f32> {
        match (self.class, self.timing) {
            (CarryClass::Shotgun, _) | (_, ReloadTiming::Shells { .. }) => None,
            (CarryClass::Pistol, ReloadTiming::Magazine { .. }) => Some(PISTOL_DROP_START),
            (_, ReloadTiming::Magazine { seat }) => empty.then_some(seat * EMPTY_DROP),
        }
    }

    /// The trigger finger comes off the trigger while sprinting or reloading.
    pub fn trigger_off(&self) -> f32 {
        self.sprint.max(self.reload)
    }
}

/// The weapon's transform in body space: the aim frame turned up by `pitch`
/// about the eyes, then the carry offset and rotation.
pub fn weapon_transform(eyes: Vec3, pitch: f32, carry: &Carry) -> (Mat3, Vec3) {
    let (offset, rotation) = carry.weapon_offset();
    let aim = Mat3::from_axis_angle(Vec3::X, pitch);
    (aim * rotation, eyes + aim * offset)
}

/// Where the hands go, in body space, for a weapon at `weapon`: each wrist's
/// target and the hand's basis, right then left.
pub fn hand_targets(
    weapon: (Mat3, Vec3),
    grips: &Grips,
    (support_target, support_off): (Vec3, f32),
) -> [(Vec3, Mat3); 2] {
    let (basis, origin) = weapon;
    let at = |p: Vec3| origin + basis * p;
    let (right_basis, left_basis) = grips.hand_bases();
    let right_hand = basis * right_basis;
    let right_wrist = at(grips.right) - right_hand * RIGHT_PALM;
    let (contact, left_palm) = grips.support_contact(left_basis);
    let mut left_grip = at(contact);
    let mut left_hand = basis * left_basis;
    if support_off > 0.0 {
        let off = support_off.clamp(0.0, 1.0);
        left_grip = left_grip.lerp(at(support_target), off);
        let from = Quat::from_mat3(&left_hand);
        let to = Quat::from_mat3(&(basis * fetch_basis()));
        left_hand = Mat3::from_quat(from.slerp(to, off));
    }
    let left_wrist = left_grip - left_hand * left_palm;
    [(right_wrist, right_hand), (left_wrist, left_hand)]
}

pub fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapon::WeaponId;

    #[test]
    fn blades_behind_long_guns_but_not_pistols_or_in_a_sprint() {
        let rifle = Carry::default();
        assert!((rifle.blade() - BLADE).abs() < 1e-6);
        let sprinting = Carry {
            sprint: 1.0,
            ..Carry::default()
        };
        assert!(sprinting.blade().abs() < 1e-6);
        let pistol = Carry {
            class: CarryClass::Pistol,
            ..Carry::default()
        };
        assert_eq!(pistol.blade(), 0.0);
        // Level aim blades about straight up.
        assert!((blade_axis(0.0) - Vec3::Y).length() < 1e-6);
    }

    #[test]
    fn each_weapon_class_gets_its_hold() {
        let hold = |key: &str| Hold::of(WeaponId::by_key(key).expect(key).def());
        assert_eq!(hold("m1911"), Hold::Pistol);
        assert_eq!(hold("m870"), Hold::Pump);
        for key in ["m4", "ak47", "m40", "pdw9"] {
            assert_eq!(hold(key), Hold::Handguard, "{key}");
        }
    }

    #[test]
    fn aiming_raises_the_weapon_to_the_eye() {
        let hip = Carry::default().weapon_offset().0;
        let ads = Carry {
            ads: 1.0,
            ..Carry::default()
        }
        .weapon_offset()
        .0;
        assert!(ads.y > hip.y && ads.x < hip.x, "{hip} -> {ads}");
        // Sprinting cancels aiming.
        let sprint = Carry {
            ads: 1.0,
            sprint: 1.0,
            ..Carry::default()
        };
        assert!((sprint.weapon_offset().0 - SPRINT_OFFSET).length() < 1e-5);
    }

    #[test]
    fn mid_reload_the_support_hand_is_at_the_pouch() {
        let carry = Carry {
            reload: 1.0,
            reload_progress: MID_RELOAD,
            ..Carry::default()
        };
        assert!((carry.support_off() - 1.0).abs() < 1e-5);
        let weapon = (Mat3::IDENTITY, Vec3::ZERO);
        let grips = Grips::default();
        let [_, (wrist, hand)] = hand_targets(weapon, &grips, carry.support_path(&grips));
        assert!((wrist + hand * LEFT_PALM - RELOAD_REACH).length() < 1e-5);
    }

    #[test]
    fn each_class_reloads_its_own_way() {
        let carry = |key: &str, p: f32| {
            let def = WeaponId::by_key(key).expect(key).def();
            let c = Carry {
                reload: 1.0,
                reload_progress: p,
                class: CarryClass::of(def),
                timing: ReloadTiming::of(def),
                ..Carry::default()
            };
            (c, Grips::held(def))
        };
        assert_eq!(carry("m1911", 0.0).0.class, CarryClass::Pistol);
        assert_eq!(carry("m870", 0.0).0.class, CarryClass::Shotgun);
        assert_eq!(carry("m40", 0.0).0.class, CarryClass::Bolt);
        assert_eq!(carry("lsw6", 0.0).0.class, CarryClass::Heavy);
        assert_eq!(carry("m4", 0.0).0.class, CarryClass::Rifle);
        // A rifle's hand goes to the magazine first, then the pouch, then
        // seats the fresh one and lets go of nothing once done.
        let (c, g) = carry("m4", 0.0);
        let ReloadTiming::Magazine { seat } = c.timing else {
            panic!("a magazine")
        };
        let at = |p: f32| {
            Carry {
                reload_progress: p,
                ..c
            }
            .support_path(&g)
        };
        assert!(at(0.0).1 < 1e-3, "starts on the handguard");
        assert!(
            at(seat * 0.3).1 > 0.9 && at(seat * 0.3).0.y > -0.25,
            "at the magazine"
        );
        assert!(
            (at(seat * 0.62).0 - RELOAD_REACH).length() < 0.01,
            "at the pouch"
        );
        assert!(
            at(seat).0.distance(at(seat * 0.3).0) < 0.01,
            "seats it in the well"
        );
        assert!(at(seat + 0.2).1 < 1e-3, "back on the handguard");
        // A pump gun's hand goes back and forth for each shell.
        let (s, g) = carry("m870", 0.0);
        let ReloadTiming::Shells { start, each, .. } = s.timing else {
            panic!("shells")
        };
        let shell = |f: f32| {
            Carry {
                reload_progress: start + each * (1.0 + f),
                ..s
            }
            .support_path(&g)
            .0
        };
        assert!(shell(0.0).distance(SHELL_POUCH) < 0.01 && shell(0.6).distance(SHELL_POUCH) > 0.1);
    }

    #[test]
    fn pistols_are_held_out_and_carried_muzzle_down() {
        let pistol = Carry {
            class: CarryClass::Pistol,
            ..Carry::default()
        };
        assert!(
            pistol.weapon_offset().0.z < HIP_OFFSET.z - 0.1,
            "held out in front"
        );
        let sprint = Carry {
            sprint: 1.0,
            ..pistol
        };
        let (offset, rotation) = sprint.weapon_offset();
        let muzzle = rotation * Vec3::NEG_Z;
        assert!(offset.y < -0.4 && muzzle.y < -0.8, "{offset} {muzzle}");
    }

    #[test]
    fn the_support_palm_rests_on_the_handguard() {
        // Every two-handed weapon's file has the section where the third-
        // person hand holds it, under the marker's line.
        for key in ["m4", "ak47", "br3", "lsw6", "pdw9", "m40", "m870"] {
            let grips = Grips::held(WeaponId::by_key(key).expect(key).def());
            let section = grips.section.unwrap_or_else(|| panic!("{key}: no section"));
            assert!(
                section.half.min_element() > 0.01 && section.half.max_element() < 0.04,
                "{key}: {section:?}"
            );
        }
        // Palm straight up under a section: the grip is the section's centre
        // and the palm sits its half height plus the glove below it.
        let grips = Grips {
            left: Vec3::new(0.0, -0.08, -0.2),
            section: Some(Section {
                centre: Vec2::new(0.0, -0.05),
                half: Vec2::new(0.02, 0.025),
            }),
            ..Grips::default()
        };
        let up = basis_from_y_z(Vec3::NEG_Z, Vec3::Y);
        let (point, palm) = grips.support_contact(up);
        assert!((point - Vec3::new(0.0, -0.05, -0.2)).length() < 1e-5);
        assert!((palm.z - (0.025 + PALM_SKIN)).abs() < 1e-5);
        // Held in the hold's own basis (palm up and in), the arm's target
        // puts the palm's surface on the section's edge below its centre.
        let weapon = (Mat3::IDENTITY, Vec3::ZERO);
        let [_, (wrist, hand)] = hand_targets(weapon, &grips, (Vec3::ZERO, 0.0));
        let (_, palm) = grips.support_contact(hand);
        let surface = wrist + hand * Vec3::new(palm.x, palm.y, PALM_SKIN);
        let below = Vec3::new(0.0, -0.05, -0.2) - surface;
        assert!(
            (below.dot(hand.z_axis) - (palm.z - PALM_SKIN)).abs() < 1e-5
                && below.y > 0.02
                && below.y < 0.025 + 1e-4,
            "on the handguard's underside: {surface}"
        );
        // Pistols cup the firing hand whatever the section.
        let pistol = Grips {
            hold: Hold::Pistol,
            ..grips
        };
        assert_eq!(pistol.support_contact(up), (pistol.left, LEFT_PALM));
    }

    #[test]
    fn a_foregrip_takes_the_support_hand_off_the_handguard() {
        let plain = Grips::held(WeaponId::by_key("m4").expect("m4").def());
        let grip = Grips::held(WeaponId::by_key("m4+grip").expect("m4+grip").def());
        assert!(grip.section.is_none());
        assert!(
            (grip.left - (plain.left - Vec3::Z * SUPPORT_BACK + FOREGRIP_SHIFT)).length() < 1e-5
        );
    }

    #[test]
    fn bases_are_orthonormal() {
        let m = basis_from_y_z(Vec3::new(0.3, 1.0, 0.1), Vec3::new(0.0, 0.2, 1.0));
        assert!((m.determinant() - 1.0).abs() < 1e-5);
        assert!((m.y_axis - Vec3::new(0.3, 1.0, 0.1).normalize()).length() < 1e-5);
    }

    #[test]
    fn the_magazine_rides_the_support_hand_out_and_back() {
        let grips = Grips::default();
        let carry = |class: CarryClass, p: f32| Carry {
            reload: 1.0,
            reload_progress: p,
            class,
            timing: ReloadTiming::Magazine { seat: 0.6 },
            ..Carry::default()
        };
        let at = |class: CarryClass, p: f32| carry(class, p).magazine_pose(&grips, false);
        let empty = |class: CarryClass, p: f32| carry(class, p).magazine_pose(&grips, true);
        // A rifle's is seated until the hand has it and once it seats.
        assert_eq!(at(CarryClass::Rifle, 0.1), MagazinePose::Seated);
        assert_eq!(at(CarryClass::Rifle, 0.7), MagazinePose::Seated);
        // In between it goes down to the pouch with the hand and comes back.
        let MagazinePose::At(low) = at(CarryClass::Rifle, 0.4) else {
            panic!("in the hand");
        };
        assert!(low.y < -0.1 && low.z > 0.1, "toward the pouch: {low:?}");
        let MagazinePose::At(home) = at(CarryClass::Rifle, 0.599) else {
            panic!("in the hand");
        };
        assert!(home.length() < 0.01, "seating: {home:?}");
        // A tactical reload stows it; an empty one drops it as the hand
        // reaches the well, and the hand brings a fresh one from the pouch.
        assert_eq!(carry(CarryClass::Rifle, 0.0).drop_point(false), None);
        let drop = carry(CarryClass::Rifle, 0.0)
            .drop_point(true)
            .expect("drops");
        assert!(drop > 0.1 && drop < 0.6 * 0.42, "{drop}");
        assert_eq!(empty(CarryClass::Rifle, drop - 0.01), MagazinePose::Seated);
        assert_eq!(empty(CarryClass::Rifle, 0.3), MagazinePose::Out);
        let MagazinePose::At(fresh) = empty(CarryClass::Rifle, 0.4) else {
            panic!("the fresh one in the hand");
        };
        assert!(fresh.y < -0.1, "{fresh:?}");
        assert_eq!(empty(CarryClass::Rifle, 0.7), MagazinePose::Seated);
        // A pistol's drops out whichever reload, is gone, then comes up in
        // the hand.
        for empty in [false, true] {
            let pose = |p: f32| carry(CarryClass::Pistol, p).magazine_pose(&grips, empty);
            let drop = carry(CarryClass::Pistol, 0.0).drop_point(empty);
            assert_eq!(drop, Some(PISTOL_DROP_START));
            assert_eq!(pose(0.02), MagazinePose::Seated);
            assert_eq!(pose(0.2), MagazinePose::Out);
            assert!(matches!(pose(0.45), MagazinePose::At(_)));
            assert_eq!(pose(0.65), MagazinePose::Seated);
        }
        // A pump gun has none.
        let pump = Carry {
            reload: 1.0,
            reload_progress: 0.5,
            class: CarryClass::Shotgun,
            timing: ReloadTiming::Shells {
                start: 0.1,
                each: 0.2,
                count: 4,
            },
            ..Carry::default()
        };
        assert_eq!(pump.magazine_pose(&grips, true), MagazinePose::Seated);
        assert_eq!(pump.drop_point(true), None);
    }
}
