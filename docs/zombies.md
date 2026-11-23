# Zombies

Co-op survival for one to four players against rounds of zombies, after
Black Ops' Zombies. Zombies climb in through boarded windows. Players spend
points on wall weapons, debris, drinks, the mystery crate, the forge, and an
electric fence. Every few rounds, a pack of hounds comes instead of zombies.

Everything runs on the dedicated server, as every mode does. The rules are
engine-free in `rust/sim/src/zombies.rs` and `rust/sim/src/zombies/`. The
server runs them in `rust/game/src/net/horde.rs` (the rounds, zombie bodies,
windows, doors, power-ups, the fence) and `rust/game/src/net/machines.rs`
(the power, drink machines, mystery crate, forge). The players' side (points,
purchases, last stand, revives) is in `rust/game/src/net/server.rs`. Clients
only draw what the server sends. The map's `ZombieDirector`
(`rust/game/src/zombie_director.rs`) mirrors the server's game for the HUD
and acts as the wallet the prompts check.

Most numbers come from Black Ops III's decompiled scripts (`scripts/zm/*.gsc`
in the community's t7-source dump). Each constant names the script variable it
follows. We copy the mechanics, never the property: the machines, drinks,
crate, forge, and hounds have our own names, models (built in Blender), colours,
and icons. There are no Call of Duty names, logos, or jingles in play.

## Starting a game

The title menu's **Zombies** opens a menu styled after Black Ops 2's
(`scenes/zombies_menu.tscn`, `scripts/zombies_menu.gd`). Zombies assets load
only when it opens.

- **Solo** opens a rotating globe with five locations (Black Ops 2's Zombies
  maps, as placeholders). Pick one from the list, from its marker, or with
  the arrow buttons; drag to turn the globe. **START GAME** starts a private
  local dedicated server (port 27035) and joins it. The game skips the
  ready-up, and the pause menu pauses the server. Every location plays the
  Zombies arena.
- **Co-op** opens the match setup card: your name, the map (Zombies Arena),
  and **Listed**: **LAN** or **Internet**. **HOST GAME** starts a local
  dedicated server on port 27027 named "<name>'s Zombies". Friends find it in
  the server browser under Multiplayer, LAN. Listed on Internet, it also
  registers with the server directory and shows under Multiplayer, Internet.
  Players outside your network need UDP port 27027 forwarded to you. The game
  starts once everyone is ready. It does not pause.
- Theatre, Leaderboards, and Options on the menu are greyed-out placeholders.
- **Back** or Esc goes back one screen.

A dedicated server runs Zombies with `--mode zombies` (or `zm`) and
`--map zombies` (see [server.md](server.md)). Zombies keeps its own rules:
the server ignores custom game settings, `--bots`, and `--team-bots`, and logs
a line saying so (Black Ops has no Zombies bots). At most four players join
(`zombies::MAX_PLAYERS`). Spectators can still join beyond that. A player who
joins a game under way watches until the next round starts. Career stats and
challenges do not count in Zombies (see [progression.md](progression.md)).

When every player is down or dead, the game ends. The game-over screen shows
the rounds survived, kills, headshots, and points. After the 15 s results, a
new game counts down and starts by itself. **MAIN MENU** leaves.

## The arena

`scenes/zombies_arena.tscn`: a walled yard at night, with a side yard behind
debris through each of its west and east walls.

| Area | What is there |
| --- | --- |
| Main yard (open at the start) | Eight boarded windows. Four wall weapons on the north wall. The Second Wind and Hair Trigger machines, and the mystery crate's first spot, on the south wall. All eight hound spots |
| West yard (debris, 750) | The AK-47 wall weapon, the power switch, Bulwark and Quickload, a crate spot, and two zombie spawns. The electric fence stands across its gate |
| East lot (debris, 1000) | The M40 wall weapon, the forge, Long Stride, a crate spot, and two zombie spawns |

The map marks its parts as named nodes the server finds:

