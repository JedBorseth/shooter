# Zombies: power and drinks

The systems that make a round-based Zombies run worth playing past the first
few rounds, after Black Ops' power switch and perk machines. The rounds, the horde, doors, windows, power-ups, and last
stand are in [roadmap.md](roadmap.md#decisions) (the Zombies row) and
`TODO.md`.

Everything here runs on the dedicated server (`rust/game/src/net/horde.rs`,
`net/machines.rs`, `net/server.rs`); clients show what it sends. The rules
are in `rust/sim/src/zombies/`. Numbers come from Black Ops III's decompiled
scripts (`scripts/zm/*.gsc` in the community's t7-source dump); each constant
names the script variable it follows.

We take the mechanics, never the property: the machines and drinks have our
own names, models (built in Blender,
`tools/gear_models.py`), colours, and icons. No Call of Duty names, logos,
or jingles.

## Power

| Rule | Here | Black Ops |
| --- | --- | --- |
| Switch | One lever (`PowerSwitch` on the map) in the west yard, behind its door. A press turns the power on, free, for the rest of the game | Same: a lever behind a door, free, once a game (`_zm_power.gsc`) |
| What needs it | Every drink machine and the forge. Second Wind in a game for one works without it | Perk machines and Pack-a-Punch; Quick Revive works without power in solo (`use_solo_revive`) |
| New game | Off again | — |

The switch lights two lamps and every machine's sign when thrown.

## Drinks

Five machines (`DrinkMachines` on the map), each a cabinet in its drink's
colour with a lit header that comes on with the power
(`scripts/zombies_drink_machine.gd`). A press of the use key buys the drink
(the server decides). The weapon goes down and comes back up for the drink.

| Drink | Effect | Price | Black Ops III |
| --- | --- | --- | --- |
| Bulwark | Down on the fifth zombie hit instead of the second | 2500 | `specialty_armorvest`: 100 more health; a Jugg player survives four hits and goes down on the fifth |
| Second Wind | Revive teammates in half the time (1.5 s); alone, get back up by yourself 10 s after going down | 1500, alone 500 | `specialty_quickrevive`: `revive_get_revive_time` halves 3 s; `wait_and_revive`'s `solo_revive_time` 10 s; `revive_cost_override` 1500/500 |
| Quickload | Reloads take half the time | 3000 | `specialty_fastreload`, `perk_weapReloadMultiplier` 0.5 |
| Hair Trigger | A third more rounds a minute, and every round hits zombies twice | 2000 | `specialty_doubletap2`: `perk_weapRateMultiplier` 0.75, each bullet fired twice |
| Long Stride | Move 7% faster | 2000 | `specialty_staminup` (7% faster movement) |

Rules (`rust/sim/src/zombies/drinks.rs`):

- **Four at most** (`level.perk_purchase_limit` 4). A drink already held is
  not sold again.
- **Going down loses every drink**, including Second Wind, as in Black Ops.
- **Second Wind alone** (a game with one player) costs 500, works without
  power, and lifts the player back up 10 s after going down with the
  pistol's last stand in between; zombies ignore them and the game goes on.
  Nobody earns the points lost going down. After the third purchase the
  machine leaves (`level.solo_lives_given >= 3`). Whether a game is for one
  is decided at each purchase and each fall (one player in the game), and
  clients are told the machines' prices again whenever a player joins or
  leaves.
- **Bulwark and the two-hit rule.** Without Bulwark a hit at full health
  takes half and any hit before health has fully regenerated downs (this
  game's rule). With it each hit takes a fifth of full health, and health
  regenerates between hits as usual, so five hits in a row down.
- **Prediction.** Quickload, Hair Trigger, and Long Stride change handling,
  so the drinks held ride in the predicted player state
  (`PlayerState::drinks`, five bits in the owner's state), and the client
  predicts them like any other movement or weapon rule. Hair Trigger's
  double damage and Bulwark are the server's.

**HUD.** The drinks held sit as small badges above the round counter
(`scripts/zombies_drinks_row.gd`), in the order bought, each in its machine's
colour with a glyph: a shield, a cross, a magazine, a round, chevrons.

**Differences from Black Ops, on purpose:**

- No drinking animation yet: the weapon re-raises. Black Ops plays a bottle
  in the hands for about three seconds.
- No jingle or drink sound. These must come from real CC0 recordings; none
  suitable was found yet (see the report for candidates).
- Long Stride does not lengthen sprint: this game's sprint is already
  unlimited (a recorded decision).

## Reference

- Black Ops III: `_zm_perks.gsc` (`perk_purchase_limit`, perk loss on going
  down), `_zm_perk_juggernaut.gsc`, `_zm_perk_quick_revive.gsc`,
  `_zm_perk_sleight_of_hand.gsc`, `_zm_perk_doubletap2.gsc`,
  `_zm_perk_staminup.gsc`, `_zm_laststand.gsc` (`revive_get_revive_time`),
  `_zm.gsc` (`wait_and_revive`), `_zm_power.gsc`.
- Hit counts with and without the health perk: the Call of Duty wiki's
  Black Ops III perk page (four hits survived, down on the fifth).
