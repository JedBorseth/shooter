# Movement

How the player moves, and the Call of Duty reference each rule follows. The
rules live in `rust/sim/src/movement.rs` (`MoveParams` holds every number,
`MoveState` the per-tick state); ladders are `rust/sim/src/movement/ladder.rs`
and the weapon mount `rust/sim/src/mount.rs`. The engine side (collision,
step-up, ledges, cover, ladders) is `rust/game/src/body.rs` and
`rust/game/src/ladder.rs`, and the camera is `rust/game/src/player.rs`. The
client predicts every rule here and the server runs the same code, so any new
state goes into `MoveState` and its wire encoding
(`rust/sim/src/net/protocol.rs`).

The reference is Modern Warfare (2019) and MWII class movement (community
measurements and the classic IW engine values), with Black Ops 2's prone.
Numbers are approximate; the aim is the feel, not a copy.

## Ground and sprint

| | Here | Reference |
| --- | --- | --- |
| Walk | 5.0 m/s x the weapon's `move_scale` (0.88 LSW-6 to 1.05 pistols and SMGs) | rifle walk about 4.7 to 5 m/s |
| Crouch walk | 2.6 m/s x the same scale | about half a walk |
| Aimed | the walk x the weapon's `ads_move_scale` (0.4 M40 to 0.75 pistols), eased in with the aim | ADS movement speed per weapon |
| Rest to 95% walk | 0.23 s (accel 8, friction 6) | about 0.2 to 0.25 s |
| Walk to a stop | 0.28 s (stop speed 2.5) | about 0.25 to 0.3 s |
| Strafe, backpedal | 0.8 x, 0.7 x | classic IW 0.8, 0.7 |
| Sprint | 6.9 m/s x the weapon's `move_scale` | about 6.5 to 7 m/s |
| Walk to sprint | about 0.22 s (`sprint_in_time` 0.25 s) | about 0.2 to 0.3 s |
| Sprint to walk | the weapon's `sprint_out_time` (0.12 s pistols to 0.38 s LSW-6 and RL-4) | heavier guns are slower out |
| 45-degree diagonal sprint | 0.88 x sprint, turned mostly forward | no full speed on diagonals |

Sprint and crouch are toggles: a press starts a sprint, and it lasts until the
player stops pushing forward, crouches, fires, aims, starts a reload, or
knifes. Sprint is unlimited (no stamina): a recorded project decision, which
is also why there is no Extreme Conditioning perk. The speed cap follows
`MoveState::sprint_blend`, which builds over `sprint_in_time` while sprinting
and winds down over the weapon's sprint-out time once it ends, so the legs and
the gun come out of a run together. While sprinting the sideways input counts
for half, so a diagonal sprint runs mostly forward. Perks scale every ground
speed on top ([create-a-class.md](create-a-class.md)).

## Jump, air, and landing

| | Here | Reference |
| --- | --- | --- |
| Jump | 6.2 m/s up, gravity 19: about 1 m, 0.65 s | about 1 m |
| Jump buffer, coyote time | 0.1 s each | forgiving jumps |
| Touchdown after a full jump | keeps 0.7 x horizontal speed | about 0.65 x |
| Recovery | 1.2 s: speed cap from 0.75 x and jump from 0.75 x back to 1 | about 1.2 to 1.8 s |
| Air steering | up to 2.5 m/s toward the input (accel 4) | about half a walk |
| Air strafing | never adds speed | none |
| Fall damage | from a 12.5 m/s landing (about a 4 m drop), rising to a kill at 19.5 m/s (about 10 m) | IW fall damage between a minimum and a lethal height |

A landing counts by the time in the air: under 0.2 s (a curb, a stair) not at
all, a full count from 0.6 s. Repeated jumps start inside the last landing's
recovery, so each is lower and a bunny hop loses speed. Air steering turns the
velocity but never makes it longer than at takeoff (or the 2.5 m/s steer).
Fall damage comes from `HealthParams` in `rust/sim/src/player.rs`; Lightweight
takes none.

