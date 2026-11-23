# Call of Duty patterns by area

The conventions this project follows, area by area, with what is already built
and where its rules live. Use this as the starting point for step 1 of the
skill, then confirm specifics (especially numbers) from a source before you
hard-code them. Numbers here marked "about" are rough on purpose.

When you settle a new pattern, or the project picks a side where the games
disagree, add it here in the same style.

## Movement

- Walk, toggle sprint, toggle crouch, slide from a sprint, jump, mantle. This
  project uses classic toggle sprint with **no tactical sprint**, and **no
  prone** until there is a crawl animation (docs/roadmap.md, TODO.md).
- Sprint lowers the weapon; firing or aiming out of a sprint waits a short
  sprint-out time per weapon class (SMGs fastest, LMGs and snipers slowest).
- Slide: a short burst from a sprint into crouch, with a camera dip; jump
  during a slide cancels it (slide-cancel). Landing from height dips the view;
  long falls hurt.
- Forgiving input: jump buffering and coyote time, fast stops and turns.
- ADS slows movement by weapon class. No stamina, no inertia that fights the
  player's input.
- Rules: `rust/sim/src/movement.rs`.

## Gunplay

- Hitscan bullets at multiplayer ranges, with damage falloff by range bands per
  weapon and hit-zone multipliers (head highest, limbs lowest). Penetration
  through thin surfaces (`penetration.rs`).
- Time-to-kill about 200 to 300 ms at a weapon's best range; each class owns a
  range (SMG close, AR mid, LMG mid to long and slow handling, sniper one-shot
  to the upper body with a bolt).
- Recoil: learnable per-weapon pattern, mostly vertical climb the player pulls
  against, small random variation; the sights always show where the round goes.
- Hip-fire spread that blooms while firing; ADS is near-perfect on rifles and
  SMGs. Scopes sway; hold breath steadies them for a few seconds.
- Reloads: tactical (round chambered, faster) and empty (slower, ends with a
  bolt release or charge). Rounds count once the magazine seats. Swapping
  weapons cancels a reload before that point.
- Weapon switch with a short holster and draw; quickdraw-style handling as a
  stat. Pick up a dropped weapon with a hold of the interact key, swapping for
  the one in hand.
- Melee on its own button with a lunge toward a close enemy; a hit kills.
- Rules: `rust/sim/src/weapon/`, numbers: `godot/data/weapons/*.ron`, write-up:
  docs/weapons.md.

## Health and damage

- 100 health in core modes, regeneration after a few seconds without damage,
  no health packs. Hardcore (if added): low health, minimal HUD, friendly fire.
- Damage feedback: red edges, a direction indicator toward the shooter, the
  picture draining colour at low health, a heartbeat and hurt breathing. Never
  so strong it hides the screen centre.
- Death: a short death camera (2 s), then a killcam showing the killer's view
  (2.5 s before the kill to 2 s after, skippable with the use key), then
  respawn. The kill that ends a match, or wins a Search and Destroy round,
  plays for everyone as a final killcam from 5 s before, slowed around the
  kill (`rust/sim/src/killcam.rs`).

## Hit and kill feedback

- Hit markers on every hit, a distinct marker and sound for headshots, and a
  larger, red kill marker. A short kill sound distinct from the hit sound.
- "+N" score popups near the crosshair, medal banners for named feats, a kill
  feed line per kill (killer, weapon, victim, headshot mark).
- Rules: `rust/sim/src/score.rs`, docs/audio.md for the cues.

## Scoring, medals, and streaks

- Kills score a base amount, with bonuses for headshots, longshots, point blank,
  multi-kills (double, triple, and up), streaks, revenge, saving a teammate,
  first blood, ending an enemy's streak, and objective play. Use plain
  descriptive medal names.
- Streaks are not built yet. Call of Duty has two systems: killstreaks (kills
  in one life, reset on death; the Modern Warfare games) and scorestreaks
  (score in one life; Black Ops). Ask the user which before building either.
  Generic rewards are fine (spy plane/UAV, counter-UAV, care package, air
  strike); their names and icons must be ours.

## Modes

Standard rules, from Call of Duty's core playlists. Built ones are in
`rust/sim/src/` and the README.

| Mode | Call of Duty rules | Here |
| --- | --- | --- |
| Team Deathmatch | 6v6, first to 75 kills or 10 minutes | `tdm.rs`, built |
| Free-for-all | every player for themselves, first to 30 kills | built (`--mode ffa`) |
| Search and Destroy | one life per round, attackers plant at A or B, defenders defuse; first to the round limit, sides swap at halftime | `snd.rs`, rounds built; the charge is next (TODO.md) |
| Hardpoint | one zone at a time, rotating about every 60 s, a point a second while held uncontested, to 250 | `hardpoint.rs`, built |
| Capture the Flag | take the enemy flag to your base while yours is home, halftime swap, overtime on a tie | `ctf.rs`, built |
| Gun Game | free-for-all up a weapon ladder; a kill advances, a melee kill sets the victim back a weapon | `gun_game.rs`, built without the melee setback |
| Domination | three flags (A, B, C), a point per flag held every few seconds, to about 200, halftime swap | TODO.md |
| Kill Confirmed | a kill scores only when a teammate collects the dropped tag; collecting your team's denies it | not built |
| Zombies | rounds of growing hordes, points for hits and kills spent on doors, weapons, and perks, barriers to repair | `zombies.rs`; straight-line chase and the two-hit rule are the user's choices |

