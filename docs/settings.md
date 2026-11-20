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
