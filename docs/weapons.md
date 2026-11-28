# Weapons

How firing, recoil, aiming, reloads, the knife, and grenades work, and the
numbers that set each weapon's identity. The rules live in
`rust/sim/src/weapon/` (the per-tick combat state in `mod.rs`, the weapon
files' shape and attachments in `def.rs`, time to kill in `ttk.rs`), the knife
in `rust/sim/src/melee.rs`, grenades and rockets in
`rust/sim/src/equipment.rs`, and the numbers in `godot/data/weapons/*.ron`
(times in seconds, angles in degrees, distances in metres). Client and server
run the same rules: spread, pellets, and recoil jitter are hashed from the
shot counter, so prediction and the server agree.

The reference is modern Call of Duty's gunplay (a recoil climb the player
pulls against, hip-fire bloom, sprint-out, a reload that can be cut once the
magazine seats) with Black Ops 2's weapon classes, roster roles, attachments,
and equipment.

## Roster

Players have 100 health. Every weapon file names its `class` (Black Ops 2's
Create-a-Class groups, which also order the class menu) and `unlock_level`
([create-a-class.md](create-a-class.md)). Damage falls linearly from `near` to
`far` between the two falloff distances; rounds stop at the weapon's `range`.

| Weapon | Class | Fire | RPM | Damage (falloff m) | Head x | Mag / reserve | Reload (s) | Move | Attachments | Level |
|---|---|---|---|---|---|---|---|---|---|---|
| M4A1 | Assault rifle | auto | 750 | 30 to 21 (25–50) | 1.4 | 30 / 120 | 2.1 / 2.6 | 1.0 | RD, AC, S, G, EM | 1 |
| AK-47 | Assault rifle | auto | 600 | 34 to 24 (7–45) | 1.4 | 30 / 120 | 2.4 / 2.9 | 0.97 | RD, AC, S, G, EM | 4 |
| KR-12 | Assault rifle | auto | 540 | 40 to 30 (25–50) | 1.35 | 20 / 100 | 2.5 / 3.0 | 0.95 | RD, AC, S, G, EM | 31 |
| BR-3 | Assault rifle | 3-round burst | 800 | 34 to 25 (4–40) | 1.4 | 30 / 120 | 2.3 / 2.8 | 0.98 | AC, S, G, EM | 12 |
| PDW-9 | SMG | auto | 1000 | 25 to 14 (15–30) | 1.3 | 32 / 128 | 1.9 / 2.3 | 1.05 | AC, S, G, EM | 1 |
| CQ-10 | SMG | auto | 1150 | 33 to 14 (6–22) | 1.1 | 32 / 128 | 1.95 / 2.35 | 1.05 | S, G, EM | 28 |
| LSW-6 | LMG | auto | 650 | 30 to 22 (30–60) | 1.35 | 75 / 225 | 4.8 / 5.6 | 0.88 | RD, AC, S, G, EM | 16 |
| M870 | Shotgun | pump (0.55 s cycle) | 600 | 10 pellets x 18 to 10 (10–30) | 1.2 | 6 / 30 | 0.45 + 0.5 a shell + 0.45 | 0.97 | S, G, EM | 8 |
| SG-12 | Shotgun | semi | 450 | 8 pellets x 24 to 9 (5–15) | 1.2 | 8 / 32 | 2.4 / 2.9 | 0.97 | RD, S, G, EM | 34 |
| DM-7 | Marksman | semi | 300 | 70 to 55 (30–70) | 1.5 | 10 / 50 | 2.6 / 3.1 | 0.95 | S, G, EM | 37 |
| M40 | Sniper | bolt (1.05 s cycle) | 600 | 100 | 1.5 | 5 / 25 | 3.0 / 3.6 | 0.92 | S, EM | 22 |
| M1911 | Pistol | semi | 400 | 40 to 25 (10–30) | 1.4 | 8 / 32 | 1.5 / 1.9 | 1.05 | none | 1 |
| BP-9 | Pistol | 3-round burst | 1150 | 35 to 22 (9–25) | 1.3 | 30 / 90 | 1.7 / 2.1 | 1.05 | (RD, S, EM) | 13 |
| RL-4 | Launcher | single rocket | 60 | rocket blast | | 1 / 2 | 3.2 | 0.9 | none | 19 |

