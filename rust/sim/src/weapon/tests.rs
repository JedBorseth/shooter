use super::*;

const DT: f32 = 1.0 / 60.0;

fn id(key: &str) -> WeaponId {
    WeaponId::expect(key)
}

fn ready_with(loadout: [&str; 2]) -> CombatState {
    let mut s = CombatState::new(loadout.map(id));
    s.action = Action::Ready;
    s
}

fn ready() -> CombatState {
    ready_with(["m4", "m1911"])
}

fn run(
    state: &mut CombatState,
    input: CombatInput,
    ctx: CombatContext,
    ticks: usize,
) -> Vec<CombatEvent> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        step(state, &input, &ctx, DT, &mut events);
    }
    events
}

fn fire() -> CombatInput {
    CombatInput {
        fire: true,
        ..Default::default()
    }
}

fn aim() -> CombatInput {
    CombatInput {
        aim: true,
        ..Default::default()
    }
}

fn shots(events: &[CombatEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, CombatEvent::Fired { .. }))
        .count()
}

/// Presses fire once every `period` ticks for `ticks` ticks.
fn tap_fire(state: &mut CombatState, period: usize, ticks: usize) -> Vec<CombatEvent> {
    let mut events = Vec::new();
    for i in 0..ticks {
        let input = CombatInput {
            fire: i % period == 0,
            ..Default::default()
        };
        step(state, &input, &CombatContext::default(), DT, &mut events);
    }
    events
}

#[test]
fn full_auto_matches_fire_rate() {
    let mut s = ready();
    let events = run(&mut s, fire(), CombatContext::default(), 120);
    // 750 RPM for 2 s is 25 rounds.
    assert_eq!(shots(&events), 25);
    assert_eq!(s.weapon().magazine, 5);
}

#[test]
fn semi_auto_needs_separate_presses() {
    let mut s = ready();
    s.active = 1;
    let held = run(&mut s, fire(), CombatContext::default(), 60);
    assert_eq!(shots(&held), 1);
    let mut events = Vec::new();
    for i in 0..60 {
        let input = CombatInput {
            fire: i % 12 == 6,
            ..Default::default()
        };
        step(&mut s, &input, &CombatContext::default(), DT, &mut events);
    }
    assert_eq!(shots(&events), 5);
}

#[test]
fn empty_magazine_auto_reloads_and_refills() {
    let mut s = ready();
    let events = run(&mut s, fire(), CombatContext::default(), 200);
    assert!(events.contains(&CombatEvent::ReloadStarted {
        weapon: id("m4"),
        empty: true
    }));
    let events = run(
        &mut s,
        CombatInput::default(),
        CombatContext::default(),
        200,
    );
    assert!(events.contains(&CombatEvent::ReloadFinished { weapon: id("m4") }));
    assert_eq!(s.weapon().magazine, 30);
    assert_eq!(s.weapon().reserve, 90);
}

#[test]
fn sprinting_blocks_fire_then_sprint_out_delays_it() {
    let mut s = ready();
    let sprint = CombatContext {
        sprinting: true,
        ..Default::default()
    };
    let events = run(&mut s, fire(), sprint, 30);
    assert_eq!(shots(&events), 0);

    // 0.22 s sprint-out is 13 ticks at 60 Hz.
    let events = run(&mut s, fire(), CombatContext::default(), 12);
    assert_eq!(shots(&events), 0);
    let events = run(&mut s, fire(), CombatContext::default(), 2);
    assert_eq!(shots(&events), 1);
}

#[test]
fn sprinting_cancels_reload() {
    let mut s = ready();
    s.slots[0].magazine = 10;
    run(
        &mut s,
        CombatInput {
            reload: true,
            ..Default::default()
        },
        CombatContext::default(),
        30,
    );
    assert!(s.is_reloading());
    run(
        &mut s,
        CombatInput::default(),
        CombatContext {
            sprinting: true,
            ..Default::default()
        },
        1,
    );
    assert!(!s.is_reloading());
    assert_eq!(s.weapon().magazine, 10);
}

