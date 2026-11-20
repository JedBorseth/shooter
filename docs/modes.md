# Objective modes: Hardpoint, Capture the Flag, Gun Game

The rules these modes follow and the Call of Duty behaviour each rule comes
from. Search and Destroy ([search-and-destroy.md](search-and-destroy.md)) and
Domination ([domination.md](domination.md)) have their own pages. The rules
live in `rust/sim` (`hardpoint.rs`, `ctf.rs`, `gun_game.rs`, and the match
clock and periods in `tdm.rs`); the server runs them
(`rust/game/src/net/server.rs`) and the clients draw what it sends.

## Match periods

Every match has a period that the server sends with the match state
(`tdm::Period`, in `GameEvent::Match`): regulation, the second half (modes
played in halves), or overtime. Overtime is either timed (its own clock) or
untimed, when the server ends it once the mode's condition lapses. The match
HUD calls each new period out in the middle of the screen with a short sting
(`match_stings.gd`), and the clock reads `OVERTIME` while untimed overtime
runs.

## Hardpoint

| Rule | Here | Call of Duty |
| --- | --- | --- |
| Zone | One live zone; a team alone in it scores a point a second; contested or empty scores nothing | Same in every Hardpoint since Black Ops 2 |
| Rotation | Every 60 s to the next zone in the map's fixed order, wrapping around | 60 s in Black Ops 2 onward public play and the competitive rules; zones go in a fixed order players learn |
| Limit | 250 points or 10 minutes | 250 points; Black Ops 6 public play |
| Overtime | When the clock runs out while a team that is not ahead has a living player in the live zone (holding or contesting), play goes on. The zone keeps scoring and keeps rotating. Overtime ends the moment no team that is not ahead stands in the zone; the team ahead then wins (a draw if level). A trailing team holding the zone alone can score past the leader and win | See below |

**Overtime.** This is the project's rule from the TODO ("overtime when the
trailing team is in the zone at the buzzer"). Call of Duty's public Hardpoint
ends at the buzzer, and the sources we could reach (official mode guides for
Modern Warfare III and Black Ops 6) do not describe a Hardpoint overtime, so
the details follow the rule's plain reading: holding and contesting both keep
it alive, and it has no clock of its own, since the zone still moves after its
minute and a team that is not standing in the new zone ends it.
`hardpoint::overtime_holds` decides it; `NetServer::hardpoint_overtime` starts
and ends it.

## Capture the Flag

To do: halves and overtime (TODO.md).

## Gun Game

To do: the weapon sting (TODO.md).
