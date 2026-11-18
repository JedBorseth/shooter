# Graphics: how the game should look

The target is a **Treyarch Black Ops game** (Black Ops 2 and Black Ops 3) built
with the **craft of the modern Modern Warfare games** (2019 onward). Treyarch
sets the look: bold colour, strong light, clean readable shapes, punch. Modern
Warfare sets the finish: real materials, grounded light, detailed weapons,
dense worlds, effects that linger. When the two pull apart, Treyarch's
readability and colour win, and Modern Warfare's detail fills in around them.

Use this file for any change to how something looks: maps, props, materials,
light, grade, post-processing, weapons, characters, effects, and the graphics
presets. It extends [docs/art-direction.md](../../../../docs/art-direction.md),
which records how each part is built today.

## The Treyarch base (Black Ops 2 and 3)

What makes a frame read as Black Ops:

- **Colour with intent.** Saturated and punchy rather than neutral, but never
  cartoonish. Each map owns a strong palette and time of day (a golden low
  sun, a hard noon, neon at dusk, a cold night) that you could name from a
  thumbnail. Warm light against cool shade, done with restraint.
- **Strong key light.** One clear sun or moon with crisp, readable shadows,
  bright skies, and lit surfaces that pop. Interiors are lit by visible
  sources (windows, fixtures, signs), not murk.
- **Contrast and depth.** Deep but readable blacks, bright highlights, firm
  contrast in the midtones. Haze separates distance layers so the play space
  stands out from the backdrop.
- **Bloom and flares as signature, not wash.** Bright things glow (sun, lamps,
  neon, muzzle flashes, explosions), and the sun throws a flare and lens dirt
  when it is in view. Black Ops 2's look leaned on this; keep it on highlights
  only so it never fogs the screen.
- **Readability first.** Players are the clearest thing in any frame: value
  contrast against the background, a rim of light on their silhouette, quiet
  surfaces where enemies stand. Landmarks are bold and colour-coded so players
  know where they are at a glance.
- **Clean, confident shapes.** Bevelled edges that catch a specular line,
  clean surfaces with moderate wear, bold signage and colour accents. A little
  heightened over real life.
- **Viewmodels that shine.** The gun in hand is crisp, high contrast, with sharp
  specular highlights on metal and polymer, and reads instantly. Camos and
  finishes can be bold.
- **Effects with punch.** Oversized bright muzzle flashes, hot tracers, chunky
  sparks and debris, explosions with a white-hot core and a fat smoke column.

## The Modern Warfare craft (2019 onward)

What we add on top:

- **Real materials.** Photoreal PBR from scanned CC0 sets: roughness that
  varies across a surface so highlights break up, layered wear (chipped paint
  on edges, dust in crevices, grime rising from the ground, water stains under
  sills), wet surfaces where it has rained.
- **Grounded light.** Bounce light that fills rooms and shade naturally (baked
  lightmaps), ambient occlusion and contact shadows that sit every object on
  the ground, light shafts and haze through windows and doorways, physically
  plausible exposure from bright outdoors to dim interiors. Muzzle flashes and
  explosions light their surroundings for a frame or two.
- **Weapons as hero props.** Real proportions and machining (screws, knurling,
  receiver seams, rail teeth), worn finishes on edges and contact points,
  optics with glass that reflects and a reticle that glows, sights that hold
  zero as the gun sways.
- **Effects that stay.** Muzzle smoke that drifts off the barrel after a spray,
  heat shimmer, dust knocked off surfaces by impacts and footsteps, ejected
  cases that bounce and rest, per-material impact puffs and debris with decals
  that last.
- **Lived-in worlds.** Dense dressing: clutter, cables, pipes, signage, trash,
  furniture, parked vehicles, posters, light fixtures, all modelled (see the
  Blender rule in the art direction). Every room tells you what it was.
- **Atmosphere.** Aerial perspective and haze by distance, a real sky (HDRI)
  that matches the sun, distant skyline detail.
