//! Mounting the weapon on cover: a ledge, a barrier's top, a sandbag wall,
//! or a windowsill, as Modern Warfare (2019) and its successors play it.
//!
//! - **Mount** by aiming down sights at cover in front of the gun and
//!   pressing melee (the reference's default "ADS + Melee"). The eye moves up
//!   or down until the gun rests on the edge: the body rises from a crouch or
//!   sinks from standing, on the one crouch blend that drives the capsule,
//!   the eye, the shot origin, and the hit zones together, so nothing
//!   disagrees with where the bullets go.
//! - **Mounted**, the body stays put, the view turns only within a cone about
//!   the way it mounted ([`YAW_LIMIT`], [`PITCH_UP`], [`PITCH_DOWN`]), the kick
//!   is cut to [`KICK_SCALE`] and the aimed sway to [`SWAY_SCALE`]. A reload
//!   keeps the mount.
//! - **Leave** by letting go of aim, moving, jumping, crouching, sprinting,
//!   pressing melee again, switching weapons, throwing, or turning past the
//!   cone (the client stops the view at the cone's edge, and lets it through
//!   when the player keeps pushing). The press that leaves does what it
//!   normally does: a crouch press crouches, a jump jumps.
//!
//! The engine finds the cover (a probe at weapon height in front of the eye,
//! `MoveEnv::mount`); everything else is decided here, so the client
//! predicts it exactly.

use glam::Vec2;

use crate::movement::{MoveEnv, MoveParams, MoveState, Stance};

/// How far above the surface the eye sits when mounted (m): the aimed
/// weapon's handguard is about this far under the sight line, so it rests
/// on the edge.
pub const REST: f32 = 0.08;
/// A surface up to this far (m) below where a crouched body's gun would rest
/// still mounts; the gun rests a little above it.
pub const LOW_SLACK: f32 = 0.08;
/// The view's cone when mounted: radians either side of the mount's yaw, and
/// up and down from level. The reference allows about this much before the
/// view stops (measured by eye from play, not a published number).
pub const YAW_LIMIT: f32 = 40.0 * std::f32::consts::PI / 180.0;
pub const PITCH_UP: f32 = 20.0 * std::f32::consts::PI / 180.0;
pub const PITCH_DOWN: f32 = 30.0 * std::f32::consts::PI / 180.0;
/// Past the cone by this much (radians), the mount lets go. The client holds
/// the view at the edge until the player pushes on.
pub const LIMIT_SLACK: f32 = 1.0 * std::f32::consts::PI / 180.0;
/// A movement input longer than this leaves the mount.
pub const MOVE_EXIT: f32 = 0.3;
/// Seconds for the mount to settle in (or let go): the kick and sway scales
/// and the drawn rest follow it.
pub const SETTLE_TIME: f32 = 0.18;
/// Share of the kick and of the aimed sway kept once mounted. The reference
/// removes most of the recoil (players measure 70 to 90% less) and nearly
/// all idle sway.
pub const KICK_SCALE: f32 = 0.35;
pub const SWAY_SCALE: f32 = 0.15;

/// Heights above the feet (m) where the engine looks for a surface to mount
/// on: from where a crouched body's gun rests (less [`LOW_SLACK`]) to where
/// a standing body's does.
pub fn reach(params: &MoveParams) -> (f32, f32) {
    (
        params.eye_height_at(1.0) - REST - LOW_SLACK,
        params.eye_height_at(0.0) - REST,
    )
}

