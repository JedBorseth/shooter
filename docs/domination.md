# Domination

Two teams fight over three flags, A, B, and C (`--mode dom`,
`rust/sim/src/domination.rs`). The reference game is **Black Ops 2**
(Treyarch, in line with the project's graphics target), played the classic
way with no twists.

## Rules

| Rule | Here | Call of Duty |
| --- | --- | --- |
| Flags | Three: A by team 0's spawn, B in the middle, C by team 1's | Same in every game since Call of Duty 4 |
| Capture | Stand in a flag's circle for 10 s (`DominationRules::capture_seconds`) | 10 s in Call of Duty 4 and Black Ops 2 (`dom.gsc`) |
| More capturers | Each extra teammate adds one person's speed: 2 take 5 s, 3 take 3.3 s | The scripts' use rate is the number of claimants (CoD4 and Black Ops 2 `gameobjects.gsc`); later games cap the speed-up |
| Enemy flags | Take as long as neutral ones, in one step | Same in Black Ops 2; Modern Warfare (2019) onward first neutralises an enemy flag, which this project does not |
| Contested | Both teams in the circle: the capture stalls and keeps its progress | Same ("progress is stalled until only one team is left on the flag", Modern Warfare beta notes, 2019) |
| Leaving | The capturing team walking out, or the owners stepping on, starts the capture over | Same in Black Ops 2; Black Ops 3 onward lets progress decay slowly |
| Team score | Every 5 s, a point per owned flag (`point_seconds`) | Same: one point per flag every five seconds |
| Score limit | 200 | 200 in Black Ops 2 and Modern Warfare (2019) |
| Time limit | 10 minutes in two halves | Black Ops 2 plays two rounds of 5 minutes |
| Halftime | At half the clock the teams swap sides, every flag goes back to neutral, and everyone respawns at their new side; the score carries over (`halftime_due`), and the match moves to its second half (`tdm::Period::SecondHalf`) | Same: Black Ops 2's second round starts from neutral flags with the sides swapped |
| Draw | The team ahead when the clock runs out wins; level is a draw | Same in Black Ops 2; overtime belongs to later games |

Flags are circles 3.5 m in radius and 4 m tall
(`DEFAULT_RADIUS`; Black Ops 2's flag triggers are about 120 to 160 units).
Feet up to 0.5 m below the floor still count, for uneven ground. Only living
players count. In the lobby's warm-up flags change hands for practice but
score nothing; they stop through the countdown and the results.

Kills score nothing for the team. Halftime needs a clock: with no time limit
there is one half.

## Player score

| Award | Points | When |
| --- | --- | --- |
| Flag Captured (medal) | 150 | Standing in the circle when the team takes the flag; every capturer gets it |
| Flag Defense | +50 on the kill | A kill with the killer or the victim on or within 5 m of a flag the killer's team owns |
| Flag Assault | +50 on the kill | The same near a flag the killer's team does not own (neutral or enemy) |

Captures and defences count on the scoreboard (`ScoreState::captures`,
`ScoreState::defends`). Black Ops 2's exact score values could not be
confirmed from a source; these follow this project's Hardpoint and Capture
the Flag scale (a kill is 100).

## Spawns

After the first spawn at each side, spawns follow the flags
(`DominationState::spawn_threats`): each flag a team owns counts as three
teammates at its circle, each enemy flag as an enemy there, and the side of
the map only breaks ties (`SpawnRules::for_mode`). A team that takes the
enemy's home flag and loses its own flips the spawns, as in Call of Duty.
The general spawn rules are in [spawns.md](spawns.md).

## Bots

Bots play the objective (`domination::bot_goals`, `NetServer::dom_objectives`):

- The two nearest bots run back to stop an enemy capture of a flag their
  team owns.
- A bot within 6 m of the edge of a flag its team does not own takes it,
  whatever else it was doing.
- One bot in four guards a flag the team owns, holding spots inside its
  circle so an enemy capture is contested.
- The rest take the flags the team does not own: the nearest, with every
  third attacker going for the next nearest, so a team splits over two flags
  (at the start, its home flag and B).
- With all three flags, everyone guards.
- Within 8 m of its flag a bot holds a spot inside the circle (a new one
  every 4 s); further away it hunts its way there, fighting what it meets.

Black Ops 2's bots (`bot_dom_think`) play the same way: a bot at a flag it
does not hold takes it, the rest go for the nearest flags they do not hold,
and a few go back when one is being taken. See [bots.md](bots.md).

## Network

The server decides every capture. It sends the map's flags on joining
(`GameEvent::DominationFlags`) and every flag's owner, claim, progress,
contest, and capturer count whenever any of those but the progress changes
(`GameEvent::Domination`). Clients run a capture on at its known speed
between events (`NetClient::domination_info`).

## On screen

`godot/scripts/net/domination_view.gd`, as Black Ops 3 marks its flags:

| Piece | Where | What |
| --- | --- | --- |
| Flag row | Under the minimap | A, B, C as lettered hexagons in the owner's colour (sand while neutral, flashing between the two sides twice a second while contested), with a ring filling round each in the capturing side's colour |
| Markers | Over each flag | The same hexagon with CAPTURE or DEFEND over it and the distance under it, held at the screen's edge when off screen; none for the flag you stand in |
| Capture bar | Below the crosshair, in a circle | CAPTURING B, CONTESTED, LOSING B (the enemy's capture of your flag; standing on it alone stops it), or B SECURED, over a 260 px bar filling with the capture |
| World | Each flag | A ring decal at the circle's edge and a low wall of light around it, in the owner's colour (a pulse when it changes) |
| Calls | Upper third | WE'VE TAKEN B, WE'VE LOST B, and ENEMY TOOK B, each with a short chime (there is no announcer); the match HUD calls HALFTIME ("Switching sides") for the second half ([modes.md](modes.md)) |
| Scoreboard | Tab | CAPS and DEF columns |

Black Ops 2 and 3 show the same pieces: the flags' state near the minimap,
a world icon over each flag (Black Ops 3's are hexagons with the action word
over them), a capture bar in the middle while you capture, and the
announcer's "we've taken / we've lost" lines. The hexagons are drawn in
code, not Black Ops 3's icons.

## Maps

A map marks its flags with `Node3D` children of a `DominationFlags` node,
each placed at the circle's floor centre, with a `label` metadata (A, B, C)
and an optional `radius` metadata. As in Call of Duty, A and C sit near
each team's spawn and B in the middle:

| Map | A (team 0's side) | B | C (team 1's side) |
| --- | --- | --- | --- |
| Breakwater | The west main road, between the pickup and the analyser shelter | The crossing under the loading rack's deck (3 m circle) | A's twin on the east main road |

Breakwater's are in `DOM_FLAGS` in `tools/breakwater_layout.py` (A and C are
twins).

Maps without flags get three between the teams' spawns
(`domination::fallback_points`): on the line between them (A at 22%, B at
50%, C at 78%) when the sides are at least 40 m apart, otherwise across the
middle 13 m apart (the testing arena).

Not built yet: an announcer, a flag pole model at each flag, and overtime;
see [TODO.md](../TODO.md).
