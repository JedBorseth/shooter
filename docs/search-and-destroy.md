# Search and Destroy

Rounds with one life each (`--mode snd`, `rust/sim/src/snd.rs`), played the
classic Call of Duty way with no twists. The owner chose the plain mode and
rejected a half-defuse checkpoint ([roadmap.md](roadmap.md) decisions). The
match shell (lobby, countdown, results, scoring) is in [modes.md](modes.md).

## Rounds

- First to 6 rounds (`--rounds N`). The teams swap sides after one round
  short of the target, so in a 6-round match both attack five times before
  a decider (`SndRules::halftime_after`). Team 0 attacks first.
- Each round: a 5 s freeze at your side (look, aim, reload, change class;
  no moving or firing), then 1:45 of play, then 6 s of result. There is no
  match clock.
- Nobody respawns during a round, and a death plays no killcam. After 1.5 s
  the dead watch a living teammate over the shoulder, and fire steps to the
  next one (`godot/scripts/net/snd_hud.gd`). Players who join after the
  freeze wait for the next round.
- A round ends when a team is wiped out, when the clock runs out (the
  defenders win), when the charge goes off (the attackers win), or when it
  is defused (the defenders win). If both teams die in the same moment
  before a plant, the defenders take it.
- A round won by wiping out the other team replays its last kill for
  everyone: ROUND WINNING KILL, or the final killcam when it wins the match
  ([killcam.md](killcam.md)).

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
| After the plant | Wiping out the attackers does not win: the defenders must defuse. Wiping out the defenders wins at once | Same |
| While using | Held still, weapon put away (no firing, aiming, reloading, or switching); looking around is free. In first person the charge comes up into view in the hands: planting, the left hand steadies the case and the right index finger taps its keypad; defusing, the case is turned round with its detonator block on the right, the left hand flat on it, and the right hand brings wire cutters in to snip one wire, then the other, a cut every 1.4 s (`viewmodel::defuse_hands`, `wire_cutters` in tools/gear_models.py; [snd-defuse.jpg](media/snd-defuse.jpg)) | Same; the first-person plant keys the bomb's pad and Black Ops 2's defuse works its wires |
| Input | Hold interact (F; X / Square on a controller, the reload button, when standing where it would plant or defuse) | Hold Use (F; X / Square) |

Who knows what: the attackers see who carries it (an ESCORT marker over a
teammate, a PICK UP marker where it lies); the defenders are told nothing
about the carrier, though they see a dropped charge lying in the world, as in
Call of Duty. Everyone sees the planted site. Only a player's own team hears
of a plant or defuse under way (`GameEvent::Charge`), but everyone sees the
kneeling pose (`PlayerView::charge_use`).

The hit zones kneel with the body, as Call of Duty's hitboxes follow the
third-person plant and defuse animations: over 0.3 s the body goes as low as
a crouch, the weapon drops, and the support hand reaches for the keypad
(`hitbox::action::kneel` and `KNEEL_REACH`, which the drawn character uses
too). A shot at standing head height passes over a planter.
`hitbox_fit_test` measures the kneel against the drawn soldier.

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

Breakwater's (`CHARGE_SITES` in `tools/breakwater_layout.py`), on Red's half:

- **A**: the pump house's east room on the north route, by the export pumps.
- **B**: Red's quay by the loading-arm berth, on the south route.

Each has a solid marker crate with its letter on every side
(`gear_models.charge_site_a` and `_b`, built in Blender), which stands in
every mode as set dressing. On Slamcity the defenders reach either site in
about 7 s at a sprint, the attackers in about 11 s. Nuketown, Warehouse, and
the Testing Area mark no sites, and the title screen offers Search and
Destroy only on Slamcity and Breakwater.

## Bots

Bots play the charge (`charge_objectives` in the server, through the bots'
objectives, as in Capture the Flag and Hardpoint):

- A bot standing where holding interact would plant or defuse holds it
  (`ObjectiveKind::Use`), looking about, and fights first if an enemy is in
  sight. The same filter as for players holds it still with its weapon away.
