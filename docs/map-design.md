# Map design

How we lay out multiplayer maps, what we learned from real Call of Duty maps,
and the rules a map scene follows. Agents follow the
[map-design skill](../.agents/skills/map-design/SKILL.md); this page is the
background. Each shipped map is described in [maps.md](maps.md).

## References

We study real Call of Duty minimaps before drawing a layout. They are
Activision's images, so they live only on the developer's machine, never in
this repository, a PR, or a build:

- `~/.local/share/shooter/map-references/cod-minimaps/by-game/<game>/<Map>.png`:
  about 285 multiplayer minimaps from CoD4 to BO6, a `sheet_<game>.jpg`
  contact sheet per game, and `index.json` with each file's wiki source.
- `uv run --with pillow tools/map_blockout/fetch_cod_minimaps.py` downloads
  them again from the Call of Duty wiki and rebuilds the sheets. It skips maps
  it already has, so it can be rerun to add games.

We take structure from them (proportions, how lanes form, where landmarks and
power positions sit), never a whole map: no layout, name, landmark, or look
copied one to one, even renamed.

### What each family teaches

| Family | Maps | What to take |
| --- | --- | --- |
| CoD4 city blocks | Crash, Strike, District, Backlot, Crossfire | A dense block of two-storey buildings with streets cut between them; a central landmark street or plaza (Crash's wreck, Strike's statue); streets that jog so lines stay short; buildings you can enter that overlook streets. |
| BO2 compact loops | Raid, Standoff, Slums, Express, Hijacked | A small, rounded footprint; a clear centre with loops round it; every route spawn to spawn about the same length; Slums' plaza landmark with many ways in. |
| MW2 and MW3 towns | Skidrow, Invasion, Seatown, Resistance, Karachi, Terminal, Arkaden | Denser interiors; a curving main street; many cut-throughs between lanes. |
| Open junctions | Piccadilly, Crossfire | One dominant street or junction with buildings round its edges. MW 2019's Piccadilly is the furthest from three lanes, and players often avoid its open middle. |
| Big-shed interiors | Atlas Superstore (MW 2019), Vacant (CoD4) | A yard start at each end and several staggered ways in; big blocks in a grid with aisles between, not three corridors; a middle under a breach in the roof; an upstairs room near each side; Vacant's overgrown lot against industrial interiors. |
| Container yards | Shipment, Crash | Containers and stacked stock as hard cover in an outdoor yard. |

### What the good ones share

