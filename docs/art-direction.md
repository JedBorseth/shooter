# Art direction

How the game looks, and how its worlds, weapons, effects, characters, and HUD are built. What each part costs is in [performance.md](performance.md); each map's own layout and dressing is in [maps.md](maps.md).

![Both teams in several poses, the F4 hitbox overlay, and close-ups of an enemy and an ally](media/characters.jpg)

## The look

The target is **Treyarch's Black Ops 2 and 3 built with the craft of the modern Modern Warfare games**: bold colour, a strong key light, clean readable shapes, and punchy effects, finished with photoreal materials and wear, grounded bounce light, detailed weapons, dense dressing, and effects that linger. Where the two pull apart, readability and colour win. The full target, what to leave out, and how to check a change are in the cod-design skill's [graphics reference](../.agents/skills/cod-design/references/graphics.md).

In practice that is **grounded stylized realism**: real materials (brick, concrete, painted steel, worn wood) from CC0 PBR scans on simple, readable shapes under clean light. Everything a player looks at is a modelled mesh built in Blender; Godot primitives and CSG are only for collision, effects, and debug shapes (`tools/test_models.py` fails CI when a prop, game piece, or weapon scene builds a primitive mesh). Props are real, artist-made CC0 models, never procedural or AI-made: the owner asked for free, good-quality assets.

Rules for every character, prop, and map:

- **Read the team at a glance.** Teams differ in value first (allies light tan and olive, enemies dark charcoal) and in hue second (a blue or red-orange accent on the helmet band and armbands). Value survives colour blindness, fog, and distance; the accent confirms it up close.
- **Make the head obvious.** A helmet with a coloured band, dark glasses, and an optional face covering give the head a shape and colour apart from the torso, so headshots are aimable at range.
- **Keep accents rare.** The team colour appears only on the band and armbands, with a little self-light so it reads in shade.
- **Texture detail stays quiet.** Texture adds material, not noise: no high-contrast patterns or bright graffiti where players look for enemies. Characters stay the most contrasting thing in a scene.

## World surfaces

![Slamcity (a map since cut) and the Testing Area without (left) and with (right) the PBR world textures](media/world-textures.jpg)

Level geometry draws with one shader, `godot/shaders/world.gdshader` (its body is `world.gdshaderinc`). It projects a texture set in world space from the axis each face points along most, so boxes need no UVs and textures never stretch when a box is resized. Low-frequency noise (`assets/textures/noise/macro.png`) varies brightness and roughness across large surfaces so repeats do not show, and walls darken toward the floor with grime (`grime`, `grime_height`). Untextured paint, plastic, and laminate take a shared smudge map (`assets/textures/noise/wear.png`, from ambientCG's SurfaceImperfections008). A model that moves (a sentry head) projects in its own space instead (`object_space`). Curved shapes switch projection axis where their normal turns past 45°, which shows as a seam on strongly patterned textures: use calm textures on cylinders, or give modelled props their own UVs.

`tools/build_textures.py` builds the library from the tables in `tools/world_looks.py`:

- **Texture sets** (`SETS`, written to `godot/assets/textures/<set>/`): pinned CC0 ambientCG sets at 1K, packed into an albedo, an OpenGL normal map, and an ORM map (R occlusion, G roughness, B metallic). A set marked `grey` stores brightness only, so one set serves many paint colours.
- **Looks** (`LOOKS`, written to `godot/materials/world/<look>.tres`): a set plus a tint, the texture's size in metres, roughness and metallic scales, and grime. Maps reference looks by name. The generated maps' meshes use their own table, `SLAMCITY_LOOKS` (Breakwater's table in `tools/breakwater_models.py` extends it).

To add a surface, add a set or a look, run the tool, import, and run it again with `--fix-imports` so the textures are VRAM compressed with mipmaps (its docstring has the commands). Normal maps use two-channel compression and the shader rebuilds their blue channel.

Per-preset extras, compiled in or out by `GraphicsQuality` rewriting `#define` lines (a uniform that switches code off still slows every pixel):

- **Close-up detail** (High, `WORLD_DETAIL`): a fine shared detail normal adds grain and roughness breakup within about 10 m. Rough, non-metal surfaces take it fully; each texture set scales it (`DETAIL` in `tools/world_detail.py`, written into each material as `detail`): dirt and broad concrete the most, brick and planks about half, paint, steel, and tiles a quarter or less, tread plate none.
- **Anti-tiling** (`ANTI_TILE` in `world_looks.py`): big isotropic surfaces (asphalt, concrete, plaster) blend in a second sample shifted by a fraction of a repeat, in patches. It is shifted rather than turned, so its shading falls on the same side of its bumps as the first sample's.
- **Wet surfaces** (Medium and High, `WORLD_WET`, only on a map with `wetness` in `scripts/looks.gd`): surfaces darken and turn glossy, walls less and unevenly with narrow run-off streaks, and from half wet puddles collect on the ground with a darker soaked rim. Streaks and puddles share one noise read. No map sets it today.

### Skyline windows

![The skyline of Slamcity (a map since cut) without (left) and with (right) the windows](media/skyline-windows.jpg)

Skyline buildings beyond a map's boundary are plain boxes. A look with a `windows` entry uses the `world_windows.gdshader` variant, which draws a grid of framed windows with sills on every wall above the ground floor. Behind each pane is a room that exists only in the shader (interior mapping, `shaders/rooms.gdshaderinc`): the view ray meets a box behind the glass and colours the wall, floor, or ceiling it hits, with a door or shelf on the back wall, blinds drawn part way in some windows, and a few rooms lit warmer. Every room differs by a hash of its cell, the glass reflects the sky, and edges are antialiased from screen-space derivatives so distant windows do not shimmer. It costs no geometry or textures. Windows players cannot use on a generated map's own buildings are panes with the same rooms (`shaders/window_pane.gdshader`); on the buildings players fight around, their blinds are drawn and the rooms dim, so a fake window never reads as a firing position.

## Props and decals

![Covered cars, street lamps, crates, bins, barrels, jersey barriers, and roof plant across the maps](media/map-props.jpg)

Modern Call of Duty maps read as lived-in because every wall base, doorway, and corner carries hand-made clutter; the prop library is how this project gets the same density.

- **The library** (`tools/prop_library.py`, 95 Poly Haven props): each prop's source, the parts it keeps (names or shell patterns), its triangle budget (0.3k to 6k; the textures carry the fine detail), tags (`street`, `alley`, `interior`, `workshop`, `military`, `wall`, ...), and, for a modular kit's pieces (chain-link fence, gutters, air ducts, industrial pipes, shutter windows, work lights, a street bench), the kit folder whose one texture set they share. A source authored at the wrong scale gets a `height`.
- **The build** (`blender -b --python tools/build_props.py -- <name>`): downloads the 1K glTF (cached in `~/.cache/shooter-props`), keeps the named parts, decimates to the budget, puts the origin at the bottom centre, and writes `godot/assets/props/<folder>/<name>.gltf` with its textures. Poly Haven's packages leave out alpha maps, so the builder packs the source's `alpha` or `opacity` map into the colour texture and makes wire mesh, crate holes, and grilles alpha cut-outs; glass stays blended. Godot generates LODs at import.
- **The catalog** (`godot/assets/props/catalog.json`, written by `python3 tools/prop_catalog.py --write`): each prop's scene path, size in metres, triangles, tags, source, and authors (95 entries). `python3 tools/prop_catalog.py --find street crate` searches it. CI (`tools/test_prop_catalog.py`) fails when a library prop is not built, the catalog is stale, a budget is broken, or a source is missing from `godot/assets/CREDITS.md`.
- **Seeing it** without the editor: `godot/tests/capture_props.gd` photographs every prop (or `PROPS=a,b`, or `TAGS=street,crate`) in a grey studio beside a 1.8 m post banded every 0.5 m, captioned with its size, and writes contact sheets of 24.

Adding a prop: find it on [Poly Haven](https://polyhaven.com/models), list its parts with `blender -b --python tools/build_props.py -- --inspect <asset>`, add it to the library, build it, import (`godot --headless --path godot --import`), run `uv run tools/build_textures.py --fix-imports` and import again, credit it, write the catalog, and look at its picture.

Props never change how a map plays. Each one that should block players or bullets stands on a box in a hidden collision combiner, so collision stays as simple as the blockout and impacts still report a surface.

**Decals** bring scanned road damage, dirt, drainage covers, and signs of use to street, alley, and industrial dressing, following the industrial surface wear of Modern Warfare (2019)'s maps and the readable shapes of Black Ops 2 and 3. Use subtle wear on broad surfaces and keep graffiti and road paint to small accents. The library is `tools/decal_library.py`; `uv run tools/build_decals.py <name>` takes ambientCG 1K decals or crops 2K atlases (a crop in thousandths of the source image), caps textures at 512 px on the long side, and writes RGBA albedo, OpenGL normals, and ORM with matching opacity into `godot/assets/textures/decals/` (tintable markings are stored grey). The same tool generates the bullet hole, puddle, and oil stain decals. `python3 tools/decal_catalog.py --write` writes the [catalog](../godot/assets/textures/decals/catalog.json) (texture paths, footprint in metres, tags, source, authors); `--find floor road damage` searches it, and CI checks that every entry has its textures and import files, is current, and is credited. Decals cost per pixel and turn on decal shading for the whole frame, so long road markings are painted as thin geometry instead (the generated maps' road lines are strips in their city mesh, baked into the lightmap).

**The game's own pieces** (the sentry, weapon racks, the Capture the Flag pole, scorestreak aircraft and drones, the charge and its sites) and the Testing Area's architecture are procedural models in `tools/gear_models.py` (`MODELS`), built with the shared model API in `tools/slamcity_models.py` (bevelled boxes, hulls, subdivided cages, lofts, tubes, 3D text). `blender -b --python tools/build_models.py` writes each to `godot/assets/models/<name>/<name>.gltf` and its materials, folded from the same looks, to `godot/materials/models/`; `godot/tools/model_import.gd` swaps them in at import. The sentry's eye, whose glow code drives, exports plain glTF materials (`STANDARD`). `tools/test_models.py` fails when a model is missing or older than its source (`models.hash`), or when a look is defined twice.

## Set dressing

![The Testing Area's set dressing: the armoury line, the control tower, the impact area, and the site office](media/testing-dressing.jpg)

The Testing Area (a working training range) is dressed by `tools/map_dressing.py` as `map_testing_dressing`, with the same model API as its architecture. What is placed is listed in [maps.md](maps.md); how it stays cheap:

- **Merged by area, split by role** (`gear_models.SPLIT`). A map model is written as several meshes, its parts sorted into quadrants by where they stand. Large parts bake into the lightmap. Small parts (litter, brass, weeds, strike marks, sign lettering) go into `_detail` meshes that cast no shadow, take light from the probes, hide beyond 30 m (`DETAIL_RANGE` in `godot/tools/model_import.gd`), and hide on Low. What lies flat on the floor goes into `_ground` meshes that cast no shadow. The rest also goes, in one surface, into a shadow-only `_shadow` twin, so each shadow cascade draws it once (`scripts/map_render.gd`). Repeated props (sandbags, pallets, target stands) are built in place inside these meshes, so a hundred cost a handful of draws.
- **Every slab and panel shaded on its own.** The Testing Area's floor is one quad per 4 m slab and each wall panel one quad, each with its own tint and roughness in its vertex colours, with grime banked toward walls and cover and broad damp patches. Streaks down from the copings and rust from the scuppers are thin strips laid over the panels in the same material. The floor's texture repeats every 5.3 m, not every slab, so no two slabs match, and each saw-cut joint is one connected strip whose width and shade wander. Keep surfaces one quad each: finer vertex grids cost frame time from distant slivers, and thousands of loose joint pieces break the lightmap bake. The perimeter walls are precast panels slotted between cast posts, so the sun draws a shadow down every joint; the posts collide (`WallPosts`).
- **Stains without decals.** Oil, scuffs, and tyre tracks are fans and strips in the floor's own material, darkest (and, for oil, glossiest) in the middle and fading to the slab at their rims.

Anything a player could walk into has a box in the scene's hidden collision (in the Testing Area `MapMetal`, `MapWood`, `CoverBlockers`, and `MapDirt`).

Breakwater has its own build tools, which run on the shared generator kit first written for Slamcity, a map since cut; see [maps.md](maps.md) and [map-design.md](map-design.md).

**Breakwater's weathering** uses the same idea in its own builder: a `shaded` part (`tools/build_slamcity_city.py`) carries a colour and a weight per vertex, and `tools/build_breakwater_city.py` blends the look's own tint toward that colour by the weight, so a stain sits in the surface's material, matches its texture exactly, and fades into it at the rim. The layout lays them with `stain` (oil under the tankers, the pickup, the bowser, the garage doors, the pumps and manifolds; damp under every downpipe's shoe), `streak` (rust run from the roller-door hoods, louvres, grilles, the control house's clad band, and the bollards and ladders down the quay face; dirt run from window sills), `tank_streak` (rust down the storage tanks from under the roof edge), and `band` (the weed-black tide line and a paler wet band on the quay face, weed on the moored tanker's antifouling). Weights are authored for sRGB tints and lifted by `STAIN_GAIN`, because the blend happens in linear colour. Streaks, bands, and tank runs bake into the lightmap with their surface and cost nothing per pixel. Stains on the ground are different: baked, each small fan's lightmap chart came out with dark rims and boxes, so they are drawn in a multiply-blend material instead (`shaders/stain_mul.gdshader`, in `_stain` meshes out of the bake), which darkens the lit ground under them by the vertex colour, white at the rim. It costs a few small blended fans and needs no light of its own.

**Breakwater's water** is one flat plane in a dark harbour green-grey (`shaders/sea.gdshader`: two scrolling noise normal maps, a little lighter at grazing angles, the engine's Fresnel reflection of the sky). Where it meets the quay face, the moored tanker's hull, and the rock revetment, a strip laid just over it (`shore` in the layout, `shaders/sea_shore.gdshader`) draws a broken line of foam at the wall, clumps drifting out about a quarter of the strip, and churned, silty water fading out across 1.3 to 1.6 m, so the water reads as meeting the wall. Both are cheap: no depth or screen reads.

**Vehicles** are built like Slamcity's cars, from cross-section cages smoothed by subdivision (`loft`): the road tanker (`tanker_truck` in `tools/breakwater_models.py`) has a cab-over cab with a raked windscreen in a black surround, door seams, side glass, steps, mirrors on arms, a slatted grille and bumper lamps, an oval polished-aluminium barrel (a plain metal look smudged by the shared wear map, so no texture seams wrap its curves) with compartment bands, lids, a walkway and folding rail, the bottom-loading cabinet with its couplers, hose tubes, side guards, mudguards, a rear frame with lamp clusters and a ladder, ADR plates, and lathed truck wheels with tread blocks, dished rims, and wheel nuts (`truck_wheel`; dual on the rear axles). The site pickup is a double cab on the same wheels with an open bed and a light bar.

**Breakwater's buildings.** Every open window (one a player can see and shoot through) gets a steel window frame in its reveal, with a mullion, a transom, and jagged shards of glass left in some corners of each pane (`Wall._glazing` in `tools/slamcity_layout.py`, switched on by Breakwater's `shell` wrapper), so it reads as a shot-out window rather than a hole. The control room's consoles are operator desks with monitors on stands (thin bezels and a dim process display: a header bar, pipe runs, tank levels, and now and then a red alarm), keyboards with key rows, mice, a phone, a binder and papers; chairs, shelving and a second desk fill the room. The storage tanks use the painted-metal scan at a 6 m repeat with its relief turned down (`normal` 0.45), so it reads as painted plate rather than concrete grain. The moored tanker's bow is lofted through seven sections to a raked stem, fine at the waterline and flared under the deck, its deck edge kept crisp by a doubled cage point; the hull paint is matt. The tanks' spiral stairs hang on a helical stringer with brackets back to the shell. Roof plant (vents, a duct run with its fan) sits on the roofs players cannot reach. Beyond the boundary the ground is concrete apron, not flat sand, and the grey hill wedges are gone (the horizon and sky are stream B's: `tools/build_horizon.py`).

