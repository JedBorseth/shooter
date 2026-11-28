# Settings

The Options screen (Options on the title screen, or in the pause menu)
follows the options Call of Duty exposes on PC, Black Ops 6 and Modern Warfare
III first, and offers only what the engine honours. It has five tabs:
Controls, Keybinds (Button layout while a controller plays), Graphics, Audio,
and General (LB and RB, or Page Up and
Page Down, step between them). Picking a setting shows what it does in the
pane beside the list. Every change applies at once and saves itself shortly
after (0.75 s) to `user://settings.cfg`; Reset to Default (R, or Y on a
controller) asks first, then puts every setting and keybind back. Its look
is in [art-direction.md](art-direction.md#pause-menu-and-options).

The settings live in `godot/scripts/settings.gd` (autoload `Settings`: the
defaults, allowed ranges, and how each applies), and the menu is
`godot/scripts/pause_menu.gd`. `godot/tests/settings_test.gd` checks loading,
saving, and the ranges.

## Controls

| Setting | Values (default) | What it does | Call of Duty |
| --- | --- | --- | --- |
| Input device | Keyboard & mouse, Controller (the controller if one is connected at first launch) | The device that plays; the other is locked out of play | CoD takes both at once; locking is a project decision |
| Mouse sensitivity | 0.1 to 5 (1.00) | Turn per mouse movement | Mouse Sensitivity |
| Aim sensitivity | 0.3 to 2× (1.00×) | Turn speed aiming down sights, as a share of the above | ADS Sensitivity Multiplier |
| Invert mouse Y | off | | Invert Vertical Look |
| Look speed | 1 to 14 (7) | Right stick turn speed; each step 20% faster, level 6 turning at the base 200° a second across and 150° up and down. Each level shows its band (Very low to Very high) | Horizontal/Vertical Stick Sensitivity (Black Ops Cold War onward, 1 to 14) |
| Aim response curve | Standard, Linear, Dynamic (Standard) | Standard raises deflection to the curve strength; Linear turns in step with the stick; Dynamic is an S-curve (smoothstep), fine at first and quick through the middle (`look::StickResponse`) | Aim Response Curve Type |
| Curve strength | 1 to 3 (2.00) | The Standard curve's exponent | our own extra |
| Right stick deadzone | 2 to 30% (10%) | Radial inner deadzone of the look stick | Right Stick Min |
| Left stick deadzone | 2 to 40% (12%) | The movement actions' deadzone | Left Stick Min |
| Trigger deadzone | 0 to 60% (25%) | How far a trigger pulls before it fires or aims | Left/Right Trigger Deadzone (one value for both here) |
| Invert Y axis | off | The right stick | Invert Vertical Look |
| Aim assist | on | See below | Aim Assist (the types are left out by decision) |
| Vibration | on | Shots, hits, and explosions | Controller Vibration |
| Button prompts | Auto, Xbox, PlayStation (Auto) | Which controller's button images prompts show (see Button prompts in [art-direction.md](art-direction.md)) | Controller button prompt type |

The controller heading shows the connected controller's name, or that none
is connected.

### Input device

The device that plays has the gameplay actions; the other device's bindings
are taken off them, so it cannot move, aim, or fire. Menus (`ui_*`) and a few
shared actions (pause, chat, team chat, the controls legend) take both, so
either device can always reach the menu. A short notice at the top of the
screen says when a controller connects or disconnects, and when someone plays
with the locked-out device (`godot/scripts/ui/input_notice.gd`). In play,
prompts follow the device that plays; menu footers follow the device used
last. Locking one device out keeps aim assist with controllers and stops a
bumped mouse or controller from taking over.

### Aim assist

Call of Duty's controller aim assist, with only its parts
(`rust/sim/src/look.rs`, `AimAssistParams::default`). Mouse aiming never gets
it. It works on enemies within 80 m, fading from 55 m.

- **Slowdown.** The stick turns slower with the crosshair on or near an
  enemy's body: 0.65 x at the hip, 0.5 x aimed. The area round the body is
  four times its radius, and at least 2° past it so distant enemies get some.
- **Rotational.** While the player moves or looks, the view follows 45% of
  the enemy's motion relative to the camera, toward the upper chest, so
  strafing past an enemy, or an enemy strafing, keeps the crosshair with them.
- **No snap.** Starting to aim never pulls the view onto an enemy.
- The assist stays on one enemy until another is 40% nearer the crosshair.

Targets are marked by an `AimAssistTarget` node on each body
(`rust/game/src/aim_assist.rs`). There is one tuning; the setting only turns
aim assist on or off. Call of Duty's assist types (Precision, Focusing, and
so on) are left out by decision.

