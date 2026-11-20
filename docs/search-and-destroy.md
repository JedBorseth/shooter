# Search and Destroy

Rounds with one life each (`--mode snd`, `rust/sim/src/snd.rs`), played the
classic Call of Duty way with no twists. The owner chose the plain mode and
rejected a half-defuse checkpoint ([roadmap.md](roadmap.md) decisions).

## Rounds

- First to 6 rounds. The teams swap sides after round 5, so both attack five
  times before a decider (`SndRules::halftime_after`).
- Each round: a 5 s freeze at your side (look, aim, reload, change loadout;
  no moving or firing), then 1:45 of play, then 6 s of result.
- Nobody respawns during a round. The dead spectate a living teammate over
  the shoulder; players who join after the freeze wait for the next round.
- A round ends when a team is wiped out, when the clock runs out (the
  defenders win), when the charge goes off (the attackers win), or when it
  is defused (the defenders win). If both teams die in the same moment, the
  defenders take it.

## The charge

| Rule | Here | Call of Duty |
| --- | --- | --- |
| Carrier | A random living attacker at the start of each round | Same (Call of Duty 4 onward) |
| Dropped | Where its carrier dies, settled on the floor; a carrier who leaves drops it where they were | Same |
| Picked up | Any living attacker who walks over it (1.2 m); defenders cannot | Same: touch to pick up, attackers only |
| Plant | Hold interact for 4 s inside a site (`SndRules::plant_time`) | 5 s in Call of Duty 4, Black Ops 2 and 3, and WWII; this project chose 4 s |
| Fuse | 45 s; it replaces the round clock (`RoundState::plant`) | 45 s in every era |
| Defuse | Hold interact for 7 s within 1.6 m of the charge (`SndRules::defuse_time`) | 5 s stock, 7.5 s in competitive settings and Advanced Warfare |
| Interrupted | Letting go, moving off, or dying starts a plant or defuse over from nothing | Same; Modern Warfare (2019) keeps half a defuse, which this project does not |
| One at a time | A second defender holding interact waits until the first lets go | Same |
| After the plant | Wiping out the attackers no longer wins: the defenders must defuse. Wiping out the defenders wins at once | Same |
| While using | Held still, weapon put away (no firing, aiming, reloading, or switching); looking around is free | Same |
| Input | Hold interact (F; X / Square on a controller, the reload button, when standing where it would plant or defuse) | Hold Use (F; X / Square) |

Who knows what: the attackers see who carries it (an ESCORT marker over a
teammate, a PICK UP marker where it lies); the defenders are told nothing
about the carrier, though they see a dropped charge lying in the world, as in
Call of Duty. Everyone sees the planted site. Only a player's own team hears
of a plant or defuse under way (`GameEvent::Charge`), but everyone sees the
kneeling pose (`PlayerView::charge_use`).

The server decides every pickup, drop, plant, and defuse. Clients work out
from what it last sent whether holding interact would plant or defuse where
they stand (`Charge::spot`) and hold their own player still with the same
filter the server uses (`PlayerInput::using_charge`), so prediction agrees.

## Sites

A map marks its sites with `Marker3D` children of a `ChargeSites` node, each
a box centred on the marker (`size` metadata, full size) named by a `label`
metadata (A, B). Maps without them get two sites on the defenders' side of
the middle, one to each flank (`fallback_charge_sites` in the server).

Slamcity's (`CHARGE_SITES` in `tools/slamcity_layout.py`):

- **A**: the pawn shop's back room on the north street, east of the El.
- **B**: the ticket hall's front room on the market side.

Each has a solid marker crate with its letter on every side
(`gear_models.charge_site_a` and `_b`, built in Blender), which stands in
every mode as set dressing. The defenders reach either site in about 7 s at a
sprint, the attackers in about 11 s.

## Bots

Bots play the charge (`charge_objectives` in the server, through the bots'
objectives, as in Capture the Flag and Hardpoint):

- A bot standing where holding interact would plant or defuse holds it
  (`ObjectiveKind::Use`), looking about, and fights first if an enemy is in
  sight. The same filter as for players holds it still with its weapon away.
- Attackers: the carrier runs to the round's target site (picked at random
  each round) and plants; the others push into that site to clear it; the
  nearest attacker bot fetches a dropped charge; after the plant they hold
  spots round it.
- Defenders: split between the sites, holding spots in each. Once the charge
  is planted, the nearest runs to defuse it and the rest clear round it.

Call of Duty's bots (Black Ops 2 and 3 combat training) play it the same
way: the carrier goes to a site, the defence splits, and defenders rush the
defuse.

## Scores and medals

| Play | Points | Call of Duty |
| --- | --- | --- |
| Plant | 200 | Plant and defuse pay a few kills' worth in every era (Black Ops 2: Bomb Planted and Bomb Defused medals) |
| Defuse | 200 | as above |
| Wire Cutter | +100: a defuse with an attacker still alive | Call of Duty 4 and Black Ops 2's Ninja Defuse is for a defuse with enemies alive near it; ours is any attacker alive |
| Clutch | +200: win the round as your team's last one standing, against at least one enemy, from a team of two or more | the Last Man Standing / Clutch medals |
| Ace | +300: kill every enemy who started the round yourself (at least three) | the Ace of later titles and esports |

Kills score personal points as usual, but never a team point: only rounds
do. Plants and defuses show on the scoreboard in the PLANT and DEF columns.

## The explosion

When the fuse runs out the charge goes off: everyone within 13 m with a clear
line takes 200 damage at its heart, falling to 20 at the edge, as Call of
Duty 4's `radiusDamage(origin, 512, 200, 20)`. Kills are credited to the
planter (none to their own team, as with every blast) and show as "Charge"
in the kill feed. Clients draw the frag grenade's blast, under the charge's
own explosion sound.

## Sound

A sting when the charge is planted, its beep speeding up as the fuse runs
down, and a close explosion: see [audio.md](audio.md).

## Still to come

See the Search and Destroy section of [TODO.md](../TODO.md).
