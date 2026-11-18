# Weapon rules

How firing, recoil, aiming, and reloads work, and the numbers that set each weapon's identity. The rules live in `rust/sim/src/weapon/` and the numbers in `godot/data/weapons/*.ron`. Both client and server run them, so they are deterministic (see the conventions in [TODO.md](../TODO.md)).

## Recoil

Each shot kicks the view by the weapon's `recoil.pattern` entry for that shot of the spray (the last entry repeats), plus hashed variation (`recoil.jitter`), scaled down while aimed (`ads_scale`) and crouched (`crouch_scale`, 0.85 by default). The kick splits in two:

- **Climb** (`1 - kick_return` of it) turns the view and stays, as if the mouse had moved. The simulation reports it with each `CombatEvent::Fired`; the shooter's client turns its look by it over about 25 ms. Pulling down against a spray holds the aim where the player puts it, and nothing drifts back after the trigger is let go.
- **Snap** (`kick_return`) is an aim offset that settles back at `recovery_rate` once `recovery_delay` has passed since the shot. Automatics return 45% within about 50 ms (a visible bounce of about 0.1–0.15° that settles before the next round); pistols, shotguns, and the sniper return 60–70%, so a single shot kicks hard and settles near the target. Patterns start light and build over the first five or six rounds, so tapping is rewarded over holding, and each weapon drifts its own way (the AK climbs hardest and hooks right on rounds 2–5, the M4 leans right then swings back left, the LSW-6 bounces side to side, the PDW-9 jitters), with hashed sideways variation on every shot.

Both parts are aim, and the camera looks along the aim, so the screen centre and the sights are always where the next round goes. The per-shot camera punch (`camera_punch` on each `WeaponView`) only rolls the view and pulses the zoom slightly; it never moves the centre.

## Spread

- Hip cone: `spread.hip`, opening to `hip_moving` with speed and to `air` in the air, 20% tighter crouched.
- Hip fire blooms: each shot adds `spread.bloom` degrees up to `bloom_max`; the bloom holds through a spray and shrinks at `bloom_recovery` degrees a second once the trigger rests.
- Aimed: `spread.ads` (0 for every rifle and SMG). Every weapon drifts slightly while aimed, with a 0.3 Hz breath in it (`spread.ads_sway`, 0.12–0.15°); scopes sway more and can be steadied by holding breath. Sway is part of the aim, so the sights show it truthfully.

## Aiming, reloads, and switches

- Aiming takes `ads_time` and works through a reload and a bolt or pump cycle, but not a switch. The aimed amount is eased (`ads_ease`), and the spread, kick scale, sway, zoom, and weapon model all follow the same curve.
- At the hip a long gun sits in the lower right with its rear sight, receiver, and both hands in frame (the scenes' `hip_position` and `hip_rotation` plus `HIP_CLOSER` and `HIP_LONG_POS`/`HIP_LONG_ROT` in `rust/game/src/viewmodel.rs`), and it sprints muzzle-down across the body, still in the lower frame (`SPRINT_LONG_POS`/`SPRINT_LONG_ROT`), as modern Call of Duty viewmodels do.
- A magazine reload takes `time` (tactical) or `empty_time` (empty, which ends with the bolt release or charge). Its rounds count at `mag_in` seconds, when the fresh magazine seats; a reload cut short after that keeps them.
- Once the weapon is back on the shoulder (`WeaponDef::reload_ready_at`: 0.45 s after the seat for a tactical reload, 0.8 s for an empty one, the very end for a bolt action), firing ends the reload and fires: the rest is only the settle into the grip. The first-person clip is back on the shoulder at exactly that moment and eases out when cut.
- Sprinting does not cancel a reload, and a reload can start while sprinting. Firing ends a sprint and waits `sprint_out_time`.
- A switch that cuts off a bolt or pump cycle, or an empty reload before its charge, leaves the weapon needing its action worked (`WeaponState::needs_cycle`): it cycles when drawn again, so switching never skips a cycle.
- Re-selecting the weapon in hand while it is being put away brings it back up from as far as it got.
- **First raise.** A weapon's first draw of a game or round, and a weapon picked up, comes up with a first raise (`Raise::First`, `first_raise_time`): the draw, then the action worked (an AR's charging handle or an AK's, a pistol's slide racked, the bolt or the pump worked), and it cannot fire until that ends. Later draws, and every respawn after the first, are ordinary draws. This follows Call of Duty's `firstRaiseAnim`, with Modern Warfare III's 2023 rule (it plays at the start of a game or round and on a pickup, not on every life). Times are Black Ops 2's for each class (assault rifles 1.4–1.7 s, SMGs 1.0–1.25 s, LMGs 1.85 s, shotguns 1.65 s, snipers 1.75 s, pistols 0.85–1.1 s) scaled to our faster draws: M4A1 and BR-3 1.1 s, AK-47 1.2 s, PDW-9 0.8 s, LSW-6 1.7 s, M870 1.35 s, M40 1.4 s, M1911 0.8 s. A switch away during a first raise ends it; the weapon just draws next time.
- A burst in progress is dropped by a reload, a sprint, or a switch.

