# Zombies: hound rounds, power-ups, power, drinks, the mystery crate, and the forge

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

## Hound rounds

Every few rounds the zombies stay away and a pack of hounds comes instead,
Black Ops' dog rounds under our own name and look (rules in
`rust/sim/src/zombies/hounds.rs`, run by `net/horde.rs`):

| Rule | Here | Black Ops III (`_zm_ai_dogs.gsc`) |
| --- | --- | --- |
| When | The first in round 5, 6, or 7; then every 4 or 5 rounds | `dog_round_tracker`: `randomintrange(4, 7)` from round 1, then `randomintrange(4, 6)` |
| Pack | 6 a player in the first two hound rounds, 8 after | `dog_round_spawning`: `players.size * 6`, then `* 8` |
| Health | 400, 900, 1300, then 1600 | `dog_health_increase` |
| At once | Two a player | `num_player_valid * 2` |
| Pace | The first 7 s in, then every 3, 2.5, 2, or 1.5 s by hound round, less the share of the pack already sent (at least 0.05 s) | `wait(1)` + `wait(6)`, `waiting_for_next_dog_spawn` |
| Arrival | A strike lands at a hound spot (`HoundSpawns` on the map) 10 to 25 m from the player it hunts, not the last one used; the hound stands there 1.5 s later | `dog_spawn_factory_logic` (400 to 1000 units), `dog_spawn_fx` (`wait(1.5)`) |
| Bite | 40% of a player's health (three bites down a player without a rest), 16% with Bulwark (seven). Reach 1.6 m, through to 2.3 m | `meleedamage` 40 of 100 health (250 with Jugg); `ai_meleeRange` 100 units in dog rounds |
| Speed | 6.2 m/s: faster than any zombie, a little slower than a sprint (7.4) | `zombie_move_speed = "sprint"` |
| Points | As a zombie: 10 a hit, 60 a kill, 100 a headshot kill, 130 a knife kill; kills can drop power-ups | `dog_death`'s `player_add_points("death")` |
| End | The last hound killed leaves a Max Ammo where it fell | `dog_round_aftermath` |

Hounds run straight at the nearest standing player like zombies (no
pathfinding, a project decision), never stagger, and ignore downed
players. They crumble to ash with a cold flare when killed instead of
falling as ragdolls, as Black Ops' burst.

**Look.** Our own creature (`tools/build_hound.py`, built and rigged in
Blender): a gaunt, hairless, ash-grey hound, long in the leg, with a ridge
of bone spurs down its back, bared teeth, and cold blue-white eyes. Black
Ops' burn orange. Its gallop, bite, and pant are keyed procedurally in the
same script. Its strike is a cold bolt that crackles on the spot for 1.5 s
first (`scripts/zombies_hound_strike.gd`). The HUD calls the round
"ROUND n  HOUNDS" and counts "HOUNDS LEFT".

**Online.** Hounds ride in the horde's snapshots with a bit marking them
(`ZombieView::hound`), are rewound for lag compensation with their own
four-legged hit zones (`hitbox::hound_hitboxes`: head, chest, belly,
forelegs, hind legs, haunches), and are drawn as `Hound`
(`rust/game/src/hound.rs`). Strikes are `ZombiesEvent::HoundStrike`.

**Differences from Black Ops, on purpose:** no hound sounds yet (they must
be real CC0 recordings; none found), no fog change for the round, and
hound spots only in the main yard.

## Power-ups

Kills drop glowing tokens any standing player walks over for the whole team
(`rust/game/src/net/horde.rs`, rules in `rust/sim/src/zombies/powerups.rs`).
When a drop comes, and which, follow Black Ops III's `_zm_powerups.gsc`:

| Rule | Here | Black Ops III |
| --- | --- | --- |
| Earned drops | Once the team has earned 2000 points together, the next kill drops one; the step grows 14% each time and counts from the points earned then | `watch_for_drop`: `zombie_powerup_drop_increment` 2000, times 1.14 per drop |
| Lucky drops | Any kill also drops one 3 times in 100 | `powerup_drop`: `randomint(100) <= 2` |
| Per round | Four at most | `zombie_powerup_drop_max_per_round` 4 |
| Which | Dealt in turn from a shuffled deck of every kind, shuffled again when it runs out; a kind that cannot drop now is passed over | `get_next_powerup`, `get_valid_powerup` |
| On the ground | 15 s still, then 11.5 s blinking (half-second, then quarter, then tenth), then gone | `powerup_timeout` |
| Max Ammo | Every standing player's weapons full | `full_ammo` |
| Insta-Kill, Double Points | 30 s each | `zombie_powerup_insta_kill_time`, `..._double_points_time` |
| Kaboom (the nuke) | Every zombie in play dies; every player gets 400 | `nuke` |
| Fire Sale | Drops only once the crate has moved, and not while one is on. For 30 s the mystery crate stands at every spot and a spin costs 10 anywhere; a Fire Sale spin never lands on the duck or counts towards a move. When it ends, each spare crate goes once its spin is done | `func_should_drop_fire_sale`, `start_fire_sale`, `toggle_fire_sale_on` (`zombie_cost = 10`), `_box_opened_by_fire_sale` |
| Carpenter | Drops only once five windows are bare. Boards go back on every window, nearest to where it was taken first, one every 0.1 s; then every player gets 200 | `func_should_drop_carpenter`, `start_carpenter` |

