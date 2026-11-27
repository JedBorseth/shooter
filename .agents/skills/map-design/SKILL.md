---
name: map-design
description: Design or redesign a multiplayer map's layout (Breakwater, a new map, a layout review). Study real Call of Duty minimaps, block the plan out with tools/map_blockout, measure it, show it as a minimap beside the references, and get the user's approval before building it.
---

# Map design

Use this before building a new multiplayer map, rebuilding a map's layout, or
judging whether a layout is good. The art can be polished later; a bad layout
cannot. In November 2026 the user rejected a long three-strip city layout
(Slamcity, since cut) and a first redesign drawn the same way, then approved a
plan built from real Call of Duty maps. Work the same way.

Breakwater is the game's one multiplayer map ([SCOPE.md](../../../SCOPE.md));
the Testing Area is for practice. A new or second map needs the owner's yes
first.

## 1. Study the references

The user wants maps that play like real Call of Duty maps. Look at real ones
first, and say what you take from each.

- Local minimaps: `~/.local/share/shooter/map-references/cod-minimaps/`.
  `by-game/<game>/<Map>.png` holds about 285 multiplayer minimaps (CoD4 to
  BO6), with `by-game/sheet_<game>.jpg` contact sheets and `index.json`. The
  first study's urban sheets are `sheet_a.jpg` to `sheet_d.jpg`.
- If the folder is missing, rebuild it:
  `uv run --with pillow tools/map_blockout/fetch_cod_minimaps.py`.
- The images are Activision's. Never commit them, attach them to a PR, or ship
  them. Describe what you take from them instead.
- [docs/map-design.md](../../../docs/map-design.md) lists which maps teach what,
  the principles below in more detail, and past decisions.

Ask the user which reference family the map should feel like (show the sheets),
unless they already said. Their standing answer for city maps: CoD4 city
blocks (Crash, Strike, District, Backlot) with BO2 compact loops (Raid,
Standoff, Slums), and two sides that are different but balanced rather than
mirrored.

Take structure, never a whole map: no map's layout, name, landmarks, or look
copied one to one, even renamed.

## 2. What the good ones share

1. A roughly square footprint (aspect 1.4:1 at most). Long strips turn every
   lane into a hallway.
2. Lanes made of many medium buildings (8 to 20 m) with streets winding
   between them, not three parallel bands. Streets form loops of different
   sizes.
3. Main streets that bend, jog, or run diagonally, so no line runs the length
   of the map.
4. Usually not mirrored: each area has its own identity, and balance comes from
   matched walking times and sightlines, not identical halves. A 180 degree turn
   about the centre with different dressing is a good middle ground.
5. One strong central landmark with four or more ways in.
6. Two-storey buildings you can enter, overlooking streets, that double as
   cut-throughs between lanes. Each team gets a place to hold near its side.
7. Every lane has a purpose: a different engagement range (close, mid, long)
   and character.

## 3. Block it out and measure it

Write the plan with `tools/map_blockout/carve.py` (start solid, carve streets,
rooms, doors; place cover; every piece gets its twin). Copy
`tools/map_blockout/breakwater_2026_11.py` as a starting point. Then:

```sh
uv run --with numpy --with matplotlib tools/map_blockout/blockout.py <plan.py>
```

It prints walking times and sightlines, and writes `plan.png`,
`sightlines.png`, and `minimap.png`. Targets for a 6v6 core map:

| Measure | Target |
| --- | --- |
| Footprint | 70 to 90 m a side |
| Spawn to spawn | 75 to 90 m walk (10 to 12 s sprinting) |
| Spawn to the centre | 5 to 6 s, both teams within 0.3 s |
| Median longest eye-height line | 32 m or less |
| Floor that sees past 40 m | under 10% |
| Longest line anywhere | 60 m or less |
| From a spawn area | no line into the enemy's half |

Iterate on the plan until it meets them. Fix the biggest problem each round
(the longest lines, an unbalanced spot, a lane that merges with another), not
the smallest.

## 4. Show it before building it

Show the user `minimap.png` (it reads like the references) and
`sightlines.png`, with a short table against the current map, and say which
reference each idea came from. Wait for approval: a rebuild discards art, so
the layout must be settled first. Keep the approved plan in
`tools/map_blockout/`.

## 5. Build it

Build the approved plan in the map's generator (Breakwater:
`tools/breakwater_layout.py`), keep the plan's numbers, run the map's checks, and
then polish with the rating loop (a fresh subagent scores each aspect out of 10
until the agreed target).
