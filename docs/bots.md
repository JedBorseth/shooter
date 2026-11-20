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
