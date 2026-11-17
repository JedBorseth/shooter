# Art direction

How the game should look, and how the characters (other players, range dummies, and the player's own arms) are built.

![Both teams in several poses, the F4 hitbox overlay, and close-ups of an enemy and an ally](media/characters.jpg)

## The look

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

`shaders/character.gdshader` bumps the normal from a height field, using screen-space derivatives (no tangents or UVs needed): the canvas weave and soft creases around the limbs on cloth, and a leather grain with creases on gloves and boots that fades out before it gets smaller than a pixel. It reuses the weave sample the shader already took, so it adds no texture reads.

### Props

![Covered cars, street lamps, crates, bins, barrels, jersey barriers, and roof plant across the three maps](media/map-props.jpg)

Props are CC0 models from [Poly Haven](https://polyhaven.com/models), built by `tools/build_props.py` in Blender: it downloads the 1K glTF, keeps the named parts, decimates to a triangle budget (1.5k to 6k; the textures carry the fine detail), puts the origin at the bottom centre, and writes `godot/assets/props/<name>/<name>.gltf` with its textures beside it. Godot generates LODs at import.

Props never change how a map plays. Each one that should block players or bullets stands on a box in a hidden CSG combiner (`Blockers`, `CoverBlockers`, or the map's existing combiners, now hidden), so collision stays exactly as simple as the blockout and impacts still report a surface. Crate stacks are scaled to fill their old blockout box; jersey barriers are scaled to fill the old low cover.

### Slamcity

![Slamcity: Slam Junction over the avenue, the derailed car in the alley, a platform, the avenue from the platform, the diner, and the climb up the derailed car's aisle](media/slamcity-junction.jpg)

Slamcity is a city block at dusk cut by an elevated railway. Every surface of it is generated, so the whole map is rebuilt from code:

- `tools/slamcity_layout.py` is the one description of the map: collision, visual parts (bevelled boxes, hulls, cylinders, tubes, 3D and neon letters, pictures from the sign atlas, and window panes), placed models, props, decals, baked lights, spawns, lanes, and zones. Walls are written once with their doors and windows (`Wall`), and the east half is the west half mirrored, with its own signs and colours (`Side`).
- `tools/slamcity_models.py` models the cars, buses, trucks, the train, water towers, the shop interiors, and the street furniture from the same primitives. Solid stairs are one stepped extrusion (the `extrude` part: any flat polygon swept along a vector, its caps split into convex pieces), so their sides bake without seams. `tools/build_slamcity_city.py` runs in Blender, builds every part and model, merges them by chunk (a 3 x 2 grid, a finer grid for small things, and the skyline) and by material, and writes `godot/assets/maps/slamcity/city.gltf`. Each chunk also gets a shadow-only twin with all its opaque geometry in one surface.
- Looks come from `tools/world_looks.py`. Textured looks that share a texture set and scale share one material, their tint in the vertex colour; plain colours and lights fold into three palette materials. About 50 materials cover the map. `godot/tools/slamcity_import.gd` swaps them in at import.
- Windows players cannot use are panes with a room behind the glass (`shaders/window_pane.gdshader`, the same interior mapping as the skyline). On the buildings players fight around, their blinds are drawn and the rooms dim, so they never read as firing positions.
- `tools/build_slamcity_signs.py` draws every billboard, poster, shop sign, package, and route map into one atlas. Every brand and name on them is made up.
- `tools/build_slamcity.py` writes the scene and refuses a layout that fails `tools/slamcity_checks.py`: collision must be mirrored; no spawn point may be in sight of the enemy's spawns, roof, platform, train, or diner roof; no line longer than 60 m may run clear down the avenue or the alley, longer than 50 m between the teams' high ground (lines that thread both rows of the stalled train's windows are allowed: keyholes, not lanes), or longer than 35 m from a platform down its side of the avenue; at least 15 spawn points must hide from the team's own high ground and 8 from an enemy inside the team's half. Every solid collider blocks sight in these checks, the derailed car's tilted boxes included. CI runs them (`tools/test_slamcity_layout.py`), with hashes that catch a scene, mesh, or lightmap left stale against the layout; `godot/tests/slamcity_test.gd` checks the spawn sightlines again with real physics rays and bakes a navigation mesh to prove bots can reach every zone and flag.

![Slamcity's plan: the spawn yards at each end, the back alley, the tenements and pawn shops, the avenue, Slam Junction and the wreck on the mirror line, the storefronts and the ticket hall, with spawns, flags, Hardpoint zones, and bot lanes](media/slamcity-plan.jpg)

Hardpoint rotates through five zones on the mirror line: the Ticket Hall, Under the El, the Roadworks in the pit, the Back Alley, and the Platforms. Every move goes at least 16 m to a zone whose footprint does not touch the last one's (`check_hardpoint_hops`), so the fight moves across the map instead of a room over or a floor up, and the south storefronts, the avenue, and the alley take turns as the approach.

The map's set pieces are the derailed car, nose-down in the alley with its roof crushed, its sides torn and sooted, and a bogie hanging by its cables, and the burst water main in the roadworks pit (`godot/scenes/effects/water_spray.tscn`): a column of thin velocity-aligned streaks with a little mist and splash, about 1 ms of GPU time when it fills the screen on Medium (the `slamcity_closeups` benchmark views watch both).

#### Decals and trim

Slamcity's surfaces carry CC0 ambientCG decals (`tools/build_decals.py` packs them into `godot/assets/textures/decals/`): worn crossings, dashed centre lines, and edge lines (grey in the texture, painted white or yellow by the Decal's modulate), patched and broken asphalt, manholes, rain streaks under the roof edges, parking bays in the yards, and puddles and oil stains the tool generates (their ORM map makes puddles nearly mirror-smooth, so they catch the sky). Following the rule that texture detail stays quiet, there is no graffiti where players look for enemies.

Visual trim has no collision, so it never changes movement or cover: window frames, sills, lintels, cornices, quoins, and downpipes on the buildings.

### Light

![Slamcity, the Testing Area, and Zombies before (left) and after (right) the lighting pass](media/lighting.jpg)

Each map is lit by a CC0 sky image (an HDRI from [Poly Haven](https://polyhaven.com), downloaded by `tools/build_skies.py` into `godot/assets/skies/`) and one directional light standing in for its sun or moon. The tool prints where the sun sits in each image; the map rotates its sky (`Environment.sky_rotation`) so the sun lines up with its light, and tilts the light to the same elevation. The sky also lights the shade (ambient light from the sky), so shadows pick up its colour instead of a flat grey.

| Map | Sky | Mood |
| --- | --- | --- |
| Testing Area | `kloofendal_48d_partly_cloudy_puresky` | Clear midday with clouds, high sun, light haze |
| Slamcity | `kloppenheim_06_puresky` | Dusk: a low sun due south behind the storefronts (12°, above the sky's own 5° so the streets keep some light), long shadows, warm haze, and neon, lit windows, and street lamps baked into the lightmap |
| Zombies | `qwantani_moon_noon_puresky` | Moonlit night with stars, blue moonlight, dim fog, and street lamps that flicker (`scripts/lamp_flicker.gd`) |

![Before (left) and after (right) the baked lightmaps: Slamcity's street, a corner store, the alley, and the Testing Area](media/lightmaps.jpg)

Each map also has a baked LightmapGI: three bounces of sunlight and sky light, so rooms, the walkway, and the shade under awnings get soft bounced light and darken where the sky can't reach. `tools/bake_lightmaps.sh` bakes it in two steps:

1. `godot/tools/bake_map_meshes.gd` turns each map's visible CSG into meshes with lightmap UVs (saved in `godot/assets/maps/<map>/`), places them beside the CSG, and hides the CSG, which keeps its collision and impact surface. Running it again replaces what it made. Props get lightmap UVs at import.
2. The editor plugin in `godot/addons/lightmap_baker/` opens each scene in the editor, presses its own Bake Lightmaps button, and saves. It needs a GPU (Xvfb works with one) and does nothing unless `BAKE_LIGHTMAPS` is set.

Rebake a map after changing its geometry or light. Lights that exist only to be baked (Slamcity's ceiling lights in the flats and stores) sit under a node with `scripts/baked_lights.gd`, which removes them in the game: their light is already in the lightmap and its probes, which also light characters.

Zombies adds volumetric fog, which only the High preset draws.

### The graded picture

![The Testing Area, Slamcity, and Zombies before (left) and after (right) the grade, the bloom change, and the lens](media/graded-picture.jpg)

The final picture of a map is set in `godot/scripts/looks.gd`, not in its scene, so a map that a tool regenerates keeps it. For each listed map it holds:

- **Environment overrides**: exposure, bloom, and haze. Bloom lifts only what is brighter than white (`glow_bloom` is 0), so lamps, muzzle flashes, and the sky's sun glow without a veil over the whole picture. Zombies makes up for the lost veil with exposure, so the horde reads as well as before.
- **A colour grade**: an S-curve on brightness around a pivot, a toe that deepens the darkest shadows without clipping them, a cool tint in the shadows and a warm one in the highlights that both fade out at black and white, and a saturation. A bright colour that would leave the range is pulled toward its own grey, so it keeps its hue. `Looks.build_lut` turns the grade into a 32³ look-up table when the map loads, and the renderer samples it inside the tonemap pass, so a grade costs nothing per frame. Tune a grade offline against a screenshot before you try it in the engine: the maths is a few lines. The renderer reads the table at texel centres with no half-texel correction, so texel *i* must hold the grade of input (*i* + 0.5) / 32 (`Looks.texel_input`); a table built as *i* / 31 darkens every shadow by more than 1% of full brightness.

| Map | Grade |
| --- | --- |
| Testing Area | Crisp midday: firm contrast, cool shadows, warm highlights, slightly muted colour |
| Zombies | Cold night: muted colour, blue shadows, warm lamp highlights, shadows kept readable |
| Slamcity | Dusk: firmer contrast, cool shade against warm lamp and sun light. Its environment overrides keep bloom to the neon, the lamps, and the lit windows (`glow_hdr_threshold` 1.3), so sunlit surfaces never glare |

Nuketown is not listed and keeps the look authored in its scene.

On top of that, for every map:

- **The lens** (`scripts/lens.gd`): a vignette and a little film grain, and on High a contrast-adaptive sharpening pass. The vignette deepens slightly while aiming, and `suppress()` closes it in for a moment (for rounds passing close).
- **Health**: the colour drains from the picture as health falls from 70% to 10% and comes back as it regenerates. Death is grey. This uses the tonemap pass's saturation, so it works on every preset. The damage vignette (`shaders/damage_vignette.gdshader`) multiplies the edges of the picture toward a dark blood red, so the middle of the screen stays clear and the sky never turns pink.
- **Texture sharpness**: Medium and High read slightly sharper mipmaps.

What each of these costs is in [performance.md](performance.md).

### Weapons

![Every weapon at the hip, the M4 firing, its smoke, and the weapon racks](media/weapon-materials.jpg)

The weapons are being rebuilt from detailed CC0 source models, one family at a time. Rebuilt so far: the AK-47 and the M1911, from Stein Games' Free Classic Weapons Pack (rigged, about 10k to 17k triangles, 2K PBR textures). The others still use the low-poly pack with the old baked textures (`tools/build_weapons_legacy.py`).

`tools/build_weapons.sh` rebuilds the set in one command. `tools/fetch_weapon_sources.py` downloads the source packs (pinned by size and SHA-256, cached in `~/.cache/shooter-weapons/sources`), and `tools/build_weapons.py` runs in Blender and, for each weapon in its `WEAPONS` table:

1. Imports the rigged source in its rest pose. Every face goes to the part its bone moves, so the magazine, the slide, the hammer, and the rounds in the magazine come out as their own nodes.
2. Turns it to face -Z, scales it to the real weapon's length and sight radius, puts the rear sight's aiming point (the top of the notch) on the origin, and pitches it so the front post's tip lies on the -Z axis. Aimed, the post sits level in the notch exactly where the camera points, which is where shots land.
3. Repacks the source's maps into the game's layout at 2048 px: sRGB colour, an OpenGL normal map (the sources are DirectX style), and ORM (occlusion, roughness, metallic). The source's own UVs stay.
4. Exports `<key>.glb` in metres with one material for the weapon (`<key>.tres`), the shared cartridge material on the rounds, and the shared tritium glow on night sights, and `World`, every fixed part merged into one node for other players' weapons and racks (the magazine stays separate for third-person reloads), then writes the marker positions (`Muzzle`, `Ejection`, `OpticMount`, `GripRight`, `GripLeft`) into the view scene and a summary (`<key>.json`: triangles, nodes, markers).

Parts each rebuilt weapon exports (origins at the part's pivot, so a trigger or hammer turns about its pin):

| Weapon | Nodes |
| --- | --- |
| AK-47 | `Body`, `Magazine` (with its `Rounds`), `MagazineSpare` (hidden, for a reload that brings a fresh one up), `ChargingHandle` (the bolt carrier and its handle), `Selector`, `Trigger`, `MagRelease`, `OpticRail` (shown only with an optic) |
| M1911 | `Body`, `Slide` (with the front sight's `SightGlow`), `Magazine` (with its `Rounds`), `MagazineSpare`, `Hammer`, `SlideStop`, `Safety`, `Trigger`, `MagRelease` |

These weapons aim with their own modelled sights, so their view scenes have no `IronSights` node. An optic sits on the `OpticMount` marker; when the mount is too low for it, the weapon hangs that much lower (`WeaponView::set_sight_drop`), so the optic's dot is on the sight line and the iron sights show in the bottom of its window. Weapon racks show the same third-person weapon other players carry, laid on its side (`WeaponPickup`).

`godot/tests/weapon_models_test.gd` checks every rebuilt weapon's parts and markers and that its front sight's tip is within 0.6 mm of the sight line. `godot/tests/capture_weapons.gd` captures each weapon at the hip, aimed, mid-reload, and inspected, in other players' hands, and on the racks, for reviewing.

Each shot shows a flash with ragged petals that change every shot, a short plume along the barrel seen from the side, and every few shots a puff of grey smoke that drifts up and fades in the world (`shaders/muzzle_flash.gdshader`, `shaders/muzzle_smoke.gdshader`, built in `WeaponView`).

### Effects

![Sparks and dust on a wall, bullet holes, motion blur on a fast turn, and muzzle smoke](media/effects.jpg)

- **Impacts** (`scenes/effects/impact.tscn`): glowing spark streaks that follow their velocity (`shaders/spark.gdshader`), chips of debris in the surface's colour, a soft lit dust puff that grows and fades, and a bullet hole (`tools/build_decals.py` makes its albedo and normal map) tinted per surface. Flesh gets blood droplets and mist instead; metal sparks but does not chip.
- **Tracers** (`shaders/tracer.gdshader`): a hot core with a soft edge, bright enough to bloom.
- **Ambient dust**: a few dozen motes drift in the air around the player and catch the light.
- **Motion blur**: an option in the settings, off by default.

Depth of field while aiming was tried and left out: the weapon is drawn into a thin slice of depth (so it never clips into walls), so a near blur can only blur the whole weapon, iron sights included, which makes aiming worse.

### Palette

Every vertex of the model carries its palette role. `rust/game/src/character/palette.rs` maps each role to a colour per team, and `shaders/character.gdshader` draws all of them with one material per team.

| Role | Allies | Enemies |
| --- | --- | --- |
| `uniform`, `pants` | tan (0.56, 0.51, 0.39) | charcoal (0.20, 0.21, 0.22) |
| `vest`, `webbing`, `helmet` | olive (0.40, 0.39, 0.28) | near black (0.11, 0.12, 0.12) |
| `gloves`, `boots`, `mask` | dark brown and olive | black |
| `skin` | (0.60, 0.43, 0.33) | same |
| `visor` | glossy near black | same |
| `accent` | blue (0.16, 0.56, 1.0) | red-orange (1.0, 0.24, 0.10) |

There are no teams in the game yet. Until Team Deathmatch lands (Phase 4), every other player and every range dummy uses the enemy palette, and the player's own first-person arms use the ally palette.

### Budgets

| Item | Budget | Soldier today |
| --- | --- | --- |
| Third-person triangles | 15k | 14.5k (body 13.7k, gear 0.8k) |
| Bones | 60 | 53 (Godot's humanoid profile) |
| Materials | One per team, shared by every character of that team | 1 (`shaders/character.gdshader`, 11 roles) |
| Draw calls per character | 4 | 2 (body and gear) plus the weapon |
| Textures | Shared, small | The canvas weave from the world texture library |

Godot generates LODs for the meshes at import, so distant soldiers cost less.

### Character detail

![Both teams on the range, an ally up close, and the zombie horde with a close-up](media/characters-v2.jpg)

`tools/build_character_assets.py` merges each of the soldier's meshes (body, gear, first-person arms) into one surface. Each vertex stores its rest-pose position (UV and UV2.x) and its palette role (UV2.y). `shaders/character.gdshader` then:

- colours each role from the team palette, and collapses roles a team does not wear (a zombie's helmet and goggles) to a point, shadows included;
- adds detail in rest-pose space, so it sticks to the body while it animates: a fabric weave on cloth, faded patches that differ per character, and dirt toward the feet;
- for zombies, varies the skin tone per zombie and adds blood, so a horde does not look cloned.

The per-character seed is an instance uniform (`variant`, set by `SoldierRig`), so every character of a team still shares one material. A 24-zombie horde went from about 630 draw calls to about 195.

## Building a character

### Sources

Characters start from Quaternius's [Universal Animation Library](https://opengameart.org/content/universal-animation-library) and [Universal Animation Library 2](https://opengameart.org/content/universal-animation-library-2), both CC0. The free Standard tiers include a mannequin on a humanoid rig and about 90 animations: locomotion, crouch, jumps, slides, deaths, hit reactions, pistol aim poses, and zombie moves.

### Pipeline

1. Download both Standard zips and extract them.
2. Run the asset builder:

   ```sh
   uv run tools/build_character_assets.py \
       "<UAL1>/Godot/AnimationLibrary_Godot_Standard.glb" \
       "<UAL2>/Unreal-Godot/UAL2_Standard.glb"
   ```

   It writes three files to `godot/assets/characters/`:
   - `soldier/soldier.glb`: the soldier. The mannequin's triangles are split into surfaces by the bone each one follows (skin, uniform, gloves, pants, boots). The gear is lofted around the body from its own vertices, so it fits the body and is skinned to the same bones. A separate `ArmsFP` mesh holds only the arms, for the first-person view.
   - `animations/ual1.glb` and `animations/ual2.glb`: the animations the game uses, on each library's own skeleton, with everything else stripped out.
3. Import in Godot (open the editor, or run `godot --headless --path godot --import`). The `.import` files retarget all three files onto Godot's `SkeletonProfileHumanoid` with `bone_map_ual1.tres` and `bone_map_ual2.tres`. Bones get the profile's names (`Hips`, `RightHand`, ...) under a skeleton named `%GeneralSkeleton`, and bone axes are normalised. Animations from either library then play on the soldier. The animation files import as `AnimationLibrary` resources. Clips whose names end in `_Loop` lose the suffix and loop.

To add an animation, add its name to `UAL1_ANIMATIONS` or `UAL2_ANIMATIONS` in the tool and run it again. To use a different body or animation pack on another rig, give its file a bone map: in the import dock, select the `Skeleton3D`, choose **Retarget > Bone Map > New BoneMap**, and set the profile to `SkeletonProfileHumanoid`. Name its materials after palette roles so team colours apply.

### Runtime

The scenes that use the soldier:

- `scenes/characters/soldier.tscn`: `SoldierRig`, the third-person character. Range dummies (`scenes/props/target_dummy.tscn`) and other players (`scenes/remote_player.tscn`) instance it.
- `scenes/characters/fp_arms.tscn`: `FpArms`, the player's own arms. It is a child of the viewmodel in `scenes/player.tscn`.

`SoldierRig` (`rust/game/src/character/rig.rs`) takes one frame of state: velocity, floor contact, stance, sprint, view angles, aim-down-sights amount, weapon, reload progress, and whether the player is alive. Three parts turn that frame into a pose:

1. **An animation tree, built in code.** Speed-driven blend spaces for standing (idle, walk, jog, sprint) and crouching, with playback scaled to the real speed so feet don't slide. A backward walk plays the cycle in reverse. Further blends add the slide and the jump. An upper-body layer holds the arms in a firing pose, and a transition plays the death. The rig advances the tree itself, so a pose depends only on the time it is given.
2. **`SoldierPoser`, a skeleton modifier** (`poser.rs`) that runs after the animation. It turns the hips toward the movement direction and the spine back toward the aim, so strafing reuses the forward cycles. It bends the spine, neck, and head with the aim pitch, leans the torso away from hits, and places the weapon relative to the eyes (hip, aim, sprint, reload, kick, switch). Two-bone IK then puts both hands on the weapon, and the fingers curl around it. While a soldier dies, the weapon stays in the right hand.
3. **The weapon.** Each gun has one scene for both views (`scenes/weapons/*_view.tscn`). The `GripRight` and `GripLeft` markers and `grip_rake` say where the hands go. A view set to `world_model` skips the viewmodel shader and the sounds, and its muzzle flash faces the camera.

![First-person arms: M4 at the hip, aimed, sprinting, and reloading, the pistol aimed, and the M870 pump](media/first-person-arms.jpg)

`FpArms` (`arms.rs`) uses the same model, poser, and grip markers. It shows only the arms mesh, draws it with the viewmodel shader (its own FOV, so it never clips into walls), and anchors the shoulders behind and below the camera. The viewmodel rig moves the weapon, and the hands follow it. During a reload the support hand leaves the weapon to fetch a magazine.

## Hit zones

`rust/sim/src/hitbox.rs` defines a player's hit zones: a head sphere and capsules for the neck, chest, stomach, hips, arms, and legs. Its inputs are feet position, yaw, pitch, and stance (standing, crouching, sliding). The layouts were measured from the soldier's `Idle`, `Crouch_Idle`, and `Slide` poses. The arms hold a rifle and turn with the pitch.

| Zone | Damage |
| --- | --- |
| Head | the weapon's `headshot` multiplier (1.2 to 1.5) |
| Chest, stomach | 1.0 |
| Arms | 0.9 |
| Legs | 0.85 |

With these, the M40 still kills with one shot to the head, chest, or stomach, but not to a limb.

The hitboxes are a pure function of snapshot fields, not of the animated skeleton. The dedicated server never runs animation, and Phase 3's lag compensation has to rebuild the hitboxes a player had several ticks ago. A pure function makes both cheap and identical on every machine. The model is posed to match the shapes, and the debug overlay shows both at once.

- `HitboxRig` (`rust/game/src/hitbox_rig.rs`) turns the shapes into physics bodies on the `hitboxes` layer. Each body carries `hit_zone` metadata, so the player's hitscan gets the zone from whatever it hits.
- **F4** toggles the overlay in debug builds: every character's hitboxes drawn over it, coloured by zone. Release builds ignore the key, and the overlay is depth tested, so it never shows players through walls.
- Range dummies block movement with a capsule on the `bodies` layer (layer 5). Bullets ignore that layer and hit the zones.
- Other players' hitboxes are drawn but not hittable yet. Shots against them resolve on the server together with lag compensation (Phase 3).

## Weapon sights

![Aimed down the M4, AK-47, M870, M1911, and PDW-9 sights, and the M4 at the hip](media/iron-sights.jpg)

The gun pack models its sights as solid blocks, which look like slabs when you aim through them. `IronSights` (`rust/game/src/sights.rs`) cuts those blocks out of the imported mesh and builds new sights in code, in the same flat-shaded, hard-edged style:

| Style | Weapons | Rear | Front |
| --- | --- | --- | --- |
| `ar` | M4A1 | Flip-up aperture between two protective ears | A-frame tower, square post inside U-shaped ears |
| `ak` | AK-47 | Tangent leaf with a wide U-notch blade | Post inside a round hood that is open at the top |
| `shotgun` | M870 | Ghost ring between ears | Ramped blade with a red fiber-optic rod |
| `pistol` | M1911 | Notched blade with two green tritium dots | Post with one tritium dot |
| `reflex` | PDW-9 | Mini reflex sight: hooded window, coated lens, red dot | |

Rules the sights follow:

- **Sight line.** Each weapon scene's origin is on the sight line, so at full ADS the sight line is the camera's axis. An aperture is centred on it. A notch sight follows "equal height, equal light": the front post's tip is level with the top of the rear blade, and the post fills the middle of the notch.
- **Contrast.** Sight steel is darker than the weapon body, so the sight picture reads against it. Dots and fibers are emissive, so they show in shade and pick up a little glow.
- **Clear picture.** Each weapon's `ads_position` puts the rear sight close to the eye, so the aperture or notch is large on screen. The support hand holds the handguard low enough to stay out of the sight picture. The AK, M870, M1911, and PDW-9 models sit lower under their sight lines than the pack draws them, so the new sights stand clear of the receiver.

A `Sights` node in each `scenes/weapons/*_view.tscn` sets the style, the rear and front positions along the sight line, the height of the weapon under each sight, a size scale, the glow colour, and `cuts`: boxes in the view's space. Any triangle of the imported model whose centre lies in a box is removed. The cut meshes and the built sights are cached and shared by every copy of a weapon, including third-person ones.

To review sights, capture every weapon at the hip and aimed: `CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 --script res://tests/capture_sights.gd` (needs a GPU and a display, or Xvfb).

## Zombies and other characters

A Zombies mode would use the same skeleton and pipeline. Universal Animation Library 2 already includes `Zombie_Idle_Loop`, `Zombie_Walk_Fwd_Loop`, and `Zombie_Scratch`. To add a zombie:

1. Add those clips to the tool's list.
2. Give a zombie body its own palette, or make a new body and give it a bone map.
3. Build an animation tree from those clips. The soldier's poser is optional, because zombies don't carry weapons.
