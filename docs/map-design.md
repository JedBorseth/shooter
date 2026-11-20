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

## Bot navigation

The server bakes the bots' navigation mesh from the map's static colliders on
the world layer when it loads the map (`BotNavigation::bake` in
`rust/game/src/net/bots.rs`), inside a box around every spawn point, bot lane,
and objective, widened by 40 m sideways, 20 m down, and 40 m up. Far scenery
can keep its collision: without the box, Nuketown's backdrop mountains, more
than a kilometre across, made Godot's crash check stop the bake ("Baking
interrupted"), and bots stood still there in every mode. Keep spawns and
objectives inside the playable space and the box covers it.

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

Warehouse is on the title screen under Free Roam and in the local match
setup for every mode except Search and Destroy. Servers load it with
`--map warehouse`. The layout is the 6v6 rebuild below. Team 0 starts in the
north parking lot and team 1 on the south truck apron; each has four doors into
the hall. Twenty-eight neutral respawns sit in the aisles of both halves and at
the yards' ends (see [tdm-spawns.md](tdm-spawns.md)). Bots follow three lanes,
west to east: the west racking aisle, the centre aisle through the mid
stockpile, and the east rolls aisle. Capture the Flag falls back to flag
positions derived from the spawns. Hardpoint rotates through five zones
(`Hardpoints`): the Mid Stockpile, Shipping, the South Crossover, the North
Crossover, and Receiving; the two crossovers and Shipping and Receiving are
each other's 180-degree twins, and the order alternates halves. Reverb zones give the
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
  corridors. Wall departments (ink room, maintenance shop; plate room, and an
  open forklift charging bay since the variation pass) cut the back and front
  aisles into stretches.
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
  with a flat roof at 3.4 m, which looks down the back aisle over a parapet.
  At the ink room, two pallets of stock against the wall step 1.1 m and 2.2 m,
  so players mantle up to the roof (Call of Duty's mantle reaches 1.35 m
  here); the plate room has a fixed ladder instead (see the variation pass).
- **The roll perch** in the mid stockpile: 1.1 m, then 2.0 m, then the roll
  tops at 2.6 m.