Reload is tactical / empty. Attachments: RD red dot, AC ACOG, S suppressor, G
grip, EM extended magazine. The BP-9's file lists three attachments, but a
class gives a sidearm no attachment slots, so the class editor cannot fit them
([create-a-class.md](create-a-class.md)). The BR-3, PDW-9, and CQ-10 carry
their own reflex sights, so they take no red dot.

What each one is for (breakpoints worked out from the files):

| Weapon | Role | ADS / sprint-out (s) |
|---|---|---|
| M4A1 | All-rounder: four hits to about 39 m, five beyond | 0.24 / 0.22 |
| AK-47 | Hits harder up close (three to about 9 m), falls off further (five past about 41 m), kicks more and wanders | 0.27 / 0.25 |
| KR-12 | Battle rifle: three hits to about 42 m, four beyond, at 540 rpm; slower than the AK-47 point-blank, faster than either rifle from 10 m out. Pays with a 20-round magazine, the heaviest straight climb of the rifles, and slower handling. Black Ops 2's SCAR-H (625 rpm, three to four hits). The AK-47 model with its furniture refinished as black polymer | 0.29 / 0.27 |
| BR-3 | One burst kills point-blank (to about 6.5 m); a second burst beyond | 0.25 / 0.23 |
| PDW-9 | Four hits (180 ms) to 15 m, five to about 22 m, eight from 30 m; forgiving on limbs | 0.19 / 0.16 |
| CQ-10 | The fastest fire (1150 rpm): four hits to about 12.5 m, beating the PDW-9 there, then seven or eight; a hard, jumpy climb. Black Ops 2's Skorpion EVO (33 damage to 10 m, 22 to 21 m, 18 beyond, no headshot bonus) | 0.17 / 0.14 |
| LSW-6 | Four hits to about 49 m, five beyond; 75 rounds, heavy kick, slow everything | 0.42 / 0.38 |
| M870 | Ten pellets: one shot to about 9.5 m, two to about 13 m; 0.55 s pump, loads shell by shell | 0.24 / 0.21 |
| SG-12 | Semi-automatic: eight pellets at 450 rpm, one shot to about 7.5 m, two (133 ms apart) to about 10 m, nothing past 30 m; an eight-round box magazine, reloaded like a rifle's. Black Ops 2's S12 (eight pellets, 30 damage to 3.75 m, 20 to 7.5 m, nothing past 15 m, 468 rpm). Ghost-ring sights | 0.26 / 0.22 |
| DM-7 | Marksman: two hits to the chest anywhere and never one, a headshot kills to about 39 m; a 2.5x scope (`ads_fov_scale` 0.45) with light sway; 10 rounds. Black Ops 2's SVU-AS (70 damage, two to kill, a headshot one, about 415 rpm); ours caps at 300 rpm so a pair is 200 ms. The M14 with a riflescope and an olive glass-fibre stock | 0.34 / 0.30 |
| M40 | One shot to head or chest at any range (stomach 0.9, arms 0.75, legs 0.7) | 0.42 / 0.36 |
| M1911 | Sidearm: three hits to about 19 m, four beyond | 0.16 / 0.12 |
| BP-9 | Burst sidearm: three rounds a pull at 1150 rpm with a 0.12 s pause, one burst kills to about 11 m, two beyond. Black Ops 2's B23R (a three-round burst at about 1145 rpm). Three-dot sights | 0.17 / 0.12 |
| RL-4 | Launcher: a rocket a pull, one loaded and two spare; a kill within about 4.1 m of the blast, a direct hit through Flak Jacket, and one hit downs any scorestreak aircraft (see [Launchers](#launchers)). Black Ops 2's RPG | 0.45 / 0.38 |

## Attachments

Black Ops 2's attachments, each a point in a class. Every combination a weapon
offers is its own weapon-table variant, built at load by applying each
attachment's changes in turn (`m4+reddot+grip`; `Attachment::apply` and
`variants_of` in `weapon/def.rs`): up to three attachments, one optic at most.
The first- and third-person models show every part fitted
(`WeaponView::set_attachments`).

| Attachment | Effect |
| --- | --- |
| Red Dot | a reflex sight in place of the irons; aims 5% faster |
| ACOG | a magnified prism sight seen through its own eyepiece with a lit chevron (the view zooms to `ads_fov_scale` x 0.55, at most 0.45); aims 0.06 s slower, moves 10% slower aimed |
| Suppressor | no muzzle flash or tracers, quieter, and firing does not show the shooter on enemy minimaps (`radar.rs`); damage falloff starts at 70% and ends at 75% of the distance |
| Grip | a quarter less kick (pattern and jitter) |
| Extended magazine | half again as many rounds (rounded); a magazine reload takes 0.2 s longer and seats 0.15 s later |

Attachments unlock by kills with the weapon ([create-a-class.md](create-a-class.md)).

## Recoil

Each shot kicks the view by the weapon's `recoil.pattern` entry for that shot
of the spray (the last entry repeats), plus hashed variation
(`recoil.jitter`), scaled down while aimed (`ads_scale`), crouched or prone
(`crouch_scale`, 0.85 by default), and mounted on cover (`mount::KICK_SCALE`,
0.35; see [movement.md](movement.md)). The pattern starts over once the
trigger has rested a moment. The kick splits in two:

- **Climb** (`1 - kick_return` of it) turns the view and stays, as if the
  mouse had moved. The simulation reports it with each `CombatEvent::Fired`,
  and the shooter's client turns its look by it over a few hundredths of a
  second (`CLIMB_RATE` in `player.rs`). Pulling down against a spray holds the
  aim where the player puts it, and nothing drifts back after the trigger is
  let go.
- **Snap** (`kick_return`) is an aim offset that settles back at
  `recovery_rate` once `recovery_delay` has passed since the shot. Automatics
  return 45%, a visible bounce that settles before the next round; pistols,
  shotguns, the marksman rifle, the sniper, and the launcher return 55–70%, so
  a single shot kicks hard and settles near the target.

Patterns start light and build over the first five or six rounds, so tapping
is rewarded over holding, and each weapon drifts its own way: the AK-47 climbs
hard and hooks right on rounds 2–5 then left, the M4A1 leans right then swings
back left, the KR-12 climbs hardest of the rifles, the LSW-6 bounces side to
side, the PDW-9 jitters.

Both parts are aim, and the camera looks along the aim, so the screen centre
and the sights are always where the next round goes. The per-shot camera
punch (`camera_punch` on each `WeaponView`) only rolls the view and pulses the
zoom slightly; it never moves the centre.

## Spread and sway

- Hip cone: `spread.hip`, opening to `hip_moving` with speed and to `air` in
  the air; 0.8 x crouched, 0.6 x prone.
- Hip fire blooms: each shot adds `spread.bloom` degrees up to `bloom_max`;
  the bloom holds through a spray and shrinks at `bloom_recovery` (6 degrees a
  second by default) once the trigger rests.
- Aimed: `spread.ads` (0 for the M4A1, BR-3, PDW-9, CQ-10, and every scoped or
  pellet weapon; 0.05 to 0.1 for the AK-47, KR-12, LSW-6, and pistols).
- Every weapon drifts slightly while aimed (`spread.ads_sway`, 0.12–0.15° for
  most guns),
  0.4 x prone and 0.15 x mounted. Scopes (`scope` in the file: the DM-7 and
  M40) sway more, and holding breath (the sprint button while scoped) steadies
  them for `hold_breath` seconds (4 s), after which the player is winded for
  `breath_recovery` (2.5 s). Sway is part of the aim, so the sights show it
  truthfully.

## Aiming, reloads, and switches

- Aiming takes `ads_time` and works through a reload and a bolt or pump
  cycle, but not a switch. The aimed amount is eased (`ads_ease`), and the
  spread, kick scale, sway, zoom, move speed, and weapon model all follow the
  same curve.
- A magazine reload takes `time` (tactical) or `empty_time` (empty, which ends
  with the bolt release or charge). Its rounds count at `mag_in` seconds, when
  the fresh magazine seats; a reload cut short after that keeps them. A
  shotgun's per-round reload loads shell by shell, and firing interrupts it.
- Once the weapon is back on the shoulder (`WeaponDef::reload_ready_at`: 0.45 s
  after the seat for a tactical reload, 0.8 s for an empty one, the very end
  of a bolt action's empty reload), firing ends the reload and fires: the rest
  is only the settle into the grip. The first-person clip is back on the
  shoulder at exactly that moment and eases out when cut.
- Sprinting does not cancel a reload, and a reload can start while sprinting.
  Firing or aiming ends a sprint, and the first shot waits the weapon's
  `sprint_out_time`.
- A switch (the weapon-switch button, the mouse wheel, or 1 and 2) lowers over
  `lower_time` and raises over `raise_time`. A switch that cuts off a bolt or
  pump cycle, or an empty reload before its charge, leaves the weapon needing
  its action worked (`WeaponState::needs_cycle`): it cycles when drawn again,
  so switching never skips a cycle. Re-selecting the weapon in hand while it
  is being put away brings it back up from as far as it got.
- **First raise.** A weapon's first draw of a game or round, and a weapon
  picked up, comes up with a first raise (`Raise::First`, `first_raise_time`):
  the draw, then the action worked (a charging handle, a pistol's slide
  racked, the bolt or the pump worked), and it cannot fire until that ends.
  Later draws, and every respawn after the first, are ordinary draws. This
  follows Call of Duty's `firstRaiseAnim`, with Modern Warfare III's rule (it
  plays at the start of a game or round and on a pickup, not on every life).
  Times are Black Ops 2's for each class scaled to our faster draws: M4A1 and
  BR-3 1.1 s, AK-47 1.2 s, KR-12 1.25 s, PDW-9 0.8 s, CQ-10 0.75 s, LSW-6
  1.7 s, M870 1.35 s, SG-12 1.3 s, DM-7 1.3 s, M40 1.4 s, M1911 0.8 s, BP-9
  0.85 s, RL-4 1.3 s. A switch away during a first raise ends it.
- A burst in progress is dropped by a reload, a sprint, or a switch.
- Below a quarter of a magazine the weapon clicks and the HUD warns of low
  ammo (`WeaponDef::low_ammo`; a launcher only when empty).
- Inspect (I, or D-pad down) plays the weapon class's own inspect; any action
  cancels it.
- Perks speed handling: Sleight of Hand halves reloads, Fast Hands halves
  switches, Dexterity halves the sprint-out ([create-a-class.md](create-a-class.md)).

## Damage zones

The head uses `damage.headshot`. The chest counts 1; other zones use the
weapon's `zones` (`stomach`, `arm`, `leg`; default 1, 0.9, 0.85). The M40 kills
in one shot only to the head or chest (stomach 0.9, arms 0.75, legs 0.7); the
DM-7 takes 0.8 on arms and 0.75 on legs; the PDW-9 is forgiving on limbs (1.0,
1.0, 0.95). The zones themselves are in [art-direction.md](art-direction.md).

## Penetration

Rounds go through thin cover (`rust/sim/src/penetration.rs`; the engine
measures each wall in `rust/game/src/penetration.rs`). Each weapon has a
`penetration` budget in metres of wood: 0.6 M40, 0.55 DM-7, 0.45 LSW-6, 0.42
KR-12, 0.36 AK-47, 0.32 BR-3, 0.3 M4A1, 0.16 PDW-9, 0.12 CQ-10 and M1911, 0.1
BP-9, 0.08 shotguns. Crossing a wall spends its thickness times the
material's resistance, read from the collider's `surface` tag:

| Material | Resistance | Damage kept (before the budget share) |
| --- | --- | --- |
| Glass | 0.4 | 0.95 |
| Wood | 1 | 0.8 |
| Concrete (and untagged) | 3 | 0.6 |
| Metal | 5 | 0.55 |

After a wall the round keeps the material's share times `1 - 0.5 x` the share
of the budget the wall used. A round stops in a wall that costs more than the
budget left, after two walls, or at a wall thicker than 0.6 m. Client and
server use the same rules, so local hits and the server's agree.

## Time to kill

Chest hits on a 100-health player with the trigger at the weapon's maximum
rate, from `cargo test -p shooter-sim ttk_table -- --nocapture`
(`rust/sim/src/weapon/ttk.rs`). Shotgun pellets land in proportion to a 0.25 m
torso disc inside the pellet cone, and nothing lands past a weapon's `range`
(-). Shots to kill in brackets.

| weapon | 5 m | 10 m | 15 m | 20 m | 30 m | 40 m | 50 m | 70 m |
|---|---|---|---|---|---|---|---|---|
| ak47 | 200 ms (3) | 300 ms (4) | 300 ms (4) | 300 ms (4) | 300 ms (4) | 300 ms (4) | 400 ms (5) | 400 ms (5) |
| bp9 | 104 ms (3) | 104 ms (3) | 277 ms (4) | 277 ms (4) | 329 ms (5) | 329 ms (5) | 329 ms (5) | 329 ms (5) |
| br3 | 150 ms (3) | 385 ms (4) | 385 ms (4) | 385 ms (4) | 385 ms (4) | 385 ms (4) | 385 ms (4) | 385 ms (4) |
| cq10 | 157 ms (4) | 157 ms (4) | 209 ms (5) | 313 ms (7) | 365 ms (8) | 365 ms (8) | 365 ms (8) | 365 ms (8) |
| dm7 | 200 ms (2) | 200 ms (2) | 200 ms (2) | 200 ms (2) | 200 ms (2) | 200 ms (2) | 200 ms (2) | 200 ms (2) |
| kr12 | 222 ms (3) | 222 ms (3) | 222 ms (3) | 222 ms (3) | 222 ms (3) | 222 ms (3) | 333 ms (4) | 333 ms (4) |
| lsw6 | 277 ms (4) | 277 ms (4) | 277 ms (4) | 277 ms (4) | 277 ms (4) | 277 ms (4) | 369 ms (5) | 369 ms (5) |
| m1911 | 300 ms (3) | 300 ms (3) | 300 ms (3) | 450 ms (4) | 450 ms (4) | 450 ms (4) | 450 ms (4) | 450 ms (4) |
| m4 | 240 ms (4) | 240 ms (4) | 240 ms (4) | 240 ms (4) | 240 ms (4) | 320 ms (5) | 320 ms (5) | 320 ms (5) |
| m40 | 0 ms (1) | 0 ms (1) | 0 ms (1) | 0 ms (1) | 0 ms (1) | 0 ms (1) | 0 ms (1) | 0 ms (1) |
| m870 | 0 ms (1) | 550 ms (2) | 1100 ms (3) | 2750 ms (6) | 9350 ms (18) | 17050 ms (32) | 26400 ms (49) | - |
| pdw9 | 180 ms (4) | 180 ms (4) | 180 ms (4) | 240 ms (5) | 420 ms (8) | 420 ms (8) | 420 ms (8) | 420 ms (8) |
| sg12 | 0 ms (1) | 133 ms (2) | 1067 ms (9) | 1867 ms (15) | 4267 ms (33) | - | - | - |

### Class bands

Each class has a band its chest time to kill and its handling must land in
(`ttk::band`); `cargo test -p shooter-sim every_weapon_lands_in_its_class_band`
fails for any weapon outside its band, so a new or retuned weapon keeps its
class's shape. The bands follow Black Ops 2's classes: assault rifles kill in
200–400 ms at every range (the SCAR-H's three hits at 625 rpm, the M8A1's one
burst point-blank); SMGs are faster than any rifle to about 15 m and slower
past 20 m (the Skorpion EVO's 33 damage to 10 m, 22 to 21 m, 18 beyond); LMGs
hold their kill to long range and handle slowest; shotguns kill in one or two
shots up close and not at all at range (the S12's 30 damage per pellet to
3.75 m, nothing past 15 m); marksman rifles take two shots to the chest
anywhere and never one (the SVU-AS's 70 damage); snipers one; pistols a
sidearm's few hits.

| Class | Chest TTK (ms) by range | ADS (s) | Sprint-out (s) | Move |
|---|---|---|---|---|
| Assault rifle | 5 m 120–320; 10–30 m 180–400; 40 m 180–420; 50–70 m 220–450 | 0.22–0.32 | 0.20–0.30 | 0.93–1.0 |
| SMG | 5 m 120–220; 10 m 120–250; 15 m 120–300; 20 m 150–350; 30 m 300–600; 50 m 300–800 | 0.15–0.22 | 0.12–0.20 | 1.0–1.05 |
| LMG | 5–40 m 220–400; 50–70 m 250–450 | 0.36–0.50 | 0.32–0.45 | 0.82–0.92 |
| Shotgun | 3–5 m 0–150; 10 m 0–600; 20 m 500 or more, or no kill | 0.20–0.30 | 0.18–0.26 | 0.95–1.0 |
| Marksman | 5–20 m 50–300; 50–70 m 50–400 (never one shot) | 0.30–0.40 | 0.26–0.36 | 0.92–0.98 |
| Sniper | one shot at 5–100 m | 0.38–0.50 | 0.32–0.42 | 0.88–0.95 |
| Pistol | 5 m 80–350; 15 m 100–450; 30 m 250–900 | 0.12–0.20 | 0.08–0.15 | 1.0–1.1 |
| Launcher | explosive: handling only | 0.35–0.60 | 0.30–0.50 | 0.85–0.95 |

## Melee

The knife has its own button (V, or R3 on a controller) and works with any
weapon in hand, as in modern Call of Duty. Aimed at mountable cover, the same
press mounts the weapon instead ([movement.md](movement.md)).

- A strike does 150 damage, a kill on a full-health player. It lands 0.1 s
  after the press and the weapon is back up 0.45 s after that. It cuts a
  reload or a bolt or pump cycle short, like a switch: a magazine already
  seated stays in, and a cut cycle is worked when the strike ends. It cannot
  start during a switch, a throw, or a mantle, and it ends a sprint.
- An enemy within 3.2 m (feet to feet) and 45° of the view is lunged at: the
  player is thrown at them at 14 m/s, stops 0.9 m short, and strikes on
  arrival. Without one, the strike reaches 1.5 m in a 70° cone either side.
  Prone, the knife swings without a lunge.
- The client picks the lunge target from what it draws and sends the offset
  with the input, so prediction and the server move the attacker the same
  way. The server drops a lunge that no enemy (rewound to the attacker's view,
  as for shots) stands within 1 m of, so it cannot be used as a dash. The
  strike itself is checked like a shot: against rewound players, with 0.5 m of
  extra reach, and never through a wall.
- In Gun Game a knife kill sets the victim back a weapon and does not move
  the killer up.
- The swing (`rust/game/src/knife.rs`): the support hand leaves the weapon,
  which drops down and right out of the way, and slashes a blackened knife
  (`tools/build_knife.py`) across the centre as the strike lands, following
  through past the target as Call of Duty's does. In third person the soldier
  stabs overhand with the support hand. A knife kill crumples the body where
  it stands (snapshots mark knife hits with their own force level,
  `HitPart::KNIFE_FORCE`). Online, our own knife strike drops the other
  player's body at once (`RemotePlayer::apply_melee`); the kill marker and
  points wait for the server, and if it never confirms the kill within a
  second the player stands again.

## Equipment

Black Ops 2's lethal and tactical slots. The frag is the lethal (G, or RB on a
controller); the tactical is a flashbang or smoke grenade (Q, or LB), picked
in the class ([create-a-class.md](create-a-class.md)). The throw is part of
the predicted weapon state (`Action::Throwing`).

- **Throw.** A press pulls the pin (0.3 s) and lowers the weapon. Letting go
  throws overhand after a 0.15 s wind-up; the weapon is back up 0.45 s after
  the release. A throw cuts a reload or a bolt or pump cycle short like the
  knife, and cannot start during a switch, a knife strike, or a mantle. Fire,
  aim, switches, and pickups wait for it. Fast Hands takes 60% of the wind-up
  and recovery; the fuse keeps real time.
- **Cooking.** The frag's 3 s fuse burns from the pin, so holding the button
  cooks it; a frag held past its fuse goes off in the hand. Tacticals do not
  cook: the fuse starts as they leave the hand (1.5 s flashbang, 1 s smoke).
- **Flight.** The grenade leaves the hand at 21 m/s, 9° above the view, plus
  the thrower's velocity, and falls at 14 m/s²: a level throw first lands
  about 10 m out, a high one about 30 m. It bounces off walls and floors (32%
  of the speed into a surface back, 70% along it), rolls when it meets the
  ground slower than 1.4 m/s (slowing at 3.5 m/s²), and settles. It passes
  through players.
- **Frag blast.** 150 damage within 3 m, falling to 20 at 8 m: a kill on a
  full-health player out to about 4.9 m. A body is hit only if the blast can
  see its shins, chest, or head. No friendly fire, but a thrower's own grenade
  hurts them; the kill is the thrower's and the feed shows "Frag". Flak Jacket
  takes 35%. A player killed with the pin out drops the grenade, live, at
  their feet.
- **Flashbang.** Hurts no one. Whoever can see it is blinded and deafened: the
  screen whites out, holds, and fades, and the ears ring, for 0.6 to 4 s by
  how hard it hits (`equipment::flash_amount`): full strength out to about
  7 m, nothing at 20 m, and only about a third of it with the flash straight
  behind. A wall in between stops it. Tactical Mask halves it; bots are
  blinded for 75% of a player's time.
- **Smoke.** Pops into a cloud that grows to 5 m across over 2 s, hangs for
  16 s, and thins away over the last 3 (`equipment::smoke_radius`). It blocks
  bots' sight (`equipment::smoke_blocks`), and inside it a grey fog covers the
  screen. Snapshots list every cloud still hanging, with its age, so a player
  who joins late sees it as it is.
- **Feel.** A blast rattles the camera (`FpsPlayer.blast_shake`), vibrates a
  controller out to 25 m, and within 9 m rings in the ears for up to 2 s
  (`godot/scripts/equipment/ear_ring.gd`). The HUD shows the grenades carried,
  and a marker round the crosshair points at each live frag within 8 m,
  pulsing faster and redder the closer it is
  (`godot/scripts/ui/grenade_danger.gd`).
- **Counts.** The class decides: one frag a life (two with the Extra Lethal
  wildcard, none with the slot empty) and one or two tacticals, refilled at
  each spawn (per round in Search and Destroy). Gun Game hands out guns only.
- **The throw drawn.** The support hand throws while the firing hand keeps the
  weapon, dipped (`rust/game/src/grenade_hold.rs`): the grenade comes up, the
  pin comes out, the arm cocks back and waits while it cooks, then comes over
  and lets go at the moment the simulation releases it. Other players see the
  same throw (`PlayerView::throw`, `ActionPose::Throw`), and the server's arm
  hit zones follow the same path (`shooter_sim::hitbox::action`), so a shot at
  a thrower's raised arm lands where the arm is drawn (`hitbox_fit_test`).
- **Online.** The server flies every grenade and decides the damage;
  snapshots carry each live grenade, and a blast event sets off the effect.
  The thrower's client flies its own throw from the release, through the same
  rules and the same map, and hands over when the server's grenade arrives.

Bots throw frags, flashbangs, and smoke and run from live frags through the
same buttons; see [bots.md](bots.md).

## Launchers

The RL-4 is Black Ops 2's RPG: a shoulder-fired tube in the sidearm slot
(`class: Launcher`), one rocket loaded and two spare. A weapon file with
`rocket: true` fires a rocket instead of a round; the rocket rides the
grenades' machinery as `equipment::Kind::Rocket` (flight, snapshots, blast,
and the drawn model).
It launches with real RPG recordings and its rocket roars in flight
([audio.md](audio.md#the-rl-4-toolsbuild_launcher_audiopy)).

- The rocket leaves the tube 0.6 m ahead of the eye (at the eye if a wall is
  closer) along the view, at 65 m/s, sagging at 1.5 m/s² (under 2 m by
  100 m). It does not carry the firer's movement.
- It goes off on whatever it meets: the map (and solid props) through the
  same sweep as a grenade, or a player whose body it passes within 0.4 m of
  (feet to 1.8 m; the firer excepted; `GrenadeSim::strike_bodies`). With
  nothing in the way it goes off in the air after 4 s, as Black Ops 2's
  rockets do at the end of their flight.
- The blast does 200 within 1.5 m, falling to 30 at 6 m, none beyond
  (`equipment::rocket_damage`): a kill within about 4.1 m. Flak Jacket lets
  35% through, except to the player the rocket struck: a direct hit kills
  through it (Black Ops 2: only a direct impact kills a Flak Jacket player). A
  body is hit only if the blast can see its shins, chest, or head. No friendly
  fire, but the firer's own rocket hurts them.
- A rocket passing within 6 m of an enemy scorestreak aircraft (UAV or
  Counter-UAV) brings it down in one hit,
  as Black Ops 2's `rocketdamageoneshot` does ([scorestreaks.md](scorestreaks.md)).
- The kill is the launcher's: the feed shows the RL-4, and the kill counts
  toward its kills.
- Online the server flies every rocket; the firer's client draws its own from
  the shot until the server's arrives, as with a throw.
- The reload (3.2 s, the rocket counted at 2.2 s) takes the tube off the
  shoulder, muzzle up, while the support hand slides the next rocket into the
  muzzle (`ClipKind::Launcher`). The carrier moves at 90% (Black Ops 2: 90%).
- The model (`tools/weapon_rl4.py`) is the project's own: a 40 mm tube with a
  venturi, olive heat guards, a pistol grip, flip-up sights, and an 85 mm
  warhead. The rocket in flight is the same model's `Magazine` node with a
  motor flame and smoke trail (`scenes/equipment/rocket.tscn`).
- It takes no attachments and has no class band beyond handling. One bot in
  four carries it as its sidearm and fires it only at enemy aircraft
  (`class::bot_class`).

## First-person animation

The viewmodel (`rust/game/src/viewmodel.rs`, clips in `weapon_clips.rs`)
follows the animation set of Call of Duty's weapon files (Black Ops 2's
`WEAP_ANIM_*` slots and the offsets the engine blends with them: sprint,
slide, mantle, stance, strafe, and damage kick). Each is procedural, timed
from the simulation so the clip ends exactly when the rules allow the next
action.

- **Hip and sprint.** At the hip a long gun sits in the lower right with its
  rear sight, receiver, and both hands in frame (the scenes' `hip_position`
  and `hip_rotation` plus `HIP_CLOSER` and `HIP_LONG_POS`/`HIP_LONG_ROT`), and
  it sprints muzzle-down across the body, still in the lower frame
  (`SPRINT_LONG_POS`/`SPRINT_LONG_ROT`), as modern Call of Duty viewmodels do.
- **Slide.** The weapon cants left about 10°, tips up a touch, and pulls in,
  sprung in with a small overshoot and a thump as the body lands, then eased
  out; much less when aimed.
- **Mantle and vault.** The weapon tucks down and right while the support hand
  plants palm-down on the ledge, pressing down as the body rises; it comes
  back up over the sprint-out.
- **Stance and movement.** Crouching eases the weapon down, in, and right
  (Black Ops 2's `vDuckedOfs`/`vDuckedRot` at `posMoveRate` 6). Walking pulls
  it back a touch and tips it by 1° (half of `vStandMove`/`vStandRot`).
  Walking and strafing keep the weapon steady in the frame, as Modern Warfare
  (2019 on) and Black Ops 6 hold it: the walking bob is about 4 mm and a third
  of a degree (`WALK_BOB_POS`/`WALK_BOB_ROT`), a strafe leans it 1.5°
  (`STRAFE_ROLL`; Black Ops 2's `vStrafeRot` is 6), and starting or stopping
  shoves it at most 1 cm (`MOVE_SHOVE`). The camera's head bob carries most of
  the walking motion; sprinting keeps its full running loop. Prone, the crawl
  rolls the weapon onto its side ([movement.md](movement.md)).
- **Damage kick.** A hit knocks the weapon away from the attacker, out over
  100 ms and back over 400 ms (Black Ops 2's `DamageKickAngles`), up to about
  4° for a rifle round. Unlike Black Ops 2, it is mostly at the hip: aimed,
  the sights stay on the bullet path, and the simulation's flinch moves the
  aim instead.
- **Hands on the weapon.** As in Call of Duty, each weapon has its own hand
  pose, with the fingers resting on its grip and handguard. Ours are fitted to
  each model and stored in its weapon file (`hands`, `hand_tuning`), and
  `hand_poses_test.gd` keeps the fingers out of the model; see
  [art-direction.md](art-direction.md).
- **Each gun's own reload.** As Call of Duty animates every weapon's own
  mechanism, the reload clips follow each model's real action
  (`weapon_clips::Feed`, `ClipKind::Pistol { knob }`):
  - CQ-10 (MAC pattern): the magazine comes down out of the pistol grip and
    goes straight back up it, the gun canted so the grip's heel faces the
    support hand, as Black Ops Cold War's MAC-10 is reloaded; empty, the hand
    draws the cocking knob on top.
  - BP-9 (Tec-9 pattern): turned to show its left side, the magazine ahead of
    the trigger guard changed there; empty, the support hand pulls the
    cocking knob on the left side (it has no slide). Its inspect eases that
    knob back to check the chamber.
  - KR-12: with rounds left, the AK speed reload (the fresh magazine knocks
    the old one out forward and rocks straight in); empty, the ordinary AK
    reload.
  - DM-7 (M14 pattern): the bolt locks open on the last round and stays open
    through the magazine change until the operating rod handle is drawn and
    let go.
  - SG-12 (Saiga pattern): the fat box is hooked in on its lip, hangs while
    the palm drops under it, and is slapped home.
  - RL-4: the tube is drawn back and tipped up so the muzzle comes within the
    support arm's reach, and the hand feeds the rocket in.
  - Frame strips (top to bottom: CQ-10, BP-9, KR-12, SG-12, RL-4):
    [new-gun-reloads.jpg](media/new-gun-reloads.jpg).

To review them, `godot/tests/capture_weapon_feel.gd` has a scenario for each
(among them `first_raise`, `first_raise_bolt`, `first_raise_pump`, `throw`,
`throw_quick`, `slide`, `crouch`, `crawl`, `vault`, `mantle`, `hit`, a
`reload_*` for every gun, and `inspect_*`), and `tools/plot_weapon_feel.py`
plots the traces against reference bands.