## Stance

| | Here | Reference |
| --- | --- | --- |
| Standing eye | 1.68 m (capsule 1.8 m) | IW 1.52 m (scaled to our taller capsule) |
| Crouched eye | 0.98 m (capsule 1.1 m) | IW 1.02 m |
| Crouch down | 0.25 s | about 0.2 to 0.3 s |
| Stand up | 0.3 s | about 0.25 to 0.35 s |
| Drop into a slide | 0.15 s | quick, under 0.2 s |

The stance itself flips at once (sprint, slide, and jump rules read it), but
the body follows a **crouch blend** (`MoveState::crouch`: 0 standing, 1
crouched or sliding, 2 prone) over the times above, eased with a smoothstep
(`crouch_ease`). That one blend drives the capsule height, the eye (the camera
and the shot origin on the client and the server), the speed cap while going
down, the hit zones (`hitbox::Activity::crouch`), and the third-person body
(`SoldierFrame::crouch`). Snapshots carry it as a byte
(`protocol::crouch_to_u8`). The camera draws the blend interpolated between
ticks, so it is never more than one tick from where shots leave.

Standing up needs headroom: a press to stand does nothing under a low
ceiling, and a body already rising holds where it is until the overhead is
clear. The headroom cast is a sphere
almost as wide as the capsule (`body::HEADROOM_RADIUS`, 0.34 m against
0.35 m), so a leaned-on wall is not a ceiling but an overhang's edge over the
shoulders is.

## Prone

Black Ops 2's third stance. No free motion capture or animation pack has a
belly crawl, so the prone pose and the low crawl are authored from the
soldier's idle pose (`tools/build_gait_animations.py`: `gait/Prone`, and
`gait/Prone_Crawl` with its `_R`, `_B`, and `_L` directions). Frame strips of
the side, backward, and diagonal crawls: [prone-crawls.jpg](media/prone-crawls.jpg).
Lying head down the Testing Area's 25° ramp, flat and then along it:
[prone-slope.jpg](media/prone-slope.jpg).