- **Crouch tunnels.** Each long north–south production line is split by a
  transfer conveyor at 1.4 m: a crouching or sliding player (1.1 m) gets
  through, a standing one does not (Atlas's shortcuts).
- **Effects:** the breach's sun shaft and denser haze, a torn cable tray over
  the north crossover throwing sparks with a flash, two failing high-bays that
  flicker (back and front aisles), an amber beacon turning on the parking
  lot's forklift, and a toppled rack's spilled sheets in the north bay. (The
  wall steam went in the detail pass; the south half's twins of the sparks,
  beacon, and spill went in the variation pass.)

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

### Detail pass (November 2026)

![Warehouse before (left) and after (right) the detail pass: the fallen forklift by the west wall, the burnt-out switchboard, the smoke reaching the roof, the breach's rubble and puddles, the roof leak, and the south yard](media/warehouse-detail.jpg)

The user's direction: make the hall look lived in and run down, as if a
professional map maker had done a large detail pass; break up the repeating
floor and walls; keep collision clean; and replace the steam on a blank wall
with a fallen-over forklift smoking like it is broken, and more like it.

Call of Duty's warehouse maps (Modern Warfare's Atlas Superstore, the
wrecked interiors of Black Ops 3) dress a big shed this way: a few story set
pieces you can read from across the room, clutter banked where it would
gather, ground blended from painted masks rather than tiled, and none of the
clutter in player clip.

- **Floor** (`warehouse_floor.gdshader`). Every 5.5 m slab between the saw
  cuts is its own pour: its own tone, roughness, and shifted, turned patch of
  the detail and mask textures, with the seams hidden in the joints, so the
  repeat never shows. One slab in ten is a newer patch pour ringed in sealant.
  On top, masks painted over the whole map from the layout
  (`tools/build_warehouse_floor_masks.py`, `floor_layout_a/b.png` at 10 px/m):
  dust banked against walls, stock, and machines; forklift tyre tracks down
  every long clear aisle with the polish they leave; puddles that fill the
  slab's low spots first; oil under machines, trucks, and the wreck; scorch;
  extinguisher powder; damp; and broken slab. Broken slab is cut in with
  parallax occlusion mapping (`floor_damage.png`, spalls and cracks 3.5 cm
  deep indoors, potholes 6 cm in the yards): purely visual, so collision stays
  the flat slab. The yards use the same shader with weathered, stained
  concrete.
- **Walls** (`warehouse_tiltup.gdshader`). Every tilt-up panel is its own
  casting (tone and texture patch), some lifting inserts weep rust, rain
  streaks run down from the roof flashing on some panels, and a damp band with
  a salt line sits at the foot. Walls read the floor masks at their foot: dust
  banks against them, damp rises where water stands, and soot climbs above a
  fire, widening as it rises.
- **Set pieces** (`tools/map_blockout/warehouse_detail_layout.py`):
  - North, west wall: a forklift on its side where the low stock stood,
    smouldering. A dark smoke column rises, slows, and spreads into a haze
    under the roof; small flames lick in the engine bay; a cable arcs; oil and
    scorch spread on the slab and soot up the wall; its load, a broken pallet,
    and a knocked-over cone lie round it. It lies with its guard against the
    wall and its chassis to the aisle, and collides as one box 1.15 m high,
    flush with the chassis: the same cover the low stock gave, and players
    mantle onto it.
  - South, east wall: a switchboard that arced and burnt out, its middle door
    blown open and swung flat against the next section, breakers charred,
    cables spilled, blue-white arcing and a wisp
    of smoke, soot up the wall, cordoned with cones and barrier tape, an
    emptied extinguisher in its powder.
  - Middle: the roofing that came down through the breach: a rubble mound, a
    folded deck sheet, debris, torn insulation hanging off the edges, a joist
    hanging by one end (above 5 m), rain dripping off the edges into puddles,
    and weeds in the wet slab.
  - North, back aisle: the flickering high-bay now hangs off one hanger on its
    chain at 5.5 m, swinging, its light swinging with it, spitting sparks.
  - South: a roof leak dripping into a mop bucket, with a wet-floor sign.
  - Oil drums on a spill tray by the maintenance shop, one knocked over and
    leaking; pallet jacks left in two aisles.
- **Clutter**: litter drifts (box blanks, stretch film, strapping, paper,
  cups, bottles, offcuts), broken pallets, and debris against stock, machines,
  and walls; extinguisher points, disconnect boxes, and notice boards on the
  walls; a dumpster, bin bags, tyres, weeds, potholes, and oil stains in each
  yard. Models: `tools/build_warehouse_detail_props.py` (Blender).

**Collision.** Litter, debris, cones, tape, signs, weeds, and wall dressing
never collide. A set piece that blocks has one simple shape: the wreck's box,
the switchboard's cabinet, the drums, the dumpster, the bucket. The rubble
mound is one low convex hull (0.3 m crown, slopes under 20 degrees), so
sprinting or sliding players ride over it. `warehouse_test.gd` checks the
wreck is mantle-height cover clipped flush with its chassis, the mound rises
smoothly, no clip box is an
ankle-high snag, and clutter casts no shadow.

**Cost** (GTX 1660 Ti, 1080p, GPU ms, before / after, six views): Low
5.5–7.3 / 5.6–8.1, Medium 9.6–11.9 / 9.9–12.4, High 12.1–14.6 /
12.5–15.5. Views add 0.1–0.8 ms (the floor's masks about 0.2 ms); looking
straight at the wreck's smoke adds about 1 ms. Warehouse was already over the
budget before this pass.

Rebuild:

```sh
blender -b --python tools/build_warehouse_detail_props.py
python3 tools/map_blockout/warehouse_photo_layout.py
uv run --with numpy --with pillow --with scipy tools/build_warehouse_floor_masks.py
xvfb-run -a godot --path godot --script res://tools/warehouse_build_headless.gd
```

### Variation pass (November 2026)

![Warehouse after the variation pass: the scanned office's break area and a desk room through its glass (top), the plate room's ladder and the truck apron's container (bottom)](media/warehouse-variation.jpg)

The user's direction: the map read as a mirror image, so take out duplicated
pieces and give the two halves their own identity with ideas from other Call
of Duty maps, make one office look like an office they scanned in real life,
and add a ladder up onto an area.

Call of Duty's maps are rarely mirrored: balance comes from matched walking
times and sightlines, while each side's rooms, yards, and ways up differ (see
"What the good ones share" above). The halves keep the 6v6 plan's footprints
and cover heights where they block lines, so the measures hold, and change
what stands in them:

| | North (team 0) | South (team 1) | Reference |
| --- | --- | --- | --- |
| Yard | Two box trucks, pallet stock, a forklift with its beacon, a covered car in the parking lot | One truck, a 20 ft shipping container where the second truck stood (same gate overlap), a wall of scrap bales three high, empty pallets | Shipment's and Crash's container yards |
| Door baffles | Four pallet stacks | Two pallet stacks, two of paper rolls | |
| Room roof | Ink room: crates to mantle | Plate room: a fixed ladder, the only way up | Crash's courtyard ladder onto a rooftop; Rust's ladder up from Maintenance |
| Wall department | Maintenance shop (walled room) | Open forklift charging bay: chargers, a forklift on charge, a spare battery, a guard rail | |
| Office, ground floor | The scanned office (below) | QC lab: a box compression tester, sample files | Vacant's and Atlas's lived-in rooms |
| Office, upstairs | Desks | QC test benches | |
| Effects | Sparks over the crossover, spilled sheets, beacon | Burnt switchboard's arcing (detail pass) | |

**The ladder** is the sim's ladder (`rust/sim/src/movement/ladder.rs`, Call of
Duty's grab, climb, slide down, and lowered weapon), marked with a
`LadderVolume` from the layout's `ladders` list. It is a 3.4 m yellow steel
ladder with 0.3 m rungs on the plate room's hall-facing wall, its stiles
carried 1.05 m past the roof edge as grab rails; one clip box fills the gap
between rungs and wall, no higher than the roof. The roof is a riskier perch
than the ink room's: the climb is slower than two mantles and the weapon is
down the whole way.

**The scanned office.** The user's phone LiDAR scan of a real in-plant office
(about 10.6 x 6.0 m inside, a 2.84 m ceiling) is the reference, not an asset:
like a Call of Duty map's photogrammetry, it was measured and rebuilt as clean
game models (`tools/build_warehouse_variation_props.py`), and the scan is not
in the repository. It fills the shift office's ground floor beside its stair:

- a desk room at each end behind a full-height glass partition with a black
  anodized frame and head track, its glass door standing open, and plank
  vinyl floors; a polished-concrete break area between;
- in the break area, a wall TV over a stainless table with a laser printer
  on its undershelf, a black mini fridge with a microwave and paper cups on
  it, framed woodland prints, two chairs, and a putting mat on the floor;
- grey oak-laminate desks with drawer pedestals and monitors, a filing
  cabinet, a jacket and a hi-vis vest on hooks;
- white panel walls, a suspended ceiling at 2.85 m with linear LED strips, a
  white-framed glass door to the back aisle, and a window over the plant.

The partitions, door, and window collide for players but carry
`surface = glass`, so rounds pass through them as they do Call of Duty's
interior glass: glass keeps up to 95% of a round's damage, less a share for
the pane's thickness (`rust/sim/src/penetration.rs`), about 91% for an M4
round through a partition straight on. Bots see, and are seen, through glass
and never take it for cover (`clear_line` in `rust/game/src/net/bots.rs`).
The way through jinks: the bay door (moved to the office's west end) into the
south desk room, its partition door at the east end into the break area, then
the north partition's door at the east end again. The partition doorways are
1.6 m wide under a 2.3 m head and the glass door 1.7 m by 2.4 m, so the bots'
navigation mesh (0.4 m radius, 1.8 m tall, baked in 0.25 m cells) runs through
each; `warehouse_test.gd` bakes it with the bots' settings and walks every
door and the stair. The glass draws with an unlit fresnel sheen
(`shaders/office_glass.gdshader`, single-sided, no fog): lit, double-sided
panes cost about 5 ms on Medium with a partition filling the screen. One lamp
lights all three rooms through the glass. Litter and wall dressing keep out of
the office (the layout's `clear` areas).

**Measures** (the built scene through `warehouse_grid.gd` and
`warehouse_measure.py`, which sample collision, so the office glass counts as
opaque there; the office is interior, off every long line): start to start 75 m (10.1 s) both before and after;
median longest eye-height line 20.4 m before, 21.0 m after; floor seeing past
40 m 2% before and after; longest line 50 m before, 48 m after.

**Cost** (GTX 1660 Ti, 1080p, GPU ms, before / after, at the new places):

| View | Low | Medium | High |
| --- | --- | --- | --- |
| Scanned office, break area | 5.87 / 6.18 | 10.21 / 10.88 | 13.13 / 14.11 |
| Through the office glass | 6.23 / 6.79 | 10.77 / 11.81 | 14.12 / 15.44 |
| Plate room ladder | 6.46 / 6.41 | 11.22 / 11.22 | 14.15 / 14.11 |
| Charging bay | 5.37 / 5.43 | 9.38 / 9.54 | 11.99 / 12.15 |
| Truck apron | 5.73 / 5.70 | 9.57 / 9.54 | 12.38 / 12.33 |
| Hall, centre looking north | 6.29 / 6.46 | 10.19 / 10.28 | 13.34 / 13.41 |

Rebuild:

```sh
blender -b --python tools/build_warehouse_variation_props.py
uv run --with trimesh --with numpy --with scipy tools/build_warehouse_clip.py shipping_container_blue qc_bench
# The office glass material is set in the models' .gltf.import files.
python3 tools/map_blockout/warehouse_photo_layout.py
uv run --with numpy --with pillow --with scipy tools/build_warehouse_floor_masks.py
xvfb-run -a godot --path godot --script res://tools/warehouse_build_headless.gd
```

## Breakwater, November 2026

Breakwater is the third 6v6 core map, built to contrast with Slamcity (a city
at dusk) and Warehouse (an industrial interior): a coastal fuel terminal on a
mole between two harbour basins, in late-morning sun. Its plan is
[`breakwater_2026_11.py`](../tools/map_blockout/breakwater_2026_11.py), in the
Black Ops 2 three-lane tradition. It takes structure, never a layout, from:

- **BO2's compact loops** (Raid, Standoff, Slums): a nearly square footprint,
  a central landmark with four ways in, and every route spawn to spawn about
  the same length.
- **A landmark with high ground each team climbs on its own side** (Slums'
  plaza, Standoff's market): the truck loading rack, an island deck under a
  canopy, set diagonally across the middle like Slamcity's derailed car, with
  a tanker parked in the bay on each side. Each team's stair climbs onto its
  own end of the deck; a crossing under the deck's middle joins the bays.
- **Lanes that change character halfway**, as Slamcity's do: the north route
  runs along Blue's quay (mid to long range, stacked containers and the
  loading-arm berth for cover) and then through the pump house into Red's
  tank farm (close quarters round the storage tanks); the south route is its
  turned twin. The main road is the mid-range lane into the rack.
- **Two-storey buildings near each team's side** that link the lanes: the
  control house (Red: the laboratory) with an upper floor over the main road
  and the quay, and the workshop (Red: the fire station) with a roof deck over
  the tank farm.

Each team's half is the other's turned 180 degrees about the centre, dressed
differently (white block and blue cladding against sand block and cream;
white tanks against silver; control room consoles against lab benches; a
lathe against a foam unit).

| Measure | Plan | Target |
| --- | --- | --- |
| Footprint | 84 × 74 m | 70 to 90 m a side |
| Spawn to spawn | 78.6 m (10.6 s) | 75 to 90 m |
| Spawn to the rack | 5.7 s and 5.8 s | 5 to 6 s, within 0.3 s |
| Median longest eye-height line | 29.0 m | 32 m or less |
| Floor that sees past 40 m | 7% | under 10% |
| Longest line | 58 m | 60 m or less |

### Building it

Breakwater is built the way Slamcity is, and reuses Slamcity's kit:

- [`tools/breakwater_layout.py`](../tools/breakwater_layout.py) describes the
  map once with Slamcity's primitives (`shell`, `Wall`, `stair`, `roof`, ...).
- [`tools/breakwater_models.py`](../tools/breakwater_models.py) models the
  terminal: the tanker trucks, storage tanks with their spiral stairs, pipe
  racks, pumps, switchgear, transformers, containers, the marine loading
  arms, bollards and fenders, and the moored product tanker and far tank farm
  beyond the boundary. Its `LOOKS` table adds the terminal's paints and
  claddings to Slamcity's looks.
- [`tools/build_breakwater_city.py`](../tools/build_breakwater_city.py) runs in
  Blender and writes `godot/assets/maps/breakwater/city.gltf`, merged by chunk
  and material with shadow-only proxies, as Slamcity's builder does.
- [`tools/build_breakwater.py`](../tools/build_breakwater.py) writes
  `godot/scenes/breakwater.tscn`. It refuses a layout that fails
  [`tools/breakwater_checks.py`](../tools/breakwater_checks.py), which CI also
  runs (`tools/test_breakwater_layout.py`): collision twinned; **no clear
  eye-height line from anywhere in one spawn yard to anywhere in the other**;
  no spawn point in sight of the enemy's spawns, upper floor, roof deck, or
  half of the deck; no line over 55 m across the ground or 50 m between the
  teams' high ground; Hardpoint moves of at least 16 m; and at least 15 spawn
  points hidden from our own high ground and 8 from an enemy inside our half.

The checks shaped the build: pipe bridges with clad screens cross the main
road at the gate and where it meets the rack (they close the deck's and the
far upper floors' lines down the road into the yard while the road stays open
under them); the control house's upper floor has no windows on its east face;
and a plant room and a louvred screen bound the workshop's roof deck, so it
fights over the tank farm rather than across the map to the other roof.

Rebuild:

```sh
uv run --with numpy --with matplotlib tools/map_blockout/blockout.py tools/map_blockout/breakwater_2026_11.py
blender -b --python tools/build_breakwater_city.py
python3 tools/build_breakwater.py
tools/bake_lightmaps.sh res://scenes/breakwater.tscn
MINIMAPS=breakwater godot --headless --path godot --script res://tools/build_minimaps.gd
```

### Playing Breakwater

Breakwater is on the title screen under Free Roam and in the local match
setup for every mode, Search and Destroy included; servers load it with
`--map breakwater`.

- **Spawns:** 20 per team, the first ones in the gate yard, then the gate
  road, the gate lane, and the ground floors of the control house and the
  workshop.
- **Bots** follow three lanes, west to east: the quay and the pump house; the
  main road, through the crossing under the deck; and the tank farm and the
  fire pumps.
- **Capture the Flag:** each team's flag stands in its gate yard by the main
  road's mouth.
- **Domination:** A on Blue's main road between the pickup and the analyser
  shelter, B in the crossing under the rack's deck (3 m circle), C A's twin
  on Red's main road.
- **Hardpoint** rotates through the Loading Rack (round the crossing), the
  Pump House, the Rack Deck (a floor up), and the Fire Pumps.
- **Search and Destroy:** Red defends. A is in the pump house's east room on
  the north route; B is on Red's quay by the containers on the south route.
