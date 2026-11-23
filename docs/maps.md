# Maps

Every shipped map: its layout, the modes it carries, its look, and how it is
built. How we lay maps out, and what we learned from real Call of Duty maps,
is in [map-design.md](map-design.md). How a scene marks its spawns and
objectives is under "Map markers" there.

| Map | Scene | Server key | Modes in the menu | Built from |
| --- | --- | --- | --- | --- |
| Slamcity | `scenes/slamcity.tscn` | `slamcity` | TDM, FFA, Gun Game, CTF, Domination, Hardpoint, Search and Destroy, Free Roam | `tools/slamcity_layout.py` |
| Warehouse | `scenes/warehouse.tscn` | `warehouse` | TDM, FFA, Gun Game, CTF, Domination, Hardpoint, Free Roam | `tools/map_blockout/warehouse_photo_layout.py` |
| Breakwater | `scenes/breakwater.tscn` | `breakwater` | TDM, FFA, Gun Game, CTF, Domination, Hardpoint, Search and Destroy, Free Roam | `tools/breakwater_layout.py` |
| Nuketown | `scenes/nuketown.tscn` | `nuketown` | TDM, FFA, Gun Game, CTF, Domination, Hardpoint, Free Roam | `tools/import_nuketown.py` (Black Ops 2's Nuketown 2025) |
| Testing Area | `scenes/main.tscn` | `arena` | Its own title-screen route; TDM, FFA, Gun Game, CTF, Domination, Hardpoint as "Testing Arena" | `map_testing_area` in `tools/gear_models.py` |
| Zombies Arena | `scenes/zombies_arena.tscn` | `zombies` | Zombies only | `map_zombies_arena` in `tools/gear_models.py` |

The menu lists come from `godot/scripts/tdm_setup.gd` (`MAPS`, `SND_MAPS`,
`ZOMBIES_MAPS`, `FREE_ROAM_MAPS`); a dedicated server takes
`--map arena|nuketown|slamcity|warehouse|breakwater|zombies`
(`godot/scripts/net/bootstrap.gd`). Search and Destroy is offered only on maps
that mark their sites. The five multiplayer maps have a minimap
(`godot/assets/minimaps/`); Zombies has none.

## Slamcity

![Slamcity: the avenue toward Slam Junction, the train under the El, a platform, the north street's alley, the diner, and the market](media/slamcity-junction.jpg)

A city block at dusk, just after rain, round Slam Junction, a station on an
elevated railway (the El). 84 x 68 m. It follows Call of Duty 4's city blocks
(Crash, Strike, District, Backlot) with Black Ops 2's compact loops (Raid,
Standoff, Slums). The plan is `tools/map_blockout/slamcity_2026_11.py`.

- **Spawns.** Blue (team 0) starts in the bus depot yard on the west edge, Red
  (team 1) in the depot on the east edge; 20 spawn points per team, the first
  ones in the yard, the rest along the depot lanes and in the ground floors
  nearby.
- **Three routes.** Slam Avenue (mid range) runs from each depot into Junction
  Plaza, where a car of the stalled train has fallen from the El and lies
  diagonally under the platforms. Each team has its own stair up to its
  platform (6.6 m), and the stalled train between the platforms is the way
  across. The north street runs past the tenement and the loading dock,
  through the pawn shop under the El, into the far market (close quarters).
  The market runs past the diner, through the ticket hall, into the far south
  street.
- **Buildings you can enter** join the routes: the tenement (an upper floor
  and a fire escape), the bodega, the diner (a roof terrace), the record
  store, and the subway hall.
- **Balance.** Red's half is Blue's turned 180 degrees about the centre,
  dressed as different places: the tenement is a hotel, the diner a
  laundromat, the pawn shop the ticket hall.

| Measure (plan, `blockout.py`) | Value |
| --- | --- |
| Spawn to spawn | 79.4 m (10.7 s sprinting) |
| Spawn to the plaza | 5.6 s, both teams |
| Median longest eye-height line | 30.2 m |
| Floor that sees past 40 m | 8% |
| Longest line | 59.5 m |

Objectives (all in `tools/slamcity_layout.py`):

- **Capture the Flag:** each flag in its depot yard, by the shed at the
  avenue's mouth.
- **Domination:** A and C on Slam Avenue between each depot and the plaza
  (about 17 m from their spawn), B round the plaza's kiosk with a wider
  circle (4.5 m).
- **Hardpoint:** Junction, Pawn Shop, The Platforms, Ticket Hall, in that
  order.
- **Search and Destroy:** A in the pawn shop's back room on the north street,
  B in the ticket hall's front room on the market side, both on the east half
  where the defenders spawn (sides swap at halftime).
- **Bots** follow three lanes: North, Middle, South.

**Built with:** `tools/slamcity_layout.py` describes the map once; Blender
builds its meshes (`tools/build_slamcity_city.py`, models in
`tools/slamcity_models.py`, signs from `tools/build_slamcity_signs.py`) into
`godot/assets/maps/slamcity/city.gltf`; `tools/build_slamcity.py` writes the
scene and refuses a layout that fails `tools/slamcity_checks.py` (also run in
CI by `tools/test_slamcity_layout.py`). The look and its grade are in
[art-direction.md](art-direction.md).

```sh
uv run tools/build_slamcity_signs.py
blender -b --python tools/build_slamcity_city.py
python3 tools/build_slamcity.py
tools/bake_lightmaps.sh res://scenes/slamcity.tscn
MINIMAPS=slamcity godot --headless --path godot --script res://tools/build_minimaps.gd
```

## Warehouse

A corrugated-box plant in daylight, rebuilt from reference photos of a real
plant. Its two sides are built separately and balanced by walking times and
sightlines, not by symmetry. It takes structure from Modern Warfare's Atlas
Superstore (a yard start at each end, big blocks in a grid with aisles
between, a middle under a breach in the roof, overwatch reached by a stair)
and from Call of Duty 4's Vacant (an overgrown lot against industrial
interiors). The plan is `tools/map_blockout/warehouse_2026_11b.py`.

