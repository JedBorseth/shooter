# Maps

Every shipped map: its layout, the modes it carries, its look, and how it is
built. How we lay maps out, and what we learned from real Call of Duty maps,
is in [map-design.md](map-design.md). How a scene marks its spawns and
objectives is under "Map markers" there.

| Map | Scene | Server key | Modes in the menu | Built from |
| --- | --- | --- | --- | --- |
| Breakwater | `scenes/breakwater.tscn` | `breakwater` | TDM, FFA, Gun Game, CTF, Domination, Hardpoint, Search and Destroy, Free Roam | `tools/breakwater_layout.py` |
| Testing Area | `scenes/main.tscn` | `arena` | Its own title-screen item, and the Map row of every match setup but Search and Destroy | `map_testing_area` in `tools/gear_models.py` |

Breakwater is the one multiplayer map. The match setup's map row
(`MAPS` in `godot/scripts/tdm_setup.gd`) offers it and the Testing Area,
Breakwater first; Search and Destroy, which needs marked sites, is played on
Breakwater with no map row, and Free Roam loads Breakwater straight from the
title screen. A dedicated server takes `--map breakwater|arena`
(`godot/scripts/net/bootstrap.gd`), Breakwater by default. Every map has a
minimap (`godot/assets/minimaps/`).

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

**Dressing.** Besides the terminal's own modelled plant, Breakwater places
props from the library (`prop()` in the layout; the scene points kit pieces
at their kit's folder): oil drums on yellow spill pallets (`drums()`), the
chain-link fence kit along the west boundary (`fence_run()`), kit downpipes
with a shoe, hopper head, and scupper on the garage row, the control house,
the workshop, the pump house, and the meter shed (`downpipe()`), flood lights
and isolator boxes over doors, and bins, tyres, jerrycans, gas bottles, a
compressor, a welding cart, a tool chest, a steel rack, and an office desk
(`dressing()`). Pallets are modelled EUR pallets (1.2 x 0.8 m: bottom boards,
nine blocks, stringers, five deck boards in three weathered woods), stacked
with a little turn and shift each. Anything a player could stand behind has a
blocker, so collision stays twinned. In the game, props shorter than 1.3 m
cast no sun shadow (theirs is in the lightmap) and hide beyond 45 m
(`scripts/breakwater.gd`), like the map mesh's detail.

**Interiors and shade.** Every room carries its services (`interiors()`):
galvanised cable trays with drops to the switchgear, distribution boards,
staff lockers and a microwave in the mess, filing cabinets upstairs, a green
pipe header high through both pump rooms to a valve, racks, totes and a
caged work lamp in the workshop and stores. Ceilings carry twin-tube
battens (`batten`) and the rack's canopy eight LED floodlights
(`canopy_flood`), all lit and baked (`light()`), so the canopy and interiors
read as lit spaces in daylight, as a working terminal's do. Painted steel is
paint, not bare metal: the Breakwater looks keep paint dielectric (metallic
paint lost its diffuse light in baked shade with no reflection probes), and
roof undersides are off-white liner panels (`soffit`).

**Beyond the boundary** (`backdrops()`, the `far` chunk) the land is laid in
parts that never overlap, each with Red's twin: the landside west of the gate
(the access road with its markings and footway, a lorry park with waiting
tankers, a car park, the terminal office, a canteen, a weighbridge hut, the
depot workshop and fire station, light masts, dry-grass verges, and the quay
face along the basin) and the main tank farm south of the map (bunded
compounds with gravel floors, big tanks, roads, and pipe tracks on sleepers),
whose twin runs on east of Blue's basin. The far chunk casts no shadows, so
far tanks, sheds and bund walls get a fake sun shadow: a multiply stain swept
north by the sun (`far_shadow_box`, `far_shadow_round`). The far geometry is
split into one mesh per quadrant (`far_nw` ... `far_se`) so each culls on its
own, and its fine detail (`far_*_detail`: rails, marks, fittings) stops
drawing beyond 90 m; one far mesh drawn whole cost about 0.4 ms on Medium.

**Built with:** the shared generator kit first written for Slamcity, a cut
map: the primitives in `tools/slamcity_layout.py`, the models in
`tools/slamcity_models.py`, the mesh builder in `tools/build_slamcity_city.py`,
the looks in `tools/world_looks.py`, and the chain-link pattern from
`tools/build_slamcity_signs.py`. They keep their names because Breakwater's
mesh hash covers their bytes. `tools/breakwater_layout.py` describes the map;
`tools/breakwater_models.py` models the terminal (road tankers with polished
aluminium barrels drawn by `shaders/polished_alu.gdshader`, storage tanks
with staggered plate welds, anchor chairs, flanged nozzles, gate valves,
foam chambers and spiral stairs, pipe racks, loading arms, the additive skid
and manifold, the moored product tanker with its catwalk, manifold and hose
crane, stepped accommodation, bridge wings, funnel, freefall lifeboat and
accommodation ladder, and the far tank farm); `tools/build_breakwater_city.py` builds
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
The perimeter wall's precast texture repeats every 5.3 m on 4 m panels, so
neighbouring panels never show the same stains; the floor's slabs vary more in
shade and carry small damp and oil patches as well as the broad ones; the
sandbags are slumped and squarer, with the folded end tucked under; the
culvert is darker, weathered concrete.
