---
name: cod-design
description: Design every in-scope feature the way Call of Duty does it (SCOPE.md lists what is in). Use before building or changing any in-scope player-facing feature or the system behind it (movement, gunplay, weapons, equipment, health, modes, spawns, scoring, medals, streaks, bots, HUD, menus, settings, controls, audio, graphics and visual quality, netcode feel, progression), without being asked. Finds the Call of Duty precedent, pins down its rules, numbers, inputs, and edge cases, follows it unless the user or a recorded project decision says otherwise, and records the reference in docs and the PR.
---

# Design it like Call of Duty

This game is a Call of Duty-style multiplayer shooter. Players arrive with years
of Call of Duty muscle memory, and the user wants every feature to meet it.
"What would Call of Duty do?" is the first design question for every feature,
not only the obvious ones: a settings slider, a kill feed line, a reload cancel,
a lobby countdown, and a sound cue all have a Call of Duty answer.

Use this skill before you write code for any in-scope feature or behaviour
change a player can see, hear, or feel, and for the systems behind them. It
tells you how a feature should work, not which features to add: check
[SCOPE.md](../../../SCOPE.md) first, and do not build a Call of Duty feature
the scope leaves out (or cut on purpose) unless the user asks for it. Never
copy Call of Duty's maps, names, art, or audio; match its rules and feel. Skip it only
for work no player touches: CI, build scripts, refactors that keep behaviour,
and tooling. When unsure, use it.

## Order of authority

1. **What the user asked for in this task.** If it departs from Call of Duty,
   build what they asked and say in one line how Call of Duty differs.
2. **Recorded project decisions.** The decisions table and "Arcade feel rules"
   in [docs/roadmap.md](../../../docs/roadmap.md), and the area docs
   ([weapons.md](../../../docs/weapons.md), [audio.md](../../../docs/audio.md),
   [art-direction.md](../../../docs/art-direction.md),
   [map-design.md](../../../docs/map-design.md),
   [spawns.md](../../../docs/spawns.md)). Some are deliberate choices
   between Call of Duty eras (toggle sprint with no tactical sprint, a 120 Hz
   tick). Do not undo them to match a different game.
3. **Call of Duty's standard behaviour,** found with the steps below.

Never invent a twist on a core Call of Duty rule. If a variation seems better,
build the standard version and offer the variation to the user as a question.
The user has rejected twists before (Search and Destroy's defuse).

## Steps

### 1. Find the precedent

Name the Call of Duty games that have the feature and how each one does it.
Start from [references/patterns.md](references/patterns.md), which lists the
conventions this project follows by area, then confirm the details:

- Search the web for the specifics (the Call of Duty wiki, patch notes,
  developer blogs, frame-by-frame breakdowns, and community data sites that
  measure timings and damage). Use `WebSearch`; prefer sources that measured
  over sources that remember.
- For layout and level questions, use the [map-design](../map-design/SKILL.md)
  skill and its local minimap references.
- For HUD and menu look, the user's references are in
  `~/.cache/shooter-ui-refresh/references/` when present.

If the games disagree, prefer, in order: what this project already chose for
a neighbouring feature, then the modern era (Modern Warfare 2019 onward and
Black Ops 6), then the classic era (Call of Duty 4 through Black Ops 3). If the
choice changes how the game plays and no project decision covers it (for
example killstreaks against scorestreaks), ask the user, showing two or three
options with the games each comes from and your recommendation.

### 2. Pin down the whole behaviour

A feature is not Call of Duty-like because its headline matches. Write down,
before coding:

- **Rules and numbers:** durations, damage, distances, speeds, scores, limits,
  cooldowns, with the source of each. Approximate is fine when sources only
  measure roughly; say so.
- **Inputs:** the default keyboard and mouse binding and the default controller
  binding, tap against hold, and toggle against hold where Call of Duty offers
  both.
- **Interactions and edge cases:** what cancels it, what it cancels, what
  happens during a sprint, a reload, a switch, a mantle, ADS, in the air, while
  dead, at round end, and when the player disconnects.
- **Feedback:** what the player sees (HUD element and where it sits, screen
  effect, viewmodel motion), hears (cue, its mix priority, whether enemies hear
  it too), and feels (rumble).
- **Options:** which settings Call of Duty exposes for it and their defaults.
- **Multiplayer and bots:** what the server decides, what the client predicts,
  what other players see and hear, and how bots use it.

Missing any of these is how features end up feeling "off" in playtests.

### 3. Build it to match

- Follow the project's split: rules in `rust/sim` (deterministic, shared by
  client and server), presentation in `rust/game` and GDScript, data in
  `godot/data/`. See [TODO.md](../../../TODO.md) conventions.
- Put the Call of Duty numbers in data or named constants with a comment that
  says where they came from, so a later tune knows what it is moving away from.
- Write tests for the rules and the edge cases from step 2.

### 4. Check it against the reference

Compare the result with what you wrote in step 2: measure timings in a test or
a capture, read the numbers back from the running game, and take screenshots
for anything visual (see the `run` skill and the GPU capture notes in
[docs/performance.md](../../../docs/performance.md)). Fix the differences, or
record why a difference is intended.

### 5. Record the reference

- Add or update the rule in the area doc (or `docs/roadmap.md` for a new
  decision), with the Call of Duty behaviour it follows and any intended
  difference.
- In the PR body, add a **Call of Duty reference** section: the games and
  behaviour followed, the key numbers with sources, and any deliberate
  difference with its reason.

## Graphics

Visual work has its own target: a Treyarch game in the line of Black Ops 2 and
Black Ops 3 (bold colour, strong key light, clean readable shapes, punchy
effects), finished with the best of the Modern Warfare games from 2019 on
(photoreal materials and wear, grounded bounce light, detailed weapons, dense
dressing, effects that linger). Read
[references/graphics.md](references/graphics.md) before any change to maps,
materials, light, grade, post-processing, weapons, characters, effects, or the
graphics presets. It says what to take from each, what to leave out, where
each part is set, how presets trade it against the performance budget, and how
to check a change against references.

## Take the design, never the property

Mechanics, rules, numbers, input conventions, layout principles, and UX
patterns are what we take. Everything that makes a specific game recognisable
is off limits:

- No Activision art, icons, emblems, sounds, music, voice lines, fonts, models,
  maps, or text in the repository, a PR, or a build. Reference screenshots and
  minimaps stay in local caches and are described, not attached.
- No game, map, perk, streak, character, or weapon names that belong to Call of
  Duty, even renamed lightly. Generic military and genre terms (UAV, care
  package, frag, Team Deathmatch, Search and Destroy, killcam, headshot) are
  fine. Our weapons keep their own names (M4A1, PDW-9, LSW-6).
- No map, HUD, or menu copied one to one, even recoloured. Build our own in
  our art direction ([art-direction.md](../../../docs/art-direction.md), the
  calm translucent HUD style).