- **The Field (team 0, north).** An overgrown lot where the west wing was torn
  down: grass over broken slab, a guard booth, a fence closing off the
  substation. The team enters through two doors in the north wall, a dock
  door in the west wall, and the hole a box truck punched through it (the
  crashed truck still in it).
- **The Docks (team 1, south).** A concrete truck apron 1.25 m below the hall,
  with scrap bales, barriers, pallets, and a shipping container. The team
  enters by a personnel stair, a vehicle ramp, or two dry vans backed into
  the bays: mantle up into the trailer from the apron and walk through into
  the hall.
- **North half: production.** Machine lines run north to south: the flexo
  line (with a 1.4 m crouch tunnel under a transfer conveyor), the die cutter,
  the ink room (two crate steps up to its roof), maintenance, and receiving.
  A steel catwalk at 2.8 m runs over the middle line, with a stair from the
  back aisle and a ladder at its south end. The two-storey shift office has a
  ground floor rebuilt from the owner's scan of a real in-plant office; its
  glass partitions stop players but not bullets.
- **South half: high-bay storage.** Pallet racking in east-west rows whose
  ends are staggered so no aisle runs through, a QC cell, a forklift charging
  bay, the wrapper and strapper, and the plate room, whose roof only a ladder
  reaches. The pick mezzanine is an open steel deck at 3 m with flow racks
  under it, a stair from the dispatch aisle, and a ladder toward the middle.
- **Middle.** The mid stockpile, a stack of paper rolls under a breach in the
  roof, with the fallen roofing round it and a sun shaft through the haze.
- **Set pieces.** A forklift on its side smoulders in receiving; a switchboard
  that arced and burnt out stands on the south half's east wall; a high-bay
  swings off one hanger over the north back aisle; a roof leak drips into a
  mop bucket.

There are 6 starts per team and 28 respawns either team can use, across the
aisles and both yards. Warehouse is roofed (`roofed` metadata on the scene's
root), so scorestreaks that need open sky are out
([scorestreaks.md](scorestreaks.md)).

| Measure | Plan (`blockout.py`) | Built scene (`warehouse_measure.py`) |
| --- | --- | --- |
| Start to start | 84.6 m (11.4 s) | 87 m (11.7 s) |
| Start to the mid stockpile, north / south | 5.9 / 5.6 s | 5.8 / 6.0 s |
| Median longest eye-height line | 29.0 m | 24.2 m |
| Floor that sees past 40 m | 7% | 6.5% |
| Longest line | 52 m | 53 m |

No team start sees into the other half on the built scene.

Objectives (in the scene, written by the builder):

- **Capture the Flag:** no flag markers; each flag stands at the centre of its
  team's six starts.
- **Domination:** A inside the north end of production, B at the mid
  stockpile, C in the dispatch aisle near the ramp.
- **Hardpoint:** Mid Stockpile, Receiving, Pick Mezzanine, Catwalk, Quality
  Control.
- **Bots** follow three lanes: West, Centre, East. Their direction runs from
  their own starts toward the enemy's, which on Warehouse is north to south.

