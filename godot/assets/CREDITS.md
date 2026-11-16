# Asset credits

Gameplay assets below are CC0 (public domain). No attribution is required, but we list them anyway.

| Asset | Files | Source | License |
| --- | --- | --- | --- |
| Ultimate Gun Pack by Quaternius (`AssaultRifle2_1`, `Pistol_1`, `AssaultRifle_2`, `SubmachineGun_2`, `Shotgun_2`, `SniperRifle_1`) | `weapons/m4/m4.fbx`, `weapons/pistol/pistol.fbx`, `weapons/ak47/ak47.fbx`, `weapons/pdw9/pdw9.fbx`, `weapons/m870/m870.fbx`, `weapons/m40/m40.fbx` | [OpenGameArt](https://opengameart.org/content/low-poly-guns-pack) | CC0 |
| The Free Firearm Sound Library by bart (AR-15 `D_24P`/`D_32P`, 1911 `A_34P`/`A_42P`, AK-47 `C_27P`–`C_36P`, Carl Gustav M45 `G_20P`–`G_35P`, Mossberg `N_26P`/`N_30P`, Tikka `W_24P`/`W_29P`; split into snap, body, and tail layers by `tools/build_weapon_audio.py`) | `audio/weapons/<weapon>/{snap,body,tail}_N`, `audio/weapons/ak47_fire.ogg` (the sentry) | [OpenGameArt](https://opengameart.org/content/the-free-firearm-sound-library) | CC0 |
| Gun Reload Sounds by SpringySpringo | `audio/weapons/m4_reload.ogg`, `audio/weapons/pistol_reload.ogg` | [OpenGameArt](https://opengameart.org/content/gun-reload-sounds) | CC0 |
| Gun Reload Sound Effects by BMacZero (`clipload1`) | `audio/weapons/dry_fire.ogg` | [OpenGameArt](https://opengameart.org/content/gun-reload-sound-effects) | CC0 |
| Impact Sounds by Kenney (`footstep_concrete/wood/grass/snow`, `impactMining`, `impactMetal_light`, `impactWood_light`, `impactSoft_medium/heavy`, `impactPunch_medium`; trimmed, mono, loudness-normalised) | `audio/footsteps/concrete_1..4.ogg`, `audio/footsteps/wood_1..4.ogg`, `audio/footsteps/dirt_1..4.ogg`, `audio/impacts/concrete_1..3.ogg`, `audio/impacts/metal_1..3.ogg`, `audio/impacts/wood_1..3.ogg`, `audio/impacts/dirt_1..3.ogg`, `audio/impacts/flesh_1..3.ogg`, `audio/player/land.ogg`, `audio/player/slide.ogg` (snow crunch layer) | [Kenney](https://kenney.nl/assets/impact-sounds) | CC0 |
| Interface Sounds by Kenney (`tick_004`, `confirmation_001`) | `audio/impacts/hitmarker.ogg`, `audio/impacts/kill.ogg` | [Kenney](https://kenney.nl/assets/interface-sounds) | CC0 |
| UI Audio by Kenney (`click1`, `rollover2`) | `audio/ui/click.ogg`, `audio/ui/hover.ogg` | [Kenney](https://kenney.nl/assets/ui-audio) | CC0 |
| RPG Audio by Kenney (`cloth1`, `metalClick`) | `audio/player/slide.ogg` (cloth layer, slowed), `audio/player/low_ammo.ogg` | [Kenney](https://kenney.nl/assets/rpg-audio) | CC0 |
| Metal footsteps on concrete by Thimras (`metal_steps_01`, `13`, `14`, `15`) | `audio/footsteps/metal_1..4.ogg` | [OpenGameArt](https://opengameart.org/content/metal-footsteps-on-concrete) | CC0 |
| Metal Impact Sounds by BMacZero (`bing1`, `clink1`, `clink2`, `clink3`) | `audio/impacts/headshot.ogg`, `audio/player/shell_1..3.ogg` | [OpenGameArt](https://opengameart.org/content/metal-impact-sounds) | CC0 |
| Hurt Sound Effects by EZduzziteh (`hurt_01`, `hurt_04`) | `audio/player/hurt_1.ogg`, `audio/player/hurt_2.ogg` | [OpenGameArt](https://opengameart.org/content/hurt-sound-effects) | CC0 |
| Equipment Clicks III by LFA (bolt-action rifle segment, 2.03 s to 3.08 s) | `audio/player/bolt_cycle.ogg` | [OpenGameArt](https://opengameart.org/content/equipment-clicks-iii) | CC0 |
| Shotgun Reload Sound Effects by zer0_sol (`Rack`) | `audio/player/shotgun_pump.ogg` | [OpenGameArt](https://opengameart.org/content/shotgun-reload-sound-effects) | CC0 |
| Medal chime, synthesized for this project with ffmpeg (two decaying sine tones, E6 then B6, with a quiet E7 shimmer; command in the commit that added it) | `audio/ui/medal.ogg` | Original | CC0 |
| Universal Animation Library and Universal Animation Library 2 by Quaternius (Standard): the mannequin, its rig, and the animations listed in `tools/build_character_assets.py`. The soldier's uniform regions, gear, and first-person arms are built from the mannequin by that tool | `characters/soldier/soldier.glb`, `characters/animations/ual1.glb`, `characters/animations/ual2.glb` | [OpenGameArt](https://opengameart.org/content/universal-animation-library), [OpenGameArt](https://opengameart.org/content/universal-animation-library-2) | CC0 |

## Zombies menu reference assets

`zombies/` contains the globe texture, background/flare images, five map signposts,
the hover sound, and quiet looping `audio/main-theme.mp3` music from the user-supplied
[JedBorseth/bo2-menu asset collection](https://github.com/JedBorseth/bo2-menu/tree/main/public/assets),
revision `1f4b968c47238dfaefb45f36743352e05a24d1cc`.
`agency_bold.ttf` was converted losslessly from that collection's `fonts/AGENCYB.woff`
to a Godot-supported TrueType container. No new imagery was needed.

These reference assets are not covered by the CC0 statement above. The source
repository supplies no asset license; the original game imagery/audio and font
retain their respective owners' rights. The globe shader and menu code are local
implementations.
