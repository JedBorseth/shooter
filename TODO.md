# TODO

The open work for building this into a modern-military, Call-of-Duty-style multiplayer FPS. The phases, decisions, and reference projects are in [docs/roadmap.md](docs/roadmap.md).

## How to pick up a task

1. Pick an unchecked task. Tasks marked **good first task** need little context.
2. Open an issue or draft PR that names the task, so two people don't take the same one.
3. Follow the conventions below. Run the checks before you ask for review:

   ```sh
   cd rust && cargo fmt --all && cargo clippy --all-targets && cargo test
   cd .. && godot --headless --path godot --script res://tests/smoke_test.gd
   uvx --from gdtoolkit==4.5.0 gdformat godot/scripts godot/tests
   uvx --from gdtoolkit==4.5.0 gdlint godot/scripts godot/tests
   ```

   CI runs the same checks on every pull request.

4. For visual changes, attach screenshots from `godot/tests/capture_screens.gd` (see the README).
5. In the PR that finishes a task, tick its box here.

### Conventions

- **Rust by default.** Gameplay rules, simulation, and netcode go in Rust. Engine-independent logic goes in `rust/sim` (no Godot types, deterministic, unit-tested). Godot-facing classes go in `rust/game`.
- **GDScript where it is honestly simpler:** UI, HUD, small effect scripts, editor tools, and test harnesses.
- **Determinism.** Anything the server and client both simulate must produce the same result from the same inputs. Use `shooter_sim::rng` (hashed from shared counters), never a stateful random generator, wall-clock time, or frame delta inside `rust/sim`. Use `shooter_sim::fmath` for `sin`, `cos`, `exp`, `powf`, and `atan2`, never the `f32` methods: platform math libraries differ in the last bits, so a Windows client would drift from a Linux server.
- **Dependencies** are welcome when they save real work. They must be open source and self-hostable: no service that needs someone else's API key or account. Say why you added one in the PR.
- **Assets** must be CC0 or another license that allows commercial use and modification. Credit every asset in [godot/assets/CREDITS.md](godot/assets/CREDITS.md).

## Phase 1: single-player feel (in progress)

Done: movement (toggle sprint, toggle crouch, slide, jump), gamepad support with aim assist, six data-driven weapons (M4A1, AK-47, PDW-9, M870, M40, M1911), viewmodel shader and procedural motion, hitscan with headshots, tracers, shell ejection, per-surface impacts and footsteps, weapon racks, health with regeneration, fall damage, death and respawn, damage feedback, pause and settings menu, audio buses, a training sentry, HUD.

### Weapons and gunplay