## Keybinds and controls

The second tab follows Input device, as Call of Duty splits its Keyboard &
Mouse and Controller tabs.

With keyboard and mouse it is **Keybinds**: every gameplay action can be
rebound; choose a binding, then press a key or mouse button; Esc or B
cancels. A key already used by another action swaps with it. Each row also
shows the action's controller button under the current controller layout.
The defaults are in `godot/project.godot` and the list in
`Settings.REBINDABLE`.

With a controller it is **Button layout**: Call of Duty's controller presets
instead of free remapping, over a read-only map of every button (what it
does, its name, its glyph) that updates as the presets change. Gameplay
prompts and the controls legend follow the layout, because they read the
input map. The presets are in `godot/scripts/controller_layouts.gd`; they move
only the face buttons, bumpers, triggers, and stick clicks, as Call of Duty's
do; the D-pad, View, and Menu stay put.

| Setting | Options (default) | What it does | Call of Duty |
| --- | --- | --- | --- |
| Button layout | Default, Tactical, Bumper Jumper, Bumper Jumper Tactical, Lefty (Default) | Tactical: crouch on R3, melee on B. Bumper Jumper: jump on LB, tactical on A. Bumper Jumper Tactical: both. Lefty: fire and aim, lethal and tactical trade sides | Button Layout Preset (Black Ops 2 onward, Modern Warfare 2019 onward) |
| Stick layout | Default, Southpaw, Legacy, Legacy Southpaw (Default) | Southpaw swaps the sticks. Legacy: left stick forward, back, and turn; right stick strafe and look up and down. Legacy Southpaw mirrors it. The Southpaw layouts swap the stick clicks too, so sprint stays on the walking stick | Stick Layout Preset |
| Flip triggers and bumpers | off | Each bumper swaps with the trigger on its side, on any layout | "Flipped" layouts (Black Ops), Flip L1/L2 with R1/R2 (Modern Warfare) |

Call of Duty also ships Stick and Move, Charlie, Nomad, Brawler, and
One-Hand Gunslinger presets. They are left out until their exact maps are
confirmed from the games' own menus; published sources disagree on them. The
trigger deadzone applies to whichever actions the layout puts on a trigger.

The table below is the default layout.

| Action | Keyboard and mouse | Controller |
| --- | --- | --- |
| Move | W A S D | Left stick |
| Look | Mouse | Right stick |
| Jump (and mantle, vault) | Space | A |
| Sprint (a toggle; hold breath while scoped) | Shift | L3 |
| Crouch / slide (a toggle; hold for prone) | C | B |
| Prone | Ctrl | (hold B) |
| Fire | Left mouse | RT |
| Aim | Right mouse | LT |
| Reload | R | X |
| Interact | F | X (shares reload) |
| Melee (and mount while aimed at cover) | V | R3 |
| Lethal (frag) | G | RB |
| Tactical (flashbang or smoke) | Q | LB |
| Switch weapon | Mouse wheel, 1 (primary), 2 (secondary) | Y |
| Use scorestreak | 4 | D-pad right |
| Next scorestreak | 5 | D-pad left |
| Inspect weapon | I | D-pad down |
| Loadout (class menu) | L | D-pad up |
| Scoreboard | Tab | Back |
| Pause | Esc | Start |
| Chat, team chat | Enter, Y | |
| Show controls | F1 | |
| Refill ammo (Testing Area) | T | |

The movement and weapon rules behind each are in [movement.md](movement.md)
and [weapons.md](weapons.md).

## Graphics