#[test]
fn recoil_climbs_then_recovers() {
    let mut s = ready();
    run(&mut s, fire(), CombatContext::default(), 30);
    let peak = s.recoil;
    assert!(
        peak.x > 3f32.to_radians(),
        "view climbs during a spray: {peak:?}"
    );
    run(&mut s, CombatInput::default(), CombatContext::default(), 90);
    assert!(s.recoil.length() < peak.length() * 0.01);
}

#[test]
fn ads_tightens_spread_and_takes_ads_time() {
    let mut s = ready();
    let ctx = CombatContext {
        move_ratio: 1.0,
        ..Default::default()
    };
    assert!(s.spread_degrees(&ctx) > 3.0);
    run(&mut s, aim(), ctx, 10);
    assert!(s.ads > 0.5 && s.ads < 1.0);
    run(&mut s, aim(), ctx, 10);
    assert_eq!(s.ads, 1.0);
    assert_eq!(s.spread_degrees(&ctx), 0.0);
}

#[test]
fn switching_lowers_then_raises() {
    let mut s = ready();
    let events = run(
        &mut s,
        CombatInput {
            switch_to: Some(1),
            ..Default::default()
        },
        CombatContext::default(),
        20,
    );
    assert!(events.contains(&CombatEvent::Switched {
        weapon: id("m1911")
    }));
    assert_eq!(s.active, 1);
    run(&mut s, CombatInput::default(), CombatContext::default(), 20);
    assert_eq!(s.action, Action::Ready);
}

#[test]
fn spread_is_deterministic() {
    let simulate = || {
        let mut s = ready();
        run(&mut s, fire(), CombatContext::default(), 60)
    };
    assert_eq!(simulate(), simulate());
}

#[test]
fn shotgun_fires_spread_pellets() {
    let shotgun = id("m870");
    let offsets: Vec<_> = (0..8).map(|i| pellet_offset(shotgun, 3, i)).collect();
    let cone = 4.5f32.to_radians();
    assert!(offsets.iter().all(|o| o.length() <= cone + 1e-6));
    let distinct = offsets
        .iter()
        .enumerate()
        .all(|(i, a)| offsets[i + 1..].iter().all(|b| (*a - *b).length() > 1e-5));
    assert!(distinct, "pellets go in different directions: {offsets:?}");
    assert_eq!(pellet_offset(id("m4"), 3, 0), Vec2::ZERO);
}

#[test]
fn pump_cycles_between_shots() {
    let mut s = ready_with(["m870", "m1911"]);
    let events = tap_fire(&mut s, 5, 30);
    // The first shot starts a 0.62 s pump; taps during it do nothing.
    assert_eq!(shots(&events), 1);
    assert!(events.contains(&CombatEvent::Cycled { weapon: id("m870") }));
    let events = tap_fire(&mut s, 5, 30);
    assert_eq!(shots(&events), 1, "fires again once the pump is worked");
}

#[test]
fn bolt_action_stays_scoped_while_cycling() {
    let mut s = ready_with(["m40", "m1911"]);
    run(&mut s, aim(), CombatContext::default(), 30);
    assert_eq!(s.ads, 1.0);
    let both = CombatInput {
        fire: true,
        aim: true,
        ..Default::default()
    };
    run(&mut s, both, CombatContext::default(), 2);
    assert!(matches!(s.action, Action::Cycling { .. }));
    assert_eq!(s.ads, 1.0);
}

#[test]
fn shotgun_loads_shell_by_shell_and_fire_interrupts() {
    let mut s = ready_with(["m870", "m1911"]);
    s.slots[0].magazine = 2;
    let reload = CombatInput {
        reload: true,
        ..Default::default()
    };
    let mut events = run(&mut s, reload, CombatContext::default(), 1);
    // start 0.45 s + 2 rounds at 0.5 s each = 1.45 s, 87 ticks.
    events.extend(run(
        &mut s,
        CombatInput::default(),
        CombatContext::default(),
        88,
    ));
    let loaded = events
        .iter()
        .filter(|e| matches!(e, CombatEvent::RoundLoaded { .. }))
        .count();
    assert_eq!(loaded, 2);
    assert_eq!(s.weapon().magazine, 4);
    assert!(s.is_reloading());

    let events = run(&mut s, fire(), CombatContext::default(), 1);
    assert_eq!(shots(&events), 1, "firing interrupts the reload");
    assert_eq!(s.weapon().magazine, 3);
    assert!(!s.is_reloading());
}