- `Spawns`: zombie spawn points, with optional `zone`, `via`, and `barrier`
  metadata.
- `Barriers`: the windows (`scripts/zombies_barrier.gd`).
- `Doors`: the debris (`scripts/zombies_door.gd`, with `price` and `zone`).
- `WallWeapons`: the racks.
- `PowerSwitch`, `DrinkMachines`, `MysteryCrates`, `Forge`, `Traps`, and
  `HoundSpawns`.

The machines' scenes are in `scenes/zombies/`.

## Controls

The use key (F, or the gamepad's interact button) does everything in Zombies:

- buy a wall weapon, its ammo, debris, a drink, a crate spin, a forge job, or
  the fence;
- throw the power switch;
- hold it at a window to rebuild;
- hold it beside a downed teammate to revive them.

Prompts show the price, or say what is missing. The T refill key is off.
Esc opens the pause menu. With a gamepad, aiming down sights near a zombie
eases the view onto it (Call of Duty's target aim assist, Zombies only;
`rust/sim/src/look.rs`).

## Rounds and the horde

`zombies::Rounds` runs the loop. A 4 s break comes before round 1 and 8 s
between rounds. A round's zombies spawn over time, up to the alive cap. The
round ends when the last one dies.

| Rule | Value |
| --- | --- |
| Zombies a round | 6, 8, 13, 18, 24, then 5 more each round (`zombies_in_round`) |
| Health | 150 in round 1, 100 more each round through round 9, then 10% more each round (`zombie_health`), Black Ops' curve |
| Alive at once | 24 (Black Ops' limit); the rest wait their turn |
| Spawn interval | 2 s in round 1, 5% shorter each round, never under 0.4 s |
| Gaits | Walk 1.4 m/s, jog 2.8, run 4.6, each zombie up to 8% faster or slower than that (`chase::SPEED_JITTER`). Rounds 1 to 3 all walk. Joggers join from round 4 (a fifth more of the horde each round) and runners from round 7 (12% more each round, at most 70%) |
| Spawn points | A random open one, skipping any within 8 m of a standing player while another is free |

Every speed stays under the player's 5 m/s walk and 7.4 m/s sprint, so a
player can always get away. The gait of each zombie is hashed from the round
and its number, so a round plays out the same way each time.

### The chase

Zombies have no pathfinding, by project decision (`zombies/chase.rs`). Each
tick, a zombie heads straight for the nearest standing player, and the physics
slides it along whatever is in the way. Two exceptions keep straight lines
from pinning zombies to walls:

- A zombie spawned in a side yard first walks to that yard's gate (its `via`
  point).
- A zombie that moves at under a quarter of its speed for 1 s walks to the
  open gate on the shortest way to its target, then chases again.

Zombies ignore downed players, and ignore a player who has just been revived
for 2 s.

A zombie stops and swings once its target is within 1.3 m. The swing connects
0.45 s later if the target is still within 1.9 m, and the next can start 1.2 s
after the first. A heavy hit (force level 8, about 75 damage, from a gun or
grenade) staggers a zombie in place for 0.5 s. Hit zones are posed hunched as
the shamble is drawn (`chase::hitbox_pose`), so a headshot aims at the drawn
head.

Zombie bodies collide with the map, each other, players, and window boards.
Player bodies do not collide with zombies. That way, a player's prediction
never runs into a zombie drawn where the server no longer has it. Black Ops
lets zombies body-block players.

### Windows

The eight window spawns sit outside the walls, each tied to its window's
barrier. A zombie from one walks to its window and tears off one of its six
boards every 1.1 s. Once the window is bare, it climbs through and chases.
Bullets pass between the boards. Players are kept inside by a separate
blocker.

To rebuild, look at a damaged window and hold use. One board goes up at once,
then one a second (Black Ops III's `do_post_chunk_repair_delay`). Letting go
or looking away starts over. Each board pays 10 while the player's running
total for the round stays under 50 times the round number, at most 500
(`rebuild_barrier_cap_per_round`). As in Black Ops, the board that reaches the
cap is not paid.

## Getting hit, last stand, and revives

Players have the two-hit rule, this game's own. A hit at full health takes
half. Any hit before health has fully regenerated downs. Bulwark changes this
(see [Drinks](#drinks)). Frags hurt only zombies, so a player falls only by
these rules. Black Ops' own frags can down you.

A hit that would kill downs the player instead: Black Ops' last stand
(`zombies/laststand.rs`, Black Ops III's `_zm_laststand.gsc` numbers).

| Rule | Value | Black Ops III |
| --- | --- | --- |
| Down | Prone on the belly, crawling at the prone crawl's 0.8 m/s, health held at 1 (the screen stays red). The M1911 is loaded, with two magazines in reserve. No grenades, knife, jump, sprint, or weapon switch. They cannot be hurt, and zombies ignore them | Last stand pistol, `doubleclip` |
| Cost of going down | 5% of their points, rounded up to the next 10, and every drink | `penalty_downed`, `round_up_to_ten` |
| Revive | A teammate within 1.9 m holds use for 3 s (1.5 s with Second Wind), with the weapon away. The bleed-out clock stops meanwhile | `revive_trigger_radius` 75 units, `revive_get_revive_time` |
| Revived | Standing, at full health, with their weapons back. Zombies ignore them for 2 s. The reviver earns the points they lost | — |
| Bleed-out | 45 s. Then they are dead until the next round starts, and every other player loses 10% of their points | `player_lastStandBleedoutTime`, `penalty_no_revive` |
| Game over | Every player down or dead. A lone player going down ends the game, unless they hold Second Wind | — |

At the start of each round, the dead come back with the starting kit. After
round 6, anyone coming back has at least 1500 points (`spectator_respawn_player`).

Reviving looks the way Black Ops' revive does. In first person, the left hand
goes flat on the teammate, and the right jabs a syrette in beside it and
presses its plunger (`viewmodel::revive_hands`, `revive_syrette` in
`tools/gear_models.py`). Other players see the reviver kneel over them. See
[zombies-revive.jpg](media/zombies-revive.jpg).

## Points

Every player starts with 500 points (`zombie_score_start`) and the M1911, with
two frags. Each later round adds two frags, up to four.

| Earned by | Points |
| --- | --- |
| A hit that does not kill | 10 |
| A kill | 60 |
| A headshot kill | 100 |
| A knife kill | 130 |
| A board put back | 10, up to the round's cap |
| Kaboom | 400 to every player |
| The Carpenter | 200 to every player |
| Reviving a teammate | The points they lost going down |

Hounds pay as zombies do. Double Points doubles everything in the table but
revives. Kills by Kaboom and the fence pay nothing of their own.

## Wall weapons and debris

Wall weapons hang on racks: look at one and press use. A weapon costs its
price. Ammo for one already carried costs half (Black Ops). Ammo for a
weapon carried upgraded costs 4500 (`get_upgraded_ammo_cost`). The weapon
goes into its own slot, primary or sidearm, as every pickup does.

| Weapon | Price | Where |
| --- | --- | --- |
| M1911 | 500 | Main yard |
| M870 | 900 | Main yard |
| PDW-9 | 1000 | Main yard |
| M4A1 | 1200 | Main yard |
| AK-47 | 1400 | West yard |
| M40 | 1500 | East lot |

Debris blocks each side yard's gate: 750 west and 1000 east. Clearing it
opens the yard, adds its zombie spawns to the rotation, and makes its gate a
waypoint for stuck zombies.

## Power

One lever (`PowerSwitch`, `scripts/zombies_power_switch.gd`) stands in the
west yard, behind its debris. A press turns the power on for the rest of the
game, free. It lights two lamps and every machine's sign. A new game starts
with it off. This follows Black Ops' power switch (`_zm_power.gsc`).

The drink machines, the forge, and the fence need the power. The mystery crate
does not. Second Wind in a game for one does not either (Black Ops'
`use_solo_revive`). A map without a power switch is always powered.

## Drinks

Five machines (`DrinkMachines`, `scripts/zombies_drink_machine.gd`) sell
Black Ops' perks under our own names (`zombies/drinks.rs`). Each is a cabinet
in its drink's colour, with a lit header that comes on with the power. A press
of use buys the drink.

| Drink | Effect | Price | Black Ops III |
| --- | --- | --- | --- |
| Bulwark | Down on the fifth zombie hit instead of the second. Each hit takes a fifth of full health, and health regenerates between hits as usual | 2500 | `specialty_armorvest` (Jugger-Nog): a player survives four hits and goes down on the fifth |
| Second Wind | Revive teammates in half the time (1.5 s). Alone, get back up by yourself 10 s after going down | 1500, alone 500 | `specialty_quickrevive`: `revive_get_revive_time`, `solo_revive_time` 10 s, `revive_cost_override` |
| Quickload | Reloads take half the time | 3000 | `specialty_fastreload`, `perk_weapReloadMultiplier` 0.5 |
| Hair Trigger | A third more rounds a minute, and every round does double damage to zombies | 2000 | `specialty_doubletap2`: `perk_weapRateMultiplier` 0.75, each bullet fired twice |
| Long Stride | Move 7% faster | 2000 | `specialty_staminup` |

- A player holds at most four drinks (`level.perk_purchase_limit`). A drink
  already held is not sold again.
- Going down loses every drink, Second Wind included, as in Black Ops.
- **Second Wind alone.** In a game with one player, Second Wind costs 500 and
  works without power. A player who holds it and goes down gets back up after
  10 s, with the last-stand pistol in between. The game goes on meanwhile.
  Nobody earns the points lost going down. After the third purchase, the
  machine leaves (`level.solo_lives_given >= 3`). Whether a game is for one is
  decided at each purchase and each fall. Clients are told the machines'
  prices again whenever a player joins or leaves.
- **Prediction.** Quickload, Hair Trigger, and Long Stride change handling.
  The drinks held ride in the predicted player state (`PlayerState::drinks`,
  five bits), and the client predicts them like any other movement or weapon
  rule. Bulwark and Hair Trigger's damage are the server's.

Drinking puts the weapon down for 2.3 s (`Raise::Drink`,
`weapon::DRINK_SECONDS`), with no fire meanwhile, as Black Ops' perk bottle
plays. In first person, the right hand brings the bottle up and the left thumb
flicks its cap off. Three gulps follow with the bottle tipped to the mouth,
and then it is tossed away down to the right (`viewmodel::drink_hands`; each
bottle is `drink_bottle_<key>` in `tools/gear_models.py`). See
[zombies-drink.jpg](media/zombies-drink.jpg). Other players see no bottle:
their weapon stays in hand.

Long Stride does not lengthen sprint, because this game's sprint is already
unlimited (a recorded decision).

## The mystery crate

An olive supply crate with a die stencilled on it (`rust/game/src/mystery_crate.rs`,
rules in `zombies/mystery.rs`). It stands at one of the map's three
`MysteryCrates` spots: the main yard's at the start, then the west yard's and
the east lot's. It needs no power.

| Rule | Here | Black Ops III (`_zm_magicbox.gsc`) |
| --- | --- | --- |
| Price | 950 | `zombie_treasure_chest_cost` |
| Cycle | Weapons rise out of it and cycle for 4 s, slowing down: 20 at 0.05 s, 10 at 0.1 s, 5 at 0.2 s, 3 at 0.3 s, then 0.1 s | The same 40-step loop in `treasure_chest_weapon_spawn` |
| Pool | M4A1, AK-47, BR-3, KR-12, PDW-9, CQ-10, M870, SG-12, M40, DM-7, and LSW-6, equally weighted (`mystery::POOL`). Never one the buyer carries in any form. Not the M1911 | Box weapons at weight 1 unless a map weights them; `treasure_chest_canplayerreceiveweapon` skips carried ones |
| Taking it | Only the buyer, with use while looking at it, into its slot. It sinks back over 12 s, and the crate closes if it is left | `treasure_chest_timeout` 12 s |
| Moving | Never in the first 3 spins at a spot. 15% from the 4th to the 7th. Certain by the 8th the first time. After a move, 30% from the 8th to the 12th and 50% from the 13th | `treasure_chest_should_move` |

When the crate moves, it lands on the duck instead of a weapon. The duck is
our own sign for the move: a rubber duck, built in Blender. It floats up for
3 s, and the buyer gets the 950 back. The crate lifts away and shows up at
another spot 6 s later.

The server sends `ZombiesEvent::Crate` (spot, phase, buyer, weapon, and when
the phase ends) on every change and on joining. The weapon is sent when the
cycle starts, so the display lands on it. Taking it is an ordinary predicted
pickup: the client's prompt treats a crate holding a weapon out to this player
as a free rack, and the server checks that it is.

## The forge

The upgrade machine, Black Ops' Pack-a-Punch under our own look: a
furnace-press in the east lot, lit when the power is on
(`rust/game/src/forge.rs`, rules in `zombies/forge.rs`).

| Rule | Here | Black Ops III |
| --- | --- | --- |
| Needs | The power on, 5000 points, and a weapon in hand that has an upgrade, with another weapon carried | `vending_weapon_upgrade_cost` 5000; Pack-a-Punch is powered |
| Working | The weapon goes in and the other comes up. 3.35 s later the upgrade comes out, full, for its owner alone | `wait(0.35)` + `wait(3)` in `_zm_pack_a_punch.gsc` |
| Taking it | Use while looking at it, like a rack. Left for 15 s, it is lost | `level.pack_a_punch.timeout` 15 s |
| The upgrade | Twice the damage, half again the magazine, twice the reserve, no attachments, "MK II" after its name, and a cold blue sheen in first and third person | Set per weapon in Black Ops; these are its typical changes |
| Ammo | A wall rack of the plain weapon refills the upgraded one for 4500 | `get_upgraded_ammo_cost` 4500 |

Upgraded weapons are the weapon table's `<key>+forged` variants
(`weapon::def::forged`), one for every weapon except the laser. The upgrade
rides in the weapon id like an attachment: it is predicted, sent in
snapshots, and drawn with the plain weapon's model and sounds plus the sheen
(`shaders/forged.gdshader`, `WeaponView::set_forged`). A lone sidearm cannot
be upgraded, since that would leave the player with nothing in this game's
two slots.

### Alternate ammo

An upgraded weapon can go back in for 2500 (Black Ops III's re-pack,
`aat_cost`). The same weapon comes out, full, with an alternate ammo type
drawn at random, never the one it had (`zombies/alt_ammo.rs`). A first
upgrade has none, as in Black Ops III. The HUD names it after the weapon,
where an attachment goes ("M4A1 MK II · ARC").

| Ammo | Here | Black Ops III |
| --- | --- | --- |
| Arc | A hit sets it off 20% of the time. The zombie hit is electrocuted, and the charge jumps to the nearest zombie in reach, and on: up to 8 jumps and 9 kills, with a reach of 3.05 m at first and 0.51 m less each jump. 5 s before the same player's weapon can set it off again, 2 s before anyone's can | Dead Wire (`_zm_aat_dead_wire.gsc`): `aat::register(…, 0.2, 0, 5, 2, 1, …)`, `create_lightning_chain_params(8, 9, 120)`, radius decay 20 units |
| Cinder | 15% of the time. The zombie hit bursts into flame and dies, and every zombie within 3.05 m burns, losing a sixth of its health every 0.5 s from 0.25 s on, seven times. 15 s before the same player's weapon can set it off again | Blast Furnace (`_zm_aat_blast_furnace.gsc`): `aat::register(…, 0.15, 0, 15, 0, 1, …)`, 120 units, `self.health / 6` seven times |

Either can go off on the killing hit, and only from shots (not the knife or
grenades). Everything it kills counts and pays the shooter 60. Clients see the
Arc as a cold bolt jumping between the struck, and Cinder as a burst of flame
(`scripts/zombies_alt_ammo_fx.gd`, `ZombiesEvent::AltAmmoFired`). There are
two types, not Black Ops III's five (no Fireworks, Thunder Wall, or Turned).

## The electric fence

A trap across the west gate, after Black Ops' electric trap (`Traps/WestGateFence`,
`scenes/zombies/electric_trap.tscn`, `scripts/zombies_trap.gd`; rules in
`zombies/traps.rs`). It is five wires on an 8 m run between two steel posts,
2.4 m high and 1 m deep, with a switch box on the wall beside the gate that
buys it.

| Rule | Here | Black Ops III (`_zm_traps.gsc`, `_zm_trap_electric.gsc`) |
| --- | --- | --- |
| Price | 1000, with the power on | `zombie_cost` 1000; traps wait for `power_on` |
| On | 40 s, then 60 s cooling down before it can be bought again | `_trap_duration` 40, `_trap_cooldown_time` 60 |
| Zombies | Any zombie or hound in it dies within 1.25 s. Nobody is paid and nothing drops | `zombie_trap_death`'s `wait(randomfloat(1.25))` |
| Players | A player who walks in goes down. With Bulwark, it takes a fifth of their health instead, every 2.5 s they stay | `player_elec_damage`: down without Jugger-Nog, 50 of 250 with it |

The fence blocks nobody: zombies and players walk through it, as in Black
Ops. Its wires glow and flicker while it is on. The models are `trap_fence`,
`trap_fence_lit`, and `trap_switch` in `tools/gear_models.py`. It does not
shake the screen of a player it shocks.

## Power-ups

Kills drop glowing tokens. Any standing player who walks over one (within
1.6 m) takes it for the whole team. When a drop comes, and which one, follow
Black Ops III's `_zm_powerups.gsc` (`zombies/powerups.rs`).

| Rule | Here | Black Ops III |
| --- | --- | --- |
| Earned drops | Once the team has earned 2000 points together, the next kill drops one. The step grows 14% each time and counts from the points earned then | `watch_for_drop`: `zombie_powerup_drop_increment` 2000, times 1.14 per drop |
| Lucky drops | Any kill also drops one 3 times in 100 | `powerup_drop`: `randomint(100) <= 2` |
| Per round | Four at most | `zombie_powerup_drop_max_per_round` 4 |
| Which | Dealt in turn from a shuffled deck of every kind, shuffled again when it runs out. A kind that cannot drop now is passed over | `get_next_powerup`, `get_valid_powerup` |
| On the ground | 15 s still, then 11.5 s blinking (half-second, then quarter, then tenth), then gone | `powerup_timeout` |

| Power-up | Effect | Black Ops III |
| --- | --- | --- |
| Max Ammo | Every player still in the game gets full ammo | `full_ammo` |
| Insta-Kill | Any hit kills, for 30 s | `zombie_powerup_insta_kill_time` |
| Double Points | Points doubled, for 30 s | `zombie_powerup_double_points_time` |
| Kaboom (the nuke) | Every zombie in play dies, dropping nothing, and every player gets 400 | `nuke` |
| Carpenter | Drops only once five windows are bare. Boards go back on every window, nearest to where it was taken first, one every 0.1 s. Then every player gets 200. A second one taken while one works restarts it from there | `func_should_drop_carpenter`, `start_carpenter` |
| Fire Sale | Drops only once the crate has moved, and not while one is on. For 30 s, a crate stands at every spot and a spin costs 10 anywhere. A Fire Sale spin never lands on the duck or counts towards a move. When it ends, each spare crate goes once its spin is done. Another taken while one is on adds 30 s | `func_should_drop_fire_sale`, `toggle_fire_sale_on` (`zombie_cost = 10`), `_box_opened_by_fire_sale` |

Points earned toward a drop count everything paid (hits, kills, boards, and
Kaboom's and the Carpenter's pay), doubled under Double Points, as Black Ops'
`score_total` does.

Each token is its own Blender model (`tools/gear_models.py`, shown by
`scripts/zombies_power_up.gd`): an ammo can, a skull, "x2", a bomb, a
carpenter's tote with a saw standing in it, and a price tag stamped "10". The
token spins and bobs with its name above it.

A crate travelling between spots when a Fire Sale starts lands at once. Black
Ops instead holds it hidden until the sale ends.

## Hound rounds

Every few rounds, the zombies stay away and a pack of hounds comes instead:
Black Ops' dog rounds under our own name and look (`zombies/hounds.rs`).

| Rule | Here | Black Ops III (`_zm_ai_dogs.gsc`) |
| --- | --- | --- |
| When | The first in round 5, 6, or 7, then every 4 or 5 rounds | `dog_round_tracker`: `randomintrange(4, 7)` from round 1, then `randomintrange(4, 6)` |
| Pack | 6 a player in the first two hound rounds, 8 after | `dog_round_spawning`: `players.size * 6`, then `* 8` |
| Health | 400, 900, 1300, then 1600 | `dog_health_increase` |
| At once | Two a player | `num_player_valid * 2` |
| Pace | The first 7 s in, then every 3, 2.5, 2, or 1.5 s by hound round, less the share of the pack already sent (at least 0.05 s) | `wait(1)` + `wait(6)`, `waiting_for_next_dog_spawn` |
| Arrival | A strike lands at a hound spot (`HoundSpawns`) 10 to 25 m from the player it hunts, not the last spot used. The hound stands there 1.5 s later | `dog_spawn_factory_logic` (400 to 1000 units), `dog_spawn_fx` (`wait(1.5)`) |
| Bite | 40% of full health, so three bites down a player without a rest; 16% with Bulwark (seven). Reach 1.6 m, through to 2.3 m | `meleedamage` 40 of 100 health (250 with Jugger-Nog); `ai_meleeRange` 100 units |
| Speed | 6.2 m/s: faster than any zombie, a little slower than a sprint (7.4) | `zombie_move_speed = "sprint"` |
| End | The last hound killed leaves a Max Ammo where it fell | `dog_round_aftermath` |

Hounds run straight at the nearest standing player like zombies, ignore downed
players, and never stagger. When killed, they flare cold and crumble to ash
instead of falling as ragdolls, as Black Ops' hounds burst. The HUD calls the
round "ROUND n  HOUNDS" and counts "HOUNDS LEFT".

The hound is our own creature (`tools/build_hound.py`, built, rigged, and
animated in Blender, drawn by `rust/game/src/hound.rs`): a gaunt, hairless,
ash-grey hound, long in the leg, with a ridge of bone spurs, bared teeth, and
cold blue-white eyes, where Black Ops' burn orange. Its strike is a cold light
that crackles on the spot for 1.5 s before the bolt falls
(`scripts/zombies_hound_strike.gd`). There is no fog change for the round, and
all hound spots are in the main yard.

## HUD and scoreboard

`scripts/zombies_hud.gd` and `scripts/zombies_drinks_row.gd`:

- The round: tally marks for rounds 1 to 5 and numbers after, flashing white
  when a round starts.
- Points, with "+N" and "-N" popups.
- "ZOMBIES LEFT n", "HOUNDS LEFT n", or "NEXT ROUND IN n".
- The timers of Insta-Kill, Double Points, and the Fire Sale.
- Banners: "GET READY", "ROUND n", "ROUND n SURVIVED", and each power-up's
  name.
- Last stand: "BLEEDING OUT n", "REVIVING name", "BEING REVIVED BY name", or
  "GETTING BACK UP", with a revive bar, and "Hold [use] to revive name" beside
  a downed teammate.
- The drinks held, as a row of badges in the corner in the order bought.
  Each is in its machine's colour with a glyph: a shield, a cross, a magazine,
  a round, or chevrons.
- The game-over screen.

Every element scales and fades with the HUD size and opacity settings
(`Settings.fit_hud`), about the corner or edge it hangs from.

The match scoreboard (`scripts/net/zombies_board.gd`) is one team,
"SURVIVORS", ordered by points. Its columns are points, kills, downs, revives,
headshots, and ping, as in Black Ops' co-op board. The dead show faint.

## Online

- **Snapshots.** Zombies and hounds ride in snapshots every 4 server ticks
  (30 Hz). Each `protocol::ZombieView` carries centimetre positions, a byte of
  facing, a hound bit, and wrapping swing and hit counters, sent field by field
  as changes from the acknowledged snapshot.
- **Lag compensation.** Clients draw zombies one send interval behind other
  players (`protocol::zombie_lag`). The server rewinds them by the same amount
  for lag compensation. It uses their hunched hit zones, and hounds' four-legged
  zones (`hitbox::hound_hitboxes`).
- **Drawing.** `net/horde_view.rs` draws each one as a puppet `Zombie` or
  `Hound`, playing swings, flinches, and deaths as the counters change.
- **Events.** Everything else is a `ZombiesEvent`: round, points, boards,
  doors, drops, timers, downs and revives, power, machines, drinks, the crate,
  the forge, traps, hound strikes, and alternate ammo. Each is sent on change,
  and in full to a player who joins.

## Look and sound

- **Zombies.** Four zombie bodies (`assets/characters/zombie*`) share the
  soldiers' rig. Far from the camera, they animate every second frame beyond
  18 m and every fourth beyond 32 m. A killed zombie falls as a ragdoll and
  lies for 10 s.
- **Sound.** Zombies groan as they come, snarl when they swing, and cry out
  when they die. A sting marks each round's start and end
  (`assets/audio/zombies/`).
- **Missing sounds.** Hounds, drinks, the machines, the crate, the forge, and
  the fence have no sounds. They need real CC0 recordings (not built yet; see
  `TODO.md`).

The arena's dressing and lighting are in [art-direction.md](art-direction.md).

## Tests

- **Rust.** Unit tests sit beside each rule in `rust/sim/src/zombies.rs` and
  `rust/sim/src/zombies/`.
- **Godot.** The tests are `godot/tests/zombies_*_test.gd`:
  - `server`, `net`, and `local` cover the dedicated server, clients, and the
    title screen's local servers.
  - `drinks`, `crate`, `forge`, `hounds`, and `trap` cover the machines.
  - `menu` and `listing` cover the menu and the Internet listing.
- **Captures.** The `capture_zombies_*.gd` scripts take the screenshots.

## Reference

- Black Ops III's decompiled scripts:
  - Hounds: `_zm_ai_dogs.gsc`.
  - The fence: `_zm_traps.gsc` and `_zm_trap_electric.gsc`.
  - Power-ups: `_zm_powerups.gsc`, `_zm_powerup_carpenter.gsc`, and
    `_zm_powerup_fire_sale.gsc`.
  - Rebuilding: `_zm_blockers.gsc` and `_zm.gsc`.
  - The forge: `_zm_pack_a_punch.gsc` and `_zm_pack_a_punch_util.gsc`, with
    `_zm_weapons.gsc` for the upgraded ammo cost.
  - The crate: `_zm_magicbox.gsc`.
  - Drinks: `_zm_perks.gsc` and the `_zm_perk_*.gsc` scripts.
  - Last stand: `_zm_laststand.gsc`, `_zm_score.gsc`, and `_zm.gsc`
    (`wait_and_revive`).
  - Power: `_zm_power.gsc`.
  - Alternate ammo: `aats/` and `aat_shared.gsc`.
- Hit counts with and without the health perk: the Call of Duty wiki's Black
  Ops III perk page (four hits survived, down on the fifth).
- The menu: Black Ops 2's Zombies menu and globe.
