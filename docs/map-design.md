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

## Warehouse, November 2026

[Warehouse](../godot/scenes/warehouse.tscn) is an editable Cyclops blockout
based on supplied reference photos and a top-down sketch. The sketch sets
the layout: a parking strip across the top, a narrow left wing, a large main
hall, and a truck-bay apron along the bottom. The two office footprints are
open-top rooms with provisional interior doorways. Spawn 2 and the truck
apron sit 1.25 m below the hall, with a 5 m ramp at each of the two marked
bays and another at the left wing entrance.

The [blockout plan](../tools/map_blockout/warehouse.py) uses estimated
dimensions. Its measurements approximate routes on a single horizontal
plane. The 8 m Cyclops shell remains editable. Hide the fitout's separate
`Overhead` group in the editor to see the floor from above.

The first detail pass uses the box-machine reference photos for the
inspection windows, corrugated-sheet infeed, and control pendant. The
blue rolling bin and palletized sheets come from the surrounding staging
area in the photos. The props use the readable industrial silhouettes of
[Meat](https://www.callofduty.com/guides/multiplayer-maps/call-of-duty-guide-modern-warfare-iii-multiplayer-map-meat)
as a style reference. The custom props are generated by
[`tools/build_warehouse_props.py`](../tools/build_warehouse_props.py).
The parked forklift is from the [Innerscene 3D Parts Library](https://www.innerscene.com/tools/library/3d-parts/electric-counterbalance-forklift-7acc0547)
under CC0, with paint changed to yellow and static meshes batched by material.

The full fitout uses the original photos as its material and equipment guide.
The Jumbo infeed, crawl hood, die cutter, sheeter, and rotary press have
separate Blender models over the existing collision brushes. Stock racks,
paper rolls, pallet stacks, scrap hoppers, and strapping carts make the
production and storage areas feel crowded. Offices have workstations,
filing cabinets, and their own ceilings. The exterior uses marked bays,
delivery trucks, and pallet staging. Rebuild the models with
[`tools/build_warehouse_dressing.py`](../tools/build_warehouse_dressing.py).
The [placement definitions](../tools/map_blockout/warehouse_dressing_layout.py)
are also used by the 2D blockout to account for the new floor obstacles.

The second pass adds four glass cubicles, scattered packing slips, and a
wraparound dispatch counter to the fictional office modules. Fixed
personnel-door panels and contrasting dock bumpers visually distinguish
the service and loading areas. The shop floor adds supply carts, loose
cartons, and paperwork around the existing production stations. Rebuild
the new models with
[`tools/build_warehouse_second_pass.py`](../tools/build_warehouse_second_pass.py).

The wing has a stock annex and the dispatch office now sits under an
L-shaped roof.

### Photo-matched pass

The 6v6 rebuild below replaced this pass's interior layout, spawns, and yards;
its shell, roof structure, materials, and models remain.

A third pass rebuilds Warehouse from nine reference photos of a corrugated-box
plant and the camera map that marks where each was taken. Where the photos and
the sketch disagree, the photos win for what is visible and the sketch fills in
the rest. The hall now runs east to x 44, and the roof is a grey corrugated deck
at 9.3 m on black open-web joists and girders, with gray steel columns. The
southeast corner has a 3.8 m CMU storage platform, and the dispatch office and
shipping room walls rise to the roof. Three production lines run east–west on
the sketch's machine blocks. The north line ends in the white die-cutter and
turns north into the blue delivery section and its duct. The middle line ends at
the east aisle. The south line starts at the arch strapper beside the AOPACK
finishing cell. Palletized corrugated stock, paper rolls, hoppers, and pallet
towers stand where the photos show them. The high-bays are shadowed downlights
with a faint uplight, over a polished slab with saw cuts and cracks. Tilt-up
walls show lifting inserts, and the scene adds light haze and dust.

- Layout source: [`tools/map_blockout/warehouse_photo_layout.py`](../tools/map_blockout/warehouse_photo_layout.py)
  writes `godot/tools/warehouse_photo_layout.json`.
- Builder: [`godot/tools/warehouse_photo_build.gd`](../godot/tools/warehouse_photo_build.gd)
  rebuilds the Cyclops shell and the `PhotoPass` group in the open scene. It is
  run from the editor, for example through the Godot AI MCP bridge's `execute_gdscript`.
- Assets: [`tools/build_warehouse_photo_textures.py`](../tools/build_warehouse_photo_textures.py)
  (procedural textures), [`tools/build_warehouse_photo_props.py`](../tools/build_warehouse_photo_props.py)
  (stock, building parts), and [`tools/build_warehouse_photo_machines.py`](../tools/build_warehouse_photo_machines.py)
  (production equipment), all written to `godot/assets/props/warehouse_photo/`.
- Surfaces: `godot/shaders/warehouse_floor.gdshader`, `godot/shaders/warehouse_tiltup.gdshader`,
  and `godot/materials/warehouse/`.

#### Playing Warehouse

Warehouse is on the title screen as a free-roam route and in the local match
setup for every mode except Search and Destroy. Servers load it with
`--map warehouse`. The layout is the 6v6 rebuild below. Team 0 starts in the
north parking lot and team 1 on the south truck apron; each has four doors into
the hall. Twenty-eight neutral respawns sit in the aisles of both halves and at
the yards' ends (see [tdm-spawns.md](tdm-spawns.md)). Bots follow three lanes,
west to east: the west racking aisle, the centre aisle through the mid
stockpile, and the east rolls aisle. Capture the Flag and Hardpoint fall back
to flag and zone positions derived from the spawns. Reverb zones give the
hall and the two upstairs offices their own echo. Screen walls close the yards
and an invisible boundary stands behind them; the wing is sealed.
`godot/tests/warehouse_test.gd` checks the spawns, floor, and body clearance,
the prop collision, the tunnels, roofs, offices, and walkways (below), and the
route from the menu.

### 6v6 rebuild (November 2026)

The user's direction: flow like a real 6v6 Call of Duty map rather than match
the building exactly, with Modern Warfare's **Atlas Superstore** as the model of
a warehouse map done properly. The approved plan is
[`warehouse_2026_11.py`](../tools/map_blockout/warehouse_2026_11.py); the
building is the real one's single square hall from its fire plan (no wing).

What it takes from Atlas Superstore, as structure rather than layout:

- **A yard start at each end.** Atlas starts one team in the Backlot and the
  other at the Loading Docks; here the parking lot (north) and the truck apron
  (south), each with box trucks and pallet walls set as staggered gates.
- **Several ways in, each staggered.** Four doors per side, with a pallet
  baffle just inside each so no line runs from a yard into the hall, and PVC
  strip curtains in the doorways.
- **Big blocks in a grid, aisles between.** Atlas's shelving blocks become
  machine lines (flexo and die cutter in the north; rotary, folder-gluers, and
  strapping in the south), pallet racking faced onto the aisles, and roll and
  stock blocks, about 6 to 13 m with 4 to 5 m aisles: many routes, not three
  corridors. Wall departments (ink room, maintenance shop; plate room, tooling
  crib) cut the back and front aisles into stretches.
- **A middle under an open roof.** Atlas's ceiling is open to the sky in
  places; here a 12 m roof breach over the mid stockpile lets a sun shaft and
  haze fall on a stack of jumbo rolls, with fallen deck panels around it.
- **An upstairs office near each team** (Carpenters Office and Security in
  Atlas): the shift office (north-east) and the QC lab (south-west), two
  storeys, a stair inside, an open-plan upper floor whose window band looks
  over the racking and the bay.
- **Overwatch reached by a stair:** a steel crossover deck at 2.6 m over a
  middle-row machine in each half, with rails along its sides and a stair from
  the cross aisle.

Verticality and set pieces added on review:

- **Room roofs.** The ink room (north) and plate room (south) are one storey
  with a flat roof at 3.4 m. Two pallets of stock against the wall step 1.1 m
  and 2.2 m, so players mantle up to the roof (Call of Duty's mantle reaches
  1.35 m here), which looks down the back aisle over a parapet.
- **The roll perch** in the mid stockpile: 1.1 m, then 2.0 m, then the roll
  tops at 2.6 m.
- **Crouch tunnels.** Each long north–south production line is split by a
  transfer conveyor at 1.4 m: a crouching or sliding player (1.1 m) gets
  through, a standing one does not (Atlas's shortcuts).
- **Effects:** the breach's sun shaft and denser haze, a torn cable tray over
  each crossover throwing sparks with a flash, two failing high-bays that
  flicker (back and front aisles), amber beacons turning on the yard
  forklifts, steam from a wall vent in each half, and a toppled rack's spilled
  sheets in each bay.

Stairs are clipped as one ramp (steps to see, a slope to walk on), as Call of
Duty clips stairs, and rails as one box from deck to top rail.

| Measure | Photo pass | 6v6 rebuild | Target |
| --- | --- | --- | --- |
| Start to start | 110 m (14.8 s) | 75 m (10.1 s) | 75 to 90 m |
| Median longest eye-height line | 33.8 m | 20.5 m | 32 m or less |
| Floor that sees past 40 m | 33% | 2% | under 10% |
| Longest line | 80 m | 50 m | 60 m or less |

Measured on the built scene: `godot/tests/warehouse_grid.gd` samples its real
collision on a 0.5 m grid and [`warehouse_measure.py`](../tools/map_blockout/warehouse_measure.py)
reports the targets (one floor per cell, so upstairs is not measured).

**Bots.** Lanes used to be read as running west to east, with a team's
direction taken from its spawns' mean X; Warehouse's teams start north and
south. Bots now take the direction between the two teams' starts and run each
lane that way (`lane_route` in `rust/game/src/net/bots.rs`), which is
unchanged on maps whose teams start west and east. `bots_test.gd -- --warehouse`
plays a minute of bot TDM on Warehouse in CI.

**Collision.** Every prop collided as one box around its whole model, so the
overhead extraction hood walled off the floor under it, forklifts blocked the
air around their forks and mast, the incline conveyor blocked everything under
its belt, and every shelf of a rack stopped bullets. [`build_warehouse_clip.py`](../tools/build_warehouse_clip.py)
voxelizes each model and writes clip boxes to
`godot/assets/props/warehouse_photo/clip.json`, the way Call of Duty maps use
player clip: boxes that follow the solid parts, keep every opening at least
0.3 m tall open (rack shelves, the space under overhead parts), and skip
details too thin to matter. The builder gives every placed prop those boxes. The test fails on a collider
with nothing visible around it, a prop box that reaches past its model, or a
return to single boxes.

**Photo 10** looks across the feed end of the rotary die-cutter. Its models,
now placed by the 6v6 layout:

- `rotary_feeder`: the navy feed section with yellow handwheels, a black
  bellows guard, a blue and silver roller feed table behind a rusted yellow side
  guide, the control column and red lockout box, the operator's clipboard, cup,
  bottle, and ear defenders, and an overhead blue rail with red LED counters and
  a convex safety mirror. It sits under the south crossover.
- `pallet_rack_orange` (orange uprights, teal beams, curved board formers)
  faces the racking blocks onto the aisles, and `trim_duct` (a 1 m galvanized
  duct on the floor) runs along the bay walls as waist-high cover.
- Cable ladder trays at 5.8 m across the south bay and a branch north at 6.5 m.
- A grey primed roof deck in place of the black one, and stronger high-bay
  uplight, so the deck reads light grey above black joists as in the photo.

Rebuild:

```sh
uv run --with numpy --with matplotlib tools/map_blockout/blockout.py tools/map_blockout/warehouse_2026_11.py
python3 tools/map_blockout/warehouse_photo_layout.py
blender -b --python tools/build_warehouse_photo_machines.py -- rotary_feeder pallet_rack_orange trim_duct
uv run --with trimesh --with numpy --with scipy tools/build_warehouse_clip.py
# A real renderer is needed: headless Godot saves MultiMesh instances empty,
# so the script refuses to rebuild those parts under --headless.
xvfb-run -a godot --path godot --script res://tools/warehouse_build_headless.gd
GRID_OUT=grid.json godot --headless --path godot --script res://tests/warehouse_grid.gd
uv run --with numpy --with matplotlib tools/map_blockout/warehouse_measure.py grid.json --centre=13.5,8
```

`godot/tests/capture_warehouse.gd` renders the photo viewpoints and a top-down
plan for comparison.
