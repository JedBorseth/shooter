//! The same input stream must produce bit-identical state on every run, build
//! profile, and platform. Client prediction depends on it: a client and the
//! server that disagree here mispredict constantly.

use shooter_sim::movement::MoveEnv;
use shooter_sim::net::TICK_DT;
use shooter_sim::net::protocol::Predicted;
use shooter_sim::net::subtick;
use shooter_sim::player::{self, FireAim, PlayerEvent, PlayerInput, PlayerParams, PlayerState};
use shooter_sim::weapon::CombatEvent;
use shooter_sim::{Vec2, Vec3};
use shooter_sim::{fmath, rng};

/// A scripted but varied input stream: strafing, turning, sprinting, sliding,
/// jumping, aiming, firing, reloading, switching weapons, and knifing, with
/// and without a lunge. Each press of fire carries a press aim somewhere
/// along the turn since the last tick (some forged past it), checked against
/// the previous input as the server checks it.
fn input(tick: u32) -> PlayerInput {
    let raw = raw_input(tick);
    let Some(previous) = tick.checked_sub(1).map(raw_input) else {
        return raw;
    };
    let pressed = raw.fire && !previous.fire;
    let r = |salt: u32| rng::unit(tick, salt);
    let input = PlayerInput {
        fire_aim: pressed.then(|| FireAim {
            yaw: (previous.yaw - raw.yaw) * (r(10) * 1.4 - 0.2),
            pitch: (previous.pitch - raw.pitch) * (r(11) * 1.4 - 0.2),
        }),
        ..raw
    }
    .quantized();
    PlayerInput {
        fire_aim: subtick::check(Some((previous.yaw, previous.pitch)), &input),
        ..input
    }
}

fn raw_input(tick: u32) -> PlayerInput {
    let r = |salt: u32| rng::unit(tick / 7, salt);
    PlayerInput {
        wish: Vec2::new(r(1) * 2.0 - 1.0, if r(2) < 0.8 { 1.0 } else { -0.4 }),
        yaw: fmath::sin(tick as f32 * 0.013) * 2.0,
        pitch: fmath::cos(tick as f32 * 0.007) * 0.3,
        jump: tick % 97 < 2,
        sprint: tick % 150 < 2,
        crouch: tick % 150 == 70 || tick % 150 == 71,
        fire: r(3) < 0.3,
        aim: r(4) < 0.4,
        reload: tick % 400 == 300,
        switch_to: (tick % 900 == 600).then_some(((tick / 900) % 2) as u8),
        pickup: None,
        melee: tick % 500 == 450 || tick % 500 == 451,
        lunge: (tick % 1000 == 450).then_some(Vec2::new(0.8, -2.6)),
        view_tick: None,
        fire_aim: None,
    }
    .quantized()
}

/// What one run produced: a hash of every bit of state after each tick, a
/// hash of every shot's angles, the final state, and how many shots left
/// from a press aim.
struct Run {
    state_hash: u64,
    shot_hash: u64,
    last: Predicted,
    press_shots: u32,
}

fn fnv(hash: &mut u64, bits: u32) {
    *hash ^= u64::from(bits);
    *hash = hash.wrapping_mul(0x0100_0000_01b3);
}

/// Runs `ticks` ticks on a flat floor and hashes every bit of the state and
/// of every shot.
fn run(ticks: u32) -> Run {
    let params = PlayerParams::default();
    let mut p = Predicted {
        state: PlayerState::default(),
        position: Vec3::ZERO,
        on_floor: true,
    };
    let mut events = Vec::new();
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut shot_hash = 0xcbf2_9ce4_8422_2325u64;
    let mut press_shots = 0;
    let mut mix = |bits: u32| fnv(&mut hash, bits);
    for tick in 0..ticks {
        let input = input(tick);
        let env = MoveEnv {
            on_floor: p.on_floor,
            can_stand: true,
            ledge: None,
        };
        events.clear();
        let v = player::step(&mut p.state, &input, env, &params, TICK_DT, &mut events);
        p.position += v * TICK_DT;
        if p.position.y <= 0.0 {
            p.position.y = 0.0;
            p.state.movement.velocity.y = 0.0;
        }
        p.on_floor = p.position.y <= 0.0;

        for f in [p.position.x, p.position.y, p.position.z] {
            mix(f.to_bits());
        }
        let m = &p.state.movement;
        for f in [m.velocity.x, m.velocity.y, m.velocity.z, m.slide_time] {
            mix(f.to_bits());
        }
        let c = &p.state.combat;
        for f in [c.ads, c.cooldown, c.recoil.x, c.recoil.y, c.since_shot] {
            mix(f.to_bits());
        }
        mix(c.shots_fired);
        mix(u32::from(c.weapon().magazine));
        mix(m.stance as u32);
        mix(events.len() as u32);

        for event in &events {
            if let PlayerEvent::Combat(CombatEvent::Fired {
                yaw,
                pitch,
                aim_offset,
                ..
            }) = *event
            {
                assert_eq!((yaw, pitch), input.shot_view());
                if input.fire_aim.is_some() {
                    press_shots += 1;
                }
                for f in [yaw, pitch, aim_offset.x, aim_offset.y] {
                    fnv(&mut shot_hash, f.to_bits());
                }
            }
        }
    }
    Run {
        state_hash: hash,
        shot_hash,
        last: p,
        press_shots,
    }
}