| | Here | Reference |
| --- | --- | --- |
| Get down | the prone button (Ctrl), or crouch held 0.3 s (C, or B on a controller): down through a crouch, 0.25 s to it and 0.45 s on (0.7 s) | Black Ops' `cl_stanceHoldTime` 300 ms; IW stance changes pass through a crouch |
| Get up | the prone button or a crouch held 0.3 s stands (0.5 s to a crouch, 0.3 s on); a crouch tap or jump rises to a crouch; a sprint press stands and runs | Black Ops: the held stance button stands from prone; jump raises the stance one step |
| Eye, capsule | eye 0.39 m (the drawn body's eyes), capsule 0.72 m (as wide as it is tall: the upper body) | IW prone viewheight 11 units (0.28 m), on a 30-unit box; scaled to our taller body |
| Body | lies along the aim with the eyes over the player's origin; the legs trail 1.45 m behind | IW's prone body: the box covers the upper body, the legs are checked behind it |
| Crawl | 0.8 m/s, any direction; aiming, the weapon's aimed speed scale slows it more | IW `player_proneSpeedScale` about 0.15 of a walk |
| Crawl animations | one each way, blended by the travel's direction: forward (the knees draw up in turn), backward (toes dug in, pulling the body back), and to each side (the leading knee draws up and out, the trailing leg drags, the hips fishtail) | the IW player animation set's prone crawl forward, back, left, and right |
| Room | lying down needs nothing solid within 1.45 m behind the feet at 0.2 m up (two rays); rising waits for headroom (to a crouch, then to standing) | Call of Duty's "can't go prone here" |
| Slope | the body lies along the ground: the server finds the ground under the chest and 1.1 m behind under the knees (two rays) and sends the slope with the body (`PlayerView::prone_slope`, half degrees, up to 30°); the drawn body and its hit zones turn about the eyes to lie along it, and the aim is raised on the body by as much, so the gun still points where it aims | the IW engine pitches a prone body to the ground (`proneDirectionPitch` and `proneTorsoPitch` in the player state) |
| Weapon | lowered while getting down or up; crawling at the hip faster than 0.3 m/s lowers it (no fire): it rolls onto its side and rocks forward with each elbow pull while the support hand reaches ahead and drags ([first-person crawl](media/prone-crawl-first-person.jpg)); aimed, the crawl keeps the gun up | IW weapons' prone-in, prone-out, and crawl animations (Black Ops' `crawlInAnim`, `crawlForwardAnim`, `crawlOutAnim`); no fire until settled |
| Turning | the view turns freely while the legs have room; a turn that would sweep them into a wall stops where they last had room (checked in 7° steps, so a flick can't pass a thin wall), and legs already against one turn freely | the IW engine checks the prone body as it turns and holds the view (`BG_CheckProneTurned`) |
| Accuracy | hip cone 0.6 x (crouched 0.8 x), the crouch's steadier kick, aimed sway 0.4 x | IW `hipSpreadProneMin` under the ducked one; weapons' `idleProneFactor` about 0.4 |
| View | pitch held between 30° down and 50° up (`look::PRONE_PITCH_DOWN`, `PRONE_PITCH_UP`), eased in as the body goes down; the whole body turns with the aim | the ground and the neck bound the prone view |
| Not allowed | sprint, slide, jump, mantle, vault, ladder grab, melee lunge (the knife still swings); off a ledge the body comes up to a crouch | |

Black Ops 2 has no dive to prone (Black Ops 1's dolphin dive), so there is
none here. The blend runs on from a crouch (1 to 2), so the capsule, the
camera, the shot origin, the hit zones (`hitbox::poses::PRONE`, and
`PRONE_CRAWLS`, measured every 45° of travel), the third-person body, and the
speed cap all follow one curve. The stance hold lives in
`MoveState::crouch_hold`, `hold_from_prone`, and `prone_held`; the input
carries the prone button. Footsteps prone play at the crouched step's quiet
level. Hard bots drop-shot (go prone in a close fight); see [bots.md](bots.md).
`godot/tests/prone_test.gd` checks the timings, heights, crawl, holds, and
room; `net_test.gd` predicts prone online with no corrections.

## Slide

| | Here | Reference |
| --- | --- | --- |
| Entry speed | 1.1 x sprint x the weapon's `move_scale` (7.6 m/s with an M4A1) | about sprint speed, at most 1.1 x |
| Duration | 0.65 s (holds speed 0.15 s, then friction 1.4) | about 0.6 to 0.7 s |
| Distance | 3.5 to 4.5 m from a rifle sprint (tested) | about 3.5 to 4 m |
| To earn one | 0.3 s of unbroken sprint at 0.85 x sprint speed or more, and 0.5 s on the ground since the last | a short sprint |
| From the air | crouch while sprinting in the air queues one; it starts on landing if earned and the heading is within about 45° of the view, else the player lands crouched | IW8 `Slide_CanSlideFromAir`: a minimum air speed and a heading within an angle of the view |
| Steering | strafe turns it up to 1.4 rad/s (80° a second), keeping its speed; the view never steers it | IW8 locks the slide's direction, lets the view turn freely, and lets the strafe input bend it |
| Wind-down | 0.5 s after it ends, friction eases from 0.3 x back to full | IW8 `slide_outTimeMs` and the slide-out friction scale |
| Off a ledge | ends after coyote time (0.1 s): standing and sprinting on, as a slide-cancel, or crouched under a low ceiling | IW8 ends a slide that loses the ground |
| On a slope | friction scales by 1 - 2.5 x the slope's sine along the slide (none down a 25° ramp, 2 x up it), and gravity pulls at 0.3 x its share along it; never over 1.2 x the entry speed. Down a 25° ramp a slide runs 1.2 to 1.5 x as far as on the flat; up it, 0.4 to 0.85 x (`a_slide_runs_on_downhill_and_dies_uphill`) | IW8 `Slide_FrictionScale`: slides run on downhill and die uphill (its numbers are not public) |

Every slide starts at the same speed, whatever the speed before it, so speed
never stacks. A slide-cancel (jump or crouch during the slide) stands up and
sprints on with the slide's speed, but only up to sprint speed, and restarts
the sprint clock; the next jump press jumps. The cooldown runs only on the
ground, so a jump cannot wait it out.

A slide queued in the air is judged on the speed the player fell with, and it
replaces the landing: the slide's own entry speed takes over and the landing
recovery is cleared, as IW8's slide start clears the jump state. The earn
rules still hold, so a jump cannot skip them. Pressing crouch again in the air
takes the queued slide back; crouch in the air without a sprint still crouches
at once. The wind-down follows any slide's end, so its speed fades into the
crouch instead of snapping to it.

Two tests hold every chain of slides, cancels, jumps, and air slides, at every
timing, to within 3% of a plain sprint's average speed
(`chained_slide_cancels_never_outrun_a_sprint`,
`jump_and_air_slide_chains_never_outrun_a_sprint`). The engine reads the slope
from a ray under a sliding body's feet (`MoveEnv::floor_normal`), so a replay
from a placed body sees the floor the first run did. IW8's numbers for these
are not public; ours are tuned by feel and measured in the tests.

## Mantle and vault

| | Here | Reference |
| --- | --- | --- |
| Mantle | ledges 0.5 to 1.35 m above the feet, deep enough to stand on, 0.22 s + 0.2 s per metre | climb onto anything chest high |
| Trigger | a jump press at it, or jump held in the air (a jumped-at ledge too high from the ground climbs once in reach) | hold jump to mantle |
| Low ledge | room only to crouch on top: the climb ends crouched | crouch-height mantles |
| Vault | thin cover or a window sill 0.5 to 1.2 m high, with ground within 1.6 m below on the far side, 0.18 s + 0.15 s per metre | vault over thin cover and through windows |
| Vault speed | keeps the run's speed (at least 3 m/s, at most a sprint) and drops off the far side without a landing penalty (0.4 s grace) | keeps momentum |
| Window | ducks through when a standing body would not fit (and only when a crouched one does), standing up on the far side once clear | |

The engine looks for what is climbable (`body::find_climb`): a wall within
0.75 m at knee height, its top just past the face, and whether the top runs on
(a ledge, with stand or crouch room above it) or drops away again (thin cover,
with clear room over it and ground beyond). The simulation picks a mantle over
a vault and climbs at a velocity fixed when the climb starts
(`MoveState::mantle_dir`), so turning mid-climb does not pull the player off
the wall. `MoveState::vaulting` marks a vault, which carries on over the top
and ducks through windows at slide speed. Dexterity climbs in 60% of the time.

Other players see a vault as one (`PlayerView::vaulting`, set by
`MoveState::in_vault` from the start of the rise until the drop off the far
side lands): the run's last stride drives the right knee up, the support hand
leaves the lowered rifle to plant on the top, the hips turn and roll toward
the hand while both legs tuck and swing over, and the feet reach down ahead
for the landing (`gait/Vault`, authored by `tools/build_gait_animations.py`).
A mantle plays the climb. The hit zones pose the same vault, hand plant
included (`hitbox::action::vault_plant`).

## Weapon mount

Modern Warfare (2019) and its successors' weapon mount (`rust/sim/src/mount.rs`).

| | Here | Reference |
| --- | --- | --- |
| Mount | aim at cover and press melee (V, or R3 on a controller) | ADS + Melee, the default activation |
| Cover | a level top 0.55 m ahead, from 0.82 m (a crouched body's gun) to 1.60 m (a standing body's) above the feet, its face within 0.65 m of the body, clear air 6 cm over it for 1.2 m: ledges, barriers, sandbag walls, windowsills | flat surfaces at crouching or standing height |
| Prone | prone, low cover from 0.23 m (a prone body's gun) to 0.90 m (a crouched one's): the body props itself up onto it on the crouch blend's prone half, and lies back down on letting go | MWII's prone mount on low cover |
| A wall's side | with no top in reach, a face square to the aim 0.3 m to one side within 0.65 m, and the line of fire past it clear for 1.85 m: the gun rests against the wall's edge at the height it is at | mounting on the vertical edge of a wall or a corner |
| Cue | `[V] Mount` in the interaction prompt while aimed at cover | an icon on the crosshair |
| Height | the eye moves to 8 cm over the cover (`REST`) on the crouch blend, so the gun rests on the edge: the body rises from a crouch or sinks from standing | the view lowers or rises onto the surface |
| Effect | kick 35% (`KICK_SCALE`), aimed sway 15% (`SWAY_SCALE`), both settling in over 0.18 s; the first-person weapon thumps onto the edge and its idle breathing and look lag nearly stop | most recoil removed (players measure 70 to 90% less) and nearly all idle sway |
| View | stops 40° either side of the mount's yaw, 20° up and 30° down; pushing on about 11° more (`MOUNT_PUSH_OUT` in `player.rs`) goes through and lets go | the view stops at the mount's limits |
| Leave | let go of aim, move, jump, crouch, sprint, press melee again, switch weapons, throw, or turn through the edge; the press does its own thing too (a crouch crouches) | the same exits (MWII calls them "Weapon Mount Exit") |
| Reload | keeps the mount | reloads mounted |

The engine looks for cover only while aiming or mounted, along the mount's
own yaw once mounted (`body::find_mount`, three rays a tick), and reports its
height (`MoveEnv::mount`); client and server find the same. Because the mount
moves the eye on the same crouch blend as a stance change, the camera, the
shot origin, the capsule, and the hit zones agree. The state is
`MoveState::mounted`, `mount_yaw`, `mount_crouch`, and `mount_blend`;
snapshots carry the blend (`PlayerView::mount`), and other players' bodies
lean 4° onto the gun (cosmetic, inside the hit zones' fit). The Testing
Area's 1.2 m concrete barriers, its 0.84 m sandbag walls (mounted crouched),
windowsills, the culvert's 0.44 m sandbags (mounted prone), and the tall
walls' ends (on their sides) mount. `godot/tests/weapon_mount_test.gd` checks
each case.

## Ladders

Classic IW and Modern Warfare (2019) ladders (`rust/sim/src/movement/ladder.rs`).

| | Here | Reference |
| --- | --- | --- |
| Grab | move into the rungs from up to 0.45 m in front, on the ground or in the air; or walk off the floor at its top toward its open side | walk into it; step onto it from the top |
| Climb | toward the rungs climbs up at 2.0 m/s, away from them climbs down at 2.4 m/s, full speed within 60° of straight at or away; no gravity, no strafe. Dexterity climbs 1.75 x as fast | about a rung every 0.15 s, 1.5 to 2 m/s |
| Top | the feet climb 6 cm over the top floor, then step onto it over 0.32 s (a 4.14 m ladder takes about 2.1 s of climbing) | climbs over the top on its own |
| Bottom | standing on the floor without climbing up lets go | steps off at the bottom |
| Slide down | crouch drops at up to 6 m/s (accelerating at 14 m/s²); climbing up or crouch again catches it; no fall damage | MW2019's crouch-to-slide down ladders |
| Jump | pushes off backward (3 m/s back, 3 m/s up), no regrab for 0.4 s | jump lets go |
| Weapon | lowered (no fire, aim, knife, or grenades; a reload carries on), back up over the weapon's sprint-out | lowered on ladders |
| Look | free; the body keeps facing the rungs, turning at most 90° at the hips | classic IW caps the look; this does not, so a player stepping on from the top is not turned sideways |

A map marks a ladder with a `LadderVolume` (`rust/game/src/ladder.rs`): a node
at the ladder's foot on the face of its rungs, -Z pointing the way a climber
faces, with `length` set to the height of the floor at the top. It builds its
own catch volume on the "ladders" physics layer (9), and `body::tick` reports
where the feet are relative to it (`MoveEnv::ladder`): one point query a tick.
The ladder's own collision must stop at the top floor's height so a climber
can step over it. The state is `MoveState::ladder_facing`, `ladder_slide`,
and `ladder_cooldown`; snapshots carry the ladder's heading
(`PlayerView::ladder`), and the hit zones and the third-person body face its
rungs (`gait::Turning::hold`).

Other bodies put their hands and feet on the rungs in diagonal pairs, two
rungs a move (`ladder::limb_targets`), with the weapon put away; a rung rings
under each 0.3 m climbed. In first person the hands do the same as the weapon
goes away: `ViewmodelRig` finds the rungs from the feet and moves each hand's
hold into the first-person projection (`effects::world_to_viewmodel`), so the
hands show over the rungs whatever the field of view. Every climber's hands
close round the rung, backs of the hands out (`poser::RUNG_GRIP`). The Testing
Area's tower has a ladder.

## Collision and stepping

| | Here | Reference |
| --- | --- | --- |
| Step height | 0.45 m (`body::STEP_HEIGHT`) | classic IW 18 in (0.46 m) |
| Stairs at an angle | climbed at any angle the input pushes into the risers (tested 0, 45, 60°) | no catching on risers |
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
| Walk bob | 2.2 cm, lowest just after each footfall, 1.2 cm sway and 0.23° roll alternating feet | a few cm, a dip at each step |
| Jump landing | about 5 cm dip and a 1.5° nod, more for a longer drop, back in about 0.35 s | about 4 to 6 cm and 1 to 2° over about 0.3 s |
| Hit flinch | away from the shot: a hit from the right turns the head left and rolls it; from in front tips it up, from behind down | directional |
| Death | our own body falls as a ragdoll; the camera pulls back and up from the eyes to 1.6 m high and 2.6 m behind it, watching it fall for 1.1 s, then turns to the killer with the body in the bottom of the frame | third-person death showing your own body, then a killcam ([killcam.md](killcam.md)) |

The bob runs on the footstep stride phase (`footsteps::Stride`), with the foot
alternating each step so the sway and roll are continuous. Changing gait
mid-stride keeps the phase, so neither the bob nor the weapon's stride motion
jumps. The bob and the strafe tilt follow the Camera motion setting
([settings.md](settings.md)). The landing nod and the flinch turn the look
itself and spring back, so the screen centre, the sights, and the shots never
disagree; the flinch's roll is visual only (Toughness cuts the flinch). The
death camera stops short of walls, follows the body down a drop, and never
dips toward the floor where the body lands. Our own body
(`FpsPlayer::leave_own_body`) is a soldier posed as we died (stance, crouch,
aim, weapon), drawn alive for one frame and then dead, so it falls as a
ragdoll with the killing hit's push and stays in the corpse yard like
anyone's. It shows once the camera is out of its head, and is never built on a
dedicated server.

A respawn starts every camera offset over: the death camera, landing dip and
nod, flinch, tilt, and bob. Look sensitivity follows the aim zoom only, not
sprint or slide FOV or kill pulses.

## Checking movement

`godot/tests/movement_test.gd` drives the real player body through scripted
inputs on a flat floor and checks the rules above (CI runs it). With
`MOVEMENT_TRACE_DIR` set it writes every tick as CSV, and
`uv run tools/plot_movement.py DIR/movement.csv OUT` plots speed, eye heights,
the crouch blend, and camera pitch and roll against the reference values.
`rust/sim/src/movement.rs` and `movement/ladder.rs` hold the unit tests for
each rule.