Treyarch's "every lane needs a purpose"
([Xbox Wire, 2025](https://news.xbox.com/en-us/2025/10/27/how-treyarch-crafts-multiplayer-maps-call-of-duty-black-ops-7/))
and players' views
([GamesRadar](https://www.gamesradar.com/how-to-build-a-great-call-of-duty-multiplayer-map-according-to-the-players-that-know-the-game-best))
agree on these:

1. A roughly square footprint (1.4:1 at most). Long strips turn lanes into
   hallways.
2. Lanes formed by many medium buildings (8 to 20 m) with streets that loop,
   not three parallel bands.
3. Streets that bend, jog, or run diagonally; no line runs the length of the
   map.
4. Balance from matched walking times and sightlines, not identical halves.
   Each area has its own identity. A 180 degree turn about the centre,
   dressed differently, is a good middle ground.
5. A strong central landmark with four or more ways in.
6. Two-storey buildings you can enter that overlook streets and link lanes,
   with a place to hold near each team's side.
7. Each lane has a purpose: a different engagement range and character.

## Measuring a layout

A plan is a Python module written with `tools/map_blockout/carve.py`: the
map starts solid, streets, yards, and rooms are carved open, and cover is set
back in. Every piece is placed with its twin (turned 180 degrees about the
centre by default, mirrored across x = 0 with `carve.SYMMETRY = "mirror"`)
unless it says `twin=False`, which is how an asymmetric plan is drawn. x runs
west to east, z north to south, in metres. The plan also names its two
spawns, `ROUTES`, `PROBES`, and `LANES`.

```sh
uv run --with numpy --with matplotlib tools/map_blockout/blockout.py <plan.py> [--out DIR]
```

It prints walking distances and times (sprinting, 7.4 m/s) spawn to spawn,
along each route, and from each spawn to each probe; the longest clear
eye-height line from every metre of floor (cover marked `low` does not block
it); and any floor that cannot be reached. It writes `plan.png`,
`sightlines.png` (with the line where both teams arrive at the same moment),
and `minimap.png`, drawn like a Call of Duty minimap to sit beside the
references.

Targets for a 6v6 core map:

| Measure | Target |
| --- | --- |
| Footprint | 70 to 90 m a side |
| Spawn to spawn | 75 to 90 m walk (10 to 12 s sprinting) |
| Spawn to the centre | 5 to 6 s, both teams within 0.3 s |
| Median longest eye-height line | 32 m or less |
| Floor that sees past 40 m | under 10% |
| Longest line anywhere | 60 m or less |
| From a spawn area | no line into the enemy's half |

A plan only approximates the built map; Breakwater's generator checks
(below) test the layout it is built from.

Approved plans stay in `tools/map_blockout/`: `breakwater_2026_11.py`.

## Generator checks

Breakwater is generated from one layout file, and the generator refuses a
layout that breaks the map's rules (`tools/breakwater_checks.py`, which reuses
the sight tests in the shared kit's `tools/slamcity_checks.py`; CI runs them
through `tools/test_breakwater_layout.py`):

- collision is the same on both halves, turned about the centre (rooms
  dressed differently on each side are exempt);
- no spawn point is in sight of the enemy's spawns or high ground, and no
  eye-height line runs from one spawn yard into the other;
- no ground line at eye height runs past 55 m, nor past 50 m between the two
  teams' high ground;
- every Hardpoint move goes at least 16 m, to a zone that does not overlap
  the last one seen from above;
- whichever of our high ground or streets the enemy takes, some spawn points
  stay out of their sight, so the server always has somewhere safe to put us.

The checks shape the build. Breakwater's pipe bridges across the main road,
the control house's blank east face, and the screens round the workshop's
roof deck all exist to pass them.

## Lessons from our maps

- **Strips fail.** A three-strip city map (Slamcity, since cut), 124 x 61 m
  and mirrored, measured 118 m spawn to spawn with 45% of its floor seeing
  past 40 m and a 120 m longest line: the alley, the avenue, and the space under the El ran into one
  open field. The owner rejected it and a redesign drawn the same way. The
  city-block plan that replaced it meets every target.
- **A turned twin can still read as a mirror.** Warehouse's halves (a map
  since cut) were one half turned 180 degrees; even dressed differently, the
  owner found it a mirror. Its last plan drew each half on its own
  (`twin=False`), balanced by walking times and sightlines.
- **High ground faces its own fight.** A roof or upper floor that sees across
  the map to the enemy's high ground or spawn turns into a sniper duel; give
  it walls or screens so it covers its own lane.
- **Close spawn-yard lines with structure, not fog.** Screens, bridges, and
  staggered baffles inside doorways keep a street open to walk while cutting
  the line from a yard into the map.

## Building for play

Collision follows Call of Duty's player clip, not the visible mesh:

- Stairs collide as one ramp (steps to see, a slope to walk on); rails as one
  box from deck to top rail.
- A prop collides with a few boxes fitted to its solid parts that keep every
  opening at least 0.3 m tall open (rack shelves, the space under overhead
  parts) and skip thin details.
- Anything a sprinting or sliding player could catch a foot on either never
  collides (litter, debris, cones, tape, signs, weeds, wall dressing) or gets
  one smooth shape (a rubble mound is one low hull, slopes under 20 degrees).
- A map edge that needs player clip is tall, thin walls on the
  movement-only layer (16): they stop players, while bullets pass through.

Heights and widths that matter (`MoveParams` in `rust/sim/src/movement.rs`):

| Size | Value | Use |
| --- | --- | --- |
| Standing height | 1.8 m | Headroom everywhere players walk |
| Crouch and slide height | 1.1 m | A 1.4 m gap is a crouch tunnel: crouch or slide through, never stand |
| Prone height | 0.72 m | |
| Mantle | 0.5 to 1.35 m | Ledges to climb onto; stack cover in steps under 1.35 m to make a way up |
| Bot doorway | 1.4 m or wider | The bots' navigation mesh (0.375 m radius, 0.125 m cells) stays open through it at any grid alignment |

A collider's `surface` metadata sets its footsteps, impacts, and
penetration: `glass` stops players but keeps up to 95% of a round's damage
(`rust/sim/src/penetration.rs`), and bots see through it and never take it as
cover. Ladders are `LadderVolume` nodes (`rust/sim/src/movement/ladder.rs`).

Bots bake their navigation mesh from the map's static colliders on the world
layer when the server loads the map (`BotNavigation::bake` in
`rust/game/src/net/bots.rs`), inside a box round every spawn point, bot lane,
and objective, widened by 40 m sideways, 20 m down, and 40 m up. Keep spawns
and objectives inside the playable space and the box covers it; far scenery
can keep its collision.

## Map markers

The server reads a map's gameplay from named `Node3D` children of the scene's
root (`rust/game/src/net/server.rs`). Each is optional and has a fallback.

| Node | Children | Metadata | Without it |
| --- | --- | --- | --- |
| `SpawnPoints` | One per spawn, facing the way players should | `team` 0 or 1; none or -1 for either team | Each team lined up through the player start, 14 m apart |
| `BotLanes` | One per lane, its children points from team 0's side to team 1's | | Bots roam the navigation mesh |
| `Flags` | Capture the Flag bases | `team` 0 or 1 | Each base at the centre of its team's spawns |
| `DominationFlags` | A, B, C in order | `radius` (default `domination::DEFAULT_RADIUS`, 1 to 12 m), `label` | A by team 0's spawns, B in the middle, C by team 1's |
| `Hardpoints` | Zones in rotation order | `size` (full box, default 10 x 4 x 10 m), `label` | No Hardpoint zones |
| `ChargeSites` | Search and Destroy sites | `size` (default 5 x 3.6 x 5 m), `label` | Two sites on the defenders' side of the middle, one to each flank |

A map script can build its markers in `prepare_server()`, which the server
calls before reading them. `ReverbZones` and a `Soundscape` set the map's acoustics
([audio.md](audio.md)).

## Tools

- **Look at a map.** With a display, `just capture <map> <dir>` saves
  eye-level PNGs with the game's lighting (`godot/tests/capture_view.gd`).
  `<map>` is a server key (`breakwater`, `arena`), `testing`, or
  a `res://` scene path. It picks up
  to 12 viewpoints from the map's spawn, lane, and objective markers, at
  standing eye height on the floor below. Environment variables:
  `CAPTURE_AUTO` (number of views), `CAPTURE_TOP=1` (adds an overhead
  `top.png`), `CAPTURE_PRESET` (`low`, `medium`, or `high`, the default),
  `CAPTURE_VIEWS='name:x,y,z,heading,pitch[,fov];...'` (feet position in
  world metres; heading 0 faces -Z, 90 faces +X; positive pitch looks up; FOV
  vertical), and `CAPTURE_VIEWS_FILE` to reshoot a saved `views.json`.
- **Drive the editor.** `just agent-editor` and the editor's MCP bridge let an
  agent open a scene and render it from any eye
  ([editor-mcp.md](editor-mcp.md)).
- **Minimaps.** `godot --headless --path godot --script
  res://tools/build_minimaps.gd` renders each multiplayer map's minimap from
  its collision (`MINIMAPS=breakwater` limits it) into
  `godot/assets/minimaps/` and `godot/scripts/net/minimap_data.gd`. Rebuild it
  whenever a map's collision changes.
- **Props.** `python3 tools/prop_catalog.py --find <tags>` searches
  `godot/assets/props/catalog.json` (size, triangles, tags, source);
  `--write` rebuilds it from `tools/prop_library.py`, and CI checks it.
  `godot/tests/capture_props.gd` renders studio pictures and contact sheets of
  the library beside a 1.8 m post.
- **Decals.** `python3 tools/decal_catalog.py --find <tags>` searches
  `godot/assets/textures/decals/catalog.json`.
- **Dressing the generated maps.** `tools/map_dressing.py` dresses the Testing
  Area and shades its slabs and panels per vertex;
  Breakwater places props from its layout file.
- **Lightmaps.** `tools/bake_lightmaps.sh [res://scenes/<map>.tscn ...]` bakes
  (with no argument: the Testing Area and Breakwater).
  Lights that exist only for the bake go under a `BakedLights` node
  (`scripts/baked_lights.gd`), which frees them in the game. A LightmapGI bakes
  only what is under its parent, so it sits at the scene's root; keep lamp
  housings out of the bake or each lamp shadows itself off the room.
  `godot/tests/lightmaps_test.gd` fails a bake that is black.

### ProtonScatter, FuncGodot, and TrenchBroom

- **ProtonScatter** scatters props along paths or across areas in the editor.
  Pick them from the prop catalog.
- **FuncGodot** builds TrenchBroom `.map` brush geometry into a scene. New
  `FuncGodotMap` nodes use `trenchbroom/shooter_map_settings.tres`, which
  shares TrenchBroom's entity definitions and FuncGodot's default inverse
  scale of 32 (one Quake unit is 1/32 m).

To set up TrenchBroom 2025.1 (on Linux, its AppImage, run from outside the
checkout):

1. In Godot, open `addons/func_godot/func_godot_local_config.tres`. Set
   **TrenchBroom Game Config Folder** to your TrenchBroom user games folder
   plus `Shooter` (on Linux, `~/.TrenchBroom/games/Shooter`) and **Map Editor
   Game Path** to the checkout's `godot/` directory, then click **Export
   func_godot settings**.
2. Open `trenchbroom/shooter_game_config.tres` and click **Export
   GameConfig**. This writes the game config, `Shooter.fgd`, and the icon to
   the games folder.
3. Restart TrenchBroom, pick **Shooter**, and set its **Game Path** to the
   same `godot/` directory so it finds `assets/textures`.

Keep what they generate as ordinary Godot resources and nodes before shipping
a scene: the export presets leave out both add-ons and `trenchbroom/`.

## Decisions

The owner made these; follow them unless the owner changes them.

- Maps play like real Call of Duty maps, and every layout is approved as a
  minimap before it is built.
- City maps: CoD4 city blocks with BO2 compact loops, and two sides that are
  different but balanced rather than mirrored (the owner's answer for
  Slamcity, since cut).
- Breakwater is the one multiplayer map; Warehouse was cut on 2026-11-27
  ([SCOPE.md](../SCOPE.md)).
- Everything a player looks at is a modelled mesh built in Blender; Godot
  primitives and CSG stay for collision, effects, and debug shapes.
- Reference material (such as the Call of Duty minimaps) is measured and
  rebuilt from, never committed.
