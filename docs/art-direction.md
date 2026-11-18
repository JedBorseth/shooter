# Art direction

How the game should look, and how the characters (other players, range dummies, and the player's own arms) are built.

![Both teams in several poses, the F4 hitbox overlay, and close-ups of an enemy and an ally](media/characters.jpg)

## The look

The target is a **Treyarch Black Ops game (Black Ops 2 and 3) built with the craft of the modern Modern Warfare games**: bold colour, a strong key light, clean readable shapes, and punchy effects, finished with photoreal materials and wear, grounded bounce light, detailed weapons, dense dressing, and effects that linger. Where the two pull apart, readability and colour win. The full target, what to leave out, and how to check a change are in the cod-design skill's [graphics reference](../.agents/skills/cod-design/references/graphics.md).

The game aims for **grounded stylized realism**: real materials (brick, concrete, painted steel, worn wood) on simple, readable shapes, under clean light. Surfaces use CC0 PBR texture sets, so they read as what they are up close. Shapes stay low-poly and hard-edged, so silhouettes and sightlines stay clear and the game stays cheap to run (see [performance.md](performance.md)).

Rules for every character, prop, and map:

- **Read the team at a glance.** The two teams differ in value first: allies wear light tan and olive, enemies wear dark charcoal. They differ in hue second: allies have a blue accent and enemies a red-orange accent, on the helmet band and the armbands. Value contrast survives colour blindness, fog, and distance; the accent confirms it up close.
- **Make the head obvious.** A helmet with a coloured band, dark goggles, and a face mask give the head a shape and colour that differ from the torso, so headshots are aimable at range.
- **Keep shapes simple.** Gear is a few dozen faceted triangles each: a helmet dome, a plate carrier with three magazine pouches and a radio, a belt with side pouches, knee pads, and armbands. Avoid noisy camouflage patterns that break up the silhouette.
- **Keep accents rare.** The team colour appears only on the helmet band and armbands, with a little self-light so it reads in shade. Everything else is muted.
- **Texture detail stays quiet.** A texture adds material, not noise: no high-contrast patterns, decals, or graffiti where players look for enemies. Characters must stay the most contrasting thing in a scene.

### World surfaces

![Slamcity and the Testing Area before (left) and after (right) the PBR world textures](media/world-textures.jpg)

Level geometry uses one shader, `godot/shaders/world.gdshader`. It projects a texture set in world space from the axis each face points along most, so boxes need no UVs, textures never stretch when a box is resized, and each pixel samples its textures once. Low-frequency noise varies brightness and roughness across large surfaces, so repeats don't show, and walls darken toward the ground with grime.

`tools/build_textures.py` builds the library from the tables in `tools/world_looks.py`:

- **Texture sets** (`godot/assets/textures/<set>/`): pinned CC0 sets from [ambientCG](https://ambientcg.com) at 1024², packed into an albedo, an OpenGL normal map, and an ORM map (R ambient occlusion, G roughness, B metallic). A set marked `grey` stores brightness only, so one set serves many paint colours.
- **Looks** (`godot/materials/world/<look>.tres`): a set plus a tint, the texture's size in metres, roughness and metallic scales, and how much grime. Maps reference looks by name. Slamcity has its own look table, `SLAMCITY_LOOKS` in `tools/world_looks.py` (see Slamcity below).

To add a surface, add a set or a look to the tool, run it, import, and run it again with `--fix-imports` (its docstring has the commands). Textures import VRAM compressed with mipmaps; normal maps use two-channel compression, and the shader rebuilds their blue channel.

Curved shapes switch projection axis where their normal turns past 45°, which shows as a seam on strongly patterned textures. Use calm textures on cylinders, or give modelled props their own UVs.

### Skyline windows

![Slamcity's skyline before (left) and after (right) the windows](media/skyline-windows.jpg)

The skyline buildings beyond the boundary are plain boxes, so the world shader draws their windows: a look with a `windows` entry in `tools/world_looks.py` uses the `world_windows.gdshader` variant and gets a grid of framed windows with sills on every wall above the ground floor. Behind each pane is a room that exists only in the shader (interior mapping): the view ray is intersected with a box behind the glass, and the wall, floor, or ceiling it hits is coloured, with a door or shelf on the back wall, blinds drawn part way in some windows, and a few rooms lit warmer than the rest. Every room differs by a hash of its cell, the glass reflects the sky through the engine's own specular light, and the edges are antialiased from screen-space derivatives so distant windows do not shimmer. It costs no geometry or textures, and parallax makes the rooms move correctly as the player walks past.

On the buildings players fight around, a fake window could read as a firing position. Slamcity's panes there keep their blinds drawn and their rooms dim (see Slamcity below); elsewhere, keep fake windows off those buildings.

### Character relief

Characters carry their detail in baked maps, not in the shader. `tools/build_character_assets.py` bakes each kit into one texture atlas: albedo, a tangent-space normal map (cloth folds, ripstop, MOLLE webbing, Cordura, stitching, and leather grain), and ORM (ambient occlusion, roughness, metallic). See [Building a character](#building-a-character).

### Props

![Covered cars, street lamps, crates, bins, barrels, jersey barriers, and roof plant across the three maps](media/map-props.jpg)

Props are CC0 models from [Poly Haven](https://polyhaven.com/models), built by `tools/build_props.py` in Blender: it downloads the 1K glTF, keeps the named parts, decimates to a triangle budget (1.5k to 6k; the textures carry the fine detail), puts the origin at the bottom centre, and writes `godot/assets/props/<name>/<name>.gltf` with its textures beside it. Godot generates LODs at import.

Props never change how a map plays. Each one that should block players or bullets stands on a box in a hidden CSG combiner (`Blockers`, `CoverBlockers`, or the map's existing combiners, now hidden), so collision stays exactly as simple as the blockout and impacts still report a surface. Crate stacks are scaled to fill their old blockout box; jersey barriers are scaled to fill the old low cover.

### Modelled game pieces

Everything a player looks at is a modelled mesh, built in Blender; Godot's primitive meshes and CSG stay for collision, effects, and debug shapes only. The Testing Area (a concrete training range: cast slabs, precast walls, interlocking-block cover, a lined culvert, a clad steel platform and ramp) and the Zombies arena (a walled compound at night: block walls between brick piers under barbed wire, boarded window bays with rubble-strewn alcoves behind, brick columns, broken slabs on the dirt) are `map_*` models, built the same way and baked into their lightmaps; each sits exactly on the map's old CSG, which stays as hidden collision. The game's own pieces (the sentry, the weapon racks, the Zombies window boards and door barricades, the power-ups, the Capture the Flag pole and plinth, and the hidden laser with its vault and pedestal) are procedural models in `tools/gear_models.py`, built with Slamcity's model API (bevelled boxes, hulls, subdivided cages, lofts, tubes, and 3D text). `blender -b --python tools/build_models.py` writes each to `godot/assets/models/<name>/<name>.gltf`, and its materials, folded from the same looks as Slamcity's, to `godot/materials/models/`. Those project their textures in the model's own space (the world shader's `object_space`), so a turning sentry head, a spinning power-up, or a board torn off a window keeps its texture as it moves. The laser draws through the viewmodel shader, which reads standard materials, so it exports plain glTF materials instead. `tools/test_models.py` fails in CI when a model is missing or older than its source, or when a prop, Zombies piece, or weapon scene builds a primitive mesh again.

### Slamcity

![Slamcity: Slam Avenue towards Slam Junction, the derailed car under the platforms, a platform, the fire escape off the north street, the diner, and the market](media/slamcity-junction.jpg)

Slamcity is a city block at dusk round Slam Junction, a station on an elevated railway. Its layout comes from studying real Call of Duty maps ([map-design.md](map-design.md)): city blocks you can enter with streets cut between them, round a central plaza. Every surface of it is generated, so the whole map is rebuilt from code:

- `tools/slamcity_layout.py` is the one description of the map: collision, visual parts (bevelled boxes, hulls, cylinders, tubes, 3D and neon letters, pictures from the sign atlas, and window panes), placed models, props, decals, baked lights, spawns, lanes, and zones. Walls are written once with their doors and windows (`Wall`), and Red's half is Blue's turned half round the centre, dressed as different places with its own names, colours, and interiors (`Side`): the tenement is a hotel, the diner a laundromat, the pawn shop the ticket hall.
- `tools/slamcity_models.py` models the cars, buses, trucks, the train, water towers, the shop interiors, and the street furniture from the same primitives. Solid stairs are one stepped extrusion (the `extrude` part: any flat polygon swept along a vector, its caps split into convex pieces), so their sides bake without seams. `tools/build_slamcity_city.py` runs in Blender, builds every part and model, merges them by chunk (a 3 x 2 grid, a finer grid for small things, and the skyline) and by material, and writes `godot/assets/maps/slamcity/city.gltf`. Each chunk also gets a shadow-only twin with all its opaque geometry in one surface.
- Looks come from `tools/world_looks.py`. Textured looks that share a texture set and scale share one material, their tint in the vertex colour; plain colours and lights fold into three palette materials. The plain colours (and the vehicle paint, more lightly) carry a shared smudge map, `world_wear` (`assets/textures/noise/wear.png`), that only ever makes them a little rougher and darker, so counters, appliances, and fittings are not flat colour. About 50 materials cover the map. `godot/tools/slamcity_import.gd` swaps them in at import.
- Windows players cannot use are panes with a room behind the glass (`shaders/window_pane.gdshader`, the same interior mapping as the skyline). On the buildings players fight around, their blinds are drawn and the rooms dim, so they never read as firing positions.
- `tools/build_slamcity_signs.py` draws every billboard, poster, shop sign, package, and route map into one atlas. Every brand and name on them is made up.
- `tools/build_slamcity.py` writes the scene and refuses a layout that fails `tools/slamcity_checks.py`: collision must match its twin turned half round (except the furniture of rooms each half dresses differently, and the derailed car across the centre); no spawn point may be in sight of the enemy's spawns, upper floor, platform, train, or roof terrace; no line longer than 50 m may run clear at eye height across the streets and the plaza, between the teams' high ground (lines that thread both rows of the stalled train's windows are allowed: keyholes, not lanes), and none longer than 35 m from a platform down its own team's streets; at least 15 spawn points must hide from the team's own high ground and 8 from an enemy pushed into the middle of the team's half. Every solid collider and stair blocks sight in these checks, the derailed car's turned boxes included. CI runs them (`tools/test_slamcity_layout.py`), with hashes that catch a scene, mesh, or lightmap left stale against the layout; `godot/tests/slamcity_test.gd` checks the spawn sightlines again with real physics rays and bakes a navigation mesh to prove bots can reach every zone and flag.

![Slamcity's plan as a minimap: Blue's depot on the west edge and Red's on the east, the north street, Slam Avenue, and the market as the three routes, the tenement and hotel, the diner and laundromat, the pawn shop and the ticket hall under the El, and Junction Plaza with the derailed car in the middle](media/slamcity-plan.jpg)

The layout is 84 x 68 m. Blue spawns in the bus depot on the west edge and Red in the depot on the east; three routes join them: Slam Avenue into Junction Plaza (mid range, under the platforms), the north street through the pawn shop into the far market (close quarters), and the market through the ticket hall into the far south street. Buildings you can enter link the routes: the tenement (with an upper floor over the avenue and the plaza, reached by its stair or by the fire escape from the north street), the bodega, the diner (a roof terrace over the market), the record store, and the subway hall. Each team has its own stair up to its platform; the stalled train between them is the way across. Spawn to spawn is a 79 m walk, and both teams reach the plaza in 5.6 s.

Hardpoint rotates through four zones: Junction, the Pawn Shop, the Platforms, and the Ticket Hall. Junction and the Platforms are their own twins; the Pawn Shop and the Ticket Hall are each other's, so their lean towards one team cancels over a rotation. Every move goes at least 16 m to a zone whose footprint does not touch the last one's (`check_hardpoint_hops`).

The map's set piece is the derailed car: a car of the stalled train that came off the El at the station and lies across the plaza under the platforms, sunk to its floor in the paving, its steel torn and scorched at the nose, and its nose still burning (`godot/scenes/effects/crash_fire.tscn`: flame tongues (`shaders/flame.gdshader`: additive, tapering, torn by upward-scrolling noise) and embers over a baked glow, under a smoke column of large noise-edged, sunlit puffs that bends away over the roofs, `smoke_plume.tscn` with `shaders/smoke.gdshader`). Players walk through it by its doors and its torn nose; wreckage across the aisle between each pair of facing doors keeps it cover to move through rather than a window across the plaza.

#### Decals and trim

Slamcity's road markings (the crossings, centre lines, edge lines, and parking bays) are thin painted strips in the city mesh, baked into the lightmap: as decals they cost about a third of a millisecond. Its other surfaces carry CC0 ambientCG decals (`tools/build_decals.py` packs them into `godot/assets/textures/decals/`): patched and broken asphalt, manholes, rain streaks under the roof edges, and puddles and oil stains the tool generates (their ORM map makes puddles nearly mirror-smooth, so they catch the sky). Following the rule that texture detail stays quiet, there is no graffiti where players look for enemies.

Visual trim has no collision, so it never changes movement or cover: window frames, sills, lintels, cornices, quoins, and downpipes on the buildings.

### Light

![Slamcity, the Testing Area, and Zombies before (left) and after (right) the lighting pass](media/lighting.jpg)

Each map is lit by a CC0 sky image (an HDRI from [Poly Haven](https://polyhaven.com), downloaded by `tools/build_skies.py` into `godot/assets/skies/`) and one directional light standing in for its sun or moon. The tool prints where the sun sits in each image; the map rotates its sky (`Environment.sky_rotation`) so the sun lines up with its light, and tilts the light to the same elevation. The sky also lights the shade (ambient light from the sky), so shadows pick up its colour instead of a flat grey.

| Map | Sky | Mood |
| --- | --- | --- |
| Testing Area | `kloofendal_48d_partly_cloudy_puresky` | Clear midday with clouds, high sun, light haze |
| Slamcity | `kloppenheim_06_puresky` | Dusk: a low sun due south behind the storefronts (9°, above the sky's own 5° so the streets keep some light), long shadows, warm haze, and neon, lit windows, and street lamps baked into the lightmap |
| Zombies | `qwantani_moon_noon_puresky` | Moonlit night with stars, blue moonlight, dim fog, and street lamps that flicker (`scripts/lamp_flicker.gd`) |

![Before (left) and after (right) the baked lightmaps: Slamcity's street, a corner store, the alley, and the Testing Area](media/lightmaps.jpg)

Each map also has a baked LightmapGI: three bounces of sunlight and sky light, so rooms, the walkway, and the shade under awnings get soft bounced light and darken where the sky can't reach. `tools/bake_lightmaps.sh` bakes it in two steps:

1. `godot/tools/bake_map_meshes.gd` turns any visible CSG left in a map into meshes with lightmap UVs (saved in `godot/assets/maps/<map>/`), places them beside the CSG, and hides the CSG, which keeps its collision and impact surface. Running it again replaces what it made. Every map is now modelled (the Testing Area and Zombies arena are `map_*` models from `tools/gear_models.py`, Slamcity is `city.gltf`), so it finds nothing to do; the CSG stays only as hidden collision. Models and props get lightmap UVs at import.
2. The editor plugin in `godot/addons/lightmap_baker/` opens each scene in the editor (switching to its tab if the editor stays on the project's main scene), presses its own Bake Lightmaps button, and saves. It needs a GPU (Xvfb works with one) and does nothing unless `BAKE_LIGHTMAPS` is set.

The baker can report success and still write an all-black lightmap when it loses the GPU partway (another process rendering at the same time), which darkens the whole map. So the script then checks every lightmap it baked (`godot/tools/check_lightmap.gd`, using `godot/tools/lightmap_check.gd`) and fails on a black one, and CI checks the committed lightmaps the same way (`godot/tests/lightmaps_test.gd`). Bake with the GPU otherwise idle.

Rebake a map after changing its geometry or light. Lights that exist only to be baked (Slamcity's ceiling lights in the flats and stores) sit under a node with `scripts/baked_lights.gd`, which removes them in the game: their light is already in the lightmap and its probes, which also light characters.

Zombies adds volumetric fog, which only the High preset draws.

### The graded picture

![The Testing Area, Slamcity, and Zombies before (left) and after (right) the grade, the bloom change, and the lens](media/graded-picture.jpg)

The final picture of a map is set in `godot/scripts/looks.gd`, not in its scene, so a map that a tool regenerates keeps it. For each listed map it holds:

- **Environment overrides**: the tone curve, exposure, haze, ambient occlusion, and changes to the shared bloom. The Testing Area uses the ACES filmic curve for hard daylight; Slamcity and Zombies keep AgX, which holds the hue of neon and lamps.
- **The sun** (`sun`): the map's sun can be set stronger against the sky light. The Testing Area's sun is 3.2 rather than its scene's 2.0, with less exposure, so sunlit ground nears white while shade goes dark; the baked bounce light stays as it was baked.
- **Bloom** (`Looks.BLOOM`, the same for every listed map): only what is brighter than white blooms (`glow_bloom` is 0), and the glow is added like light scattered in a lens rather than screened over the picture. A faint wide level gives the sun, lamps, and muzzle flashes a soft halo. Zombies makes up for the lost veil with exposure, and blooms a little lower and stronger, so the horde reads and the lamps carry the night.
- **Lens dirt** (`scripts/lens_dirt.gd`): smudges and specks built in code from a fixed seed. The glow pass multiplies its bloom by them (`Environment.glow_map`), so the dirt shows only where something bright blooms through it.
- **A colour grade**: an S-curve on brightness around a pivot, a toe that deepens the darkest shadows without clipping them, a cool tint in the shadows and a warm one in the highlights that both fade out at black and white, and a saturation. A bright colour that would leave the range is pulled toward its own grey, so it keeps its hue. `Looks.build_lut` turns the grade into a 32³ look-up table when the map loads, and the renderer samples it inside the tonemap pass, so a grade costs nothing per frame. Tune a grade offline against a screenshot before you try it in the engine: the maths is a few lines. The renderer reads the table at texel centres with no half-texel correction, so texel *i* must hold the grade of input (*i* + 0.5) / 32 (`Looks.texel_input`); a table built as *i* / 31 darkens every shadow by more than 1% of full brightness.

| Map | Grade |
| --- | --- |
| Testing Area | Hard midday: a filmic curve, a strong sun, deep shade, cool shadows, warm highlights |
| Zombies | Cold night: muted colour, blue shadows, warm lamp highlights, a low pivot and a little more sky light so walls separate from the dark |
| Slamcity | Dusk: firmer contrast with a toe that keeps the near street's blacks under the haze, and less sky colour in the near haze, so only the distance fades; cool shade against warm lamp and sun light. Its environment overrides keep bloom to the neon, the lamps, and the lit windows (`glow_hdr_threshold` 1.3), so sunlit surfaces never glare |

Nuketown is not listed and keeps the look authored in its scene.

On top of that, for every map:

- **The lens** (`scripts/lens.gd`): a vignette and a little film grain, and on High a contrast-adaptive sharpening pass. The vignette deepens slightly while aiming. Rounds passing close (`suppress()`) close it in and drain some colour for a moment, and on High also blur the edges of the picture. After death the picture fades down.
- **The sun in the lens** (`shaders/lens_flare.gdshaderinc`, Medium and High): when a map's sun is in view, a glare around it, a few faint coloured ghosts on the line through the middle of the screen, and the lens dirt lit up near it. Five rays from the camera test whether the sun is hidden, so the flare fades as the sun slips behind an edge instead of blinking off. Each map sets its strength (`flare` in `MAPS`; the night has none).
- **Contact shadows** (`scripts/grounding.gd`): a soft dark patch on the ground under every soldier and zombie, so they stand on the ground in the shade and where the sun's shadow is soft or far. Each is a flat quad drawn with `shaders/contact_shadow.gdshader` and laid on the ground (tilted to a slope) by a short ray down from the feet a few times a second; it fades as the feet leave the ground and between 60 and 80 m. It reads no depth and is no Decal, so it costs only the pixels it covers.
- **The scope** (`shaders/scope.gdshader`): one pass that multiplies the picture: black outside the tube, a soft eye-relief shadow that slides to one side as sway and turning move the scope off the eye, a faint coloured fringe at the rim, and glass that darkens toward its edge.
- **Health**: the colour drains from the picture as health falls from 70% to 10% and comes back as it regenerates. Death is grey. This uses the tonemap pass's saturation, so it works on every preset. The damage vignette (`shaders/damage_vignette.gdshader`) multiplies the edges of the picture toward a dark blood red, so the middle of the screen stays clear and the sky never turns pink.
- **Texture sharpness**: Medium and High read slightly sharper mipmaps.
- **World surfaces** (`shaders/world.gdshaderinc`): on High, a fine shared detail normal adds grain and roughness breakup to every textured surface within about 10 m, where the texture alone would blur. Low and Medium compile it out (see [performance.md](performance.md)). A map can be wet (`wetness` in `MAPS`): from Medium up, surfaces darken and turn glossy, and from half wet, puddles collect on the ground. Zombies is just after rain.

What each of these costs is in [performance.md](performance.md).

### Weapons

![Every weapon at the hip, the M4 firing, its smoke, and the weapon racks](media/weapon-materials.jpg)

Every gun is built from a detailed CC0 source model. From Stein Games' Free Classic Weapons Pack (rigged, about 10k to 17k triangles, 2K PBR textures): the AK-47, the M1911, the PDW-9 (an MP5), the BR-3 (an M14), and the M40 (a Remington 700). From 3dmodelscc0's Guns & Explosives Pack (parts as separate objects, 2K PBR textures): the M4A1 and the M870. The LSW-6 is an RPK pattern derived from the AK-47: the tool stretches the barrel 175 mm ahead of the gas block, and models a folded bipod and a 75-round drum. The tool also models the suppressor (a 172 mm can with wrench flats, grip grooves, a vent ring, and a stepped cap) and the vertical foregrip (a rail clamp and a tapered grip with finger grooves) in metres, in the shared optics finish, and the fired cases that weapons eject (`CASES`: 5.56, 7.62x39 in lacquered steel, 7.62x51, 9 mm, .45, and a 12-gauge shell with a brass head), in `shared/casings.glb`. Each view scene names its case (`shell_model`). The M870's source is light, so the tool rounds it with one level of subdivision, creasing edges sharper than 35 degrees so the receiver keeps its corners (`subdivide`).

`tools/build_weapons.sh` rebuilds the set in one command. `tools/fetch_weapon_sources.py` downloads the source packs (pinned by size and SHA-256, cached in `~/.cache/shooter-weapons/sources`), and `tools/build_weapons.py` runs in Blender and, for each weapon in its `WEAPONS` table:

1. Imports the source in its rest pose. Every face goes to the part its bone moves (or, for a pack that splits parts into separate objects, the object it belongs to), so the magazine, the slide, the hammer, and the rounds in the magazine come out as their own nodes. For a magazine the source models empty, the tool adds the top rounds (`CARTRIDGE_PROFILES`).
2. Turns it to face -Z, scales it to the real weapon's length and sight radius, puts the rear sight's aiming point (the top of the notch) on the origin, and pitches it so the front post's tip lies on the -Z axis. Aimed, the post sits level in the notch exactly where the camera points, which is where shots land. A source whose sights sit off its centre line (the M4A1's, by 0.8 mm) moves sideways onto the line (`sight.x`), and `sight_edits` reshapes sights too small to read at game scale (see [Weapon sights](#weapon-sights)).
3. Repacks the source's maps into the game's layout at 2048 px: sRGB colour, an OpenGL normal map (DirectX-style sources are flipped), and ORM (occlusion, roughness, metallic, from the source's packed map or its separate ones). The source's own UVs stay.
4. Exports `<key>.glb` in metres with one material for the weapon (`<key>.tres`), the shared cartridge material on the rounds, and the shared tritium glow on night sights, and `World`, the whole weapon merged into one node for other players' weapons and racks (nothing moves on them), then writes the marker positions (`Muzzle`, `Ejection`, `OpticMount`, `GripRight`, `GripLeft`) into the view scene and a summary (`<key>.json`: triangles, nodes, markers).

The repacked textures keep weapon steel at a roughness of at least 0.32 and its bare-steel colour at most 0.6 (sRGB), so worn edges read as blued or parkerised steel rather than chrome, and every texture island's colour is grown 16 texels into the empty space round it (`pad_islands`), so mipmaps never pull the gaps' colour onto the edges. Each rifle's view scene also gets a `ChargingHandle` marker where the hand works the handle (the right-hand knob on the AK-47, LSW-6, and BR-3, the left-hand lever on the PDW-9, the T-handle's rear on the M4A1), which the reload clips reach for. The M870 aims over its receiver at a brass bead the tool adds on the barrel's end.

At the hip each long gun is turned 5 degrees inward and rolled 4 degrees (`hip_rotation`, faded out as it comes up to aim), so its side, magazine, and firing hand show rather than only its top. An extended magazine stretches the magazine 35% along its length about its top, so it still seats in the well (`WeaponView::lengthen_magazine`).

Parts each rebuilt weapon exports (origins at the part's pivot, so a trigger or hammer turns about its pin):

| Weapon | Nodes |
| --- | --- |
| M4A1 | `Body`, `Magazine` (with its `Rounds`), `MagazineSpare`, `ChargingHandle`, `BoltCarrier` (seen through the port), `DustCover` (hinged at its lower edge), `Selector`, `Trigger`, `RearSight` (the flip-up aperture, which folds flat under an optic) |
| M870 | `Body`, `Pump` (with the `BoltCarrier` that rides back with it), `Trigger` |
| LSW-6 | `Body`, `Bipod`, `Magazine` (the drum, with its `Rounds`), `MagazineSpare`, `ChargingHandle`, `Selector`, `Trigger`, `MagRelease`, `OpticRail` |
| AK-47 | `Body`, `Magazine` (with its `Rounds`), `MagazineSpare` (hidden, for a reload that brings a fresh one up), `ChargingHandle` (the bolt carrier and its handle), `Selector`, `Trigger`, `MagRelease`, `OpticRail` (shown only with an optic) |
| M1911 | `Body`, `Slide` (with the front sight's `SightGlow`), `Magazine` (with its `Rounds`), `MagazineSpare`, `Hammer`, `SlideStop`, `Safety`, `Trigger`, `MagRelease` |
| PDW-9 | `Body`, `Magazine` (with its `Rounds`), `MagazineSpare`, `ChargingHandle` (the cocking lever), `BoltCarrier`, `Selector`, `Trigger`, `MagRelease`, `OpticRail` (the claw mount) |
| BR-3 | `Body`, `Magazine` (with its `Rounds`), `MagazineSpare`, `ChargingHandle` (the operating rod handle), `BoltCarrier`, `BoltStop`, `Safety`, `Selector`, `Trigger`, `MagRelease`, `OpticRail` |
| M40 | `Body`, `Scope`, `Bolt` (the bolt and its handle, for the bolt cycle), `BoltStop`, `Magazine` (the internal box with its `Rounds`), `Safety`, `Trigger` |

In first person `WeaponView` moves these parts the way a modern Call of Duty viewmodel does (`animate_action` in `rust/game/src/viewmodel.rs`): the `BoltCarrier` runs back and home within each shot's cycle (an AK-pattern `ChargingHandle` rides the carrier instead, so it reciprocates), an AR's bolt locks open on the last round and slams home on the bolt release, the `DustCover` flies open on the first shot or charge and stays open, the `ChargingHandle` is drawn back with the hand on an empty reload's charge and the inspect's chamber check, the `Trigger` swings back about its pin on each shot and stays pulled through automatic fire, the pistol's `Hammer` falls on each shot and is cocked again by the slide, its `SlideStop` rises while the slide is locked back, and the `MagRelease` is pressed as the magazine comes out. The `Selector` and `Safety` stay put: the game has no fire-mode switch to move them. The `Bipod` and the M40's `BoltStop` stay put too.

The M4A1, AK-47, LSW-6, and M1911 aim with their own modelled sights and the M870 along its receiver at its bead, so their view scenes have no `IronSights` node. The PDW-9 and BR-3 carry a reflex sight on their rails, with the sight line set at its dot. The M40 aims through a riflescope the tool models (`riflescope`: eyepiece, power ring, 30 mm tube, turrets, objective bell, rings, coated lenses) on its rail; its axis is the sight line. An optic sits on the `OpticMount` marker; when the mount is too low for it, the weapon hangs that much lower (`WeaponView::set_sight_drop`), so the optic's dot is on the sight line and the iron sights show in the bottom of its window. Weapon racks show the same third-person weapon other players carry, laid on its side (`WeaponPickup`), without a shadow (the table hides it, but every shadow cascade would still draw the gun). `World` is decimated to a fifth of the first-person triangles; every copy shares the weapon's imported material, and it drops to Godot's coarser LODs sooner than other meshes (`WORLD_LOD_BIAS`), since a gun is small beside the player holding it.

`godot/tests/weapon_models_test.gd` checks every rebuilt weapon's parts and markers, that its front sight's tip is within 0.6 mm of the sight line, and, looking from the aimed eye, that the iron sights make a clear picture (see [Weapon sights](#weapon-sights)). `godot/tests/capture_weapons.gd` captures each weapon at the hip, aimed, mid-reload, and inspected, in other players' hands, and on the racks, for reviewing, and `godot/tests/capture_turntable.gd` renders each model in a neutral studio from six angles, with wireframes, in first person and as `World`.

Each shot shows a flash with ragged petals that change every shot, a short plume along the barrel seen from the side, and every few shots a puff of grey smoke that drifts up and fades in the world (`shaders/muzzle_flash.gdshader`, `shaders/muzzle_smoke.gdshader`, built in `WeaponView`).

### Effects

![Sparks and dust on a wall, bullet holes, motion blur on a fast turn, and muzzle smoke](media/effects.jpg)

- **Impacts** (`scenes/effects/impact.tscn`): glowing spark streaks that follow their velocity (`shaders/spark.gdshader`), chips of debris in the surface's colour, a soft lit dust puff that grows and fades, and a bullet hole (`tools/build_decals.py` makes its albedo and normal map) tinted per surface. Flesh gets blood droplets and mist instead; metal sparks but does not chip.
- **Tracers** (`shaders/tracer.gdshader`): a hot core with a soft edge, bright enough to bloom.
- **Ambient dust**: a few dozen motes drift in the air around the player and catch the light.
- **Motion blur**: an option in the settings, off by default.

Depth of field while aiming was tried and left out: the weapon is drawn into a thin slice of depth (so it never clips into walls), so a near blur can only blur the whole weapon, iron sights included, which makes aiming worse.

### Palette

Every vertex of a character carries its palette role. Cloth and gear roles are *tinted*: the atlas holds only their shading detail, centred on mid grey, and `rust/game/src/character/palette.rs` gives each team a colour per role. Skin, eyes, lenses, and hardware (`metal`) keep the atlas colour. `shaders/character.gdshader` draws every role of a kit with one material per team.

| Role | Allies | Enemies |
| --- | --- | --- |
| `uniform`, `pants` | tan (0.60, 0.53, 0.40) | charcoal (0.22, 0.23, 0.24) |
| `vest`, `helmet` | coyote (0.50, 0.44, 0.32) | near black (0.13, 0.14, 0.15) |
| `webbing` | ranger green (0.40, 0.40, 0.30) | near black (0.11, 0.12, 0.12) |
| `gloves`, `boots`, `mask` | brown and tan | black |
| `skin`, `eyes`, `visor`, `metal` | the atlas colour | same |
| `accent` | blue (0.16, 0.56, 1.0) | red-orange (1.0, 0.24, 0.10) |

Teams differ in value first and in accent hue second. A faint rim in each team's accent colour (strongest for the dark enemies) keeps a silhouette apart from a dark background at night and indoors.

### Budgets

| Item | Budget | Soldier today |
| --- | --- | --- |
| Third-person triangles | 25k | 20.9k in one mesh (body, uniform, kit); 4.2k past 15 m. Zombie: 11.4k; 3.4k past 15 m |
| Bones | 60 | 53 (Godot's humanoid profile) |
| Materials | One per team and kit, shared by every character of that team | 1 (`shaders/character.gdshader`, 13 roles) |
| Draw calls per character | 2 | 1 plus the weapon |
| Textures | One atlas per kit | 2048 px albedo, normal, and ORM, VRAM-compressed (about 11 MB with mipmaps) |

Each third-person kit also carries `BodyFar`, the same character decimated to about a fifth of its triangles on the same atlas. `SoldierRig` draws it instead of `Body` past 15 m (visibility ranges with a 1 m margin). Godot also generates LODs for both meshes at import, but its simplifier does poorly on a kit of many small parts, so the far mesh is what makes distant characters cheap. Zombies use a stronger LOD bias, because a horde is two dozen bodies at mid range.

### Character detail

![The zombie horde, two zombies up close, an ally and an enemy in boots, and an enemy's bare head under the helmet](media/characters-v4.jpg)

`shaders/character.gdshader` reads the kit's atlas and:

- colours each tinted role from the team palette, with faded patches that differ per character;
- collapses to a point, shadows included, the roles a team does not wear (a zombie's helmet, eye protection, plate carrier, and belt) and the part groups a character does not wear (`hidden_parts`: a balaclava, a neck gaiter, or a bare face, and the skin each covering replaces);
- varies the skin tone per character, and for zombies adds sickly skin, grime, and blood.

The per-character seed is an instance uniform (`variant` and `hidden_parts`, set by `SoldierRig`), so every character of a team still shares one material.

## Building a character

### Sources

- The skeleton, rest pose, and animations: Quaternius's [Universal Animation Library](https://opengameart.org/content/universal-animation-library) and [Universal Animation Library 2](https://opengameart.org/content/universal-animation-library-2), both CC0. The free Standard tiers include a mannequin on a humanoid rig and about 90 animations.
- The body, skin, eyes, and clothes: [MakeHuman](https://static.makehumancommunity.org/assets/assetpacks/index.html) CC0 asset packs, built with MPFB 2 (MakeHuman for Blender, GPL-3.0, used only as a build tool). The tool pins each download by hash. The MakeHuman file server refuses a bare browser user agent, so the tool sends `curl/8.0`.
- The kit (helmet with cover and mounts, glasses, balaclava and neck gaiter, plate carrier with pouches, magazines, radio and straps, belt and holster, knee pads, knuckle guards, team bands): modelled in Blender Python by `tools/blender_gear.py`.

### Pipeline

1. Download the two animation libraries' Standard zips and extract them under `~/.cache/ual-src` (or pass `--ual1` and `--ual2`).
2. Run the asset builder: `uv run tools/build_character_assets.py`. It downloads and checks its other sources, installs MPFB 2 into a private Blender profile, and runs `tools/blender_characters.py` in Blender, which:
   - builds a MakeHuman human (`KITS` in the tool sets its proportions, skin, and clothes) and dresses it;
   - fits the body and clothes onto the soldier skeleton bone by bone, so the game's bones, rest pose, and proportions never change (`godot/tests/character_models_test.gd` checks every bone against `tests/data/soldier_skeleton.json`);
   - transfers skin weights from the mannequin, models the kit around the body (`tools/blender_gear.py`), and removes skin and cloth that the kit always covers. Hard gear follows one bone rigidly (the helmet, the belt's pouches, the boot shafts); skinned gear (plates, straps, cummerbund, knee pads, holster) has its transferred weights smoothed over its own mesh (`smooth_weights`), so both faces of a thickened shell bend together instead of tearing apart. Each boot gets a modelled rubber sole (`boot_soles`): the boot's footprint widened 5 mm, with a sprung toe, a heel block, an arch cut, and a lugged tread in the baked relief;
   - packs every part into one atlas and bakes albedo, normal (procedural fabric, webbing, and grain), roughness, metallic, occlusion, and palette-role maps.

   The tool then writes `soldier/soldier.glb` (one mesh, `Body`, and its far copy, `BodyFar`, with one material `character_soldier`, on the original skeleton) and its atlas, `soldier/soldier_{albedo,normal,orm}.png`; `arms/arms.glb` with its own atlas, the first-person arms (the `arms` kit: modelled gloves and cloth-simulated sleeves, baked from high-detail sources, and a field watch on the support wrist, trimmed to the arms so the whole atlas goes to what the player sees; see below); and `zombie/zombie.glb` with its own atlas: the same body and clothes with bare hands and no kit, so nothing under a helmet or plates is cut away. `scenes/characters/zombie_body.tscn` wears it, and `scenes/zombies/zombie.tscn` instances that. It also writes `animations/ual1.glb` and `animations/ual2.glb`: the animations the game uses, on each library's own skeleton. `--kits soldier` rebuilds one kit, and `--reuse` reassembles from the last bake without Blender.
3. Import in Godot (open the editor, or run `godot --headless --path godot --import`). The `.import` files retarget the files onto Godot's `SkeletonProfileHumanoid` with `bone_map_ual1.tres` and `bone_map_ual2.tres`. Bones get the profile's names (`Hips`, `RightHand`, ...) under a skeleton named `%GeneralSkeleton`, and bone axes are normalised. Animations from either library then play on the soldier. Clips whose names end in `_Loop` lose the suffix and loop.

Each vertex stores its palette role and its part group in UV2. To add an animation, add its name to `UAL1_ANIMATIONS` or `UAL2_ANIMATIONS` in the tool and run it again.

To review the characters, capture both teams at several distances and in several lights, mid-animation poses, turntables, wireframes, the first-person arms, and the horde: `CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 --script res://tests/capture_characters.gd` (needs a GPU and a display, or Xvfb).

### Runtime

The scenes that use the soldier:

- `scenes/characters/soldier.tscn`: `SoldierRig`, the third-person character. Range dummies (`scenes/props/target_dummy.tscn`) and other players (`scenes/remote_player.tscn`) instance it.
- `scenes/characters/fp_arms.tscn`: `FpArms`, the player's own arms. It is a child of the viewmodel in `scenes/player.tscn`.

`SoldierRig` (`rust/game/src/character/rig.rs`) takes one frame of state: velocity, floor contact, stance, sprint, view angles, aim-down-sights amount, weapon, reload progress, and whether the player is alive. Three parts turn that frame into a pose:

1. **An animation tree, built in code.** Speed-driven blend spaces for standing (idle, walk, jog, sprint) and crouching, with playback scaled to the real speed so feet don't slide. A backward walk plays the cycle in reverse. Further blends add the slide and the jump. An upper-body layer holds the arms in a firing pose, and a transition plays the death. The rig advances the tree itself, so a pose depends only on the time it is given.
2. **`SoldierPoser`, a skeleton modifier** (`poser.rs`) that runs after the animation. It turns the hips toward the movement direction and the spine back toward the aim, so strafing reuses the forward cycles. It bends the spine, neck, and head with the aim pitch, leans the torso away from hits, and places the weapon relative to the eyes (hip, aim, sprint, reload, kick, switch). Two-bone IK then puts both hands on the weapon, and the fingers close in a shape for what each hand holds (`Hold` in `poser.rs`, chosen per weapon class, never per mesh): the firing hand wraps the pistol grip with the index on the trigger, and lays it straight along the frame while sprinting or reloading; the support hand makes a loose C under a rifle's handguard, wraps a shotgun's pump, or cups the firing hand on a pistol. Each shape also sets how far each finger fans at its knuckle and how far the thumb turns in front of the palm (opposition), so the lower fingers spread down a pistol grip, the support fingers fan along a handguard with the thumb up its near side, and the fingers close up together round a pump. In first person, on a handguard or pump, the support palm rests on the weapon's surface: `grips_of` (`rig.rs`) cuts the weapon's shown parts at the support marker once per weapon, and the IK puts the palm on that cross-section instead of a fixed distance from the marker, so the hand fits whatever handguard a weapon has. The snap follows the marker along the barrel (a pump stroke) and lets go as a reload clip takes the marker away. Third-person hands stay on the weapon file's grips, the numbers the server poses the arm hit zones from (`carry.rs`). In third person the support hand sits 8 cm behind the handguard marker, so the elbow bends and drops under the rifle. While a soldier dies, the weapon stays in the right hand.
3. **The weapon.** Each gun has one scene for both views (`scenes/weapons/*_view.tscn`). The `GripRight` and `GripLeft` markers and `grip_rake` say where the hands go. A view set to `world_model` skips the viewmodel shader and the sounds, and its muzzle flash faces the camera.

![First-person arms: M4 at the hip, aimed, sprinting, and reloading, the pistol aimed, and the M870 pump](media/first-person-arms.jpg)

The first-person gloves and sleeves are built by `tools/blender_hilo.py`, because the player looks at them every match. Each glove starts from the MakeHuman hand, so the fingers keep MPFB's detailed finger weights, smoothed so no nails or creases show through. The high-detail glove is that hand subdivided four times (about 0.6 mm edges, 430,000 faces) and shaped from the skeleton's hand and finger frames:

- a moulded knuckle guard with a lobe over each knuckle, flex grooves, and vents, skinned to the hand bone alone so it does not stretch;
- a padded back panel with flex lines;
- pads over each finger's first two segments, with flex ridges over the middle joint;
- fourchette seams down every finger;
- a synthetic-leather palm and fingertip caps, with foam pads at the heel of the hand and under the knuckles;
- a neoprene gauntlet with a hook-and-loop wrist strap and a rolled hem;
- stitching along every seam.

The game glove is the same hand subdivided once with only the large forms (the guard, the pads, the strap), then decimated. Each sleeve is a tube cut wider than the arm, pinned at the shoulder and along a firm cuff band, and run through Blender's cloth solver against the arm while the fabric relaxes longer than the space it has. It buckles into compression folds above the cuff and slack folds along the forearm. The game sleeve is that cloth, decimated. The high-detail sleeve is the cloth subdivided, with a felled seam down the underside, a double-stitched hem, and a cuff tab. Albedo, normals, roughness, and occlusion are baked from each high-detail mesh onto its game mesh's atlas islands. Panel colours (leather, fabric, rubber, strap, thread) differ only in value, so the team's glove colour still tints them all. The kit is about 25,000 triangles. That is a soldier's budget, spent once, on the one model that is always on screen.

`FpArms` (`arms.rs`) uses the same poser and grip markers on its own model, `arms/arms.glb`, which holds only the sleeves and gloves. It draws them with the viewmodel shader (its own FOV, so it never clips into walls), and anchors the shoulders behind and below the camera. The viewmodel rig moves the weapon, and the hands follow it. During a reload the support hand leaves the weapon to fetch a magazine.

## Hit zones

`rust/sim/src/hitbox.rs` defines a player's hit zones: a head sphere and capsules for the neck, chest, stomach, hips, arms, and legs. Its inputs are everything a snapshot says about a body: feet position, yaw, pitch, stance (standing, crouching, sliding), activity (sprinting, in the air, mantling, reloading, aim, and the weapon in hand), and how far along its animations it is (stride phase, reload progress, mantle progress). What you see is what you hit: the zones follow the drawn soldier in every state another player can be seen in.

- **Base poses.** Each state starts from the skeleton of the animation the character plays in it, recorded in `rust/sim/src/hitbox/poses.rs`. Still poses (`Idle`, `Crouch_Idle`, `Slide`, `Jump`) are averaged over their clips. The standing blend is measured at 16 stride phases at each speed a player moves at (the walk, an aimed walk, the jog, the 4 m/s run every non-sprinting player settles at, and the sprint), the crouched walk at 16 phases, and a mantle's climb at 17 points; the zones pick the measured poses around the snapshot's phase and blend them (`rust/sim/src/hitbox/gait.rs`, shared with `SoldierRig`).
- **Animation progress.** The server keeps each player's stride phase, reload progress, and mantle progress (`hitbox::clock::BodyClock`) and sends them in every snapshot (a byte each). Every loop runs on that one phase: their timelines are stretched to a second and offset so the left foot is furthest forward at phase 0, and `SoldierRig` seeks them to it rather than letting them run free. The climb is seeked to the mantle's progress and blends in over its first 15%, the same share the zones blend it in. So every client draws a body at the moment of each animation its hit zones are posed at, and the server's rewind poses the same moment. `godot/tests/hitbox_fit_test.gd` generates `poses.rs` (`HITBOX_FIT_DUMP=$PWD/rust/sim/src/hitbox/poses.rs`, then `cargo fmt`); regenerate it when an animation changes.
- **Velocity.** Snapshots carry none, so a sample blended between two snapshots takes the velocity from one to the other (`RemoteSample::velocity`). The client's interpolation and the server's rewind blend the same two snapshots, so both get the same velocity. It picks the loop and turns the hips toward a strafe, with the spine turned back to the aim, as the poser does.
- **Aim.** The pitch bends the spine, neck, and head in the same shares the character's poser uses, so the head and chest lean as drawn.
- **Arms.** The weapon sits where the character holds it: at the hip, aimed, sprinting, or mid-reload, from the same numbers (`rust/sim/src/hitbox/carry.rs`, which the poser and `SoldierRig` read too). The hands go on that weapon's grips (each weapon file's `grips`, matching its scene's `GripRight` and `GripLeft` markers) and two-bone IK places the elbows, as on the character.
- What the zones do not follow: the rig's smoothing between states (a fraction of a second), the climb fading out after a mantle ends (0.15 s), a knife swing (half a second; snapshots say who is swinging, not how far along), and the short hit flinch.

`hitbox_fit_test.gd` poses a remote player through every state (idle, walking, running, sprinting, strafing, backpedalling, crouched and crouch-walking, sliding, aimed, reloading, mantling, in the air, each weapon, pitch from -89° to 89°, turning) and, per zone, measures how far the skeleton's bones are from the capsule axes and how far outside the zone's shapes the skinned mesh lies. In every state, moving, reloading, and climbing ones included, it fails when a zone's bones stray more than 6 cm from its axes or when one in ten of its vertices lies more than 5 cm outside its shapes; only a pump gun's reach for shells (8 cm, arm) and a mantle's legs (9 cm) get a little more. It also checks that each weapon file's `grips` match its scene's markers. `capture_hitbox_fit.gd` captures the overlay in the same states.

| Zone | Damage |
| --- | --- |
| Head | the weapon's `headshot` multiplier (1.2 to 1.5) |
| Chest, stomach | 1.0 |
| Arms | 0.9 |
| Legs | 0.85 |

With these, the M40 still kills with one shot to the head, chest, or stomach, but not to a limb.

The hitboxes are a pure function of snapshot fields, not of the animated skeleton. The dedicated server never runs animation, and Phase 3's lag compensation has to rebuild the hitboxes a player had several ticks ago. A pure function makes both cheap and identical on every machine: the client's zones and the server's rewind call the same function on the same interpolated snapshot. The debug overlay shows the zones over the model.

- `HitboxRig` (`rust/game/src/hitbox_rig.rs`) turns the shapes into physics bodies on the `hitboxes` layer. Each body carries `hit_zone` metadata, so the player's hitscan gets the zone from whatever it hits.
- **F4** toggles the overlay in debug builds: every character's hitboxes drawn over it, coloured by zone. Release builds ignore the key, and the overlay is depth tested, so it never shows players through walls.
- Range dummies block movement with a capsule on the `bodies` layer (layer 5). Bullets ignore that layer and hit the zones.

## Weapon sights

![Aimed down the M4A1, AK-47, LSW-6, M870, M1911, PDW-9, and BR-3 sights](media/iron-sights.jpg)

The M4A1, AK-47, LSW-6, and M1911 aim through the source models' own sights, and the M870 over its receiver at its bead. Modelled at real size, some of those sights are too small to read at game scale: a real post is 2 mm wide, 3 px at half a metre on a 900-line screen. `tools/build_weapons.py` reshapes them in view space (`sight_edits`, `edit_sights`), and moves the M4A1, whose sights sit 0.8 mm left of the source's centre line, onto the line (`sight.x`):

| Weapon | Rear | Front |
| --- | --- | --- |
| M4A1 | Flip-up leaf, its 2.4 mm precision peep opened to the 4.6 mm close-range aperture | Post thickened from 1.5 to 2.4 mm |
| AK-47, LSW-6 | Tangent leaf, its U-notch opened from 2.8 to 4 mm | Post thickened from 1.7 to 3.4 mm, inside its hood |
| M870 | The receiver's top, 13 mm under the eye | A 5 mm brass bead the tool adds on the barrel's end (`add_bead`) |
| M1911 | Notched blade, as modelled | Post with a green tritium dot, as modelled |

The PDW-9 and BR-3 carry a mini reflex sight (`IronSights` style `reflex`, `rust/game/src/sights.rs`): a hooded window, a coated lens, and a 0.6 mm red dot, about 6 px across aimed, so it stays crisp through the bloom. The same sight is the red dot attachment.

The ACOG (`acog` in `rust/game/src/sights.rs`) is a magnified prism sight in the manner of modern Call of Duty optics, not a scope overlay. Aimed, the whole view zooms (the weapon's `ads_fov_scale`) and the eye sits 4.5 cm behind the eyepiece (`ACOG_EYE_RELIEF`), so its rim frames a clear picture about half the screen tall with a lit chevron on a thin post and three drop marks. The housing ahead of the eyepiece is its own mesh (`SightBody`), hidden from 90% aimed, when it is behind the rim or inside the picture anyway, and any weapon or hand fragment inside the lens past the reticle is cut away (`viewmodel_behind_lens` and the `viewmodel_scope_lens` global, set each frame by `ViewmodelRig`), so the eye sees only the zoomed world and the reticle. The cut lives in copies of `viewmodel.gdshader` and `character.gdshader` built at runtime (`lens_clipped_shader` in `viewmodel.rs`), which the local player's weapon and arms switch to only while an ACOG is fitted; every other draw keeps a shader without `discard`, and with it the cheap depth prepass. It stands 1.2 cm higher than a red dot (`ACOG_RISER`), as on the real rifle, and an aimed shot shoves it only a third as far toward the eye. The M40 keeps its full scope overlay.

Rules the sights follow:

- **Sight line.** Each weapon scene's origin is on the sight line, so at full ADS the sight line is the camera's axis. An aperture is centred on it. A notch sight follows "equal height, equal light": the front post's tip is level with the top of the rear blade, and the post sits in the middle of the notch with light either side of it.
- **Readable at game scale.** Posts are 2.4 mm or more, so they show at least 4 px wide aimed; peeps and notches open enough to frame them.
- **Clear picture.** Nothing in front of the eye covers the front sight. The M870 sits 25 cm out when aimed, its firing hand low on the wrist, so neither the receiver nor the thumb over the wrist rises into the picture; held closer, the thumb came within a centimetre of the eye and the near plane cut it into slivers. Its support hand holds the back half of the pump, within reach of a bent arm.

`godot/tests/weapon_models_test.gd` traces the picture from where the eye is when aimed: the line just over each front sight must run clear through the rear sight, and a millimetre under its tip, on the line and 0.6 mm either side, the first thing the eye meets must be the front sight itself.

To review sights, capture every weapon at the hip and aimed: `CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 --script res://tests/capture_sights.gd` (needs a GPU and a display, or Xvfb). `LOOK=0.6,0` aims at a bare wall instead of the range's targets, and `WEAPONS=m4,m870` captures only some.

## Zombies and other characters

Zombies use the same skeleton and pipeline. Five zombie kits in `tools/build_character_assets.py` are five different people in their own clothes: a young man in a sweater and cargo trousers (`zombie`), a heavy middle-aged man in work overalls (`zombie_b`), an old man in a jacket and jeans (`zombie_c`), a slim young man in a T-shirt and jeans (`zombie_d`), and a woman in a blouse, a pencil skirt, and ankle boots (`zombie_e`). Each has its own MakeHuman body, skin, and atlas (2048 for `zombie`, 1024 for the others, which are seen less and never close for long), with bare hands and no kit. Each has short hair and stubble painted onto the head (`paint_hair` in `tools/blender_characters.py`; soldiers get the same, a close crop under the helmet and stubble on bare faces): a hairline mask over the scalp, sideburns, and the jaw, which the skin bake turns into strands of the kit's hair colour with a little relief. MakeHuman's own hair is alpha-tested cards, which the one opaque character material cannot draw. Zombie kits are not tinted: their clothes keep the garments' own colours (`Kit::Zombie` in `palette.rs`), and the shader adds sickly skin, grime, and blood that differ per zombie.

`scenes/characters/zombie_body.tscn` lists the other bodies in `SoldierRig.variants`, and each zombie wears one of the four, picked from its seed. `SoldierRig` plays the Universal Animation Library 2 zombie clips when `zombie` is set and adds glowing eyes. To add another character, add a kit to `KITS` (its MakeHuman body, clothes, and gear), and either a scene that instances its model under a `SoldierRig` or an entry in a rig's `variants`.


## HUD

The HUD follows the modern Call of Duty layout (Modern Warfare 2019 on and Black Ops 6) in our own calm style: small clusters in the corners, square-cornered translucent graphite plates, Chakra Petch numerals with a soft drop shadow, colour on text and thin bars only, and a white keyline for whatever is ours. The pieces draw themselves (`_draw`) with the helpers in `godot/scripts/ui/hud_paint.gd`.

| Where | What | Script |
| --- | --- | --- |
| Top left | Match score block: the mode and clock, a row per side with the score and a bar filling toward the limit, and LEADING, TRAILING, or TIED beside the limit. Outside a match, your own score, kills, deaths, and streak | `net/score_block.gd`, `score_feed.gd` |
| Top centre | Mode pieces: Hardpoint's zone strip, the flags' status in Capture the Flag, and who is alive in Search and Destroy | the mode's view |
| Top right | Frame rate, when turned on | `hud.gd` |
| Bottom left | Health (cross, number, segmented bar with a trail after each hit), the kill feed above it (killer, weapon silhouette, headshot mark, victim), and chat above that | `hud_vitals.gd`, `net/kill_feed.gd`, `net/tdm_hud.gd` |
| Bottom right | Weapon name and attachment, silhouette, big magazine count, smaller reserve, and a tick per round | `hud_weapon.gd` |
| Centre | Crosshair, hit markers, damage crescents, ammo warnings and prompts with key caps, points and medals | `crosshair.gd`, `damage_overlay.gd`, `hud_centre.gd`, `score_feed.gd` |
| Upper third | Announcement banners: the title on a dark band with thin wings in the call's colour | `net/match_banner.gd` |
| Lower third | The death card ("KILLED BY", the killer's name and weapon, the respawn countdown) and the hint under it | `hud_centre.gd`, `net/match_banner.gd` |
| Bottom edge | The controls legend, from the live bindings, for 14 seconds after the first spawn (F1 toggles it) | `hud_centre.gd` |

Deliberate differences from Call of Duty: health shows as a number and bar, because health here regenerates in steps worth reading; the match state reads LEADING or TRAILING rather than Call of Duty's wording; there is no minimap yet.

Weapon silhouettes in `assets/ui/weapons/` are rendered from the weapon models by `godot/tools/weapon_icons.gd`; run it again after a model changes.
