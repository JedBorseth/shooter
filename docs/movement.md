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
and ducks through windows at slide speed. Ladders and weapon mounting are not
done: no map has ladders yet.

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
| Death | drops with the body for 0.4 s, then rises 1.5 m high and 2.4 m behind where the player fell, facing the killer | third-person death, then a killcam |

The bob runs on the footstep stride phase (`footsteps::Stride`), with the
foot alternating each step so the sway and roll are continuous. Changing gait
mid-stride keeps the phase, so neither the bob nor the weapon's stride motion
jumps. The landing nod and the flinch turn the look itself and spring back,
so the screen centre, the sights, and the shots never disagree; the flinch's
roll is visual only. The death camera stops short of walls.

A respawn starts every camera offset over: the death camera, landing dip and
nod, flinch, tilt, and bob. Look sensitivity follows the aim zoom only, not
sprint or slide FOV or kill pulses.

## Checking movement

`godot/tests/movement_test.gd` drives the real player body through scripted
inputs on a flat floor and checks the rules above (CI runs it). With
`MOVEMENT_TRACE_DIR` set it writes every tick as CSV, and
`uv run tools/plot_movement.py DIR/movement.csv OUT` plots speed, eye heights,
the crouch blend, and camera pitch and roll against the reference values.
