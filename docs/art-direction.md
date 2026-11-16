# Art direction

How the game should look, and how the characters (other players, range dummies, and the player's own arms) are built.

![Both teams in several poses, the F4 hitbox overlay, and close-ups of an enemy and an ally](media/characters.jpg)

## The look

The game uses clean, low-poly modern military art: flat colours, hard edges, and no photographic textures. This matches the Quaternius guns the arsenal already uses. A small team can make this style look finished, and it renders fast.

Rules for every character and prop:

- **Read the team at a glance.** The two teams differ in value first: allies wear light tan and olive, enemies wear dark charcoal. They differ in hue second: allies have a blue accent and enemies a red-orange accent, on the helmet band and the armbands. Value contrast survives colour blindness, fog, and distance; the accent confirms it up close.
- **Make the head obvious.** A helmet with a coloured band, dark goggles, and a face mask give the head a shape and colour that differ from the torso, so headshots are aimable at range.
- **Keep shapes simple.** Gear is a few dozen faceted triangles each: a helmet dome, a plate carrier with three magazine pouches and a radio, a belt with side pouches, knee pads, and armbands. Avoid noisy camouflage patterns that break up the silhouette.
- **Keep accents rare.** The team colour appears only on the helmet band and armbands, with a little self-light so it reads in shade. Everything else is muted.

### Palette

Each surface of the model is named after a palette role. `rust/game/src/character/palette.rs` maps each role to a colour per team.

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
| Materials | One per palette role, shared by every character of a team | 11 roles |
| Draw calls per character | 12 | 11 surfaces plus the weapon |
| Textures | None | None |

Godot generates LODs for the meshes at import, so distant soldiers cost less.

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