- Attackers (`shooter_sim::snd::bots`, `SndPlan` on the server): the round
  starts with a target site picked at random. The carrier waits with the
  team about 14 m short of it, and every 2.5 to 5 seconds decides to go and
  plant with a chance equal to the share of the round gone (never under
  30%, so the team does not wait at the site until it is found), always once
  85% of it is gone. About three in ten of the other attackers escort it, then
  guard spots within 6.5 m of the plant spot; with three or more attacker
  bots one lurks at the other site; the rest gather with the carrier and
  push into the target site to clear it as the carrier goes to plant. The
  nearest attacker bot fetches a dropped charge; after the plant they hold
  spots round it.
- Switching sites: what the attackers' bots have seen in the last 4 seconds
  near each site (within 8 m), plus two for each attacker killed there in
  the last 20 seconds, is the site's threat. At 5 or more, with the other
  site at least 3 quieter, they switch, once a round and only in its first
  60%: attackers pushing a site always see someone, and a late switch only
  runs the clock down.
- Defenders: split between the sites, holding spots in each. When the
  defenders' bots see one site clearly under attack (threat 2 or more, and
  more than the other), all but the first defender of the other site rotate
  to it; that one anchors. Once the charge is planted, the nearest runs to
  defuse it and the rest clear round it.
- Hard bots bait: arriving to plant or defuse with an enemy seen in the
  last 5 seconds, heard, or on the minimap within 25 m, a Hard bot taps
  interact for 0.8 s, lets go, and watches where the enemy was for 1.6 s
  before doing it for real; once per spot a life. It fakes a defuse half the
  time and a plant one time in five, since a fake plant costs the attackers
  time at the site.

Defender bots frag enemies within about 10 m of the nearest site
(Black Ops 2's `bot_sd_grenade`: 384 units), and bots pop smoke over a plant
or defuse. Black Ops 2's bots (`maps/mp/bots/_bot_sd.gsc`) are the reference for the
carrier's timing (`sd_get_time_frac`: the chance to go and plant is the share
of the round gone, checked every 2.5 to 5 s, forced past 85%) and the escort
share (`protectcarrier`: three in ten, within 256 units of the plant spot).
Their defenders take a random site and rush the defuse. Site switching, the
lurker, the defenders' rotation, and fakes are what players do; Black Ops
2's bots do not. See [bots.md](bots.md).

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
in the kill feed.

It looks far bigger than a frag, as Call of Duty's bomb does: a fireball
rolling up out of the site, dust and dirt thrown out across it, a dark smoke
column climbing about 15 m and hanging for about 10 s, chips of rubble, a
scorch mark, and a flash that lights 26 m round
(`CombatEffects.charge_explosion`, `scenes/effects/charge_explosion.tscn`).
It is the frag's explosion 2.5 times as big under the charge's own fireball,
column, and ground skirt. The screen rattles out to about 50 m, hardest
within 3 m. It rumbles a controller out to 50 m and rings in the ears inside
18 m, twice a frag's reach. Clients draw it when the charge's status
turns from planted to detonated, so nobody joining later sees it go off.
It sounds the frag's distant bang, louder (`godot/scripts/charge_explosion.gd`).

## On screen

`godot/scripts/net/snd_hud.gd` and `snd_view.gd`:

- Pips for who is alive on each team at the top centre, with the round
  number and ATTACKING or DEFENDING under them.
- The freeze banner (ROUND N, with ATTACK or DEFEND and the goal) and the
  result (ROUND WON or ROUND LOST, with a halftime note before the swap).
- A marker over each site (PLANT or DEFEND; once planted, DEFUSE or DEFEND
  on that site only), the attackers' PICK UP and ESCORT markers, a line
  under the crosshair saying what holding interact does where you stand, and
  a bar while you plant or defuse (a smaller one for a teammate).
- Calls: CHARGE PLANTED (with the site), CHARGE DEFUSED, CHARGE DROPPED,
  CHARGE PICKED UP, YOU HAVE THE CHARGE.
- The planted charge's lamp blinks once a second.

The charge's beeps and its own explosion and plant sounds are not built yet;
see [TODO.md](../TODO.md).