- [x] First-person arms attached to the weapons: the soldier's arms mesh, with IK onto `GripRight`/`GripLeft` markers on each weapon view (`rust/game/src/character/arms.rs`).
- [x] Moving parts: `tools/build_weapons.py` splits each gun's magazine, pump, bolt, or slide into its own node. Keyframed clips in `rust/game/src/weapon_clips.rs` drive them and the hands, built per weapon class from each weapon's own timings in seconds: reloads tilt the weapon to show its left side, pull the magazine with a flick and let it fall, fetch a second magazine mesh, seat it with a jolt exactly when the simulation counts its rounds (`mag_in`), then slap the bolt release (M4), pull the charging handle (other rifles), drop the slide (pistol), or work the bolt (M40) on an empty reload, and re-shoulder with a small overshoot. Shotguns tilt to the loading port, thumb each shell in, and pump after an empty reload. Draws differ per class (a pistol's wrist flick, a rifle's roll in, a heavy gun's swing, a pump gun canted to show its port, a bolt action's steady rise) and holsters for sidearms and long guns, and the inspect checks the magazine and the chamber. Each beat emits `WeaponView::foley_event` (`mag_out`, `mag_in`, `bolt_release`, `charge`, `shell_in`, `pump_back`, `pump_forward`, `bolt_up`, `bolt_back`, `bolt_forward`, `bolt_down`, `draw`, `holster`, `ads_in`, `ads_out`, `inspect`) for the sounds.
- [x] Keyframed draw and holster animations (a press check on the pistol), and the firing hand working the bolt.
- [x] The action's small parts move in first person: bolt carriers (or an AK's handle) with each shot, an AR's bolt locked open on empty until the release, the dust cover, charging handles on a charge or chamber check, triggers, the pistol's hammer and slide stop, and magazine releases.
- [x] Bullet tracers every Nth round (`tracer_every` in the weapon file), for the local player, other players, and the sentry, from one pool.
- [x] Fired cases from an ejection-port marker on each `WeaponView`, in first and third person, that land in the world and stay (`CombatEffects::throw_case`).
- [x] Bullet penetration through thin cover, with per-material damage reduction (`shooter_sim::penetration` rules, `rust/game/src/penetration.rs` traces).
- [x] Per-surface impact effects and sounds (metal, concrete, wood, dirt, flesh), from `surface` metadata on colliders.
- [x] Data-driven weapon definitions: RON files in `godot/data/weapons/`, loaded at startup and hot-reloaded in debug builds.
- [ ] Detailed weapon models from CC0 source packs (`tools/build_weapons.sh`): every gun done (the LSW-6 derived from the AK-47 with a longer barrel, bipod, and 75-round drum); modelled fired cases (brass, lacquered steel, and shotgun shells), suppressor, and foregrip done; the extended magazine shows; the M4A1's finish is painted from its geometry (`tools/weapon_finish.py`); the red dot and ACOG housings are modelled (`tools/weapon_optics.py`).
- [x] More weapons: AK-47, PDW-9 SMG (with a red dot), M870 pump shotgun (pellets, shell-by-shell reload), M40 bolt-action sniper (scope overlay).
- [ ] Black Ops 2-style roster, one weapon per PR, each held to its class's time-to-kill band (`every_weapon_lands_in_its_class_band`, [docs/weapons.md](docs/weapons.md#class-bands)):
  - [x] Weapon classes and their bands; Create-a-Class grouped by class; bots carry any weapon of their default class's weapon class (`class::bot_class`).
  - [x] CQ-10 SMG (the Skorpion EVO's role: fastest fire, shortest reach), from the Classic Weapons Pack's MAC-11. Its own firing sound is held for listening; it fires and reloads with the PDW-9's until then.
  - [x] BP-9 burst pistol (the B23R's role), from the Classic Weapons Pack's Tec-9 with built-in three-dot sights. Its own firing sound is held for listening; it fires with the M1911's until then.
  - [x] KR-12 battle rifle (the SCAR-H's role: slower, harder-hitting), the AK-47 model with its walnut refinished as black polymer. Its own firing sound is held for listening; it fires with the AK-47's until then.
  - [x] SG-12 semi-automatic shotgun (the S12's role), box-fed, modelled by `tools/weapon_sg12.py` on the M870's receiver. Its own firing sound is held for listening; it fires with the M870's until then.
  - [x] DM-7 marksman rifle (the SVU-AS's role), the M14 with a riflescope and an olive glass-fibre stock. Its own firing sound is held for listening; it fires with the BR-3's until then.
  - [x] RL-4 launcher (the RPG's role): a free-flight rocket that goes off on what it meets, modelled by `tools/weapon_rl4.py`. Its own launch sound is held for listening; it fires with the M870's until then.
  - [x] Rockets bring down scorestreak aircraft (Black Ops 2: one hit downs any): the UAV and Counter-UAV fly a ring over the map that both ends compute, and bullets wear them down ([docs/scorestreaks.md](docs/scorestreaks.md#in-the-sky)).
  - [x] Bots carry launchers (one in four) and fire them at enemy aircraft. They still never turn one on players, as Black Ops 2's do (aimed at the feet).
- [x] LMG (LSW-6: 75 rounds, slow aim and reload) and a burst-fire rifle (BR-3: `FireMode::Burst`, three rounds a pull, one burst kills up close).
- [x] Attachments: red dot, ACOG, suppressor, grip, and extended magazine, as weapon variants (`Attachment::apply`), chosen in the loadout menu, with their parts on the first- and third-person models. The ACOG aims through its own eyepiece, with a lit chevron and the zoomed world inside it, instead of the sniper's overlay.
- [x] Hold-breath sway for scoped weapons.
- [x] Weapon inspect on a key press (I, or D-pad down), its own per class and cancelled by any action: rifles check the magazine and the chamber (a heavy one slower), a pistol press-checks at the front serrations and shows both sides, a pump gun rolls its loading port up and draws the pump a touch, a bolt action checks its magazine and lifts and draws its bolt. Each check lands with a knock (`weapon_clips::inspect_jolts`).
- [x] **good first task** Low-ammo click and a "LOW AMMO" HUD warning.
- [ ] Equipment (`rust/sim/src/equipment.rs`, [docs/weapons.md](docs/weapons.md#equipment)):
  - [x] Frag grenade rules: cook, overhand throw, bounce, roll, a blast that falls off with distance and stops at walls, self-damage, a live drop on death, kill credit, counts per mode, server authority with the throw predicted.
  - [x] First-person pin pull, cook, and throw with the arms; a third-person throw pose; camera shake and ear ring by distance; a danger indicator and a grenade count on the HUD.
  - [ ] Throw sounds (pin, spoon, bounces) and a close explosion sound from the sound pass; the combat effects' explosion in place of the stand-in blast.
  - [x] Flashbang and smoke grenade, picked in the loadout.
  - [x] Players who join after a smoke popped see the cloud (snapshots list the hanging clouds).
  - [x] Bots blinded by flashbangs and unable to see through smoke (`equipment::smoke_blocks`).
  - [x] Bots throw grenades and run from live ones: frags at bunched or hiding enemies (cooked on Normal and Hard), flashbangs at a hiding enemy, smoke over a retreat to cover (`shooter_sim::bot::plan_throw`, `rust/game/src/net/bots.rs`).
  - [ ] Bots bank grenades off walls on purpose (today a bounce counts only when the checked flight happens to take one), and pop smoke to cover a plant or defuse in Search and Destroy.
- [x] Melee: a knife on its own button (V, R3) with a lunge at enemies within about 3 m, predicted and lag-compensated (`rust/sim/src/melee.rs`), swung in the support hand in first and third person (the knife modelled in Blender, `tools/build_knife.py`), knife kills in the feed, 130-point knife kills in Zombies, and bots that knife up close.
  - [x] Knife kills crumple where they stand, from the strike (the client drops the body on its own strike; the server confirms).

### Movement

- [x] Mantling: jump at a ledge 0.5 to 1.35 m high while moving toward it to climb onto or over it (the engine finds the ledge, `movement::step` climbs; predicted like any move).
- [x] Vaults over thin cover and through windows that keep the run's speed, mantles by holding jump in the air, and crouch-height mantles that end crouched ([docs/movement.md](docs/movement.md)). Other players vault with a vault of their own (a hand planted on the top, legs swung over), not the mantle's climb, and their hit zones follow it.
- [x] Ladders: grab, climb at 2 m/s, step off at the top or bottom, slide down with crouch, jump off, weapon lowered (`rust/sim/src/movement/ladder.rs`, `LadderVolume` markup in `rust/game/src/ladder.rs`). The Testing Area's tower ladder is climbable.
- [x] Ladders: first-person hands on the rungs as the weapon goes away, closed round each rung ([docs/movement.md](docs/movement.md#ladders)).
- [ ] Ladders on Slamcity and Zombies once their maps want them.
- [x] Mounting the weapon on cover (aim and melee, as Modern Warfare 2019): ledges, barriers, sandbag walls, and windowsills from 0.82 m to 1.6 m; the eye rests the gun on the edge, kick and sway shrink, the view stops at a cone ([docs/movement.md](docs/movement.md)). The Testing Area's 1.2 m barriers and 0.84 m sandbags mount.
- [x] Mounting on a wall's side (the tall block walls' ends) and at prone height (the culvert's 0.44 m sandbags): prone, the body props up onto low cover; beside a wall's edge, the gun rests at its height ([docs/movement.md](docs/movement.md#weapon-mount)).
- [x] Third-person mantle animation (UAL2's `ClimbUp_1m_RM`, played once as a mantle starts).
- [x] Prone (Black Ops 2's third stance): the prone button or a held crouch, 0.7 s down through a crouch, a 0.8 m/s crawl, eye 0.39 m, legs checked behind, weapon lowered crawling at the hip, steadier aim, pitch limits; an authored prone pose and low crawl for third person, hit zones measured from them ([docs/movement.md](docs/movement.md#prone)).
- [x] Directional crawls: forward, backward, and to each side, blended by the travel's direction, with the hit zones measured every 45° ([docs/movement.md](docs/movement.md#prone)).
- [x] Lying along slopes: the server measures the ground under the chest and knees and sends the slope with the body; the drawn body and its hit zones lie along it ([docs/movement.md](docs/movement.md#prone)).
  - [ ] The killcam's own recorded body (`killcam::OwnRecorder`, the local player's body on its tape) still lies flat: it is built from the predicted state, which has no slope; it would need the client's own two rays.
- [x] Turning prone stops before the legs sweep into a wall, and a first-person crawl: the weapon rolls on its side and rocks with each pull while the support hand drags the body along ([docs/movement.md](docs/movement.md#prone)).
- [ ] Prone follow-ups: the turn stop is the client's (the server takes the view it is sent, so a modified client could still sweep its own legs into a wall); bots turn prone without the check. The body turns about the eyes, as before; the IW engine's turns about the body's middle.
- [x] Footstep sounds, with surface types and a cadence tied to speed.
- [x] Fall damage and a landing camera dip scaled by fall speed.
- [x] Slide camera tilt and slide sound.
- [x] Slide-cancel: jump (or crouch again) during a slide to stand up with its speed (up to a sprint) and sprint on, without leaping; the next press jumps. Under a low ceiling the jump waits for headroom, and at a ledge it still mantles. Both start the slide cooldown.
- [x] **good first task** Head bob and strafe tilt for the camera (small, with a settings toggle).
- [x] Stance changes take time (0.25 s down, 0.3 s up) on one crouch blend that drives the capsule, the camera, the shot origin, the hit zones, and the third-person body together ([docs/movement.md](docs/movement.md)).
- [x] Slides are earned (0.3 s of sprint) and start at 1.1x sprint scaled by weapon weight; a slide-cancel keeps no more than sprint speed, so chained cancels cannot outrun a sprint.
- [x] Slides after IW8 ([docs/movement.md](docs/movement.md#slide)): crouch in the air during a sprint slides on landing, strafing steers a slide while the view looks freely, a 0.5 s low-friction wind-down after it, and sliding off a ledge ends it standing.
- [x] Slope-aware slide friction (faster downhill, slower uphill, as in IW8's `Slide_FrictionScale`): `MoveEnv::floor_normal` from a ray under the feet; `net_test.gd` slides down the Testing Area's ramp online with no corrections ([docs/movement.md](docs/movement.md#slide)).
- [x] Ground inertia (0.23 s to a walk, 0.28 s to stop), strafe and backpedal at 0.8x and 0.7x, a 6.9 m/s sprint that builds over 0.25 s and winds down over the weapon's sprint-out time, and slower diagonal sprints.
- [x] Landings cost speed (0.7x on touchdown, 1.2 s to recover) and make the next jump lower, air strafing adds no speed, and a standing jump can be steered about a metre.
- [x] Camera: the walk bob dips just after each footfall and never snaps, landings dip about 5 cm and nod the head, a hit flinches away from the shot, and the death camera leaves first person to face the killer.
- [x] Stepping: 0.45 m steps (IW), step-ups at any angle into a riser, and stairs walked down on the ground at full speed with the camera eased.
- [x] A mantle keeps the direction it started in; the server's player bodies collide like the client's; a respawn resets every camera offset.

### Feel and polish

- [x] Camera recoil punch tuned per weapon (`camera_punch` on `WeaponView`, `ads_punch_scale` on `FpsPlayer`), visual only, separate from the aim recoil in the weapon files.
- [x] Modern weapon rules ([docs/weapons.md](docs/weapons.md)): recoil splits into a climb that stays and a snap that settles, so the sights always show where rounds go and pulling down holds a spray; hashed per-shot camera roll; hip-fire bloom; crouch steadies the kick; aimed idle sway on every weapon; aiming through a reload; reloads that carry on through a sprint and count their rounds when the magazine seats; switches never skip a bolt cycle or an empty-reload charge; per-weapon zone multipliers; class handling times and a time-to-kill table (`rust/sim/src/weapon/ttk.rs`). Look sensitivity follows the aim zoom only, not sprint or slide FOV.
- [x] Weapon motion with weight (`ViewmodelRig` in `rust/game/src/viewmodel.rs`): the weapon lags the player's own turning and cants into it, reacts to starts, stops, strafes, jumps, and landings (about 3.5 cm and 4 degrees on a jump landing), bobs in a figure-8 on the footstep stride shared with the camera, runs a sprint loop, breathes at the hip, follows the sprint-out back onto target exactly when it can fire, swings through a small arc and settles when aiming, and kicks back hard per shot (hashed, about 2 cm aimed, peaking within two frames) while the aimed sights only bounce around the point of aim. The aim zoom eases with the weapon.
- [x] First-person animation set from Call of Duty's weapon files: a first raise at the start of a game or round and on a pickup (with its own time per weapon), a slide pose, a mantle and vault hand plant, eased crouch, walk and strafe poses, and a damage kick on the weapon ([weapons.md](docs/weapons.md#first-person-animation)).
- [x] The newer guns' own reloads and inspects, fitted to their mechanisms: the CQ-10's grip magazine and top knob, the BP-9's forward magazine and side knob, the KR-12's speed reload, the DM-7's locked-open operating rod, the SG-12's slapped box, and the RL-4's rocket in reach ([weapons.md](docs/weapons.md#first-person-animation)).
- [x] Weapon-feel trace: `godot/tests/capture_weapon_feel.gd` records the weapon's pose, the view, and the aim each frame through scripted scenarios; `tools/plot_weapon_feel.py` plots them against reference bands.
- [x] Damage feedback when the player is hit: red vignette, directional indicator, hit sounds.
- [x] Audio buses (Master, Weapons, Effects, UI) with a compressor on Weapons and a limiter on Master.
- [x] Layered gunshots (snap, body, and tail, each picked from several recordings) with no dead air before the transient. Rebuild them with `tools/build_weapon_audio.py`.
- [x] A per-shot mechanical layer (bolt and spring clack) for each gun, and a hollow last-rounds layer for the final few shots in a magazine: synthesized by `tools/build_weapon_audio.py mech` into `mech_*.wav` and `hollow_*.wav`, played by `WeaponView` (`mech_db`, `hollow_db`). The ring covers the last fifth of a magazine (two to five rounds) and builds to the last round. Replace them with recordings when we have good CC0 ones.
- [x] Reverb zones on maps: `ReverbZone` boxes (`rust/game/src/acoustics.rs`) in the testing arena's tunnel and Slamcity's rooms (the tenement hallway and flat, pawn shop, store, diner, laundromat, ticket hall, and train). The reverb follows the listener rather than each source (unlike `Area3D` reverb buses), so it also covers the player's own gunfire. Nuketown has zones for the two houses' ground floors; the Zombies arena has none yet.
- [x] Weapon handling heard beat by beat: real CC0 recordings (magazine out and in, bolt release, charging handle, pump, bolt, shells, dry fire) per weapon, played from `WeaponView`'s `foley_event` signal and timed to land on the animation's contact, plus shared draw, holster, aim, inspect, and pickup layers (`weapon_foley.rs`, `tools/build_weapon_audio.py foley`, `docs/audio.md`). Other players' reloads play the same beats at their position.
- [x] Gunshots from the raw CC0 recordings for every gun (the BR-3 and LSW-6 have their own), near crack over the shooter-position blast and real terrain echo, a synthesized low body, five takes per layer, an indoor tail chosen by where the listener is, slapback off the nearest walls outdoors, and near, mid, and far layers for other players' fire that arrive at the speed of sound (`docs/audio.md`).
- [x] The body and the mix: breathing after sprints and when hurt, the held breath, a heartbeat at low health, being hit (a soft thud; no muffle or ringing ears, so hearing stays clear), a death fall, own jump and mantle sounds, kit and cloth under steps; designed hit, headshot, and kill markers (a headshot kill plays both); loudness priority (close gunfire ducks footsteps and distant fire), a Player bus, and a documented voice budget (`player_sounds.rs`, `docs/audio.md`).
- [x] World one-shots (wind gusts, loose debris), an optional distant battle beyond the map (Testing Area, Zombies), and match stings (start, win, loss, draw, 30-second warning, Search and Destroy rounds, a Hardpoint call of its own); `tools/build_world_audio.py stings`.
- [ ] An announcer, once CC0 voice recordings good enough exist.
- [x] Positional audio for impacts and the sentry's gunfire.
- [x] Other players' rounds traced on the client (`bullets.rs`, `ShotTracer.shot_traced` for effects): near misses crack (rifles) or whiz (pistols, SMGs, buckshot) past the listener and suppress the lens, rounds landing near the listener are heard, metal and concrete ricochet, and every impact plays from a pool of ten voices. Eight impact takes per surface with debris and reflection layers, and designed flesh hits.
- [x] Occlusion for positional audio, including other players' gunfire: a ray from the camera to every playing `AudioStreamPlayer3D`; a sound behind a wall plays 9 dB quieter through a low-pass copy of its bus.
- [x] **good first task** Kill confirmation sound for hit markers.
- [x] Other players' footsteps, jumps, landings, slides, mantles, and reloads, positional, with an audible range per gait (`rust/game/src/footsteps.rs`). Teammates are quieter than enemies, and your own steps are quieter than both.
- [x] Footstep sets for grass, gravel, tile, and carpet. Untagged geometry gets its surface from the hit triangle's material name (`rust/game/src/surfaces.rs`, Jolt's ray face index), so imported maps need no tagging.
- [x] Other players' gunfire uses each weapon's own snap, body, and tail layers. The near layers fall off faster than the echo (`rust/game/src/gunfire.rs`).
- [x] A rear cue: positional sounds behind the listener have their highs shelved down, so front and back sound different.
- [x] Ambience: per-map outdoor beds and an indoor room tone that cross-fade at reverb zones, prop hums and buzzes, and gunfire ducking (`rust/game/src/ambience.rs`, synthesized by `tools/build_world_audio.py`).
- [x] Footsteps and Ambience volume sliders, and an Audio mix option (Headphones, Speakers, Competitive).
- [ ] An HRTF spatializer (for example a Steam Audio GDExtension) to replace stereo panning and the rear shelf, if playtests want more precise elevation and front/back.
- [ ] Footsteps on water and in puddles, and on stairs (a hollower step).
- [ ] Other players' weapon switches and bolt or pump cycles, positional.
- [x] Hit markers pop in and settle; kill markers are larger, thicker, and red.
- [x] Ragdoll deaths (`rust/game/src/character/ragdoll.rs`): a killed soldier or zombie becomes twelve jointed capsules from the pose it was drawn in, pushed at the part the killing shot hit (snapshots carry the hit shape and a force level, `protocol::HitPart`), buckling at the knees and folding forward or sideways by the part hit and the shot's side for its first half second, then limp. At most six move at once (four in Zombies), and a body at rest freezes into its skeleton. Bodies and dropped guns stay in the scene's `CorpseYard` after a respawn (30 s for soldiers, 10 s for zombies, at most ten bodies), then sink away. No red hit flash on characters. Clients only: dedicated servers build none.
- [x] Hit reactions by where the round lands (`SoldierRig::hit_at`): a head hit snaps the head along the shot (about 18°), a body hit tips the torso along it and twists it away from the side hit, an arm hit loosens that hand on the weapon for 0.18 s, and a leg hit buckles the knees (the stance blend dips 0.3 toward a crouch). Harder hits react more. A remote player hit by our own shot flinches once: the snapshot that confirms the hit only updates the push a death would take.
- [x] Turning on the spot: standing still, the legs stay planted while the aim turns up to 60° (the spine takes it), then the feet step round at 360°/s with the walk loop stepping in place. The server's body clock keeps the feet (`gait::Turning`) and sends the offset and the step in snapshots, and the hit zones turn the legs about the feet and blend the step, so they match the drawn body (`hitbox_fit_test`).
- [x] Locomotion transitions and planted feet: a slide drops in with `Slide_Start` and gets up with `Slide_Exit`; a jump pushes off with `Jump_Start` and lands with a knee flex from `Jump_Land`, as strong as the fall was long; the torso leans into starts and back out of stops; and within 15 m of the camera a standing soldier's feet rest on the ground under them (one ray a foot; the hips drop at most 9 cm, and past what the straight leg reaches the toe points down to a lower stair, so the hit zones stay put and the foot still stands on the ground).
- [x] Third-person weapon handling per class (`hitbox::carry::CarryClass`, shared with the arm hit zones): rifles' support hand goes to the magazine, down to the vest pouch, and back up to seat a fresh one when the reload counts its rounds; pistols drop the hand to the belt and back to the grip; pump guns go from pouch to loading port for each shell; each class tilts its own way mid-reload. Pistols are held out in both hands and carried muzzle down in a sprint; LMGs ride low. Switches lower the old weapon before the new one comes up, shots kick as hard as each weapon's recoil with a push to the shoulder, and `SoldierFrame::action` gives planting, defusing, and throwing a pose for when snapshots carry them.
- [x] Zombies: hit zones posed hunched as the shamble is drawn (`Activity::hunch`: the head about 30 cm further forward and 20 cm lower than a soldier's; standing still, lower on bent knees), so headshots land on the drawn head; heavy hits (force 8, about 75 damage) stagger a zombie in place for 0.5 s with `Hit_Knockback`; deaths are ragdolls that fall from wherever the zombie was, mid-air included.
- [x] Ragdoll joints settle about the limbs' natural rest: once a body has toppled, each cone is rebuilt through `PhysicsServer3D::joint_make_cone_twist` with explicit frames from the skeleton's rest pose (arms hanging), narrows to 80% of its swing, and lets the limb slump toward rest, so bodies no longer lie with a limb held at the angle they died in.
- [x] Ragdolls come to rest on their own instead of at the 5 s cap: the ground drag no longer rewrites a slow part's velocity every frame (which kept every body awake), and a settled body whose parts all stay nearly still for a quarter second counts as at rest; `zombie_reactions_test` checks it.
- [x] Directional gait: walk, jog, and crouched-walk cycles in eight directions (crossover strafes, backpedals that reach back toe first, diagonals), authored from the forward cycles with foot-path IK by `tools/build_gait_animations.py`; the hit zones are measured in every direction.
- [x] Blends between two authored gait directions run the stride faster by the blended foot path's loss (`gait::blend_cadence`), so planted feet keep pace with the ground.
- [ ] Behind the strafes (112.5°, 157.5°), the blend of a strafe and a back diagonal plants its feet poorly and slides about a fifth of the stance: author those directions too, or warp the phases so both loops plant together.
- [x] Sprint diagonals: the sprint is authored in its forward diagonals too, and a sprint drifting sideways blends toward them instead of turning the hips of the forward sprint toward the drift; the hit zones are measured on up the sprint within the diagonals, and halfway up it (5 m/s).
- [x] Third-person support hand on the handguard: the palm rests on each weapon's measured handguard or pump section (`section` in its weapon file, from `tests/bake_support_sections.gd`) instead of a few centimetres under it, and the server's arm hit zones move with it; with a foregrip both move down onto the grip.
- [x] Bladed rifle stance in third person: the upper chest turns 15° (support shoulder forward, the head still on the aim; square in a sprint, a throw, and with a pistol), so the support hand reaches up the handguard (2 cm behind the marker instead of 8) and the stock sits 3 cm further out of the chest. The hit zones blade the same way.
- [x] The extended magazine shows on other players' guns (its own stretched `WorldMagazine`, held and through reloads).
- [x] Arm hit zones follow a grenade throw (`hitbox::action`, shared with the drawn third-person throw). The knife swing and planting or defusing still leave them on the weapon: snapshots carry no knife progress yet (only `meleeing`), and nothing plants until Search and Destroy has a bomb.
- [x] Dropped magazines: other players' empty reloads (and every pistol reload) drop the old magazine, which falls, bounces, and lies on the ground for 5 s (pooled, at most six); tactical reloads still stow it in the pouch.
- [x] Third-person magazines: other players' reloads take the magazine out of the gun with the support hand (to the vest pouch and back for rifles and machine guns; a pistol's drops and a fresh one comes from the belt), from `WorldBody` and `WorldMagazine` meshes the weapon build exports.
- [x] The player's own body in the death camera: a ragdoll of our own body falls where we died, and the camera pulls back to watch it before turning to the killer.
- [x] Score popups (+100, headshot and longshot bonuses) and medals at the crosshair.

### Settings and UX

- [x] Pause menu (Esc) with resume, settings, and quit.
- [x] Settings: mouse sensitivity, ADS sensitivity multiplier, FOV, camera motion, volume sliders, window, vsync, and frame cap, saved in `user://settings.cfg`.
- [x] Graphics presets (Low, Medium, High) and a render scale with FSR upscaling, applied to every map (`godot/scripts/graphics.gd`).
- [x] A rendering benchmark with a GPU budget per preset, run before and after every visual change (`tools/benchmark.sh`, `docs/performance.md`).
- [x] Invert Y and keybind remapping (Settings → Key bindings; saved in the `keys` section of `settings.cfg`).
- [x] Gamepad support: bindings, stick deadzones and response curve, turn boost, rumble, and CoD-style aim assist (slowdown, rotational, and target aim assist on zombies; gamepads only, on enemy players and bots too).
- [x] Gamepad settings in the settings menu: look speed, deadzone, curve, invert Y, aim assist toggle, vibration.
- [x] D-pad left and right pick the weapon slots, and the reload button (X) interacts when something is in front of the player. Prompts name the bound key or button.
- [x] An input device setting (Keyboard & mouse or Controller) that locks the other device out of play; menus and pause take both, a reminder shows when the locked device is used, and losing the controller pauses the game.
- [x] Button prompts with controller glyph images instead of button names: Xbox and PlayStation sets (Settings → Controls → Button prompts: Auto, Xbox, PlayStation) and mouse buttons, with menu footers following the device used last (`godot/scripts/ui/glyphs.gd`).
- [x] **good first task** Show the controls overlay only for the first minute, or toggle it with F1.
- [x] Controller tuning as Call of Duty offers it: aim response curve type (Standard, Linear, Dynamic), right stick, left stick, and trigger deadzones (`docs/settings.md`).
- [x] Interface settings: colour-blind team palettes (protanopia, deuteranopia, tritanopia), HUD size, and HUD opacity (`docs/settings.md`).
- [ ] Zombies HUD and the mode views' top-centre strips under HUD size and opacity (they draw world markers on the same layer, so they need splitting first).
- [ ] Custom team colours by hue, as Black Ops 6 offers, and colour-blind palettes for the 3D team bands on soldiers.
- [ ] Controller button remapping (Call of Duty's custom controller layouts, and its presets such as Tactical and Bumper Jumper).

## Phase 2: netcode core (in progress)

- [x] Choose the transport: Godot's `ENetConnection` as a packet pipe, with serialization in Rust (decision and reasons in `docs/roadmap.md`).
- [x] Fixed-tick loop shared by client and server (120 Hz, `shooter_sim::TICK_RATE`), with sequence numbers on every input and tick numbers on every snapshot. Snapshots go out at 60 Hz.
- [x] Input packets: each send carries every unacknowledged input (up to 16) to survive packet loss. The server keeps an input queue per player (`shooter_sim::net::input_queue`).
- [x] Client-side prediction: a history of inputs and resulting states per input; on each snapshot, compare, and on a mismatch restart from the server's state and replay pending inputs through `body::tick`.
- [x] Smooth error correction: blend visual position after a misprediction instead of snapping.
- [x] Snapshot interpolation for remote players, with an adaptive jitter buffer.
- [x] Interpolation delay sized by the connection, not a fixed 100 ms floor: two snapshot intervals, widened to the longest recent gap between snapshots while packets go missing, plus a jitter margin (`SnapshotClock`). About 35 to 50 ms on a clean link, which cuts peeker's advantage and deaths behind cover by about 50 to 65 ms.
- [x] Snapshot delta compression against the last acknowledged snapshot. Quantize positions and angles.
- [x] A determinism test in `rust/sim`: the same input stream gives bit-identical state across runs and in release and debug builds.
- [x] Network condition simulator (latency, jitter, loss) for local testing, plus a debug overlay (ping, loss, prediction error).
- [x] Headless dedicated server export: an export preset with the `dedicated_server` feature, a `--server` command-line flag, no rendering or audio.
- [x] Third-person player model for remote players: CC0 soldier with locomotion blend spaces, aim pitch, hand IK on the weapon, reload, and death (`SoldierRig`, see `docs/art-direction.md`).
- [x] Server-side shots: the server resolves `CombatEvent::Fired` against other players' rewound hitboxes, applies damage, keeps score, and sends hit confirmations, damage, kills, and score as reliable game events.
- [x] Input rate limits: draining a backlog costs credit that builds up one input per tick (`InputQueue`, at most a quarter second banked), so a client that sends inputs faster than the tick rate gets throttled instead of moving faster.
- [ ] Replays on slopes: `body::place` restores the body's floor contact but not its remembered floor normal, so a correction replayed on the ramp may drift and correct again. Add a ramp run to `godot/tests/net_test.gd` and restore the floor normal if it shows up.
- [x] Reliable game events on the reliable ENet channel: hits, damage, kills (`NetClient.kill_reported`, ready for a kill feed), and score.
- [x] Reliable events for Phase 4: joins and leaves with player names and teams, the match phase and clock, and the scoreboard.
- [x] Chat: Enter to everyone, Y to your team (`shooter_sim::chat` cleans and rate-limits messages on the server).
- [x] Hit reactions for third parties: `PlayerView` carries a wrapping hit count and the latest hit's heading, so everyone sees the flinch.
- [x] Validate view ticks against the shooter's round trip: `LagHistory::rewind_target_for` rewinds no further than the ENet round trip plus 0.25 s of interpolation slack explains (and never past the 300 ms cap).
- [x] Bandwidth: send the owner's full state only when it differs from what the client predicted, or delta-compress it like remote players. Measured by `godot/tests/bandwidth_test.gd` (a 6v6 bot match, [docs/netcode.md](docs/netcode.md)): 168 kbps down (94 of it the receiver's own state) and 106 kbps up per client at an 80 ms round trip, with 60 Hz snapshots and 120 Hz inputs.
  - [x] A reproducible bench: the server encodes every bot's snapshots and inputs as if it were a client and counts them; CI fails past a budget.
  - [x] The receiver's own state as field-by-field changes from the acknowledged snapshot's (generic `net::delta`): 94 kbps to 15.
  - [x] Inputs: view angles on a 2^-14 radian grid, and each input in a packet as changes from the one before: 106 kbps up to 37.
  - [x] Other players field by field through the same coder, and the Zombies horde as changes from the acknowledged one: about 68 kbps down in a 6v6, and 52 in Zombies with 24 zombies (83 before).
  - [ ] Follow-ups: code a packet's first input against the newest input the client saw acknowledged (about 6 kbps up); send inputs at 60 Hz, two ticks a packet, to halve packet headers (about 19 kbps up, up to 8 ms more input delay on odd ticks: an owner decision); the own state as an "in sync" bit when it matches the client's prediction (about 13 kbps down, for 4 up).

## Phase 3: lag compensation

- [x] Hitboxes per body part (head, chest, stomach, arms, legs) on the hitboxes collision layer, with damage multipliers (`rust/sim/src/hitbox.rs`, `HitboxRig`).
- [x] Server hitbox history: a second of sent snapshots (`net::lag::LagHistory`); hitboxes are rebuilt from position, view angles, and stance.
- [x] Rewind on fire: each firing input carries the client's interpolation tick; the server checks the shot against hitboxes at that tick, capped at 300 ms.
- [x] Debug visualization: with F4 on, the server's rewound hitboxes for each of your shots are drawn in white next to the client's.
- [x] Unit tests for rewind math in `rust/sim` (`net::lag`), and a netcode test that kills a strafing bot through a lossy, 110 ms link.
- [x] Sub-tick fire timing: a click between ticks fires from the view angles and remote-player view tick of the frame it was clicked on, not the next tick's (`net::subtick`, `PlayerInput::fire_aim`). The server accepts press angles only between the previous input's view and this one's; held automatic fire keeps the tick's angles.
- [x] What you see is what you hit: hit zones follow the animated soldier. They pose from the snapshot's activity (sprinting, airborne, mantling, reloading, aim, weapon) and the velocity between the two snapshots a sample blends, as well as stance and view angles: base poses measured from the animations and blended by speed (`rust/sim/src/hitbox/poses.rs`, `gait.rs`), hips turned toward a strafe, the aim pitch bent up the spine as the poser does, and arms on each weapon's grips with the character's own carry and IK (`hitbox/carry.rs`, shared with `SoldierRig`). `hitbox_fit_test.gd` measures bone-to-zone drift and skin outside the zones in every state and fails past 6 cm in settled states.
- [x] Animation progress in snapshots: the server keeps each body's stride phase and reload and mantle progress (`hitbox::clock`) and sends a byte of each; the third-person loops and climb are seeked to them, and the hit zones pose the measured moment (16 stride phases at each speed players move at, 17 points of a climb). Moving, reloading, and mantling bodies now match their zones within 6 cm like still ones.

## Phase 4: game loop

- [x] Health, regeneration after a delay, death, and respawn (`shooter_sim::player`). Respawn picks the original spawn until the spawn system exists.
- [x] Bots that play Team Deathmatch (`--bot-skill easy|normal|hard`): a navigation mesh baked from the map's collision, field-of-view and line-of-sight perception, reaction time, turn-rate-limited aim with shrinking error, bursts and taps, strafing, sprinting between fights, random loadouts, and players taking bots' places (`rust/game/src/net/bots.rs`, `rust/sim/src/bot.rs`).
- [x] Bots: take cover when hurt (a nearby spot the threat cannot see; they crouch, reload, and heal there), and follow the map's lanes (`BotLanes`), picking the one with the fewest teammates. They throw nothing yet.
- [x] A bot match bench (`godot/tools/bot_bench.gd`): whole bot-only matches headless, written to JSON with the winner, scores, every bot's kills and deaths, tactics, and each mode's objective log (Search and Destroy rounds, Hardpoint zone arrivals, Domination flag time), so bot changes are compared by outcome (`docs/bots.md`).
- [ ] Bots play like Black Ops 2's Combat Training bots (`docs/bots.md`):
  - [x] Search and Destroy: the carrier goes to plant as the clock runs down, escorts guard the plant spot, pushers gather and go in together, attackers switch sites when the target is held, a lurker takes the other site, and defenders rotate to a site under attack (`shooter_sim::snd::bots`, `SndPlan`).
  - [x] Grenades on the objective: an enemy seen near a site, flag, zone, or flag base is worth a frag on its own (flash if no frag), smoke covers a plant, a defuse, and a push onto an enemy flag, and Easy bots throw nothing (`BotObjective::area`, `smoke`).
  - [x] Search and Destroy: Hard bots fake a plant or defuse (`BotObjective::bait`).
  - [ ] Search and Destroy on Warehouse favours the defenders with bots (attackers won 5 to 8 rounds of 28 on the bench, before and after the plan): look at its sites' approaches.
  - [x] Domination: bots spread over the flags instead of one team piling onto one (the pile-up was bots trapped in a room the navigation bake shut off, #312), and a bot at a flag its team does not own takes it.
  - [x] Hardpoint: bots head for the next zone before the live one moves (`hardpoint::bots_rotating`).
  - [x] Capture the Flag: escorts ring the carrier instead of chasing its heels.
  - [x] Awareness: enemies on the minimap (unsuppressed fire, UAV sweeps) draw bots' eyes and patrols, and footsteps heard round a corner turn them.
  - [x] Combat feel: aim round corners on the path, peek from cover and fight from it, and on Hard slide into fights, crouch up close, and drop-shot prone early in a close fight.
- [x] Team Deathmatch rules on the server: two teams (new players join the smaller), 75 kills or 10 minutes, no friendly fire, a lobby that waits for everyone to ready up, a countdown, and a results phase before the next match (`rust/sim/src/tdm.rs`).
- [x] Round end screen: VICTORY, DEFEAT, or DRAW with the scoreboard, then the next match's countdown.
- [x] Team colours from the match: teammates wear the allies' colours and show their name; enemies wear the enemies' colours and show none.
- [x] Spawn system: a map's `SpawnPoints` (with an optional `team` metadata) are weighted away from enemies, their line of sight, and recent deaths, with a pull toward the player's own side. Matches start with each team on its own side.
- [x] Kill feed, scoreboard (Tab), and match timer HUD (`godot/scripts/net/tdm_hud.gd`).
- [x] Killcam from the server snapshot buffer (`rust/sim/src/killcam.rs`, `rust/game/src/net/killcam.rs`, `godot/scripts/net/killcam_hud.gd`): clients record the last 16 s of snapshots and replay a kill from the killer's eyes, with the others drawn as the killer saw them, Call of Duty's timings, the killer's card, a skip (use key) and a Skip killcams setting; the server holds the respawn until it is over. The kill that ends a match, or wins a Search and Destroy round by elimination, replays for everyone as the final killcam, slowed around the kill. Search and Destroy plays no killcam on death.
- [x] Killcam replays grenades: the tape records each snapshot's grenades and smoke clouds, and the replay draws them in flight, plays each frag's or flashbang's blast as it goes off, and shows the smoke aged to the moment (`Tape::grenades_at`, `blasts_between`, `smokes_at`). The guns the replay's bodies drop go when it ends (`CorpseYard::clear_replay_weapons`).
- [ ] Killcam follow-ups: hide bodies of live players who die while a killcam plays; hide live grenades and smoke clouds under the replay (a cloud still hanging live shows twice, once at its replayed age); first-person sounds for the killer's reloads and footsteps.
- [x] First real map: Slamcity rebuilt as a three-lane Team Deathmatch map (alley and rooftop, street, interiors), mirrored so both teams get the same lanes, with spawn points per team and a generator check that no line runs clear between the spawn yards (`tools/build_slamcity.py`).
- [x] Slamcity redesigned as a high-detail procedural city block (`tools/slamcity_layout.py`, built in Blender by `tools/build_slamcity_city.py`): tenements with stairwells, fire escapes, and roofs; Slam Junction's elevated station with a stalled train to cross through; a derailed car lying in the alley as a climb to the tracks; storefronts with furnished interiors and a diner roof terrace; a roadworks pit under the station; neon, an original sign atlas, and a dusk sky. Build-time checks keep collision mirrored, spawns out of enemy sight, and every long line down the avenue and alley broken.
- [x] Slamcity laid out again from studying real Call of Duty minimaps (`tools/map_blockout/`, `docs/map-design.md`): a square block of streets and enterable buildings round Junction Plaza and the derailed car, three routes with their own ranges, and halves balanced by a half turn instead of a mirror.
- [x] Slamcity: sites for Search and Destroy's charge: A in the pawn shop's back room, B in the ticket hall, each with a lettered marker crate (`CHARGE_SITES` in `tools/slamcity_layout.py`; `gear_models.charge_site_a` and `_b`). Maps without `ChargeSites` get two sites on the defenders' side of the middle.
- [x] Breakwater, a third 6v6 core map: an original coastal fuel terminal in late-morning sun, laid out in the Black Ops 2 three-lane tradition round a diagonal truck loading rack (`tools/map_blockout/breakwater_2026_11.py`, [map-design.md](docs/map-design.md#breakwater-november-2026)). Built like Slamcity from one layout (`tools/breakwater_layout.py`, models in `tools/breakwater_models.py`, Blender mesh, baked lightmap), with spawns, bot lanes, and objectives for every mode, a minimap, a generator check that no line runs clear between the spawn yards, and a place in the local match setup and on the server (`--map breakwater`).
- [ ] Breakwater art passes: set dressing and wear (decals, stains, litter drifts, signage) on the quay, rack, and tank farm; the sea's shoreline foam and the moored tanker's detail; interiors of the pump house and drum store; reverb and ambience (gulls, water lapping, pumps humming); a grade in `scripts/looks.gd`; a GPU budget pass (6.68 ms on Medium, worst 6.94 ms, against Slamcity's 7.58 ms in the same run).
- [x] Server browser: LAN search, master server list, favorites, direct connect by IP or host name, ping, and leave or lose the connection back to the browser.
- [x] Lobby flow on the server: warm-up until every player is ready (`--auto-start` skips it).
- [x] Lobby and loadout menu (L): primary weapon choice (applies from the next spawn) and Ready; opens by itself in the lobby.
- [x] A Team Deathmatch button on the title screen that runs a local server with bots and joins it.
- [x] Hardpoint (`--mode hardpoint`, and a title-screen button): one live zone at a time, moving every 60 seconds through the map's `Hardpoints` in a fixed order; a team alone in it scores a point a second, a contested zone scores nothing, first to 250. Capture, securing, defense, and assault awards, time on the hill on the scoreboard, objective bots, spawns kept off the hill, and a zone outline, wall of light, marker, and calls on the client (`rust/sim/src/hardpoint.rs`, `godot/scripts/net/hardpoint_view.gd`). Slamcity's five zones sit on its mirror line: the roadworks pit, under the El, the stalled train, the ticket hall, and the back alley.
- [x] Hardpoint overtime: when the clock runs out with the team behind in the zone (holding or contesting), play goes on until no team that is not ahead stands in it ([modes.md](docs/modes.md)).
- [x] Hardpoint zones for the testing arena (4), Nuketown (4), and Warehouse (5), checked on every map by `hardpoint_zones_test.gd` ([modes.md](docs/modes.md)).
- [ ] Hardpoint follow-ups: announcer voice lines; on Nuketown only one of six bots reached the Cul-de-sac in a minute's test (its navigation mesh never baked on a headless server; fixed, worth re-checking the Cul-de-sac).
- [x] Minimap, top left (`godot/scripts/net/minimap.gd`): each map's plan rendered from its collision by `godot/tools/build_minimaps.gd`, turning with the player (or north up), with teammates and the mode's objectives ([art-direction.md](docs/art-direction.md#minimap)).
- [x] Enemies on the minimap: red dots where an unsuppressed weapon fired, for 3 seconds, decided by the server and sent only to the shooter's enemies (`rust/sim/src/radar.rs`).
- [x] Scorestreaks (Black Ops 2): earned from score within a life (objective score counts, the count resets on death and loops after the dearest), three chosen in the class menu, held through death; UAV and Counter-UAV, a streak column and calls on the HUD, minimap tags, and bots calling theirs in ([docs/scorestreaks.md](docs/scorestreaks.md)).
- [x] Care Package: a crate from a flyover onto the spot aimed at, captured in 0.5 s by its caller and 3 s by anyone else, with a UAV or Counter-UAV inside; bots call and take them ([docs/scorestreaks.md](docs/scorestreaks.md#care-package)).
- [x] Airstrike: three strikes on points marked on the map's plan, after Black Ops 2's Lightning Strike; its kills count nothing toward streaks; bots mark their nearest enemies ([docs/scorestreaks.md](docs/scorestreaks.md#airstrike)).
- [ ] Scorestreaks follow-ups: the aircraft's, drones', jets', and crate's sounds (engine loops, the shoot-down), a UAV's damage smoke trail, UAV assist points, announcer lines, a full-map view for the dead or spectating. Warehouse is roofed over, so for now it leaves out the Care Package and the Airstrike (greyed out with "NO OPEN SKY"; `roofed` map metadata). Decide whether Warehouse gets a skylight or roof hatch so they can come back.
- [x] Perks that need streaks: Hardline, Ghost, and Hard Wired ([create-a-class.md](docs/create-a-class.md#perks)).
- [x] Sentry Gun (Black Ops 2's, 800): set down in front of its caller, a half-circle field of fire, 650 health that bullets wear at 60% and blasts at double, 240 s; bots place theirs and shoot enemy ones ([docs/scorestreaks.md](docs/scorestreaks.md#sentry-gun)).
- [x] Guided Missile (Black Ops 2's Hellstorm, 700): flown from its nose camera, boosted, split into a cluster over enemies near the aim; bots fly theirs at their nearest enemy ([docs/scorestreaks.md](docs/scorestreaks.md#guided-missile)).
- [x] Attack Chopper (Black Ops 2's Stealth Chopper, 1100): guards a point marked on the map for a minute, hopping between spots and firing bursts on the nearest enemy it sees; bullets do half and a rocket downs it; bots call theirs on an enemy and shoot enemy ones down ([docs/scorestreaks.md](docs/scorestreaks.md#attack-chopper)).
- [ ] Attack Chopper follow-ups: rotor and gun sounds, evading when locked on (Black Ops 2's), and flares.
- [ ] Guided Missile follow-ups: its launch, flight, and boost sounds, a warning to enemies under it, and restoring the pilot's view angle when it ends. (Enemies can shoot it down.)
- [ ] Sentry Gun follow-ups: its own firing sound, a minimap icon for our side's, and carrying and moving it (Black Ops 2 lets the owner pick it up). Care Packages hold one at Black Ops 2's weight.
- [x] Blind Eye (the Attack Chopper passes them by), Cold-Blooded (no red name or crosshair when aimed at; Guided Missiles neither box nor target them), and Engineer (enemy Sentry Guns and Care Packages through walls), with Black Ops 2's red name and crosshair when aiming at an enemy ([create-a-class.md](docs/create-a-class.md#perks)).
- [ ] Engineer's Care Package reroll and booby trap; Blind Eye's stand-still grace period once its value is known; a seventeenth perk needs wider perk sets on the wire (the sixteen bits are full).
- [x] Player names: sent in the hello (`Settings.player_name`), cleaned by the server, and listed in the roster.
- [x] Names in the kill feed and scoreboard.

## Zombies

Done: rounds with a growing, toughening horde, zombies that walk straight at the player (walk, jog, and run gaits) with a scratch attack, the two-hit rule, points for hits and kills, a round counter, points and game-over HUD, and a night arena (`godot/scenes/zombies_arena.tscn`, rules in `rust/sim/src/zombies.rs`).

- [x] Spend points: wall weapons with prices, ammo at half price, and debris over two gates that opens side yards with their own spawns and wall weapons (`scripts/zombies_door.gd`; the director is the wallet).
- [x] Power-ups that drop from kills: Max Ammo, Insta-Kill, Double Points, and a nuke (rules in `rust/sim/src/zombies.rs`, tokens in `scripts/zombies_power_up.gd`).
- [x] Start Zombies with only the M1911 (`WeaponState::carried`; an empty slot cannot be drawn until a weapon is bought).
- [x] Zombie sounds (groans, attacks, deaths; CC0, `tools/build_zombie_audio.py`) and round stings (synthesized).
- [x] Barriers: zombies spawn outside and tear the boards off their window; players rebuild them for points (`scripts/zombies_barrier.gd`).
- [x] Zombies on the dedicated server (`--mode zombies`, `--map zombies`): the server runs the horde, rounds, points, wall-weapon prices, and windows' boards (`rust/game/src/net/horde.rs`); zombies are sent in every other snapshot, quantized (about ten bytes each), and hit with lag compensation; clients draw them as puppets and the map's director mirrors the server for the HUD.
- [x] Doors, rebuilding windows, and power-ups on the server: a press buys a door, holding use rebuilds a window a board every 0.6 s (10 points, up to 300 a round), and power-ups are taken by any standing player for the whole team (Max Ammo for everyone, the Nuke pays every player 400).
- [x] Rebuild speed from Black Ops III's scripts: a board goes up as soon as use is held, then one a second (`do_post_chunk_repair_delay`), paying 10 while the round's running total stays under 50 times the round, at most 500 (`rebuild_barrier_cap_per_round`).
- [x] Black Ops III's power-up drop cycle ([docs/zombies.md](docs/zombies.md#power-ups)): a drop earned every 2000 points the team earns (14% more each time), a 3% chance on any kill, four a round, kinds dealt from a shuffled deck; 15 s on the ground and 11.5 s blinking. The Carpenter (once five windows are bare) boards every window back up, nearest first, and pays everyone 200.
- [x] Downed state and revives for co-op (`rust/sim/src/zombies/laststand.rs`, Black Ops III's numbers): the second hit downs instead of killing; downed players crawl with the sidearm (two spare magazines), take no harm, and zombies ignore them; a teammate holding use within 1.9 m for 3 s revives them (held still, as the client predicts), earning the 5% of points they lost going down; after 45 s they bleed out, every other player loses 10%, and they come back at the next round. Everyone down ends the game, so solo play ends on the first down.
- [x] Downed players crawl prone (Black Ops' last stand), at the prone crawl's 0.8 m/s, the pistol up through the crawl.
- [ ] A revive animation for the reviver.
- [x] Drinks and the power ([docs/zombies.md](docs/zombies.md), Black Ops III's perk numbers under our own names and machines): a power switch in the west yard; Bulwark (down on the fifth hit), Second Wind (half-time revives; alone, 500 points, no power needed, and back up by yourself after 10 s, three times a game), Quickload (half-time reloads), Hair Trigger (a third faster fire, double damage on zombies), and Long Stride (7% faster); four at most, all lost on going down; badges on the HUD.
  - [ ] A drinking animation (a bottle in the hands for about 3 s) instead of the weapon's re-raise.
  - [ ] Drink and machine sounds from real CC0 recordings (a cap, a gulp, a machine hum); none suitable found yet.
- [x] A mystery crate ([docs/zombies.md](docs/zombies.md#the-mystery-crate)): a random weapon for 950 points from a weighted pool, never one carried, cycling up out of it for 4 s and held out to the buyer for 12 s; after enough spins (Black Ops III's odds) the duck, a refund, and a move to another of three spots.
  - [ ] Crate sounds (the lid, the cycle, the duck) from real CC0 recordings.
  - [x] Fire Sale ([docs/zombies.md](docs/zombies.md#power-ups)): once the crate has moved, a power-up that puts a crate at every spot for 30 s at 10 points a spin; sale spins never land on the duck.
- [x] The forge, an upgrade machine ([docs/zombies.md](docs/zombies.md#the-forge)): with the power on, 5000 points upgrades the weapon in hand in 3.35 s (twice the damage, half again the magazine, twice the reserve, a blue sheen), taken within 15 s or lost; its wall ammo costs 4500.
  - [ ] Re-forging an upgraded weapon for an alternate ammo type (Black Ops III's 2500-point AAT).
  - [ ] Forge sounds from real CC0 recordings.
- [x] The title screen's Zombies runs on a local dedicated server: Solo starts a private one that skips the ready-up and pauses; Co-op hosts one friends on the network find under Multiplayer, LAN (up to four players, ready-up, no pause). The scoreboard shows points, kills, downs, revives, and headshots.
- [ ] Remove the offline `ZombieDirector` game once the benchmark and the capture scripts run Zombies on a server (they are its only users now).
- [ ] Zombies on the Internet tab: a hosted co-op game registers with a master server.
- [x] Hound rounds ([docs/zombies.md](docs/zombies.md#hound-rounds), Black Ops III's dog-round numbers): every 4 or 5 rounds from round 5 to 7, a pack of Blender-built hounds struck into the arena, the last leaving a Max Ammo.
  - [ ] Hound sounds (breathing, snarls, bites, the strike's crack) from real CC0 recordings.
- [ ] Playable versions of the menu's maps; today every map loads the one arena.
- [x] A cheaper zombie for big hordes: two surfaces per character instead of eleven (a 24-zombie horde draws about 195 calls instead of 630).
- [x] Animate distant zombies at a reduced rate: every second frame beyond 18 m and every fourth beyond 32 m, staggered across zombies.
- [x] A horde that is not one costume marching in step: each zombie's walk starts at its own stride phase and its speed varies ±8% round the round's; clothes darkened and dirtied so they stop glowing under the floodlights; faces lifted out of black.

## Search and Destroy

Rounds with one life each (`rust/sim/src/snd.rs`, `--mode snd`), on Slamcity. Classic rules: the attackers plant a charge at site A or B, the defenders stop them or defuse it.

- [x] Rounds: attackers and defenders, a freeze, 1:45 of play, wipe-outs and time wins, the dead sit out until the next round, joiners wait for it, and the teams swap sides at halftime. Round wins are the match score (first to 6).
- [x] Spectating a living teammate after death, alive pips beside the score bar, round banners, and a Mode choice in the local match setup.
- [x] The charge ([docs/search-and-destroy.md](docs/search-and-destroy.md)): a random attacker carries it, drops it on death, and any attacker picks it up by walking over it; a 4-second plant at site A (the pawn shop's back room) or B (the ticket hall), a 45-second fuse that replaces the round clock, and a 7-second defuse that starts over if interrupted. The interact button held rides in `PlayerInput`; planting or defusing holds the player still with the weapon away, on the server and in prediction (`PlayerInput::using_charge`). Site markers, a PICK UP or ESCORT marker for the attackers, the hold prompt, the plant and defuse bar, the charge model with a blinking lamp, the kneeling pose on other players, and calls (`scripts/net/snd_view.gd`).
  - [x] Hit zones that follow the kneeling plant and defuse pose (`hitbox::action::kneel`, `Activity::kneel`; checked by `hitbox_fit_test`).
  - [ ] Hit zones partway through any stance change (a crouch, or the 0.3 s kneel at the charge) stray up to 12 cm from the drawn legs: the zones lerp joint positions between the standing and crouched poses while the drawn body blends bone rotations. Fully in a stance they match.
  - [x] First-person hands on the charge while planting or defusing: the case held low in view, the left hand steadying it, the right index finger tapping its keypad (`viewmodel::keypad_hands`, `tests/keypad_hands_test.gd`).
  - [ ] Separate first-person plant and defuse motions (both type on the keypad today; a defuse could work the wires with cutters, as Black Ops 2's does).
- [x] Bots play the objective (`NetServer::charge_objectives`, `ObjectiveKind::Use`): the carrier runs to the round's target site and plants, the other attackers push into that site, the nearest attacker fetches a dropped charge, and after the plant they hold round it; defenders split between the sites, and once it is planted the nearest runs to defuse while the rest clear round it. A bot fights an enemy in sight before planting or defusing (`tests/snd_bots_test.gd`).
  - [ ] Bots that pick their site from what they see (switching when one is held), lurk on the flanks, and fake a plant or defuse to bait a shot.
- [x] Plant and defuse scores (+200 each) and medals: Wire Cutter (+100, a defuse with an attacker alive), Clutch (+200, winning a round as your team's last one standing against at least one enemy), and Ace (+300, killing every enemy who started the round, at least three), with their own emblems and plants and defuses on the scoreboard (`snd::RoundRecord`, `score::Award`). The charge going off kills within 13 m (200 damage falling to 20, Call of Duty 4's), credited to the planter, with a "Charge" kill feed line and the frag blast's effect.
- [x] The charge's own, larger explosion effect: fireball, dust, smoke column, chips, scorch, flash, and a screen shake to about 50 m (`CombatEffects.charge_explosion`).
- [ ] The charge's accelerating beeps, its own explosion sound, and plant, defuse, and detonation stings: synthesized clips need the owner's listening approval before merging (docs/audio.md).

## Phase 5: online services (all self-hosted)

- [x] Nakama (self-hosted, Docker Compose) for accounts, parties, and matchmaking. Use the `nakama-godot` client.
  - [x] The stack (`docker/online`), device sign-in with a username on the title screen, and progression and classes on the account ([docs/online.md](docs/online.md)).
  - [x] Ranked dedicated servers (`--accounts`): sign-in required, unlocks checked against the account, match XP reported by the server ([docs/online.md](docs/online.md#ranked-servers)).
  - [x] Parties (invite by name) and quick play per mode, with a match of the party's own and bots when nobody else is searching ([docs/online.md](docs/online.md#parties-and-quick-play)).
  - [x] Sign in on another machine: an email and password linked to the account, then name or email and password there.
  - [ ] Party chat and a friends list (Nakama has both), and a party lobby that follows the leader into a server browser match.
  - [ ] Matchmaking by skill and ping (Nakama matchmaker properties), once there are enough players to choose from.
  - [ ] Run the stack on the public host behind TLS, so `borseth.ddns.net:7350` answers.
- [x] Dockerfile for the headless server (Godot export + Rust extension): `docker/server`, published to `ghcr.io/jedborseth/shooter-server` from `main` ([docs/server.md](docs/server.md)).
- [x] Strip textures from the server export (558 MB of the 693 MB pack is `.ctex` the server never draws): every image is marked "Strip Visuals" in the Linux Server preset at export time (`tools/server_export_filter.py`).
- [x] Server allocation: Nakama match handler starts or assigns a dedicated server and hands its address to players. Agones on Kubernetes later.
  - [x] The allocator (`rust/allocator`): starts a ranked server container per match through the Docker socket, on its own port pair, which stops once empty ([docs/online.md](docs/online.md#server-allocation)).
  - [x] The Nakama match handler (`shooter_match.lua`) that calls it for each match and hands players the address.
- [x] Server-side validation and anti-cheat basics: packet floods and garbage disconnect; aim snaps onto hits, view ticks the round trip does not explain, and lag switching are flagged in the server log, never kicked ([docs/netcode.md](docs/netcode.md#server-checks), `shooter_sim::net::integrity`). Speed, teleports, and fire rate cannot be cheated: the server simulates them from inputs.
  - [ ] Follow-ups: send each client only the enemies it could see or hear (against wallhacks; killcams and the minimap need a plan for the rest), and a way for an operator to see flags without reading the log (once there are accounts).

## Visual overhaul

Grounded stylized realism: real materials and props on simple shapes, inside the GPU budget in [docs/performance.md](docs/performance.md). Every step shows benchmark numbers before and after.

- [x] Graphics presets and the rendering benchmark.
- [x] PBR world textures: a CC0 texture library and one world-space surface shader for every map (`tools/build_textures.py`, `godot/shaders/world.gdshader`).
- [x] Lighting: CC0 sky images lined up with each map's sun, sky light in the shade, haze, colour grading, flickering lamps and volumetric fog (High) in Zombies.
- [x] Baked lightmaps on every map, with baked-only interior lights in Slamcity (`tools/bake_lightmaps.sh`).
- [x] Lightmap checks catch a bake that is black only on the map's main surfaces, in shade (`godot/tools/lightmap_check.gd`, #240's failure).
- [x] Weapons: PBR materials, smoothed shading, baked wear, better muzzle flash and smoke (`tools/build_weapons.py`).
- [x] Map props: CC0 Poly Haven models (cars, lamps, crates, barrels, bins, barriers, AC units, shutters) on the existing collision (`tools/build_props.py`).
- [x] Map meshes: the blockout CSG is baked into meshes with lightmap UVs (`godot/tools/bake_map_meshes.gd`).
- [x] Testing Area set dressing: a control tower, a site office, an ammunition container, target stands and steel, sandbags, pallets, floodlights, cables, drains, litter, and weeds, every floor slab and wall panel shaded on its own with streaks, stains, and tyre tracks, joints cut unevenly, and the wall's panels slotted between cast posts (`tools/map_dressing.py`, `docs/art-direction.md#set-dressing`).
- [x] Zombies arena set dressing: barricades, burnt-out cars, rubble and torn boards, blood from the windows, abandoned kit, wet streaks, emergency beacons, and floodlights on the columns (`map_zombies_dressing`).
- [x] Map atmosphere: a Zombies night sky drawn in code with the moon on the moonlight's bearing, a rendered skyline of tree belts and a lit town over its walls (`tools/build_horizon.py --night`), fog pooled in the side yards and window bays (High), and open country beyond the Testing Area's walls: fields, hedgerows, woods, and farms rendered in Blender from real tree meshes (`tools/build_horizon.py`).
- [x] Building trim on Slamcity: window frames, sills, storefronts, a gutter, and downpipes; road paint, damaged asphalt, manholes, and rain streaks as decals.
- [x] A fire escape on the flats, up to the roof (built in the generator; Poly Haven's `modular_fire_escape` could replace it once assembled from its parts).
- [x] Characters: one material and two surfaces per character, rest-space fabric and wear detail, and per-zombie skin tone and blood (`shaders/character.gdshader`).
- [x] Effects and post: spark streaks, debris, soft dust, blood, bullet-hole decals, glowing tracers, ambient dust, and a motion blur option. Depth of field while aiming was left out (it blurs the iron sights).
- [ ] Combat effects at modern Call of Duty fidelity (`rust/game/src/effects.rs`, `docs/art-direction.md#effects`):
  - [x] Blood that is dark, wet, and lit instead of glowing, with spatter on the wall behind a close target; bullet holes that stay (pooled per preset, oldest reused first) and never land on characters; other players' rounds draw their impacts.
  - [x] Bullet holes per surface class (four variants each, soot ring, roughness), flipbook dust, debris that lands on the floor, and exit effects where a round penetrates.
  - [x] Tracers as camera-facing ribbons with a minimum pixel width, from other players too.
  - [x] Ejected cases handed to the world: they bounce, land, clink on what they hit, and stay; third-person ejection.
  - [x] Lit muzzle smoke that stays in the world, lingering barrel smoke after sustained fire, and a muzzle light that stays on its side of walls.
  - [x] Muzzle flash flipbooks: a front star, a side plume, and their smaller aimed frames, rendered in Blender, a random frame per shot, sized per class.
  - [x] Soft (depth-faded) gun smoke on High, first and third person (`muzzle_smoke.gdshader` `SOFT_SMOKE`).
  - [ ] Heat haze on High: priced at +0.4 ms in the firefight (a screen copy); waits for High headroom on Slamcity.
  - [x] An explosion and fire effect with an API for grenades (`CombatEffects.explosion`, `CombatEffects.fire`); frag grenades use the explosion.
  - [x] The smoke grenade's cloud and the flashbang's pop as pooled flipbook effects (`CombatEffects.smoke`, `CombatEffects.flash_pop`, Unity Labs' Cloud flipbooks).
  - [x] A frag that reads as dirt thrown up, not a white puff: a short bounded orange core, jets of dirt from a Blender-rendered flipbook, a lingering dust cloud and ground skirt, modelled rubble chips, and a larger scorch with fragment pits (`tools/build_explosion_fx.py`).
  - [x] Concrete impacts: modelled chips tinted by the surface instead of flat cubes, and sparks only on metal (shorter, thinner, fewer).
  - [x] The smoke grenade's cloud: density variation, sun shading, and a ragged edge; the flashbang pops as a short white-hot burst with a grey puff.
  - [x] A wound mark where a character was hit (character shader, `wounds.rs`).
- [x] Skyline windows with fake rooms behind them (interior mapping in `world.gdshader`), and bump-mapped cloth, leather, and creases on characters.
- [x] Soldiers as believable people: a MakeHuman body fitted to the existing skeleton, a combat shirt and trousers, modelled kit (helmet with cover and mounts, glasses, balaclava or neck gaiter or a bare face per character, plate carrier with pouches, magazines and a radio, belt and holster, knee pads), all in one mesh with one baked texture atlas and a team tint (`tools/build_character_assets.py`).
- [x] A loaded soldier's build: more muscle and weight and MakeHuman region targets (chest, back, neck, arms, thighs; girth only, joints on the skeleton), plates 4 cm proud, padded straps and belt, hip and first-aid pouches, and a hydration carrier on the back (25.4k triangles). Arm and chest hit zones widened to match.
- [x] Zombies as distinct people: five MakeHuman bodies (age, build, skin, one a woman) in their own civilian clothes, picked per zombie (`zombie_body.tscn`, `SoldierRig.variants`).
- [x] Zombies: painted short hair and stubble per body.
- [x] Zombies: painted wounds, blood soaked and run from them, a bloody mouth, rips in the cloth, grime and mud, per body (baked, `add_decay`).
- [x] Soldiers: a modelled combat shirt (knit torso, ripstop raglan sleeves with pockets, elbow pads, cuff tabs, and a zipped mock collar) baked from a high-detail source with folds and stitching (`tools/blender_wardrobe.py`).
- [x] Soldiers: modelled combat boots (leather foot, toe cap, heel counter, nylon shaft, laces and eyelets, padded collar) baked from high-detail sources, with the trousers bloused over them.
- [x] Soldiers: five faces (MakeHuman head shapes as blend shapes, each with its skin tone), picked per soldier.
- [x] First-person arms: their own model and atlas (sleeves with sewn cuffs, gloves with knuckle guards, a field watch), and finger shapes per hold (pistol grip with trigger discipline, rifle handguard, pump, two-handed pistol).
- [x] Firing hands closed round every weapon's grip, in first and third person: each `GripRight` sits on its model's grip and is raked as the grip is, and hand-set values live in `hand_tuning`, which the hand-pose bake keeps.
- [x] The LSW-6's support hand clear of the red dot's window: it was the index curling over the handguard's top, not the thumb; it now lies along the handguard (`hand_tuning`).
- [x] First-person gloves and sleeves: modelled seams, palm panels, and knuckle padding, and cloth-simulated sleeve folds, baked from high-detail sources into the arms atlas (`tools/blender_hilo.py`).

## Phase 6 and later: production polish

- [x] Post-processing pass: a colour grade per map as a look-up table built in code (`godot/scripts/looks.gd`; Testing Area, Zombies, Slamcity, and Nuketown), bloom that lifts only what is bright, a lens pass with a vignette, film grain, and sharpening on High (`godot/scripts/lens.gd`), and colour that drains as health drops. (Motion blur is an option; depth of field while aiming was left out because it blurs the iron sights.)
- [x] A dusk look for the rebuilt Slamcity in `godot/scripts/looks.gd` (cool shade, warm light, bloom on the neon only).
- [x] Picture fidelity, first round: bloom added like light through a lens with lens dirt and a sun flare that hides behind cover, contact shadows under characters, close-up surface detail on world surfaces (High), wet surfaces and puddles in Zombies, a scope lens with eye-relief shadow, suppression and death on the lens, and a frame profiler that alternates each variant with the full frame (`godot/tests/profile_frame.gd`).
- [x] A look for Nuketown in `godot/scripts/looks.gd` (desert noon; only free settings).
- [x] Temporal antialiasing on High (in place of 2× MSAA plus SMAA, same cost), close-up detail only on rough non-metal surfaces, a hurt vignette with smears, hit flashes, and a heartbeat, suppression on Low, softer puddles.
- [x] Eye adaptation on every preset (rays from the camera, no GPU time; slow, within a stop, against each map's own typical light) and a key light for the first-person weapon and arms in the viewmodel shader.
- [x] The first-person key reaches the gloves and sleeves too (a first-person copy of the character shader), and in shade it becomes the sky's light, so the weapon keeps its shape in shade and under a roof. The weapon already takes the sun's shadow.
- [x] Grounding: contact shadows shaped by stance (longer and darker crouched and sliding, turned with the body) that fade to a core in direct sun, and tight contact ambient occlusion in the Testing Area and Slamcity (`godot/scripts/grounding.gd`, `godot/tests/capture_grounding.gd`).
- [x] Wet walls: narrow vertical run-off streaks that stay soaked on wet maps (Zombies), sharing the puddles' noise read, compiled out on dry maps.
- [x] Scope glass with the slight green-amber cast and light loss of coated optics (`shaders/scope.gdshader`).
- [x] Raising the sniper scope shows the zoomed world in its eyepiece and grows it into the scope picture while the surround fades in, instead of a black eyepiece and a cut at 85% (`scope_eyepiece`, `scope_open`).
- [x] A grenade throw lowers the weapon out of the frame in the firing hand, instead of leaving it hanging in view with no hand on it (`grenade_hold::weapon_dip`).
- [x] First-person throw sleeve: the support arm no longer rises through the middle of the view about 20 cm from the lens at the release. Through the throw the arms are drawn life size about the support shoulder, the body moves up and forward as a thrower's torso turns, and the elbow swings out beside the head from the draw-back through the release, so the forearm crosses in from the edge of the view at arm's length (`FpArms::throw_shift`, `throw_pole`, `throw_scale`, `grenade_hold::elbow_out`; `fp_throw_arm_test`).
- [x] Close-up detail strength per texture set (`tools/world_detail.py` `DETAIL`), and anti-tiling by a shifted sample instead of a turned one.
- [x] Range props: the impact area's steel plates swing when hit and settle (`scripts/steel_plate.gd`, visual only, through the `struck` group `CombatEffects` calls on every impact), and a wall rack of rifles on the ammunition container using the third-person weapon models (`scripts/wall_rack.gd`).
- [ ] Picture fidelity, next (see the round 11 review): contact-hardening sun shadows, daytime volumetric light on High, moving clouds, self-shadowing on the first-person weapon (the weapon and arms cast no shadow: in the sun's shadow map they would also throw a floating gun's shadow on the ground, since the local player has no body shadow).
- [ ] Lighting pass: baked lightmaps or SDFGI tuning per map, volumetric fog, reflection probes.
- [x] Performance budget and profiling: a 6.5 ms Medium GPU gate per map (`just perf-gate`, `docs/perf/budgets.json`, fails on a GPU machine, never on GitHub's runners), CPU time per system on the client and server (`tools/cpu_bench.sh`, `SHOOTER_CPU_PROFILE`), lazy hit zones and animation LOD (client process step 9.1 to 7.2 ms in a 6v6), Warehouse baked (29.5 to 6.4 ms). See [docs/performance.md](docs/performance.md).
  - [ ] Slamcity (8.5 ms) and Breakwater (6.4 to 7.1 ms) over the Medium gate: the rest is Medium's own passes (SSAO's normal prepass 1.4 ms, glow 0.75, SMAA 0.7). Owner decision: turn one down on Medium (for example SSAO off on maps whose lightmap already darkens corners, or FXAA in place of SMAA), or keep 8 ms for these maps.
  - [ ] Low over 4 ms on Slamcity, Breakwater, and Zombies (4.7 to 5.8 ms).
  - [ ] Warehouse: the strewn floor litter (MultiMeshes lit by the lightmap's probes) reads darker than it did lit live; bake it as meshes or brighten the probes.
  - [ ] Client CPU: deferred calls and skeleton updates are 2.4 ms of a 6v6 frame (most of what is left); the server's character movement (`move_and_slide` and probes) is 1.4 of its 1.8 ms tick.
- [x] Loadouts and a create-a-class screen: Pick 10 classes (two weapons with up to three attachments, frags, tacticals, wildcards), checked by the server, five custom classes saved locally and six defaults, picked in the match menu ([docs/create-a-class.md](docs/create-a-class.md)).
- [ ] Create-a-Class follow-ups: perks and their slots (with Perk Greed), pistol attachments (with Secondary Gunfighter), and Tactician (a tactical on the lethal button).
  - [x] Perks in the simulation and on the server: Black Ops 2's three tiers and Perk Greed in the class rules, Lightweight, Fast Hands, Sleight of Hand, and Dexterity predicted, Flak Jacket, Toughness, and Tactical Mask where they are decided, unlock levels, bots with a perk of each tier (`rust/sim/src/perk.rs`).
  - [x] Perks in the class menu, with icons, and on the default classes.
  - [x] Dead Silence and Awareness footsteps.
  - [x] Scavenger packs.
  - [x] Bots go for Scavenger packs nearby when low on ammunition.
- [x] Progression: XP, levels, weapon unlocks: score is XP plus a match bonus, levels 1 to 55, Create-a-Class at level 4, weapons, tacticals, and wildcards by level, attachments by kills with the weapon, kept locally (`rust/sim/src/progression.rs`, `godot/scripts/progress_store.gd`).
- [ ] Progression follow-ups: weapon levels shown in the menu. (Prestige is done: [docs/progression.md](docs/progression.md).) (Progress on the account, with unlocks checked by ranked servers, is done: [docs/online.md](docs/online.md).)
- [ ] Long-term progression, Black Ops 2's ([docs/progression.md](docs/progression.md)):
  - [x] Career stats counted by the server (`rust/sim/src/stats.rs`), challenges with XP (`rust/sim/src/challenge.rs`): each weapon's camo ladder, gold, class mastery, the final camo, and career, mode, and scorestreak challenges; kept on the account (ranked servers report them) or locally.
  - [x] A challenges screen and a combat record (Career, on the title screen).
  - [x] Camos on the weapons (first person, third person, killcam), picked in Create-a-Class; cosmetics (prestige, card, camos) on the wire, checked by ranked servers.
  - [x] Prestige: 10 and Master, with emblems and permanent unlock tokens; the icon on the scoreboard and in the kill feed.
  - [ ] Calling cards, picked in the profile, on the scoreboard and the killcam card.
- [x] Gun Game: free-for-all up a ladder of the eight weapons, with a local match button on the title screen (`rust/sim/src/gun_game.rs`).
- [x] Capture the Flag: flags, takes, drops, returns, and captures on the server (`rust/sim/src/ctf.rs`), bots that attack, defend, return, and carry, flag markers and calls on the client (`godot/scripts/net/ctf_view.gd`), and a title screen button.
- [ ] More modes: Domination, the Black Ops 2 way ([docs/domination.md](docs/domination.md)). (Free-for-all is done: `--mode ffa`, on Gun Game's teamless path. Search and Destroy has its own section.)
  - [x] Rules on the server: three flags, 10 s captures sped up by more capturers, contests, a point a flag every 5 s, 200 to win, halftime side swap, flag spawns, capture, defence, and assault awards (`rust/sim/src/domination.rs`, `--mode dom`).
  - [x] Flag markers in the world and on the HUD, calls and stings, the scoreboard's columns, and a title screen choice (`godot/scripts/net/domination_view.gd`).
  - [x] Objective bots: capture, defend, and take back flags (`domination::bot_goals`).
  - [x] Flags placed by hand on Slamcity, Nuketown, and Warehouse, and fallback flags laid across the middle on maps whose sides are close (the testing arena).
  - [ ] Follow-ups: an announcer voice for the calls, a flag pole model at each flag (Blender), overtime on a draw (Black Ops 3 onward), and Nuketown bot play: its bots move now (the navigation mesh bakes inside the playable area), but in `domination_bots_test -- --nuketown` one team takes all three flags in the first half minute and holds them, so only two of six bots capture (the test wants three). Bots that push back for a lost flag when the team owns none would fix it.
  - [ ] Testing arena bots barely play objectives: its fallback spawns stand 14 m apart on one open floor. Real `SpawnPoints` at the north and south walls would fix it, but nine server tests (net, killcam, bot equipment, Domination, CTF halves, Hardpoint overtime, local pause, Zombies) are written around the fallback layout, so moving the spawns means reworking them.
- [x] Gun Game: a knife kill sets the victim back a weapon, as in the classic rules.
- [x] Gun Game: a sound for each new weapon and for a set-back (`match_stings.gd`; the sting files are in their own PR, held for a listen).
- [x] Capture the Flag: halftime side swap and a sudden-death overtime round on a draw, as in Black Ops 2 ([modes.md](docs/modes.md)).
- [ ] Capture the Flag: a carrier speed or weapon penalty if playtests call for one.

## Tooling

- [x] GitHub Actions CI: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, gdlint and gdformat, and the headless smoke test (download Godot 4.7.2 in the job).
- [x] Release builds: export presets for Linux, Windows, and macOS clients, exported on `main` and published on `v*` tags.
- [x] Dedicated server build: a Linux export preset with `dedicated_server=true` (done, exported by CI), packaged as a Docker image and pushed to GitHub Container Registry from CI (`.github/scripts/server-image.sh`).
- [x] Determinism test in CI: record an input sequence, run it through `shooter_sim::player::step` twice (and on each OS in the build matrix), and compare state hashes. Catches prediction desyncs before netcode depends on them. (`rust/sim/tests/determinism.rs`)
- [ ] Visual and performance checks on a GPU runner: run `capture_screens.gd` and a scripted fly-through that records frame times, on a self-hosted runner with a GPU, for trusted branches only.
- [x] A `justfile` with `build`, `run`, `test` (the tests listed in CI), `screens`, and `ci` recipes.
- [x] CI flakes fixed at the root, with a flake log in `docs/ci.md` (#351), and Godot tests sped up: two real-time lanes (#352) and three fixed-rate workers (#355), taking the Godot job from about 8 minutes to about 5.
- [ ] Start the longest fixed-rate Godot tests first (`nuketown_bounds_test` takes 35 to 60 s and starts mid-run), so the three workers finish together.
- [ ] The "server keeps up with its tick rate" checks in `bots_test`, `hardpoint_test`, and `domination_bots_test` time the wall clock. They have about 6x headroom on CI but could trip on a heavily loaded machine; measure the server's own tick cost instead.
- [ ] Clippy on Rust 1.99 flags three `needless_range_loop` lints in `rust/game/src/character/poser.rs`; fix them before CI's stable toolchain moves to it.
- [ ] **good first task** Editor plugin or script that measures a weapon model's sight line and muzzle to fill in `WeaponView` offsets automatically (what was done by hand for the M4 and M1911).
