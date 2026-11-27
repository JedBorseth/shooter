# Bots

Bots fill matches, so most games are played against them. They play the way
Black Ops 2's Combat Training bots do: through the same `PlayerInput` a
client sends, by the same rules, with a difficulty that changes how well they
see, react, aim, and use their kit, never what they are allowed to do. Every
bot runs on the dedicated server.

Code:

- `rust/sim/src/bot.rs`: skill levels, the aim model, grenade throw planning,
  running from frags, flash blindness (engine free).
- `rust/game/src/net/bots.rs`: the server's bot brain (`BotBrain`):
  perception, navigation, fighting, cover, grenades, anti-air, and the
  navigation bake (`BotNavigation`).
- `rust/game/src/net/server.rs`: bot spawning and team fill, classes and
  scorestreaks, and each mode's objectives (`bot_objectives` and the
  functions it calls).
- Mode rules, engine free: `rust/sim/src/snd/bots.rs` (Search and Destroy
  round plan, kept tick to tick by `rust/game/src/net/snd_plan.rs`),
  `rust/sim/src/domination.rs` (`bot_goals`), `rust/sim/src/hardpoint.rs`
  (`bots_rotating`).

## Reference

Black Ops 2's multiplayer bot scripts (`maps/mp/bots/_bot.gsc`,
`_bot_combat.gsc`, `_bot_loadout.gsc`, and one per mode: `_bot_sd.gsc`,
`_bot_dom.gsc`, `_bot_koth.gsc`, `_bot_ctf.gsc`), from the decompiled T6
scripts. Distances convert at 1 unit = 2.54 cm. The scripts are a reference
for rules and numbers only; nothing is copied.

## Starting bots

| How | What |
| --- | --- |
| Local match setup | Team modes: **Ally bots** and **Enemy bots** difficulty chips; free-for-all modes: one **Bots** chip. Bots fill the empty places. |
| `--bots N --bot-skill easy\|normal\|hard` | A dedicated server with N bots (at most 50). Each player who joins takes a bot's place, and a bot comes back when a player leaves; teams stay even. |
| `--team-bots A,E --team-bot-skill easy,hard` | Custom games: each team keeps exactly that many bots (allies at most 11, enemies 12) at its own difficulty, and players do not take their places. Free-for-all modes add the two numbers. See [custom-games.md](custom-games.md). |

Bots are named "Bot 1", "Bot 2", and so on. Without `--bot-skill` the bots
are scripted test bots that strafe and fire bursts and hurt nobody unless
`--bot-damage` is set.

**Loadouts.** Each life a bot takes one of the six default classes, as Call
of Duty's bots do, with its primary swapped for any weapon of the same class
and its sidearm for any secondary a default class carries (Marksman bots
take marksman rifles as well as snipers), plus one perk from each tier
(`class::bot_class`). One bot in four carries the RL-4 launcher as its
sidearm, for enemy aircraft, as Black Ops 2's bots pick launchers among
their secondaries.

**Scorestreaks.** Each bot brings all three streaks, the UAV, the
Counter-UAV, and the Care Package (`BOT_STREAKS`), and calls a streak as
soon as it holds one it can use. Bots go for a landed Care Package, their
own within 40 m and anyone's within 15 m, and hold use on it. See [scorestreaks.md](scorestreaks.md).

**Cosmetics.** Each bot shows a low prestige (0 to 3) and a career calling
card, fixed by its id (`bot_cosmetics`).

## Difficulty

Black Ops 2 has four levels (`bot_set_difficulty`); this game has three
(`Skill::params`):

| Black Ops 2 | Reaction | Grenades |
| --- | --- | --- |
| Recruit | 1200 to 1600 ms | never (`bot_AllowGrenades 0`) |
| Regular | 600 to 800 ms | yes |
| Hardened | 400 to 700 ms | yes |
| Veteran | 40 to 70 ms | yes |