**Look.** Daylight over the yards with the Warehouse horizon strip
(`tools/build_horizon.py --warehouse`) and terrain out past the fences; a
baked lightmap inside (`texel_scale` 0.7), with the high-bays as bake-only
lights and one failing high-bay left live so it can flicker. The slab
(`shaders/warehouse_floor.gdshader`) shades every pour on its own and reads
painted masks for dust, tyre tracks, puddles, oil, soot, and broken concrete
(`tools/build_warehouse_floor_masks.py`). The tilt-up walls
(`shaders/warehouse_tiltup.gdshader`, `tools/build_warehouse_wall_damage.py`)
use a scanned cast-concrete face with spalls, cracks, and repairs drawn per
panel, colour only. `scripts/prop_variation.gd` varies the colour and wear of
repeated stock when the map loads, by a hash of each node's name, so every
player sees the same loads. The Field's grass thins on Low. The soundscape
(`tools/warehouse_audio.py`, through `tools/build_world_audio.py warehouse`)
has its own beds for the Field, the Docks, the hall, and the breach.

**Built with:** the shell (walls, floors, roofs) is Cyclops blocks, kept
editable in the editor. Everything else is generated:

- `tools/map_blockout/warehouse_photo_layout.py` places everything from the
  plan (helped by `warehouse_walkways.py`, `warehouse_detail_layout.py`,
  `warehouse_outdoors.py`, and `warehouse_dressing_layout.py`) and writes
  `godot/tools/warehouse_photo_layout.json`.
- Blender builds the models into `godot/assets/props/warehouse_photo/`:
  `tools/build_warehouse_photo_props.py` (stock, building parts, and the
  picked, knocked, torn, and crushed stock variants),
  `build_warehouse_photo_machines.py`, `build_warehouse_detail_props.py`,
  `build_warehouse_variation_props.py`, `build_warehouse_walkways.py`,
  `build_warehouse_field.py`, and `build_warehouse_outdoors.py`. Textures come
  from `tools/build_warehouse_photo_textures.py`.
- `tools/build_warehouse_clip.py` voxelizes each model and writes fitted clip
  boxes to `clip.json`.
- `godot/tools/warehouse_photo_build.gd` rebuilds the scene from the JSON;
  `godot/tools/warehouse_build_headless.gd` runs it outside the editor
  (`WAREHOUSE_PARTS` picks parts). It needs a real renderer: under
  `--headless` MultiMesh instances save empty, so it refuses.

```sh
python3 tools/map_blockout/warehouse_photo_layout.py
uv run --with numpy --with pillow --with scipy tools/build_warehouse_floor_masks.py
uv run tools/build_warehouse_wall_damage.py
xvfb-run -a godot --path godot --script res://tools/warehouse_build_headless.gd
tools/bake_lightmaps.sh res://scenes/warehouse.tscn
MINIMAPS=warehouse godot --headless --path godot --script res://tools/build_minimaps.gd
GRID_OUT=grid.json godot --headless --path godot --script res://tests/warehouse_grid.gd
uv run --with numpy --with matplotlib tools/map_blockout/warehouse_measure.py grid.json --centre=18.75,3.25
```

`godot/tests/warehouse_test.gd` checks spawns, clearance, prop collision,
the access structures, dock traversal, and entry through the menu;
`godot/tests/prop_variation_test.gd` checks the stock variation.

## Breakwater

