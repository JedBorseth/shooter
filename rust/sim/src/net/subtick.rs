//! Sub-tick fire timing: a shot fired by a button press leaves from the view
//! the player had when the button went down, not the next tick's.
//!
//! The simulation runs at [`TICK_RATE`](crate::TICK_RATE), but the client
//! renders faster and reads the mouse every frame. A click between two ticks
//! is acted on by the next tick, up to a tick later, when a flick has already
//! carried the crosshair on. So the client captures the view angles at the
//! press event and sends them with the tick's input as a [`FireAim`]: the
//! press angles relative to the tick's. The first shot fired on that input
//! leaves from them ([`PlayerInput::shot_view`]), on the predicting client
//! and on the server alike; the view itself still ends the tick at the tick's
//! angles. The same input carries the remote-player view tick the client was
//! drawing at the press, so the server rewinds hitboxes to what the player saw
//! when they clicked (`net::lag`). (Valve's "sub-tick" input in CS2 works the
//! same way.)
//!
//! Held automatic fire does not use it: those shots come from the tick's own
//! cadence (`weapon::step`'s cooldown), so the tick's angles are the view at
//! that moment.
//!
//! The server cannot see the mouse, so it accepts press angles only between
//! the previous input's angles and this input's ([`check`]): a turn between
//! two ticks passes every angle in between, and anything else could be an aim
//! bot snapping the shot while the view stays put. The client runs the same
//! check before it predicts, so both sides use the same angles.

use std::f32::consts::{FRAC_PI_2, TAU};

use crate::bot::wrap_angle;
use crate::player::{FireAim, PlayerInput};

/// Press angles are sent in steps of 1/65536 of a turn (about 0.0055°), the
/// resolution of every other angle on the wire.
pub const STEP: f32 = TAU / 65536.0;
/// Largest press offset the wire carries: a quarter turn either way. A
/// legitimate one is a tick's worth of mouse motion; this only bounds the
/// encoding.
pub const LIMIT_STEPS: i32 = 16384;
/// [`LIMIT_STEPS`] in radians.
pub const LIMIT: f32 = FRAC_PI_2;
/// Slack past the turn between the two inputs that still counts as between
/// them: rounding to [`STEP`] and float error on large accumulated yaws.
/// About 0.01°, too little to steer a shot.
pub const TOLERANCE: f32 = 2.0 * STEP;

/// The wire's whole steps for an offset, clamped to [`LIMIT_STEPS`]. `None`
/// for a value that is not a number.
pub fn steps(offset: f32) -> Option<i32> {
    offset
        .is_finite()
        .then(|| ((offset / STEP).round() as i32).clamp(-LIMIT_STEPS, LIMIT_STEPS))
}

/// The offset `steps` stands for. Both sides compute it the same way, so the
/// value is bit-identical on the client and the server.
pub fn from_steps(steps: i32) -> f32 {
    steps as f32 * STEP
}

/// A press aim as the wire carries it, or `None` if it cannot be sent.
pub fn quantize(aim: FireAim) -> Option<FireAim> {
    Some(FireAim {
        yaw: from_steps(steps(aim.yaw)?),
        pitch: from_steps(steps(aim.pitch)?),
    })
}

/// The press aim `input` may use, given the angles of the input applied just
/// before it (`None` when there was none, such as the first input).
///
/// Each axis must lie between the previous input's angle and this input's,
/// within [`TOLERANCE`] (yaw the short way around the circle). An axis outside
/// that range is clamped to it, so a forged flick lands no further than the
/// view actually turned. An input that does not fire, or has no previous
/// input to compare with, gets none.
///
/// `input` must already be quantized (`PlayerInput::quantized`). The result
/// is then exactly what the wire carries, and checking it again with the same
/// previous angles returns it unchanged, which is how the server's check
/// agrees with the client's.
pub fn check(previous: Option<(f32, f32)>, input: &PlayerInput) -> Option<FireAim> {
    let aim = input.fire_aim.filter(|_| input.fire)?;
    let (previous_yaw, previous_pitch) = previous?;
    let yaw = clamp_axis(aim.yaw, wrap_angle(previous_yaw - input.yaw))?;
    let pitch = clamp_axis(aim.pitch, previous_pitch - input.pitch)?;
    Some(FireAim { yaw, pitch })
}