## Light

Each map is lit by one directional light for its sun or moon and a sky that also lights the shade, so shadows take the sky's colour instead of a flat grey. Daytime skies are CC0 HDRIs from Poly Haven, downloaded by `tools/build_skies.py` into `godot/assets/skies/`; the tool prints where the sun sits in each image, and the map rotates its sky so the sun lines up with its light at the same elevation.

| Map | Sky | Mood |
| --- | --- | --- |
| Testing Area | `range_day.hdr` (`kloofendal_48d_partly_cloudy_puresky`) and a rendered horizon of open country | Clear midday, high sun, light haze |
| Breakwater | `breakwater_day.hdr` (`sunflowers_puresky`: fair-weather cumulus, its sun at the same 43° as the old clear sky, so the lightmap and shadows still match) | Late-morning harbour, hard sun, broken cloud |

**Horizons.** `tools/build_horizon.py` renders the country beyond a map's walls in Blender from fixed seeds: the Testing Area's valley, fields, hedgerows, woods, and farms (every tree a real mesh lit by a sun at the map's elevation), and with `--map breakwater` Breakwater's harbour (`tools/horizon_harbour.py`): the mole running on east and west with tank farms, sheds, a container terminal whose ship-to-shore cranes reach over the water, and refinery blocks with columns, banded chimneys and a flare stack; a rubble breakwater with a red-and-white lighthouse across the north channel; tankers, container ships and tugs at anchor; and the far shores 2.5 to 4 km off, a town of white and ochre blocks under terracotta roofs along the water and scrub and pine hills rising to a ridge of wind turbines. The harbour is lit by the map's own sky image, turned so its sun stands where the game puts it. Each writes a 360° strip storing the land premultiplied by its coverage, and a second image storing each pixel's distance, so the sky shader (`day_sky.gdshader`) fades the land into the map's haze as far as it really stands. It costs two texture reads on sky pixels near the horizon. The Testing Area sets the shader in its scene; Breakwater's scene keeps a plain panorama (its tool belongs to the world builder) and `looks.gd` swaps the shader in at load (`sky`), with clearer air for the strip than the map's own short-range haze. Nothing stands near enough for the missing parallax to show. The sky's reflections and the lightmap see the plain sky. Rerun the tool after changing a map's sun elevation.

![Without (left) and with (right) the baked lightmaps: Slamcity's (a map since cut) street, a corner store, the alley, and the Testing Area](media/lightmaps.jpg)

**Lightmaps.** Every map has a baked LightmapGI: bounced sun and sky light, so rooms, walkways, and the shade under awnings get soft bounced light and darken where the sky cannot reach, and its probes light characters and the first-person weapon. No map uses live GI below High (see [performance.md](performance.md)). `tools/bake_lightmaps.sh [scene ...]` bakes in two steps:

1. `godot/tools/bake_map_meshes.gd` turns any visible CSG left in a map into meshes with lightmap UVs and hides the CSG, which keeps its collision. Every map is modelled, so it usually finds nothing; models and props get lightmap UVs at import.
2. The editor plugin in `godot/addons/lightmap_baker/` opens each scene in the editor, presses its own Bake Lightmaps button, and saves. It needs a GPU (Xvfb works with one) and does nothing unless `BAKE_LIGHTMAPS` is set.

The baker can report success and still write a black lightmap when it loses the GPU partway, or one black only on the map's main surfaces, which shows only in shade. So the script checks every lightmap it baked (`godot/tools/check_lightmap.gd`, using `lightmap_check.gd`): it fails on a black atlas, when more than 30% of the map's surface gets no light, or when more than 40% of any mesh holding 5% of the map's texels does. CI checks the committed lightmaps the same way (`godot/tests/lightmaps_test.gd`), including that a copy of the Testing Area's lightmap with its walls and floor blacked out fails. Bake with the GPU otherwise idle, and rebake after changing a map's geometry or light. Lights that exist only to be baked sit under a node with `scripts/baked_lights.gd`, which removes them in the game.

## The graded picture

The final picture of a map is set in `godot/scripts/looks.gd`, not its scene, so a map that a tool regenerates keeps it. `GraphicsQuality.adopt` applies it as each map loads; a map not listed keeps the look authored in its scene (both maps are listed). For each listed map, `MAPS` holds:

- **`env`**: overrides of the tone curve, exposure, haze, ambient occlusion, and bloom. Both maps use the ACES curve for hard daylight (AgX, which a map's scene may author, holds the hue of neon and lamps better but flattens sun against shade). Both use a tight contact occlusion (radius 0.5 m, a strong detail pass) that draws a crisp line where walls and boots meet the ground instead of a wide halo.
- **`sun`**: the Testing Area's sun is 2.9 against its scene's 2.0 and Breakwater's 4.0 against 2.1, each with less exposure, so sunlit ground nears white while shade goes dark (the baked bounce keeps its level, so the key-to-fill ratio rises); a little more sky light keeps soldiers and the first-person weapon readable in shade.
- **`exposure`**: how far eye adaptation may move the exposure, below.
- **`grade`**: an S-curve on brightness around a pivot, a `lift` (a gamma below 1 under the pivot) that opens up shade without greying black, a toe that deepens the darkest shadows without clipping, a cool shadow tint and a warm highlight tint that both fade out at black and white, and a saturation. A colour that would leave the range is pulled toward its own grey, so it keeps its hue. `Looks.build_lut` turns the grade into a 32³ look-up table when the map loads, and the renderer samples it inside the tonemap pass, so a grade costs nothing per frame. The renderer reads the table at texel centres with no half-texel correction, so texel *i* holds the grade of input (*i* + 0.5) / 32 (`Looks.texel_input`). Tune a grade offline against a screenshot first: the maths is a few lines.
- **`flare`** and **`wetness`**: the sun flare's strength and how wet the map is.

| Map | Grade |
| --- | --- |
| Testing Area | Hard midday: a filmic curve, a strong sun, deep shade, cool shadows, warm highlights, and a sun flare |
| Breakwater | Late-morning harbour: a strong warm sun against a cool sea-air sky (sun 4.0, exposure 0.56), blue-grey sea haze that steps the far tanks back, cool shade opened up by `lift` so it stays readable, warm highlights, bloom only above 1.35 so sunlit white tanks stay crisp, lens dirt, and a sun flare |

On top of that:

- **Bloom** (`Looks.BLOOM`, every listed map): only what is brighter than white blooms (`glow_bloom` 0), added like light scattered in a lens rather than screened over the picture, with a faint wide level that gives the sun, lamps, and muzzle flashes a soft halo.
- **Lens dirt** (`scripts/lens_dirt.gd`): smudges and specks built in code from a fixed seed. The glow pass multiplies its bloom by them (`Environment.glow_map`), so the dirt shows only where something bright blooms through it.
- **Eye adaptation** (`scripts/eye_adaptation.gd`, every preset): the picture opens up in covered shade (an alley, a doorway, the culvert) by up to `brighter` stops and closes down by up to `darker` when the player looks into the sun. It is estimated from the world, not the picture: eight probes a second cast rays from the camera into the sky (how much is open) and one toward the sun (sunlit, and whether the sun in view is hidden), so it costs no GPU time and muzzle flashes and explosions never pump it. The light found is compared with the level typical of the map's views (`reference`) and the eye makes up 60% of the difference, easing slowly and closing down twice as fast as it opens. A ray that starts inside a collider does not hit it, so where the camera can sit inside geometry (the Testing Area's culvert roof) a thin collider (`SkyCovers`) catches the rays.
- **The lens** (`scripts/lens.gd`, a full-screen pass under the HUD): a vignette and a little film grain on Medium and High (the "Lens effects" setting turns them off), and on High a contrast-adaptive sharpen. The vignette deepens while aiming, and on High the periphery blurs a little (never the middle, where the sights are). Rounds passing close (`suppress()`) close it in and drain some colour for a moment, and on High also blur the edges. After death the picture fades down. Low draws the lens only for suppression and death.
- **The sun in the lens** (`shaders/lens_flare.gdshaderinc`, Medium and High, maps with a `flare`): a glare round the sun, a few faint coloured ghosts on the line through the screen's middle, and the lens dirt lit up near it. Five rays test whether the sun is hidden, so the flare fades as the sun slips behind an edge. The lens swaps to its `_sun` shaders only while it has a flare to draw.
- **The first-person key light** (`scripts/viewmodel_light.gd`): a shadowless key on the first-person weapon and arms only, the way a shooter lights its viewmodel. It takes the sun's colour and height, swings round over the player's shoulder, and grows as the player turns into the sun, so a backlit weapon keeps its shape. In shade (one ray toward the sun, six times a second) the key becomes the sky: a light from above and ahead in the sky's colour, so a black rifle and its gloves keep their shape and the sheen along the rail. The sky's part wraps further round the weapon than the sun's (a dome, not a point), takes only part of the baked crevice occlusion, and adds a Fresnel sheen of the sky on faces whose reflection looks up, the way a Call of Duty viewmodel reads in shade by the sky on its top faces and edges rather than by its near-black paint; under a roof (a ray straight up) it drops to a quarter and the baked probes do the rest. The viewmodel shader adds it from two global shader parameters (`shaders/viewmodel_key.gdshaderinc`), and the arms use a first-person copy of the character shader with the same key, so the world pays nothing for it.
- **Contact shadows** (`scripts/grounding.gd`): a soft dark patch on the ground under every soldier, so they stand on the ground in shade and where the sun's shadow is soft or far. It follows the stance (a small oval standing, longer and darker crouched, laid out behind the feet in a slide, turned with the body), and in direct sun it fades to a faint core. Each is a flat quad (`shaders/contact_shadow.gdshader`) laid on the ground, tilted to a slope, by a short ray down a few times a second; it fades as the feet leave the ground and between 60 and 80 m. It reads no depth and is no Decal, so it costs only the pixels it covers.
- **The scope** (`shaders/scope.gdshader`, drawn by `scripts/scope_overlay.gd`): one pass that multiplies the picture: black outside the tube, a soft eye-relief shadow that slides to one side as sway moves the scope off the eye, a faint coloured fringe at the rim, glass that darkens toward its edge, and the slight cast and light loss of coated glass.
- **Health**: colour drains from the picture as health falls from 70% to 10% and comes back as it regenerates, through the tonemap pass's saturation, so it works on every preset; death is grey. The damage vignette (`shaders/damage_vignette.gdshader`, driven by `scripts/damage_overlay.gd`) multiplies the edges toward a dark blood red, broken into smears reaching in from the rim, so the middle stays clear and the sky never turns pink; each hit darkens it and pushes it in for a moment, and below 35% health it beats with the heart, faster as health falls. At full health it is hidden.
- **Texture sharpness**: Medium and High read slightly sharper mipmaps (a negative mipmap bias).

What each preset draws is in [performance.md](performance.md).

## Weapons

![Every weapon at the hip, the M4 firing, its smoke, and the weapon racks](media/weapon-materials.jpg)

### Models

Every gun is a real model in metres, with its moving parts as separate nodes. Most start from CC0 source packs; the rest are modelled from real dimensions by our own Blender scripts.

| Weapon | Built from |
| --- | --- |
| M4A1 | 3dmodelscc0's Guns & Explosives Pack (`M4A1`), finish painted from its geometry |
| AK-47 | Stein Games' Free Classic Weapons Pack (`AK47`) |
| LSW-6 | The AK-47, made an RPK: the barrel stretched 175 mm ahead of the gas block, a folded bipod, a 75-round drum |
| KR-12 | The AK-47, its walnut refinished as black polymer |
| M1911 | Classic Weapons Pack (`1911`) |
| PDW-9 | Classic Weapons Pack (`MP5`) |
| CQ-10 | Classic Weapons Pack (`Mac10`: a MAC-11 in a rail cage), scaled to 9 mm rounds |
| BP-9 | Classic Weapons Pack (`Tec9`); the pack ships only a colour map, so its other maps are derived from it |
| BR-3 | Classic Weapons Pack (`M14`) |
| DM-7 | The M14 with a modelled riflescope, its walnut refinished as olive glass fibre |
| M40 | Classic Weapons Pack (`M700`), made military (`militarize`): a heavy barrel (24 mm at the muzzle, not 18), a beavertail forend, a raised cheek piece on two posts, an olive glass-fibre stock, and a modelled riflescope with tall target turrets |
| M870 | Modelled by `tools/weapon_m870.py`: a service-pattern pump with an 18.5 inch barrel, a cut ejection port whose bolt face rides back with the pump, a brass bead, and "M870 12 GA" engraved on the receiver |
| SG-12 | Modelled by `tools/weapon_sg12.py` on the M870's receiver, trigger group, and stock: a gas-operated 12 gauge fed from an eight-round box, after Black Ops 2's S12 |
| RL-4 | Modelled by `tools/weapon_rl4.py`: a 40 mm reusable tube with a venturi, glass-fibre heat guards, a pistol grip, and an 85 mm warhead standing out of the muzzle, after Black Ops 2's RPG |

The build also models the attachments and shared parts: the suppressor (a 172 mm can) and the vertical foregrip (`ATTACHMENTS`), the red dot and ACOG housings (`tools/weapon_optics.py`), and the fired cases (`CASES`: 5.56, 7.62x39 in lacquered steel, 7.62x51, 9 mm, .45, and a 12-gauge shell with a brass head, in `shared/casings.glb`). Each view scene names its case (`shell_model`). The combat knife (`tools/build_knife.py`: a blackened clip-point blade with a fuller and a 2 mm honed edge, a grip of stacked leather washers between fibre spacers, about 12k triangles) and the frag, flashbang, and smoke grenades (`tools/build_grenade.py`: M67, M84, and M18 patterns sharing one fuse, spoon, and ring) are modelled with no source files, and painted like the modelled guns on 1024 px maps by `tools/weapon_equipment.py`: the knife's coat wears to steel on the spine, its leather darkens in the washers' grooves, and its ricasso carries an etched mark; the grenades take chipped olive or grey enamel, a dull zinc fuse, and their name and lot number stencilled round the body (`stencils` in `weapon_finish`, text unrolled round the can). The M870 (`tools/weapon_m870.py`) is the classic police pump: a receiver whose rear sweeps into the wrist, a rounded trigger guard, a knurled magazine cap, and oiled walnut furniture (the `walnut` finish bakes wave-texture grain along the gun, with open pores in the normal map); the SG-12 shares its receiver and takes the stock in polymer.

`tools/build_weapons.sh [key ...]` rebuilds the set: `tools/fetch_weapon_sources.py` downloads the packs (pinned by size and SHA-256, cached in `~/.cache/shooter-weapons/sources`), `tools/build_weapons.py` runs in Blender, and Godot imports the result. For each weapon in `WEAPONS` the build:

1. Imports the source in its rest pose and gives every face to the part its bone moves (the magazine, the slide, the hammer, the rounds). A magazine modelled empty gets its top rounds (`CARTRIDGE_PROFILES`).
2. Faces it down -Z, scales it to the real weapon from its cartridge length, puts the rear sight's aiming point on the origin, and pitches it so the front sight's tip lies on the -Z axis. Aimed, the sights line up exactly where shots land.
3. Repacks the source's maps at 2048 px into sRGB colour, an OpenGL normal map, and ORM (occlusion, roughness, metallic), keeping the source's UVs. Steel keeps a roughness of at least 0.32 and bare steel at most 0.6 sRGB, so worn edges read as blued or parkerised rather than chrome, and every texture island's colour is grown 16 texels outward (`pad_islands`) so mipmaps never bleed the gaps onto edges.
4. Exports `<key>.glb` with one material (`<key>.tres`), writes the markers (`Muzzle`, `Ejection`, `OpticMount`, `GripRight`, `GripLeft`, and `ChargingHandle` where the hand works it) into `scenes/weapons/<key>_view.tscn`, and writes a summary (`<key>.json`: triangles, nodes, markers).

First-person models are 6k to 19k triangles. For other players and racks the build also exports `World`, the whole gun merged and decimated to a fifth, and for guns with a magazine `WorldBody` and `WorldMagazine`. During another player's reload those two replace `World` and the magazine follows the support hand; on an empty reload (and every pistol reload) the old magazine drops instead, a loose copy that bounces once and lies for 5 s, at most six at once, never drawn past 40 m (`rust/game/src/character/magazines.rs`). With an extended magazine the third-person gun always draws `WorldBody` and a stretched `WorldMagazine` (two draw calls); otherwise a gun is one. Third-person guns switch to coarser LODs sooner than other meshes (`WORLD_LOD_BIAS`). Racks show the same third-person gun laid on its side, casting no shadow (`rust/game/src/pickup.rs`).

### Finishes

Where a source's maps are flat or missing, `tools/weapon_finish.py` paints the finish from the geometry: the M4A1, the M40, the modelled guns, and the optics. Each face gets a zone from rules on its part, position, and distance from the bore (hard-anodised aluminium, phosphated steel, stippled polymer, the magazine, an oily bolt, matte sights, rubber, glass fibre, brass, and the rocket's hull and warhead). Cycles then bakes occlusion, a normal map with machined edges rounded by a bevel shader, an edge mask, and seamless noise on fresh UVs, and the composition wears coatings to bare metal on exposed edges, puts carbon in crevices and dust on top faces, and lets scratches show only in reflections. `seam_fix` stops UV seams reading as wear: the bevel shader reads an island's border texels off their face and finds an edge at every seam, which wore bare-metal chips along a smooth sphere's seams. With it, the edge mask drops the outer two texels of every island and grows the interior back over them. Rounded models (the knife, grenades, M870, optics, riflescope, suppressor, and foregrip: `seam_fix` True) do the same to the normal map and grow each island's finished colour out over the gaps (`uv_margin`: 6 to 8 texels), so mipmaps never take a neighbour's paint; hard-edged guns (the M4A1, M40, SG-12, and RL-4: `"edges"`) keep their normal maps, because their seams are mostly real machined edges whose bevels sit in those border texels. Markings are engraved in Barlow Condensed: the M4A1's SAFE, SEMI, AUTO and rollmark (5 mm letters, larger than a real rollmark's 3 mm so they stay legible at the texture's density) and the ACOG's "2.5X". `WEAPON_FINISH_DEBUG=<dir>` keeps the baked masks. Pack guns with walnut furniture can refinish it (`wood_to`), and parts a source paints in the wrong finish are re-projected onto the right patch of the atlas (`refinish`: the AK pattern's near-white charging handle and selector, whose lever is also thinned from 7.5 to 3 mm).

### Moving parts

| Weapon | Nodes besides `Body` |
| --- | --- |
| M4A1 | `Magazine`, `MagazineSpare`, `ChargingHandle`, `BoltCarrier`, `DustCover`, `Selector`, `Trigger`, `RearSight` (folds flat under an optic) |
| AK-47, KR-12 | `Magazine`, `MagazineSpare`, `ChargingHandle` (the carrier and its handle), `Selector`, `Trigger`, `MagRelease`, `OpticRail` |
| LSW-6 | As the AK-47, with the drum as `Magazine`, and `Bipod` |
| M1911 | `Slide` (with the front sight's `SightGlow`), `Magazine`, `MagazineSpare`, `Hammer`, `SlideStop`, `Safety`, `Trigger`, `MagRelease` |
| PDW-9 | `Magazine`, `MagazineSpare`, `ChargingHandle` (the cocking lever), `BoltCarrier`, `Selector`, `Trigger`, `MagRelease`, `OpticRail` |
| CQ-10 | `Magazine`, `MagazineSpare`, `ChargingHandle`, `Selector`, `MagRelease`, `OpticRail` |
| BP-9 | `Magazine`, `MagazineSpare`, `ChargingHandle`, `Trigger`, `MagRelease` |
| BR-3 | `Magazine`, `MagazineSpare`, `ChargingHandle` (the operating rod handle), `BoltCarrier`, `BoltStop`, `Safety`, `Selector`, `Trigger`, `MagRelease`, `OpticRail` |
| DM-7 | As the BR-3, with `Scope` in place of `OpticRail` |
| M40 | `Scope`, `Bolt`, `BoltStop`, `Magazine` (the internal box), `Safety`, `Trigger` |
| M870 | `Pump` (with the `BoltCarrier` that rides with it), `Trigger` |
| SG-12 | `Magazine`, `MagazineSpare`, `BoltCarrier`, `ChargingHandle`, `Trigger` |
| RL-4 | `Magazine` (the loaded rocket, which also flies as the rocket), `MagazineSpare`, `Trigger` |

Every magazine carries its `Rounds`. Each part's origin is its pivot, so a trigger or hammer turns about its pin.

In first person `WeaponView` moves them as a modern Call of Duty viewmodel does (`animate_action` in `rust/game/src/viewmodel.rs`): a self-loader's carrier runs back and home within each shot (an AK's handle rides it), an AR's or M14's bolt locks open on the last round, the dust cover flies open on the first shot and stays open, the charging handle is drawn on an empty reload's charge and the inspect, the trigger swings back on each shot and stays pulled through automatic fire, a pistol's hammer falls and is cocked again by the slide while its slide stop rises on empty, and the magazine release is pressed as the magazine comes out. Selectors, safeties, and the bipod stay put: the game has no fire-mode switch. Each gun's reload follows its real mechanism; see [weapons.md](weapons.md).

At the hip each long gun is turned inward and rolled a few degrees (`hip_rotation` on its view scene, faded out as it comes up to aim), so its side, magazine, and firing hand show rather than only its top. An extended magazine stretches the magazine 1.35 times along its length about its top, so it still seats in the well (`WeaponView::lengthen_magazine`).

`godot/tests/weapon_models_test.gd` checks every weapon's parts and markers, that the front sight's tip is within 0.6 mm of the sight line, that the maps carry real detail, and that the optics and racks draw. `godot/tests/capture_weapons.gd` captures each weapon at the hip, aimed, mid-reload, inspected, in other players' hands, and on the racks (`WEAPONS`, `PARTS`, `MAP`, and `CAMO` narrow it); `godot/tests/capture_turntable.gd` renders each model in a neutral studio from five angles, with a receiver close-up, wireframes, and `World`. All captures need a GPU and a display (or Xvfb) and write to `CAPTURE_DIR`.

### Sights and optics

![Aimed down the M4A1, AK-47, LSW-6, M870, M1911, PDW-9, and BR-3 sights](media/iron-sights.jpg)

| Weapon | Aims through |
| --- | --- |
| M4A1, AK-47, KR-12, LSW-6, M1911 | The model's own iron sights, reshaped where needed (below) |
| M870 | Its receiver's top, 13 mm under the eye, at a 5 mm brass bead |
| SG-12 | A ghost ring and fibre-optic blade built in code (`IronSights` style `shotgun`) |
| BP-9 | A notched rear blade and post built in code (style `pistol`), since the pack's sights sit flush and hide the post |
| PDW-9, BR-3, CQ-10 | A compact red dot (style `reflex`) |
| DM-7 (2.5x), M40 (4x) | A modelled riflescope and the scope overlay |
| RL-4 | Its modelled aperture and post, 70 mm over the bore |

A real sight is too small to read at game scale (a 2 mm post is 3 px at half a metre on a 900-line screen), so the build reshapes some in view space (`sight_edits`):

| Weapon | Rear | Front |
| --- | --- | --- |
| M4A1 | The 2.4 mm precision peep opened to the 4.6 mm close-range aperture | Post thickened from 1.5 to 2.4 mm |
| AK-47, KR-12, LSW-6 | U-notch opened from 2.8 to 4 mm | Post thickened from 1.7 to 3.4 mm, inside its hood |

The M4A1's sights sit 0.8 mm left of the source's centre line, so the build moves it onto the line (`sight.x`). On the AK pattern's irons the eye sits 16 cm further back than with an optic (`iron_eye_back`), with the cheek on the stock, so the dust cover's ribbed rear shows low in the frame instead of being sliced by the near plane; the arms come back with it (`FpArms::set_body_shift`).

Rules the sights follow:

- **Sight line.** Each view scene's origin is on the sight line, so at full ADS it is the camera's axis. A notch sight follows "equal height, equal light": the post's tip level with the top of the rear blade, centred in the notch.
- **Readable.** Posts are 2.4 mm or more, so they show at least 4 px wide aimed.
- **Clear picture.** Nothing in front of the eye covers the front sight, and nothing of the weapon comes within 4 cm of the eye on irons. The M870 sits 25 cm out when aimed, so neither the receiver nor the firing thumb rises into the picture.

`weapon_models_test.gd` traces the picture from the aimed eye: the line just over each front sight must run clear through the rear sight, and a millimetre under its tip, on the line and 0.6 mm either side, the first thing the eye meets must be the front sight. `godot/tests/capture_sights.gd` captures every weapon at the hip and aimed (`WEAPONS=m4,m870` for some, `LOOK=yaw,pitch` to aim at a bare wall).

**Red dot.** A hooded window, a coated lens, and a red dot built in code (`rust/game/src/sights.rs`), inside the modelled housing. The dot is about 10 px across aimed at 1080p, a 2 MOA dot as Call of Duty draws it, deep red and lit 3.6 times past its colour (`Style::glow_energy`; tritium vials stay at 6; the source colour is deep red so it keeps its red through the tonemap) so it blooms into a halo instead of burning white. The glass passes nearly everything in the middle and tints blue-green only near the hood (`sight_glass.gdshader`). The red dot attachment is the same sight. Aimed, every red dot window sits 10 cm from the eye, about an eighth of the screen's width: the attachment slides along the rail from the `OpticMount` to sit there (`reflex_seat`), and stands on a 12 mm riser, a lower-third co-witness. When a mount is too low for an optic, the weapon hangs that much lower (`WeaponView::set_sight_drop`), so the dot stays on the sight line.

**ACOG.** A 2.5x prism sight in the manner of modern Call of Duty optics, not a scope overlay. Aimed, the view zooms (the weapon's `ads_fov_scale`) and the eye sits 4.5 cm behind the eyepiece (`ACOG_EYE_RELIEF`), so its rim frames a clear picture about half the screen tall with a lit chevron on a thin post and three drop marks. It stands 1.2 cm higher than a red dot (`ACOG_RISER`) and an aimed shot shoves it only 35% as far toward the eye. The housing ahead of the eyepiece (`SightBody`) hides from 90% aimed, and any weapon or hand fragment inside the lens is cut away by a copy of the viewmodel and character shaders with a `discard` (`lens_clipped_shader`), which the local weapon and arms use only while an ACOG is fitted; every other draw keeps the cheap depth prepass.

The housings (`tools/weapon_optics.py`, `tools/build_weapons.sh acog reddot`, into `assets/weapons/optics/`) are finished like the M4A1 on a 1024 px texture, with edge wear in a 2 mm band (a 3.5 mm one wore the round housings into speckle): the red dot about 3.1k triangles, the ACOG about 9.1k. The suppressor (ceramic-coated, browned by gas toward the muzzle, "SD-556" on its left) and the foregrip (stippled polymer) are painted the same way on their own 1024 px maps. The DM-7's and M40's riflescope (`riflescope` in `build_weapons.py`) has its own painted 1024 px maps on a `Scope` material (`<key>_scope_*`): hard-anodised tube and turrets, a rubber eye guard and power ring, steel rings, and its power engraved on the left. Each exports `World`, decimated to 30%, for other players.

**Riflescope.** The DM-7 and M40 keep the full scope overlay (`scripts/scope_overlay.gd`), but raising the scope reads as bringing it to the eye: from half aimed the zoomed world shows in the eyepiece's glass (`scope_eyepiece` on the view scene); from 60% the cut widens to the overlay's lens while the overlay's surround and reticle fade in; at 85% the weapon hides and the overlay is complete.

### Muzzle flash and smoke

Each shot shows a flash seen down the barrel and a plume along it seen from the side (two quads crossed along the barrel), each a random frame of a 4 x 4 atlas rendered in Blender from glowing gas (`tools/build_muzzle_flash.py`, `assets/textures/effects/muzzle_flash_4x4.png`, drawn by `shaders/muzzle_flash.gdshader`). Aimed, the flash uses the atlas's smaller, tighter rows (a shotgun keeps its full blast), so it does not wash out the sight picture. The flash is as big as the view scene's `Quad_flash`, and the plume 1.6 times as long (2.5 times on third-person guns). The flash shows for 0.05 s, so automatic fire flickers. Other players' muzzle light shows only when the camera can see the muzzle, so a shooter behind a wall does not light the near side of it.

A puff of gun smoke leaves the muzzle at most every 0.14 s (`shaders/muzzle_smoke.gdshader`, a frame of the smoke flipbook lit by the scene). In first person each puff is placed in the world where the muzzle shows on screen, so it stays where the shot was fired and walls hide it. After six or more rounds in quick succession a thin wisp rises off the barrel once firing stops. On High each puff also fades out over its last 35 cm in front of whatever is behind it (`SOFT_SMOKE`, compiled out on Low and Medium).

Fired cases fly out of the ejection port and, after 0.09 s, are handed to the world where they appeared on screen (`effects::viewmodel_to_world`); see Effects.

## Effects

![Sparks and dust on a wall, bullet holes, motion blur on a fast turn, and muzzle smoke](media/effects.jpg)

Every combat effect comes from a fixed pool in `CombatEffects` (`rust/game/src/effects.rs`), never one node per shot, sized by the graphics preset. Only clients draw them; the dedicated server pays nothing. Characters and first-person models draw on their own visual layer that decals skip, so a hole never lands on someone walking past.

| Pool (`BUDGETS`) | Low | Medium | High |
| --- | ---: | ---: | ---: |
| Impacts playing | 12 | 20 | 32 |
| Bullet holes kept | 16 | 48 | 96 |
| Blood spatters kept | 4 | 12 | 24 |
| Tracers in flight | 8 | 16 | 24 |
| Scorch marks kept | 4 | 8 | 12 |
| Fires | 2 | 4 | 6 |
| Smoke clouds | 2 | 3 | 4 |
| Fired cases | 12 | 24 | 40 |

- **Impacts** (`scenes/effects/impact.tscn`, `scripts/impact.gd`): chips of the surface (the modelled chip, `assets/effects/chip.glb`, tinted from the surface's colour, so pale concrete sheds pale flecks) that tumble, land on the floor below, and shrink away (`shaders/debris.gdshader`), and a dust puff in the surface's colour from the smoke flipbook. Only metal sparks (`shaders/spark.gdshader`); concrete, wood, dirt, and glass give chips and dust. A round leaving the far side of cover (`exit_impact`) throws a faster, tighter spray along its travel and leaves a hole 1.5 times as wide. Other players' rounds draw the same impacts, holes, and blood where the client traces them (`ShotTracer.shot_traced`).
- **Bullet holes**: 16 cm decals, four variants per surface class (concrete, metal, wood, glass, dirt), each with its own albedo, normal, and roughness and a soot ring (`tools/build_combat_fx.py`). Wood holes run along the grain; the rest turn at random.
- **Marks that stay**: holes and blood stay on the walls until the preset's budget is full, then the oldest is reused, as in Call of Duty. Decals fade out between 40 and 50 m.
- **Blood** is dark and wet, never glowing: lit droplets that stretch along their flight (`shaders/blood.gdshader`), most thrown out of the far side and some back toward the shooter, and a darker mist that hangs for a moment. A hit with a wall within 2 m behind spatters it (four generated spatters with a glossy wet core, sized by how far behind the wall is).
- **Tracers** (`shaders/tracer.gdshader`): a 7 m ribbon at 320 m/s, slow enough to follow as in Call of Duty, with a hot core, an orange glow, and a head brighter than its tail. It never draws thinner than 2.5 pixels; widened past its true 1.8 cm it fills solid and dims to match, so a far tracer is a steady line. Every `tracer_every`-th round of each weapon is a tracer, for other players as for you; a suppressor removes them.
- **Fired cases**: the modelled cases tumble, bounce off what they meet (one ray a frame while moving), clink on hard bounces, lie where they fall for 10 s, and shrink away. Other players' weapons throw theirs within 30 m.
- **Explosions** (`scenes/effects/explosion.tscn`, `scripts/explosion.gd`, `CombatEffects.explosion`): a frag going off on dirt or concrete, not a fuel fireball. A short orange core gone in about half a second (Unity Labs' CC0 Explosion02HD flipbook from its burning frames on, `shaders/explosion.gdshader`); a fan of dark dirt jets thrown up out of the ground that tear into clumps as they thin (a 16-frame flipbook rendered in Blender from lit scattering volumes, `tools/build_explosion_fx.py`, played by `shaders/dirt_burst.gdshader` with the scene's light, so it is brown in the sun and never glows at night); a rising dust cloud and a ring of dust rolled along the ground; rubble chips thrown high that land and rest; a few sparks; a 0.14 s flash of light; a 3.4 m scorch decal with soot, thrown dirt, and fragment pits; and a camera shake within 22 m. Per preset (Low, Medium, High) it draws 5, 6, 6 dirt jets and 24, 30, 36 chips (`DETAIL`). A pool of three. Frag grenades and RL-4 rockets play it (`scenes/equipment/frag_blast.tscn`, `rocket_blast.tscn`).
- **Search and Destroy's charge** (`CombatEffects.charge_explosion`, `scripts/charge_explosion.gd`): the explosion 2.5 times as big, under a fireball 6.5 m across, a smoke column rising to about 15 m, a skirt of dust to about 10 m, and a flash lighting 26 m round, with a camera shake out to about 50 m.
- **Rockets** (`scenes/equipment/rocket.tscn`): the RL-4's loaded rocket in flight, with the motor's flame and light at its tail and a smoke trail left hanging behind it.
- **Fire** (`scenes/effects/fire.tscn`, `CombatEffects.fire`): upright flames playing Unity Labs' CC0 SmallFlame01 loop (`shaders/fire_flame.gdshader`), dark smoke, embers, and a flickering light, burning for a set time and then dying down over 2 s.
- **Smoke grenades** (`scenes/effects/smoke_screen.tscn`, `CombatEffects.smoke`): a bank of billows that pours out over 2 s, hangs, and thins away over its last 3 s, each billow churning through Unity Labs' CC0 Cloud loops (`shaders/smoke_screen.gdshader`). It is lit as one round bank (the side toward the sun bright, the far side and underside in shade), each billow its own shade, the rim broken into wisps while the core stays opaque. Inside it a grey fog covers the screen and the billows are not drawn. 14, 18, or 22 billows by preset, smaller where there are more. A cloud can start part-way through (`age`), for a player who joins after it popped. The rules (5 m radius, 16 s) are in [weapons.md](weapons.md).
- **Flashbangs** (`scenes/effects/flashbang.tscn`, `CombatEffects.flash_pop`): a white-hot star-shaped burst for 0.08 s (a head-on frame of the muzzle flash atlas), a spray of white sparks (10, 14, or 20 by preset), a 0.15 s flash of light, and a small grey puff that churns and drifts for a couple of seconds. A pool of two.
- **Ambient dust**: 90 motes drift in the air round the player and catch the light (`AmbientDust` in `scenes/player.tscn`).
- **Motion blur**: a setting, off by default (`scripts/motion_blur.gd`).

`godot/tests/capture_explosion.gd` films an effect as a frame strip in the sun, in shade, and at night (`CAPTURE_EFFECT` is `explosion`, `charge`, `smoke`, or `flash`; `CAPTURE_PRESET`, `CAPTURE_ONLY`). `godot/tests/combat_fx.gd` saves frame strips of blood, impacts, and holes, and with `FX_BENCH=1` measures a scripted firefight; see [performance.md](performance.md).

Left out on purpose: depth of field while aiming (the weapon is drawn into a thin slice of depth so it never clips into walls, so a near blur would blur the iron sights too), and a heat haze over hot barrels (it needs a copy of the screen, about 0.4 ms on High; see [performance.md](performance.md)).

## Characters

### Palette

Every vertex of a character carries its palette role (UV2, with its part group). Cloth and gear roles are *tinted*: the atlas holds only their shading detail, centred on mid grey, and `rust/game/src/character/palette.rs` gives each team a colour per role. Skin, eyes, the visor, and hardware (`metal`) keep the atlas colour. `shaders/character.gdshader` draws every role of a kit with one material per team.

| Role | Allies | Enemies |
| --- | --- | --- |
| `uniform` | tan (0.60, 0.53, 0.40) | charcoal (0.22, 0.23, 0.24) |
| `pants` | (0.50, 0.45, 0.34) | (0.20, 0.21, 0.22) |
| `helmet`, `vest` | coyote (0.52, 0.46, 0.34), (0.45, 0.40, 0.29) | (0.225, 0.235, 0.225), (0.15, 0.155, 0.16) |
| `webbing` | ranger green (0.40, 0.40, 0.30) | (0.16, 0.165, 0.17) |
| `gloves`, `boots`, `mask` | browns and tan | near black |
| `skin`, `eyes`, `visor`, `metal` | the atlas colour | the atlas colour |
| `accent` (helmet band, armbands) | blue (0.16, 0.56, 1.0) | red-orange (1.0, 0.24, 0.10) |

Teams differ in value first and in accent hue second (a unit test, `teams_differ_in_value_first`, holds it). A faint rim in each team's colour (strongest for the dark enemies) keeps a silhouette apart from a dark background at night and indoors.

### Budgets

| Item | Budget (`character_models_test.gd`) | Current |
| --- | --- | --- |
| Soldier triangles | 26,000 | 25.4k in one mesh (`Body`); 5.1k in `BodyFar` |
| First-person arms triangles | 26,000 | 24.8k |
| Bones | | 53 (Godot's humanoid profile) |
| Materials | One per team and kit, shared by every character of that team | 1 (`shaders/character.gdshader`, 13 roles) |
| Surfaces per character | 1 | 1, plus the weapon |
| Textures | One atlas per kit | 2048 px albedo, normal, and ORM for the soldier and the arms |

`SoldierRig` draws `BodyFar` instead of `Body` past 15 m (`FAR_DISTANCE`, visibility ranges with a 1 m margin). Godot also generates LODs at import, but its simplifier does poorly on a kit of many small parts, so the far mesh is what makes distant characters cheap. Distant characters also animate less often (see [performance.md](performance.md)).

### Faces and variation

Characters carry their detail in baked maps, not in the shader: each kit's atlas has albedo, a tangent-space normal map (cloth folds, ripstop, MOLLE webbing, Cordura, stitching, leather grain), and ORM. On top of that, `shaders/character.gdshader`:

- colours each tinted role from the team palette, with faded patches that differ per character;
- collapses to a point, shadows included, the roles a team does not wear and the part groups a character does not wear (`hidden_parts`);
- multiplies the skin by the tone that goes with the soldier's face (`skin_tone`) and varies it a little per character;

The per-character values are instance uniforms (`variant`, `hidden_parts`, `skin_tone`, and the wound marks), set by `SoldierRig`, so every character of a team still shares one material.

**Face coverings.** From its seed (`palette::hidden_parts`) each soldier wears a balaclava, a neck gaiter over the nose, or neither, so a squad is not a row of clones. A covering hides the skin it covers.

**Faces.** A soldier wears one of five faces. The build makes the base MakeHuman human again with other macros (`heads` in the soldier kit: a dark-skinned man, an East Asian man, an older man, and a lean man of mixed descent), fitted the same way, and stores each one's difference from the base head as a sparse blend shape on `Body` (`add_head_shapes` in `tools/blender_characters.py`), never further out than the base head under the helmet or in front of the eyes where the glasses sit. The offsets ride through decimation and the face coverings, so a balaclava takes the face's shape too. `SoldierRig` picks a face per character (`palette::face`, independent of the covering), blends 70 to 100% of the way to it, and sets its skin tone (`HEAD_TONES`). `BodyFar` has no blend shapes: past 15 m a face's shape does not show.

**Hair.** Hair and stubble are painted onto the head (`paint_hair` in `tools/blender_characters.py`): a hairline mask over the scalp, sideburns, and the jaw, which the skin bake turns into strands of the kit's hair colour with a little relief. Soldiers get a close crop under the helmet. MakeHuman's own hair is alpha-tested cards, which the one opaque character material cannot draw.

**Wound marks** (`rust/game/src/character/wounds.rs`). Where a shot lands, the skin or cloth darkens to blood for 5 s, then fades over 3 s: a ragged mark within about 8 cm of the hit, darkest and wettest in the middle. A character shows its four latest marks. Each rides the bone nearest the hit and goes to the shader as a point in the mesh's space and a strength (`wound_0` to `wound_3`), where a distance test per vertex draws it. Corpses keep their marks; a respawn clears them. The first-person arms never get any (`wound_marks_test.gd`).

### Building a character

**Sources**

- The skeleton, rest pose, and animations: Quaternius's [Universal Animation Library](https://opengameart.org/content/universal-animation-library) and [Universal Animation Library 2](https://opengameart.org/content/universal-animation-library-2), both CC0 (the free Standard tiers).
- The body, skin, eyes, and clothes: [MakeHuman](https://static.makehumancommunity.org/assets/assetpacks/index.html) CC0 asset packs, built with MPFB 2 (MakeHuman for Blender, GPL-3.0, used only as a build tool). The tool pins each download by SHA-256. The MakeHuman file server refuses a bare browser user agent, so the tool sends `curl/8.0`.
- The kit (`tools/blender_gear.py`): a high-cut helmet with a cover, rails, and a night-vision mount; ballistic glasses; a balaclava and a neck gaiter; a plate carrier with cummerbund, padded shoulder straps, magazine, admin, and radio pouches, and a hydration carrier on the back; a battle belt with a holster, hip, dump, and first-aid pouches; knee pads; knuckle guards; team bands; and a field watch for the first-person arms. The plates stand about 4 cm proud of the chest and back, so the kit has a loaded soldier's bulk.
- The build: a loaded soldier's, not MakeHuman's slim default. The macro has more muscle (0.85) and weight (0.62), and region targets (`targets` in the kit) thicken the chest, back, neck, arms, and thighs. It is girth only: `fit_to_skeleton` puts every joint back on the skeleton's, so the animations and hit zones are unchanged.
- The combat shirt and boots: modelled from the body and the fitted shoe by `tools/blender_wardrobe.py`, with high-detail sources baked onto them.

**Pipeline**

1. Download the two animation libraries' Standard zips and extract them under `~/.cache/ual-src` (or pass `--ual1` and `--ual2`).
2. Run `uv run tools/build_character_assets.py`. It downloads and checks its other sources, installs MPFB 2 into a private Blender profile, and runs `tools/blender_characters.py` in Blender, which for each kit in `KITS`:
   - builds a MakeHuman human (proportions, skin, and clothes from the kit) and dresses it;
   - fits the body and clothes onto the soldier skeleton bone by bone, so the game's bones, rest pose, and proportions never change (`godot/tests/character_models_test.gd` checks every bone against `tests/data/soldier_skeleton.json`);
   - transfers skin weights from the mannequin, models the kit around the body, and removes skin and cloth the kit always covers. Hard gear follows one bone rigidly (the helmet, the belt's pouches, the bloused trouser cuffs); skinned gear (plates, straps, cummerbund, knee pads, holster) has its weights smoothed over its own mesh (`smooth_weights`), so both faces of a thickened shell bend together. Each boot gets a modelled rubber sole with a lugged tread (`boot_soles`);
   - packs every part into one atlas and bakes albedo, normal, roughness, metallic, occlusion, and palette-role maps.
3. Import in Godot (`godot --headless --path godot --import`). The `.import` files retarget every file onto Godot's `SkeletonProfileHumanoid` with `bone_map_ual1.tres` and `bone_map_ual2.tres`: bones get the profile's names (`Hips`, `RightHand`, ...) under `%GeneralSkeleton`, so animations from either library play on the soldier. Clips whose names end in `_Loop` lose the suffix and loop.

`--kits soldier` rebuilds one kit, `--reuse` reassembles from the last bake without Blender, and `--animations-only` rebuilds just the animation sets.

**Outputs** (under `godot/assets/characters/`)

| File | What |
| --- | --- |
| `soldier/soldier.glb` and its atlas | `Body` and `BodyFar`, one material `character_soldier` |
| `arms/arms.glb` and its atlas | The first-person gloves, sleeves, and watch (the `arms` kit), trimmed to the arms so the whole atlas goes to what the player sees |
| `animations/ual1.glb`, `ual2.glb` | The clips the game uses, on each library's own skeleton |
| `animations/gait.glb` | Directional loops, the vault, prone, and the crawls (`tools/build_gait_animations.py`) |

To add an animation, add its name to `UAL1_ANIMATIONS` or `UAL2_ANIMATIONS` and run the tool with `--animations-only`. The lists hold only clips the game plays or authors from, since every shipped clip stays in memory.

**The combat shirt** (`tools/blender_wardrobe.py`) is two fabrics, as issued shirts are: a stretch-knit torso under the plate carrier and ripstop sleeves joined by raglan seams. It starts as the body itself, cut at the hem, the mock collar, and the cuffs, smoothed so no anatomy shows, and pushed out by how loose the cloth is there, so every vertex moves with the body's weights. Each sleeve has a pocket with a flap and a hook-and-loop field, a padded elbow panel, and a cuff tab over the glove; the collar has a short zip. The game shirt is that shell subdivided once with the large forms, then decimated. The high-detail shirt is the shell subdivided three times with compression folds, cuff bunching, tuck folds, seams, stitching, and the zip's teeth, baked onto the game shirt's atlas islands (`bake_pairs` in `tools/blender_hilo.py`). Fabric texture stays coarse: at the third-person atlas's texel size a finer weave only aliases.

**The combat boots** (`combat_boots` in the same file) are modelled over MakeHuman's fitted `shoes03`, which gives the foot's shape: the shoe is united with a shaft that follows the leg 19 cm up the shin (`BOOT_HEIGHT`) and remeshed into one shell. The trouser leg inside the boot is deleted and a bloused cuff (`blouses` in `tools/blender_gear.py`) goes round the top. The high-detail boot has a leather foot with a toe cap and heel counter over a rubber rand, a nylon shaft, a gusseted tongue with eyelets every 19 mm and laces, a padded collar, a pull loop, and flex creases. The game boot is about 1,150 triangles, weighted to the foot, toe, and shin.

**Reviewing.** `CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 --script res://tests/capture_characters.gd` captures both teams at 2, 10, 30, and 60 m in sun, shade, indoors, and at night, mid-animation poses, turntables and wireframes, the first-person arms, the five faces, and wound marks (`CAPTURE_SETS` picks sections; needs a GPU and a display, or Xvfb).

### The rig at runtime

The scenes that use the soldier:

- `scenes/characters/soldier.tscn`: `SoldierRig`, the third-person character. Range dummies (`scenes/props/target_dummy.tscn`) and other players (`scenes/remote_player.tscn`) instance it.
- `scenes/characters/fp_arms.tscn`: `FpArms`, the player's own arms, a child of the viewmodel in `scenes/player.tscn`.

`SoldierRig` (`rust/game/src/character/rig.rs`) takes one frame of state (`SoldierFrame`): velocity, floor contact, stance and the crouch blend, sprint, view angles, aim, weapon, reload progress, alive, mantling or vaulting, knife swing, the server's stride phase and mantle progress, the feet's turn, a hand action (planting, defusing, throwing), a ladder, a mount on cover, and the slope under a prone body. Three parts turn it into a pose:

1. **An animation tree, built in code.** Speed-driven blend spaces for standing (idle, walk, jog, sprint) and crouching, each in eight directions of travel, and the prone crawl in four, with playback scaled to the real speed so feet don't slide. Further blends add the slide, the jump, the climb, and the vault. An upper-body layer holds the arms in a gripping pose. The rig advances the tree itself, seeking every loop to the server's stride phase, so a pose depends only on the state it is given.
2. **`SoldierPoser`, a skeleton modifier** (`poser.rs`) that runs after the animation. It turns the spine back toward the aim while the feet stay planted, bends the spine, neck, and head with the aim pitch (`carry::PITCH_SHARES`), applies hit reactions, and places the weapon relative to the eyes (hip, aim, sprint, reload, kick, switch). Two-bone IK puts both hands on the weapon, and the fingers close in a shape for what each hand holds (`Hold`, chosen per weapon class): the firing hand wraps the pistol grip with the index on the trigger; the support hand makes a loose C under a handguard, wraps a pump (shotguns), or cups the firing hand (pistols). Each shape also sets how far each finger fans and how far the thumb turns across the palm. On a handguard or pump the support palm rests on the weapon's surface: in first person on a cross-section cut from the model once per weapon (`grips_of`), in third person on a section stored in the weapon file (`section` in its `grips`), because the server poses the arm hit zones from the same numbers (`carry::Grips::support_contact`). `godot/tests/bake_support_sections.gd` writes those sections; `support_sections_test.gd` fails if one drifts more than 2 mm from the model. On a vertical foregrip the hand and the arm zones move down onto the grip (`carry::FOREGRIP_SHIFT`).
3. **The weapon.** Each gun has one scene for both views (`scenes/weapons/*_view.tscn`). The `GripRight` and `GripLeft` markers and `grip_rake` say where the hands go. A view set to `world_model` skips the viewmodel shader and the sounds.

**Bladed stance.** A soldier holding a long gun stands bladed, as a rifle stance is: the upper chest turns 15° about the spine (`carry::BLADE`), support shoulder forward, and the neck turns back as much so the head still faces the aim. It squares up in a sprint, while the support hand throws, and with a pistol. The stock comes out of the chest onto the shoulder and the support hand reaches 2 cm behind the handguard marker (`SUPPORT_BACK`). The hit zones blade the same way.

**Weapon handling by class** (`rust/sim/src/hitbox/carry.rs`, shared with the arm hit zones). Each weapon is carried as one of five classes (`CarryClass::of`):

| Class | Weapons | Carry | Reload path |
| --- | --- | --- | --- |
| Rifle | rifles, SMGs, and launchers | At the shoulder | To the magazine, down to a vest pouch, back up to seat a fresh one, back to the handguard |
| Heavy | magazines over 50 rounds | Lower and canted, taking the weight | As a rifle, with a bigger tilt |
| Bolt | bolt-action rifles | As a rifle | As a rifle |
| Pistol | sidearms | Out at arm's length in both hands; muzzle down at the side in a sprint | Down to the belt as the empty drops, back up to the grip's base |
| Shotgun | per-round reloads | As a rifle | From a side pouch to the loading port and back for each shell |

The support hand's path is timed to the share of the reload at which the fresh magazine seats (`ReloadTiming`), and mid-reload each class tilts its own way (rifles roll the well toward the support hand, pistols tip up, pump guns roll the port over). On an empty reload the old magazine drops free (snapshots carry `PlayerView::reload_empty`). A switch lowers the old weapon for 0.22 s, then raises the new one over 0.3 s. Each shot kicks as hard as the weapon's first recoil kick relative to the M4's (0.6× to 2.2×) and pushes the shoulder and spine back 1.2° at a rifle's kick. A hand action kneels the body with the weapon lowered and a hand to the ground (Search and Destroy's plant and defuse), or swings the support arm over to throw. On a ladder the body faces the rungs with hands and feet on them and the weapon away. A mounted weapon leans the body 4° onto it.

**Transitions** (`rig.rs`). A slide drops in with `Slide_Start`, played from where the hips start down at twice its speed, so the body is down as soon as the hit zones take the slide pose, and gets up with the rise of `Slide_Exit` over 0.25 s. A jump pushes off with `Jump_Start` for 0.25 s into the air loop. A landing after more than 0.2 s in the air bends the knees with `Jump_Land`, fully after 0.4 s more, for 0.55 s, held to a fifth of the clip's squat because the hit zones do not crouch on landing. The torso leans 6° into a start and out of a stop at a walk's acceleration, on a soft spring.

**Feet on the ground** (`SoldierPoser::plant_feet`). A soldier standing within 15 m of the camera casts one ray down under each foot. Where the ground is higher or lower than under the body, the hips drop (at most 9 cm, to stay near the hit zones, `MAX_PELVIS_DROP`) and two-bone IK bends each leg to reach it. Where the straightened leg still falls short (a foot over the next stair down), the toe points down until the ball of the foot reaches the ground, up to 49° (`MAX_TOE_REACH`). Moving bodies' feet follow their stride.

**Directional gait** (`tools/build_gait_animations.py`, `gait::DIRECTIONS`). Neither CC0 library has strafes or backpedals, so the walk, the jog, and the crouched walk are authored in seven more directions (45° apart) from their forward cycles, and the sprint in its two forward diagonals. Each ankle's path is turned about its mean position to the new direction, so the planted foot slides opposite the travel. Directions behind the strafes start from the forward cycle played backward, so a backpedal reaches back toe first. The hips turn toward a strafe (20° walking, 28° jogging, 22° crouched) and the spine turns back so the chest stays on the aim; the line between the feet turns the same way, furthest at the back diagonals, so the trailing foot crosses in front and the legs never pass through each other in any direction or blend of two. Sideways the steps are shorter and quicker (cadence 1 + 0.2·|sin|, `LATERAL_CADENCE`), and between two authored directions the stride clock speeds up by as much as the blend shortens the step (`gait::blend_cadence`), so a planted foot keeps pace with the ground. A sprint drifting sideways (at most about 27°) blends toward its forward diagonal. Reference: Call of Duty's third-person locomotion has eight-way walk, run, and crouch-walk cycles with crossover strafes and a backpedal that reaches back; Modern Warfare (2019) blends them by the travel's direction.

**Prone.** `gait.glb` also holds `Prone` (on the belly, chest propped on the elbows, the eyes over the player's origin and the legs trailing 1.4 m behind, as the IW engine's prone body does) and the low crawl in four directions (`Prone_Crawl`, `_B`, `_L`, `_R`; the IW engine's `prone_crawl_forward`, `_back`, `_left`, `_right`), on one stride phase so they blend. The crouch blend runs 0 standing to 1 crouched to 2 prone, and a prone body lies along the slope under it (up to 30°, `hitbox::MAX_PRONE_SLOPE`), turned about the eyes. See [movement.md](movement.md) for the rules.

![Prone soldiers crawling in each direction](media/prone-crawls.jpg)

**Turning on the spot** (`gait::Turning`). Standing still, the hips and legs stay on the planted feet while the aim turns, and the spine takes the difference. Once the aim is more than 60° from the feet, they step round at 360° a second until within 5°; the aim is never more than 90° from them. While the feet swing, the walk loop blends in up to 70%, so the legs step instead of sliding. The server's body clock runs the same rule and sends the turn and the step (a byte each), so the hit zones follow the drawn body through a turn.

**Hit reactions** (`SoldierRig::hit_at`). A hit that does not kill reacts at the part it landed on (`protocol::HitPart`, or the hit shape our own shot struck): a head hit snaps the head along the shot (18° for a rifle round), a body hit tips the torso along the shot (9°) and turns it away from a round from the side (8°), an arm hit loosens that hand's grip to 75% for 0.18 s and turns the shoulder back, and a leg hit buckles the knees (the stance blend dips 0.3 toward a crouch) and folds the body forward. Each is a damped spring scaled by the square root of the force level, up to twice a rifle's. The hit zones do not follow these flinches. When our own shot hits another player, their body flinches at once; the confirming snapshot does not flinch it again.

**Deaths** (`ragdoll.rs`, `corpses.rs`). When the frame says a character is dead, the poser captures every bone's pose as drawn, and the next frame twelve capsules take the body: hips, spine, chest, head, upper and lower arms, upper and lower legs, sized from the hit zones' radii, 70 kg in all, joined by cone-twist joints (spine and chest 30° swing and 20° twist, neck 40°/45°, shoulders 80°/45°, hips 60°/20°) and one-way hinges at the elbows and knees (up to 140°). The capsules sit on the `ragdoll` layer (8), which collides with the world only, so bodies never block players, bullets, or each other. Every part starts at the character's velocity (60% of its run along the ground), and the part the killing shot hit takes a push along the shot: 24 N·s plus 4 N·s per force level (about a tenth of the damage), a little less on the head.

A shot body buckles, as Call of Duty's deaths do, instead of toppling back stiff. For about half a second short-lived muscles (`Buckle`) bend its knees, hips, and spine toward a plan chosen from the part hit and the side the round came from, then let go over 0.3 s. Shot from the front, the body hunches over the wound (up to 55° of forward bend), its struck shoulder turns back, and it goes down on that side; from behind it folds onto its knees and pitches forward; from the side it bends with the push and drops on the near knee; a leg hit gives that leg (105° against 50° at the knees) and the body goes down over it; a headshot folds the legs where it stands. A force level of 10 or more (a close shotgun blast, a sniper round) folds less and the push carries the body. A knife kill crumples where it stands: the legs give at once (see [weapons.md](weapons.md)). The muscles only turn parts against each other, so the body's centre of mass still goes with the push and gravity.

As a body settles, each limb's joint cone is rebuilt about its natural rest and narrows, and slack muscles slump it, so a body does not come to rest with a knee or an arm raised. A body that has stopped falling drags along the ground down to 0.2 m/s, then is left to friction. It is at rest once every capsule has moved slower than 0.08 m/s (and 0.6 rad/s) for a quarter second. Once a body is at rest, after 5 s, or when a seventh ragdoll starts, the oldest freezes: its capsules are freed and the skeleton keeps the pose. The held gun drops as a rigid body. `godot/tests/ragdoll_test.gd` checks the falls, the buckle, the cap, the knife crumple, and respawns.

A remote player's body goes to the scene's `CorpseYard` when they die, so a respawn gets a fresh body while the old one lies where it fell for 30 s, at most ten at once, then the oldest sinks 0.6 m into the ground over 1.5 s and is freed. Dropped guns lie for 15 s, at most eight. Guns dropped in a killcam's replay go when it ends.

### First-person arms

![First-person arms: M4 at the hip, aimed, sprinting, and reloading, the pistol aimed, and the M870 pump](media/first-person-arms.jpg)

The first-person gloves and sleeves are built by `tools/blender_hilo.py`, because the player looks at them every match. Each glove starts from the MakeHuman hand, so the fingers keep MPFB's finger weights. The high-detail glove is that hand subdivided four times (about 0.6 mm edges) and shaped from the skeleton's hand and finger frames: a moulded knuckle guard with a lobe over each knuckle, a padded back panel, pads on each finger with flex ridges, fourchette seams, a synthetic-leather palm and fingertips, a neoprene gauntlet with a hook-and-loop wrist strap, a rolled hem, and stitching along every seam. The game glove is the same hand subdivided once with only the large forms. Each sleeve is a tube cut wider than the arm, pinned at the shoulder and the cuff, and run through Blender's cloth solver while the fabric relaxes longer than the space it has, so it buckles into compression folds above the cuff and slack folds along the forearm. The game sleeve is that cloth; the high-detail sleeve adds a felled seam, a double-stitched hem, and a cuff tab. Albedo, normals, roughness, and occlusion are baked from each high-detail mesh onto its game mesh (`bake_pairs`). Panel colours differ only in value, so the team's glove colour tints them all. The kit is about 25,000 triangles: a soldier's budget, spent on the one model that is always on screen.

`FpArms` (`rust/game/src/character/arms.rs`) uses the same poser and grip markers on `arms/arms.glb`. It draws them with a first-person copy of the character shader (the viewmodel's own FOV and its key light, `palette::first_person_shader`), and anchors the shoulders behind and below the camera. The arms are drawn 1.12 times life size so the shoulders can sit behind the camera, but the hands are scaled back to life size (`SoldierPoser::hand_scale`), so they fit the grips. The viewmodel rig moves the weapon, and the hands follow it; during a reload the support hand leaves it to fetch a magazine.

While the support hand throws a grenade, the arms come back to life size about the support shoulder, and the body moves 8 cm up, 4 cm out, and 14 cm forward, as a thrower's torso turns (`throw_scale`, `throw_shift`). From the draw-back through the release the support elbow bends toward a point out beside the head (`throw_pole`), so the forearm comes over from the left edge of the view as in an overhand throw. `fp_throw_arm_test.gd` checks the elbow stays outside the view and the arms are life size as the arm comes over.

`FpArms` also draws what the hands hold instead of the weapon: Search and Destroy's charge and the wire cutters (`show_charge`, `show_cutters`).

### Hand poses per weapon

Call of Duty, like every shooter whose source or decompilation we read (Halo's first-person animations, Doom 3's view models, Team Fortress 2's arms, the grab-point poses of Godot's XR tools), authors the hands on each weapon by hand: no game we found solves fingers at runtime. We have no animator, so each weapon file carries a fitted pose (`hands` in `godot/data/weapons/*.ron`): every finger's curl per joint and spread at the knuckle, the firing index off the trigger (`index_off`; other players' soldiers lift it while sprinting or reloading), the support hand on a vertical foregrip for weapons that take one (`left_foregrip`), and how far each hand sits off its grip marker (`seat`). `godot/tests/bake_hand_poses.gd` fits them to the model (`rust/game/src/character/grasp.rs`, `FpArms.fit_hands`) the way an animator would pose them:

1. The weapon's surface around each grip becomes a small distance field (4 mm cells, measured to 16 mm), cut from the meshes the first-person weapon shows. It is negative inside a closed part, so a finger buried in a receiver reads as inside it; which side of an edge or corner a point is on comes from angle-weighted pseudo-normals (Bærentzen and Aanæs). An open part (a sheet) counts by distance only.
2. The hand slides onto the surface (at most 12 mm): the least move that takes the palm (tested as a plate) and every knuckle out of the weapon.
3. Every finger closes from nearly straight along its grip's shape (the `HandShape`s in `poser.rs`), all joints together, each joint stopping where the finger beyond it meets the surface. The gloved finger is tested as a chain of capsules measured from `arms.glb` (12.5, 11.5, and 10 mm radius from knuckle to tip), allowed to press 10% of its radius in. A finger that meets nothing keeps the shape; thumbs close further, so they rest on the weapon's side.
4. Anything still inside is nudged out: up to 12° of curl and 6° of spread from where the finger closed, wider if that is not enough.

Every weapon in the bake's `WEAPONS` list is fitted. The game only reads the result: the weapon files are compiled into the game, so re-run the bake and rebuild after changing a model, a grip marker, or the gloves. A hand's fitted pose fades as it leaves its grip (the support hand fetching a magazine, the firing hand working a bolt), but not through a pump's stroke. Other players' soldiers pose the same firing hand and foregrip fist; on a bare handguard their support hand holds further back, where the fit does not apply, so it keeps the hold's shape.

`godot/tests/hand_poses_test.gd` holds every weapon in its stored poses and fails if a sampled finger goes more than 1 mm into it, a palm more than 5.5 mm (a flat plate cannot wrap a narrow foregrip as a fist does), an arm falls more than 5 mm short of its hand, a finger goes unmeasured, a weapon has no fitted poses, or the firing hand's middle, ring, or little finger closes less than 120° round the grip. It does not compare the poses with a fresh fit. Fingers the fit cannot clear are listed in the bake's `KNOWN_DEPTHS` with how far they may go in: the M1911's support index (6 mm) and the LSW-6's support middle finger (2 mm), not yet diagnosed. Fixing the model or the marker should remove each entry.

Each `GripRight` sits on its model's grip, and each `grip_rake` follows the grip's front strap:

| Rake | Weapons |
| --- | --- |
| 12° | CQ-10 |
| 15° to 20° | M1911 16°, RL-4 18°, BP-9 20° |
| 22° | M4A1, PDW-9, LSW-6 |
| 25° | AK-47, KR-12 |
| 32° | BR-3, DM-7 |
| 38° | M40 |
| 45° | M870, SG-12 |

The markers live in three places that must agree: `tools/build_weapons.py` (which patches them into the scene), the view scene (`GripRight`, `GripLeft`, `grip_rake`), and the weapon file's `grips` (which the server's arm hit zones read; `hitbox_fit_test.gd` fails if they differ).

Values set by hand after the fit go in the weapon file's `hand_tuning`: the game applies them over the fitted pose (`HandTune::apply`), and the bake checks the poses with them applied (`FpArms.with_tuning`) but writes only `hands`, so a re-bake keeps them. They are:

- AK-47, KR-12, and LSW-6: the firing thumb lies forward along the receiver's right side under its edge, instead of standing up beside the dust cover in the aimed picture.
- AK-47 and KR-12: the support hand sits 17 mm lower with its index bent at the tip, so the thumb and index stay below a red dot's window.
- LSW-6: the support index lies straight along the handguard's side (knuckle 25°, spread −16°) instead of curling over its top edge into the red dot's window.
- M1911: the firing thumb lies over the backstrap instead of standing up beside the slide.

`FpArms.try_hand` measures a candidate pose for one hand (each finger's depth) without a rebuild, and `FpArms.preview_hand` draws one in the game, for choosing such values.

### Hit zones

`rust/sim/src/hitbox.rs` defines a player's hit zones: a head sphere and capsules for the neck, chest, stomach, hips, arms, and legs (13 shapes in five zones: head, chest, stomach, arm, leg). Its inputs are everything a snapshot says about a body: feet position, yaw, pitch, stance and crouch blend, and activity (sprinting, in the air, mantling or vaulting, reloading, aim, the weapon in hand, a throw, kneeling at the charge, the prone slope) and how far along its animations it is. What you see is what you hit: the zones follow the drawn soldier in every state another player can be seen in. Damage per zone is in [weapons.md](weapons.md).

| Shape | Radius |
| --- | --- |
| Head | 13.5 cm |
| Neck | 6.5 cm |
| Chest | 21 cm |
| Stomach | 17 cm |
| Upper arm, forearm | 6.5 cm, 5.5 cm |
| Thigh, shin | 8.5 cm, 6.5 cm |

- **Base poses.** Each state starts from the skeleton of the animation the character plays in it, recorded in `rust/sim/src/hitbox/poses.rs`: still poses (standing, crouched, prone, sliding, in the air), the standing blend at 16 stride phases at each of six speeds (`gait::STRIDE_POSITIONS`: the walk, an aimed walk, the jog, the 4 m/s run, halfway up a sprint, and the sprint) in every measured direction, the crouched walk, the prone crawl every 45°, and a mantle's climb and a vault at 17 points each. Still poses are measured holding a pistol, which never blades, since the zones blade the upper chest themselves. The zones pick the measured poses around the snapshot's phase and blend them (`rust/sim/src/hitbox/gait.rs`, shared with `SoldierRig`).
- **Animation progress.** The server keeps each player's stride phase, reload progress, mantle progress, and turn (`hitbox::clock::BodyClock`) and sends them in every snapshot (a byte each). Every loop runs on that one phase, and `SoldierRig` seeks to it rather than letting loops run free; the climb or vault is seeked to the mantle's progress and blends in over its first 15%, as the zones do. So every client draws the moment the zones are posed at, and the server's rewind poses the same moment.
- **Velocity.** Snapshots carry none, so a sample blended between two snapshots takes the velocity from one to the other (`RemoteSample::velocity`). The client's interpolation and the server's rewind blend the same two snapshots, so both get the same velocity. It picks the poses by speed and direction of travel as the tree does (`gait::POSE_DIRECTIONS`, `SPRINT_POSE_DIRECTIONS`).
- **Aim.** The pitch bends the spine, neck, and head in the same shares the poser uses.
- **Arms.** The weapon sits where the character holds it, by class, from the same numbers (`carry.rs`), and two-bone IK puts the hands on that weapon's `grips`. A throw and a plant move the support arm as drawn (`hitbox/action.rs`).
- What the zones do not follow: the rig's smoothing between states (a fraction of a second), the climb fading out after a mantle (0.15 s), a knife swing, and the short hit flinch.

The zones are a pure function of snapshot fields, not of the animated skeleton. The dedicated server never runs animation, and lag compensation has to rebuild the zones a player had several ticks ago (see [netcode.md](netcode.md)). A pure function makes both cheap and identical on every machine. `godot/tests/hitbox_fit_test.gd` generates `poses.rs` (`HITBOX_FIT_DUMP=$PWD/rust/sim/src/hitbox/poses.rs`, then `cargo fmt`); regenerate it when an animation changes.

`hitbox_fit_test.gd` poses a remote player through every state (idle, every gait and direction, crouched, prone and crawling, sliding, aimed, reloading, mantling, vaulting, in the air, each weapon, pitch from −89° to 89°, turning, throwing, kneeling) and, per zone, measures how far the skeleton's bones are from the capsule axes and how far outside the zone the skinned mesh lies. It fails when a zone's bones stray more than 6 cm from its axes or one in ten of its vertices lies more than 5 cm outside; a pump gun's reach for shells (arm, 8 cm) and a mantle's legs (9 cm) get a little more. `capture_hitbox_fit.gd` captures the overlay in the same states.

- `HitboxRig` (`rust/game/src/hitbox_rig.rs`) turns the shapes into physics bodies on the `hitboxes` layer (3). Each carries `hit_zone` metadata, so hitscan gets the zone from whatever it hits. Posing is lazy: the shapes move when something is about to test against them (`hitbox_rig::sync_all`) or every 100 ms.
- **F4** (`debug_hitboxes`) draws every character's zones over it, coloured by zone, and, online, the zones the server rewound for each of our shots in white. Only debug builds can turn it on, and the overlay is depth tested, so it never shows players through walls.
- Range dummies block movement with a capsule on the `bodies` layer (5). Bullets ignore that layer and hit the zones.

To add another character, add a kit to `KITS` (its MakeHuman body, clothes, and gear), and either a scene that instances its model under a `SoldierRig` or an entry in a rig's `variants`.

## Menus and HUD

The menus and HUD follow Treyarch's Black Ops 2 and Black Ops 3 (the owner's pick, 2026-11-27), in our own art, names, and icons. The theme is code: `godot/scripts/ui/ui_theme.gd` (colour tokens, type, and a style for every Control), merged into the default theme by the `UiThemeLoader` autoload.

- **Colour.** Near-black, square-cornered translucent plates with 1 px cool-grey borders. One hot orange (`ACCENT`, #F85A08) means focus or the open tab and nothing else: a focused row or tile gets 3 px orange rules along its top and bottom edges, and the focused menu item, the open tab, and the primary action are solid orange plates with a soft glow. Yellow means ready, reward, or you (`READY`, `LEVEL`, `SELF`). Cyan and red are only for teams (`team_colours.gd`). Light label plates with dark type (`LabelPlate`) carry team names and description titles.
- **Type.** Chakra Petch SemiBold (wide, squared) for screen titles, banners, and big numerals; Barlow Condensed for menu items, tabs, labels, rows, and HUD text; Barlow for sentences. Titles, tabs, HUD labels, and banners are caps; rows and descriptions are mixed case.
- **Shapes** (`godot/scripts/ui/hud_shapes.gd`): hexagon objective markers and badges; long banners with pointed ends and gapped leader lines; big words with a thin cyan and red colour split at their edges; a light edge along the top and outer side of HUD plates; a thin cross on an empty slot.
- **Frame** (`godot/scripts/ui/screen_frame.gd`): full-screen menus darken the scene from the left, add faint scan lines, and draw a header (title bracket, a small context line, the title in caps, one cool underline) and a black footer band for button prompts.

The research notes, sampled colours, and private reference screenshots this follows are kept out of the repository; nothing in it is traced or copied from Call of Duty.

### Front end

The screens outside a match follow Black Ops 3's multiplayer menus, with Black Ops 2's custom game rules list for the match setup.

| Screen | What it does like Black Ops 3 | Script |
| --- | --- | --- |
| Title | The frame's header ("MULTIPLAYER" under the game's name), a list of caps items on dark plates that hug their text (the focused one a solid orange plate with a glow and a small orange pointer beside it, with a brief flash as focus lands), gaps between groups instead of headings, a line under the list saying what the item does, a card in the lower left showing where the item leads (the map's picture with a cut corner, its kind in the header strip, the map on a light plate and the mode under it), your name in pale yellow with a blue hexagon level badge and XP bar top right, and key prompts in the footer band. The backdrop is a soft, warm still of Breakwater's loading rack, darkened from the left | `title_screen.gd`, `ui/map_card.gd`, `ui/player_card.gd` |
| Match setup and Game Settings | Opening a mode pushes the screen: the header names the mode, the list dims but for the open item, and a panel with cut corners slides in from the right, the same size on every page. Rule rows are drawn as Black Ops 3's settings rows (a name box and a value box; the value between small arrows; focus grows orange rules along the row's top and bottom and turns the value orange; a yellow star in a column before the name marks a rule off its default, as Black Ops 2 marks one), then a Game Settings link row and START MATCH at the foot. On the right, the map's picture with its name on a light plate, and a description of the focused row (or of its value) under a light title plate with its default in yellow. Esc or B goes back | `tdm_setup.gd`, `ui/rule_row.gd`, `ui/custom_settings_page.gd` |
| Join Server | The same panel: the address field and CONNECT, the status line (a plate with a red edge on an error, the theme's error colour), recent servers as rows, and a description on the right; Enter and Esc prompts in the footer band | `join_server.gd` |
| Loading | While a match's map loads and until the server lets you in: the map's picture softened and darkened, the map name in small grey caps over the mode in big squared caps beside the mode's mark (a hexagon with a simple figure per mode), for team modes a band with each side's team mark (the same marks as the lobby and scoreboard, on a dark hexagon) and YOUR TEAM / ENEMY on a light plate either side of a hexagon "VS" chip with gapped leader lines off its ends, the mode's objective, your card (your name on a yellow strip), and a tip over a thin white loading bar in a black band at the foot, with the cancel prompt | `ui/loading_screen.gd`, `net/net_status.gd` |

Deliberate differences: no character render behind the menu (we have no specialists), so the backdrop is the map; no party list, so the top right shows only you; the loading screen shows only your card, not every player's. Its bar and map name are white and grey rather than Black Ops 3's orange, since orange means focus. Left and right step a rule row's value and A steps it on, wrapping.

### Pause menu and Options

The pause menu and the Options screen (`godot/scenes/ui/pause_menu.tscn`, `godot/scripts/pause_menu.gd`) follow Black Ops 3's in-match menu and PC options, in our own art. The game behind is blurred and dimmed under the screen frame.

- **Pause.** The title reads PAUSED (MENU on someone else's server, where the match keeps running). A list of caps items sits under the header; each is a dark plate that hugs its words, and the focused one is a solid orange plate. A line under the list says what the focused item does. On the right, a match card (`ui/pause_card.gd`) shows the map's picture, the mode in squared caps, the map on a light label plate, and in a match the clock and both sides' scores on plates tinted in each side's colour, with LEADING, TRAILING, or TIED and the score limit. Your name and level sit in the header's top right.
- **Options.** The title reads OPTIONS over a context line (PAUSED, or MAIN MENU from the title screen). Caps tabs hang from the header's underline; the open one is solid orange, and blank end tabs hold the previous and next tab keys (Page Up and Page Down, or LB and RB glyphs once a controller is used). Rows (`ui/option_row.gd`) are 620 px wide, 37 px tall with 5 px between them: black at 85% with a 1 px border, the name in mixed case at the left, and the value in grey from 300 px. A slider is a 2 px white track with a 5 x 14 thumb and its number beside it; a check box sits left of the name; a drop-down shows its value and a chevron, and its list opens under the value column; a key binding shows the key in grey caps, centred, with the controller button as a grey glyph on the right, and an orange outline while it waits for a key. Groups have a name in mixed case above them and a 30 px gap. The focused row, from the keys, a controller, or the mouse over it (hover moves the focus, as Black Ops 3 does), gets orange rules along its top and bottom that grow from the middle in 90 ms. A device locked out of play (the one not picked under Input device) shows LOCKED beside its section's name and its rows in one grey; they stay adjustable. The pane on the right names the focused setting on a light label plate and says what it does and its default. A long tab scrolls so the focused row stays a row clear of the edges, and fades at an edge while more rows lie past it.
- **Footer and confirm.** The footer band holds key caps with their words (Select, Back or Resume, Reset to Default); while a binding waits for a key, only Cancel. Reset to Default asks in a confirm band (`ui/confirm_band.gd`): the screen dims, a black band between two thin light rules crosses it, a light label plate names the action over the question, and No (focused) and Yes sit side by side.

Deliberate differences from Black Ops 3: the pause page is a list beside a match card, not Black Ops 3's hub of tiles, as the game has fewer places to go; there is no Apply prompt, because every setting applies at once; the controller's buttons are listed beside each key binding rather than on a tab of their own.

### Around the match

The screens between the fighting follow Black Ops 3's, drawn at a 1600x900 design size and scaled to the screen, with the pieces in `godot/scripts/ui/match_paint.gd`.

| Screen | What it does like Black Ops 3 | Script |
| --- | --- | --- |
| Countdown | A dark plate hugging the mode (or "ROUND 3 · ATTACKING", the side in full white) in tracked caps over the objective; under it the seconds as a big squared numeral with the colour split, popping in from 1.3x and fading over its last 150 ms | `net/match_banner.gd` |
| Round and match result | The result in big caps on a long plate with pointed ends, top centre: VICTORY on green, DEFEAT on crimson, round results and DRAW on steel grey. It opens from the middle, gapped leader lines draw off its points, and the split settles as the word appears. Why it ended sits under it on a solid grey plate whose edge is the winning side's colour (white for a draw). HALFTIME and OVERTIME use the same slot on a smaller plate | `net/match_banner.gd`, `net/match_results.gd` |
| Team strip | The loading screen's VS band, so the two agree: a band with pointed ends across the middle, each half tinted its side's colour with a team rule along its top, each side's name on a light plate, then its mark, then its score against the hexagon VS chip, and the goal ("FIRST TO 75") on a tab under the chip. The halves slide in as the scores count up. After a match, the match's XP sits under it on one plate with our XP mark (a square with cut corners) | `net/match_banner.gd` |
| Scoreboard | Hold Tab (Back on a controller), and after a match under the result. A header with the clock, the mode and its goal or round, and the column names; a block per side with its name on a light plate, its score in big numerals, and its mark as a watermark the score overlaps; every score is grouped with commas ("1,030"), as in the match report and on the pause card (`MatchPaint.number`); a near-solid row per player tinted with the side's colour, every other number column banded. Your row has thin yellow rules and pale yellow type (yellow means you; orange stays for focus); players out of the round fade with a skull. Free-for-all is one list with each place, the top three in gold, silver, and bronze | `net/scoreboard.gd` |
| Match report | Under the results board, as wide as it: the score, the match bonus, and the total; the level mark (the front end's blue hexagon with the level in it), the XP bar (earlier progress grey, this match's gain yellow, filling over 1.2 s) and the next level's mark hollow; then what unlocked and how far each weapon used is from its next attachment | `net/xp_summary.gd` |
| Death | The screen dims, red-black at its edges; KILLED BY on a small plate with leader lines in the killer's colour over one card: the killer's colour down its edge, the name in big condensed type, the weapon's silhouette over its name. A kill without a gun shows a drawn mark over its name instead (the kill feed's knife, Create-a-Class's grenade line icons, the charge; the RL-4's silhouette for a rocket, shrunk into the same box as every silhouette so the card keeps its width); killing yourself reads Yourself in your yellow; a fatal fall reads Fall damage with a falling mark, an unknown death Killed in action with a skull, both behind an empty grey mark, so the card is never half empty. The killcam's weapon row draws the same marks. The card has a hairline border, as the killcam's name cards do. The respawn bar fills along the card's bottom edge, with RESPAWN IN in whole seconds on a tab under it | `net/death_card.gd` |
| Killcam | A short white flash, then the title on a steel plate with a chevron end and leader lines (KILLCAM, FINAL KILLCAM, or ROUND WINNING KILL, with the winners named under it, WINNERS in yellow only for your side), a thin line under it that fills as the replay plays, the weapon top right, and the killer's and victim's name cards (each with a hairline border) either side of the hexagon VS chip in a band at the foot | `net/killcam_hud.gd` |

Deliberate differences: the end-of-match strip is our loading screen's band, not Black Ops 3's full-width strip; there are no calling cards or emblems (cut), so name cards and team marks are plain shapes drawn in code; the XP mark is our own square, and orange stays for focus, so earlier XP progress is grey.

### HUD

The HUD follows Black Ops 3's in-match layout in our own art (the research reference is section 5 of the private spec; nothing is traced). The pieces draw themselves (`_draw`) with the helpers in `godot/scripts/ui/hud_paint.gd` and `hud_shapes.gd`, and the HUD size setting scales the corner clusters (`Settings.fit_hud`; see [settings.md](settings.md)).

| Where | What | Script |
| --- | --- | --- |
| Top left | The minimap (a wide window with a thin light frame, red contour lines round the map's edge, and a ticked compass strip under it), the mode's rows under it (Domination's A, B, C), and short streak notices under those ("UAV ONLINE", "ENEMY UAV OVERHEAD") | `net/minimap.gd`, the mode's view, `net/streaks_hud.gd` |
| Top centre | Medals: our own emblem with its name under it, up to three side by side, the newest coming in from the right | `score_feed.gd` |
| Top right | Frame rate, when turned on (off by default, as Call of Duty's counter is) | `hud.gd` |
| Bottom left | The score block: our team's mark (our own geometric glyph) on a dark disk, the clock in a capsule on its rim (red and beating in the last 30 seconds), the mode beside it, our score big over theirs with a thin bar each toward the limit, and LEADING, TRAILING, or TIED under the disk with the limit beside it. The kill feed above it (killer, weapon silhouette, headshot mark, victim on a dark plate per row; enemies salmon, allies cyan, you yellow), and chat above that. Outside a match, your own score in the same form | `net/score_block.gd`, `net/kill_feed.gd`, `net/tdm_hud.gd`, `score_feed.gd` |
| Bottom right | The health ring (where Black Ops 3 shows its specialist charge), the ammo plate left of it (magazine big, reserve smaller, red when low or out), the weapon's name under it, the tactical and lethal slots above it (a cross when empty), and the scorestreak column above the ring | `hud_vitals.gd`, `hud_weapon.gd`, `net/streaks_hud.gd` |
| Centre | Crosshair, hit markers (white X, red for a kill), red direction arcs, the grenade danger marker, the "+N" points just above the crosshair with bonus lines under it, the blinking ammo warning under it, and prompts on a dark plate below | `crosshair.gd`, `damage_overlay.gd`, `ui/grenade_danger.gd`, `score_feed.gd`, `hud_centre.gd` |
| Upper third | Announcement banners | `net/match_banner.gd` |
| Lower third | The death card ("KILLED BY" on a small plate with leader lines in the killer's colour, then one card: a full-height edge in their colour, their team mark and name, their weapon's silhouette and name, the respawn bar along its bottom edge, and RESPAWN IN on a tab under it) and the hint under it | `net/death_card.gd`, `hud_centre.gd`, `net/match_banner.gd` |
| Bottom centre | A scorestreak earned: "UAV READY" on a yellow plate with "Press [4]" under it. After each kill of ours, "KILLED" over the victim's name on a plate, for two seconds (Black Ops 3's kill card, as a name plate: there are no calling cards) | `net/streaks_hud.gd`, `hud_centre.gd` |
| Bottom edge | Subtitles, when turned on (`ui/captions.gd`), and the controls legend, from the live bindings and only for actions that work here, for 14 seconds after the first spawn (F1 toggles it) | `hud.gd`, `hud_centre.gd` |

The corner clusters sit on planes turned a little toward the screen's centre, as Black Ops 3's do: a slight skew (`HudShapes.tilt`) lifts the bottom clusters toward the middle and dips the minimap's far edge. Plates drawn under it use `HudShapes.smooth_rect`, so their edges stay smooth. Objective markers everywhere are pointy-topped hexagons, as Black Ops 3's are, in the holder's colour (sand with a dark letter while neutral; cyan or red with a white one) with a light rim, the action word above on a dark notched plate, a pointer and the distance under it, and a ring that fills round the hexagon during a capture (`ui/objective_marks.gd`). A mode's markers are placed together so they never pile up: one off screen, or one that would sit on a corner cluster, holds to an ellipse inside the screen (its foot above the bottom clusters) with an arrow toward its spot and without its words; markers closer than 34 px slide apart; a farther marker drops its words where they would cross a nearer one's; and the nearest draws on top. Medals use round, shield, and other shapes; hexagons are kept for objectives and badges.

Team colours (cyan for your side, red for the enemy) come from `godot/scripts/ui/team_colours.gd`, never a local constant, so the colour-blind palettes in Settings reach every piece. Black Ops 3 timings followed: hit markers hold 160 ms and fade over 80 ms (a kill's red one 260 ms); points add up within 1.5 s, pop from 125% over 120 ms, and fade over 300 ms; medals land from 160% in 150 ms, hold 2.2 s, and fade over 250 ms; kill feed rows live 5 s, fade over 400 ms, and slide up in 120 ms; direction arcs fade over 1.2 s; the ready plate rises in 150 ms and holds 3 s.

Deliberate differences from Call of Duty: health shows as a number in the ring, because it regenerates in steps worth reading; the score block says LEADING or TRAILING in our own words; the team mark, medal emblems, and streak icons are our own simple shapes drawn in code; the kill card is a name plate.

Weapon silhouettes (`assets/ui/weapons/`) are rendered from the weapon models by `godot/tools/weapon_icons.gd`; run it again after a model changes.

#### Button prompts

![The controls legend with keyboard and mouse, an Xbox controller, and a PlayStation controller](media/button-prompts.jpg)

Prompts show the button to press as Call of Duty's do: a keyboard key in a light key cap, and a mouse or controller button as its image (Kenney's Input Prompts, CC0, in `godot/assets/ui/glyphs/`), in an Xbox and a PlayStation set. Settings, Controls, "Button prompts" picks Auto (the connected controller's set, PlayStation when its name or USB vendor says Sony), Xbox, or PlayStation, as Black Ops 6 and Modern Warfare III do on PC.

Prompt strings name their key in square brackets ("[F] Take M870", "Hold [pad:x] to plant the charge"). `Settings.key_label` and `prompt_key` write a keyboard key by name and a controller or mouse button as a token (`pad:a`, `pad:rt`, `mouse:left`; pad tokens use the Xbox position names on any controller), and `hud_paint.gd` draws each bracketed key, anywhere in the line, as a cap or image (`godot/scripts/ui/glyphs.gd`). Labels that cannot draw images write the button in words (`Glyphs.plain_text`). In play, prompts follow the input device that plays; menu footers follow the device used last and switch the moment the other is touched, as Call of Duty's menus do (`ui/footer_keys.gd`).

#### Minimap

![The minimap on Warehouse (a map since cut) in Domination, with the score block under it](media/minimap.jpg)

The minimap follows Black Ops 2 and every Call of Duty since (`godot/scripts/net/minimap.gd`), drawn as Black Ops 3 draws its: a wide window, top left, about 73 by 41 m, centred on you and turning with you so you always face up, with a ticked compass strip under it (letters every 45 degrees, our heading marked at its centre). Settings, General, "Rotate minimap" turns that off and keeps north up, as Modern Warfare 2019's option does. You are a yellow arrow, teammates are cyan chevrons pointing where they look, and the mode's objectives (Capture the Flag's flags, the live Hardpoint, Domination's flags, Search and Destroy's sites and charge) sit on it as hexagons in their holder's colour, held at the edge when out of the window and spread apart so none covers another. The plan is recoloured as Black Ops 3 shades its minimap: open ground a pale grey-blue slab, rooms a shade darker, and solid blocks dark with a thin light edge. Thin red contour lines, rounded at the corners, ring the ground off the map (`shaders/minimap.gdshader`). Enemies show only as red diamonds where the server reveals them (`rust/sim/src/radar.rs`): firing without a suppressor puts the shooter on every enemy's minimap where they fired, for 3 seconds; knives and grenades show nothing. The server sends each player only the dots their own radar shows (`RadarView` in each snapshot). Scorestreaks add to it (UAV sweeps, Counter-UAV static, Care Packages; see [scorestreaks.md](scorestreaks.md)). It hides in killcams and on maps without a plan.

Each map's plan is a top-down image rendered from its collision by `godot/tools/build_minimaps.gd` (`godot --headless --path godot --script res://tools/build_minimaps.gd`, or `MINIMAPS=breakwater` for one map). It casts rays down through every pixel: floors are graphite, lighter the higher they stand; walls and solid cover are pale, cut a little above the floor so building interiors show, as Call of Duty's do; roofed floors are a shade darker, so buildings read as blocks; and a pale line marks every step taller than a kerb. Our plans are flat and colourless where Black Ops 2 painted its from the level's textures, to keep the HUD quiet. Run the tool again after a map's layout changes. The images are in `godot/assets/minimaps/`, and each map's square in metres is in `godot/scripts/net/minimap_data.gd`.
