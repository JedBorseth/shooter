# Weapon rules

How firing, recoil, aiming, and reloads work, and the numbers that set each weapon's identity. The rules live in `rust/sim/src/weapon/` and the numbers in `godot/data/weapons/*.ron`. Both client and server run them, so they are deterministic (see the conventions in [TODO.md](../TODO.md)).

## Recoil

Each shot kicks the view by the weapon's `recoil.pattern` entry for that shot of the spray (the last entry repeats), plus hashed variation (`recoil.jitter`), scaled down while aimed (`ads_scale`) and crouched (`crouch_scale`, 0.85 by default). The kick splits in two:

- **Climb** (`1 - kick_return` of it) turns the view and stays, as if the mouse had moved. The simulation reports it with each `CombatEvent::Fired`; the shooter's client turns its look by it over about 40 ms. Pulling down against a spray holds the aim where the player puts it, and nothing drifts back after the trigger is let go.
- **Snap** (`kick_return`) is an aim offset that settles back at `recovery_rate` once `recovery_delay` has passed since the shot. Automatics return about 30% (a bounce of about 0.1° between rounds); pistols, shotguns, and the sniper return 60–70%, so a single shot kicks hard and settles near the target.

Both parts are aim, and the camera looks along the aim, so the screen centre and the sights are always where the next round goes. The per-shot camera punch (`camera_punch` on each `WeaponView`) only rolls the view and pulses the zoom slightly; it never moves the centre.

## Spread

- Hip cone: `spread.hip`, opening to `hip_moving` with speed and to `air` in the air, 20% tighter crouched.
- Hip fire blooms: each shot adds `spread.bloom` degrees up to `bloom_max`; the bloom holds through a spray and shrinks at `bloom_recovery` degrees a second once the trigger rests.
- Aimed: `spread.ads` (0 for every rifle and SMG). Every weapon drifts slightly while aimed (`spread.ads_sway`, about 0.05°); scopes sway more and can be steadied by holding breath. Sway is part of the aim, so the sights show it truthfully.

## Aiming, reloads, and switches

- Aiming takes `ads_time` and works through a reload and a bolt or pump cycle, but not a switch.
- A magazine reload takes `time` (tactical) or `empty_time` (empty, which ends with the bolt release or charge). Its rounds count at `mag_in` seconds, when the fresh magazine seats; a reload cut short after that keeps them.
- Sprinting does not cancel a reload, and a reload can start while sprinting. Firing ends a sprint and waits `sprint_out_time`.
- A switch that cuts off a bolt or pump cycle, or an empty reload before its charge, leaves the weapon needing its action worked (`WeaponState::needs_cycle`): it cycles when drawn again, so switching never skips a cycle.
- Re-selecting the weapon in hand while it is being put away brings it back up from as far as it got.
- A burst in progress is dropped by a reload, a sprint, or a switch.

## Damage zones

The head uses `damage.headshot`. Other zones use the weapon's `zones` (`stomach`, `arm`, `leg`; default 1, 0.9, 0.85). The M40 kills in one shot only to the head or upper chest (stomach 0.9, arms 0.75, legs 0.7); the PDW-9 is forgiving on limbs.

## Time to kill

Chest hits on a 100-health player with the trigger at the weapon's maximum rate, from `cargo test -p shooter-sim ttk_table -- --nocapture` (`rust/sim/src/weapon/ttk.rs`). Shotgun pellets land in proportion to a 0.25 m torso disc inside the pellet cone. Shots to kill in brackets.

| weapon | 5 m | 10 m | 15 m | 20 m | 30 m | 40 m | 50 m | 70 m |
|---|---|---|---|---|---|---|---|---|
| ak47 | 200 ms (3) | 200 ms (3) | 200 ms (3) | 200 ms (3) | 300 ms (4) | 300 ms (4) | 300 ms (4) | 300 ms (4) |
| br3 | 150 ms (3) | 150 ms (3) | 150 ms (3) | 385 ms (4) | 385 ms (4) | 385 ms (4) | 385 ms (4) | 385 ms (4) |
| lsw6 | 277 ms (4) | 277 ms (4) | 277 ms (4) | 277 ms (4) | 277 ms (4) | 277 ms (4) | 277 ms (4) | 277 ms (4) |
| m1911 | 300 ms (3) | 300 ms (3) | 300 ms (3) | 450 ms (4) | 450 ms (4) | 450 ms (4) | 450 ms (4) | 450 ms (4) |
| m4 | 240 ms (4) | 240 ms (4) | 240 ms (4) | 240 ms (4) | 240 ms (4) | 320 ms (5) | 320 ms (5) | 320 ms (5) |
| m40 | 0 ms (1) | 0 ms (1) | 0 ms (1) | 0 ms (1) | 0 ms (1) | 0 ms (1) | 0 ms (1) | 0 ms (1) |
| m870 | 0 ms (1) | 620 ms (2) | 3720 ms (7) | 14260 ms (24) | 32240 ms (53) | 57660 ms (94) | 90520 ms (147) | 177320 ms (287) |
| pdw9 | 200 ms (4) | 200 ms (4) | 267 ms (5) | 333 ms (6) | 467 ms (8) | 467 ms (8) | 467 ms (8) | 467 ms (8) |

Identity by class:

| Weapon | Role | Handling (ADS / sprint-out, s) |
|---|---|---|
| M4A1 | All-rounder: 4 hits to 30 m, 5 beyond | 0.24 / 0.22 |
| AK-47 | Hits harder (3 to 20 m), slower and kicks more | 0.27 / 0.25 |
| BR-3 | One burst kills inside 15 m; a second burst beyond | 0.25 / 0.23 |
| LSW-6 | Four hits at every range, 75 rounds, slow everything | 0.42 / 0.38 |
| PDW-9 | Fastest up close, falls off past 15 m, quickest handling | 0.19 / 0.16 |
| M870 | One shot to about 8 m, two to 12 m | 0.24 / 0.21 |
| M40 | One shot to head or upper chest at any range | 0.42 / 0.36 |
| M1911 | Sidearm: 3 hits to 15 m | 0.16 / 0.12 |