## Melee

The knife has its own button (V, or R3 on a gamepad) and works with any weapon in hand. Rules are in `rust/sim/src/melee.rs`.

- A strike kills a full-health player (150 damage). It lands 0.1 s after the press and the weapon is back up 0.45 s after that. It cuts a reload or a bolt or pump cycle short, like a switch: a magazine already seated stays in, and a cut cycle is worked when the strike ends. It cannot start during a switch or a mantle, and it ends a sprint.
- An enemy within 3.2 m (feet to feet) and 45 degrees of the view is lunged at: the player is thrown at them at 14 m/s, stops 0.9 m short, and strikes on arrival. Without one, the strike reaches 1.5 m in a 70-degree cone either side.
- The client picks the lunge target from what it draws and sends the offset with the input, so prediction and the server move the attacker the same way. The server drops a lunge that no enemy (rewound to the attacker's view, as for shots) stands within 1 m of, so it cannot be used as a dash. The strike itself is checked like a shot: against rewound players, with 0.5 m of extra reach, and never through a wall.
- In Zombies the knife does the same 150 damage (one strike kills in round 1) and a knife kill pays 130 points. In Gun Game a knife kill sets the victim back a weapon and does not move the killer up.
- Bots knife enemies that come within about 2.5 m in front of them.

## Equipment

The frag grenade is the lethal: G, or RB on a gamepad. Rules are in `rust/sim/src/equipment.rs`; the throw is part of the predicted weapon state (`Action::Throwing`).

- A press pulls the pin and lowers the weapon. The fuse (3 s) burns from the pin. Holding the button cooks the grenade in hand; letting go throws it overhand after the pin is out (0.3 s) and a 0.15 s wind-up. A grenade held past its fuse goes off in the hand. The weapon is back up 0.45 s after the release. A throw cuts a reload or a bolt or pump cycle short like the knife, and cannot start during a switch, a knife strike, or a mantle. Fire, aim, switches, and pickups wait for it.
- The grenade leaves the hand at 21 m/s, 9° above the view, plus the thrower's velocity, and falls at 14 m/s²: a level throw first lands about 10 m out, a high one about 30 m. It bounces off walls and floors (32% of the speed into a surface back, 70% along it), rolls when it meets the ground slower than 1.4 m/s (slowing at 3.5 m/s²), and settles. It passes through players and zombies.
- The blast does 150 damage within 3 m, falling to 20 at 8 m: it kills a full-health player out to about 5 m. A body is hit only if the blast can see its shins, chest, or head. No friendly fire, but a thrower's own grenade hurts them; a kill counts as the thrower's, and the kill feed shows "Frag".
- A player killed with the pin out drops the grenade, live, at their feet.
- The throw is drawn with the support hand while the firing hand keeps the weapon, dipped (`rust/game/src/grenade_hold.rs`): the hand brings the grenade up, the pin comes out, the arm cocks back and waits there while the grenade cooks, then comes over and lets go, follows through, and returns to the handguard. Other players see the same throw: snapshots carry how far along it is (`PlayerView::throw`), and the soldier's support arm follows the same path in third person (`ActionPose::Throw`). The grenade leaves the hand at the moment the simulation releases it; the thrown grenade has no pin or spoon.
- A blast rattles the camera (the explosion effect's `FpsPlayer.blast_shake`), vibrates a controller out to 25 m, harder up close, and within 9 m rings in the ears for up to 2 s (a generated tone, `scripts/equipment/ear_ring.gd`). The HUD shows the frags carried above the weapon name, and a frag marker round the crosshair, pointing at each live grenade within 8 m, that pulses faster and redder the closer it is (`scripts/ui/grenade_danger.gd`).
- Counts: one frag a life in Team Deathmatch, Free-for-all, Capture the Flag, Hardpoint, and Search and Destroy (so one a round there), refilled at each spawn; none in Gun Game, which hands out guns only. Zombies starts with two and gives two more at the start of each later round, up to four (Black Ops). The practice arena gives one a life; the T refill does not give grenades.
- Online the server flies every grenade and decides the damage; snapshots carry each live grenade, and a blast event sets off the effect. The thrower's client flies its own throw from the release, through the same rules and the same map, and hands over when the server's grenade arrives.

## First-person animation

The viewmodel (`rust/game/src/viewmodel.rs`, clips in `weapon_clips.rs`) follows the animation set of Call of Duty's weapon files (Black Ops 2's 88 `WEAP_ANIM_*` slots and the offsets the engine blends with them: sprint, slide, mantle, stance, strafe, and damage kick). Each is procedural, timed from the simulation so the clip ends exactly when the rules allow the next action.

- **Slide.** The weapon cants left about 10 degrees, tips up a touch, and pulls in, sprung in with a small overshoot and a thump as the body lands, then eased out; much less when aimed.
- **Mantle and vault.** The weapon tucks down and right while the support hand leaves it to plant palm-down on the ledge (lower on a vault's cover), pressing down as the body rises; it comes back up over the sprint-out, as the sprint pose does.
- **Stance and movement.** Crouching eases the weapon down, in, and right (Black Ops 2's `vDuckedOfs`/`vDuckedRot` at `posMoveRate` 6) instead of snapping. Walking pulls it back a touch and tips it (`vStandMove`/`vStandRot`). Strafing leans it into the move by up to 5 degrees (`vStrafeRot` is 6).
- **Damage kick.** A hit knocks the weapon away from the attacker, out over 100 ms and back over 400 ms (Black Ops 2's `DamageKickAngles`), up to about 4 degrees for a rifle round. Unlike Black Ops 2, it is mostly at the hip: aimed, the sights stay on the bullet path, and the simulation's flinch moves the aim instead.
- **Sprint.** A long gun runs canted muzzle-down across the lower frame with its stock clear of the eye (`SPRINT_LONG_POS`/`SPRINT_LONG_ROT`).
- Still to come from the same list: quick and empty raises and drops (for perks and a locked slide), prone and crawl, and a mounted low-ready.

To review them, the weapon-feel capture has a scenario for each (`first_raise`, `first_raise_bolt`, `first_raise_pump`, `throw`, `throw_quick`, `slide`, `crouch`, `vault`, `mantle`, `hit`); see `godot/tests/capture_weapon_feel.gd`.

## Damage zones

The head uses `damage.headshot`. Other zones use the weapon's `zones` (`stomach`, `arm`, `leg`; default 1, 0.9, 0.85). The M40 kills in one shot only to the head or upper chest (stomach 0.9, arms 0.75, legs 0.7); the PDW-9 is forgiving on limbs.

## Time to kill

Chest hits on a 100-health player with the trigger at the weapon's maximum rate, from `cargo test -p shooter-sim ttk_table -- --nocapture` (`rust/sim/src/weapon/ttk.rs`). Shotgun pellets land in proportion to a 0.25 m torso disc inside the pellet cone, and nothing lands past a weapon's `range` (-). Shots to kill in brackets.

| weapon | 5 m | 10 m | 15 m | 20 m | 30 m | 40 m | 50 m | 70 m |
|---|---|---|---|---|---|---|---|---|
| ak47 | 200 ms (3) | 300 ms (4) | 300 ms (4) | 300 ms (4) | 300 ms (4) | 300 ms (4) | 400 ms (5) | 400 ms (5) |
| br3 | 150 ms (3) | 385 ms (4) | 385 ms (4) | 385 ms (4) | 385 ms (4) | 385 ms (4) | 385 ms (4) | 385 ms (4) |
| lsw6 | 277 ms (4) | 277 ms (4) | 277 ms (4) | 277 ms (4) | 277 ms (4) | 277 ms (4) | 369 ms (5) | 369 ms (5) |
| m1911 | 300 ms (3) | 300 ms (3) | 300 ms (3) | 450 ms (4) | 450 ms (4) | 450 ms (4) | 450 ms (4) | 450 ms (4) |
| m4 | 240 ms (4) | 240 ms (4) | 240 ms (4) | 240 ms (4) | 240 ms (4) | 320 ms (5) | 320 ms (5) | 320 ms (5) |
| m40 | 0 ms (1) | 0 ms (1) | 0 ms (1) | 0 ms (1) | 0 ms (1) | 0 ms (1) | 0 ms (1) | 0 ms (1) |
| m870 | 0 ms (1) | 550 ms (2) | 1100 ms (3) | 2750 ms (6) | 9350 ms (18) | 17050 ms (32) | 26400 ms (49) | - |
| pdw9 | 180 ms (4) | 180 ms (4) | 180 ms (4) | 240 ms (5) | 420 ms (8) | 420 ms (8) | 420 ms (8) | 420 ms (8) |

Identity by class:

| Weapon | Role | Handling (ADS / sprint-out, s) |
|---|---|---|
| M4A1 | All-rounder: 4 hits to 30 m, 5 beyond | 0.24 / 0.22 |
| AK-47 | Hits harder up close (3 to about 9 m), falls off further (5 past 42 m), kicks more and wanders | 0.27 / 0.25 |
| BR-3 | One burst kills point-blank (to about 6 m); a second burst beyond | 0.25 / 0.23 |
| LSW-6 | Four hits to about 49 m, five beyond; 75 rounds, heavy kick, slow everything | 0.42 / 0.38 |
| PDW-9 | Wins from 7 to 15 m against everything (1000 rpm), falls off past 20 m, quickest handling | 0.19 / 0.16 |
| M870 | Ten pellets: one shot to about 9 m, two to 12 m; 0.55 s pump | 0.24 / 0.21 |
| M40 | One shot to head or upper chest at any range | 0.42 / 0.36 |
| M1911 | Sidearm: 3 hits to 15 m | 0.16 / 0.12 |