![Breakwater from above, the control house over the main road, a tanker under the loading rack, the quay, the rack's deck, and the tank farm](media/breakwater.jpg)

A coastal fuel terminal on a mole between two harbour basins, in late-morning
sun. 84 x 74 m, in the Black Ops 2 three-lane tradition: compact loops round a
central landmark (Raid, Standoff, Slums). The plan is
`tools/map_blockout/breakwater_2026_11.py`.

- **Spawns.** Blue (team 0) at the truck gate on the west edge, Red (team 1)
  at the east gate; 20 spawn points per team, from the gate yard out to the
  gate road, the gate lane, and the ground floors of the nearest buildings.
- **The loading rack** is the landmark: an island deck under a canopy, set
  diagonally across the middle, with a tanker in the bay on each side. Each
  team climbs onto its own end of the deck by its own stair; a crossing under
  the deck's middle joins the bays.
- **Three routes.** The main road (mid range) runs from the gate into the
  rack. The north route runs along the quay past the loading arms and the
  moored tanker (mid to long range), then through the pump house into the far
  tank farm (close quarters round the tanks). The south route is its turned
  twin, through the fire pumps.
- **Buildings you can enter:** the control house (an upper floor over the
  main road and the quay), the meter shed, the workshop (a roof deck over the
  tank farm), the drum store, and the pump shed.
- **Balance.** Red's half is Blue's turned 180 degrees, dressed differently:
  the control house is the laboratory, the workshop the fire station.

| Measure (plan, `blockout.py`) | Value |
| --- | --- |
| Spawn to spawn | 78.6 m (10.6 s) |
| Spawn to the rack | 5.7 and 5.8 s |
| Median longest eye-height line | 29.0 m |
| Floor that sees past 40 m | 7% |
| Longest line | 58 m |

Pipe bridges with clad screens cross the main road at the gate and where it
meets the rack, so the deck and the far upper floors cannot see down the road
into the spawn yard; the control house has no windows on its east face; and
a plant room and a louvred screen bound the workshop's roof deck, so it fights
over the tank farm rather than across to the other roof.

Objectives (all in `tools/breakwater_layout.py`):

- **Capture the Flag:** each flag in its gate yard, by the gatehouse at the
  main road's mouth.
- **Domination:** A and C on each team's main road, B in the crossing under
  the rack's deck (3 m circle).
- **Hardpoint:** Loading Rack, Pump House, Rack Deck (a floor up), Fire Pumps.
- **Search and Destroy:** A in the pump house's east room on the north route,
  B on Red's quay by the containers on the south route.
- **Bots** follow three lanes: North, Middle, South.

**Built with:** the same pipeline as Slamcity, reusing its primitives and
models. `tools/breakwater_layout.py` describes the map;
`tools/breakwater_models.py` models the terminal (tankers, storage tanks with
spiral stairs, pipe racks, loading arms, the moored tanker, the far tank
farm); `tools/build_breakwater_city.py` builds
`godot/assets/maps/breakwater/city.gltf` in Blender;
`tools/build_breakwater.py` writes the scene and refuses a layout that fails
`tools/breakwater_checks.py` (also run in CI by
`tools/test_breakwater_layout.py`).

```sh
blender -b --python tools/build_breakwater_city.py
python3 tools/build_breakwater.py
tools/bake_lightmaps.sh res://scenes/breakwater.tscn
MINIMAPS=breakwater godot --headless --path godot --script res://tools/build_minimaps.gd
```

## Nuketown

Black Ops 2's Nuketown 2025 (`mp_nuketown_2020`), converted from the game's
files: two houses facing each other across a cul-de-sac, with yards, garages,
the bus, and the population sign. Its geometry and materials are the
original's. The runtime assets live in Git LFS under
`godot/assets/maps/nuketown/`; a build without them shows "Nuketown is not
installed in this build." and does not load it.

- **Spawns.** `scripts/nuketown.gd` builds `SpawnPoints` from
  `godot/data/nuketown_spawns.json` (29 points over both yards, houses, and
  lanes). The six outermost ground points on each side are that team's
  starts; every point is a respawn.
- **Capture the Flag:** no flag markers; each flag stands at the centre of its
  team's starts.
- **Domination:** three authored flags (A, B, C).
- **Hardpoint:** Cul-de-sac, West House, Front Lot, East House.
- **Bots** have no lanes; they move on the navigation mesh baked round the
  playable area.
- **Interactive props** from the original: cars that smoke, burn, and explode;
  mannequins and loose props that break into debris; the population sign,
  which counts the players on the server.
- **Easter egg:** a wall near the spawn leads to a vault holding the Prism
  Laser.

Its look is a graded desert noon (`scripts/looks.gd`); lighting in
[nuketown-lighting.md](nuketown-lighting.md). The conversion, collision
fixes, props, and the easter egg are in [assets/nuketown.md](assets/nuketown.md).

## Testing Area

A 64 m square concrete training range by day, inside 6 m precast walls with
open country beyond (a horizon strip from `tools/build_horizon.py`). It is
where players try movement and weapons offline: an armoury line of weapon
benches to pick guns from, target dummies and swinging steel plates in front
of the east wall, a lined culvert to crawl through, a steel platform with a
ramp, two block walls for tall cover, and a control tower with a ladder. Only
offline, `T` refills all ammo here.

- **Spawns.** None are authored, so a server lines the teams up round the
  player start, 14 m apart, and CTF and Domination fall back to points worked
  out from those spawns.
- **Hardpoint:** Crossroads, The Platform, Tunnel, Target Range.
- **Bots** have no lanes.

**Built with:** `map_testing_area` in `tools/gear_models.py`, dressed by
`tools/map_dressing.py` (per-slab shading, stains, the tower, office,
container, targets, sandbags, floodlights), built by
`blender -b --python tools/build_models.py`. Hidden CSG in the scene keeps the
collision. The dressing is described in [art-direction.md](art-direction.md).

## Zombies Arena

A walled compound 48 m square at night under a moon and a far skyline:
eight boarded window bays with alcoves behind them where zombies gather, a
gate east and west into side yards, each blocked by debris players clear
for points, wall weapons, five
drink machines, three mystery crate spots, a power switch, an electric fence
trap across the west gate, and spawn points for hounds. Zombies only, solo or
co-op (`--map zombies`). The mode and the arena's machines are in
[zombies.md](zombies.md).

**Built with:** `map_zombies_arena` in `tools/gear_models.py` and
`tools/map_dressing.py`, like the Testing Area, with its floodlights baked
into the lightmap (`tools/bake_lightmaps.sh res://scenes/zombies_arena.tscn`)
and its skyline from `tools/build_horizon.py --night`.