| Ours | Easy | Normal | Hard |
| --- | --- | --- | --- |
| Reaction (first sight to first shot) | 0.65 s | 0.4 s | 0.25 s |
| Fastest turn | 170°/s | 280°/s | 420°/s |
| Aim error on first sight, shrinking by | 7°, 1.2/s | 5°, 2.0/s | 3.5°, 3.2/s |
| Aims at the head, per engagement | never | 15% | 35% |
| Fires within | 3.5° | 2.5° | 1.8° |
| Automatic bursts, pause | 0.3 s, 0.45 s | 0.45 s, 0.3 s | 0.6 s, 0.2 s |
| Field of view | 90° | 110° | 120° |
| Takes a grenade chance | never | 55% | 75% |
| Cooks frags | no | to 0.2 s past reaching | to 0.05 s past reaching |

Easy sits near Regular's reactions, Normal near Hardened's, Hard between
Hardened and Veteran. Easy also never takes cover, patrols to the minimap,
aims round corners, knifes, or throws smoke.

## Awareness

- **Sight**: an enemy within 90 m, inside the field of view, with a clear
  line from the bot's eye. Smoke blocks it; glass (colliders tagged
  `surface = "glass"`) does not, up to three panes, and is no cover.
  Perception runs every 6 ticks, spread over the bots. The current target
  wins near ties, so bots do not flick between targets.
- **Being shot**: the attacker is noticed whichever way the bot faces, and
  for 2 s the bot turns toward where the shot came from.
- **Footsteps**: an enemy within 7 m is noticed whichever way the bot faces,
  scaled by Dead Silence and the bot's own Awareness (`perk::heard`). Steps
  heard through a wall or round a corner turn its eyes that way for 2 s;
  crouched steps are too quiet.
- **Minimap**: bots read the minimap a player on their side sees
  (`shooter_sim::radar`: unsuppressed fire for a few seconds, the team's UAV
  sweeps, nothing while an enemy Counter-UAV jams it). As Black Ops 2's
  `bot_update_patrol` (512 units, every 5 to 10 s), Normal and Hard bots
  patrol toward the nearest dot within 60 m: at once inside 13 m, otherwise
  every 5 to 10 s, giving up after 6 s. Every skill turns its eyes to a dot
  or heard steps within 20 m while walking, 40 m while guarding.
- **Memory**: a bot hunts the spot where it last saw an enemy for 4 s.

## Fighting

- **Aim**: the view turns toward the target no faster than the skill's turn
  rate, off by an error that shrinks the longer it watches, and fires only
  after the reaction time and within the fire cone (`bot::Engagement`).
  Automatic weapons fire in bursts; semi-automatics and burst weapons are
  tapped no faster than a person pulls a trigger (slower on Easy).
- **Movement**: it strafes, changing direction every 0.6 to 1.5 s, closes on
  targets past 35 m, and backs off inside 4 m (not with a shotgun). It aims
  down sights past 12 m (not with a shotgun), and sometimes crouches against
  targets past 18 m.
- **Knife**: above Easy, a bot facing an enemy inside the lunge range
  knifes, as players do.
- **Hard only**: a bot sprinting into an enemy 5 to 14 m away slides into
  the fight. Within 15 m, in its first second of a fight, each strafe change
  drop-shots (goes prone, firing) three times in ten; once prone it fights
  on prone. Otherwise each strafe change crouches it half the time. Black
  Ops 2's bots go prone only to plant, defuse, and hold a flag; the owner
  asked for these on Hard.
- **Corners** (Normal and Hard): where the path turns more than about 35°
  within 4 m, the bot aims round the corner before stepping out.
- **Cover** (Normal and Hard): under 45 health and shot in the last 1.5 s,
  a bot (not a flag carrier or one planting) runs to the nearest reachable
  spot 3.5 to 9 m away, not toward the threat, where a standing chest is
  hidden from it, popping smoke between itself and the threat on the way if
  it carries some. There it crouches and reloads, for 4 s at most or until
  back to 90 health. Once its magazine is half full and its health 60, it
  stands to look over its cover for 0.7 s in every 2 s; if the threat is in
  sight then, it fights from there.
- **Anti-air**: with no enemy player in sight, a bot carrying a launcher
  turns it on an enemy UAV or Counter-UAV no more than 70° overhead and
  within a rocket's flight, leads it, fires, and reloads; it switches back
  when the sky is clear or an enemy shows (`threat_requires_rocket` in
  `_bot_combat.gsc`).

## Grenades

A bot looks for a grenade chance about once a second, and after a chance its
skill passes on, waits 2.5 s. Only one bot checks a throw against the map
each tick.