- **Characters with real kit.** Fabric with folds and seams, plate carriers,
  pouches, straps, and gloves modelled, not painted flat.

## What we leave out

- Modern Warfare's dark, desaturated murk and black interiors where players
  hide in shadow. Shade always keeps enough fill to spot a player.
- Black Ops 3's sci-fi look (glowing armour, holograms, future tech). Our
  setting is present-day military.
- Bloom that fogs the whole screen, heavy film grain, strong chromatic
  aberration, depth of field that blurs the sights, and motion blur on by
  default.
- Camouflage on characters that breaks up their silhouette, and busy textures
  or decals where players look for enemies.
- Anything lifted from a real Call of Duty game: no textures, models, skies,
  or screenshots in the repository. Reference images stay local.

## Turning it into settings

Where each part lives today (details in docs/art-direction.md and
docs/performance.md):

| Part of the look | Where it is set |
| --- | --- |
| Map palette, exposure, haze, bloom, colour grade | `godot/scripts/looks.gd` (`MAPS`, `BLOOM`, the grade LUT) |
| Key light and sky | each map's sun and `Environment`, CC0 HDRIs from `tools/build_skies.py` |
| Bounce light | baked LightmapGI (`tools/bake_lightmaps.sh`) |
| Materials and wear | `godot/shaders/world.gdshader(inc)`, looks in `tools/world_looks.py` |
| Sun flare, lens dirt, vignette, sharpening | `godot/scripts/lens.gd`, `scripts/lens_dirt.gd`, `shaders/lens_flare.gdshaderinc` |
| Grounding | SSAO per preset, `scripts/grounding.gd` contact shadows |
| Presets | `godot/scripts/graphics.gd` |

Starting points when tuning a map's grade, to check against captures, not
rules: saturation a little above neutral (about 1.05 to 1.15); an S-curve with
the toe deepening shadows without crushing them; cool shadows and warm
highlights that both fade out toward black and white; bloom only above white
(HDR threshold at or above 1), stronger on emissives than on sunlit surfaces.
Players in the deepest shade of a map must still read against the wall behind
them at 30 m on Medium.

## Performance decides how much

The budget in [docs/performance.md](../../../../docs/performance.md) still gates
every visual change (GTX 1060 at 1080p: Low 4 ms, Medium 8 ms, High 12 ms of
GPU time at the worst view), with the benchmark comparison in the PR.

The Treyarch base is mostly cheap: colour grade, light direction, contrast,
bloom on highlights, clean shapes, and good art. The Modern Warfare craft is
mostly expensive: volumetrics, detail normals, dense dressing, lingering
particles. So presets cut Modern Warfare craft first and never cut the
Treyarch read: Low still has the map's palette, grade, key light, and readable
players; Medium adds grounding, bloom, and flare; High gets the full finish.
Bake what can be baked (lightmaps, AO, wear in textures) rather than compute it
every frame.

## Checking a look change

1. Collect references for the area you are changing into
   `~/.local/share/shooter/look-references/` (create it if missing): Black Ops
   2 and 3 for colour, light, and readability; Modern Warfare 2019 onward for
   materials, weapons, and effects. Screenshots from press kits, reviews, and
   capture sites are fine to look at locally; never commit them.
2. Capture before and after with `godot/tests/capture_picture.gd` or
   `BENCH_CAPTURE=<dir> tools/benchmark.sh`, under the GPU lock other threads
   use, on Low, Medium, and High.
3. Put the captures beside the references and check each point:
   - Would the frame pass for a Black Ops map in a thumbnail (palette, key
     light, contrast)?
   - Is every player the clearest thing in frame, including in shade and at
     range?
   - Up close, do materials, wear, and the weapon hold up next to Modern
     Warfare?
   - Is anything in "What we leave out" creeping in?
   - Does Low keep the read?
4. Fix the biggest gap first, then recheck. Put the before and after captures
   and the benchmark table in the PR, with which reference each change chased.