Match flow: a pre-match lobby or warm-up, a countdown, the match with a clock
and score limit, a banner (VICTORY, DEFEAT, DRAW), the final killcam, then the
scoreboard and the next match counting down.

## Spawns

- Spawn away from enemies, out of their sightlines, and away from recent
  deaths, near teammates when it is safe. Objective modes spawn by side.
- Initial spawns are fixed per side; Search and Destroy never respawns.
- Write-up: docs/spawns.md.

## Loadouts and progression

- Create-a-Class: primary, secondary, lethal, tactical, perks, attachments
  (Gunsmith-style slots). Change at a spawn and in the Search and Destroy
  freeze. Today: primary pick per spawn, attachments, M1911 secondary.
- Progression (not built): XP from score, player levels unlocking weapons and
  equipment, weapon levels unlocking attachments, camo challenges, and an
  after-action report at match end.

## Equipment

- Lethals: frag (cookable, about a 3 to 4 second fuse, thrown on its own
  button), sticky grenade, claymore-style mine, remote charge.
- Tacticals: flashbang, stun, smoke.
- Each on its own button, carried in small counts, refilled on respawn.

## HUD

- Compact corner clusters, calm translucent plates (the user's taste; see the
  memory notes and docs/art-direction.md): minimap and compass in one corner,
  team score, clock, and winning state in another, ammo (magazine large,
  reserve small) with weapon name and equipment counts in a third.
- Centre: crosshair that opens with spread and hides in ADS, hit markers,
  interaction prompts ("Hold F to ...") with a fill ring, announcement banners.
- Kill feed at a screen edge; scoreboard on Tab or the controller's Select
  button with score, kills, deaths, assists, objective stats, and ping.
- Enemies appear on the minimap when they fire an unsuppressed weapon, and
  under a spy plane. Teammates always show.
- Objective markers with the distance, clamped to the screen edge when behind.

## Menus and settings

- Title: Play (mode list), Options, Quit. Match setup and a server browser.
  Pause is a full-screen page that never stops the game online.
- Options tabs in Call of Duty's order of importance: controller, keyboard and
  mouse, graphics, audio, interface. Each option has a one-line description.
- Mouse sensitivity, separate ADS sensitivity multiplier, invert Y, FOV slider
  (affects the world, with a separate ADS field-of-view setting), toggle or
  hold for sprint, crouch, and ADS.
- Controller: sensitivity per axis, deadzones (inner and outer per stick),
  response curve (standard, linear, dynamic), aim assist on or off and its type,
  button layouts and stick layouts (default, tactical, southpaw), vibration.
- Every menu works with a controller and a mouse.

## Controls

This project's bindings (`godot/project.godot`, `[input]`) follow Call of
Duty's defaults. Bind anything new the way Call of Duty does, after checking
the current default:

| Action | Keyboard and mouse | Controller |
| --- | --- | --- |
| Fire, aim | left, right mouse | right, left trigger |
| Jump, mantle | Space | A / Cross |
| Crouch, slide | C or Ctrl | B / Circle |
| Sprint | Shift | click left stick |
| Melee | V | click right stick |
| Reload | R | X / Square |
| Interact | F (hold) | not bound yet; Call of Duty holds X / Square (tap reloads, hold interacts) |
| Switch weapon | 1, 2, mouse wheel | Y / Triangle |
| Scoreboard | Tab | Back / View / touchpad |
| Lethal, tactical | G for the frag (lethal), Q for the tactical (flashbang or smoke) | RB for the lethal, LB for the tactical |

Toggle sprint and toggle crouch are this project's defaults.

## Audio

- Gunshots loud, bassy, compressed, with distance layers and indoor tails;
  enemies hear your shots at range; suppressors shrink that.
- Footsteps loud enough to track enemies by, directional, quieter for your own
  than others'; surface types; crouch walking quieter.
- Distinct cues for hit, headshot, kill, medals, objective events, match start
  and end, low health. A clear mix priority: enemy footsteps and gunfire above
  ambience and your own handling.
- Announcer lines for objective and streak events (TODO.md: once good CC0
  recordings exist). Write our own lines; never reuse Call of Duty's.
- Write-up: docs/audio.md. Audio changes are reviewed by the user from clips
  before they merge.

## Netcode feel

- Dedicated servers, server authority, client prediction of your own movement
  and shots, interpolation of others, favor-the-shooter lag compensation with a
  rewind cap. Hit markers predicted; kills and score from the server.
- Built: docs/roadmap.md phases 2 and 3, `rust/sim/src/net/`.

## Bots

- Bots fill empty slots and leave as players join, play the mode's objective,
  and have difficulty levels that change reaction time and aim, not rules.
- Rules: `rust/sim/src/bot.rs`.

## Graphics

- Treyarch's Black Ops 2 and 3 look, finished with the modern Modern Warfare
  games' craft. Full target, exclusions, and checks: [graphics.md](graphics.md).

## Maps

- Three-lane structure with loops, a central landmark, buildings you can enter,
  sightlines capped per lane. Use the map-design skill.
