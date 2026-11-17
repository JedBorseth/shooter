# Asset credits

Gameplay assets below are CC0 (public domain), except the Barlow and Chakra Petch fonts, which are under the SIL Open Font License 1.1. No attribution is required, but we list them anyway.

| Asset | Files | Source | License |
| --- | --- | --- | --- |
| Ultimate Gun Pack by Quaternius (`AssaultRifle2_1`, `SubmachineGun_2`, `Shotgun_2`, `SniperRifle_1`, `AssaultRifle_5`, `Bullpup_3`, and the accessories `Silencer_1` and `Grip`) | `weapons/m4/m4.fbx`, `weapons/pdw9/pdw9.fbx`, `weapons/m870/m870.fbx`, `weapons/m40/m40.fbx`, `weapons/lsw6/lsw6.fbx`, `weapons/br3/br3.fbx`, `weapons/suppressor/suppressor.fbx`, `weapons/foregrip/foregrip.fbx` | [OpenGameArt](https://opengameart.org/content/low-poly-guns-pack) | CC0 |
| The same guns and accessories rebuilt by `tools/build_weapons.py` (smoothed, unwrapped, baked PBR textures made for this project) | `weapons/<key>/<key>.glb`, `weapons/<key>/<key>_{albedo,normal,orm}.png`, `weapons/<key>/<key>.tres` | Derived from the gun pack above | CC0 |
| Free Classic Weapons Pack v1.1 by Stein Games (`AK47`, `1911`), fetched and pinned by `tools/fetch_weapon_sources.py`, then split into moving parts, scaled to the real weapons, and given the game's sight lines and texture layout by `tools/build_weapons.py` | `weapons/ak47/ak47.glb`, `weapons/ak47/ak47_{albedo,normal,orm}.png`, `weapons/pistol/pistol.glb`, `weapons/pistol/pistol_{albedo,normal,orm}.png` | [itch.io](https://stein-indie.itch.io/classic-weapons-pack) | CC0 |
| Cartridge texture and tritium glow, made for this project by `tools/build_weapons.py` | `weapons/shared/*` | Original | CC0 |
| The Free Firearm Sound Library by bart (AR-15 `D_24P`/`D_32P`, 1911 `A_34P`/`A_42P`, AK-47 `C_27P`–`C_36P`, Carl Gustav M45 `G_20P`–`G_35P`, Mossberg `N_26P`/`N_30P`, Tikka T3 `W_29P` near and `W_24P` down-range and Springfield 1917 `B_24P` near and `B_16P` down-range, blended for the M40; split into snap, body, and tail layers by `tools/build_weapon_audio.py`) | `audio/weapons/<weapon>/{snap,body,tail}_N`, `audio/weapons/m40_fire.ogg`, `audio/weapons/ak47_fire.ogg` (the sentry) | [OpenGameArt](https://opengameart.org/content/the-free-firearm-sound-library) | CC0 |
| The Free Firearm Sound Library by bart, raw master tracks (mechanical foley: AR-15 `D_2`–`D_16`, AK-47 `C_7`–`C_14`, 1911 `A_2`–`A_17`, Walther PPQ `X_4`–`X_19`, Carl Gustav M45 `G_2`–`G_10`, PPSh `P_0`–`P_8`, Benelli Nova `O_0`–`O_8`, Charles Daly `H_4`–`H_7`, Winchester Model 12 `K_0`–`K_13`, Tikka T3 `W_2`–`W_13`, Savage 10 `T_1`–`T_6`, Mosin `M_2`–`M_7`, Mossberg 190 `N_10`/`N_11`, SKS `U_3`–`U_5`; cut, aligned on their hits, pitched and coloured per weapon by `tools/build_weapon_audio.py foley`) | `audio/weapons/<weapon>/foley/*` | [Internet Archive](https://archive.org/details/the-free-firearm-sound-effects-library-by-still-north-media) | CC0 |
| Synthesized for this project (cloth, kit rattle, held breath; `tools/build_weapon_audio.py foley`) | `audio/foley/*`, `audio/weapons/laser/foley/*` | This repository | CC0 |
| Synthesized for this project (bullet cracks, whizzes, ricochets, flesh hits, and the debris, ring, and reflection layers of impact takes 4-8, over the Kenney takes 1-3; `tools/build_world_audio.py bullets`) | `audio/bullets/*`, `audio/impacts/*_4..8.ogg`, `audio/impacts/{tile,grass,gravel,carpet,flesh}_*.ogg` | This repository | CC0 |
| Impact Sounds by Kenney (`footstep_concrete/wood/grass/snow`, `impactMining`, `impactMetal_light`, `impactWood_light`, `impactSoft_medium/heavy`, `impactPunch_medium`; trimmed, mono, loudness-normalised) | `audio/footsteps/concrete_1..4.ogg`, `audio/footsteps/wood_1..4.ogg`, `audio/footsteps/dirt_1..4.ogg`, `audio/impacts/concrete_1..3.ogg`, `audio/impacts/metal_1..3.ogg`, `audio/impacts/wood_1..3.ogg`, `audio/impacts/dirt_1..3.ogg`, `audio/player/land.ogg`, `audio/player/slide.ogg` (snow crunch layer) | [Kenney](https://kenney.nl/assets/impact-sounds) | CC0 |
| Interface Sounds by Kenney (`tick_004`, `confirmation_001`) | `audio/impacts/hitmarker.ogg`, `audio/impacts/kill.ogg` | [Kenney](https://kenney.nl/assets/interface-sounds) | CC0 |
| UI Audio by Kenney (`click1`, `rollover2`) | `audio/ui/click.ogg`, `audio/ui/hover.ogg` | [Kenney](https://kenney.nl/assets/ui-audio) | CC0 |
| RPG Audio by Kenney (`cloth1`, `metalClick`) | `audio/player/slide.ogg` (cloth layer, slowed) | [Kenney](https://kenney.nl/assets/rpg-audio) | CC0 |
| Impact Sounds by Kenney (`footstep_grass_000`–`004`, `footstep_carpet_000`, `001`, `003`, `footstep_concrete_001`, `002` resampled and brightened, `impactGlass_light_000`–`002`; trimmed, mono, matched in loudness by `tools/build_world_audio.py`) | `audio/footsteps/grass_1..5.ogg`, `audio/footsteps/carpet_1..3.ogg`, `audio/footsteps/tile_3..4.ogg`, `audio/impacts/glass_1..3.ogg` | [Kenney](https://kenney.nl/assets/impact-sounds) | CC0 |
| Different steps on wood, stone, leaves, gravel and mud by TinyWorlds (`gravel`, `stone01`; resampled and tilted into variations by `tools/build_world_audio.py`) | `audio/footsteps/gravel_1..4.ogg`, `audio/footsteps/tile_1..2.ogg` | [OpenGameArt](https://opengameart.org/content/different-steps-on-wood-stone-leaves-gravel-and-mud) | CC0 |
| Ambience loops, synthesized for this project by `tools/build_world_audio.py` (wind, birds, traffic, crickets, room tone, an air conditioner, a lamp's buzz) | `audio/ambience/{day,city,night,interior,hum,buzz}.ogg` | Original | CC0 |
| Metal footsteps on concrete by Thimras (`metal_steps_01`, `13`, `14`, `15`) | `audio/footsteps/metal_1..4.ogg` | [OpenGameArt](https://opengameart.org/content/metal-footsteps-on-concrete) | CC0 |
| Metal Impact Sounds by BMacZero (`bing1`, `clink1`, `clink2`, `clink3`) | `audio/impacts/headshot.ogg`, `audio/player/shell_1..3.ogg` | [OpenGameArt](https://opengameart.org/content/metal-impact-sounds) | CC0 |
| Hurt Sound Effects by EZduzziteh (`hurt_01`, `hurt_04`) | `audio/player/hurt_1.ogg`, `audio/player/hurt_2.ogg` | [OpenGameArt](https://opengameart.org/content/hurt-sound-effects) | CC0 |
| Barlow Condensed (Bold, ExtraBold) and Barlow (Medium, SemiBold) by The Barlow Project Authors: sentences, the score feed, and medals | `fonts/Barlow*.ttf` (license in `fonts/OFL.txt`) | [Google Fonts](https://github.com/google/fonts/tree/main/ofl) | OFL-1.1 |
| Chakra Petch (Medium, SemiBold, Bold) by The Chakra Petch Project Authors: titles, buttons, labels, and HUD numerals | `fonts/ChakraPetch-*.ttf` (license in `fonts/OFL-ChakraPetch.txt`) | [Google Fonts](https://github.com/google/fonts/tree/main/ofl/chakrapetch) | OFL-1.1 |
| Prism Laser sounds, synthesized for this project by `tools/make_laser_sounds.py` (frequency sweeps and decaying partials, standard library only) | `audio/weapons/laser/{snap,body,tail}_1..3.wav`, `audio/player/phase.wav` | Original | CC0 |
| Medal chime, synthesized for this project with ffmpeg (two decaying sine tones, E6 then B6, with a quiet E7 shimmer; command in the commit that added it) | `audio/ui/medal.ogg` | Original | CC0 |
| PBR texture sets from ambientCG by Lennart Demes: `Road012A`, `Bricks051`, `Bricks060`, `Fabric061`, `Concrete047A`, `Concrete031`, `Concrete034`, `Concrete036`, `CorrugatedSteel005`, `CorrugatedSteel006C`, `Ground103`, `Metal046A`, `Planks023A`, `Plaster001`, `PaintedMetal006`, `DiamondPlate007D` (1K, repacked by `tools/build_textures.py`) | `textures/<set>/{albedo,normal,orm}.jpg` | [ambientCG](https://ambientcg.com) | CC0 |
| Sky images (HDRIs) from Poly Haven: `kloofendal_48d_partly_cloudy_puresky` by Greg Zaal and Jarod Guest, `qwantani_late_afternoon_puresky` and `qwantani_moon_noon_puresky` by Greg Zaal and Jarod Guest (2K, downloaded by `tools/build_skies.py`) | `skies/range_day.hdr`, `skies/slamcity_sunset.hdr`, `skies/zombies_night.hdr` | [Poly Haven](https://polyhaven.com/hdris) | CC0 |
| Prop models from Poly Haven (1K glTF, decimated and re-centred by `tools/build_props.py`): `concrete_road_barrier_02` by Amal Kumar; `Barrel_01` by Jorge Camacho; `Barrel_02` by Jorge Camacho; `barrel_03` by Serhii Khromov; `wooden_crate_02` by James Ray Cock and Jurita Burger; `wooden_military_crate` by Prabhjinder Singh; `cardboard_box_01` by Rahul Chaudhary; `old_tyre` by MP; `metal_trash_can` by GurJas Studios; `exterior_aircon_unit` by Monsta3D; `fire_hydrant` by Gonçalo Felício; `rollershutter_door` by MP; `street_lamp_01` by Josh Dean; `utility_box_01` by James Ray Cock; `covered_car` by MP; `security_light` by Maximilian Schuster | `props/<name>/` | [Poly Haven](https://polyhaven.com/models) | CC0 |
| Decals from ambientCG by Lennart Demes: `RoadLines020C`, `RoadLines028C`, `RoadLines023C`, `AsphaltDamage001`, `ManholeCover010`, `Leaking016B` (packed by `tools/build_decals.py`). The bullet hole is generated by the same tool for this project | `textures/decals/` | [ambientCG](https://ambientcg.com) | CC0 |
| Zombies Sound Pack by artisticdude (all 24 clips, sorted into groans, attacks, and deaths, trimmed and normalised by `tools/build_zombie_audio.py`) | `audio/zombies/{groan,attack,death}_N.ogg` | [OpenGameArt](https://opengameart.org/content/zombies-sound-pack) | CC0 |
| Zombies round stings, synthesized for this project by `tools/build_zombie_audio.py` (a detuned drone and inharmonic bells) | `audio/zombies/round_start.ogg`, `audio/zombies/round_end.ogg` | Original | CC0 |
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

## Nuketown map assets

`maps/nuketown/generated/` contains the runtime geometry, model placements, and
converted textures for Nuketown 2025 (`mp_nuketown_2020`), extracted from the
user-supplied Call of Duty: Black Ops II installation using C2Mv3 v3.0.5.
These original game assets are not CC0 and retain their respective owners' rights;
no separate redistribution license was supplied with the export. The converter
and Godot scene integration are local implementations. See
[the export and rebuild notes](../../docs/assets/nuketown.md) for provenance.
