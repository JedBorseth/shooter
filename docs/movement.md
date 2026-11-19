# Movement

How the player moves, and the Call of Duty reference each rule follows. The
rules live in `rust/sim/src/movement.rs` (`MoveParams` holds every number);
the engine side (collision, step-up, ledges) is `rust/game/src/body.rs`, and
the camera is `rust/game/src/player.rs`. The client predicts every rule here
and the server runs the same code, so any new state goes into `MoveState` and
its wire encoding (`rust/sim/src/net/protocol.rs`).

The reference is Modern Warfare (2019) and MWII class movement: community
measurements and the classic IW engine values. Numbers are approximate; the
aim is the feel, not a copy.

## Ground and sprint

| | Here | Reference |
| --- | --- | --- |
| Walk | 5.0 m/s x weapon speed scale (0.88 to 1.05) | rifle walk about 4.7 to 5 m/s |
| Rest to 95% walk | 0.23 s (accel 8, friction 6) | about 0.2 to 0.25 s |
| Walk to a stop | 0.28 s (stop speed 2.5) | about 0.25 to 0.3 s |
| Strafe, backpedal | 0.8 x, 0.7 x | classic IW 0.8, 0.7 |
| Sprint | 6.9 m/s x weapon speed scale | about 6.5 to 7 m/s |
| Walk to sprint | 0.22 s (`sprint_blend` over 0.25 s) | about 0.2 to 0.3 s |
| Sprint to walk | the weapon's `sprint_out_time` (0.12 s pistol to 0.38 s LSW) | heavier guns are slower out |
| 45-degree diagonal sprint | 0.88 x sprint, turned mostly forward | no full speed on diagonals |

A sprint is a toggle. Its speed cap follows `MoveState::sprint_blend`, which
builds over `sprint_in_time` while sprinting and winds down over the weapon's
sprint-out time (read from the weapon file, owned by weapon feel) once it
ends, so the legs and the gun come out of a run together. While sprinting the
sideways input counts for half, so a diagonal sprint runs mostly forward.

## Jump, air, and landing

| | Here | Reference |
| --- | --- | --- |
| Jump | 6.2 m/s up, gravity 19: about 1 m, 0.65 s | about 1 m |
| Touchdown after a full jump | keeps 0.7 x horizontal speed | about 0.65 x |
| Recovery | 1.2 s: speed cap from 0.75 x and jump from 0.75 x back to 1 | about 1.2 to 1.8 s |
| Air steering | up to 2.5 m/s toward the input (accel 4) | about half a walk |
| Air strafing | never adds speed | none |

A landing counts by the time in the air: under 0.2 s (a curb, a stair) not at
all, a full count from 0.6 s. Repeated jumps start inside the last landing's
recovery, so each is lower and a bunny hop loses speed. Air steering turns
the velocity but never makes it longer than at takeoff (or the 2.5 m/s steer).
New state: `MoveState::air_time` and `MoveState::land_slow`.

## Stance

| | Here | Reference |
| --- | --- | --- |
| Standing eye | 1.68 m (capsule 1.8 m) | IW 1.52 m (scaled to our taller capsule) |
| Crouched eye | 0.98 m (capsule 1.1 m) | IW 1.02 m |
| Crouch down | 0.25 s | about 0.2 to 0.3 s |
| Stand up | 0.3 s | about 0.25 to 0.35 s |
| Drop into a slide | 0.15 s | quick, under 0.2 s |

The stance itself flips at once (sprint, slide, and jump rules read it), but
the body follows a **crouch blend** (`MoveState::crouch`, 0 standing to 1
crouched) over the times above, eased with a smoothstep (`crouch_ease`). That
one blend drives the capsule height, the eye (the camera and the shot origin
on the client and the server), the speed cap while going down, the hit zones
(`hitbox::Activity::crouch`), and the third-person body (`SoldierFrame::crouch`).
Snapshots carry it as a byte (`PlayerView::crouch`). The camera draws the
blend interpolated between ticks, so it is never more than one tick from
where shots leave.

Standing up waits while there is no headroom: the stance stays standing and
the blend holds until the overhead is clear. The headroom cast is a sphere
almost as wide as the capsule (0.34 m against 0.35 m), so a leaned-on wall is
not a ceiling but an overhang's edge over the shoulders is.