/// One minute of play.
const TICKS: u32 = 60 * shooter_sim::TICK_RATE;

#[test]
fn same_inputs_give_bit_identical_state() {
    let a = run(TICKS);
    let b = run(TICKS);
    assert_eq!(a.state_hash, b.state_hash);
    assert_eq!(a.shot_hash, b.shot_hash);
    assert_eq!(a.last, b.last);
    let pa = a.last;
    // The script exercises the whole simulation, not a player standing still.
    assert!(
        pa.state.combat.shots_fired > 100,
        "fired {}",
        pa.state.combat.shots_fired
    );
    assert!(
        pa.position.length() > 20.0,
        "moved {}",
        pa.position.length()
    );
    // Presses that fire at once leave from their press aim.
    assert!(a.press_shots > 10, "{} press shots", a.press_shots);
}

/// The press aim only steers shots: the same run without it ends in exactly
/// the same state, view included.
#[test]
fn press_aim_changes_no_state() {
    let with = run(TICKS);
    let mut p = Predicted {
        state: PlayerState::default(),
        position: Vec3::ZERO,
        on_floor: true,
    };
    let params = PlayerParams::default();
    let mut events = Vec::new();
    let mut with_shots = Vec::new();
    let mut without_shots = Vec::new();
    for tick in 0..600 {
        let input = input(tick);
        let env = MoveEnv {
            on_floor: true,
            can_stand: true,
            ledge: None,
        };
        let mut q = p;
        events.clear();
        player::step(&mut p.state, &input, env, &params, TICK_DT, &mut events);
        with_shots.extend(events.iter().copied());
        events.clear();
        let plain = PlayerInput {
            fire_aim: None,
            ..input
        };
        player::step(&mut q.state, &plain, env, &params, TICK_DT, &mut events);
        without_shots.extend(events.iter().copied());
        assert_eq!(p.state, q.state, "tick {tick}");
    }
    assert_eq!(with_shots.len(), without_shots.len());
    assert!(with.press_shots > 0);
    assert!(
        with_shots != without_shots,
        "press shots leave from different angles"
    );
}

/// The hash a reference run produced. CI runs this on Linux, Windows, and
/// macOS, so a platform whose float math differs fails here before players
/// see mispredictions. If a deliberate simulation change moves it, update it.
// Melee: the script knifes every few seconds, sometimes with a lunge.
// Weapon tuning, round 4: firing ends a reload once the weapon is back up,
// heavier patterns for the M4, AK, BR-3, and LSW-6, new damage ranges, and
// a fuller aimed breath.
// Movement, round 1: stance changes blend over time, slides are earned and
// capped, and a mantle keeps the direction it started in.
// Movement, round 2: ground inertia, strafe and backpedal scales, and a
// sprint that builds and winds down.
// Movement, round 3: landings cost speed and recover, tired jumps, and air
// steering that never adds speed.
const REFERENCE_HASH: u64 = 0xc6af_afd8_c4f7_6557;
/// The same for every shot's view angles and aim offset (sub-tick press aims
/// included).
const SHOT_REFERENCE_HASH: u64 = 0xe2ef_ea58_c6c7_b1e6;

#[test]
fn state_matches_the_reference_on_every_platform() {
    let run = run(TICKS);
    let hash = run.state_hash;
    assert_eq!(hash, REFERENCE_HASH, "simulation hash {hash:#018x}");
    let shots = run.shot_hash;
    assert_eq!(shots, SHOT_REFERENCE_HASH, "shot hash {shots:#018x}");
}