#[test]
fn shotgun_reload_finishes_when_full() {
    let mut s = ready_with(["m870", "m1911"]);
    s.slots[0].magazine = 5;
    let mut events = run(
        &mut s,
        CombatInput {
            reload: true,
            ..Default::default()
        },
        CombatContext::default(),
        1,
    );
    events.extend(run(
        &mut s,
        CombatInput::default(),
        CombatContext::default(),
        120,
    ));
    assert!(events.contains(&CombatEvent::ReloadFinished { weapon: id("m870") }));
    assert_eq!(s.weapon().magazine, 6);
    assert_eq!(s.action, Action::Ready);
}

#[test]
fn scope_sways_and_holding_breath_steadies_it() {
    let mut s = ready_with(["m40", "m1911"]);
    let sway_over = |s: &mut CombatState, input: CombatInput, ticks: usize| {
        let mut peak = 0.0f32;
        for _ in 0..ticks {
            step(s, &input, &CombatContext::default(), DT, &mut Vec::new());
            peak = peak.max(s.scope_sway().length());
        }
        peak
    };
    let free = sway_over(&mut s, aim(), 120);
    assert!(free > 0.2f32.to_radians(), "scope sways: {free}");

    let hold = CombatInput {
        aim: true,
        hold_breath: true,
        ..Default::default()
    };
    let steady = sway_over(&mut s, hold, 120);
    assert!(steady < free * 0.2, "breath steadies: {steady} vs {free}");
    assert!(s.holding_breath);

    // 4 s of breath; after that the player is winded and sways more.
    sway_over(&mut s, hold, 150);
    assert!(!s.holding_breath);
    assert!(s.winded > 0.0);
}

#[test]
fn unscoped_weapons_never_sway() {
    let mut s = ready();
    run(&mut s, aim(), CombatContext::default(), 60);
    assert_eq!(s.scope_sway(), Vec2::ZERO);
    assert_eq!(s.scope_amount(), 0.0);
}

#[test]
fn pickup_replaces_the_matching_slot() {
    let mut s = ready();
    let events = run(
        &mut s,
        CombatInput {
            pickup: Some(id("m870")),
            ..Default::default()
        },
        CombatContext::default(),
        1,
    );
    assert!(events.contains(&CombatEvent::Switched { weapon: id("m870") }));
    assert_eq!(s.slots[0].id, id("m870"));
    assert_eq!(s.slots[1].id, id("m1911"));
    assert_eq!(s.active, 0);
    assert!(matches!(s.action, Action::Raising { .. }));
}

#[test]
fn picking_up_a_carried_weapon_refills_ammo() {
    let mut s = ready();
    s.slots[1].reserve = 3;
    run(
        &mut s,
        CombatInput {
            pickup: Some(id("m1911")),
            ..Default::default()
        },
        CombatContext::default(),
        1,
    );
    assert_eq!(s.slots[1].reserve, 32);
    assert_eq!(s.active, 0, "refilling does not switch weapons");
}

#[test]
fn reload_cannot_skip_a_pump_cycle() {
    let mut s = ready_with(["m870", "m1911"]);
    let mut events = Vec::new();
    // Mash fire and reload on alternate ticks for one second.
    for i in 0..60 {
        let input = CombatInput {
            fire: i % 2 == 0,
            reload: i % 2 == 1,
            ..Default::default()
        };
        step(&mut s, &input, &CombatContext::default(), DT, &mut events);
    }
    // One shot, a 0.62 s pump, then a second shot interrupting the queued reload at most.
    assert!(shots(&events) <= 2, "fired {}", shots(&events));
}