## Slide

| | Here | Reference |
| --- | --- | --- |
| Entry speed | 1.1 x sprint x weapon speed scale (7.6 m/s with a rifle) | about sprint speed, at most 1.1 x |
| Duration | 0.65 s (holds speed 0.15 s, then friction 1.4) | about 0.6 to 0.7 s |
| Distance | about 4 m from a rifle sprint | about 3.5 to 4 m |
| To earn one | 0.3 s of unbroken sprint, 0.5 s on the ground since the last | a short sprint |
| From the air | crouch while sprinting in the air queues one; it starts on landing if earned and the heading is within about 45 degrees of the view, else the player lands crouched | IW8 `Slide_CanSlideFromAir`: a minimum air speed and a heading within an angle of the view |
| Steering | strafe turns it up to 1.4 rad/s (80 degrees a second), keeping its speed; the view never steers it | IW8 locks the slide's direction, lets the view turn freely, and lets the strafe input bend it |
| Wind-down | 0.5 s after it ends, friction eases from 0.3 x back to full | IW8 `slide_outTimeMs` and the slide-out friction scale |
| Off a ledge | ends after coyote time (0.1 s): standing and sprinting on, as a slide-cancel, or crouched under a low ceiling | IW8 ends a slide that loses the ground |

Every slide starts at the same speed, whatever the speed before it, so speed
never stacks. A slide-cancel (jump or crouch during the slide) stands up and
sprints on with the slide's speed, but only up to sprint speed, and restarts
the sprint clock. The cooldown runs only on the ground, so a jump cannot wait
it out. Chaining slides and cancels as fast as the buttons allow averages no
more than a plain sprint (`chained_slide_cancels_never_outrun_a_sprint`; on
the old rules the best chain averaged 1.08 x sprint).

A slide queued in the air is judged on the speed the player fell with, and it
replaces the landing: the slide's own entry speed takes over and the landing
recovery is cleared, as IW8's slide start clears the jump state. The earn rules
still hold, so a jump cannot skip them, and chains of jumps, air slides, and
cancels at every timing average at most 1.01 x sprint
(`jump_and_air_slide_chains_never_outrun_a_sprint`). Pressing crouch again in
the air takes the queued slide back; crouch in the air without a sprint still
crouches at once. The wind-down follows any slide's end, so its speed fades
into the crouch instead of snapping to it. New state:
`MoveState::slide_queued` and `MoveState::slide_out`. IW8's numbers for these
are not public; ours are tuned by feel and measured in the tests.

## Mantle and vault

| | Here | Reference |
| --- | --- | --- |
| Mantle | ledges 0.5 to 1.35 m above the feet, deep enough to stand on, 0.22 s + 0.2 s per metre | climb onto anything chest high |
| Trigger | a jump press at it, or jump held in the air (a jumped-at ledge too high from the ground climbs once in reach) | hold jump to mantle |
| Low ledge | room only to crouch on top: the climb ends crouched | crouch-height mantles |
| Vault | thin cover or a window sill 0.5 to 1.2 m high, with ground within 1.6 m below on the far side, 0.18 s + 0.15 s per metre | vault over thin cover and through windows |
| Vault speed | keeps the run's speed (at least 3 m/s, at most a sprint) and drops off the far side without a landing penalty | keeps momentum |
| Window | ducks through when a standing body would not fit (and only when a crouched one does), standing up on the far side once clear | |

The engine looks for what is climbable (`body::find_climb`): a wall within
0.75 m at knee height, its top just past the face, and whether the top runs
on (a ledge, with stand or crouch room above it) or drops away again (thin
cover, with clear room over it and ground beyond). The simulation picks a
mantle over a vault and climbs at a velocity fixed when the climb starts
(`MoveState::mantle_dir`), so turning mid-climb does not pull the player off
the wall. `MoveState::vaulting` marks a vault, which carries on over the top
and ducks through windows at slide speed.

## Weapon mount

Modern Warfare (2019) and its successors' weapon mount (`rust/sim/src/mount.rs`).

