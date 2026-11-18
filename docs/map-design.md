# Map design

How we lay out multiplayer maps, what we learned from real Call of Duty maps,
and the decisions behind our own. Agents follow the
[map-design skill](../.agents/skills/map-design/SKILL.md); this page is the
background.

## References

We study real Call of Duty minimaps before drawing a layout. They are
Activision's images, so they live only on the developer's machine, never in
this repository:

- `~/.local/share/shooter/map-references/cod-minimaps/by-game/` (about 285
  minimaps from CoD4 to BO6, one contact sheet per game, and `index.json` with
  each file's wiki source).
- `tools/map_blockout/fetch_cod_minimaps.py` downloads them again from the Call
  of Duty wiki ([example](https://callofduty.fandom.com/wiki/File:Raid_Minimap_BOII.png))
  and builds the contact sheets.

We take structure from them (proportions, how lanes form, where landmarks and
power positions sit), never a whole map.

### What each family teaches

| Family | Maps | What to take |
| --- | --- | --- |
| CoD4 city blocks | Crash, Strike, District, Backlot, Crossfire | A dense block of two-storey buildings with streets cut between them; a central landmark street or plaza (Crash's wreck, Strike's statue); streets that jog so lines stay short; buildings you can enter that overlook streets. |
| BO2 compact loops | Raid, Standoff, Slums, Express, Hijacked | A small, rounded footprint; a clear centre with loops around it; every route spawn to spawn about the same length; Slums' plaza landmark with many ways in. |
| MW2 and MW3 towns | Skidrow, Invasion, Seatown, Resistance, Karachi, Terminal, Arkaden | Denser interiors; a curving main street; many cut-throughs between lanes. |
| Open junctions | Piccadilly, Crossfire | One dominant street or junction with buildings round its edges. MW 2019's Piccadilly is the furthest from three lanes, and players often avoid its open middle. |

### Principles

The good maps share these (Treyarch's "every lane needs a purpose",
[Xbox Wire, 2025](https://news.xbox.com/en-us/2025/10/27/how-treyarch-crafts-multiplayer-maps-call-of-duty-black-ops-7/),
and players' views in
[GamesRadar](https://www.gamesradar.com/how-to-build-a-great-call-of-duty-multiplayer-map-according-to-the-players-that-know-the-game-best)
agree):

1. A roughly square footprint. Long strips turn lanes into hallways.
2. Lanes formed by many medium buildings, with streets that loop, not three
   parallel bands.
3. Streets that bend or run diagonally; no line runs the length of the map.
4. Balance from matched walking times and sightlines, not identical halves.
   Each area has its own identity.
5. A strong central landmark with four or more ways in.
6. Two-storey buildings you can enter that overlook streets and link lanes,
   with a place to hold near each team's side.
7. Each lane has a purpose: a different engagement range and character.

## Measuring a layout

`tools/map_blockout/` draws a plan (`carve.py`), measures it, and renders it
(`blockout.py`): walking times from both spawns, the longest clear eye-height
line from every metre of floor, where the teams meet, and a minimap in the
style of the references. The skill lists the targets.

## Slamcity, November 2026

The first rebuild (#97) laid Slamcity out as three parallel strips, 124 × 61 m
and mirrored across x = 0. Measured, it walked 118 m spawn to spawn (16 s), and
45% of its floor saw further than 40 m (median longest line 39 m, longest
120 m): the alley, the avenue, and the space under the El ran together into one
open field. The user called it terrible and rejected a first redesign that kept
the strips.

After studying the minimaps, the user chose CoD4 city blocks with BO2 compact
loops, and sides that are different but balanced. The approved plan is
[`tools/map_blockout/slamcity_2026_11.py`](../tools/map_blockout/slamcity_2026_11.py):

- 84 × 68 m of carved city blocks round Junction Plaza, where a derailed El
  car lies diagonally under the station platform.
- Three lanes: Slam Avenue into the plaza (mid range); the north street through
  the pawn shop into the far market (close quarters); the market through the
  ticket hall into the far south street.
- Each team's half is the other's turned 180 degrees about the centre, dressed
  differently (tenement and hotel, diner and laundromat, pawn shop and ticket
  hall).
- 79 m spawn to spawn (11 s); both teams reach the plaza in 5.6 s; median
  longest line 30 m, 8% of floor past 40 m, longest 60 m.
