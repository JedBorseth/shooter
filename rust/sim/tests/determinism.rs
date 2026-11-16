//! The same input stream must produce bit-identical state on every run, build
//! profile, and platform. Client prediction depends on it: a client and the
//! server that disagree here mispredict constantly.

use shooter_sim::movement::MoveEnv;
use shooter_sim::net::TICK_DT;
use shooter_sim::net::protocol::Predicted;
use shooter_sim::player::{self, PlayerInput, PlayerParams, PlayerState};
use shooter_sim::rng;
use shooter_sim::{Vec2, Vec3};

/// A scripted but varied input stream: strafing, turning, sprinting, sliding,
/// jumping, aiming, firing, reloading, and switching weapons.
fn input(tick: u32) -> PlayerInput {
    let r = |salt: u32| rng::unit(tick / 7, salt);
    PlayerInput {
        wish: Vec2::new(r(1) * 2.0 - 1.0, if r(2) < 0.8 { 1.0 } else { -0.4 }),
        yaw: (tick as f32 * 0.013).sin() * 2.0,
        pitch: (tick as f32 * 0.007).cos() * 0.3,
        jump: tick % 97 < 2,
        sprint: tick % 150 < 2,
        crouch: tick % 150 == 70 || tick % 150 == 71,
        fire: r(3) < 0.3,
        aim: r(4) < 0.4,
        reload: tick % 400 == 300,
        switch_to: (tick % 900 == 600).then_some(((tick / 900) % 2) as u8),
        pickup: None,
    }
    .quantized()
}

/// Runs `ticks` ticks on a flat floor and hashes every bit of the final state.
fn run(ticks: u32) -> (u64, Predicted) {
    let params = PlayerParams::default();
    let mut p = Predicted {
        state: PlayerState::default(),
        position: Vec3::ZERO,
        on_floor: true,
    };
    let mut events = Vec::new();
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut mix = |bits: u32| {
        hash ^= u64::from(bits);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    };
    for tick in 0..ticks {
        let env = MoveEnv {
            on_floor: p.on_floor,
            can_stand: true,
        };
        events.clear();
        let v = player::step(
            &mut p.state,
            &input(tick),
            env,
            &params,
            TICK_DT,
            &mut events,
        );
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
    }
    (hash, p)
}

#[test]
fn same_inputs_give_bit_identical_state() {
    let (a, pa) = run(3600);
    let (b, pb) = run(3600);
    assert_eq!(a, b);
    assert_eq!(pa, pb);
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
}

/// The hash a reference run produced. CI runs this on Linux, Windows, and
/// macOS, so a platform whose float math differs fails here before players
/// see mispredictions. If a deliberate simulation change moves it, update it.
const REFERENCE_HASH: u64 = 0xad4c_2986_c063_32c0;

#[test]
fn state_matches_the_reference_on_every_platform() {
    let (hash, _) = run(3600);
    assert_eq!(hash, REFERENCE_HASH, "simulation hash {hash:#018x}");
}
