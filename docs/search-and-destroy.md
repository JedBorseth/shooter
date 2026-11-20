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

## Still to come

Objective bots, plant and defuse scores and medals (Clutch, Ace, Wire
Cutter), the charge's accelerating beeps, its explosion with a damage radius,
and round stings for plants and defuses: see the Search and Destroy section of
[TODO.md](../TODO.md).