/// The crouch blend (0 standing to 1 crouched) that rests the gun on a
/// surface `height` above the feet, or `None` if it is out of reach.
pub fn crouch_for(height: f32, params: &MoveParams) -> Option<f32> {
    let (low, high) = reach(params);
    if !(low..=high + 1e-4).contains(&height) {
        return None;
    }
    let (stand, crouched) = (params.eye_height_at(0.0), params.eye_height_at(1.0));
    let eye = (height + REST).clamp(crouched, stand);
    // Invert the eye's ease by bisection: a fixed number of plain f32 steps,
    // so client and server agree to the bit.
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..20 {
        let mid = 0.5 * (lo + hi);
        if params.eye_height_at(mid) > eye {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some(0.5 * (lo + hi))
}

/// What the mount rule sees of the player this tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MountInput {
    /// Aim held.
    pub aim: bool,
    /// The melee button went down this tick.
    pub melee_pressed: bool,
    /// Jump, crouch, or sprint went down this tick.
    pub stance_pressed: bool,
    pub wish: Vec2,
    pub yaw: f32,
    pub pitch: f32,
    /// The weapon can stay mounted: not switching, throwing, or swinging the
    /// knife (a reload is fine).
    pub weapon_free: bool,
    /// A weapon switch or a throw was asked for this tick.
    pub leaving: bool,
}

/// Mounts or lets go for one tick, and settles [`MoveState::mount_blend`].
/// Returns true when the melee press went to mounting or letting go, so it
/// does not also swing the knife.
pub fn step(
    state: &mut MoveState,
    input: &MountInput,
    env: MoveEnv,
    params: &MoveParams,
    dt: f32,
) -> bool {
    let mut consumed = false;
    if state.mounted {
        let off = wrap(input.yaw - state.mount_yaw).abs();
        let leave = !input.aim
            || input.melee_pressed
            || input.stance_pressed
            || input.leaving
            || !input.weapon_free
            || input.wish.length() > MOVE_EXIT
            || !env.on_floor
            || env.mount.is_none()
            || state.mantling()
            || state.on_ladder()
            || off > YAW_LIMIT + LIMIT_SLACK
            || !pitch_in_cone(input.pitch, LIMIT_SLACK);
        if leave {
            state.mounted = false;
            consumed = input.melee_pressed;
        }
    } else if input.melee_pressed
        && input.aim
        && input.weapon_free
        && !input.leaving
        && env.on_floor
        && state.stance != Stance::Sliding
        && !state.sprinting
        && !state.mantling()
        && !state.on_ladder()
        && pitch_in_cone(input.pitch, 0.0)
        && let Some(crouch) = env.mount.and_then(|h| crouch_for(h, params))
    {
        state.mounted = true;
        state.mount_yaw = input.yaw;
        state.mount_crouch = crouch;
        consumed = true;
    }
    let step = dt / SETTLE_TIME;
    state.mount_blend = if state.mounted {
        (state.mount_blend + step).min(1.0)
    } else {
        (state.mount_blend - step).max(0.0)
    };
    consumed
}

/// Whether a pitch is inside the mounted view's cone, widened by `slack`.
pub fn pitch_in_cone(pitch: f32, slack: f32) -> bool {
    (-PITCH_DOWN - slack..=PITCH_UP + slack).contains(&pitch)
}

/// The view angles (yaw, pitch) held inside the mounted cone about `mount_yaw`.
pub fn clamp_view(yaw: f32, pitch: f32, mount_yaw: f32) -> (f32, f32) {
    let off = wrap(yaw - mount_yaw).clamp(-YAW_LIMIT, YAW_LIMIT);
    (mount_yaw + off, pitch.clamp(-PITCH_DOWN, PITCH_UP))
}

/// Share of the kick kept at a mount blend (1 unmounted).
pub fn kick_scale(blend: f32) -> f32 {
    1.0 + (KICK_SCALE - 1.0) * blend.clamp(0.0, 1.0)
}

/// Share of the aimed sway kept at a mount blend (1 unmounted).
pub fn sway_scale(blend: f32) -> f32 {
    1.0 + (SWAY_SCALE - 1.0) * blend.clamp(0.0, 1.0)
}

/// Wraps an angle to `[-π, π)`.
fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 120.0;

    fn env(height: f32) -> MoveEnv {
        MoveEnv {
            mount: Some(height),
            ..MoveEnv::default()
        }
    }

    fn press() -> MountInput {
        MountInput {
            aim: true,
            melee_pressed: true,
            weapon_free: true,
            ..MountInput::default()
        }
    }

    fn held() -> MountInput {
        MountInput {
            melee_pressed: false,
            ..press()
        }
    }

    #[test]
    fn the_eye_rests_the_gun_on_the_surface() {
        let params = MoveParams::default();
        // A 1.2 m concrete barrier: the standing body sinks to rest on it.
        let c = crouch_for(1.2, &params).expect("in reach");
        assert!(c > 0.0 && c < 1.0, "{c}");
        assert!((params.eye_height_at(c) - (1.2 + REST)).abs() < 1e-4);
        // At the top of the reach the body stands; at the bottom it crouches.
        let (low, high) = reach(&params);
        assert!(crouch_for(high, &params).unwrap() < 1e-3);
        assert!(crouch_for(low, &params).unwrap() > 1.0 - 1e-3);
        // The Testing Area's 0.84 m sandbags mount crouched; the culvert's
        // 0.44 m ones (prone height) and a 1.8 m wall do not.
        assert!(crouch_for(0.84, &params).unwrap() > 0.99);
        assert_eq!(crouch_for(0.44, &params), None);
        assert_eq!(crouch_for(1.8, &params), None);
    }

    #[test]
    fn aim_and_melee_mounts_and_settles() {
        let params = MoveParams::default();
        let mut s = MoveState::default();
        let input = MountInput {
            yaw: 0.3,
            ..press()
        };
        assert!(step(&mut s, &input, env(1.2), &params, DT));
        assert!(s.mounted && s.mount_yaw == 0.3);
        assert!(s.mount_crouch > 0.0);
        for _ in 0..30 {
            step(&mut s, &held(), env(1.2), &params, DT);
        }
        assert_eq!(s.mount_blend, 1.0);
        assert!(kick_scale(s.mount_blend) < 0.5 && sway_scale(s.mount_blend) < 0.5);
    }

    #[test]
    fn nothing_to_mount_on_leaves_the_press_to_the_knife() {
        let params = MoveParams::default();
        let mut s = MoveState::default();
        assert!(!step(&mut s, &press(), MoveEnv::default(), &params, DT));
        assert!(!s.mounted);
        // Not aimed: the knife.
        let hip = MountInput {
            aim: false,
            ..press()
        };
        assert!(!step(&mut s, &hip, env(1.2), &params, DT));
        // Mid-switch or sprinting: no mount.
        let busy = MountInput {
            weapon_free: false,
            ..press()
        };
        assert!(!step(&mut s, &busy, env(1.2), &params, DT));
        s.sprinting = true;
        assert!(!step(&mut s, &press(), env(1.2), &params, DT));
    }

    #[test]
    fn every_exit_lets_go() {
        let params = MoveParams::default();
        let exits: [(&str, MountInput, MoveEnv); 9] = [
            (
                "aim let go",
                MountInput {
                    aim: false,
                    ..held()
                },
                env(1.2),
            ),
            ("melee again", press(), env(1.2)),
            (
                "jump or crouch",
                MountInput {
                    stance_pressed: true,
                    ..held()
                },
                env(1.2),
            ),
            (
                "moved",
                MountInput {
                    wish: Vec2::Y,
                    ..held()
                },
                env(1.2),
            ),
            (
                "switching",
                MountInput {
                    leaving: true,
                    ..held()
                },
                env(1.2),
            ),
            (
                "knifed",
                MountInput {
                    weapon_free: false,
                    ..held()
                },
                env(1.2),
            ),
            (
                "turned past the cone",
                MountInput {
                    yaw: YAW_LIMIT + 0.05,
                    ..held()
                },
                env(1.2),
            ),
            (
                "looked too far down",
                MountInput {
                    pitch: -PITCH_DOWN - 0.05,
                    ..held()
                },
                env(1.2),
            ),
            ("cover gone", held(), MoveEnv::default()),
        ];
        for (name, input, at) in exits {
            let mut s = MoveState::default();
            step(&mut s, &press(), env(1.2), &params, DT);
            assert!(s.mounted);
            let consumed = step(&mut s, &input, at, &params, DT);
            assert!(!s.mounted, "{name}");
            assert_eq!(consumed, input.melee_pressed, "{name}");
        }
        // Turning inside the cone and reloading (weapon free) keep it.
        let mut s = MoveState::default();
        step(&mut s, &press(), env(1.2), &params, DT);
        let turn = MountInput {
            yaw: YAW_LIMIT,
            pitch: PITCH_UP,
            wish: Vec2::new(0.1, 0.1),
            ..held()
        };
        step(&mut s, &turn, env(1.2), &params, DT);
        assert!(s.mounted);
    }

    #[test]
    fn the_view_is_held_in_the_cone_across_the_wrap() {
        let mount_yaw = std::f32::consts::PI - 0.1;
        // Turning left past +π wraps to -π but is still 0.3 rad off.
        let (yaw, pitch) = clamp_view(-std::f32::consts::PI + 0.2, -1.2, mount_yaw);
        assert!((wrap(yaw - mount_yaw) - 0.3).abs() < 1e-5);
        assert_eq!(pitch, -PITCH_DOWN);
        let (yaw, _) = clamp_view(mount_yaw - 1.5, 0.0, mount_yaw);
        assert!((wrap(yaw - mount_yaw) + YAW_LIMIT).abs() < 1e-5);
    }
}
