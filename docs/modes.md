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

**Zones.** Each map lists its zones in rotation order under a `Hardpoints`
node (a `Marker3D` per zone, its box in `size`, its HUD name in `label`).
Following Call of Duty's maps, the first zone is in the middle, the rotation
then swings from one side of the map to the other so neither team's spawn
gets two near zones in a row, and every move goes at least 16 m to a zone
whose footprint does not touch the last one's
(`godot/tests/hardpoint_zones_test.gd` checks this on every map, and that
most of each zone's footprint has floor inside its box).

| Map | Rotation |
| --- | --- |
| Slamcity | Junction, Pawn Shop, The Platforms, Ticket Hall |
| Testing Area | Crossroads, The Platform, Tunnel, Target Range |
| Nuketown | Cul-de-sac, West House, Front Lot, East House |
| Warehouse | Mid Stockpile, Shipping, South Crossover, North Crossover, Receiving |
| Breakwater | Loading Rack, Pump House, Rack Deck, Fire Pumps |

A map without `Hardpoints` still gets one zone halfway between the teams'
spawns.

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

Reference game: **Black Ops 2**, as for Domination ([domination.md](domination.md)).

| Rule | Here | Call of Duty |
| --- | --- | --- |
| Flags | Take the enemy's flag from their base or where it lies; capture at your own base, only while your own flag is home | Same since Call of Duty 4's CTF |
| Returns | Touch your own dropped flag to send it home at once; left alone it goes home after 30 s | Black Ops 2 returns on touch; dropped flags reset on their own after a timeout (about 30 s, not sourced exactly) |
| Limit | 3 captures or 10 minutes | 3 captures; Black Ops 2 plays two 5-minute halves |
| Halftime | At half the clock the teams swap bases, both flags go home, everyone starts again from their new side, and the score carries over (`ctf::halftime_due`, `Flags::set_sides`) | Black Ops 2: two rounds with the sides switched |
| Overtime | Level when the clock runs out: an overtime round as long as a half, from the first half's bases, where the next capture wins; no capture is a draw | Black Ops 2 plays overtime rounds with the sides switched each round (David Vonderhaar, Gameranx, 22 October 2012) |

**Differences.** Black Ops 2's League Play settles a draw with a two-round
stopwatch: a capture in the first overtime round sets the time to beat in the
second. That is a competitive option, not the public rule, so it is not used
here: one sudden-death round is.

## Gun Game

| Rule | Here | Call of Duty |
| --- | --- | --- |
| Ladder | Eight weapons (`gun_game::LADDER`), two kills with each | Black Ops: 20 weapons, one kill each; Modern Warfare (2019) onward: 18 to 20. Shorter here because the game has eight guns |
| Knife | A knife kill sets the victim back a weapon and does not move the killer up | Black Ops "Humiliation": a melee or ballistic knife kill sets the victim back |
| Suicide | Dying by your own hand (a fall) sets you back a weapon | Same in Black Ops |
| Calls | A new weapon shows its name in the middle of the screen with a step-up sting; a set-back shows SET BACK with a step-down sting; anyone reaching the last weapon is called in the kill feed | Call of Duty's announcer calls each upgrade and each humiliation; the stings stand in until there is a voice |
| End | The last weapon's kills win; at the buzzer, the player furthest up wins | Same |

**Differences.** Black Ops ends its ladder on the crossbow and the ballistic
knife, and later games on a melee weapon; here the last weapon is the M1911,
because the game has no crossbow or throwing knife yet.

## Calls across modes

Every mode calls the same moments the same way: the countdown banner names
the mode and its limit, halftime (Capture the Flag, Domination) and overtime
(Hardpoint, Capture the Flag) come from the match period through the match
HUD with their stings, round results in Search and Destroy use the same
banner, and the result is VICTORY, DEFEAT, or DRAW (a player's name in a
free-for-all) with the win, loss, or draw sting.