/// `offset` if it lies within [`TOLERANCE`] of the range from 0 (this input)
/// to `turn` (the previous input), otherwise the nearest point of that range
/// rounded to the wire's steps.
fn clamp_axis(offset: f32, turn: f32) -> Option<f32> {
    if !offset.is_finite() || !turn.is_finite() {
        return None;
    }
    let (low, high) = (turn.min(0.0), turn.max(0.0));
    if (low - TOLERANCE..=high + TOLERANCE).contains(&offset) {
        return Some(offset);
    }
    steps(offset.clamp(low, high)).map(from_steps)
}

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use super::*;

    fn firing(yaw: f32, pitch: f32, aim: FireAim) -> PlayerInput {
        PlayerInput {
            yaw,
            pitch,
            fire: true,
            fire_aim: Some(aim),
            ..Default::default()
        }
        .quantized()
    }

    fn aim(yaw: f32, pitch: f32) -> FireAim {
        quantize(FireAim { yaw, pitch }).unwrap()
    }

    #[test]
    fn a_press_between_the_two_ticks_is_kept() {
        // Turned right (yaw down) by 0.2 and up by 0.05 since the last tick;
        // the press came halfway through.
        let input = firing(1.0, 0.3, aim(0.1, -0.025));
        assert_eq!(check(Some((1.2, 0.25)), &input), input.fire_aim);
        // Either end of the turn counts as between.
        let at_start = firing(1.0, 0.3, aim(0.2, -0.05));
        assert_eq!(check(Some((1.2, 0.25)), &at_start), at_start.fire_aim);
        let at_tick = firing(1.0, 0.3, aim(0.0, 0.0));
        assert_eq!(check(Some((1.2, 0.25)), &at_tick), at_tick.fire_aim);
    }

    #[test]
    fn a_press_outside_the_turn_is_clamped_to_it() {
        // The view turned 0.2 left, but the press claims 0.5 further back.
        let input = firing(1.0, 0.0, aim(-0.5, 0.0));
        let checked = check(Some((0.8, 0.0)), &input).unwrap();
        assert_eq!(checked.yaw, from_steps(steps(-0.2).unwrap()));
        assert_eq!(checked.pitch, 0.0);
        // Past the other end: clamped to this tick's angle.
        let input = firing(1.0, 0.0, aim(0.0, -0.3));
        let checked = check(Some((1.0, 0.1)), &input).unwrap();
        assert_eq!(checked.pitch, 0.0);
        // A view that did not move at all leaves no room for a press offset.
        let snap = firing(1.0, 0.1, aim(0.4, -0.1));
        assert_eq!(
            check(Some((1.0, 0.1)), &snap),
            Some(FireAim {
                yaw: 0.0,
                pitch: 0.0
            })
        );
    }

    #[test]
    fn a_little_slack_is_allowed() {
        let input = firing(1.0, 0.0, aim(0.2 + STEP, 0.0));
        assert_eq!(check(Some((1.2, 0.0)), &input), input.fire_aim);
        let input = firing(1.0, 0.0, aim(0.2 + 4.0 * STEP, 0.0));
        let checked = check(Some((1.2, 0.0)), &input).unwrap();
        assert!((checked.yaw - 0.2).abs() <= STEP, "{}", checked.yaw);
    }

    #[test]
    fn yaw_is_compared_the_short_way_around() {
        // From just below π to just above -π is a small left turn of 0.2,
        // not most of a circle the other way.
        let previous = (PI - 0.1, 0.0);
        let input = firing(-PI + 0.1, 0.0, aim(-0.1, 0.0));
        assert_eq!(check(Some(previous), &input), input.fire_aim);
        let input = firing(-PI + 0.1, 0.0, aim(1.0, 0.0));
        assert_eq!(check(Some(previous), &input).unwrap().yaw, 0.0);
        // A client that never wraps its yaw compares the same way.
        let input = firing(PI + 0.1 + 4.0 * TAU, 0.0, aim(-0.1, 0.0));
        assert_eq!(check(Some(previous), &input), input.fire_aim);
    }

    #[test]
    fn checking_twice_changes_nothing() {
        // What the client sends after its check passes the server's as is.
        for (previous, input) in [
            ((0.8, 0.0), firing(1.0, 0.0, aim(-0.5, 0.0))),
            ((1.3, -0.2), firing(1.0, 0.1, aim(0.7, 0.9))),
            ((-3.0, 0.4), firing(3.1, -0.4, aim(-0.123, 0.456))),
        ] {
            let once = check(Some(previous), &input);
            let resent = PlayerInput {
                fire_aim: once,
                ..input
            }
            .quantized();
            assert_eq!(resent.fire_aim, once);
            assert_eq!(check(Some(previous), &resent), once);
        }
    }

    #[test]
    fn no_press_aim_without_firing_or_a_previous_input() {
        let input = firing(1.0, 0.0, aim(0.1, 0.0));
        assert_eq!(check(None, &input), None);
        let idle = PlayerInput {
            fire: false,
            ..input
        };
        assert_eq!(check(Some((1.2, 0.0)), &idle), None);
    }

    #[test]
    fn offsets_round_to_steps_and_stay_within_the_limit() {
        assert_eq!(steps(STEP * 3.4), Some(3));
        assert_eq!(steps(-10.0), Some(-LIMIT_STEPS));
        assert_eq!(steps(f32::NAN), None);
        assert_eq!(from_steps(LIMIT_STEPS), LIMIT);
    }
}