#[test]
fn reload_pressed_during_a_cycle_starts_after_it() {
    let mut s = ready_with(["m40", "m1911"]);
    run(&mut s, fire(), CombatContext::default(), 1);
    assert!(matches!(s.action, Action::Cycling { .. }));
    let events = run(
        &mut s,
        CombatInput {
            reload: true,
            ..Default::default()
        },
        CombatContext::default(),
        1,
    );
    assert!(events.is_empty());
    assert!(
        matches!(s.action, Action::Cycling { .. }),
        "the bolt still cycles"
    );
    let events = run(&mut s, CombatInput::default(), CombatContext::default(), 70);
    assert!(events.contains(&CombatEvent::ReloadStarted {
        weapon: id("m40"),
        empty: false
    }));
}

#[test]
fn pickup_drops_aim() {
    let mut s = ready();
    run(&mut s, aim(), CombatContext::default(), 30);
    assert_eq!(s.ads, 1.0);
    run(
        &mut s,
        CombatInput {
            aim: true,
            pickup: Some(id("m40")),
            ..Default::default()
        },
        CombatContext::default(),
        1,
    );
    assert_eq!(s.scope_amount(), 0.0);
}

#[test]
fn refill_ammo_fills_every_slot_and_ends_reload() {
    let mut s = ready();
    s.slots[0].magazine = 3;
    s.slots[0].reserve = 0;
    s.slots[1].magazine = 0;
    s.slots[1].reserve = 4;
    s.action = Action::Reloading {
        remaining: 1.0,
        empty: false,
    };
    let mut events = Vec::new();
    s.refill_ammo(&mut events);
    let m4 = id("m4").def();
    assert_eq!(s.slots[0].magazine, m4.magazine);
    assert_eq!(s.slots[0].reserve, m4.reserve);
    assert_eq!(s.slots[1], WeaponState::full(id("m1911")));
    assert_eq!(s.action, Action::Ready);
    assert!(events.contains(&CombatEvent::PickedUp { weapon: id("m4") }));

    // A weapon switch in progress is left alone.
    s.action = Action::Lowering {
        remaining: 0.2,
        to: 1,
    };
    s.refill_ammo(&mut events);
    assert!(matches!(s.action, Action::Lowering { to: 1, .. }));
}

#[test]
fn refill_ammo_ends_a_shell_by_shell_reload() {
    let mut s = ready_with(["m870", "m1911"]);
    s.slots[0].magazine = 2;
    let reload = CombatInput {
        reload: true,
        ..Default::default()
    };
    run(&mut s, reload, CombatContext::default(), 40);
    assert!(matches!(s.action, Action::Loading { .. }));
    s.refill_ammo(&mut Vec::new());
    assert_eq!(s.slots[0], WeaponState::full(id("m870")));
    assert_eq!(s.action, Action::Ready);
    let events = run(&mut s, CombatInput::default(), CombatContext::default(), 60);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, CombatEvent::RoundLoaded { .. })),
        "no more shells go in"
    );
}

#[test]
fn refill_ammo_never_skips_a_bolt_cycle() {
    // A cycle in progress keeps running.
    let mut s = ready_with(["m40", "m1911"]);
    run(&mut s, fire(), CombatContext::default(), 1);
    let Action::Cycling { remaining } = s.action else {
        panic!("the bolt cycles after a shot, got {:?}", s.action);
    };
    s.refill_ammo(&mut Vec::new());
    assert_eq!(s.action, Action::Cycling { remaining });

    // Firing the last round leaves the chamber empty: the refill ends the
    // reload that follows, and the bolt is worked before the next shot.
    let mut s = ready_with(["m40", "m1911"]);
    s.slots[0].magazine = 1;
    run(&mut s, fire(), CombatContext::default(), 1);
    run(&mut s, CombatInput::default(), CombatContext::default(), 10);
    assert!(matches!(s.action, Action::Reloading { empty: true, .. }));
    let mut events = Vec::new();
    s.refill_ammo(&mut events);
    assert_eq!(s.weapon().magazine, id("m40").def().magazine);
    assert!(matches!(s.action, Action::Cycling { .. }));
    assert!(events.contains(&CombatEvent::Cycled { weapon: id("m40") }));
    let events = run(&mut s, fire(), CombatContext::default(), 30);
    assert_eq!(shots(&events), 0, "the bolt is still being worked");
    let events = run(&mut s, CombatInput::default(), CombatContext::default(), 40);
    assert!(events.is_empty());
    assert_eq!(s.action, Action::Ready);
}