- **Frags** (10 to 28 m): at a group of enemies seen in the last second
  within 4.5 m of each other; at an enemy seen on the objective's ground;
  or at one that went out of sight 0.3 to 3 s ago where it was last seen.
- **Flashbangs** (7 to 18 m): the same objective and hiding chances, when
  the bot has no frag.
- **Smoke**: once a life, onto an objective that asks for it (below), from
  3 to 25 m, and not where a cloud already hangs (none within 6.5 m in the
  last 12 s, as `bot_combat_throw_smoke`); and between itself and a threat
  when breaking for cover.

Each throw is planned over flat ground (`bot::plan_throw`: a flat throw and
a lob, frags cooked by the skill's margin so they burst on time), then
traced against the real map with a bounded number of rays: the flight must
end within 3.5 m of the target, a frag's blast must reach it, and the
thrower must be outside its own blast. Easy bots throw nothing, as Recruits.

Black Ops 2 for comparison: above Recruit, after losing an enemy a bot
throws its lethal and tactical where it was, half the time
(`bot_combat_think`), frags only beyond 6.5 m and flashbangs beyond 16.5 m
of itself (`bot_combat_throw_lethal`, `bot_combat_throw_tactical`).

**Being on the other end.** A bot runs from a live frag within 6.5 m whose
blast can reach it, sprinting straight away, still shooting if it has a
target. A flashbang blinds it for three quarters of the flash's length
(halved by Tactical Mask): its view wanders around where its target was and
it sprays there in bursts while backing off. It cannot see through smoke.

## Navigation

The server bakes a navigation mesh from the map's static colliders on the
world layer on its second tick (`BotNavigation::bake`; the map must be the
server's first child). The agent radius is 0.375 m with 0.125 m voxels (0.1 m
up), fine enough that a 1.4 m doorway stays open whatever the grid's
alignment; agents are 1.8 m tall, climb 0.4 m, and walk slopes to 40°. The
bake is limited to a box round every spawn, lane, and objective point, 40 m
out sideways, 20 m below, and 40 m above (`bake_area`), so far-off scenery
with collision (mountains round a map) cannot make the grid too large to
bake.

- **Lanes**: maps with a `BotLanes` node (Breakwater, Warehouse)
  give lines of points from one side to the other. With no objective, a bot
  follows a lane three times in four, picking the one with the fewest
  teammates on it; otherwise it heads for an enemy spawn (45%) or anywhere
  on the mesh.
- **Detours**: a hunted objective over 35 m away gets a detour, once a life,
  through a random point in the middle third of the way, so attackers spread
  over the routes.
- **Paths** update every 2 s and whenever a moving goal (a carrier) moves.
  Within 2.5 m of an objective a bot walks straight at it.
- **Getting unstuck**: after 1.2 s without progress a bot jumps; if that
  fails it backs out toward the nearest point on the mesh, or any way but
  the one it was going, then plans afresh. A bot standing on a scrap of mesh
  cut off from the rest walks straight at its goal to get back on; after
  four path updates in a row find no way, it gives the goal up.

## Objectives

Each tick the server hands each bot a `BotObjective`: **run** (keep moving
through a fight), **hunt** (head there, stopping to fight; rushes like a
runner within 12 m), **hold** (guard a spot), or **use** (hold interact
there: plant or defuse). A defender chases an enemy it lost sight of only
within 18 m of its spot, an attacker within 12 m of itself, a runner never.
Each objective names its ground (a site, a flag, a zone, the charge), widened
by 9.75 m (Black Ops 2: 384 units); an enemy seen there is worth a grenade
on its own. Team Deathmatch, Free-for-all, and Gun Game have no objective:
bots roam and hunt.

Bots with Scavenger low on ammunition (under a magazine in reserve) detour
to a Scavenger pack within 15 m unless they are running or using
something.

**Capture the Flag** (`bot_objectives`):

- A carrier runs the flag home.
- The two bots nearest their team's dropped flag run to return it;
  attackers grab a dropped enemy flag as urgently.
- In teams of three or more, every third bot defends: it holds spots near
  its flag, or hunts the enemy carrying it.
- The rest attack the enemy flag, or escort their carrier from a spot 2 to
  10 m round it that changes every 4 s, as Black Ops 2's bots take a node 64
  to 512 units from it (`bot_ctf_capture`).

**Domination** (`domination::bot_goals`):

- The two nearest bots answer an enemy capture of a flag the team owns.
- A bot within 6 m of the edge of a flag the team does not own takes it
  (Black Ops 2's first rule in `bot_dom_think`).
- One bot in four guards an owned flag; the rest take the nearest flag the
  team does not own, every third bot the next nearest. With every flag
  owned, everyone guards.
- Near its flag a bot holds spots inside the circle so it captures or
  contests. Bots smoke an enemy-owned flag on the way in and frag a
  contested one (`bot_flag_grenade`).

See [domination.md](domination.md).

**Hardpoint**:

- Three of every four bots hold spots inside the live zone, moving every
  4 s; the fourth hunts just outside its edge for enemies coming in. Bots
  frag the enemy nearest a contested zone (`bot_hill_grenade`).
- 12 s before the zone moves, a third of a team holding it, or half of a
  team that is not, head for the next zone (`hardpoint::bots_rotating`).
  Black Ops 2's bots only know the live zone (`bot_koth_think`); players
  rotate early, and so do ours.

**Search and Destroy** (`charge_objectives`, `snd::bots`), after
`_bot_sd.gsc`:

- The carrier waits with the team 14 m short of the target site. Every 2.5
  to 5 s it decides to go and plant with a chance equal to the share of the
  round gone (never under 30%, so attackers do not stall), and always once
  85% is gone.
- About three in ten of the other attackers escort the carrier, then hold
  within 6.5 m of the plant spot; with three attacker bots or more one lurks
  on the other site; the rest gather 5 m round the carrier's waiting spot,
  then push into the site as it goes to plant.
- Attackers switch sites once a round, before 60% of it is gone, when they
  see their target held (enemies seen nearby, teammates killed there count
  double) and the other is clearly quieter.
- Defenders split over the sites. When one is clearly under attack, every
  defender guarding another site rotates to it except one anchor per site.
  They frag enemies within 9.75 m of their site (`bot_sd_grenade`).
- The nearest attacker fetches a dropped charge; the nearest defender runs
  to defuse a planted one, and the others hunt round it.
- A bot planting or defusing pops smoke 5 m out toward the other side first.
  With an enemy about (seen in the last 5 s, or heard or on the minimap
  within 25 m), a Hard bot first fakes the plant (one time in five) or the
  defuse (one in two): a 0.8 s tap, then 1.6 s watching for whoever comes to
  stop it.

See [search-and-destroy.md](search-and-destroy.md).

## Bench

`godot/tools/bot_bench.gd` plays whole bot-only matches headless on a
dedicated server in the same process and writes JSON: the winner, team
scores, every bot's kills, deaths, score, and captures, the bots' tactics
(lanes, cover, grenades, fakes), and each mode's objective log (Search and
Destroy rounds with how each ended, the site planted, and site switches;
Hardpoint zones with how soon each team arrived and how long each held it;
Domination flag ownership each second). It prints one summary line per map
and mode: score margin, kills mean and spread, K/D spread, captures, the
objective's numbers, and tactics.

```sh
godot --headless --fixed-fps 120 --path godot --script res://tools/bot_bench.gd -- \
  --modes tdm,dom,hardpoint,ctf,snd --maps breakwater,warehouse --skill normal \
  --bots 12 --matches 2 --minutes 5 --out ../bench.json
```

Maps: `breakwater`, `warehouse`, `arena`. Each match runs to its end or for
`--minutes` of play. `--seed` (default 1) changes the bots' random choices,
and each match adds its index; use the same seed before and after a change
to compare like for like.

Bot matches also drive the bandwidth bench (`godot/tests/bandwidth_test.gd`,
see [netcode.md](netcode.md)) and the CPU bench (`just cpu-bench --map
warehouse --bots 11`, see [performance.md](performance.md)).

## Tests

`godot/tests/bots_test.gd`, `bot_awareness_test.gd`,
`bot_equipment_test.gd`, and `bot_navigation_test.gd` (run with, for
example, `just test bots_test`), plus the unit tests in `rust/sim/src/bot.rs`,
`rust/sim/src/snd/bots.rs`, and `rust/game/src/net/bots.rs`.
