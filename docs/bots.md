# Bots

Bots fill matches, so most games are played against them. They play the way
Black Ops 2's Combat Training bots do: through the same inputs a player
sends, by the same rules, with a difficulty that changes how well they see,
react, aim, and use their kit, never what they are allowed to do.

Code: `rust/sim/src/bot.rs` (skill levels, aim, throw planning; engine free)
and `rust/game/src/net/bots.rs` (the server's bot brain: perception,
navigation, fighting, cover, grenades). Each mode's objective for the bots is
in `rust/game/src/net/server.rs` (`bot_objectives` and the functions it calls)
and `rust/sim/src/domination.rs` (`bot_goals`).

## Reference

Black Ops 2's multiplayer bot scripts (`maps/mp/bots/_bot.gsc`,
`_bot_combat.gsc`, and one file per mode: `_bot_sd.gsc`, `_bot_dom.gsc`,
`_bot_koth.gsc`, `_bot_ctf.gsc`), read from the decompiled T6 scripts the
Plutonium project publishes. Numbers below are from those scripts (Call of
Duty units converted at 1 unit = 2.54 cm). Nothing is copied; the scripts are
a reference for rules and numbers only.

### Difficulty

Black Ops 2 has four: Recruit, Regular, Hardened, Veteran (`bot_set_difficulty`).
This game has three, Easy, Normal, and Hard, chosen in the local match setup
(Bots: Easy, Normal, Hard) or with `--bot-skill`.

| Black Ops 2 | Reaction (ms) | Field of view | Aim converges in | Grenades | Cover |
| --- | --- | --- | --- | --- | --- |
| Recruit | 1200 to 1600 | narrow | 3.5 s | never | never |
| Regular | 600 to 800 | wide | 2.0 s | yes | yes |
| Hardened | 400 to 700 | wider | 1.5 s | yes | yes |
| Veteran | 40 to 70 | almost all round | 0.1 s | yes | yes |

Our Easy sits between Recruit and Regular, Normal between Regular and
Hardened, Hard between Hardened and Veteran (`Skill::params`).

### Awareness

- **Sight** inside a field of view, with a clear line (smoke blocks it, glass
  does not), or very close by sound.
- **Being shot**: a bot turns to where the shot came from, and (above Easy)
  patrols toward the attacker (`bot_patrol_near_enemy`).

### Minimap, footsteps, and gunfire

Black Ops 2's bots know two sets of enemies (`bot_get_enemies`): all of them
for some choices, and "on radar" ones for others: enemies who fired in the
last 2 seconds, or everyone while the team has a UAV. Above Recruit they
patrol toward the nearest enemy on radar every 5 to 10 seconds, at once when
one is within 512 units (`bot_update_patrol`, `bot_patrol_near_enemy`); a
Recruit only looks at whoever hurt it. Standing still, they look toward the
nearest radar enemy (`bot_update_lookat`).

Ours read the same minimap a player on their side sees (`shooter_sim::radar`:
unsuppressed fire for 3 seconds, the team's UAV sweeps, nothing while an
enemy Counter-UAV jams it):

- Normal and Hard bots patrol toward the nearest dot within 60 m: at once
  inside 13 m, otherwise every 5 to 10 seconds, giving up after 6 seconds.
  Objectives still come first: a defender only chases near its spot, an
  attacker near itself, a runner never.
- Every skill turns its eyes to a dot within 20 m while walking, or 40 m
  while guarding, and to enemy steps heard round a corner or through a wall
  (within the 7 m hearing range, Dead Silence and Awareness applied;
  crouched steps are too quiet).

### Combat feel

- **Corners** (Normal and Hard): where the path turns more than about 35°
  within 4 m, the bot aims round the corner before stepping out.
- **Cover**: a bot hiding to heal crouches, reloads, and, once its magazine is
  half full and its health at 60, stands to look over its cover for 0.7 s in
  every 2 s (not on Easy). If the threat is in sight then, it fights from
  there instead of waiting to heal fully.
- **Hard only**: a bot sprinting into an enemy 5 to 14 m away slides into the
  fight, and up close (under 15 m) it crouches now and then mid-fight. This
  game has no prone, so there is no drop shot; the crouch stands in for it.
  Black Ops 2's bots do neither; the owner asked for them on Hard.

### Grenades

- After losing sight of an enemy, a bot above Recruit throws its lethal and
  tactical where the enemy was, half the time (`bot_combat_think`).
- Frags only beyond 6.5 m of itself; flashbangs only beyond 16.5 m
  (`bot_combat_throw_lethal`, `bot_combat_throw_tactical`).
- Objectives: Search and Destroy defenders frag (else flash) any enemy within
  9.75 m of a site (`bot_sd_grenade`); Domination bots smoke an enemy flag on
  the way in and frag a contested one (`bot_flag_grenade`); Hardpoint bots frag
  the enemy nearest a contested zone (`bot_hill_grenade`).
- Recruits throw nothing (`bot_AllowGrenades 0`).

Ours: each mode hands a bot its objective's ground (`BotObjective::area`): a
Search and Destroy site or the planted charge, a Domination flag, the live
Hardpoint zone, a Capture the Flag base, each widened by 9.75 m. An enemy seen
there in the last second is worth a frag on its own (a flashbang without one),
not only in a group. Smoke (`BotObjective::smoke`) goes onto an enemy flag a
bot heads to take, and 5 m out from a charge being planted or defused, toward
the other side; once a life, and not where a cloud already hangs (none within
6.5 m in the last 12 s, as `bot_combat_throw_smoke`). Easy bots throw nothing.

### Objectives

- **Domination** (`domination::bot_goals`): the two nearest bots answer an
  enemy capture of a flag the team owns; a bot within 6 m of a flag the team
  does not own takes it (Black Ops 2's first rule in `bot_dom_think`); one in
  four guards an owned flag; the rest take the nearest flag the team does not
  own, every third the next nearest.
- **Hardpoint** (`hardpoint::bots_rotating`): 12 seconds before the zone
  moves, a third of a team holding it, or half of a team that is not, head
  for the next zone. Black Ops 2's bots only know the live zone
  (`bot_koth_think`); players rotate early, and so do ours.
- **Capture the Flag**: escorts take spots 2 to 10 m round their carrier,
  changing every few seconds, as Black Ops 2's take a node 64 to 512 units
  from it (`bot_ctf_capture`).

### Bench

`godot/tools/bot_bench.gd` plays whole bot-only matches headless and writes
JSON: winner, scores, every bot's kills, deaths, score, and captures, the
bots' tactics, and each mode's objective log (Search and Destroy rounds with
how each ended and the site planted; Hardpoint zones with how soon each team
arrived and how long each held it; Domination flag ownership). It prints one
summary line per map and mode: score margin, kills mean and spread, K/D
spread, and the objective's numbers.

```sh
godot --headless --fixed-fps 120 --path godot --script res://tools/bot_bench.gd -- \
  --modes tdm,dom,hardpoint,ctf,snd --maps slamcity,warehouse --skill normal \
  --bots 12 --matches 2 --minutes 5 --out ../bench.json
```

At a fixed frame rate a 12-bot match runs about six times faster than real
time on Josh's dev box. Use the same `--seed` before and after a change to
compare like for like.