| Setting | Values (default) | What it does | Call of Duty |
| --- | --- | --- | --- |
| Field of view | 80 to 120° (102°) | Horizontal field of view at 16:9; the weapon keeps its own, so it never stretches | Field of View |
| Fullscreen | off | Fill the screen instead of a window | Display Mode |
| VSync | on | Match frames to the display | VSync |
| Frame rate limit | Unlimited, 60, 120, 144, 165, 240 (Unlimited) | Cap the frame rate | Custom Frame Rate Limit |
| Graphics preset | Low, Medium, High (Medium) | Shadows, lighting, and effects in one choice; picking one also sets the render scale (77% on Low, native above) | Quality Presets |
| Render scale | 50, 59, 67, 77, 85%, Native | Draws the 3D world at that share of the window and upscales it (FSR 1 on Low, FSR 2 above); the HUD stays sharp | Render Resolution and upscaling |
| Motion blur | off | Blurs fast camera turns | World and Weapon Motion Blur |
| Lens effects | on | Vignette and a little film grain | Film Grain |

What each preset changes, and the frame budget it is held to, is in
[performance.md](performance.md) (`godot/scripts/graphics.gd`).

## Audio

| Setting | Values (default) | What it does | Call of Duty |
| --- | --- | --- | --- |
| Master, Weapons, Effects, Footsteps, Ambience, Interface | 0 to 100% (100, 80, 80, 90, 60, 70) | Bus volumes | Master, Music, Dialogue, Effects volumes |
| Audio mix | Headphones, Speakers, Competitive, Night (Headphones) | Panning width, bus offsets, and Master compression per mix (`Settings.AUDIO_MIXES`). Speakers narrows the stereo; Competitive widens it, brings footsteps up 4 dB, turns ambience down 9 dB and weapons 3 dB, and compresses; Night turns gunfire and explosions down 6 dB and compresses everything, for playing quietly | Audio Mix presets (Home Theater, Headphones, Night Mode, ...) |
| Subtitles | off | Caption lines bottom centre for calls and sound cues (`godot/scripts/ui/captions.gd`) | Subtitles |

There are no voice lines, so subtitles caption the audio cues that no banner
already shows: the 30-second time warning (`MatchStings.CAPTIONS`). A voice
line captions itself with `Settings.caption(speaker, text)`; the line shows
only while Subtitles is on. The buses are in [audio.md](audio.md).

![A subtitle line above the controls legend](media/subtitles.jpg)

## General

| Setting | Values (default) | What it does | Call of Duty |
| --- | --- | --- | --- |
| Player name | text (Player) | The name in the kill feed and on the scoreboard | Display name |
| Camera motion | on | Head bob and strafe tilt while moving | Camera Movement |
| Show FPS counter | off | Frame rate, top right | Telemetry, FPS Counter (off by default) |
| Team colours | Default, Protanopia, Deuteranopia, Tritanopia (Default) | Your side's and the enemy's colour everywhere they appear: score block, kill feed, scoreboard, minimap, objective markers, mode HUDs, chat (`godot/scripts/ui/team_colours.gd`) | Color Customization, HUD Color Palette (protanopia, deuteranopia, tritanopia) |
| HUD size | 80 to 125% (100%) | Scales the corner pieces (health, ammo, score block, minimap, kill feed, streaks, points, prompts) about the screen corner each is anchored to, margins included (`Settings.fit_hud`) | HUD size / Magnified HUD preset (Black Ops 6) |
| HUD opacity | 30 to 100% (100%) | Fades the same pieces; the crosshair, hit markers, and damage indicators keep theirs | Black Ops 6 has crosshair and minimap opacity; one slider here |
| Skip killcams | off | Respawn without watching the replay of each death; the kill that ends a round or the match still plays for everyone | Skip Killcam (a button press in Call of Duty) |
| Rotate minimap | on | The minimap turns with the player, who always faces up; off keeps north at the top | Minimap Shape and Rotation |

The palettes: Default is blue against red. Protanopia and deuteranopia keep a
blue and move the enemy to an orange from Okabe and Ito's colour-blind-safe
set (a lighter orange for protanopia, which sees red light as dim).
Tritanopia, which confuses blue with green, uses teal against a pinkish red.
Black Ops 6 also lets players pick each colour by hue and adds a world colour
filter; neither is offered.

![Default, protanopia, deuteranopia, and tritanopia palettes on the Domination HUD](media/team-colour-palettes.jpg)

![HUD size at 80% and 125%](media/hud-size.jpg)

A new HUD piece joins the size and opacity settings with
`Settings.fit_hud(piece)` once it is in the tree. Pass `own_fade = true` if
the piece animates its own `modulate`. A full-screen piece that draws its
corners itself must read the screen size from `get_viewport_rect()`, not its
own `size`, when it sizes things.
