# Settings

The settings menu (pause menu or Options on the title screen) follows the options Call of Duty exposes on PC, Black Ops 6 and Modern Warfare III first, and offers only what the engine honours. Settings live in `godot/scripts/settings.gd` (autoload `Settings`) and save to `user://settings.cfg`. The menu is `godot/scripts/pause_menu.gd`.

## Controls

| Setting | Values (default) | What it does | Call of Duty |
| --- | --- | --- | --- |
| Input device | Keyboard & mouse, Controller (the controller if one is connected at first launch) | The device that plays; the other is locked out of play, menus take both | CoD takes both at once; locking is a project decision (TODO.md) |
| Mouse sensitivity | 0.1 to 5 (1.00) | Turn per mouse movement | Mouse Sensitivity |
| Aim sensitivity | 0.3 to 2× (1.00×) | Turn speed aiming down sights, as a share of the above | ADS Sensitivity Multiplier |
| Look speed | 1 to 14 (7) | Right stick turn speed; each step 20% faster | Horizontal/Vertical Stick Sensitivity (BOCW onward, 1 to 14) |
| Aim response curve | Standard, Linear, Dynamic (Standard) | Standard raises deflection to the curve strength; Linear turns in step with the stick; Dynamic is an S-curve (smoothstep), fine at first and quick through the middle (`look::StickResponse`) | Aim Response Curve Type |
| Curve strength | 1 to 3 (2.00) | The Standard curve's exponent | — (our extra, kept from before) |
| Right stick deadzone | 2 to 30% (10%) | Radial inner deadzone of the look stick | Right Stick Min |
| Left stick deadzone | 2 to 40% (12%) | The movement actions' deadzone | Left Stick Min |
| Trigger deadzone | 0 to 60% (25%) | How far a trigger pulls before it fires or aims | Left/Right Trigger Deadzone (one value for both here) |
| Invert Y axis, Invert mouse Y | off | | Invert Vertical Look |
| Aim assist | on | See roadmap.md | Aim Assist (types left out by decision) |
| Vibration | on | | Controller Vibration |
| Button prompts | Auto, Xbox, PlayStation (Auto) | Which controller's button images prompts show (art-direction.md, Button prompts) | Controller button prompt type |

Every gameplay action can be rebound on keyboard and mouse (Keybinds), including prone, the scorestreak keys, interact, the controls legend (F1), and the Testing Area's ammo refill. Controller buttons follow the default Call of Duty layout and are not rebindable yet.

## Interface (General tab)

| Setting | Values (default) | What it does | Call of Duty |
| --- | --- | --- | --- |
| Camera motion | on | Head bob and strafe tilt | Camera Movement |
| Show FPS counter | off | Frame rate, top right | Telemetry → FPS Counter (off by default) |
| Team colours | Default, Protanopia, Deuteranopia, Tritanopia (Default) | Your side's and the enemy's colour everywhere they appear: score block, kill feed, scoreboard, minimap, objective markers, mode HUDs, chat (`godot/scripts/ui/team_colours.gd`) | Color Customization → HUD Color Palette (protanopia, deuteranopia, tritanopia) |
| HUD size | 80 to 125% (100%) | Scales the corner pieces (health, ammo, score block, minimap, kill feed, streaks, points, prompts) about the screen corner each is anchored to, margins included (`Settings.fit_hud`) | HUD size / Magnified HUD preset (Black Ops 6) |
| HUD opacity | 30 to 100% (100%) | Fades the same pieces; the crosshair, hit markers, and damage indicators keep theirs | Black Ops 6 has crosshair and minimap opacity; one slider here |

The palettes: Default is blue against red. Protanopia and deuteranopia keep a blue and move the enemy to an orange from Okabe and Ito's colour-blind-safe set (a lighter orange for protanopia, which sees red light as dim). Tritanopia, which confuses blue with green, uses teal against a pinkish red. Black Ops 6 also lets players pick each colour by hue; that and a world colour filter are not offered yet.

![Default, protanopia, deuteranopia, and tritanopia palettes on the Domination HUD](media/team-colour-palettes.jpg)

![HUD size at 80% and 125%](media/hud-size.jpg)

A new HUD piece joins the size and opacity settings with `Settings.fit_hud(piece)` once it is in the tree. Pass `own_fade = true` if the piece animates its own `modulate`. A full-screen piece that draws its corners itself must read the screen size from `get_viewport_rect()`, not its own `size`, when it sizes things.