| | Here | Reference |
| --- | --- | --- |
| Mount | aim at cover and press melee (V, or the controller's melee) | ADS + Melee, the default activation |
| Cover | a level top 0.55 m ahead, from 0.82 m (a crouched body's gun) to 1.60 m (a standing body's) above the feet, its face within 0.65 m of the body, clear air 6 cm over it for 1.2 m: ledges, barriers, sandbag walls, windowsills | flat surfaces at crouching or standing height |
| Cue | `[V] Mount` in the interaction prompt while aimed at cover | an icon on the crosshair |
| Height | the eye moves to 8 cm over the cover (`REST`) on the crouch blend, so the gun rests on the edge: the body rises from a crouch or sinks from standing | the view lowers or rises onto the surface |
| Effect | kick 35% (`KICK_SCALE`), aimed sway 15% (`SWAY_SCALE`), both settling in over 0.18 s; the first-person weapon thumps onto the edge and its idle breathing and look lag nearly stop | most recoil removed (players measure 70 to 90% less) and nearly all idle sway |
| View | stops 40 degrees either side of the mount's yaw, 20 up and 30 down; pushing on about 11 degrees more goes through and lets go | the view stops at the mount's limits |
| Leave | let go of aim, move, jump, crouch, sprint, press melee again, switch weapons, throw, or turn through the edge; the press does its own thing too (a crouch crouches) | the same exits (MWII calls them "Weapon Mount Exit") |
| Reload | keeps the mount | reloads mounted |

The engine looks for cover only while aiming or mounted, along the mount's
own yaw once mounted (`body::find_mount`, three rays a tick), and reports its
height (`MoveEnv::mount`); client and server find the same. Because the mount
moves the eye on the same crouch blend as a stance change, the camera, the
shot origin, the capsule, and the hit zones agree. New state:
`MoveState::mounted`, `mount_yaw`, `mount_crouch`, and `mount_blend`;
snapshots carry the blend (`PlayerView::mount`), and other players' bodies
lean 4 degrees onto the gun (cosmetic, inside the hit zones' fit).
The Testing Area's 1.2 m concrete barriers, its 0.84 m sandbag walls (mounted
crouched), and windowsills mount; the culvert's 0.44 m sandbags are prone
height, and the tall walls' ends would need a mount on a wall's side (left
for later). `godot/tests/weapon_mount_test.gd` checks each case.

## Ladders

Classic IW and Modern Warfare (2019) ladders (`rust/sim/src/movement/ladder.rs`).

| | Here | Reference |
| --- | --- | --- |
| Grab | move into the rungs from up to 0.45 m in front, on the ground or in the air; or walk off the floor at its top toward its open side | walk into it; step onto it from the top |
| Climb | toward the rungs climbs up at 2.0 m/s, away from them climbs down at 2.4 m/s, full speed within 60 degrees of straight at or away; no gravity, no strafe | the TODO's 1.5 to 2 m/s band (about a rung every 0.15 s) |
| Top | the feet climb 6 cm over the top floor, then step onto it over 0.32 s (a 4.14 m ladder takes 2.1 s, rung to floor) | climbs over the top on its own |
| Bottom | standing on the floor without climbing up lets go | steps off at the bottom |
| Slide down | crouch drops at up to 6 m/s (accelerating at 14 m/s²); climbing up or crouch again catches it; no fall damage | MW2019's crouch-to-slide down ladders |
| Jump | pushes off backward (3 m/s back, 3 m/s up), no regrab for 0.4 s | jump lets go |
| Weapon | lowered (no fire, aim, knife, or grenades; a reload carries on), back up over the weapon's sprint-out | lowered on ladders |
| Look | free; the body keeps facing the rungs, turning at most 90 degrees at the hips | classic IW caps the look; we do not, so a player stepping on from the top is not turned sideways |

A map marks a ladder with a `LadderVolume` (`rust/game/src/ladder.rs`): a
node at the ladder's foot on the face of its rungs, -Z pointing the way a
climber faces, with `length` set to the height of the floor at the top. It
builds its own catch volume on the "ladders" physics layer (9), and
`body::tick` reports where the feet are relative to it (`MoveEnv::ladder`):
one point query a tick. The ladder's own collision must stop at the top
floor's height so a climber can step over it. New state:
`MoveState::ladder_facing`, `ladder_slide`, and `ladder_cooldown`;
snapshots carry the ladder's heading (`PlayerView::ladder`), and the hit
zones and the third-person body face its rungs (`gait::Turning::hold`).
Other bodies put their hands and feet on the rungs in diagonal pairs, two
rungs a move (`ladder::limb_targets`), with the weapon put away; a rung rings
under each 0.3 m climbed. The Testing Area's tower has the first ladder.

## Collision and stepping

| | Here | Reference |
| --- | --- | --- |
| Step height | 0.45 m (was 0.3) | classic IW 18 in (0.46 m) |
| Stairs at an angle | climbed at any angle the input pushes into the risers (tested 0, 45, 60 degrees) | no catching on risers |
| Going down stairs | stays on the ground at full speed; the camera follows without snapping | grounded |

Engine side, in `body::tick`:

- **Step up.** A grounded move blocked by anything too steep to stand on
  (including the slanted contact where the capsule's round bottom meets a low
  riser's edge) is retried up, over, and down, toward the blocking surface's
  normal rather than along the velocity, so a player running along a riser at
  a shallow angle still steps up. When a probe past the capsule's rim sees the
  tread after next, the step goes only to the tread under the body.
- **Step down.** Walking off a stair's edge, the capsule's round bottom rolls
  on a contact too steep to count as floor, and the engine's floor snap gives
  up. When the ground just past the front rim is lower and walkable ground lies
  within a step below, the body slides down the edge at the rate the steepest
  walkable slope falls, and stays grounded, so stairs never count as a fall or
  a landing. The floor snap follows 0.45 m drops.
- **Camera.** Rises and drops the floor makes faster than any walkable slope
  could are eased out of the camera (`TickOutcome::step_height`, signed).
- The server's player bodies (`body::build_body`) collide with exactly what the
  client's (`scenes/player.tscn`) do, including player-only blockers on layer
  7 (`body::PLAYER_COLLISION_MASK`).

## Camera

| | Here | Reference |
| --- | --- | --- |
| Walk bob | 2.2 cm, lowest just after each footfall, 1.2 cm sway and 0.23 degree roll alternating feet | a few cm, a dip at each step |
| Jump landing | 5 cm dip and a 1.4 degree nod, back in about 0.35 s | about 4 to 6 cm and 1 to 2 degrees over about 0.3 s |
| Hit flinch | away from the shot: a hit from the right turns the head left and rolls it; from in front tips it up, from behind down | directional |
| Death | our own body falls as a ragdoll; the camera pulls back and up from the eyes to 1.6 m high and 2.6 m behind it, watching it fall for 1.1 s, then turns to the killer with the body in the bottom of the frame | third-person death showing your own body, then a killcam |

The bob runs on the footstep stride phase (`footsteps::Stride`), with the
foot alternating each step so the sway and roll are continuous. Changing gait
mid-stride keeps the phase, so neither the bob nor the weapon's stride motion
jumps. The landing nod and the flinch turn the look itself and spring back,
so the screen centre, the sights, and the shots never disagree; the flinch's
roll is visual only. The death camera stops short of walls, follows the
body down a drop, and never dips toward the floor where the body lands. Our
own body (`FpsPlayer::leave_own_body`) is a soldier posed as we died
(stance, crouch, aim, weapon), drawn alive for one frame and then dead, so it
falls as a ragdoll with the killing hit's push and stays in the corpse yard
like anyone's. It shows once the camera is out of its head, and is never
built on a dedicated server.

A respawn starts every camera offset over: the death camera, landing dip and
nod, flinch, tilt, and bob. Look sensitivity follows the aim zoom only, not
sprint or slide FOV or kill pulses.

## Checking movement

`godot/tests/movement_test.gd` drives the real player body through scripted
inputs on a flat floor and checks the rules above (CI runs it). With
`MOVEMENT_TRACE_DIR` set it writes every tick as CSV, and
`uv run tools/plot_movement.py DIR/movement.csv OUT` plots speed, eye heights,
the crouch blend, and camera pitch and roll against the reference values.
