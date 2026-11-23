# Progression

Black Ops 2's long-term progression: XP and levels, career stats, challenges that pay XP, weapon camos, calling cards, prestige, and a combat record. What each level and weapon kill unlocks (weapons, attachments, perks, wildcards, scorestreaks) is in [create-a-class.md](create-a-class.md).

Where progress is kept ([online.md](online.md)):

- **Signed in:** the account keeps it (`docker/online/nakama/modules/progress.lua`), and a ranked server reports matches to it.
- **Signed out:** the game keeps it in `user://progress.cfg` (`godot/scripts/progress_store.gd`).

On the first sign-in, the game's local progress is imported once into an account that has none, prestige included, with no more unlock tokens than prestiges.

## XP and levels

The rules are in `rust/sim/src/progression.rs`, as classic Call of Duty (Call of Duty 4 through Black Ops 3) has them.

| Rule | Here |
| --- | --- |
| Score is XP | Every point scored in a match is XP: kills, headshot and medal bonuses, objective awards |
| Match bonus | 100 XP a minute played (to the second, at most 60 minutes), half again for a win (`match_bonus`). A player who leaves early keeps their score but gets no bonus |
| Challenges | Each pays its XP in the match that completes it (see [Challenges](#challenges)) |
| Levels | 1 to 55, as in Black Ops 2 and 3. Level 1 to 2 takes 600 XP, and each level 400 more than the last (`xp_to_next`): 604,800 XP to level 55 |
| Create-a-Class | Opens at level 4, as in Black Ops 2 and 3 (`CREATE_A_CLASS_LEVEL`) |

The game works out a match's XP when it ends (`godot/scripts/net/match_xp.gd`) and shows it on the XP card beside the results (`godot/scripts/net/xp_summary.gd`): score, bonus, challenge XP and the challenges completed, the level bar, and what any level reached unlocks. [create-a-class.md](create-a-class.md) has a picture of the card.

## Stats

The server counts each player's stats through a match (`rust/sim/src/stats.rs`, `Recorder`) from the events it sends: shots, hits, kills, deaths, score awards, and scorestreaks earned. Black Ops 2 does the same: its game server keeps every player's stats and per-weapon stats (`_persistence.gsc`), and every challenge is a target on one of those counters.

| Stats | Keys |
| --- | --- |
| Career | `kills`, `deaths`, `headshots`, `shots`, `hits`, `score`, `time` (seconds), `matches`, `wins`, `losses`, `draws`, `longshots`, `revenge`, `doubles`, `triples` (three kills or more), `bloodthirsty` (5 kills without dying), `merciless` (10), `collateral`, `point_blank`, `aircraft` (enemy aircraft shot down) |
| Other kills | `knife.kills`, `frag.kills`, `charge.kills` |
| Per weapon | `w.<weapon>.hs`, `.long`, `.bare` (no attachments), `.noperk` (a class with no perks), `.double`, `.bt` (bloodthirsty), `.revenge`, `.ohk` (one-hit kills: a full health bar in one hit), `.shots`, `.hits`, `.deaths` (while holding it) |
| Objectives | `a.<award>`: `plant`, `defuse`, `wire_cutter`, `clutch`, `ace`, `flag_taken`, `flag_capture`, `flag_return`, `carrier_kill`, `hp_capture`, `hp_secure`, `hp_defend`, `hp_assault`, `dom_capture`, `dom_defend`, `dom_assault` |
| Scorestreaks | `s.<streak>.earned`, `s.<streak>.kills`, `s.<streak>.downed` (enemy ones shot down) |
| Modes | `m.<mode>.played`, `m.<mode>.wins` |

Stats are kept under each weapon's plain key, so every variant of the M4A1 counts as `m4`. Kills with each weapon are kept apart, in `weapon_kills`, because attachments unlock by them.

How the stats move:

1. The server sends each player their match's stats (`GameEvent::Stats`) at most every two seconds while they change, and once more when the match ends.
2. The game adds them to the player's totals with the match's XP (`ProgressStore.add`, which calls `Progression.add_match`).
3. On a ranked server, the server reports them to the account itself (`server_progress_add`).

The server does not count:

- bots;
- Zombies (Call of Duty keeps Zombies' stats apart);
- time outside play: the lobby, the countdown, and the results.

A player who leaves mid-match keeps their stats. A ranked server reports what they had; elsewhere the game keeps the stats it was last sent, at most two seconds old.

## Challenges

A challenge is a target on a stat (`rust/sim/src/challenge.rs`).

- It completes in the match that takes the stat past the target, and pays its XP in that match.
- Completion is never stored: it is worked out from the totals, so the game, the ranked server, and the account always agree, and each challenge pays once.
- During a match, each challenge is announced the moment it is done, as Black Ops 2 does: a quiet "CHALLENGE COMPLETE" plate near the top of the screen with its name, XP, and reward (`godot/scripts/net/challenge_popup.gd`). Plates show one at a time for 3.2 s; at most four wait, and older ones are dropped. `match_xp.gd` works them out from the stats the server sends, as the match would pay them if it ended then.
- The end-of-match XP card shows the challenges' XP on its own line and lists the challenges completed, with the camo or card each awarded.

![A challenge completed mid-match](media/challenge-popup.jpg)

### Weapon challenges: the camo ladder

Every weapon except the laser (the one special weapon) has 14 challenges, each earning one camo, in Black Ops 2's order for its class.

| Class | Camos 1 to 9 | Camo 10 | Camos 11 and 12 | Camo 13 | Camo 14 |
| --- | --- | --- | --- | --- | --- |
| Assault rifles, marksman rifles | Headshots: 5, 10, 15, 20, 30, 40, 50, 75, 100 | 10 longshots | 150 kills with no attachments; 150 kills with no perks | 20 double kills | 5 kills without dying, 10 times |
| SMGs, LMGs, pistols | Headshots, as above | 30 revenge kills | 150; 150 | 20 double kills | 10 times |
| Shotguns | One-hit kills: 5 to 100, as above | 30 revenge kills | 50; 50 | 5 double kills | 10 times |
| Sniper rifles | One-hit kills: 5, 10, 25, 50, 75, 100, 150, 200, 250 | 10 longshots | 50; 50 | 5 double kills | 10 times |
| Launchers | Kills: 5, 10, 20, 30, 40, 50, 60, 75, 100 | 10 longshots | 150 kills; 50 kills with no perks | 5 double kills | 5 times |

Past the ladder:

- **Gold:** all 14 of a weapon's challenges done.
- **Mastery:** Gold on every weapon of a class. This is Black Ops 2's Diamond.
- **Nebula:** mastery in every class. This is Black Ops 3's Dark Matter; Black Ops 2 had no camo for every class. It also awards the Nebula calling card.

Deliberate differences from Black Ops 2:

- **Launchers** count kills, where Black Ops 2 counted aircraft destroyed. Aircraft shot down count toward a career challenge instead, with any weapon. Launchers take no attachments, so every launcher kill is a bare one; a 150-kill challenge stands in for the attachment challenge.
- **Marksman rifles** are their own class here (Black Ops 2's were assault rifles), so they climb the assault rifle ladder.

### Career, mode, and scorestreak challenges

These are tiered, as Black Ops 2's are, and the last tier of most awards a calling card:

- **Career:** kills, headshots, wins, matches finished, longshots, revenge kills, double kills, triple kills, 5 and 10 kills without dying, knife kills, frag kills, collaterals, and aircraft shot down.
- **Modes:**
  - wins in Team Deathmatch, Free-for-All, Domination, Hardpoint, Capture the Flag, Search and Destroy, and Gun Game;
  - flag captures and defends in Domination;
  - zone captures and defends in Hardpoint;
  - captures, returns, and carrier kills in Capture the Flag;
  - plants, defuses, Wire Cutters, Clutches, and Aces in Search and Destroy.
- **Scorestreaks:** times the UAV, Counter-UAV, Care Package, and Airstrike are earned; kills with the Sentry Gun, Guided Missile, Attack Chopper, and Airstrike.

### Challenge XP

Black Ops 2's challenges pay 250 to 5,000 XP each, and a weapon's whole ladder comes to about 1.4% of the XP to level 55 there (Call of Duty wiki, "Challenges/Call of Duty: Black Ops II"; per-tier figures are not published, so this is approximate). Ours keep that share of our shorter curve:

| Challenges | XP |
| --- | --- |
| Camos 1 to 9 | 250, 250, 500, 500, 500, 750, 750, 1,000, 1,000 |
| Camos 10 to 14 | 500 each |
| Gold | 1,000 |
| A weapon's whole ladder | 9,000 of the 604,800 to level 55 |
| Mastery and Nebula | None, as Black Ops 2's Diamond paid none |
| Career, mode, and scorestreak tiers | 500 to 5,000, as Black Ops 2's |

## Camos

Each weapon has 14 ladder camos, Gold, Mastery, and Nebula, our own designs (`rust/sim/src/camo.rs`). The weapon's own finish is called Factory.

| Kind | Camos |
| --- | --- |
| Ladder, 1 to 14 | Gravel, Dune, Timber, Tundra, Cocoa, Riptide, Ember, Static, Scales, Weave, Bloom, Ink, Hazard, Circuit |
| Past the ladder | Gold, Prism (mastery), Nebula (every class) |

![Every camo pattern: the ladder, Gold, Prism (shown as the data its shader reads), and Nebula](media/camo-patterns.jpg)

- **Patterns.** `tools/build_camos.py` paints each pattern as a seamless 512 px texture (`godot/assets/camos/`). It needs only numpy and is seeded, so it rebuilds the same files.
- **Shader** (`godot/shaders/weapon_camo.gdshaderinc`; per-camo scale, metal, and style in `rust/game/src/camo_look.rs`):
  - It projects the pattern onto the gun from three sides in the gun's own space, so it needs no UVs.
  - It paints the coated parts: whatever the weapon's ORM map does not mark as bare metal.
  - Edges worn to bare metal stay bare, and the finish's grime and occlusion darken the paint, as Call of Duty's camos sit under the weapon's wear.
  - **Gold** is polished metal, **Prism** has facets whose hue turns with the view, and **Nebula** has slowly drifting clouds with glinting stars.
- **No extra draw calls.** In first person a camo is a few parameters on the material each surface already has (`viewmodel.gdshader`). In third person and killcams the weapon's shared material is swapped for one with the camo painted in (`weapon_camo.gdshader`), cached per material and camo, so every M4A1 in Gold shares one. `godot/tests/camo_draws_test.gd` checks that the draw count holds.
- **Create-a-Class.** Each weapon of a class takes a camo (`primary_camo`, `secondary_camo`), chosen from the camos that weapon has earned; a note says how many more its challenges hold. As in Black Ops 2, a camo costs no point. Changing the weapon resets its camo.
- **Others see it.**
  - The game sends its cosmetics with its class (`ClientMessage::Cosmetics`): the prestige, the calling card, and the camo on each weapon (`rust/sim/src/cosmetics.rs`). The server passes them to everyone (`GameEvent::Cosmetics`).
  - Other players' third-person weapons wear the camo.
  - In a killcam, the first-person weapon wears the killer's camo.
  - A ranked server takes the prestige from the account and drops any card or camo the account has not unlocked (`Cosmetics::checked`). Other servers pass on what the game sends.

![The M4A1 in Gold, in first person](media/camo-gold-first-person.jpg)

![The AK-47 in Dune and in Prism, inspected](media/camo-inspect.jpg)

![Another player's M4A1 in Gold and in Dune](media/camo-third-person.jpg)

![Create-a-Class: the M4A1's earned camos, with their patterns](media/camo-picker.jpg)

## Prestige

Black Ops 2's prestige (`rust/sim/src/progression.rs`):

- **When.** At level 55, the player may enter the next prestige from the career screen's Prestige tab, after confirming (`godot/scripts/ui/prestige_page.gd`).
- **What resets.** The level goes back to 1. Everything a level unlocks locks again until it is reached, Create-a-Class (level 4) included, as in Black Ops 2.
- **What it earns.**
  - The next prestige's emblem and calling card.
  - A permanent unlock token. A token unlocks one weapon, perk, wildcard, tactical, or scorestreak for good: it stays unlocked through every later prestige. The Prestige tab lists the items still locked to spend it on.
- **What is kept.** Kills with each weapon (and so its attachments), stats, challenges, camos, calling cards, the combat record, and everything unlocked with a token.
- **How many.** Ten prestiges, then **Master**, the last.
  - At Master every item unlocks for good, as Black Ops 2's Prestige Master does, and no token comes with it.
  - The level keeps counting XP to 55 and stays there.
- **Where it is done.**
  - Signed in, the account does it (`progress_prestige`, `progress_unlock`). It checks the level itself, so a game cannot prestige early.
  - Signed out, the game does it (`ProgressStore.prestige_up`, `ProgressStore.unlock_permanent`).
  - Ranked servers check classes against the account's level, prestige, and permanent unlocks (`Earned::check_class`).
- **Emblems** (our own designs, drawn as vector shapes by `godot/scripts/ui/prestige_emblem.gd`):
  - A hexagonal badge whose metal climbs with the prestige: steel for 1 to 3, bronze 4 to 6, gold 7 to 9 (one to three chevrons in each band), platinum with a star for 10, and crimson with a ringed star for Master.
  - It shows beside the name on the scoreboard and in the kill feed, and on the career screen.

![The Prestige tab: every emblem, entering the next prestige, and spending a token](media/prestige-tab.jpg)

![Prestige emblems and calling cards on the scoreboard, and emblems in the kill feed](media/prestige-scoreboard.jpg)

## Calling cards

A calling card is the banner a player shows (`rust/sim/src/card.rs`). There are 40, all our own designs:

| Cards | Earned by |
| --- | --- |
| Recruit | Everyone has it from the start |
| 27 cards | The last tier of most career, mode, and scorestreak challenges, as in Black Ops 2 |
| Nebula | Mastery in every weapon class |
| Prestige I to X, and Master | Reaching that prestige |

- **Art.** Each card is drawn as vector shapes from its key (`godot/scripts/ui/calling_card.gd`), so it is sharp at any size and needs no texture. A card is a base colour, an accent, and a motif: stripes, chevrons, a crosshair, a star, rings, a blade, bars, hexes, a burst, peaks, a grid, or waves. Prestige cards put the prestige's emblem on a band of its metal.
- **Picking.** The career screen's Calling Cards tab (`godot/scripts/ui/cards_page.gd`) shows every card.
  - Earned cards can be picked; the one shown is outlined.
  - The rest are dim, with what earns them.
  - The pick is kept with the classes (`ClassStore.set_card`), so it follows the account, and goes to the server with the class as part of the player's cosmetics.
  - A ranked server shows only a card the account has earned.
- **Where it shows:**
  - faint behind the player's name on the scoreboard;
  - under the killer's name card in a killcam, with their prestige emblem;
  - in the career screen's header.
- **Bots** show one too: a prestige from 0 to 3 and a career card, fixed by their id, as Call of Duty's bots wear ranks and cards.

![The Calling Cards tab](media/calling-cards.jpg)

![A killcam's killer card on the killer's calling card](media/calling-card-killcam.jpg)

## Career screen

**Career**, beside Options on the title screen, opens the player's challenges, combat record, prestige, and calling cards (`godot/scripts/ui/career_menu.gd`), as Black Ops 2's Barracks holds them. Its header shows the player's emblem, calling card, level, and XP to the next level. It has four tabs:

- **Challenges.** Four categories: Weapons, Career, Modes, and Scorestreaks.
  - The list on the left picks a weapon (grouped by class, with Mastery last) or a mode, each with how many of its challenges are done.
  - The panel lists that section's challenges: name, what to do, progress ("12 / 20", or COMPLETE), what it awards (a camo or a calling card), and its XP.
- **Combat Record.** Career totals, as Call of Duty's combat record shows them: K/D, win rate, accuracy (hits over shots fired), time played in matches, score per minute, and headshots. Below them is a line for each weapon, most kills first: kills, deaths while holding it, K/D, headshots, accuracy, and camos earned.
- **Prestige** and **Calling Cards**, as above.

![The career screen's weapon challenges: the M4A1's camo ladder with its progress and rewards](media/career-weapons.jpg)

![The combat record: career totals and each weapon's line](media/career-record.jpg)

## Account and caps

The account stores `xp`, `weapon_kills`, `stats`, `prestige`, `tokens`, and `permanent` (`docker/online/nakama/modules/progress.lua`). A match's report already includes its challenges' XP: the game or the ranked server works it out from the totals, and the account only adds. Call of Duty keeps rank on its own servers, so a game cannot hand itself unlocks; matches the game reports itself are capped by time instead.

| Report | Caps |
| --- | --- |
| Self-reported by the game (`progress_match`: local matches and servers that do not report to the account) | Per minute since the last award, counting at most 30 minutes: 1,500 XP, 20 weapon kills, and 600 of each stat (shots fired run fastest); `time` at most the time itself |
| Ranked server (`server_progress_add`) | 100,000 XP, 500 kills a weapon, and 100,000 of each stat per report |
| Kept on the account | At most 1,024 stat keys (192 in one report), each `a-z0-9_.` and up to 32 bytes |

Whether private matches pay XP is covered in [custom-games.md](custom-games.md).

## Call of Duty reference

- **XP and levels:** score is XP from Call of Duty 4 through Black Ops; Black Ops 2 based its match bonus on time played and mode, and Modern Warfare (2019) onward pays more for a win. Black Ops 2 and 3 have 55 levels, each needing more XP than the last, and open Create-a-Class at level 4.
- **Career screen:** Black Ops 2's Barracks holds the challenges (by weapon, career, mode, and scorestreak, each with its progress and reward) and the combat record (K/D, win/loss, accuracy, time played, and per-weapon kills, deaths, headshots, and accuracy).
- **Challenge popups:** Black Ops 2 announces each challenge mid-match as it completes, with its name and reward.
- **Stats and challenges:** Black Ops 2's challenge tables are targets on its player and weapon stats, kept by the game server. A challenge completes, and its XP is paid, in the match that crosses it.
- **The camo ladder:**
  - The numbers for each class follow games.gg, "Black Ops 2: All Weapon Camos & Challenges", and gamerant.com, "Black Ops 2 camo challenges guide": the headshot ladder 5 to 100; then longshots or revenge kills, 150 kills with no attachments, 150 with no perks, 20 double kills, and 10 Bloodthirsty medals.
  - Shotguns and snipers climb one-shot kills instead.
  - Gold is all of a weapon's camos; Diamond is Gold on every weapon of a class (Call of Duty wiki, "Diamond Camouflage"); Dark Matter is Black Ops 3's camo for every class.
- **Camos:** a camo is chosen per weapon in Create-a-Class, costs no point, sits under the weapon's wear, and shows for other players and in killcams, as Black Ops 2's do.
- **Prestige:** Black Ops 2 has ten prestiges and Prestige Master, reached at level 55. Prestiging resets the level and what it unlocks, and earns the prestige's emblem and an unlock token for one permanent unlock; at Master everything unlocks (Activision support, "Achieving Prestige Master in Call of Duty: Black Ops II"; boostmatch.gg, "Black Ops 2 Prestige System"). Camos, challenges, and the combat record carry over. The prestige icon shows beside the name in lobbies, on the scoreboard, and in the kill feed.
  - Deliberate differences: Black Ops 2 also offered a stat reset or an extra class slot for a token; here a token is only a permanent unlock. Weapon kills (attachments) are kept, so attachments need not be earned again.
- **Calling cards:** Black Ops 2 awards calling cards for the last tier of most career, game mode, and scorestreak challenges, and one per prestige. The player picks one to show, and it appears in lobbies, on the scoreboard, and on the killcam's killer card.
- **Names and designs** are our own. None of Black Ops 2's camo, challenge, or calling card names are used.
