# Zombies: power, drinks, the mystery crate, and the forge

The systems that make a round-based Zombies run worth playing past the first
few rounds, after Black Ops' power switch, perk machines, mystery box, and
Pack-a-Punch. The rounds, the horde, doors, windows, power-ups, and last
stand are in [roadmap.md](roadmap.md#decisions) (the Zombies row) and
`TODO.md`.

Everything here runs on the dedicated server (`rust/game/src/net/horde.rs`,
`net/machines.rs`, `net/server.rs`); clients show what it sends. The rules
are in `rust/sim/src/zombies/`. Numbers come from Black Ops III's decompiled
scripts (`scripts/zm/*.gsc` in the community's t7-source dump); each constant
names the script variable it follows.

We take the mechanics, never the property: the machines, drinks, crate, and
forge have our own names, models (built in Blender,
`tools/gear_models.py`), colours, and icons. No Call of Duty names, logos,
or jingles.

## Power

| Rule | Here | Black Ops |
| --- | --- | --- |
| Switch | One lever (`PowerSwitch` on the map) in the west yard, behind its door. A press turns the power on, free, for the rest of the game | Same: a lever behind a door, free, once a game (`_zm_power.gsc`) |
| What needs it | Every drink machine and the forge. Second Wind in a game for one works without it. The mystery crate never does | Perk machines and Pack-a-Punch; Quick Revive works without power in solo (`use_solo_revive`) |
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

## The mystery crate

An olive supply crate with a die stencilled on it
(`rust/game/src/mystery_crate.rs`, rules in
`rust/sim/src/zombies/mystery.rs`). It stands at one of the map's
`MysteryCrates` spots (the main yard's at the start, then the west yard's
and the east lot's) and needs no power.

| Rule | Here | Black Ops III (`_zm_magicbox.gsc`) |
| --- | --- | --- |
| Price | 950 | `zombie_treasure_chest_cost` 950 |
| Cycle | Weapons rise out of it and cycle for 4 s, slowing down: 20 at 0.05 s, 10 at 0.1 s, 5 at 0.2 s, 3 at 0.3 s | The same 40-step loop in `treasure_chest_weapon_spawn` |
| Pool | M4A1, AK-47, PDW-9, M870, M40, BR-3, LSW-6, equally weighted (weights in `mystery::POOL`), never one the buyer carries in any form; not the starting sidearm | Every box weapon at weight 1 unless a map weights it; `treasure_chest_canplayerreceiveweapon` skips carried ones |
| Taking it | Only the buyer, with the use key while looking at it, into its slot; it sinks back over 12 s and the crate closes if left | `treasure_chest_timeout` 12 s, buyer only |
| Moving | Never in the first 3 spins at a spot; 15% from the 4th to the 7th; certain by the 8th the first time; after a move, 30% from the 8th to the 12th and 50% from the 13th. Then the duck floats up, the buyer gets the 950 back, the crate lifts away and shows up at another spot 6 s later | `treasure_chest_should_move`'s odds; the teddy bear, the refund, and the fly-away |

The duck is our own sign for the move: a rubber duck, built in Blender.

**Online.** The server runs the crate and sends `ZombiesEvent::Crate` (its
spot, phase, buyer, the weapon it lands on, and when the phase ends) on
every change and on joining; each client shows it on the map's spots. The
weapon is sent when the cycle starts, so the display lands on it. Taking it
is an ordinary predicted pickup: the client's prompt treats the crate
holding a weapon out to this player as a free rack, and the server checks
the crate is holding that weapon out to them.

**Differences from Black Ops, on purpose:** the weapon goes into its own
slot (primary or sidearm), as every pickup in this game does, instead of
replacing the weapon in hand. No Fire Sale. No sounds yet (they must be real
CC0 recordings).

## The forge

The upgrade machine: a furnace-press in the east lot, behind its door, lit
when the power is on (`rust/game/src/forge.rs`, rules in
`rust/sim/src/zombies/forge.rs`).

| Rule | Here | Black Ops III |
| --- | --- | --- |
| Needs | The power on, 5000 points, and a weapon in hand that has an upgrade, with another weapon carried | `vending_weapon_upgrade_cost` 5000; Pack-a-Punch is powered |
| Working | The weapon goes in and the other comes up; 3.35 s later the upgrade comes out, full, for its owner alone | `wait(0.35)` + `wait(3)` in `_zm_pack_a_punch.gsc` |
| Taking it | Use while looking at it, like a rack; left for 15 s, it is lost | `level.pack_a_punch.timeout` 15 s |
| The upgrade | Twice the damage, half again the magazine, twice the reserve, "MK II" after its name, and a cold blue sheen sliding along it in first and third person | Hand-set per weapon in Black Ops; double damage and a bigger magazine and reserve are typical |
| Ammo | A wall rack of the plain weapon refills the upgraded one for 4500 | `get_upgraded_ammo_cost` 4500 |

Upgraded weapons are the weapon table's `<key>+forged` variants
(`weapon::def::forged`), one per plain weapon (not the laser), so the
upgrade rides in the weapon id like an attachment does: predicted, sent in
snapshots, and drawn with the plain weapon's model and sounds plus the
sheen (`shaders/forged.gdshader`, a material overlay set by
`WeaponView::set_forged`).

**Differences from Black Ops, on purpose:** no re-pack for an alternate
ammo type (Black Ops III's 2500-point AAT), no lone sidearm upgrade (it
would leave the player with nothing in this game's two slots), and no
sounds yet (real CC0 recordings only).

## Reference

- Black Ops III: `_zm_pack_a_punch.gsc` and `_zm_pack_a_punch_util.gsc`
  (cost, working time, timeout), `_zm_weapons.gsc` (upgraded ammo cost),
  `_zm_magicbox.gsc` (cost, cycle, timeout, move odds),
  `_zm_perks.gsc` (`perk_purchase_limit`, perk loss on going
  down), `_zm_perk_juggernaut.gsc`, `_zm_perk_quick_revive.gsc`,
  `_zm_perk_sleight_of_hand.gsc`, `_zm_perk_doubletap2.gsc`,
  `_zm_perk_staminup.gsc`, `_zm_laststand.gsc` (`revive_get_revive_time`),
  `_zm.gsc` (`wait_and_revive`), `_zm_power.gsc`.
- Hit counts with and without the health perk: the Call of Duty wiki's
  Black Ops III perk page (four hits survived, down on the fifth).