Points earned count everything paid: hits, kills, boards, the nuke's and
the Carpenter's pay, doubled under Double Points, as Black Ops'
`score_total` does. Kills by the nuke drop nothing.

The Carpenter's token is a carpenter's tote with a saw standing in it, and
the Fire Sale's a shop's price tag stamped "10", both built in Blender
(`tools/gear_models.py`, `powerup_tote` and `powerup_tag`). The HUD counts
the Fire Sale down beside Insta-Kill and Double Points.

**Differences from Black Ops, on purpose:** a crate travelling between spots
when a Fire Sale starts lands at once (Black Ops holds it hidden until the
sale ends). Another Fire Sale taken while one is on (two can lie on the
ground at once) adds 30 s, as Black Ops does.

**Windows.** Holding use at a window puts a board back at once, then one a
second (`do_post_chunk_repair_delay`'s `wait(1)`). Each pays 10 while the
round's running total stays under 50 times the round number, at most 500
(`rebuild_barrier_cap_per_round`); as in Black Ops, the board that reaches
the cap is not paid.

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
(the server decides). The weapon goes down and the drink is drunk from its
bottle in first person, as Black Ops' perk bottle plays (2.3 s, then the
weapon is drawn; no fire meanwhile): the right hand brings the bottle up, the
left thumb flicks its cap off, three gulps with it tipped to the mouth, and it
is tossed away down to the right (`Raise::Drink`, `weapon::DRINK_SECONDS`;
`viewmodel::drink_hands`; each drink's bottle is `drink_bottle_<key>` in its
machine's colour, built in tools/gear_models.py). Frames:
[zombies-drink.jpg](media/zombies-drink.jpg).

| Drink | Effect | Price | Black Ops III |
| --- | --- | --- | --- |
| Bulwark | Down on the fifth zombie hit instead of the second | 2500 | `specialty_armorvest`: 100 more health; a Jugg player survives four hits and goes down on the fifth |
| Second Wind | Revive teammates in half the time (1.5 s); alone, get back up by yourself 10 s after going down | 1500, alone 500 | `specialty_quickrevive`: `revive_get_revive_time` halves 3 s; `wait_and_revive`'s `solo_revive_time` 10 s; `revive_cost_override` 1500/500 |
| Quickload | Reloads take half the time | 3000 | `specialty_fastreload`, `perk_weapReloadMultiplier` 0.5 |
| Hair Trigger | A third more rounds a minute, and every round hits zombies twice | 2000 | `specialty_doubletap2`: `perk_weapRateMultiplier` 0.75, each bullet fired twice |
| Long Stride | Move 7% faster | 2000 | `specialty_staminup` (7% faster movement) |

**Reviving a teammate.** Holding use over a downed teammate puts the weapon
away, as Black Ops' revive does: in first person the left hand goes flat on
them and the right jabs a syrette in beside it and holds it there, pressing
its plunger (`viewmodel::revive_hands`, `revive_syrette` in
tools/gear_models.py); other players see the reviver kneel over them, hands
down, from the server's `Revive` events. Frames:
[zombies-revive.jpg](media/zombies-revive.jpg).

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

- Other players see no bottle yet: their weapon stays in hand while they
  drink (third person).
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
replacing the weapon in hand. No sounds yet (they must be real
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

- Black Ops III: `_zm_ai_dogs.gsc` (hound rounds), `_zm_powerups.gsc`, `_zm_powerup_carpenter.gsc`,
  `_zm_powerup_fire_sale.gsc`,
  `_zm_blockers.gsc` and `_zm.gsc` (rebuilding), `_zm_pack_a_punch.gsc` and `_zm_pack_a_punch_util.gsc`
  (cost, working time, timeout), `_zm_weapons.gsc` (upgraded ammo cost),
  `_zm_magicbox.gsc` (cost, cycle, timeout, move odds),
  `_zm_perks.gsc` (`perk_purchase_limit`, perk loss on going
  down), `_zm_perk_juggernaut.gsc`, `_zm_perk_quick_revive.gsc`,
  `_zm_perk_sleight_of_hand.gsc`, `_zm_perk_doubletap2.gsc`,
  `_zm_perk_staminup.gsc`, `_zm_laststand.gsc` (`revive_get_revive_time`),
  `_zm.gsc` (`wait_and_revive`), `_zm_power.gsc`.
- Hit counts with and without the health perk: the Call of Duty wiki's
  Black Ops III perk page (four hits survived, down on the fifth).
