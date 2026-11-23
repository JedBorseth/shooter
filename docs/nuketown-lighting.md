# Nuketown lighting

The converted Nuketown 2025 keeps its original geometry and materials (see
[assets/nuketown.md](assets/nuketown.md)). Its lighting is rebuilt in
`godot/scenes/nuketown.tscn` from the map's own exported light data, and its
colour grade is set in `godot/scripts/looks.gd`.

## Source

The export's `mp_nuketown_2020_lights.json` holds one directional light, the
sun, and 16 spot lights inside the two houses.

- Sun direction, Call of Duty Z-up: `(-0.6027, -0.5240, 0.6018)`; in Godot's
  Y-up, toward the sun: `(-0.6027, 0.6018, 0.5240)`.
- Elevation `asin(0.6018)` = 37 degrees, so the `Sun` node's rotation is
  pitch -0.645 rad and yaw -0.854 rad.
- The spot lights' positions place the warm interior lights and the carport
  fill.

The target is Black Ops 2's bright retro-future daylight: a blue sky with hazy
horizons, warm cream concrete, a salmon-orange house and a blue-grey modern
one, shaded walls that keep their colour, and dark but readable interiors.

## The scene

| Setting | Value |
| --- | --- |
| Sky | `assets/skies/range_day.hdr` panorama, `sky_rotation` 0.12 |
| Sun | Energy 2.7, warm white, shadows to 100 m |
| Ambient | From the sky, energy 1.1, so shaded facades stay blue-grey instead of near-black |
| Tonemap | AgX, exposure 0.9 |
| SSAO | Radius 1.2, intensity 1.4 |
| Fog | Density 0.0022, cool light colour, light sun scatter |

`scripts/looks.gd` replaces the scene's contrast and saturation with a graded
look-up table for a hard desert noon (firmer contrast and a toe, warm sunlit
paint against cool shade, the candy colours a shade calmer so soldiers read
against them) and raises the aerial perspective to 0.8 so the far rooftops
and the tower fade. It also gets the bloom and lens dirt every graded map
shares. The grade runs inside the tonemap pass, so it costs
nothing per frame ([art-direction.md](art-direction.md)).

Inside the houses:

- Four interior `ReflectionProbe`s (`Interior` in the scene) give the ground
  floors and garages a darker local ambient: 0.4 in the living rooms, 0.55 in
  the garages. Each box is turned to its house (17 degrees for the blue house,
  -20.7 for the yellow one), its faces sit halfway through the walls, and it
  blends over 0.1 m, so the inner faces of the walls get the interior ambient
  and the outer faces never do. `godot/tests/nuketown_bounds_test.gd` checks
  points inside and outside the rooms.
- Low-energy warm omni lights sit at the fixtures in both houses (living room,
  kitchen, hallway, fireplace, staircase), with a cool fill under the blue
  house's carport.

### No lightmap

A lightmap bake needs a second UV set, and the converted world has none on
any of its surfaces. SSAO and the interior probes stand in for baked bounce
light, so interiors are flatter than Black Ops 2's baked ones. Adding UV2 in
the importer and baking would replace them. Nuketown also has no benchmark
viewpoints, which is why its look changes only settings that cost
nothing.

## Review captures

`godot/tests/nuketown_capture.gd` loads the real scene (player, HUD,
viewmodel, environment, collision) on a GPU and saves fixed views:

```sh
CAPTURE_DIR=<dir> xvfb-run -a -s "-screen 0 1600x900x24" \
  godot --path godot --resolution 1600x900 --script res://tests/nuketown_capture.gd
```

Drop `xvfb-run` on a machine with a display. `CAPTURE_HUD=0` hides the HUD;
`CAPTURE_QA=1` adds a view of the hearth, where the export has a floor hole
that the bake repairs. The houses' ground floors are at y = -1.45 and the
street sidewalk at about -1.68, so free cameras sit at standing eye height
above those.

| View | Camera (x, y, z), yaw | Shows |
| --- | --- | --- |
| `01_street_spawn` | (-1.5, -0.08, 14), -4 | The street from the sidewalk: both houses, the bus, the haze |
| `02_blue_house_exterior` | (8, -0.17, -4), -80 | The shaded blue house's front, entrance, and glass |
| `03_blue_house_fireplace` | (17, 0.15, -6), 0 | The suspended fireplace, chimney, and hearth |
| `04_blue_house_stairs` | (17, 0.15, -3), -90 | The staircase, landing, and pendant lights |
| `05_yellow_house_exterior` | (-2, -0.15, -12), 90 | The yellow house |
| `06` to `09_player_*` | Player camera | The first-person player at the spawn, the fireplace, the stairs, and the back room |
| `10_qa_floor_hole` | (18, 0.15, -8), -41 | With `CAPTURE_QA=1`: the repaired hearth floor |

Captures: [street](media/nuketown-fidelity-street.jpg),
[blue house](media/nuketown-fidelity-blue-house.jpg),
[stairs](media/nuketown-fidelity-stairs.jpg), and
[repaired hearth floor](media/nuketown-fidelity-floor.jpg).
